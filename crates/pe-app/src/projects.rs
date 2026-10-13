//! Project lifecycle: New, Open, Save, Save As, Close, and the recent list
//! (spec.md 3.3, 4.2–4.4). Ported from VectorEffects' `projects.rs`.
//!
//! The frontend never sees `pe-core` types. What crosses IPC is a flat
//! summary, which keeps `ts-rs` out of the core crate and the wire format
//! readable. Each Tauri command has a plain twin taking `&AppState`, so the
//! whole lifecycle is tested without a webview.

use std::path::PathBuf;

use pe_core::polar::PolarFileFormat;
use pe_core::{Boat, CoreError, Project, SourceKind, io};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Context, Result};
use crate::session::{OpenProject, Session};

/// One source as the source list shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "SourceSummary.ts")]
pub struct SourceSummary {
    /// Stable id.
    pub id: u64,
    /// `"orc"`, `"polar_file"` or `"track"`.
    pub kind: String,
    /// Label.
    pub label: String,
    /// `#rrggbb`.
    pub colour: String,
    /// Whether it is shown and blended.
    pub visible: bool,
    /// Blend weight, 0–1.
    pub weight: f64,
    /// What the source holds (spec.md 8): cells with a value for a polar,
    /// samples for a track.
    pub count: u32,
    /// For a track, the samples the blend uses: neither excluded by hand
    /// nor taken out by the filters; null for a polar.
    pub used: Option<u32>,
    /// For an imported polar file, how it was read (spec.md 6).
    pub polar_file: Option<PolarFileSummary>,
    /// For an ORC polar, which certificate it is (spec.md 5.3).
    pub orc: Option<OrcSourceSummary>,
    /// For an ORR polar, the certificate metadata.
    pub orr: Option<OrcSourceSummary>,
    /// For a track, what the Tracks section lists (spec.md 7.1).
    pub track: Option<crate::tracks::TrackSummary>,
    /// Polar edits the source holds (spec.md 10.4).
    pub edits: u32,
}

/// An ORC source's certificate, as the ORC polars section lists it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "OrcSourceSummary.ts")]
pub struct OrcSourceSummary {
    /// Sail number as shown; empty when there is none.
    pub sail_no: String,
    /// Type or model.
    pub model: Option<String>,
    /// Year built.
    pub year: Option<i32>,
    /// Year of the certificate, when known.
    pub certificate_year: Option<i32>,
}

/// An imported polar file's format and axes, as the Polar files section
/// lists them (spec.md 6).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "PolarFileSummary.ts")]
pub struct PolarFileSummary {
    /// `"expedition"`, `"adrena"` or `"csv"`.
    pub format: String,
    /// The file it came from.
    pub file_name: String,
    /// Its TWA axis, degrees.
    pub twa: Vec<f64>,
    /// Its TWS axis, knots.
    pub tws: Vec<f64>,
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl SourceSummary {
    /// The summary of one source, from what is derived from it (its grid,
    /// a track's samples placed and filtered), so a summary after an edit
    /// reads the cache rather than every sample again.
    pub fn of(source: &pe_core::Source, derived: &crate::derived::Derived) -> Self {
        let orc = match &source.kind {
            SourceKind::Orc { record } => Some(OrcSourceSummary {
                sail_no: record.sail_no.clone(),
                model: record.model.clone(),
                year: record.year,
                certificate_year: record.certificate_year,
            }),
            _ => None,
        };
        let orr = match &source.kind {
            SourceKind::Orr { record } => Some(OrcSourceSummary {
                sail_no: record.sail_no.clone(),
                model: record.model.clone(),
                year: record.build_year(),
                certificate_year: Some(record.year),
            }),
            _ => None,
        };
        let (cells, polar_file) = match &source.kind {
            // The cells of the polar it gives: the table plus the beat and
            // run points (spec.md 5.3).
            SourceKind::Orc { .. } | SourceKind::Orr { .. } => {
                (pe_polar::cell_count(&derived.base), None)
            }
            SourceKind::PolarFile {
                format,
                file_name,
                polar,
                ..
            } => (
                pe_polar::cell_count(polar),
                Some(PolarFileSummary {
                    format: format_name(*format).to_owned(),
                    file_name: file_name.clone(),
                    twa: polar.twa.clone(),
                    tws: polar.tws.clone(),
                }),
            ),
            // The track summary below counts what the blend uses.
            SourceKind::Track { track } => (track.samples.len(), None),
        };
        let track = source
            .track()
            .zip(derived.track.as_ref())
            .map(|(track, placed)| crate::tracks::TrackSummary::of(source, track, placed));
        let used = track.as_ref().map(|t| t.used);
        Self {
            id: source.id.raw(),
            kind: source.kind.name().to_owned(),
            label: source.label.clone(),
            colour: source.colour.to_string(),
            visible: source.visible,
            weight: source.weight,
            count: count(cells),
            used,
            polar_file,
            orc,
            orr,
            track,
            edits: count(source.overlay.cell_overrides.len()),
        }
    }
}

/// The wire name of a polar file format.
fn format_name(format: PolarFileFormat) -> &'static str {
    match format {
        PolarFileFormat::Expedition => "expedition",
        PolarFileFormat::Adrena => "adrena",
        PolarFileFormat::Csv => "csv",
        PolarFileFormat::QtVlm => "qtvlm",
        PolarFileFormat::OpenCpn => "opencpn",
        PolarFileFormat::LuckGrib => "luckgrib",
        PolarFileFormat::WhirlwindMatrixPolar => "whirlwind_matrix_polar",
    }
}

