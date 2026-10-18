//! What a client is told before it sees a single tool (spec.md 3.7).
//!
//! A tool's description says what the tool does; nothing in fifty of them
//! says which to reach for, or in what order a polar is built. This is the
//! part of the interface that routes a request, so it is written as
//! instructions to the agent.
//!
//! **It is said twice, at two lengths, because no client can be relied on to
//! show the first.** Claude Code cuts a server's instructions at 2,048
//! characters, and other clients are not known to show instructions at all.
//! So [`INSTRUCTIONS`] is short, leads with *when*, and fits; [`GUIDE`] is
//! everything, and is what the `polarexplorer_guide` tool returns to whoever
//! asks, and what the skill the application installs says (`mcp::skill`).
//!
//! Keep every name in both real: `tests/mcp.rs` checks each tool they
//! mention still exists.

use rmcp::{tool, tool_router};

use super::{PolarExplorer, ToolResult, json};

/// What Claude Code keeps of a server's instructions, in UTF-16 units; the
/// rest is replaced by "… [truncated]".
pub const INSTRUCTIONS_LIMIT: usize = 2048;

/// The line both texts open with.
///
/// The date leads because a model's sense of "now" is its training's: asked
/// for "the last Fastnet", it must count back from today, not from what it
/// remembers.
fn today(now: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "Today is {} (UTC). \"The last\" or \"the latest\" race counts back from today: work out which edition is the most recent one already finished and look for that year by number.",
        now.format("%A %-d %B %Y")
    )
}

/// Now, from the system clock. This crate's `chrono` has no clock feature
/// (nothing else here asks the time of it), so the instant is built from
/// `SystemTime`; a clock before 1970 reads as the epoch.
pub fn now() -> chrono::DateTime<chrono::Utc> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    chrono::DateTime::from_timestamp(i64::try_from(seconds).unwrap_or(0), 0).unwrap_or_default()
}

/// The server's `instructions`, sent once at initialisation: today's date,
/// then [`INSTRUCTIONS`]. Never longer than [`INSTRUCTIONS_LIMIT`].
pub fn instructions(now: chrono::DateTime<chrono::Utc>) -> String {
    format!("{}\n\n{INSTRUCTIONS}", today(now))
}

/// What `polarexplorer_guide` returns: today's date, then [`GUIDE`].
pub fn guide(now: chrono::DateTime<chrono::Utc>) -> String {
    format!("{}\n\n{GUIDE}", today(now))
}

/// The short text: when to use the application, and the order of work.
pub const INSTRUCTIONS: &str = "\
PolarExplorer is the sailing-polar application open on this computer. Use these tools whenever the user wants a boat's polar built, compared, corrected or exported (Expedition .txt, Adrena .pol, qtVlm .csv, OpenCPN .pol, LuckGrib .tsv, CSV), or asks how a boat really sailed in a race. Do not compute polars yourself: the application owns the data and the user watches it follow you.

Call polarexplorer_guide first: it is the full manual. In short:
1. project_status, then project_new or project_open. A dirty project holds the user's unsaved work: never discard it unless told to.
2. Add sources to a boat: orc_search + orc_add or orr_search + orr_add (rating certificates), polar_files_import (polar files), track_files_import, tracker_event + tracker_import or library_search + library_import (race tracks).
A whole race: race_project builds a tab per boat with its track and matched certificates; then, per boat, search again with what you know of it (model, sail number, other races). Carry a job through; stay within the requested download scope and preserve unsaved work.
3. A track has no place in the polar until it has wind: weather_estimate, then weather_fetch (downloads; say so first).
4. Shape it: source_set (weight 0–1, visible), track_set (filters), samples_exclude, polar_edit, blend_set.
5. Read it: polar_read, blend_cell (who is behind a cell), compare, screenshot.
6. export_preview, then export_polar to an absolute path.

Conventions: speeds in knots, angles in degrees, TWA 0–180 (0–360 when the polar is asymmetric), time as UTC epoch seconds, paths absolute or beginning with ~/. Every write is one undo step (undo, redo). Tools take an optional `boat` id from boats_list; without it they act on the first boat. A refusal is a tool result that says why in words: read it and act on it.";

/// The whole manual.
pub const GUIDE: &str = "\
# PolarExplorer

PolarExplorer builds a sailing polar (boat speed for each true wind angle and true wind speed) by blending three kinds of source: ORC and ORR rating certificates, imported polar files, and polar segments derived from race tracks matched with reanalysis wind, waves and current. It exports the blend for routing software. It is open on this computer and the user sees everything you do in it: each tool calls the same command the interface calls, each change is one undo step, and the views follow.

