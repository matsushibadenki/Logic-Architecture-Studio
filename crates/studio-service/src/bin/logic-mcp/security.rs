use axum::{
    extract::{Extension, Path as RoutePath, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use hmac::{Hmac, Mac};
use mcp_core::Access;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;

#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Scope {
    Read,
    Session,
    Write,
    Approve,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Client {
    id: String,
    token_sha256: String,
    scopes: Vec<Scope>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    clients: Vec<Client>,
    #[serde(default = "default_limit")]
    requests_per_minute: u32,
}

fn default_limit() -> u32 {
    120
}

#[derive(Clone)]
pub struct Principal {
    id: String,
    scopes: Vec<Scope>,
}

impl Principal {
    fn approver(&self) -> bool {
        self.scopes.contains(&Scope::Approve)
    }
    pub fn permits(&self, access: Access) -> bool {
        self.scopes.contains(&match access {
            Access::Read => Scope::Read,
            Access::Session => Scope::Session,
            Access::Write => Scope::Write,
        })
    }
}

struct Credential {
    principal: Principal,
    digest: String,
    rate: Mutex<Rate>,
}

struct Rate {
    since: Instant,
    count: u32,
    reported: bool,
}
impl Default for Rate {
    fn default() -> Self {
        Self {
            since: Instant::now(),
            count: 0,
            reported: false,
        }
    }
}
impl Rate {
    fn allow(&mut self, limit: u32) -> (bool, bool) {
        if self.since.elapsed() >= Duration::from_secs(60) {
            *self = Self::default();
        }
        if self.count >= limit {
            return (false, !std::mem::replace(&mut self.reported, true));
        }
        self.count += 1;
        (true, false)
    }
}

struct Approval {
    id: String,
    principal: String,
    tool: String,
    args: Value,
    expires: Instant,
    decision: Option<bool>,
}

pub enum Decision {
    Allowed(String),
    Pending(String),
    Denied(String),
}

pub struct Security {
    clients: Vec<Credential>,
    approvals: Mutex<Vec<Approval>>,
    limit: u32,
    unauthenticated: Mutex<Rate>,
    audit: Mutex<Audit>,
    hosts: [String; 2],
    session_key: [u8; 32],
    _owner: File,
}

struct Audit {
    dir: PathBuf,
    file: File,
}

#[cfg(unix)]
fn check_private(metadata: &fs::Metadata, directory: bool) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    if metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err(io::Error::other(
            "MCP security files and directory must be private and owned by the current user",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn open_private(path: &Path, append: bool) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = OpenOptions::new()
        .read(true)
        .append(append)
        .create(append)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    check_private(&file.metadata()?, false)?;
    Ok(file)
}

impl Security {
    #[cfg(unix)]
    pub fn load(
        dir: &Path,
        hosts: [String; 2],
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        check_private(&fs::symlink_metadata(dir)?, true)?;
        let owner = open_private(&dir.join("owner.lock"), true)?;
        owner.try_lock()?;
        let file = open_private(&dir.join("clients.json"), false)?;
        let mut bytes = Vec::new();
        file.take(65_537).read_to_end(&mut bytes)?;
        if bytes.len() > 65_536 {
            return Err("MCP credential configuration exceeds 64 KiB".into());
        }
        let config: Config =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid MCP credential configuration")?;
        if !(1..=32).contains(&config.clients.len())
            || !(1..=6000).contains(&config.requests_per_minute)
        {
            return Err("Invalid MCP client count or rate limit".into());
        }
        let mut clients: Vec<Credential> = Vec::new();
        for client in config.clients {
            let approver = client.scopes.contains(&Scope::Approve);
            if client.id.is_empty()
                || client.id.len() > 48
                || !client
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                || client.token_sha256.len() != 64
                || !client
                    .token_sha256
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                || (approver && client.scopes.len() != 1)
                || (!approver && !client.scopes.contains(&Scope::Read))
                || clients
                    .iter()
                    .any(|c| c.digest == client.token_sha256 || c.principal.id == client.id)
            {
                return Err("Invalid or duplicate MCP credential entry".into());
            }
            clients.push(Credential {
                principal: Principal {
                    id: client.id,
                    scopes: client.scopes,
                },
                digest: client.token_sha256,
                rate: Mutex::new(Rate::default()),
            });
        }
        let mut session_key = [0; 32];
        getrandom::fill(&mut session_key)?;
        Ok(Self {
            clients,
            approvals: Mutex::new(Vec::new()),
            limit: config.requests_per_minute,
            unauthenticated: Mutex::new(Rate::default()),
            audit: Mutex::new(Audit {
                dir: dir.into(),
                file: open_private(&dir.join("audit.jsonl"), true)?,
            }),
            hosts,
            session_key,
            _owner: owner,
        })
    }

    fn session_tag(&self, who: &Principal, id: &str) -> String {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.session_key).expect("HMAC accepts a 32-byte key");
        mac.update(who.id.as_bytes());
        mac.update(b":");
        mac.update(id.as_bytes());
        format!("{:x}", mac.finalize().into_bytes())
    }

    pub fn approval(
        &self,
        who: &Principal,
        tool: &str,
        args: &Value,
    ) -> Result<Decision, &'static str> {
        let mut approvals = self.approvals.lock().unwrap();
        let now = Instant::now();
        approvals.retain(|a| a.expires > now);
        if let Some(index) = approvals
            .iter()
            .position(|a| a.principal == who.id && a.tool == tool && a.args == *args)
        {
            let approval = &approvals[index];
            return Ok(match approval.decision {
                Some(true) => Decision::Allowed(approvals.swap_remove(index).id),
                Some(false) => Decision::Denied(approval.id.clone()),
                None => Decision::Pending(approval.id.clone()),
            });
        }
        if approvals.len() >= 16 {
            return Err("approval_queue_full");
        }
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).map_err(|_| "approval_unavailable")?;
        let id: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        approvals.push(Approval {
            id: id.clone(),
            principal: who.id.clone(),
            tool: tool.into(),
            args: args.clone(),
            expires: now + Duration::from_secs(120),
            decision: None,
        });
        Ok(Decision::Pending(id))
    }

    pub fn record(
        &self,
        who: Option<&Principal>,
        tool: &str,
        outcome: &str,
        operation: Option<&str>,
        revision: Option<u64>,
        duration: Duration,
    ) -> io::Result<()> {
        let line = json!({
            "time_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "principal":who.map(|p| p.id.as_str()), "tool":tool,
            "outcome":outcome,"operation_id":operation,"revision":revision,
            "duration_ms":duration.as_millis(),
        });
        let mut audit = self.audit.lock().unwrap();
        if audit.file.metadata()?.len() >= 1_048_576 {
            fs::rename(
                audit.dir.join("audit.jsonl"),
                audit.dir.join("audit.previous.jsonl"),
            )?;
            audit.file = open_private(&audit.dir.join("audit.jsonl"), true)?;
        }
        writeln!(audit.file, "{line}")?;
        audit.file.sync_data()
    }
}

