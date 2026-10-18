//! The tools a client sees (spec.md 3.7).
//!
//! Every tool calls the command the interface calls, with the `State` the
//! handle gives, so there is one implementation of each feature: an agent's
//! edit is validated, undoable and autosaved exactly as a person's is. A
//! tool that writes ends with `write`, which emits `document://changed`.
//!
//! Results are JSON, built by [`json`] from whatever the command returned:
//! the interface's own IPC types, with no second description of them to
//! keep in step.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::IntoCallToolResult;
use rmcp::model::{
    CallToolResponse, CallToolResult, ErrorData as McpError, Implementation, ServerCapabilities,
    ServerConfig,
};
use rmcp::{ServerHandler, tool_handler};
use tauri::Manager;

use super::events;
use crate::error::AppError;

mod blend;
mod boats;
pub mod effects;
mod escape;
mod export;
pub mod guide;
mod history;
mod project;
mod sources;
mod tracks;
mod view;
mod weather;

/// One handler per client session, holding the application it drives.
///
/// Generic over the Tauri runtime so the mock application used by the
/// integration tests (`tauri::test::MockRuntime`) can drive the same code
/// path as the shipped build.
///
/// Deliberately not `Clone`: the service counts live sessions in `Drop`
/// (`session_closed`), and a clone would make that count wrong.
pub struct PolarExplorer<R: tauri::Runtime> {
    pub(crate) app: tauri::AppHandle<R>,
    tool_router: ToolRouter<Self>,
    /// The listener run this session was counted in (`on_initialized`), so
    /// `Drop` uncounts only what was counted: a request refused before it
    /// initialised, or a handler `rmcp` made for one sessionless request,
    /// must not take a session away.
    counted: std::sync::OnceLock<u64>,
}

impl<R: tauri::Runtime> std::fmt::Debug for PolarExplorer<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolarExplorer").finish_non_exhaustive()
    }
}

impl<R: tauri::Runtime> Drop for PolarExplorer<R> {
    fn drop(&mut self) {
        if let Some(generation) = self.counted.get() {
            self.app
                .state::<super::McpService>()
                .session_closed(&self.app, *generation);
        }
    }
}

/// A tool's failure, in the two shapes MCP distinguishes.
///
/// A refused command reaches the client as a **tool result** with
/// `is_error: true` and the `AppError` text the interface would show, never
/// as a JSON-RPC protocol error: a model reading the result needs to see
/// *why* `project_new` was refused. A protocol error stays for what is not
/// the request's fault: a panic in `spawn_blocking`.
pub(crate) enum ToolError {
    /// An `AppError`'s message, or a structured parameter's parse error:
    /// both are things the caller should read and can act on.
    Refused(String),
    /// The tool machinery's own failure, not the request's.
    Internal(McpError),
}

impl From<AppError> for ToolError {
    /// The message the interface would show, and — where the interface
    /// would answer with a dialog — what a client does instead.
    fn from(err: AppError) -> Self {
        let hint = match err.kind() {
            "unsaved-changes" => {
                " Save it first (project_save), or use project_discard if the user authorized losing those changes."
            }
            "never-saved" => " Pass `path` to project_save: where to save it.",
            "orc-duplicate" => " Pass allow_duplicate: true to add it again.",
            _ => "",
        };
        ToolError::Refused(format!("{err}{hint}"))
    }
}

impl IntoCallToolResult for ToolError {
    fn into_call_tool_result(self) -> std::result::Result<CallToolResponse, McpError> {
        match self {
            ToolError::Refused(message) => Ok(whirlwind_mcp::results::refused(message).into()),
            ToolError::Internal(err) => Err(err),
        }
    }
}

/// What every tool returns.
pub(crate) type ToolResult = std::result::Result<CallToolResult, ToolError>;

/// A command's answer as a tool result: JSON text for a model to read and
/// the same value as structured content. MCP's structured content is an
/// object, so a list or a bare value travels as `{"result": …}`.
pub(crate) fn json<T: serde::Serialize>(value: &T) -> ToolResult {
    let value = serde_json::to_value(value).map_err(|e| {
        ToolError::Internal(McpError::internal_error(
            format!("the result could not be written as JSON: {e}"),
            None,
        ))
    })?;
    let value = if value.is_object() {
        value
    } else {
        serde_json::json!({ "result": value })
    };
    Ok(CallToolResult::structured(value))
}

