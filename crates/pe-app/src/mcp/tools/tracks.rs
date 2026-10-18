//! The tracks group (spec.md 3.7): importing race tracks from files and
//! from trackers, their sample filters, and the samples themselves.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use tauri::Manager;

use super::{PolarExplorer, ToolError, ToolResult, absolute, json, patch_over, typed};
use crate::commands::AppState;
use crate::error::AppError;
use crate::tracks::{TrackFileRequest, TrackFilters};

/// `track_samples` answers this many samples unless asked for another number.
const DEFAULT_SAMPLES: usize = 200;
/// The most one call answers; more is a page, asked for with `offset`.
const MAX_SAMPLES: usize = 2000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibrarySearchParams {
    /// Words to find: a boat's name, sail number, model or class, builder or
    /// event. Every word must match; accents and case do not matter.
    pub query: String,
    /// Skip this many hits, for the next page (a page is 100).
    #[serde(default)]
    pub offset: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryImportParams {
    /// The track's `id` from library_search.
    pub id: String,
    /// The boat to import it into, from boats_list; the first boat without it.
    #[serde(default)]
    pub boat: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectParams {
    /// Track files to look into: GPX, GeoJSON, CSV or an archive of them. Each
    /// path absolute, or beginning with `~/`.
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackImportParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The files to import, a list of objects: {"path": "/abs/race.csv",
    /// "mapping": <a CSV's column mapping, as track_files_inspect proposed
    /// it under csv.mapping; null for other files>, "boats": <the boat
    /// names to import from the file, or null for every boat in it>}.
    pub files: Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackerEventParams {
    /// The tracker: "yellowbrick", "geovoile" or "bluewater".
    pub tracker: String,
    /// The event's address on that tracker, as it appears in a browser.
    pub url: String,
    /// Download again even though this session already holds the event.
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackerImportParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The tracker, as given to tracker_event.
    pub tracker: String,
    /// The event's `key`, from tracker_event.
    pub key: String,
    /// The ids of the event's boats to import, from tracker_event.
    pub boats: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackSetParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The track's source id, from sources_list.
    pub source: u64,
    /// Sample filters to change, an object holding only the ones to
    /// change (null clears a limit): time_start, time_end (UTC epoch
    /// seconds); min_bsp, max_bsp (kn); max_heading_change,
    /// max_awa_change, max_wind_direction_change (degrees);
    /// max_wind_speed_change (kn); tws_min, tws_max (kn); twa_min, twa_max
    /// (degrees); hs_min, hs_max (m); current_min, current_max (kn);
    /// heading_origin, speed_origin ("any", "given", "derived"); wave_mode
    /// ("off", "sectors", "relative", "absolute") with wave_sectors
    /// (["head","bow","beam","quarter","following"]), wave_min, wave_max,
    /// wave_from, wave_to (degrees); exclude_no_tide, exclude_unknown_wave,
    /// exclude_unknown_current (true/false); tack_gybe_padding_s,
    /// stop_speed_kn, stop_padding_s, utc_interval_s. The track's current
    /// filters are in sources_list under track.filters.
    #[serde(default)]
    pub filters: Option<Value>,
    /// The longest gap between positions, seconds, across which heading
    /// and speed are still derived.
    #[serde(default)]
    pub max_gap_s: Option<i64>,
    /// Which heading to prefer where the file gave one: "given" or
    /// "derived" (from the positions).
    #[serde(default)]
    pub prefer_heading: Option<String>,
    /// Which speed to prefer where the file gave one: "given" or "derived".
    #[serde(default)]
    pub prefer_speed: Option<String>,
    /// Use only downloaded (reanalysis) wind, ignoring wind the track
    /// itself supplied.
    #[serde(default)]
    pub downloaded_wind_only: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackSamplesParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// Only this track's samples; without it, every visible track's.
    #[serde(default)]
    pub source: Option<u64>,
    /// How many samples to skip: the next page.
    #[serde(default)]
    pub offset: Option<usize>,
    /// How many to answer, at most 2000 (200 when omitted).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SampleGetParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The track's source id.
    pub source: u64,
    /// The sample's id, from track_samples.
    pub sample: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SamplesExcludeParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// Sample ids, from track_samples.
    pub samples: Vec<u64>,
    /// true takes them out of the blend; false puts them back.
    pub excluded: bool,
}

/// Runs an asynchronous command to its end on this (blocking) thread.
/// Some of the interface's commands are `async fn`s so that Tauri runs them
/// off the main thread; a tool is already off it (`PolarExplorer::run`), and
/// calls the same command rather than a twin of it.
fn wait<T>(command: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(command)
}

/// A track's summary, as the interface reads it.
fn track_summary(
    state: &AppState,
    source: u64,
) -> crate::error::Result<crate::tracks::TrackSummary> {
    crate::projects::summary(state)?
        .ok_or(AppError::NoProjectOpen)?
        .sources
        .into_iter()
        .find(|candidate| candidate.id == source)
        .ok_or(AppError::Core(pe_core::CoreError::MissingSource(source)))?
        .track
        .ok_or(AppError::BadOption {
            field: "Source",
            value: format!("{source} is not a track"),
        })
}

/// Filters that take nothing out: what a new global filter or priority
/// group starts from, as the interface's own (`NO_SAMPLE_FILTERS`).
pub(super) fn no_filters() -> std::result::Result<TrackFilters, ToolError> {
    typed(
        "filters",
        serde_json::json!({
            "time_start": null, "time_end": null, "min_bsp": null, "max_bsp": null,
            "max_heading_change": null, "max_awa_change": null,
            "max_wind_speed_change": null, "max_wind_direction_change": null,
            "heading_origin": "any", "speed_origin": "any",
            "tws_min": null, "tws_max": null, "twa_min": null, "twa_max": null,
            "hs_min": null, "hs_max": null, "current_min": null, "current_max": null,
            "wave_mode": "off", "wave_sectors": [], "wave_min": null, "wave_max": null,
            "wave_from": null, "wave_to": null, "exclude_no_tide": false,
            "exclude_unknown_wave": false, "exclude_unknown_current": false,
            "tack_gybe_padding_s": null, "stop_speed_kn": null, "stop_padding_s": 0,
            "utc_interval_s": null,
        }),
    )
}

/// `patch`'s fields written over `current`: a client names only the
/// filters it changes, and the rest stay as they are. `field` names the
/// parameter in a refusal.
pub(super) fn patched(
    field: &str,
    current: &TrackFilters,
    patch: Value,
) -> std::result::Result<TrackFilters, ToolError> {
    let patch: serde_json::Map<String, Value> = typed(field, patch)?;
    let mut whole =
        serde_json::to_value(current).map_err(|e| ToolError::Refused(format!("{field}: {e}")))?;
    patch_over(field, &mut whole, patch)?;
    typed(field, whole)
}

#[tool_router(router = tool_router_tracks, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "Looks into track files before importing them: for each, its kind (gpx, geojson, csv, …), the boats it holds with their positions and time span, and for a CSV its header, first rows, row count and the proposed column `mapping` to pass to track_files_import. Reads the files only; changes nothing."
    )]
    async fn track_files_inspect(&self, Parameters(p): Parameters<InspectParams>) -> ToolResult {
        let paths = p
            .paths
            .iter()
            .map(|path| absolute(path))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let inspected = self
            .run("track_files_inspect", move |_| {
                wait(crate::tracks::inspect_track_files(paths))
            })
            .await?;
        json(&inspected)
    }

    #[tool(
        description = "Imports race tracks from files as track sources of the boat, one undo step for all. Positions only: a track has no place in the polar until its weather is fetched (weather_fetch) unless the file supplied wind. Answers the `project`, each track `imported` with its source_id, and `failures`."
    )]
    async fn track_files_import(&self, Parameters(p): Parameters<TrackImportParams>) -> ToolResult {
        let mut files: Vec<TrackFileRequest> = typed("files", p.files)?;
        for file in &mut files {
            file.path = absolute(&file.path)?;
        }
        let result = self
            .write("track_files_import", false, move |app| {
                wait(crate::tracks::import_track_files(
                    app.state(),
                    p.boat,
                    files,
                ))
            })
            .await?;
        json(&result)
    }

    #[tool(
        description = "Searches the user's track library (a SYRF boat metadata file and GeoJSON folder chosen in Settings) for tracks of earlier races: by boat name, sail number, model or class, builder or event. Answers `total` and up to 100 hits, each with its `id`, boat, sail number, model, event, dates and whether its file is available; `downloaded: false` means there is no metadata file, so there is nothing to search. Read-only and local."
    )]
    async fn library_search(&self, Parameters(p): Parameters<LibrarySearchParams>) -> ToolResult {
        let found = self
            .run("library_search", move |app| {
                crate::library::catalogue::search(
                    app.state::<AppState>().inner(),
                    &p.query,
                    p.offset,
                )
            })
            .await?;
        json(&found)
    }

    #[tool(
        description = "Imports one track from the user's track library (its `id` from library_search) into a boat as a track source, one undo step. Positions only: follow with weather_fetch for its wind, waves and current. Refused when the track's file is not available."
    )]
    async fn library_import(&self, Parameters(p): Parameters<LibraryImportParams>) -> ToolResult {
        let result = self
            .write("library_import", false, move |app| {
                crate::library::catalogue::import_database_track(app.state(), p.boat, p.id)
            })
            .await?;
        json(&result)
    }

    #[tool(
        description = "Downloads a race from a tracker (YellowBrick, Geovoile, Blue Water Tracks) and answers its title, dates, `key` and every boat with its id and name; nothing is imported yet. This reaches the tracker's site and can take a while; the event is then kept for this session. Follow with tracker_import."
    )]
    async fn tracker_event(&self, Parameters(p): Parameters<TrackerEventParams>) -> ToolResult {
        let app = self.app.clone();
        self.note("tracker_event");
        // The command is itself asynchronous: it runs the download on a
        // blocking thread and emits the interface's own progress events.
        let event = crate::trackers::tracker_event(
            app,
            p.tracker,
            p.url,
            p.refresh,
            "mcp".to_owned(),
            None,
        )
        .await
        .map_err(ToolError::from)?;
        json(&event)
    }

    #[tool(
        description = "Imports the chosen boats of a downloaded tracker event as track sources of the boat, one undo step. Positions only: follow with weather_fetch for their wind, waves and current."
    )]
    async fn tracker_import(&self, Parameters(p): Parameters<TrackerImportParams>) -> ToolResult {
        let result = self
            .write("tracker_import", false, move |app| {
                wait(crate::trackers::import_tracker_boats(
                    app.state(),
                    p.boat,
                    p.tracker,
                    p.key,
                    p.boats,
                ))
            })
            .await?;
        json(&result)
    }

    #[tool(
        description = "Changes a track's sample filters, how its heading and speed are derived, or which wind it uses; give the ones to change. Filters decide which samples make the track's polar segment: the raw positions are never changed. `filters` holds only the filters to change. Each of filters, derivation and wind choice is one undo step."
    )]
    async fn track_set(&self, Parameters(p): Parameters<TrackSetParams>) -> ToolResult {
        let derivation =
            p.max_gap_s.is_some() || p.prefer_heading.is_some() || p.prefer_speed.is_some();
        if p.filters.is_none() && !derivation && p.downloaded_wind_only.is_none() {
            return Err(ToolError::Refused(
                "there is nothing to change: give filters, max_gap_s, prefer_heading, prefer_speed or downloaded_wind_only"
                    .to_owned(),
            ));
        }
        // What the track has now: the filters a patch is written over, and
        // the half of the derivation a client did not name.
        let boat = p.boat;
        let source = p.source;
        let current = self
            .run("track_set", move |app| {
                track_summary(&app.state::<AppState>().scoped(boat), source)
            })
            .await?;
        let filters = p
            .filters
            .map(|patch| patched("filters", &current.filters, patch))
            .transpose()?;
        let summary = self
            .write("track_set", false, move |app| {
                let state = || app.state::<AppState>();
                let mut last = None;
                if let Some(filters) = filters {
                    last = Some(crate::tracks::set_track_filters(
                        state(),
                        boat,
                        source,
                        filters,
                    )?);
                }
                if derivation {
                    last = Some(crate::tracks::set_track_derivation(
                        state(),
                        boat,
                        source,
                        p.max_gap_s.unwrap_or(current.max_gap_s),
                        p.prefer_heading.unwrap_or(current.prefer_heading),
                        p.prefer_speed.unwrap_or(current.prefer_speed),
                    )?);
                }
                if let Some(downloaded_only) = p.downloaded_wind_only {
                    last = Some(crate::tracks::set_track_wind(
                        state(),
                        boat,
                        source,
                        downloaded_only,
                    )?);
                }
                last.ok_or(AppError::Internal(
                    "track_set had nothing to apply".to_owned(),
                ))
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "The samples that have a place in the polar (those with wind), as the plots draw them: `total`, and a page of `samples`, each with its track `source`, `sample` id, twa (degrees), tws and bsp (knots), whether it is `filtered` out or `excluded` by hand, and the `time_of_day` it was sailed at (night, morning, afternoon, evening by local solar time)."
    )]
    async fn track_samples(&self, Parameters(p): Parameters<TrackSamplesParams>) -> ToolResult {
        let offset = p.offset.unwrap_or(0);
        let limit = p.limit.unwrap_or(DEFAULT_SAMPLES).clamp(1, MAX_SAMPLES);
        let page = self
            .run("track_samples", move |app| {
                // The 2D plot's own read: every sample with wind, the
                // filtered ones included and flagged.
                let state = app.state::<AppState>().scoped(p.boat);
                let dots = crate::polar_plot::dots(&state, None, true)?;
                let wanted: Vec<_> = dots
                    .iter()
                    .filter(|dot| p.source.is_none_or(|source| source == dot.source_id))
                    .collect();
                let samples: Vec<Value> = wanted
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|dot| {
                        serde_json::json!({
                            "source": dot.source_id,
                            "sample": dot.sample_id,
                            "twa": dot.twa,
                            "tws": dot.tws,
                            "bsp": dot.bsp,
                            "filtered": dot.filtered,
                            "excluded": dot.excluded,
                            "time_of_day": match dot.band {
                                pe_tracks::daytime::DayBand::Night => "night",
                                pe_tracks::daytime::DayBand::Morning => "morning",
                                pe_tracks::daytime::DayBand::Afternoon => "afternoon",
                                pe_tracks::daytime::DayBand::Evening => "evening",
                            },
                        })
                    })
                    .collect();
                Ok(serde_json::json!({ "total": wanted.len(), "offset": offset, "samples": samples }))
            })
            .await?;
        json(&page)
    }

    #[tool(
        description = "One sample in full: its time (UTC epoch seconds), position, heading and speed and whether each was given or derived, boat speed, TWS, TWA, wave height, current speed and direction (toward), and whether it is filtered or excluded."
    )]
    async fn sample_get(&self, Parameters(p): Parameters<SampleGetParams>) -> ToolResult {
        let details = self
            .run("sample_get", move |app| {
                crate::tracks::sample_details(app.state(), p.boat, p.source, p.sample)
            })
            .await?;
        json(&details)
    }

    #[tool(
        description = "Takes samples out of the blend by hand, or puts them back: outliers the filters do not catch. One undo step. The samples stay in the track, drawn hollow."
    )]
    async fn samples_exclude(&self, Parameters(p): Parameters<SamplesExcludeParams>) -> ToolResult {
        let summary = self
            .write("samples_exclude", false, move |app| {
                crate::polar3d::set_excluded(app.state(), p.boat, Vec::new(), p.samples, p.excluded)
            })
            .await?;
        json(&summary)
    }
}
