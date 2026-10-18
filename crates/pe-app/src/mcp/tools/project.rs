//! The project group (spec.md 3.7): what is open, and opening, saving and
//! closing it.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use tauri::Manager;

use super::{PolarExplorer, ToolError, ToolResult, absolute, json, may_write};
use crate::boats::tracker_project::{self, BoatMatchMode};
use crate::commands::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectNewParams {
    /// The project's name.
    pub name: String,
    /// The first boat's name. A project starts with one boat.
    #[serde(default)]
    pub boat_name: Option<String>,
    /// Free notes about that boat.
    #[serde(default)]
    pub boat_notes: Option<String>,
    /// Discard unsaved changes in the open project. Refused without it.
    /// Pass true only when the user said to discard them.
    #[serde(default)]
    pub discard_unsaved: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RaceProjectParams {
    /// The race tracker: "yellowbrick", "geovoile" or "bluewater".
    pub tracker: String,
    /// The race on that tracker: its link, or the tracker's race key (for
    /// YellowBrick, e.g. "fastnet2025"). A Geovoile race sailed in legs is
    /// built from the leg its link shows.
    pub url: String,
    /// How a boat's certificates and library tracks are matched:
    /// "identical_model" (the default: the same model or class, by model,
    /// builder and length) or "exact_boat" (that boat alone, by sail number,
    /// MMSI or name and model).
    #[serde(default)]
    pub match_mode: Option<String>,
    /// Only the boats in any of these classes, as `tracker_event` lists
    /// each boat's `classes` (each of its groups), e.g. ["IRC 1", "IRC 2"]; a
    /// boat in several is one tab. Omit for every boat.
    #[serde(default)]
    pub classes: Option<Vec<String>>,
    /// Replace the open project even if it has unsaved changes. Refused
    /// without it; pass true only when the user said to discard them.
    #[serde(default)]
    pub discard_unsaved: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectOpenParams {
    /// Path to a .wpsproj file: absolute, or beginning with `~/`.
    pub path: String,
    /// Discard unsaved changes in the open project. Refused without it.
    #[serde(default)]
    pub discard_unsaved: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectSaveParams {
    /// Save here instead of the project's own path (a Save As). Absolute, or
    /// beginning with `~/`; `.wpsproj` is added when missing.
    #[serde(default)]
    pub path: Option<String>,
    /// Replace a file that is already at `path`. Refused without it, unless
    /// that file is the project's own. Pass true only when the user means
    /// the file to be replaced.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscardParams {
    /// Discard unsaved changes. Refused without it when there are any.
    #[serde(default)]
    pub discard_unsaved: bool,
}

#[tool_router(router = tool_router_project, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "Discard unsaved work and close the open project. This is irreversible and requires approval."
    )]
    async fn project_discard(&self) -> ToolResult {
        self.write("project_discard", true, |app| {
            crate::projects::close_project(app.state(), true)
        })
        .await?;
        json(&serde_json::json!({"closed":true}))
    }
    #[tool(
        description = "What PolarExplorer — the sailing-polar application open on this computer — has open: `project` is its summary (name, path, dirty, revision, the first boat's sources with ids, weights and counts, the blend's settings and coverage, undo state), or null when none is open. A good first call after polarexplorer_guide: an open project that is dirty holds the user's unsaved work."
    )]
    async fn project_status(&self) -> ToolResult {
        let project = self
            .run("project_status", |app| {
                crate::projects::project_summary(app.state(), None)
            })
            .await?;
        json(&serde_json::json!({ "project": project }))
    }

    #[tool(
        description = "Starts a new PolarExplorer project with one boat and opens it in the interface: the first step of building a polar. Refused while the open project has unsaved changes ; save first or use project_discard."
    )]
    async fn project_new(&self, Parameters(p): Parameters<ProjectNewParams>) -> ToolResult {
        if p.discard_unsaved {
            return Err(ToolError::Refused("Use project_discard to approve discarding unsaved work, then retry this tool with discard_unsaved false.".into()));
        }
        let boat =
            (p.boat_name.is_some() || p.boat_notes.is_some()).then(|| crate::projects::BoatInput {
                name: p.boat_name.unwrap_or_default(),
                notes: p.boat_notes.unwrap_or_default(),
            });
        let summary = self
            .write("project_new", true, move |app| {
                crate::projects::new_project(app.state(), p.name, boat, p.discard_unsaved)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Opens a .wpsproj project file in the interface. Refused while the open project has unsaved changes ; save first or use project_discard."
    )]
    async fn project_open(&self, Parameters(p): Parameters<ProjectOpenParams>) -> ToolResult {
        if p.discard_unsaved {
            return Err(ToolError::Refused("Use project_discard to approve discarding unsaved work, then retry this tool with discard_unsaved false.".into()));
        }
        let path = absolute(&p.path)?;
        let summary = self
            .write("project_open", true, move |app| {
                crate::projects::open_project(app.state(), path, p.discard_unsaved)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Saves the project to its own path, or to `path` as a Save As (absolute or beginning with ~/). A project that has never been saved needs `path`. A file already at `path` is replaced only with overwrite: true. Saving is not needed before an export."
    )]
    async fn project_save(&self, Parameters(p): Parameters<ProjectSaveParams>) -> ToolResult {
        let path = p.path.as_deref().map(absolute).transpose()?;
        if let Some(path) = &path {
            // The file the save would write: the command adds the extension.
            let target = crate::projects::with_extension(std::path::PathBuf::from(path));
            let own = self
                .run("project_save", |app| {
                    crate::projects::project_summary(app.state(), None)
                })
                .await?
                .and_then(|project| project.path)
                .is_some_and(|own| std::path::Path::new(&own) == target);
            if !own {
                may_write(&target, p.overwrite)?;
            }
        }
        let summary = self
            .write("project_save", false, move |app| match path {
                Some(path) => crate::projects::save_project_as(app.state(), path),
                None => crate::projects::save_project(app.state()),
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Builds a whole race in one step, as the interface's Open project from tracker… does: downloads the race from the tracker and opens a new project with one boat tab per boat, each holding that boat's track and the ORC and ORR certificates (and track-library tracks) that match it. Answers the project and, per boat, its details from the tracker (model, sail number, builder…), how many certificates and tracks were found and what is missing. Matching is by the tracker's own data and can miss (Geovoile gives only names and sail numbers, so little is matched there): follow it per boat with orc_search/orr_search, library_search and tracker_event using what you know of each boat (see the guide's \"A whole race\"). Then weather_fetch gives the tracks their wind. Replaces the open project: refused with unsaved changes ; save first or use project_discard. Cancelling the call cancels the download."
    )]
    async fn race_project(
        &self,
        Parameters(p): Parameters<RaceProjectParams>,
        ctx: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> ToolResult {
        if p.discard_unsaved {
            return Err(ToolError::Refused("Use project_discard to approve discarding unsaved work, then retry this tool with discard_unsaved false.".into()));
        }
        let tracker = match p.tracker.as_str() {
            "yellowbrick" => pe_core::track::Tracker::YellowBrick,
            "geovoile" => pe_core::track::Tracker::Geovoile,
            "bluewater" => pe_core::track::Tracker::BlueWaterTracks,
            other => {
                return Err(ToolError::Refused(format!(
                    "race_project builds races from \"yellowbrick\", \"geovoile\" or \"bluewater\", not \"{other}\""
                )));
            }
        };
        let mode = match p.match_mode.as_deref() {
            None | Some("identical_model") => BoatMatchMode::IdenticalModel,
            Some("exact_boat") => BoatMatchMode::ExactBoat,
            Some(other) => {
                return Err(ToolError::Refused(format!(
                    "match_mode is \"identical_model\" or \"exact_boat\", not \"{other}\""
                )));
            }
        };
        self.note("race_project");
        let app = self.app.clone();
        let (url, discard, classes) = (p.url, p.discard_unsaved, p.classes);
        // The download and matching run as the interface's do, off the
        // async thread; the preview they leave is confirmed below.
        let mut built = tokio::task::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let client = crate::trackers::client_of(tracker)?.into();
            tracker_project::open_with_mode(
                state.inner(),
                client,
                &url,
                discard,
                mode,
                &classes.unwrap_or_default(),
            )
        });
        let preview = tokio::select! {
            joined = &mut built => joined.map_err(|error| {
                ToolError::from(crate::error::AppError::Internal(format!("the race import task failed: {error}")))
            })??,
            // `rmcp` cancels the request's token rather than dropping this
            // future: stop the download, which the interface's Cancel does too.
            () = ctx.ct.cancelled() => {
                tracker_project::cancel_boat_import(self.app.state());
                return Err(ToolError::Refused("the race import was cancelled with its request; nothing was opened".to_owned()));
            }
        };
        let project_id = preview.project.id;
        let project = self
            .write("race_project", true, move |app| {
                tracker_project::confirm(app.state::<AppState>().inner(), project_id)
            })
            .await?;
        json(&serde_json::json!({
            "project": project,
            "boats": preview.boats,
            "warnings": preview.warnings,
        }))
    }

    #[tool(
        description = "Closes the project and returns the interface to the start screen. Refused with unsaved changes ; save first or use project_discard."
    )]
    async fn project_close(&self, Parameters(p): Parameters<DiscardParams>) -> ToolResult {
        if p.discard_unsaved {
            return Err(ToolError::Refused("Use project_discard to approve discarding unsaved work, then retry this tool with discard_unsaved false.".into()));
        }
        self.write("project_close", true, move |app| {
            crate::projects::close_project(app.state(), p.discard_unsaved)
        })
        .await?;
        json(&serde_json::json!({ "closed": true }))
    }

    #[tool(
        description = "Recently opened projects, newest first: each one's path, name and whether the file is still there."
    )]
    async fn recent_projects(&self) -> ToolResult {
        let recent = self
            .run("recent_projects", |app| {
                crate::projects::recent_projects(app.state())
            })
            .await?;
        json(&recent)
    }
}
