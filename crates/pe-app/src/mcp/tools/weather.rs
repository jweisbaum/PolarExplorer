//! The weather group (spec.md 3.7): reanalysis wind, waves and current for
//! a track's positions. The one group whose tools start a download, on the
//! person's agent's asking, through `pe-env` as the interface's own Fetch
//! weather does (invariant 4).

use std::time::Duration;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use tauri::Manager;

use super::{BoatParams, PolarExplorer, ToolResult, boat_id, json};
use crate::commands::AppState;
use crate::env::EnvJobsStatus;

/// How often `weather_fetch` looks at the job queue.
const POLL: Duration = Duration::from_millis(500);

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WeatherSourcesParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The track sources to fetch for, by id from sources_list.
    pub sources: Vec<u64>,
    /// Fetch everything again, not only what is missing.
    #[serde(default)]
    pub restart: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WeatherFetchParams {
    /// Reuse this key only when retrying the identical fetch.
    #[serde(default)]
    pub request_id: Option<String>,
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The track sources to fetch for, by id from sources_list.
    pub sources: Vec<u64>,
    /// Only "hourly" is supported. Omit to use hourly sampling.
    #[serde(default)]
    pub interval: Option<String>,
    /// Fetch everything again, not only what is missing.
    #[serde(default)]
    pub restart: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WeatherCancelParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The tracks whose fetches to cancel; without it, every one of the
    /// boat's.
    #[serde(default)]
    pub sources: Option<Vec<u64>>,
}

/// Whether a queued or running fetch is `boat`'s fetch of one of `sources`.
/// Source ids are each boat's own, so another boat's source 1 is not this
/// boat's.
fn ours(track: &crate::env::EnvJobTrack, boat: u64, sources: &[u64]) -> bool {
    track.boat_id == Some(boat) && sources.contains(&track.source_id)
}

/// Whether any of `boat`'s `sources` is still queued or fetching.
fn pending(status: &EnvJobsStatus, boat: u64, sources: &[u64]) -> bool {
    status.tracks.iter().any(|track| ours(track, boat, sources))
}

