use mcp_core::{AdapterError, AppAdapter, ToolRegistry};
use rmcp::{
    model::*,
    service::{RequestContext, RoleServer},
    transport::async_rw::AsyncRwTransport,
    ErrorData, ServerHandler, ServiceExt,
};
use serde_json::{json, Value};
use std::sync::Arc;
#[path = "logic-mcp/adapters.rs"]
mod adapters;

const IR_URI: &str = "logic://workspace/ir";
const EXPLORE: &str = "explore_architecture";

struct StudioMcp {
    registry: Arc<ToolRegistry>,
    tools: Vec<Tool>,
    sample: bool,
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
        Ok(ListToolsResult::with_all_items(self.tools.clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.as_ref();
        if !self.registry.contains(name) {
            return Err(ErrorData::invalid_params(
                "Unknown tool",
                Some(json!({"code":"unknown_tool","tool":name})),
            ));
        }
        let args = Value::Object(request.arguments.unwrap_or_default());
        Ok(match self.execute(name.to_owned(), args).await {
            Ok(value) => {
                let text = ContentBlock::text(value.to_string());
                let mut result = CallToolResult::structured(json!({"result": value}));
                result.content = vec![text];
                result.into()
            }
            Err(error) => CallToolResult::structured_error(
                json!({"error":{"code":error.code,"message":error.message}}),
            )
            .into(),
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let flags: Vec<_> = std::env::args().skip(1).collect();
    let sample = flags.iter().any(|flag| flag == "--sample");
    let adapter: Box<dyn AppAdapter> = if sample {
        Box::new(adapters::SampleAdapter::default())
    } else {
        Box::new(adapters::StudioAdapter(studio_service::workspace_root()))
    };
    let registry = ToolRegistry::new(adapter, flags.iter().any(|flag| flag == "--allow-write"))
        .map_err(|error| std::io::Error::other(error.message))?;
    let tools = registry
        .operations()
        .map(|operation| serde_json::from_value(operation.descriptor()))
        .collect::<Result<Vec<Tool>, _>>()?;
    let mcp = StudioMcp {
        registry: Arc::new(registry),
        tools,
        sample,
    };
    let transport = AsyncRwTransport::new(tokio::io::stdin(), tokio::io::stdout())
        .with_max_line_length(1_048_576);
    mcp.serve(transport).await?.waiting().await?;
    Ok(())
}
