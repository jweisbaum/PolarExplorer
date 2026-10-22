/**
 * The typed command surface.
 *
 * Types under `./generated` are produced from the Rust definitions by
 * `npm run bindings` and must never be hand-edited. This module is the only
 * place `invoke` is called, so every IPC failure is normalised into one type.
 */
import type { LibrarySettings } from "./generated/LibrarySettings";
import type { ScrapeProgress } from "./generated/ScrapeProgress";
import type { MetadataProgress } from "./generated/MetadataProgress";
import type { BoatTrackSearch } from "./generated/BoatTrackSearch";

import { invoke } from "@tauri-apps/api/core";

import { beginBusy } from "./busy";
import { msg } from "./i18n/msg";
import { unpackCompare, type ComparePacket } from "./compare/comparePacket";
import { unpackTracks, type TrackPacket } from "./map/trackPacket";
import { unpackDots, type DotPacket } from "./panels/dotPacket";
import { unpackScene, unpackSplit, type ScenePacket, type SplitPacket } from "./polar/scenePacket";
import type { WaveSplit } from "./generated/WaveSplit";
import type { AppErrorPayload } from "./generated/AppErrorPayload";
import type { AppInfo } from "./generated/AppInfo";
import type { AppSettings } from "./generated/AppSettings";
import type { AutosaveMode } from "./generated/AutosaveMode";
import type { BlendSettingsInput } from "./generated/BlendSettingsInput";
import type { BoatInput } from "./generated/BoatInput";
import type { CompareOperand } from "./generated/CompareOperand";
import type { CsvMappingInput } from "./generated/CsvMappingInput";
import type { EditOp } from "./generated/EditOp";
import type { EditSurface } from "./generated/EditSurface";
import type { EnvJobsStatus } from "./generated/EnvJobsStatus";
import type { MatrixExportOptions } from "./generated/MatrixExportOptions";
import type { ExportAxes } from "./generated/ExportAxes";
import type { ExportPreview } from "./generated/ExportPreview";
import type { ExportResult } from "./generated/ExportResult";
import type { LegacyCacheNotice } from "./generated/LegacyCacheNotice";
import type { MapProjection } from "./generated/MapProjection";
import type { OrrCatalogueInfo } from "./generated/OrrCatalogueInfo";
import type { OrrSearchResult } from "./generated/OrrSearchResult";
import type { OrrProgress } from "./generated/OrrProgress";
import type { OrcCatalogueInfo } from "./generated/OrcCatalogueInfo";
import type { OrcFilters } from "./generated/OrcFilters";
import type { OrcSearchResult } from "./generated/OrcSearchResult";
import type { PolarCell } from "./generated/PolarCell";
import type { PolarImportResult } from "./generated/PolarImportResult";
import type { PolarNodeRef } from "./generated/PolarNodeRef";
import type { PolarPdfOptions } from "./generated/PolarPdfOptions";
import type { PolarPdfResult } from "./generated/PolarPdfResult";
import type { PolarPlotResult } from "./generated/PolarPlotResult";
import type { Units } from "./generated/Units";
import type { ProjectSummary } from "./generated/ProjectSummary";
import type { RecentProject } from "./generated/RecentProject";
import type { RecoveredProject } from "./generated/RecoveredProject";
import type { SampleDetails } from "./generated/SampleDetails";
import type { TrackFileInspection } from "./generated/TrackFileInspection";
import type { TrackFileRequest } from "./generated/TrackFileRequest";
import type { TrackFilters } from "./generated/TrackFilters";
import type { TrackImportResult } from "./generated/TrackImportResult";
import type { TrackerEventView } from "./generated/TrackerEventView";

/** An error raised by a Rust command, carrying its machine-readable kind. */
export class IpcError extends Error {
  readonly kind: string;

  constructor(payload: AppErrorPayload) {
    super(payload.message);
    this.name = "IpcError";
    this.kind = payload.kind;
  }
}

