use mcp_core::{Access, AdapterError, AppAdapter, ToolRegistry};
use rmcp::{
    model::*,
    service::{RequestContext, RoleServer},
    transport::{
        async_rw::AsyncRwTransport,
        streamable_http_server::{
            session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
        },
    },
    ErrorData, ServerHandler, ServiceExt,
};
use serde_json::{json, Value};
use std::sync::Arc;
mod adapters;
mod security;
use security::{Decision, Principal, Security};
use std::time::Instant;

const IR_URI: &str = "logic://workspace/ir";
const EXPLORE: &str = "explore_architecture";

#[derive(Clone)]
struct StudioMcp {
    registry: Arc<ToolRegistry>,
    tools: Arc<[Tool]>,
    sample: bool,
    security: Option<Arc<Security>>,
    on_change: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl StudioMcp {
    async fn execute(&self, method: String, args: Value) -> Result<Value, AdapterError> {
        let registry = self.registry.clone();
        tokio::task::spawn_blocking(move || registry.invoke(&method, args))
            .await
            .map_err(|_| AdapterError::new("tool_failed", "Request worker failed"))?
    }
}

impl ServerHandler for StudioMcp {
    fn get_info(&self) -> ServerConfig {
        let builder = ServerCapabilities::builder().enable_tools();
        let (name, capabilities, instructions) = if self.sample {
            (
                "logic-studio-sample",
                builder.build(),
                "Use the in-memory sample tools. Writes require --allow-write.",
            )
        } else {
            (
                "logic-architecture-studio",
                builder.enable_resources().enable_prompts().build(),
                "Inspect revision, propose patches, then apply with expected_revision. Writes require --allow-write. Candidate adoption is GUI-only.",
            )
        };
        ServerConfig::new(capabilities)
            .with_server_info(Implementation::new(name, env!("CARGO_PKG_VERSION")))
            .with_instructions(instructions)
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools.to_vec()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.as_ref();
        if !self.registry.contains(name) {
            return Err(ErrorData::invalid_params(
                "Unknown tool",
                Some(json!({"code":"unknown_tool","tool":name})),
            ));
        }
        let args = Value::Object(request.arguments.unwrap_or_default());
        let started = Instant::now();
        let revision = args["expected_revision"].as_u64();
        let mut audit = None;
        let mut mutates = false;
        if let Some(security) = &self.security {
            let Some(principal) = context
                .extensions
                .get::<axum::http::request::Parts>()
                .and_then(|parts| parts.extensions.get::<Principal>())
            else {
                return Ok(tool_error(
                    "unauthenticated",
                    "Authentication required",
                    None,
                ));
            };
            let record = |outcome, operation| {
                security.record(
                    Some(principal),
                    name,
                    outcome,
                    operation,
                    revision,
                    started.elapsed(),
                )
            };
            let access = match self.registry.check(name, &args) {
                Ok(access) => access,
                Err(error) => {
                    let _ = record("rejected", None);
                    return Ok(tool_error(error.code, &error.message, None));
                }
            };
            if !principal.permits(access) {
                let _ = record("permission_denied", None);
                return Ok(tool_error(
                    "permission_denied",
                    "Client lacks the required tool scope",
                    None,
                ));
            }
            mutates = !matches!(access, Access::Read);
            let mut operation = None;
            if matches!(access, Access::Write) {
                match security.approval(principal, name, &args) {
                    Ok(Decision::Allowed(id)) => operation = Some(id),
                    Ok(Decision::Pending(id)) => {
                        let _ = record("approval_required", Some(&id));
                        return Ok(tool_error(
                            "approval_required",
                            "Write requires a separate approval",
                            Some(&id),
                        ));
                    }
                    Ok(Decision::Denied(id)) => {
                        let _ = record("approval_denied", Some(&id));
                        return Ok(tool_error(
                            "approval_denied",
                            "Write approval was denied",
                            None,
                        ));
                    }
                    Err(code) => {
                        return Ok(tool_error(code, "Could not create write approval", None))
                    }
                }
            }
            if !matches!(access, Access::Read)
                && record("authorized", operation.as_deref()).is_err()
            {
                return Ok(tool_error(
                    "audit_unavailable",
                    "Could not record the operation",
                    None,
                ));
            }
            audit = Some((security, principal, operation));
        }
        let result = self.execute(name.to_owned(), args).await;
        if mutates && result.is_ok() {
            if let Some(callback) = &self.on_change {
                callback();
            }
        }
        if let Some((security, principal, operation)) = audit {
            let outcome = if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            };
            let _ = security.record(
                Some(principal),
                name,
                outcome,
                operation.as_deref(),
                revision,
                started.elapsed(),
            );
        }
        Ok(match result {
            Ok(value) => {
                let text = ContentBlock::text(value.to_string());
                let mut result = CallToolResult::structured(json!({"result": value}));
                result.content = vec![text];
                result.into()
            }
            Err(error) => tool_error(error.code, &error.message, None),
        })
    }

    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        if self.sample {
            return Ok(ListResourcesResult::with_all_items(vec![]));
        }
        Ok(ListResourcesResult::with_all_items(vec![Resource::new(
            IR_URI,
            "Canonical design IR",
        )
        .with_mime_type("application/json")]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        if self.sample || request.uri != IR_URI {
            return Err(ErrorData::invalid_params(
                "Unknown resource",
                Some(json!({"code":"unknown_resource"})),
            ));
        }
        let state = self
            .execute("design.get_ir".into(), json!({}))
            .await
            .map_err(|e| ErrorData::internal_error(e.message, Some(json!({"code":e.code}))))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(state.to_string(), IR_URI).with_mime_type("application/json")
        ])
        .into())
    }

    async fn list_prompts(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        if self.sample {
            return Ok(ListPromptsResult::with_all_items(vec![]));
        }
        Ok(ListPromptsResult::with_all_items(vec![Prompt::new(
            EXPLORE,
            Some("Revision-safe architecture exploration with independent verification"),
            None,
        )]))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, ErrorData> {
        if self.sample || request.name != EXPLORE {
            return Err(ErrorData::invalid_params(
                "Unknown prompt",
                Some(json!({"code":"unknown_prompt"})),
            ));
        }
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            "Inspect the IR and its revision. Define an independent behavioral contract. Propose isolated candidates, run the same test vectors on every candidate, and report failures honestly. Never claim PPA improvements without actual synthesis and physical results. Leave adoption to the user.",
        )]).into())
    }
}

