//! MCP protocol glue: exposes [`Dotify`]'s tools through rmcp.

use std::sync::Arc;

use base64::Engine;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};

use crate::error::ToolResult;
use crate::tools::{Dotify, ToolDef, ToolOutput};

const INSTRUCTIONS: &str = "\
Dotify is a pixel art and sprite animation editor. Typical flow: project_list or project_open / \
project_create to get a project handle, project_get for its artboards (sprites) and their layer and \
frame IDs, then edit with the drawing, layer, frame and tag tools (or several edits at once with \
batch), look at the result with get_canvas, and finish with project_save and an export (export_godot \
for Godot 4). A project holds one or more artboards; pass `artboard` when it has more than one. \
Colors are #rrggbbaa (or #rrggbb) and replace pixels; (0,0) is the top-left pixel. Every edit call is \
one undoable step (undo / redo); pass expected_revision to make sure nobody edited in between. \
Tools that delete layers or frames, replace existing files or discard unsaved changes need confirm: true. \
File paths must be inside the server's root directories; relative paths start at the first root.";

/// The MCP server. Cheap to clone; every clone (one per HTTP session)
/// shares the same open projects.
#[derive(Clone)]
pub struct McpServer {
    dotify: Arc<Dotify>,
}

impl McpServer {
    pub fn new(dotify: Arc<Dotify>) -> Self {
        Self { dotify }
    }
}

fn to_tool(def: &ToolDef) -> Tool {
    Tool::new(def.name, def.description, def.input_schema.clone()).with_title(def.title).with_annotations(
        ToolAnnotations::with_title(def.title).read_only(!def.writes).destructive(def.destructive).open_world(false),
    )
}

/// Tool failures become `isError` results with a structured error body so
/// the model can read the code and fix its call.
fn to_result(result: ToolResult<ToolOutput>) -> CallToolResult {
    match result {
        Ok(ToolOutput::Json(v)) => CallToolResult::structured(v),
        Ok(ToolOutput::Image { png, info }) => {
            let data = base64::engine::general_purpose::STANDARD.encode(png);
            let mut r = CallToolResult::success(vec![
                ContentBlock::image(data, "image/png"),
                ContentBlock::text(info.to_string()),
            ]);
            r.structured_content = Some(info);
            r
        }
        Err(e) => CallToolResult::structured_error(e.to_json()),
    }
}

impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("dotify-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.dotify.tools().map(to_tool).collect()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.dotify.tool(name).map(to_tool)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.to_string();
        if self.dotify.tool(&name).is_none() && !self.dotify.read_only() {
            return Err(ErrorData::invalid_params(format!("unknown tool {name:?}"), None));
        }
        let dotify = self.dotify.clone();
        let args = request.arguments.unwrap_or_default();
        // Exports and big batches are CPU-bound; keep them off the async workers.
        let result = tokio::task::spawn_blocking(move || dotify.call(&name, args))
            .await
            .map_err(|e| ErrorData::internal_error(format!("tool panicked: {e}"), None))?;
        Ok(to_result(result).into())
    }
}