/// Reads a structured parameter: filters, blend settings, an edit.
///
/// These are the interface's own IPC types, received as plain JSON, for two
/// reasons. A value that does not parse is then a **refusal the model can
/// read** (`filters: unknown field …`), where a typed parameter would fail
/// in `rmcp` as a protocol error before the tool ran. And a client that
/// sends an object as a *string* of JSON — several do — is read here rather
/// than refused.
pub(crate) fn typed<T: serde::de::DeserializeOwned>(
    field: &str,
    raw: serde_json::Value,
) -> std::result::Result<T, ToolError> {
    let raw = match raw {
        serde_json::Value::String(text) if text.trim_start().starts_with(['{', '[']) => {
            serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text))
        }
        other => other,
    };
    serde_json::from_value(raw).map_err(|e| ToolError::Refused(format!("{field}: {e}")))
}

/// `patch` written over `base`: a client names only what it changes, and
/// the rest stays as it is. An object inside an object is patched the same
/// way, so one end of a range can be given alone.
///
/// A key `base` does not have is refused, naming the keys it has. Merged
/// in silently it would be dropped when the result is read as its type, and
/// the tool would answer success for a filter that was never set.
pub(crate) fn patch_over(
    field: &str,
    base: &mut serde_json::Value,
    patch: serde_json::Map<String, serde_json::Value>,
) -> std::result::Result<(), ToolError> {
    let Some(object) = base.as_object_mut() else {
        return Ok(());
    };
    for (key, value) in patch {
        let Some(slot) = object.get_mut(&key) else {
            let known: Vec<&str> = object.keys().map(String::as_str).collect();
            return Err(ToolError::Refused(format!(
                "{field}: there is no {key:?} here; the keys are {}",
                known.join(", ")
            )));
        };
        match value {
            serde_json::Value::Object(inner) if slot.is_object() => {
                patch_over(&format!("{field}.{key}"), slot, inner)?;
            }
            other => *slot = other,
        }
    }
    Ok(())
}

/// The id of the boat a tool acts on: the one named, or the first. For the
/// few commands that read a missing boat as "every boat" where a tool means
/// the first, as its `boat` parameter says.
pub(crate) fn boat_id(
    state: &crate::commands::AppState,
    boat: Option<u64>,
) -> crate::error::Result<u64> {
    match boat {
        Some(boat) => Ok(boat),
        None => Ok(crate::projects::summary(state)?
            .ok_or(AppError::NoProjectOpen)?
            .id),
    }
}

/// Refuses to write over a file that is already there unless the call said
/// to. The interface's save dialog asks before it replaces a file; a tool
/// has no dialog, so the caller says it in the call.
pub(crate) fn may_write(
    path: &std::path::Path,
    overwrite: bool,
) -> std::result::Result<(), ToolError> {
    if !overwrite && path.exists() {
        return Err(ToolError::Refused(format!(
            "{} already exists. Pass overwrite: true to replace it, only if the user means it to be replaced; otherwise choose another path.",
            path.display()
        )));
    }
    if overwrite {
        whirlwind_mcp::files::preserve_before_replace(path)
            .map_err(|e| ToolError::Refused(e.to_string()))?;
    }
    Ok(())
}

/// The boat a tool acts on: every tool that reads or edits a boat takes it.
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct BoatParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
}

/// A path a client gave, made absolute: as given when it is, under the
/// person's home folder when it begins with `~/`. A relative path is
/// refused: it would be relative to wherever the application happened to
/// be started, which no client can know.
pub(crate) fn absolute(path: &str) -> std::result::Result<String, ToolError> {
    let trimmed = path.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        let home = directories::UserDirs::new().ok_or_else(|| {
            ToolError::Refused("this computer has no home folder to resolve ~/ against".to_owned())
        })?;
        return Ok(home.home_dir().join(rest).to_string_lossy().into_owned());
    }
    if std::path::Path::new(trimmed).is_absolute() {
        return Ok(trimmed.to_owned());
    }
    Err(ToolError::Refused(format!(
        "the path {path:?} is not absolute: give the whole path, or one beginning with ~/"
    )))
}

/// How close a value a client gave must be to an axis value to be it.
const ON_AXIS: f64 = 1e-6;

