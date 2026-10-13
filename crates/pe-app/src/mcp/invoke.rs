//! `invoke`: any IPC command by name, for what no curated tool covers
//! (spec.md 3.7).
//!
//! One entry per command, calling the command function with the state the
//! handle gives — so a call through the escape hatch is the same call the
//! interface makes, undo and all. The test at the bottom holds the table
//! equal to `generate_handler!` minus [`EXCLUDED`], which is what makes a
//! command added later reachable the day it lands, or someone has said why
//! not.

use serde_json::Value;
use tauri::Manager;

use crate::commands::AppState;
use crate::error::{AppError, Result};

/// A command, reduced to the one shape the table can hold.
///
/// Not generic over the runtime, unlike the rest of this module: a `const`
/// of function pointers cannot be. Every entry only ever needs the managed
/// [`AppState`], and `tauri::State` is not parameterised by the runtime, so
/// [`call`] does the one `app.state()` and hands the result on.
type Handler = for<'a> fn(tauri::State<'a, AppState>, Value) -> Result<Value>;

/// A refusal that names the command, since the client sees one `invoke`.
fn bad(name: &str, why: impl std::fmt::Display) -> AppError {
    AppError::BadOption {
        field: "Command",
        value: format!("{name}: {why}"),
    }
}

/// Builds one entry: the command name, a struct of its arguments, the call.
///
/// The arguments are named and typed exactly as the command declares them,
/// so a signature that changes under the table is a compile error rather
/// than a refusal at run time. Unknown keys are refused rather than
/// ignored: a client that misspells an argument hears about it instead of
/// watching a default apply.
///
/// `async` entries are the interface's `async fn` commands, run to their
/// end on the tool's blocking thread; `stateless` ones take no state.
macro_rules! command {
    (@args $name:ident, $args:ident, { $($field:ident : $ty:ty),* }) => {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Args { $($field: $ty),* }
        let Args { $($field),* } =
            serde_json::from_value($args).map_err(|e| bad(stringify!($name), e))?;
    };
    ($name:ident, $path:path, { $($field:ident : $ty:ty),* $(,)? }) => {{
        fn handler(state: tauri::State<'_, AppState>, args: Value) -> Result<Value> {
            command!(@args $name, args, { $($field : $ty),* });
            let out = $path(state, $($field),*)?;
            serde_json::to_value(out).map_err(|e| bad(stringify!($name), e))
        }
        (stringify!($name), handler as Handler)
    }};
    (async $name:ident, $path:path, { $($field:ident : $ty:ty),* $(,)? }) => {{
        fn handler(state: tauri::State<'_, AppState>, args: Value) -> Result<Value> {
            command!(@args $name, args, { $($field : $ty),* });
            let out = tauri::async_runtime::block_on($path(state, $($field),*))?;
            serde_json::to_value(out).map_err(|e| bad(stringify!($name), e))
        }
        (stringify!($name), handler as Handler)
    }};
    (stateless $name:ident, $path:path, { $($field:ident : $ty:ty),* $(,)? }) => {{
        fn handler(_state: tauri::State<'_, AppState>, args: Value) -> Result<Value> {
            command!(@args $name, args, { $($field : $ty),* });
            let out = $path($($field),*)?;
            serde_json::to_value(out).map_err(|e| bad(stringify!($name), e))
        }
        (stringify!($name), handler as Handler)
    }};
    (stateless async $name:ident, $path:path, { $($field:ident : $ty:ty),* $(,)? }) => {{
        fn handler(_state: tauri::State<'_, AppState>, args: Value) -> Result<Value> {
            command!(@args $name, args, { $($field : $ty),* });
            let out = tauri::async_runtime::block_on($path($($field),*))?;
            serde_json::to_value(out).map_err(|e| bad(stringify!($name), e))
        }
        (stringify!($name), handler as Handler)
    }};
}

/// `cancel_boat_import`: the one command that answers nothing at all.
fn cancel_boat_import(state: tauri::State<'_, AppState>, args: Value) -> Result<Value> {
    command!(@args cancel_boat_import, args, {});
    crate::boats::tracker_project::cancel_boat_import(state);
    Ok(Value::Null)
}