/// What the frontend needs to know about the open project.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "ProjectSummary.ts")]
pub struct ProjectSummary {
    /// The project's identity; names its recovery snapshot.
    pub id: u64,
    /// Display name.
    pub name: String,
    /// Where it lives on disk; null until first saved.
    pub path: Option<String>,
    /// Whether there are changes not yet written to disk.
    pub dirty: bool,
    /// Bumped on every document change.
    pub revision: u64,
    /// Boat name.
    pub boat_name: String,
    /// Boat notes.
    pub boat_notes: String,
    /// Every source, in list order.
    pub sources: Vec<SourceSummary>,
    /// Whether there is anything to undo.
    pub can_undo: bool,
    /// Whether there is anything to redo.
    pub can_redo: bool,
    /// The history label undo would reverse: an English key to translate.
    pub undo_label: Option<String>,
    /// The history label redo would reapply.
    pub redo_label: Option<String>,
    /// Whether the polar uses current-corrected values (spec.md 7.5, D13).
    pub use_corrected: bool,
    /// Whether the global merged current includes Stokes drift.
    pub stokes_drift: bool,
    /// The Blend entry: colour, visibility, coverage and settings
    /// (spec.md 8, 12).
    pub blend: crate::blend::BlendSummary,
}

impl ProjectSummary {
    /// The summary of an open project.
    pub fn of(open: &mut OpenProject) -> Self {
        let sources = open
            .project
            .sources
            .iter()
            .map(|source| {
                let derived = open.derived.get(&open.project, source);
                SourceSummary::of(source, &derived)
            })
            .collect();
        let blend = open.derived.blend(&open.project);
        let project = &open.project;
        Self {
            id: project.id.raw(),
            name: project.name.clone(),
            path: open.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            dirty: open.dirty,
            revision: open.revision,
            boat_name: project.boat.name.clone(),
            boat_notes: project.boat.notes.clone(),
            sources,
            can_undo: open.history.can_undo(),
            can_redo: open.history.can_redo(),
            undo_label: open.history.undo_label().map(str::to_owned),
            redo_label: open.history.redo_label().map(str::to_owned),
            use_corrected: project.blend.use_corrected,
            stokes_drift: project.blend.include_stokes_drift,
            blend: crate::blend::BlendSummary::of(project, &blend),
        }
    }
}

/// A previously opened project (spec.md 3.1, 4.4).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "RecentProject.ts")]
pub struct RecentProject {
    /// Full path.
    pub path: String,
    /// File stem, for display.
    pub name: String,
    /// Whether the file is still there. A missing one is shown greyed with
    /// "Not found".
    pub exists: bool,
}

/// The optional boat fields of the new-project form (spec.md 4.2).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, TS)]
#[ts(export_to = "BoatInput.ts")]
pub struct BoatInput {
    /// Boat name.
    #[serde(default)]
    pub name: String,
    /// Notes.
    #[serde(default)]
    pub notes: String,
}