pub async fn authenticate(
    State(security): State<Arc<Security>>,
    mut request: Request,
    next: Next,
) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok());
    if !host.is_some_and(|host| {
        security
            .hosts
            .iter()
            .any(|allowed| host.eq_ignore_ascii_case(allowed))
    }) || request.headers().contains_key(header::ORIGIN)
    {
        if let Some(response) = check_rate(&security, None, &security.unauthenticated) {
            return response;
        }
        let _ = security.record(
            None,
            "http",
            "host_or_origin_denied",
            None,
            None,
            Duration::ZERO,
        );
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut headers = request.headers().get_all(header::AUTHORIZATION).iter();
    let token = headers
        .next()
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.split_once(' '))
        .and_then(|(scheme, value)| scheme.eq_ignore_ascii_case("bearer").then_some(value));
    let credential = token
        .filter(|t| {
            headers.next().is_none() && t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit())
        })
        .map(|token| format!("{:x}", Sha256::digest(token.as_bytes())))
        .and_then(|digest| {
            security
                .clients
                .iter()
                .find(|client| bool::from(client.digest.as_bytes().ct_eq(digest.as_bytes())))
        });
    let Some(credential) = credential else {
        if let Some(response) = check_rate(&security, None, &security.unauthenticated) {
            return response;
        }
        let _ = security.record(None, "http", "unauthenticated", None, None, Duration::ZERO);
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response();
    };
    let principal = &credential.principal;
    if let Some(response) = check_rate(&security, Some(principal), &credential.rate) {
        return response;
    }
    if (request.uri().path() != "/mcp") != principal.approver() {
        let _ = security.record(
            Some(principal),
            "http",
            "permission_denied",
            None,
            None,
            Duration::ZERO,
        );
        return StatusCode::FORBIDDEN.into_response();
    }
    const SESSION: &str = "mcp-session-id";
    if let Some(header) = request.headers().get(SESSION) {
        let session = header
            .to_str()
            .ok()
            .and_then(|value| value.rsplit_once('.'));
        let Some((id, _)) = session.filter(|(id, tag)| {
            bool::from(
                security
                    .session_tag(principal, id)
                    .as_bytes()
                    .ct_eq(tag.as_bytes()),
            )
        }) else {
            let _ = security.record(
                Some(principal),
                "http",
                "session_denied",
                None,
                None,
                Duration::ZERO,
            );
            return StatusCode::NOT_FOUND.into_response();
        };
        let Ok(internal) = HeaderValue::from_str(id) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        request.headers_mut().insert(SESSION, internal);
    }
    request.headers_mut().remove(header::AUTHORIZATION);
    request.extensions_mut().insert(principal.clone());
    let mut response = next.run(request).await;
    if let Some(id) = response
        .headers()
        .get(SESSION)
        .and_then(|id| id.to_str().ok())
    {
        let signed = format!("{id}.{}", security.session_tag(principal, id));
        response.headers_mut().insert(
            SESSION,
            HeaderValue::from_str(&signed).expect("valid signed session ID"),
        );
    }
    response
}