/// Commands the escape hatch will not run.
pub const EXCLUDED: &[&str] = &[
    // The service itself, which a client reached through it could turn off
    // under its own feet, and the frontend's two answers to a screenshot
    // request. Registering the service with a client is asked for by a
    // person in Settings.
    "mcp_status",
    "mcp_set",
    "mcp_rotate_token",
    "mcp_register_client",
    "deliver_capture",
    "refuse_capture",
    // The person's preferences. Every one of these answers the whole
    // settings file, which holds this service's own token: that is no
    // client's to read.
    // `set_language` also relabels the native menu from the handle, and
    // `remove_old_chunk_cache` deletes a folder on disk.
    "app_settings",
    "set_language",
    "set_theme",
    "set_units",
    "set_autosave_mode",
    "set_weather_memory",
    "set_network",
    "set_data_source",
    "set_weather_cache",
    "weather_cache_status",
    "clear_weather_cache",
    "set_projection",
    "set_plot_band",
    "set_catalogue_schedule",
    "legacy_cache_notice",
    "remove_old_chunk_cache",
    // The track library: where its files are, and scraping races into it
    // with the person's YellowBrick credentials, are only the person's at
    // the keyboard (spec.md 3.7). Searching and importing what it holds are
    // the curated tools library_search and library_import (asked 2026-10-03).
    "set_library_settings",
    "start_library_scrape",
    "library_scrape_status",
    "cancel_library_scrape",
    // The SYRF database's read-only metadata download signs in with the
    // person's PostgreSQL password, so it too is the person's at the keyboard.
    "test_database_connection",
    "start_metadata_download",
    "metadata_download_status",
    "cancel_metadata_download",
    "search_database_boats",
    "library_metadata_available",
    "import_database_track",
    // Quitting is the person's, and goes through their unsaved-changes guard.
    "quit_app",
    // Answered with packed bytes for a canvas, not JSON: `polar_read`,
    // `track_samples`, `compare` and `screenshot` are what a client reads.
    "basemap",
    "map_tracks",
    "polar_scene",
    "polar_scene_split",
    "polar_plot_dots",
    // Derived Gaussian fields for the polar canvases; screenshot exposes the view.
    "polar_pdf",
    "compare_polars",
    // These take the application handle, for the progress they emit. The
    // curated tools carry them: `weather_fetch`, `weather_cancel`,
    "start_env_fetch",
    "cancel_env_fetch",
    "tracker_event",
    // `tracker_event`, and `orc_refresh` and `orr_refresh` for the
    // catalogues' scrapes.
    "start_orr_scrape",
    "start_orc_scrape",
    // These write a file where the caller says. Their tools (`project_save`,
    // `export_polar`) refuse to replace a file that is already there unless
    // told to, as the interface's save dialog asks; by name here they would
    // be the way around that.
    "save_project_as",
    "export_polar",
];