#[tool_router(router = tool_router_weather, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "How much fetching weather for these tracks would download, worked out from their positions without fetching anything: the samples to fetch, the hourly download size and what the cache already holds. Call it before weather_fetch and tell the user when the download is large."
    )]
    async fn weather_estimate(
        &self,
        Parameters(p): Parameters<WeatherSourcesParams>,
    ) -> ToolResult {
        let estimate = self
            .run("weather_estimate", move |app| {
                tauri::async_runtime::block_on(crate::env::env_estimate(
                    app.state(),
                    p.boat,
                    p.sources,
                    p.restart,
                ))
            })
            .await?;
        json(&estimate)
    }

    #[tool(
        description = "Fetches reanalysis wind, waves and current for the given tracks' positions: a download from the public ERA5 and ocean-current archives, which gives each sample its place in the polar. Returns a durable job_id immediately. Poll weather_jobs. The fetch continues if the assistant disconnects; weather_cancel explicitly cancels it and keeps fetched samples."
    )]
    async fn weather_fetch(&self, Parameters(p): Parameters<WeatherFetchParams>) -> ToolResult {
        let dir = self.app.state::<AppState>().paths.config_dir.join("mcp");
        let journal = super::super::native::journal(&dir).map_err(super::ToolError::Refused)?;
        let owner = boat_id(&self.app.state::<AppState>(), p.boat)?;
        let target=serde_json::json!({"boat":owner,"sources":p.sources,"restart":p.restart,"interval":p.interval.as_deref().unwrap_or("hourly")}).to_string();
        let key = p.request_id.clone().unwrap_or_else(|| {
            format!(
                "weather:{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            )
        });
        let (record, new) = journal
            .start(&key, "weather_fetch", &target)
            .map_err(|e| super::ToolError::Refused(e.to_string()))?;
        if !new {
            return json(&serde_json::json!({"job_id":record.id,"job":record,"reused":true}));
        }
        let sources = p.sources.clone();
        let boat = p.boat;
        let queued = self
            .run("weather_fetch", move |app| {
                crate::env::start_env_fetch(
                    app.clone(),
                    app.state(),
                    boat,
                    p.sources,
                    p.interval.unwrap_or_else(|| "hourly".into()),
                    p.restart,
                )
            })
            .await;
        let queued = match queued {
            Ok(status) => status,
            Err(error) => {
                let _ = journal.finish(
                    &record.id,
                    whirlwind_mcp::jobs::State::Failed,
                    None,
                    Some("Could not queue weather fetch".into()),
                );
                return Err(error);
            }
        };
        let app = self.app.clone();
        let id = record.id.clone();
        let journal_for_job = journal.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match crate::env::env_jobs(app.state(), boat) {
                    Ok(status) if pending(&status, owner, &sources) => {}
                    Ok(status) => {
                        let failed = status.failure.is_some();
                        let _ = journal_for_job.finish(
                            &id,
                            if failed {
                                whirlwind_mcp::jobs::State::Failed
                            } else {
                                whirlwind_mcp::jobs::State::Succeeded
                            },
                            serde_json::to_value(status).ok(),
                            None,
                        );
                        break;
                    }
                    Err(error) => {
                        let _ = journal_for_job.finish(
                            &id,
                            whirlwind_mcp::jobs::State::Failed,
                            None,
                            Some(error.to_string()),
                        );
                        break;
                    }
                }
                tokio::time::sleep(POLL).await;
            }
        });
        json(
            &serde_json::json!({"job_id":record.id,"state":"running","jobs":queued,"poll":"weather_jobs","cancel":"weather_cancel"}),
        )
    }

    #[tool(
        description = "Cancels the weather fetches of the given tracks of the boat, or all of that boat's. What was already fetched is kept."
    )]
    async fn weather_cancel(&self, Parameters(p): Parameters<WeatherCancelParams>) -> ToolResult {
        let owner = boat_id(&self.app.state::<AppState>(), p.boat)?;
        let cancelled = p.sources.clone();
        let dir = self.app.state::<AppState>().paths.config_dir.join("mcp");
        let journal = super::super::native::journal(&dir).map_err(super::ToolError::Refused)?;
        // Commit the cancellation before removing jobs from the queue, so the completion
        // monitor cannot mistakenly report success when it observes the emptied queue.
        for job in journal
            .list()
            .map_err(|e| super::ToolError::Refused(e.to_string()))?
        {
            let target = serde_json::from_str::<serde_json::Value>(&job.target).unwrap_or_default();
            if job.operation == "weather_fetch"
                && job.state == whirlwind_mcp::jobs::State::Running
                && target["boat"].as_u64() == Some(owner)
                && cancelled.as_ref().is_none_or(|ids| {
                    target["sources"].as_array().is_some_and(|sources| {
                        sources
                            .iter()
                            .filter_map(serde_json::Value::as_u64)
                            .any(|id| ids.contains(&id))
                    })
                })
            {
                journal.finish(&job.id,whirlwind_mcp::jobs::State::Cancelled,None,Some("Explicit cancellation; already fetched samples are retained. Check weather_jobs for any other tracks still running.".into())).map_err(|e|super::ToolError::Refused(e.to_string()))?;
            }
        }
        let status = self
            .run("weather_cancel", move |app| {
                // The boat by its id: the command reads no boat as every
                // boat's fetches, and a tool's missing boat is the first.
                let boat = boat_id(&app.state::<AppState>(), p.boat)?;
                crate::env::cancel_env_fetch(app.clone(), app.state(), Some(boat), p.sources)
            })
            .await?;
        json(&status)
    }

    #[tool(
        description = "List durable weather job IDs, results and interrupted jobs from earlier app runs. Detailed active progress is available from weather_jobs."
    )]
    async fn assistant_jobs(&self) -> ToolResult {
        let dir = self.app.state::<AppState>().paths.config_dir.join("mcp");
        let journal = super::super::native::journal(&dir).map_err(super::ToolError::Refused)?;
        json(
            &serde_json::json!({"jobs":journal.list().map_err(|e|super::ToolError::Refused(e.to_string()))?}),
        )
    }

    #[tool(
        description = "The weather job queue: the track being fetched and how far along it is, the ones waiting, and the last `failure` or `warning`."
    )]
    async fn weather_jobs(&self, Parameters(p): Parameters<BoatParams>) -> ToolResult {
        let status = self
            .run("weather_jobs", move |app| {
                crate::env::env_jobs(app.state(), p.boat)
            })
            .await?;
        let dir = self.app.state::<AppState>().paths.config_dir.join("mcp");
        let journal = super::super::native::journal(&dir).map_err(super::ToolError::Refused)?;
        let mut value =
            serde_json::to_value(status).map_err(|e| super::ToolError::Refused(e.to_string()))?;
        value["assistant_jobs"] = serde_json::to_value(
            journal
                .list()
                .map_err(|e| super::ToolError::Refused(e.to_string()))?,
        )
        .map_err(|e| super::ToolError::Refused(e.to_string()))?;
        json(&value)
    }
}