fn check_rate(
    security: &Security,
    who: Option<&Principal>,
    rate: &Mutex<Rate>,
) -> Option<Response> {
    let (allowed, report) = rate.lock().unwrap().allow(security.limit);
    if report {
        let _ = security.record(who, "http", "rate_limited", None, None, Duration::ZERO);
    }
    (!allowed)
        .then(|| (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, "60")]).into_response())
}

pub async fn preview(
    State(security): State<Arc<Security>>,
    RoutePath(id): RoutePath<String>,
    Extension(principal): Extension<Principal>,
) -> Response {
    approval_action(&security, &principal, &id, None)
}

pub async fn decide(
    State(security): State<Arc<Security>>,
    RoutePath((id, action)): RoutePath<(String, String)>,
    Extension(principal): Extension<Principal>,
) -> Response {
    match action.as_str() {
        "approve" => approval_action(&security, &principal, &id, Some(true)),
        "deny" => approval_action(&security, &principal, &id, Some(false)),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

fn approval_action(
    security: &Security,
    principal: &Principal,
    id: &str,
    decision: Option<bool>,
) -> Response {
    let mut approvals = security.approvals.lock().unwrap();
    let Some(approval) = approvals
        .iter_mut()
        .find(|a| a.id == id && a.expires > Instant::now())
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Some(decision) = decision {
        if approval.decision.is_some() {
            return StatusCode::CONFLICT.into_response();
        }
        let outcome = if decision { "approved" } else { "denied" };
        if security
            .record(
                Some(principal),
                &approval.tool,
                outcome,
                Some(id),
                approval.args["expected_revision"].as_u64(),
                Duration::ZERO,
            )
            .is_err()
        {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        approval.decision = Some(decision);
        return StatusCode::NO_CONTENT.into_response();
    }
    let body = json!({"id":id,"principal":approval.principal,"tool":approval.tool,"arguments":approval.args});
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body.to_string(),
    )
        .into_response()
}