## When to use it

Whenever the user wants a polar built, examined, compared, corrected or exported, or wants to know how a boat performed in a race against its rating. Do not build polars in code or by hand.

## Order of work

1. **Look first.** project_status says what is open. `dirty: true` means unsaved work of the user's: project_new, project_open and project_close refuse to drop it. Save first, or use the explicit project_discard tool after authorization. recent_projects lists what was open before.
2. **A project and its boats.** project_new makes a project with one boat. A project holds independent boat tabs (boats_list, boat_add, boat_rename, boat_remove, boat_restore); each boat has its own sources, grid and blend. Every tool below takes an optional `boat` id and otherwise acts on the first boat.
3. **Sources.**
   - Rating certificates: orc_search then orc_add (ORC, embedded catalogue), or orr_search then orr_add (ORR). Search by boat name, sail number, model or year. Both catalogues are on this computer. orc_refresh and orr_refresh download the current certificates into them (a minute or two and about 60 MB for ORC, several minutes of requests for ORR: tell the user first), and are only needed when a recent certificate is missing.
   - Polar files: polar_files_import with absolute paths (Expedition .txt, Adrena .pol, qtVlm .csv, OpenCPN .pol, LuckGrib .csv/.tsv, CSV).
   - Tracks from files: track_files_inspect to see what a file holds, then track_files_import.
   - A whole race at once: race_project (see \"A whole race\" below).
   - Tracks from the user's track library: library_search, then library_import.
   - Tracks from a race tracker: tracker_event with the tracker's name (yellowbrick, geovoile, bluewater) and the event's address gives the boat list; tracker_import imports the chosen boats. This downloads from the tracker.
   - sources_list shows every source with its id, kind, colour, visibility, weight and counts. source_remove takes one out (undo puts it back); source_move changes only its place in the list.
4. **Weather for tracks.** A track sample has no place in the polar until it has wind. weather_estimate says how much would be downloaded; weather_fetch downloads reanalysis wind, waves and current for the given track sources and reports progress; weather_jobs and weather_cancel watch and stop it. Tell the user before a large download.
5. **Shape the result.**
   - source_set: label, colour, visible, weight. Weight runs from 0 to 1 (default 1). A hidden source is out of the blend and of every plot.
   - track_set: a track's sample filters (time window, boat-speed band, manoeuvres, waves, current), how heading and speed are derived, and whether to use the track's own wind.
   - track_samples lists the samples that have a place in the polar, with their ids; samples_exclude removes or restores individual samples; sample_get reads one in full.
   - blend_set: the output grid, the interpolation (linear or monotone_spline), smoothing, the samples a track cell needs and the samples for full confidence, asymmetric mode. blend_filters_set: filters over every track, wave ranges, priority groups.
   - polar_edit writes cell values on one source's overlay or, with no source, corrections on the blend. The imported data is never changed: every edit is an overlay.
6. **Read the result.** blend_status (settings and coverage), polar_read (a source's polar, a track's segment, or the blend as a TWA × TWS table), blend_cell (one cell: its value, whether it is direct evidence or filled, and each source behind it with its speed and share of the weight), compare (A against B per cell, with the regions where each is faster), screenshot (what the user sees). view_stage, view_boat and selection_set move the user's view.
7. **Export.** export_preview shows the grid as it would be written; export_polar writes it to an absolute path (format expedition, adrena, csv, qtvlm, opencpn, luckgrib or whirlwind_matrix_polar); export_all writes every boat's polar into a folder. WhirlwindMatrixPolar accepts matrix options (sog, sampled_pdf, gaussian_pdf, sog_bins, and optional waves with height/period {cutoffs:[...]} and direction angle {count:2|4|8|20} with equal bow-relative sectors centered on 0 degrees; defaults enable all modes and 4×4×2 wave bins, while waves:null omits waves). The same options apply to export_all. Export always recomputes from the sources.
8. **Save.** project_save writes the .wpsproj file; a project never saved needs `path`.

export_polar and a project_save to a new `path` refuse to replace a file that is already there unless `overwrite` is true: pass it only when the user means that file to be replaced.

## A whole race

When the user wants a race's boats loaded (\"load all the boats of the Fastnet\"), do the whole job without asking at each step:

1. **Find the race.** Work out the edition (see the date above) and its tracker page. If you can, read the race's own website and entry list for each boat's design, builder, sail number, owner's boat name and class: the tracker often knows less.
2. **race_project** with the tracker (yellowbrick, geovoile or bluewater) and the race's link or key. To build only some classes, pass `classes` as tracker_event lists each boat's `classes`; a boat in several is one tab. A Geovoile race in legs is built from the leg its link shows; add the other legs' tracks with tracker_event and tracker_import. Geovoile gives only each boat's name and sail number, so there step 3 finds almost everything. It opens a project with one tab per boat, each with the boat's track and the ORC and ORR certificates and library tracks the application could match from the tracker's own data. Its answer lists, per boat, what was found (`polars`, `tracks`) and the tracker's details.
3. **Fill the gaps from what you know.** For every boat — not only those with nothing found — search again with your own knowledge of it, in that boat's tab (`boat` from boats_list):
   - orc_search and orr_search by the boat's name, sail number (with and without the country), and its design or class name as sailors write it (\"Ker 46\", \"JPK 1180\", \"Sun Fast 3300\"). A boat may race under another name or a sistership may hold a certificate; add the boat's own certificate first, a sistership's only when the boat has none, and name it in the source's label (source_set).
   - library_search by name, sail number and design for tracks of the boat's other races, and library_import the ones that are the boat itself or, when it has few, its sisterships.
   - tracker_event + tracker_import for other races you know the boat sailed on YellowBrick, Geovoile or Blue Water Tracks.
   Do not add the same certificate or track twice: sources_list shows what a boat holds.
4. **Wind.** weather_estimate for every boat's tracks, tell the user the total size in one line, then weather_fetch boat by boat.
5. **Report** per boat what it holds now and what you could not find, in one table.

The application only matches on the tracker's data; you know the boats. Step 3 is where most of a fleet's certificates are found.

## Working through a job

Carry a request through to its end in one go: chain the calls, read each answer, and decide the next step yourself. Use the existing authorization for routine edits, configured-source downloads and exports. Ask only for irreversible loss, a material expansion of scope, or a choice the request leaves unresolved. Tell the user what you did at the end, not at each step.

## Conventions

- Speeds are knots, angles degrees. TWA is 0–180 in a symmetric polar and 0–360 (starboard 0–180, port 180–360) in an asymmetric one. Wind and waves are given by the direction they come FROM, current by the direction it flows TOWARD. Time is UTC epoch seconds. Longitude is −180 to 180.
- The user's display units (Settings) are not applied here: these tools speak knots and metres whatever the interface shows.
- Paths are absolute or begin with ~/.
- Ids come from the tools: boats from boats_list, sources from sources_list.
- A structured parameter (filters, settings, an edit) is a JSON object; the tool's description gives its shape. Where it holds only what to change, a key that is not one of its fields is refused, with the fields listed.
- A refusal comes back as a tool result with the reason in words. Read it; do not retry the same call.
- undo and redo step through the edits, yours and the user's alike.
- app_read and app_edit run classified commands from assistant_capabilities. invoke is the separately approved compatibility fallback.\n- weather_fetch returns a durable job_id immediately. Poll weather_jobs or assistant_jobs. Reuse request_id for identical retries; a disconnect does not cancel work. weather_cancel explicitly stops the requested tracks.\n- Export WhirlwindMatrixPolar for Hindsight Router, then pass the returned file path to its polar_import tool. Hindsight Sync and VectorEffects supply weather archives and GRIB/Zarr fields.";

#[tool_router(router = tool_router_guide, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "Read the shared MCP contract, tool effects, connection controls and workflow behavior."
    )]
    async fn assistant_capabilities(&self) -> ToolResult {
        let value = serde_json::json!({"contract_version":1,"app":"PolarExplorer","tools":super::effects::TABLE,"commands":crate::mcp::invoke::COMMAND_EFFECTS,"connection_controls":"app_settings_only","policy":"routine edits, downloads and exports run automatically; project_discard and invoke require approval","jobs":"Poll weather_jobs by durable job_id; weather_cancel is explicit; disconnect does not cancel"});
        json(&value)
    }
    #[tool(
        description = "START HERE. The manual for PolarExplorer, the sailing-polar application open on this computer: today's date, when to use it, the order of work (project, sources, weather, shaping, reading, export), and the conventions every other tool follows. Read it once before the first other call."
    )]
    async fn polarexplorer_guide(&self) -> ToolResult {
        let text = self
            .run("polarexplorer_guide", |_| Ok(guide(now())))
            .await?;
        json(&serde_json::json!({ "guide": text }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claude Code shows no more than this of a server's instructions.
    #[test]
    fn the_instructions_fit_what_a_client_shows() {
        let text = instructions(now());
        let units = text.encode_utf16().count();
        assert!(
            units <= INSTRUCTIONS_LIMIT,
            "{units} UTF-16 units, over {INSTRUCTIONS_LIMIT}"
        );
        assert!(text.starts_with("Today is "));
    }
}