export function isErrorPayload(value: unknown): value is AppErrorPayload {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as AppErrorPayload).kind === "string" &&
    typeof (value as AppErrorPayload).message === "string"
  );
}

/**
 * The commands that can take long enough to want the status bar's spinner,
 * with what it says while they run. Callers do nothing; the name is enough.
 */
const LONG_RUNNING: Readonly<Record<string, string>> = {
  open_project: msg("Opening project"),
  open_recovered: msg("Recovering project"),
  save_project: msg("Saving"),
  save_project_as: msg("Saving"),
  import_polar_files: msg("Importing polar files"),
  inspect_track_files: msg("Reading track files"),
  inspect_csv_track: msg("Reading track files"),
  import_track_files: msg("Importing tracks"),
  import_tracker_boats: msg("Importing tracks"),
  export_polar: msg("Exporting the polar"),
  orc_catalogue_info: msg("Loading the ORC catalogue"),
  reanalysis_scan: msg("Looking for Hindsight Sync"),
};

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const label = LONG_RUNNING[command];
  const done = label === undefined ? null : beginBusy(label);
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    if (isErrorPayload(raw)) throw new IpcError(raw);
    // A command that panicked, or a Tauri-level failure, arrives as a bare string.
    throw new IpcError({ kind: "unknown", message: String(raw) });
  } finally {
    done?.();
  }
}

/** The local ORR catalogue changed after a refresh. */
export const ORR_UPDATED = "orr://updated";
/** The ORC catalogue changed: a scrape finished (spec.md 5.4). */
export const ORC_UPDATED = "orc://updated";
/** Quitting waits for a scheduled catalogue scrape; quitting again stops it. */
export const QUIT_WAITING = "app://quit-waiting";