fn now_unix_s() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Drops the recovery snapshot of whatever project is about to be replaced
/// or closed: what the user chose to put down is not offered back.
pub(crate) fn forget_open(state: &AppState, session: &Session) {
    if let Some(open) = &session.open {
        crate::autosave::forget(state, open.project.id.raw());
    }
}

/// Persists the settings after a recent-list change. A failure must not fail
/// the open or save that caused it.
fn persist_recent(state: &AppState, session: &Session) {
    let _ = session.settings.save(&state.paths.settings_file());
}

/// Creates a project and makes it the open one.
#[tauri::command]
pub fn new_project(
    state: tauri::State<'_, AppState>,
    name: String,
    boat: Option<BoatInput>,
    discard_unsaved: bool,
) -> Result<ProjectSummary> {
    create(&state, name, boat, discard_unsaved)
}

/// [`new_project`] without a Tauri handle.
pub fn create(
    state: &AppState,
    name: String,
    boat: Option<BoatInput>,
    discard_unsaved: bool,
) -> Result<ProjectSummary> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::BadOption {
            field: "Project name",
            value: name,
        });
    }
    let boat = boat.unwrap_or_default();
    state.with_session(|session| {
        session.refuse_to_discard(discard_unsaved)?;
        forget_open(state, session);
        let project = Project::new(
            name,
            Boat {
                name: boat.name.trim().to_owned(),
                notes: boat.notes,
                ..Boat::default()
            },
            now_unix_s(),
        );
        session.replace(OpenProject::created(project));
        // Jobs belong to the project they were started for (spec.md 3.3).
        state.env_jobs.cancel(None);
        Ok(ProjectSummary::of(session.require_open()?))
    })
}

/// Opens a project from disk.
#[tauri::command(async)]
pub fn open_project(
    state: tauri::State<'_, AppState>,
    path: String,
    discard_unsaved: bool,
) -> Result<ProjectSummary> {
    open(&state, path, discard_unsaved)
}

/// Reads a project file, keeping a newer-schema refusal as its own kind so
/// the frontend can say "update PolarExplorer" rather than "damaged file".
pub(crate) fn read_project(path: &std::path::Path, doing: &'static str) -> Result<Project> {
    match io::load(path) {
        Ok(project) => Ok(project),
        Err(err @ CoreError::SchemaTooNew { .. }) => Err(AppError::Core(err)),
        Err(err) => Err(err).doing(doing, path.display()),
    }
}

/// [`open_project`] without a Tauri handle.
pub fn open(state: &AppState, path: String, discard_unsaved: bool) -> Result<ProjectSummary> {
    let path = PathBuf::from(path);
    // Asked before reading as well as after: a refusal that was always
    // coming should not cost a large read first. Nothing is replaced until
    // the second asking, under the same lock as the replacing.
    state.with_session(|session| session.refuse_to_discard(discard_unsaved))?;
    let project = read_project(&path, "open the project at")?;
    state.with_session(|session| {
        session.refuse_to_discard(discard_unsaved)?;
        forget_open(state, session);
        session.replace(OpenProject::loaded(project, path.clone()));
        // Jobs belong to the project they were started for (spec.md 3.3).
        state.env_jobs.cancel(None);
        session.settings.remember(&path);
        persist_recent(state, session);
        Ok(ProjectSummary::of(session.require_open()?))
    })
}

/// Saves the open project to its file.
#[tauri::command(async)]
pub fn save_project(state: tauri::State<'_, AppState>) -> Result<ProjectSummary> {
    save(&state)
}

/// [`save_project`] without a Tauri handle.
pub fn save(state: &AppState) -> Result<ProjectSummary> {
    state.with_session(|session| save_locked(state, session))
}

/// [`save`] for a caller that already holds the session lock, so it can
/// decide under the same lock whether the save should happen at all
/// (autosave in Save mode, which must not write after "Don't save").
pub(crate) fn save_locked(state: &AppState, session: &mut Session) -> Result<ProjectSummary> {
    session.require_open()?.ensure_editable()?;
    let path = session
        .require_open()?
        .path
        .clone()
        .ok_or(AppError::ProjectNeverSaved)?;
    session.save_to(path)?;
    persist_recent(state, session);
    let open = session.require_open()?;
    // Saved cleanly: there is nothing to recover (spec.md 4.5).
    crate::autosave::forget(state, open.project.id.raw());
    Ok(ProjectSummary::of(open))
}

