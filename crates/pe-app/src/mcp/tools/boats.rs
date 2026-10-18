//! The boats group (spec.md 3.7): a project's independent boat tabs.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use tauri::Manager;

use super::{PolarExplorer, ToolResult, json};
use crate::commands::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoatAddParams {
    /// The new boat's name.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoatRenameParams {
    /// The boat's id, from boats_list.
    pub boat: u64,
    /// Its new name.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoatRefParams {
    /// The boat's id, from boats_list.
    pub boat: u64,
}

/// The id every boat command names the project by: the first boat's.
fn project_id(state: &AppState) -> crate::error::Result<u64> {
    crate::boats::list(state).map(|tabs| tabs.project_id)
}

#[tool_router(router = tool_router_boats, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "The project's boats: each tab's id and name, in order. A project holds independent boats, each with its own sources, grid and blend; pass a boat's id as `boat` to any other tool to act on that boat (without it, tools act on the first)."
    )]
    async fn boats_list(&self) -> ToolResult {
        let tabs = self
            .run("boats_list", |app| crate::boats::boat_tabs(app.state()))
            .await?;
        json(&tabs)
    }

    #[tool(
        description = "Adds a boat to the project: a new, empty tab with its own sources and blend."
    )]
    async fn boat_add(&self, Parameters(p): Parameters<BoatAddParams>) -> ToolResult {
        let summary = self
            .write("boat_add", false, move |app| {
                crate::boats::add_boat(app.state(), p.name)
            })
            .await?;
        json(&summary)
    }

    #[tool(description = "Renames a boat. One undo step.")]
    async fn boat_rename(&self, Parameters(p): Parameters<BoatRenameParams>) -> ToolResult {
        let summary = self
            .write("boat_rename", false, move |app| {
                crate::boats::rename_boat(app.state(), p.boat, p.name)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Removes a boat's tab from the project. Its data is kept until the project closes: boat_restore puts the last removed boat back."
    )]
    async fn boat_remove(&self, Parameters(p): Parameters<BoatRefParams>) -> ToolResult {
        let summary = self
            .write("boat_remove", false, move |app| {
                let state = app.state::<AppState>();
                let project = project_id(&state)?;
                crate::boats::delete_boat(state, project, p.boat)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Puts the most recently removed boat back, with its sources and its own undo history."
    )]
    async fn boat_restore(&self) -> ToolResult {
        let summary = self
            .write("boat_restore", false, |app| {
                let state = app.state::<AppState>();
                let project = project_id(&state)?;
                crate::boats::restore_boat(state, project)
            })
            .await?;
        json(&summary)
    }
}