/** Every Rust command reachable from the frontend. */
export function boatApi(boatContext?: number) {
 const call = <T>(command: string, args?: Record<string, unknown>) => invokeCommand<T>(command,
   boatContext === undefined ? args : { ...args, boatContext });
 return {
  openTrackerProject: (tracker: string, url: string, discardUnsaved = false, matchMode: import("./generated/BoatMatchMode").BoatMatchMode = "identical_model", classes: string[] = []) => call<import("./generated/TrackerProjectResult").TrackerProjectResult>("open_tracker_project", { tracker, url, discardUnsaved, matchMode, classes }),
  boatImportStatus: () => call<import("./generated/BoatImportProgress").BoatImportProgress>("boat_import_status"),
  cancelBoatImport: () => call<void>("cancel_boat_import"),
  confirmTrackerProject: (projectId: number) => call<ProjectSummary>("confirm_tracker_project", { projectId }),
  discardTrackerProject: (projectId: number) => call<void>("discard_tracker_project", { projectId }),
  boatTabs: () => call<import("./generated/BoatTabs").BoatTabs>("boat_tabs"),
  addBoat: (name: string) => call<ProjectSummary>("add_boat", { name }),
  deleteBoat: (projectId: number, boatId: number) => call<ProjectSummary>("delete_boat", { projectId, boatId }),
  restoreBoat: (projectId: number) => call<ProjectSummary>("restore_boat", { projectId }),
  renameBoat: (name: string) => call<ProjectSummary>("rename_boat", { name }),
  exportAllPolars: (directory: string, format: string, matrix: MatrixExportOptions | null = null) => call<import("./generated/BoatExportResult").BoatExportResult>("export_all_polars", { directory, format, ...(matrix ? { matrix } : {}) }),
  exportLockedProject: (path: string) => call<string>("export_locked_boat_project", { path }),
  exportTab: (path: string) => call<string>("export_boat_tab", { path }),
  /** The ORC scrape's progress; fetches nothing. */
  orcScrapeStatus: () => call<import("./generated/OrcProgress").OrcProgress>("orc_scrape_status"),
  /** Downloads every country's valid ORC certificates from data.orc.org. */
  startOrcScrape: () => call<import("./generated/OrcProgress").OrcProgress>("start_orc_scrape"),
  cancelOrcScrape: () => call<void>("cancel_orc_scrape"),
  /** When a catalogue is scraped by itself: never, on startup or on shutdown. */
  setCatalogueSchedule: (catalogue: "orc" | "orr", schedule: import("./generated/ScrapeSchedule").ScrapeSchedule) =>
    call<AppSettings>("set_catalogue_schedule", { catalogue, schedule }),
  orrCatalogueInfo: () => call<OrrCatalogueInfo>("orr_catalogue_info"),
  orrSearch: (query: string, filters: OrcFilters, limit = 50, offset = 0) => call<OrrSearchResult>("orr_search", { query, filters, limit, offset }),
  orrAdd: (id: string) => call<ProjectSummary>("orr_add", { id }),
  orrScrapeStatus: () => call<OrrProgress>("orr_scrape_status"),
  startOrrScrape: (year: number) => call<OrrProgress>("start_orr_scrape", { year }),
  cancelOrrScrape: () => call<void>("cancel_orr_scrape"),
  /** Product name and version, for the start screen and About panel. */
  appInfo: () => call<AppInfo>("app_info"),

  // Project lifecycle (spec.md 3.3, 4.2–4.5). Every call that drops the open
  // project takes the user's answer to "Save / Don't save / Cancel" as
  // `discardUnsaved`; without it, Rust refuses a dirty project with kind
  // "unsaved-changes".

  /** Creates a project and makes it the open one. */
  newProject: (name: string, boat: BoatInput | null = null, discardUnsaved = false) =>
    call<ProjectSummary>("new_project", { name, boat, discardUnsaved }),
  /** Opens a `.wpsproj`. A newer file fails with kind "schema-too-new". */
  openProject: (path: string, discardUnsaved = false) =>
    call<ProjectSummary>("open_project", { path, discardUnsaved }),
  /** Imports a portable `.petab` as a locked tab in the open project. */
  importTab: (path: string) => call<ProjectSummary>("import_boat_tab", { path }),
  /** Saves to the project's file; kind "never-saved" means ask for Save As. */
  saveProject: () => call<ProjectSummary>("save_project"),
  /** Saves to a new file, adding `.wpsproj` if missing. */
  saveProjectAs: (path: string) => call<ProjectSummary>("save_project_as", { path }),
  /** Closes the open project. */
  closeProject: (discardUnsaved = false) => call<void>("close_project", { discardUnsaved }),
  /** The open project, or null on the start screen. */
  projectSummary: () => call<ProjectSummary | null>("project_summary"),
  /** The ten most recent projects, newest first, with missing files marked. */
  recentProjects: () => call<RecentProject[]>("recent_projects"),
  /** Drops one entry from the recent list. */
  forgetRecentProject: (path: string) =>
    call<RecentProject[]>("forget_recent_project", { path }),
  /** Empties the recent list. */
  clearRecent: () => call<RecentProject[]>("clear_recent"),

  // History (spec.md 4.6).

  /** Reverses the last change. */
  undo: () => call<ProjectSummary>("undo"),
  /** Reapplies the last undone change. */
  redo: () => call<ProjectSummary>("redo"),
  /** Renames the open project (undoable). */
  renameProject: (name: string) => call<ProjectSummary>("rename_project", { name }),

  // Crash recovery (spec.md 4.5).

  /** Autosaves left by a crash, newest first. */
  recoveredProjects: () => call<RecoveredProject[]>("recovered_projects"),
  /** Opens a recovered autosave as its original project, dirty. */
  openRecovered: (id: number, discardUnsaved = false) =>
    call<ProjectSummary>("open_recovered", { id, discardUnsaved }),
  /** Deletes a recovered autosave. */
  discardRecovered: (id: number) => call<RecoveredProject[]>("discard_recovered", { id }),

  // The source list (spec.md 8). Each is one undoable change.

  /** Sets a source's colour, `#rrggbb`. */
  setSourceColour: (id: number, colour: string) =>
    call<ProjectSummary>("set_source_colour", { id, colour }),
  /** Shows or hides a source; hidden sources leave the blend and every plot. */
  setSourceVisible: (id: number, visible: boolean) =>
    call<ProjectSummary>("set_source_visible", { id, visible }),
  /**
   * Sets a source's blend weight (0–1). Calls sharing a `gesture` name, one
   * after another, are one undo entry: a slider drag.
   */
  setSourceWeight: (id: number, weight: number, gesture: string | null = null) =>
    call<ProjectSummary>("set_source_weight", { id, weight, gesture }),
  /** Renames a source. */
  setSourceLabel: (id: number, label: string) =>
    call<ProjectSummary>("set_source_label", { id, label }),
  /** Moves a source to position `to` (display order only). */
  moveSource: (id: number, to: number) => call<ProjectSummary>("move_source", { id, to }),
  /** Removes a source; undo puts it back. */
  removeSource: (id: number) => call<ProjectSummary>("remove_source", { id }),

  // The blend (spec.md 8, 12). Each change is one undoable entry.

  /** Shows or hides the blend in every plot; export is unchanged. */
  setBlendVisible: (visible: boolean) => call<ProjectSummary>("set_blend_visible", { visible }),
  /** Sets the blend's colour, `#rrggbb`. */
  setBlendColour: (colour: string) => call<ProjectSummary>("set_blend_colour", { colour }),
  /** Applies the Blend settings dialog: the output grid and the settings, as one entry. */
  setBlendSettings: (settings: BlendSettingsInput) =>
    call<ProjectSummary>("set_blend_settings", { settings }),
  /**
   * What an export would write (`expedition`, `adrena` or `csv`), on the
   * project's grid or `axes`: the grid, the text, or why it is refused.
   */
  /**
   * One cell of the blend, by its indices on the output grid, with each
   * source behind its value: what hovering the blend shows. Read-only.
   */
  blendCell: (twaIndex: number, twsIndex: number) =>
    call<import("./generated/BlendCell").BlendCell>("blend_cell", { twaIndex, twsIndex }),
  /**
   * The same cell of one copy's blend in a split view (spec.md 10.5): each
   * track counted with only that direction's samples. Read-only.
   */
  blendCellSplit: (twaIndex: number, twsIndex: number, split: WaveSplit, cell: number) =>
    call<import("./generated/BlendCell").BlendCell>("blend_cell_split", { twaIndex, twsIndex, split, cell }),
  exportPreview: (format: string, axes: ExportAxes | null = null, matrix: MatrixExportOptions | null = null) =>
    call<ExportPreview>("export_preview", { format, axes, ...(matrix ? { matrix } : {}) }),
  /** Writes the blend to `path`, recomputed from the sources. */
  exportPolar: (path: string, format: string, axes: ExportAxes | null = null, matrix: MatrixExportOptions | null = null) =>
    call<ExportResult>("export_polar", { path, format, axes, ...(matrix ? { matrix } : {}) }),

  // Polar files (spec.md 6).

  /**
   * Parses and imports polar files, one source per file, as one undoable
   * change. A file that fails is listed with its line, column and a reason
   * code; the others still import.
   */
  importPolarFiles: (paths: string[]) => call<PolarImportResult>("import_polar_files", { paths }),

  // The 2D polar plot (spec.md 9.2).

  /** Shared weighted Gaussian fields, using the current blend settings and view restrictions. */
  polarPdf: (options: PolarPdfOptions) => call<PolarPdfResult>("polar_pdf", { options }),
  /**
   * Every visible polar source's curve at `tws` (null for "all": one curve
   * per source per wind speed it has), read through its edits and
   * exclusions, the domain those sources cover, and the blend's curves
   * (none while the Blend entry is hidden).
   */
  polarPlot: (tws: number | null) => call<PolarPlotResult>("polar_plot", { tws }),
  /**
   * The samples within the dot band of `tws` (every one with wind for null;
   * filtered ones too when `showFiltered`), packed as binary (layout in
   * `panels/dotPacket.ts`).
   */
  polarPlotDots: async (tws: number | null, showFiltered = false): Promise<DotPacket> => {
    const bytes = await call<ArrayBuffer | number[]>("polar_plot_dots", { tws, showFiltered });
    return unpackDots(bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes).buffer);
  },

  // Tracks from files (spec.md 7.1, 7.3, 7.4, 7.6).

  /** Reads chosen files and says what each holds: boats, or a CSV preview and guessed mapping. */
  inspectTrackFiles: (paths: string[]) => call<TrackFileInspection[]>("inspect_track_files", { paths }),
  /** Re-reads one CSV with the user's column mapping. */
  inspectCsvTrack: (path: string, mapping: CsvMappingInput) =>
    call<TrackFileInspection>("inspect_csv_track", { path, mapping }),
  /** Imports the chosen boats of each file, one source per boat, as one undoable change. */
  importTrackFiles: (files: TrackFileRequest[]) => call<TrackImportResult>("import_track_files", { files }),

  // Tracker imports (spec.md 7.2). A download reports `TRACKER_PROGRESS`
  // events, sends the boat list ahead of the positions as a
  // `TRACKER_LISTED` event when the tracker gives it first (D24), and can
  // be cancelled; the event is kept for the session.

  /**
   * Resolves a pasted event address and downloads every boat's full track,
   * or recalls the event from this session unless `refresh`. Fails with
   * kind "tracker-address", "tracker-unavailable", "tracker-no-event",
   * "tracker-decode", "tracker-unsupported", "tracker-legacy",
   * "tracker-network" or "cancelled".
   */
  trackerEvent: (tracker: "yellowbrick" | "geovoile" | "bluewater", url: string, refresh = false, download = "", listOnly = false) =>
    call<TrackerEventView>("tracker_event", { tracker, url, refresh, download, listOnly }),
  /** Stops the running event download. */
  cancelTrackerEvent: () => call<void>("cancel_tracker_event"),
  /** Imports the chosen boats of a downloaded event, one source per boat, as one undoable change. */
  importTrackerBoats: (tracker: string, key: string, boats: string[]) =>
    call<TrackImportResult>("import_tracker_boats", { tracker, key, boats }),
  /** Changes a track's time window, boat-speed band, manoeuvre threshold and origin filters (undoable). */
  setTrackFilters: (id: number, filters: TrackFilters) =>
    call<ProjectSummary>("set_track_filters", { id, filters }),
  /** Whether a track uses only downloaded weather where it supplied wind of its own (undoable). */
  setTrackWind: (id: number, downloadedOnly: boolean): Promise<ProjectSummary> => invoke("set_track_wind", { id, downloadedOnly }),
  /** How a track derives its heading and speed (spec.md 7.4): the longest gap, and whether the given or the derived heading and speed come first. */
  setTrackDerivation: (id: number, maxGapS: number, preferHeading: "given" | "derived", preferSpeed: "given" | "derived") =>
    call<ProjectSummary>("set_track_derivation", { id, maxGapS, preferHeading, preferSpeed }),
  /** One sample's time, position, motion and environment, for the map's hover. */
  sampleDetails: (sourceId: number, sampleId: number) =>
    call<SampleDetails>("sample_details", { sourceId, sampleId }),

  // The environment of track samples (spec.md 7.5, 7.7, 13). Progress
  // arrives as `ENV_PROGRESS` events; `ENV_CHANGED` says the project changed
  // under a running fetch.

  /** Queues the named tracks' fetch: what is missing, or everything with `restart`. */
  startEnvFetch: (sourceIds: number[], restart: boolean) =>
    call<EnvJobsStatus>("start_env_fetch", { sourceIds, interval: "hourly", restart }),
  /** Cancels the named tracks' fetches, or every fetch for null; finished samples are kept. */
  cancelEnvFetch: (sourceIds: number[] | null = null) =>
    call<EnvJobsStatus>("cancel_env_fetch", { sourceIds }),
  /** The fetch queue now. */
  envJobs: () => call<EnvJobsStatus>("env_jobs"),
  /** Feeds the polar from current-corrected values, or ground values (undoable). */
  setUseCorrected: (on: boolean) => call<ProjectSummary>("set_use_corrected", { on }),
  /** Includes Stokes drift in the global merged current from the next fetch (undoable). */

  /** Ordered priority groups and the additional global point filter layer. */
  setPriorityFilters: (groups: TrackFilters[], minimum: number) =>
    call<ProjectSummary>("set_priority_filters", { groups, minimum }),
  setWaveRanges: (ranges: import("./generated/WaveRangesInput").WaveRangesInput) => call<ProjectSummary>("set_wave_ranges", { ranges }),
  waveRangeMaxima: () => call<[number | null, number | null, number | null]>("wave_range_maxima"),
  setVisibleSources: (ids: number[]) => call<ProjectSummary>("set_visible_sources", { ids }),
  setGlobalFilters: (filters: TrackFilters | null) => call<ProjectSummary>("set_global_filters", { filters }),

  /** Every visible track for the map, packed as binary (layout in `map/trackPacket.ts`). */
  mapTracks: async (): Promise<TrackPacket> => {
    const bytes = await call<ArrayBuffer | number[]>("map_tracks");
    return unpackTracks(bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes).buffer);
  },

  // The 3D polar view (spec.md 10).

  /**
   * The 3D scene, packed as binary (layout in `polar/scenePacket.ts`): every
   * visible polar source's nodes and surface as edited, every sample dot,
   * and which are excluded or edited. `focus` is the source in edit mode (a
   * track's segment joins the scene); with `held`, the scene already shown,
   * only the samples' flags travel when no sample moved.
   */
  polarScene: async (focus: number | null = null, held: ScenePacket | null = null): Promise<ScenePacket> => {
    const bytes = await call<ArrayBuffer | number[]>("polar_scene", { focus, samplesKey: held?.samplesKey ?? null });
    return unpackScene(bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes).buffer, held ?? undefined);
  },
  /**
   * The blend of each copy of a split view (spec.md 10.5), packed (layout in
   * `polar/scenePacket.ts`, `unpackSplit`): one surface per copy whose
   * blend has something to say, each made from its direction's samples.
   */
  polarSceneSplit: async (split: WaveSplit): Promise<SplitPacket> => {
    const bytes = await call<ArrayBuffer | number[]>("polar_scene_split", { split });
    return unpackSplit(bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes).buffer);
  },
  /**
   * Compares two operands on the project output grid (spec.md 11), packed
   * as binary (layout in `compare/comparePacket.ts`): A, B, Δ in knots and
   * percent of B, who covers each cell, the summary and the regions where
   * each is faster by more than `thresholdKn`.
   */
  comparePolars: async (a: CompareOperand, b: CompareOperand, thresholdKn: number): Promise<ComparePacket> => {
    const bytes = await call<ArrayBuffer | number[]>("compare_polars", { a, b, thresholdKn });
    return unpackCompare(bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes).buffer);
  },
  /** One source's editable surface (spec.md 10.4): its cells as imported or binned and as edited. */
  polarEditSurface: (sourceId: number) => call<EditSurface>("polar_edit_surface", { sourceId }),
  /**
   * Edits cells of one source's surface as one undoable change; calls
   * sharing a `gesture` name (a node drag) are one undo entry.
   */
  editPolar: (sourceId: number, op: EditOp, cells: PolarCell[], gesture: string | null = null) =>
    call<ProjectSummary>("edit_polar", { sourceId, op, cells, gesture }),
  /** Chooses a track's segment statistic: `min`, `median`, `mean`, `p75`, `p90` or `max` (undoable). */
  setSegmentStatistic: (sourceId: number, statistic: string) =>
    call<ProjectSummary>("set_segment_statistic", { sourceId, statistic }),
  /**
   * Excludes a selection from the blend (`excluded` true) or includes it
   * again, as one undoable change (spec.md 10.3): polar nodes by grid place,
   * track samples by id.
   */
  setExcluded: (nodes: PolarNodeRef[], samples: number[], excluded: boolean) =>
    call<ProjectSummary>("set_excluded", { nodes, samples, excluded }),

  // ORC polars (spec.md 5). The catalogue is built into the app.

  /** The catalogue's size, provenance, countries and years; loads it on first call. */
  orcCatalogueInfo: () => call<OrcCatalogueInfo>("orc_catalogue_info"),
  /** Searches the catalogue: every word must match; best first, at most `limit`. */
  orcSearch: (query: string, filters: OrcFilters, limit: number, offset = 0) =>
    call<OrcSearchResult>("orc_search", { query, filters, limit, offset }),
  /**
   * Adds a certificate as an ORC source (undoable). One the project already
   * holds fails with kind "orc-duplicate" unless `allowDuplicate`.
   */
  orcAdd: (id: number, allowDuplicate = false) => call<ProjectSummary>("orc_add", { id, allowDuplicate }),

  // Settings (spec.md 3.4). Each returns the settings as saved.

  /** The settings. */
  setLibrarySettings: (settings: LibrarySettings) => call<AppSettings>("set_library_settings", { settings }),
  startLibraryScrape: () => call<ScrapeProgress>("start_library_scrape"),
  libraryScrapeStatus: () => call<ScrapeProgress>("library_scrape_status"),
  cancelLibraryScrape: () => call<void>("cancel_library_scrape"),
  /** Signs in read-only and checks the tables; answers the database's name. Reads no rows. */
  testDatabaseConnection: (connection: import("./generated/DatabaseConnection").DatabaseConnection) => call<string>("test_database_connection", { connection }),
  /** Reads the SYRF database's boat metadata, read-only, into `boat-metadata.json`. */
  startMetadataDownload: () => call<MetadataProgress>("start_metadata_download"),
  metadataDownloadStatus: () => call<MetadataProgress>("metadata_download_status"),
  cancelMetadataDownload: () => call<void>("cancel_metadata_download"),
  trackMetadataAvailable: () => call<boolean>("library_metadata_available"),
  searchDatabaseBoats: (query: string, offset = 0) => call<BoatTrackSearch>("search_database_boats", { query, offset }),
  importDatabaseTrack: (id: string) => call<TrackImportResult>("import_database_track", { id }),
  appSettings: () => call<AppSettings>("app_settings"),
  /** Sets the interface language; the native menu follows. */
  setLanguage: (language: string) => call<AppSettings>("set_language", { language }),
  /** Sets the theme. */
  setTheme: (theme: string) => call<AppSettings>("set_theme", { theme }),
  /** Sets the display units. */
  setUnits: (units: Units) => call<AppSettings>("set_units", { units }),
  /** Sets what autosave does. */
  setAutosaveMode: (mode: AutosaveMode) => call<AppSettings>("set_autosave_mode", { mode }),
  /** Sets how much downloaded weather is kept in memory for the session, megabytes. */
  /** Sets the reanalysis fetcher's concurrency and timeout. */
  /** Opens Hindsight Sync's product page in the system browser. */
  openHindsightSyncSite: () => call<void>("open_hindsight_sync_site"),
  /** Looks for Hindsight Sync: the saved port, 8765, then its own settings file's port. */
  reanalysisScan: () => call<import("./generated/ScanResult").ScanResult>("reanalysis_scan"),
  /** Asks Hindsight Sync on the saved port only for its archives. */
  reanalysisProbe: () => call<import("./generated/ScanResult").ScanResult>("reanalysis_probe"),
  /** Sets the port Hindsight Sync's archive API is looked for on; Rust probes it. */
  setReanalysisPort: (port: number) => call<AppSettings>("set_reanalysis_port", { port }),
  /** Chooses the archive fetches read, or none. */
  setReanalysisArchive: (id: string | null) => call<AppSettings>("set_reanalysis_archive", { id }),
  /** Sets the map projection. */
  setProjection: (projection: MapProjection) => call<AppSettings>("set_projection", { projection }),
  /** Sets how far from the 2D plot's wind speed a sample dot may be, knots either side. */
  setPlotBand: (bandKn: number) => call<AppSettings>("set_plot_band", { bandKn }),
  /**
   * The on-disk chunk cache an earlier version kept, the first time it is
   * asked in a session: what it holds; null when there is none (D27).
   */
  legacyCacheNotice: () => call<LegacyCacheNotice | null>("legacy_cache_notice"),
  /** Removes, in the background, the old chunk cache `legacyCacheNotice` announced. */
  removeOldChunkCache: () => call<null>("remove_old_chunk_cache"),

  // The map (spec.md 9.1).

  /** The bundled basemap asset, as raw bytes. */
  basemap: async (): Promise<ArrayBuffer> => {
    const bytes = await call<ArrayBuffer | number[]>("basemap");
    // Raw responses arrive as an ArrayBuffer; older shapes as a number array.
    return bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes).buffer;
  },

  // Quitting (spec.md 3.3).

  /**
   * Quits, after the unsaved-changes guard. Rust refuses with
   * "unsaved-changes" unless the project is clean or `discardUnsaved`.
   */
  quitApp: (discardUnsaved: boolean) => call<void>("quit_app", { discardUnsaved }),
  // ----- The MCP service (spec.md 3.7) -----
  /** The service's setting and live state: whether it listens, where, its token, who is connected. */
  mcpConnections: () => call<import("./generated/McpConnection").McpConnection[]>("mcp_connections"),
  mcpConnectionAction: (client: import("./generated/McpClient").McpClient, action: string) => call<void>("mcp_connection_action", {client, action}),
  mcpStatus: () => call<import("./generated/McpStatus").McpStatus>("mcp_status"),
  /** Turns the service on or off and sets its port. On issues a fresh token; off clears it. */
  setMcp: (enabled: boolean, port: number) => call<import("./generated/McpStatus").McpStatus>("mcp_set", { enabled, port }),
  /** Issues a new token; clients added before must be added again. */
  rotateMcpToken: () => call<import("./generated/McpStatus").McpStatus>("mcp_rotate_token"),
  /** Adds the service to a client's own configuration, or opens the Claude Desktop extension. */
  registerMcpClient: (client: import("./generated/McpClient").McpClient, policy?: string) =>
    call<import("./generated/McpRegistered").McpRegistered>("mcp_register_client", { client, policy }),
  /** Answers the service's `view://capture` with the stage's picture, base64 PNG. */
  deliverCapture: (id: number, pngBase64: string) => call<void>("deliver_capture", { id, pngBase64 }),
  /** Answers `view://capture` with why there is no picture. */
  refuseCapture: (id: number, reason: string) => call<void>("refuse_capture", { id, reason }),
};

}
export const api = boatApi();

/** The event carrying the environment fetch queue as it changes. */
/** A track library scrape's progress (`library::scrape::PROGRESS`). */
export const LIBRARY_SCRAPE = "library://scrape";
/** The SYRF database metadata download's progress (`library::database::PROGRESS`). */
export const LIBRARY_METADATA = "library://metadata";
export const ENV_PROGRESS = "env://progress";
/** The event saying a fetch wrote into the open project. */
export const ENV_CHANGED = "env://changed";
/** A probe found Hindsight Sync: the settings it saved changed, and the payload is its `ScanResult`. */
export const REANALYSIS_CHANGED = "reanalysis://changed";

/** The event carrying a tracker download's progress. */
export const TRACKER_PROGRESS = "tracker://progress";
/** The event carrying a downloading event's boat list, before its positions. */
export const TRACKER_LISTED = "tracker://listed";

/** The event Rust sends when the user asks to quit or close the window. */
export const QUIT_REQUESTED = "app://quit-requested";
