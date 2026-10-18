//! The view group (spec.md 3.7): which stage and boat the person is shown,
//! what is selected, and a picture of it. None of these is an edit: they
//! tell the frontend, which applies them as its own controls would.

use base64::Engine;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData as McpError};
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use tauri::{Emitter, Manager};

use super::{PolarExplorer, ToolError, ToolResult, events, json};
use crate::commands::AppState;
use crate::error::AppError;

/// The stages a client can ask for.
const STAGES: [&str; 4] = ["3d", "map", "compare", "plot"];

/// How long `screenshot` waits for the frontend: a little longer than a
/// throttled window takes to draw a frame, then give up rather than hang
/// the client.
const CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(35);

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewStageParams {
    /// "3d" (the 3D polar), "map" (the tracks on the world map; needs a
    /// track), "compare", or "plot" (the 2D polar plot, full size).
    pub stage: String,
    /// The boat whose stage to show, and whose tab; from boats_list.
    /// Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewBoatParams {
    /// The boat's id, from boats_list.
    pub boat: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectionParams {
    /// The boat the samples belong to; without it, the boat on show.
    #[serde(default)]
    pub boat: Option<u64>,
    /// Sample ids, from track_samples. An empty list clears the selection.
    pub samples: Vec<u64>,
}

/// Refuses a boat id the project does not hold.
fn known_boat(state: &AppState, boat: u64) -> crate::error::Result<()> {
    if crate::boats::list(state)?
        .tabs
        .iter()
        .any(|tab| tab.id == boat)
    {
        Ok(())
    } else {
        Err(AppError::BadOption {
            field: "Boat",
            value: boat.to_string(),
        })
    }
}

#[tool_router(router = tool_router_view, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "Shows the user a stage of a boat (the first, unless `boat` names one), on that boat's tab: the 3D polar view, the map of tracks, Compare, or the full-size 2D polar plot. Changes what is on screen, not the project."
    )]
    async fn view_stage(&self, Parameters(p): Parameters<ViewStageParams>) -> ToolResult {
        if !STAGES.contains(&p.stage.as_str()) {
            return Err(ToolError::Refused(format!(
                "there is no stage {:?}; the stages are {}",
                p.stage,
                STAGES.join(", ")
            )));
        }
        let stage = p.stage.clone();
        let boat = p.boat;
        let boat = self
            .run("view_stage", move |app| {
                let state = app.state::<AppState>();
                if let Some(boat) = boat {
                    known_boat(&state, boat)?;
                }
                let summary = crate::projects::summary(&state.scoped(boat))?
                    .ok_or(AppError::NoProjectOpen)?;
                // The interface offers the map only once there is a track.
                if stage == "map" && !summary.sources.iter().any(|source| source.kind == "track") {
                    return Err(AppError::BadOption {
                        field: "Stage",
                        value: "map: this boat has no track to show on it".to_owned(),
                    });
                }
                // One event naming the boat: the frontend shows that boat's
                // tab and sets that boat's stage, whichever was on show.
                let boat = summary.id;
                let _ = app.emit(events::STAGE, events::ViewStage { stage, boat });
                Ok(boat)
            })
            .await?;
        json(&serde_json::json!({ "stage": p.stage, "boat": boat }))
    }

    #[tool(
        description = "Shows the user a boat's tab. Changes what is on screen, not the project."
    )]
    async fn view_boat(&self, Parameters(p): Parameters<ViewBoatParams>) -> ToolResult {
        let boat = p.boat;
        self.run("view_boat", move |app| {
            known_boat(&app.state::<AppState>(), boat)?;
            let _ = app.emit(events::BOAT, events::ViewBoat { boat });
            Ok(())
        })
        .await?;
        json(&serde_json::json!({ "boat": boat }))
    }

    #[tool(
        description = "Selects samples in the interface, as a selection on the map would: they are highlighted on the map and in the polar views, where the user can exclude them. An empty list clears the selection. Not an edit."
    )]
    async fn selection_set(&self, Parameters(p): Parameters<SelectionParams>) -> ToolResult {
        let count = p.samples.len();
        self.run("selection_set", move |app| {
            let state = app.state::<AppState>();
            if let Some(boat) = p.boat {
                known_boat(&state, boat)?;
            }
            crate::projects::summary(&state)?.ok_or(AppError::NoProjectOpen)?;
            let _ = app.emit(
                events::SELECTION,
                events::ViewSelection {
                    boat: p.boat,
                    samples: p.samples,
                },
            );
            Ok(())
        })
        .await?;
        json(&serde_json::json!({ "selected": count }))
    }

    #[tool(
        description = "A picture of the stage the user is looking at (the 3D polar, the map, Compare or the full-size plot), as a PNG. Needs an open project and the window on screen."
    )]
    async fn screenshot(&self) -> std::result::Result<CallToolResult, ToolError> {
        // With no project no stage is mounted and nothing would answer;
        // refuse now rather than after the timeout.
        self.run("screenshot", |app| {
            crate::projects::summary(&app.state::<AppState>())?
                .map(|_| ())
                .ok_or(AppError::NoProjectOpen)
        })
        .await?;
        // Neither `run` nor `write` for the capture itself: the work is the
        // frontend's, and no document changes.
        let service = self.app.state::<super::super::McpService>();
        // The guard forgets the request on every way out of here, a dropped
        // future included.
        let mut pending = service.captures.request(&self.app);
        match tokio::time::timeout(CAPTURE_TIMEOUT, pending.receiver()).await {
            Ok(Ok(Ok(png))) => {
                let data = base64::engine::general_purpose::STANDARD.encode(png);
                Ok(CallToolResult::success(vec![ContentBlock::image(
                    data,
                    "image/png",
                )]))
            }
            Ok(Ok(Err(reason))) => Err(ToolError::Refused(format!(
                "the picture could not be taken: {reason}"
            ))),
            _ => Err(ToolError::Internal(McpError::internal_error(
                "the interface did not answer the capture: is the window shown and a project still open?",
                None,
            ))),
        }
    }
}