/// Every command reachable by name.
pub const TABLE: &[(&str, Handler)] = &[
    // Boats.
    command!(boat_tabs, crate::boats::boat_tabs, {}),
    command!(add_boat, crate::boats::add_boat, { name: String }),
    command!(rename_boat, crate::boats::rename_boat, { boat_context: u64, name: String }),
    command!(delete_boat, crate::boats::delete_boat, { project_id: u64, boat_id: u64 }),
    command!(restore_boat, crate::boats::restore_boat, { project_id: u64 }),
    command!(async export_all_polars, crate::boats::export_all_polars, { directory: String, format: String, matrix: Option<crate::matrix_export::MatrixExportOptions> }),
    command!(async open_tracker_project, crate::boats::tracker_project::open_tracker_project, { tracker: String, url: String, discard_unsaved: bool, match_mode: Option<crate::boats::tracker_project::BoatMatchMode>, classes: Option<Vec<String>> }),
    command!(confirm_tracker_project, crate::boats::tracker_project::confirm_tracker_project, { project_id: u64 }),
    command!(discard_tracker_project, crate::boats::tracker_project::discard_tracker_project, { project_id: u64 }),
    command!(
        boat_import_status,
        crate::boats::tracker_project::boat_import_status,
        {}
    ),
    ("cancel_boat_import", cancel_boat_import as Handler),
    command!(stateless app_info, crate::commands::app_info, {}),
    // Projects.
    command!(new_project, crate::projects::new_project, { name: String, boat: Option<crate::projects::BoatInput>, discard_unsaved: bool }),
    command!(open_project, crate::projects::open_project, { path: String, discard_unsaved: bool }),
    command!(save_project, crate::projects::save_project, {}),
    command!(close_project, crate::projects::close_project, { discard_unsaved: bool }),
    command!(project_summary, crate::projects::project_summary, { boat_context: Option<u64> }),
    command!(recent_projects, crate::projects::recent_projects, {}),
    command!(forget_recent_project, crate::projects::forget_recent_project, { path: String }),
    command!(clear_recent, crate::projects::clear_recent, {}),
    command!(recovered_projects, crate::autosave::recovered_projects, {}),
    command!(open_recovered, crate::autosave::open_recovered, { id: u64, discard_unsaved: bool }),
    command!(discard_recovered, crate::autosave::discard_recovered, { id: u64 }),
    // Edits and sources.
    command!(undo, crate::edit::undo, { boat_context: Option<u64> }),
    command!(redo, crate::edit::redo, { boat_context: Option<u64> }),
    command!(rename_project, crate::edit::rename_project, { name: String }),
    command!(set_source_colour, crate::edit::set_source_colour, { boat_context: Option<u64>, id: u64, colour: String }),
    command!(set_visible_sources, crate::edit::set_visible_sources, { boat_context: Option<u64>, ids: Vec<u64> }),
    command!(set_source_visible, crate::edit::set_source_visible, { boat_context: Option<u64>, id: u64, visible: bool }),
    command!(set_source_weight, crate::edit::set_source_weight, { boat_context: Option<u64>, id: u64, weight: f64, gesture: Option<String> }),
    command!(set_source_label, crate::edit::set_source_label, { boat_context: Option<u64>, id: u64, label: String }),
    command!(move_source, crate::edit::move_source, { boat_context: Option<u64>, id: u64, to: usize }),
    command!(remove_source, crate::edit::remove_source, { boat_context: Option<u64>, id: u64 }),
    command!(import_polar_files, crate::polar_files::import_polar_files, { boat_context: Option<u64>, paths: Vec<String> }),
    command!(polar_plot, crate::polar_plot::polar_plot, { boat_context: Option<u64>, tws: Option<f64> }),
    // Catalogues.
    command!(orc_catalogue_info, crate::orc::orc_catalogue_info, {}),
    command!(orc_scrape_status, crate::orc::orc_scrape_status, {}),
    command!(cancel_orc_scrape, crate::orc::cancel_orc_scrape, {}),
    command!(orc_search, crate::orc::orc_search, { boat_context: Option<u64>, query: String, filters: crate::orc::OrcFilters, limit: u32, offset: Option<u32> }),
    command!(orc_add, crate::orc::orc_add, { boat_context: Option<u64>, id: u32, allow_duplicate: bool }),
    command!(orr_catalogue_info, crate::orr::orr_catalogue_info, { boat_context: Option<u64> }),
    command!(orr_search, crate::orr::orr_search, { boat_context: Option<u64>, query: String, filters: crate::orc::OrcFilters, limit: u32, offset: u32 }),
    command!(orr_add, crate::orr::orr_add, { boat_context: Option<u64>, id: String }),
    command!(orr_scrape_status, crate::orr::orr_scrape_status, { boat_context: Option<u64> }),
    command!(cancel_orr_scrape, crate::orr::cancel_orr_scrape, { boat_context: Option<u64> }),
    // Tracks.
    command!(stateless async inspect_track_files, crate::tracks::inspect_track_files, { paths: Vec<String> }),
    command!(stateless async inspect_csv_track, crate::tracks::inspect_csv_track, { path: String, mapping: crate::tracks::CsvMappingInput }),
    command!(async import_track_files, crate::tracks::import_track_files, { boat_context: Option<u64>, files: Vec<crate::tracks::TrackFileRequest> }),
    command!(set_track_filters, crate::tracks::set_track_filters, { boat_context: Option<u64>, id: u64, filters: crate::tracks::TrackFilters }),
    command!(set_track_derivation, crate::tracks::set_track_derivation, { boat_context: Option<u64>, id: u64, max_gap_s: i64, prefer_heading: String, prefer_speed: String }),
    command!(set_track_wind, crate::tracks::set_track_wind, { boat_context: Option<u64>, id: u64, downloaded_only: bool }),
    command!(sample_details, crate::tracks::sample_details, { boat_context: Option<u64>, source_id: u64, sample_id: u64 }),
    command!(cancel_tracker_event, crate::trackers::cancel_tracker_event, { boat_context: Option<u64> }),
    command!(async import_tracker_boats, crate::trackers::import_tracker_boats, { boat_context: Option<u64>, tracker: String, key: String, boats: Vec<String> }),
    // Weather.
    command!(async env_estimate, crate::env::env_estimate, { boat_context: Option<u64>, source_ids: Vec<u64>, restart: bool }),
    command!(env_jobs, crate::env::env_jobs, { boat_context: Option<u64> }),
    command!(set_use_corrected, crate::env::set_use_corrected, { boat_context: Option<u64>, on: bool }),
    // The polar views' edits.
    command!(set_excluded, crate::polar3d::set_excluded, { boat_context: Option<u64>, nodes: Vec<crate::polar3d::PolarNodeRef>, samples: Vec<u64>, excluded: bool }),
    command!(polar_edit_surface, crate::polar_edit::polar_edit_surface, { boat_context: Option<u64>, source_id: u64 }),
    command!(edit_polar, crate::polar_edit::edit_polar, { boat_context: Option<u64>, source_id: u64, op: crate::polar_edit::EditOp, cells: Vec<crate::polar_edit::PolarCell>, gesture: Option<String> }),
    command!(set_segment_statistic, crate::polar_edit::set_segment_statistic, { boat_context: Option<u64>, source_id: u64, statistic: String }),
    // The blend and export.
    command!(set_blend_visible, crate::blend::set_blend_visible, { boat_context: Option<u64>, visible: bool }),
    command!(set_blend_colour, crate::blend::set_blend_colour, { boat_context: Option<u64>, colour: String }),
    command!(set_blend_settings, crate::blend::set_blend_settings, { boat_context: Option<u64>, settings: crate::blend::BlendSettingsInput }),
    command!(set_global_filters, crate::blend::set_global_filters, { boat_context: Option<u64>, filters: Option<crate::tracks::TrackFilters> }),
    command!(wave_range_maxima, crate::blend::wave_range_maxima, { boat_context: Option<u64> }),
    command!(set_wave_ranges, crate::blend::set_wave_ranges, { boat_context: Option<u64>, ranges: crate::blend::WaveRangesInput }),
    command!(set_priority_filters, crate::blend::set_priority_filters, { boat_context: Option<u64>, groups: Vec<crate::tracks::TrackFilters>, minimum: u32 }),
    command!(blend_cell, crate::blend::blend_cell, { boat_context: Option<u64>, twa_index: u32, tws_index: u32 }),
    command!(blend_cell_split, crate::blend::blend_cell_split, { boat_context: Option<u64>, twa_index: u32, tws_index: u32, split: crate::wave_split::WaveSplit, cell: u32 }),
    command!(export_preview, crate::blend::export_preview, { boat_context: Option<u64>, format: String, axes: Option<crate::blend::ExportAxes>, matrix: Option<crate::matrix_export::MatrixExportOptions> }),
];

