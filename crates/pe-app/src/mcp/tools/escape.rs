//! The escape hatch (spec.md 3.7): `invoke`, which runs any IPC command by
//! name for what no dedicated tool covers.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::{PolarExplorer, ToolResult, json, typed};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InvokeParams {
    /// A command name from the application's IPC surface, e.g.
    /// "rename_project" or "set_blend_colour".
    pub command: String,
    /// The command's arguments, an object with snake_case keys as in Rust
    /// (the boat is `boat_context`). Omitted is `{}`.
    #[serde(default)]
    pub args: Option<Value>,
}

#[tool_router(router = tool_router_escape, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "Read a classified application command. Use assistant_capabilities to inspect command effects."
    )]
    async fn app_read(&self, Parameters(p): Parameters<InvokeParams>) -> ToolResult {
        if crate::mcp::invoke::command_effect(&p.command)
            != Some(whirlwind_mcp::policy::Effect::Read)
        {
            return Err(super::ToolError::Refused(
                "This command is not available through app_read; use its dedicated tool or invoke."
                    .into(),
            ));
        }
        let args = p.args.unwrap_or_else(|| serde_json::json!({}));
        let name = p.command;
        let result = self
            .run("app_read", move |app| {
                crate::mcp::invoke::call(app, &name, args)
            })
            .await?;
        super::json(&serde_json::json!({"result":result}))
    }
    #[tool(
        description = "Run a classified reversible application edit, including detailed editing features not covered by dedicated tools."
    )]
    async fn app_edit(&self, Parameters(p): Parameters<InvokeParams>) -> ToolResult {
        if crate::mcp::invoke::command_effect(&p.command)
            != Some(whirlwind_mcp::policy::Effect::Edit)
        {
            return Err(super::ToolError::Refused(
                "This command is not available through app_edit; use its dedicated tool or invoke."
                    .into(),
            ));
        }
        let args = p.args.unwrap_or_else(|| serde_json::json!({}));
        let name = p.command;
        let result = self
            .write("app_edit", false, move |app| {
                crate::mcp::invoke::call(app, &name, args)
            })
            .await?;
        super::json(&serde_json::json!({"result":result}))
    }
    #[tool(
        description = "Runs any other application command by name with JSON arguments — the escape hatch for what no other tool covers (rename_project, set_blend_colour, set_blend_visible, set_segment_statistic, set_use_corrected, set_stokes_drift, recovered_projects, orr_catalogue_info …). Writes are undoable and the interface follows. An argument the command does not declare is refused. Settings, the track library and quitting are not reachable."
    )]
    async fn invoke(&self, Parameters(p): Parameters<InvokeParams>) -> ToolResult {
        // A command with no arguments takes an empty `Args {}`, which
        // deserialises from `{}` and not from null; and a client may send
        // the object as a string of JSON.
        let args: Value = match p.args {
            None | Some(Value::Null) => Value::Object(serde_json::Map::default()),
            Some(raw) => typed::<serde_json::Map<String, Value>>("args", raw).map(Value::Object)?,
        };
        let name = p.command;
        let opened = crate::mcp::invoke::OPENING.contains(&name.as_str());
        let result = self
            .write("invoke", opened, move |app| {
                crate::mcp::invoke::call(app, &name, args)
            })
            .await?;
        json(&serde_json::json!({ "result": result }))
    }
}