/// The index of `value` on `axis`, or a refusal that lists the axis: a
/// client names a polar's cells by wind angle and wind speed, which are
/// what it reads back, and an angle that is not on the grid is told what
/// the grid has rather than snapped to a neighbour.
pub(crate) fn on_axis(what: &str, axis: &[f64], value: f64) -> std::result::Result<u32, ToolError> {
    axis.iter()
        .position(|candidate| (candidate - value).abs() <= ON_AXIS)
        .and_then(|index| u32::try_from(index).ok())
        .ok_or_else(|| {
            let values: Vec<String> = axis.iter().map(|v| v.to_string()).collect();
            ToolError::Refused(format!(
                "{what} {value} is not on the grid; its {what} values are {}",
                values.join(", ")
            ))
        })
}

impl<R: tauri::Runtime> PolarExplorer<R> {
    /// Tells the status bar which tool a client just called. Every tool
    /// passes through here, which is also what counts a client that keeps
    /// no session.
    pub(crate) fn note(&self, name: &str) {
        self.app.state::<super::McpService>().note_tool(
            &self.app,
            name,
            self.counted.get().is_some(),
        );
    }

    /// Runs a command off the async thread, since commands lock the session.
    pub(crate) async fn run<T: Send + 'static>(
        &self,
        name: &'static str,
        f: impl FnOnce(&tauri::AppHandle<R>) -> crate::error::Result<T> + Send + 'static,
    ) -> std::result::Result<T, ToolError> {
        let app = self.app.clone();
        self.note(name);
        tokio::task::spawn_blocking(move || f(&app))
            .await
            .map_err(|e| {
                ToolError::Internal(McpError::internal_error(
                    format!("tool panicked: {e}"),
                    None,
                ))
            })?
            .map_err(ToolError::from)
    }

    /// `run`, then tell the frontend the document changed.
    ///
    /// Emits whether the closure succeeded or refused partway through: a
    /// multi-command tool (`source_set`, `track_set`) applies one command
    /// per field, so a later field refusing still leaves the earlier ones
    /// committed, and the frontend must not miss that the document changed.
    /// The closure's own error is what the caller needs to see, so a failure
    /// to emit alongside it is swallowed rather than replacing that error.
    ///
    /// `opened` is only ever true when the closure actually succeeded: a
    /// refused `project_open` leaves the previous project open, and telling
    /// the frontend `opened: true` anyway would reset its stage and
    /// selection for a document that never changed.
    pub(crate) async fn write<T: Send + 'static>(
        &self,
        name: &'static str,
        opened: bool,
        f: impl FnOnce(&tauri::AppHandle<R>) -> crate::error::Result<T> + Send + 'static,
    ) -> std::result::Result<T, ToolError> {
        let result = self.run(name, f).await;
        let emitted = events::changed(&self.app, opened && result.is_ok());
        match result {
            Ok(out) => {
                emitted.map_err(ToolError::from)?;
                Ok(out)
            }
            Err(err) => Err(err),
        }
    }
}

impl<R: tauri::Runtime> PolarExplorer<R> {
    /// The handler for one client session.
    pub fn router() -> ToolRouter<Self> {
        let mut tool_router = Self::tool_router_guide()
            + Self::tool_router_project()
            + Self::tool_router_boats()
            + Self::tool_router_history()
            + Self::tool_router_sources()
            + Self::tool_router_tracks()
            + Self::tool_router_weather()
            + Self::tool_router_blend()
            + Self::tool_router_export()
            + Self::tool_router_view()
            + Self::tool_router_escape();
        effects::apply(&mut tool_router);
        tool_router
    }
    pub fn catalogue() -> Vec<rmcp::model::Tool> {
        Self::router().list_all()
    }
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        let tool_router = Self::router();
        Self {
            app,
            tool_router,
            counted: std::sync::OnceLock::new(),
        }
    }
}

#[tool_handler(router = self.tool_router.clone())]
impl<R: tauri::Runtime> ServerHandler for PolarExplorer<R> {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "PolarExplorer",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(guide::instructions(guide::now()))
    }

    /// Counts the session once the client has finished initialising, paired
    /// with the decrement in `Drop` when the session ends.
    fn on_initialized(
        &self,
        _context: rmcp::service::NotificationContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        self.counted.get_or_init(|| {
            self.app
                .state::<super::McpService>()
                .session_opened(&self.app)
        });
        std::future::ready(())
    }
}