/// The commands that leave a different project open, or none: `invoke`
/// tells the frontend so, and it resets what opening a project resets.
pub const OPENING: &[&str] = &[
    "new_project",
    "open_project",
    "close_project",
    "open_recovered",
    "confirm_tracker_project",
];

/// Runs a command by name.
pub fn call<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    name: &str,
    args: Value,
) -> Result<Value> {
    if EXCLUDED.contains(&name) {
        return Err(AppError::BadOption {
            field: "Command",
            value: format!("{name} is not available through invoke"),
        });
    }
    match TABLE.iter().find(|(n, _)| *n == name) {
        Some((_, handler)) => handler(app.state::<AppState>(), args),
        None => Err(AppError::BadOption {
            field: "Command",
            value: format!("unknown command {name}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every command in `generate_handler!` is either in the table or
    /// deliberately excluded, so a command added later is reachable the day
    /// it lands or someone has said why not.
    #[test]
    fn the_table_covers_every_registered_command() {
        let lib = include_str!("../lib.rs");
        let start = lib.find("generate_handler![").expect("handler list");
        let end = lib[start..].find("])").expect("end of list") + start;
        let names: Vec<&str> = lib[start..end]
            .split(',')
            .filter_map(|entry| entry.trim().rsplit("::").next())
            .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
            .collect();
        assert!(names.len() > 100, "parsed only {} names", names.len());
        let missing: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| !EXCLUDED.contains(n) && !TABLE.iter().any(|(t, _)| t == n))
            .collect();
        assert!(
            missing.is_empty(),
            "commands neither in TABLE nor EXCLUDED: {missing:?}"
        );
        let stale: Vec<&str> = TABLE
            .iter()
            .map(|(t, _)| *t)
            .chain(EXCLUDED.iter().copied())
            .filter(|t| !names.contains(t))
            .collect();
        assert!(stale.is_empty(), "table names no command has: {stale:?}");
        let both: Vec<&str> = TABLE
            .iter()
            .map(|(t, _)| *t)
            .filter(|t| EXCLUDED.contains(t))
            .collect();
        assert!(both.is_empty(), "both reachable and excluded: {both:?}");
        let opening: Vec<&&str> = OPENING
            .iter()
            .filter(|n| !TABLE.iter().any(|(t, _)| t == *n))
            .collect();
        assert!(
            opening.is_empty(),
            "OPENING names no table entry: {opening:?}"
        );
    }
}