/// Saves the open project to a new file.
#[tauri::command(async)]
pub fn save_project_as(state: tauri::State<'_, AppState>, path: String) -> Result<ProjectSummary> {
    save_as(&state, path)
}

/// [`save_project_as`] without a Tauri handle.
pub fn save_as(state: &AppState, path: String) -> Result<ProjectSummary> {
    // Add the extension if the user did not, so the project is found again
    // by the same filter that saved it.
    let path = with_extension(PathBuf::from(path));
    state.with_session(|session| {
        session.require_open()?.ensure_editable()?;
        session.save_to(path)?;
        persist_recent(state, session);
        let open = session.require_open()?;
        crate::autosave::forget(state, open.project.id.raw());
        Ok(ProjectSummary::of(open))
    })
}

/// Appends `.wpsproj` unless the name already ends in exactly that, so
/// "Race.v2" becomes "Race.v2.wpsproj" rather than "Race.wpsproj".
pub fn with_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some_and(|ext| ext == io::EXTENSION) {
        return path;
    }
    let mut name = path.into_os_string();
    name.push(".");
    name.push(io::EXTENSION);
    PathBuf::from(name)
}

/// Closes the open project.
#[tauri::command]
pub fn close_project(state: tauri::State<'_, AppState>, discard_unsaved: bool) -> Result<()> {
    close(&state, discard_unsaved)
}

/// [`close_project`] without a Tauri handle. Guarded like every other path
/// that drops the open project (spec.md 3.3).
pub fn close(state: &AppState, discard_unsaved: bool) -> Result<()> {
    state.with_session(|session| {
        session.refuse_to_discard(discard_unsaved)?;
        forget_open(state, session);
        session.open = None;
        session.boats.clear();
        session.removed_boats.clear();
        // Jobs belong to the project they were started for (spec.md 3.3).
        state.env_jobs.cancel(None);
        Ok(())
    })
}

/// The open project, or null on the start screen.
#[tauri::command]
pub fn project_summary(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
) -> Result<Option<ProjectSummary>> {
    let state = state.scoped(boat_context);
    summary(&state)
}

/// [`project_summary`] without a Tauri handle.
pub fn summary(state: &AppState) -> Result<Option<ProjectSummary>> {
    state.with_session(|session| Ok(session.open.as_mut().map(ProjectSummary::of)))
}

/// The recent list, newest first, with missing files marked.
#[tauri::command]
pub fn recent_projects(state: tauri::State<'_, AppState>) -> Result<Vec<RecentProject>> {
    recent(&state)
}

/// [`recent_projects`] without a Tauri handle.
pub fn recent(state: &AppState) -> Result<Vec<RecentProject>> {
    state.with_session(|session| {
        Ok(session
            .settings
            .recent_projects
            .iter()
            .map(|path| RecentProject {
                path: path.to_string_lossy().into_owned(),
                name: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                exists: path.is_file(),
            })
            .collect())
    })
}

/// Forgets one recent entry (a missing file the user clicked, spec.md 3.1).
#[tauri::command]
pub fn forget_recent_project(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<Vec<RecentProject>> {
    forget_recent(&state, path)
}

/// [`forget_recent_project`] without a Tauri handle.
pub fn forget_recent(state: &AppState, path: String) -> Result<Vec<RecentProject>> {
    let path = PathBuf::from(path);
    state.with_session(|session| {
        session.settings.recent_projects.retain(|p| p != &path);
        session.settings.save(&state.paths.settings_file())
    })?;
    recent(state)
}

/// Clears the recent list.
///
/// Unlike the remembering on open, a failed write is **not** swallowed:
/// reaching the settings file is the whole of this operation, and a list that
/// silently comes back at the next launch is worse than an error now.
#[tauri::command]
pub fn clear_recent(state: tauri::State<'_, AppState>) -> Result<Vec<RecentProject>> {
    clear_recent_projects(&state)
}

/// [`clear_recent`] without a Tauri handle.
pub fn clear_recent_projects(state: &AppState) -> Result<Vec<RecentProject>> {
    state.with_session(|session| {
        session.settings.recent_projects.clear();
        session.settings.save(&state.paths.settings_file())
    })?;
    recent(state)
}