fn tool_error(code: &str, message: &str, approval_id: Option<&str>) -> CallToolResponse {
    let mut error = json!({"code": code, "message": message});
    if let Some(id) = approval_id {
        error["approval_id"] = json!(id);
    }
    CallToolResult::structured_error(json!({"result": null, "error": error})).into()
}

type McpError = Box<dyn std::error::Error + Send + Sync>;

fn bridge(
    root: std::path::PathBuf,
    sample: bool,
    write_enabled: bool,
) -> Result<StudioMcp, McpError> {
    let adapter: Box<dyn AppAdapter> = if sample {
        Box::new(adapters::SampleAdapter::default())
    } else {
        Box::new(adapters::StudioAdapter(root))
    };
    let registry = ToolRegistry::new(adapter, write_enabled)
        .map_err(|error| std::io::Error::other(error.message))?;
    let tools = registry
        .operations()
        .map(|operation| serde_json::from_value(operation.descriptor()))
        .collect::<Result<Vec<Tool>, _>>()?;
    Ok(StudioMcp {
        registry: Arc::new(registry),
        tools: tools.into(),
        sample,
        security: None,
        on_change: None,
    })
}

fn router(
    mut mcp: StudioMcp,
    security: Arc<Security>,
    hosts: [String; 2],
    external_approval: bool,
) -> axum::Router {
    mcp.security = Some(security.clone());
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(hosts)
        .enforce_origin_validation()
        .with_max_request_body_bytes(1_048_576)
        .with_json_response(true);
    let http: StreamableHttpService<StudioMcp, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(mcp.clone()), Default::default(), config);
    let mut router = axum::Router::new().route_service("/mcp", http);
    if external_approval {
        router = router
            .route("/approvals/{id}", axum::routing::get(security::preview))
            .route(
                "/approvals/{id}/{action}",
                axum::routing::post(security::decide),
            );
    }
    router
        .with_state(security.clone())
        .route_layer(axum::middleware::from_fn_with_state(
            security,
            security::authenticate,
        ))
}

fn hosts(address: std::net::SocketAddr) -> [String; 2] {
    [address.to_string(), format!("localhost:{}", address.port())]
}

pub async fn run_cli(
    sample: bool,
    write: bool,
    http: Option<(u16, String)>,
) -> Result<(), McpError> {
    let mcp = bridge(crate::workspace_root(), sample, write)?;
    if let Some((port, directory)) = http {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
        let address = listener.local_addr()?;
        let hosts = hosts(address);
        let security = Arc::new(Security::load(
            std::path::Path::new(&directory),
            hosts.clone(),
        )?);
        eprintln!("MCP Streamable HTTP listening on http://{address}/mcp");
        axum::serve(listener, router(mcp, security, hosts, true)).await?;
    } else {
        let transport = AsyncRwTransport::new(tokio::io::stdin(), tokio::io::stdout())
            .with_max_line_length(1_048_576);
        mcp.serve(transport).await?.waiting().await?;
    }
    Ok(())
}

pub use security::ApprovalSummary;

pub struct DesktopServer {
    security: Arc<Security>,
    pub endpoint: String,
    pub token: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl DesktopServer {
    pub fn start(
        root: std::path::PathBuf,
        session: bool,
        write: bool,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, McpError> {
        let directory = root.join(".logic/mcp");
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let hosts = hosts(address);
        let (security, token) = Security::desktop(&directory, hosts.clone(), session, write)?;
        let security = Arc::new(security);
        let mut mcp = bridge(root, false, write)?;
        mcp.on_change = Some(Arc::new(on_change));
        let router = router(mcp, security.clone(), hosts, false);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let listener = {
            let _entered = runtime.enter();
            tokio::net::TcpListener::from_std(listener)?
        };
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let worker = std::thread::Builder::new()
            .name("mcp-http".into())
            .spawn(move || {
                runtime.block_on(async {
                    tokio::select! {
                        _ = stopped => {},
                        result = axum::serve(listener, router) => {
                            if let Err(error) = result {
                                eprintln!("MCP HTTP: {error}");
                            }
                        }
                    }
                });
            })?;
        Ok(Self {
            security,
            endpoint: format!("http://{address}/mcp"),
            token,
            stop: Some(stop),
            worker: Some(worker),
        })
    }

    pub fn is_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    }

    pub fn pending(&self) -> Vec<ApprovalSummary> {
        self.security.pending()
    }

    pub fn review(&self, id: &str) -> Option<Value> {
        self.security.review(id)
    }

    pub fn decide(&self, id: &str, approve: bool) -> Result<(), &'static str> {
        self.security.desktop_decide(id, approve)
    }
}

impl Drop for DesktopServer {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
