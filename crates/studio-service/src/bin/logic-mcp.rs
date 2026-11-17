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
#[path = "logic-mcp/adapters.rs"]
mod adapters;
#[path = "logic-mcp/security.rs"]
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut flags = std::env::args().skip(1);
    let mut sample = false;
    let mut write_enabled = false;
    let mut http = None;
    let mut security_dir = None;
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--sample" => sample = true,
            "--allow-write" => write_enabled = true,
            "--http" if http.is_none() => {
                http = Some(
                    flags
                        .next()
                        .ok_or("--http requires a port")?
                        .parse::<u16>()?,
                );
            }
            "--http-security" if security_dir.is_none() => {
                security_dir = Some(flags.next().ok_or("--http-security requires a directory")?);
            }
            _ => return Err(format!("Unknown or repeated option: {flag}").into()),
        }
    }
    if http.is_some() != security_dir.is_some() {
        return Err("--http and --http-security must be used together".into());
    }
    let adapter: Box<dyn AppAdapter> = if sample {
        Box::new(adapters::SampleAdapter::default())
    } else {
        Box::new(adapters::StudioAdapter(studio_service::workspace_root()))
    };
    let registry = ToolRegistry::new(adapter, write_enabled)
        .map_err(|error| std::io::Error::other(error.message))?;
    let tools = registry
        .operations()
        .map(|operation| serde_json::from_value(operation.descriptor()))
        .collect::<Result<Vec<Tool>, _>>()?;
    let mut mcp = StudioMcp {
        registry: Arc::new(registry),
        tools: tools.into(),
        sample,
        security: None,
    };
    if let (Some(port), Some(directory)) = (http, security_dir) {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
        let address = listener.local_addr()?;
        let hosts = [address.to_string(), format!("localhost:{}", address.port())];
        let security = Arc::new(Security::load(
            std::path::Path::new(&directory),
            hosts.clone(),
        )?);
        mcp.security = Some(security.clone());
        let config = StreamableHttpServerConfig::default()
            .with_allowed_hosts(hosts)
            .enforce_origin_validation()
            .with_max_request_body_bytes(1_048_576)
            .with_json_response(true);
        let http: StreamableHttpService<StudioMcp, LocalSessionManager> =
            StreamableHttpService::new(move || Ok(mcp.clone()), Default::default(), config);
        let router = axum::Router::new()
            .route_service("/mcp", http)
            .route("/approvals/{id}", axum::routing::get(security::preview))
            .route(
                "/approvals/{id}/{action}",
                axum::routing::post(security::decide),
            )
            .with_state(security.clone())
            .route_layer(axum::middleware::from_fn_with_state(
                security,
                security::authenticate,
            ));
        eprintln!("MCP Streamable HTTP listening on http://{address}/mcp");
        axum::serve(listener, router).await?;
        return Ok(());
    }
    let transport = AsyncRwTransport::new(tokio::io::stdin(), tokio::io::stdout())
        .with_max_line_length(1_048_576);
    mcp.serve(transport).await?.waiting().await?;
    Ok(())
}
