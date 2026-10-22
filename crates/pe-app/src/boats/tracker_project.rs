//! Build a fleet off to the side; cancellation or failure leaves the open file intact.
use super::matching::Profile;
use crate::{
    commands::AppState,
    error::{AppError, Result},
    projects::ProjectSummary,
    session::OpenProject,
};
use pe_core::{Boat, Project, Source, SourceKind, track::Tracker};
use pe_trackers::{Fetcher, TrackerClient, TrackerEvent};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, TS)]
pub struct BoatImportProgress {
    pub running: bool,
    pub phase: String,
    pub current: String,
    pub done: u32,
    pub total: u32,
    pub fraction: f64,
}
#[derive(Debug, Default)]
pub struct BoatImportJob {
    progress: Mutex<BoatImportProgress>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
    preview: Mutex<Option<TrackerProjectPreview>>,
}
#[derive(Debug)]
struct TrackerProjectPreview {
    open: OpenProject,
    previous: Option<(u64, u64)>,
    discard_unsaved: bool,
}
impl BoatImportJob {
    fn update(&self, f: impl FnOnce(&mut BoatImportProgress)) {
        if let Ok(mut p) = self.progress.lock() {
            f(&mut p);
        }
    }
    fn start(&self) -> Result<Arc<AtomicBool>> {
        let mut slot = self
            .cancel
            .lock()
            .map_err(|_| AppError::Internal("boat import lock poisoned".into()))?;
        if slot.is_some() {
            return Err(AppError::Internal(
                "A tracker project is already being opened".into(),
            ));
        }
        *self
            .preview
            .lock()
            .map_err(|_| AppError::Internal("boat preview lock poisoned".into()))? = None;
        let cancel = Arc::new(AtomicBool::new(false));
        *slot = Some(cancel.clone());
        self.update(|p| {
            *p = BoatImportProgress {
                running: true,
                phase: "download".into(),
                ..Default::default()
            }
        });
        Ok(cancel)
    }
    fn finish(&self) {
        self.update(|p| p.running = false);
        if let Ok(mut slot) = self.cancel.lock() {
            *slot = None;
        }
    }
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct BoatImportReport {
    pub boat: String,
    pub details: BTreeMap<String, String>,
    pub model: Option<String>,
    pub polars: u32,
    pub tracks: u32,
    pub missing_tracks: u32,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize, TS)]
pub struct TrackerProjectResult {
    pub project: ProjectSummary,
    pub boats: Vec<BoatImportReport>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BoatMatchMode {
    #[default]
    IdenticalModel,
    ExactBoat,
}

impl BoatMatchMode {
    fn matches(self, boat: &Profile, candidate: &Profile) -> bool {
        match self {
            Self::IdenticalModel => boat.same_model(candidate),
            Self::ExactBoat => boat.same_vessel(candidate),
        }
    }
}
fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(pe_trackers::TrackerError::Cancelled.into())
    } else {
        Ok(())
    }
}
#[tauri::command]
pub fn boat_import_status(state: tauri::State<'_, AppState>) -> Result<BoatImportProgress> {
    state
        .boat_import
        .progress
        .lock()
        .map(|p| p.clone())
        .map_err(|_| AppError::Internal("boat import lock poisoned".into()))
}
#[tauri::command]
pub fn cancel_boat_import(state: tauri::State<'_, AppState>) {
    if let Ok(slot) = state.boat_import.cancel.lock()
        && let Some(cancel) = slot.as_ref()
    {
        cancel.store(true, Ordering::Relaxed);
    }
}
#[tauri::command]
pub fn confirm_tracker_project(
    state: tauri::State<'_, AppState>,
    project_id: u64,
) -> Result<ProjectSummary> {
    confirm(&state, project_id)
}

/// Only the reviewed preview can replace the session, and only if it is still current.
pub fn confirm(state: &AppState, project_id: u64) -> Result<ProjectSummary> {
    let mut preview = state
        .boat_import
        .preview
        .lock()
        .map_err(|_| AppError::Internal("boat preview lock poisoned".into()))?;
    state.with_session(|session| {
        let pending = preview
            .as_ref()
            .filter(|p| p.open.project.id.raw() == project_id)
            .ok_or_else(|| {
                AppError::Internal("The tracker project preview is no longer available".into())
            })?;
        if session
            .open
            .as_ref()
            .map(|o| (o.project.id.raw(), o.revision))
            != pending.previous
        {
            return Err(AppError::Internal(
                "The project changed during import".into(),
            ));
        }
        session.refuse_to_discard(pending.discard_unsaved)?;
        let pending = preview.take().ok_or_else(|| {
            AppError::Internal("The tracker project preview is no longer available".into())
        })?;
        crate::projects::forget_open(state, session);
        session.replace(pending.open);
        state.env_jobs.cancel(None);
        Ok(ProjectSummary::of(session.require_open()?))
    })
}

#[tauri::command]
pub fn discard_tracker_project(state: tauri::State<'_, AppState>, project_id: u64) -> Result<()> {
    discard_preview(&state, project_id)
}

/// Late dialog cleanup must not discard a newer preview or touch the open project.
pub fn discard_preview(state: &AppState, project_id: u64) -> Result<()> {
    let mut preview = state
        .boat_import
        .preview
        .lock()
        .map_err(|_| AppError::Internal("boat preview lock poisoned".into()))?;
    if preview
        .as_ref()
        .is_some_and(|p| p.open.project.id.raw() == project_id)
    {
        *preview = None;
    }
    Ok(())
}
#[tauri::command]
pub async fn open_tracker_project(
    state: tauri::State<'_, AppState>,
    tracker: String,
    url: String,
    discard_unsaved: bool,
    match_mode: Option<BoatMatchMode>,
    classes: Option<Vec<String>>,
) -> Result<TrackerProjectResult> {
    let tracker = match tracker.as_str() {
        "yellowbrick" => Tracker::YellowBrick,
        "geovoile" => Tracker::Geovoile,
        "bluewater" => Tracker::BlueWaterTracks,
        _ => {
            return Err(AppError::BadOption {
                field: "Tracker",
                value: tracker,
            });
        }
    };
    let client: Arc<dyn TrackerClient> = crate::trackers::client_of(tracker)?.into();
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        open_with_mode(
            &state,
            client,
            &url,
            discard_unsaved,
            match_mode.unwrap_or_default(),
            &classes.unwrap_or_default(),
        )
    })
    .await
    .map_err(|error| AppError::Internal(format!("The tracker project task failed: {error}")))?
}

struct RunningImport<'a>(&'a BoatImportJob);
impl Drop for RunningImport<'_> {
    fn drop(&mut self) {
        self.0.finish();
    }
}

pub fn open_with(
    state: &AppState,
    client: Arc<dyn TrackerClient>,
    url: &str,
    discard_unsaved: bool,
) -> Result<TrackerProjectResult> {
    open_with_mode(
        state,
        client,
        url,
        discard_unsaved,
        BoatMatchMode::default(),
        &[],
    )
}

/// The race with only the boats of `classes`, or whole for none (asked
/// 2026-10-04; several 2026-10-05). Each of a boat's groups is a class
/// (`pe_trackers::event::classes`), and a boat in several chosen ones stays
/// one boat, so one tab. A class no boat sails in is refused, so a mistyped
/// one is not silently dropped.
fn of_classes(event: Arc<TrackerEvent>, classes: &[String]) -> Result<Arc<TrackerEvent>> {
    let wanted: std::collections::BTreeSet<&str> = classes
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .collect();
    if wanted.is_empty() {
        return Ok(event);
    }
    let tracker = event.event.tracker;
    let class_sets: Vec<Vec<String>> = event
        .boats
        .iter()
        .map(|b| pe_trackers::event::classes(tracker, b.division.as_deref()))
        .collect();
    if let Some(missing) = wanted
        .iter()
        .find(|c| !class_sets.iter().flatten().any(|k| k == **c))
    {
        return Err(AppError::BadOption {
            field: "Class",
            value: (*missing).to_owned(),
        });
    }
    let mut only = (*event).clone();
    only.boats = event
        .boats
        .iter()
        .zip(&class_sets)
        .filter(|(_, sets)| sets.iter().any(|c| wanted.contains(c.as_str())))
        .map(|(b, _)| b.clone())
        .collect();
    Ok(Arc::new(only))
}

pub fn open_with_mode(
    state: &AppState,
    client: Arc<dyn TrackerClient>,
    url: &str,
    discard_unsaved: bool,
    match_mode: BoatMatchMode,
    classes: &[String],
) -> Result<TrackerProjectResult> {
    let previous = state.with_session(|s| {
        s.refuse_to_discard(discard_unsaved)?;
        Ok(s.open.as_ref().map(|o| (o.project.id.raw(), o.revision)))
    })?;
    let cancel = state.boat_import.start()?;
    let _running = RunningImport(&state.boat_import);
    (|| {
        let reference = client.resolve(url)?;
        let event = if let Some(event) = state.trackers.cached(reference.tracker, &reference.key) {
            event
        } else {
            let fetcher = Fetcher::new(
                pe_trackers::event::name(reference.tracker),
                Duration::from_secs(u64::from(crate::settings::DEFAULT_TIMEOUT_S)),
                cancel.clone(),
            )?;
            let event = Arc::new(client.fetch(&reference, &fetcher, &mut |p| {
                state.boat_import.update(|s| s.fraction = p.fraction())
            })?);
            state.trackers.keep(event.clone(), &reference.key);
            event
        };
        check(&cancel)?;
        let event = of_classes(event, classes)?;
        let (document, reports, warnings) = build_with_mode(state, &event, &cancel, match_mode)?;
        check(&cancel)?;
        state.with_session(|session| {
            if session
                .open
                .as_ref()
                .map(|o| (o.project.id.raw(), o.revision))
                != previous
            {
                return Err(AppError::Internal(
                    "The project changed during import".into(),
                ));
            }
            session.refuse_to_discard(discard_unsaved)
        })?;
        let mut open = OpenProject::created(document);
        let project = ProjectSummary::of(&mut open);
        check(&cancel)?;
        *state
            .boat_import
            .preview
            .lock()
            .map_err(|_| AppError::Internal("boat preview lock poisoned".into()))? =
            Some(TrackerProjectPreview {
                open,
                previous,
                discard_unsaved,
            });
        Ok(TrackerProjectResult {
            project,
            boats: reports,
            warnings,
        })
    })()
}

fn polar_source(project: &mut Project, label: String, kind: SourceKind) {
    let colour = project.next_palette_colour();
    let id = project.allocate_source_id();
    project.sources.push(Source::new(id, label, colour, kind));
}
fn track_key(tracker: Tracker, url: &str, boat: &str) -> String {
    let key = pe_trackers::event::client(tracker)
        .and_then(|client| client.resolve(url).ok())
        .map(|event| event.key)
        .unwrap_or_else(|| url.trim_end_matches('/').to_owned());
    format!("{tracker:?}:{key}:{boat}")
}

/// Purely local matching after the explicitly requested tracker download.
pub fn build(
    state: &AppState,
    event: &TrackerEvent,
    cancel: &AtomicBool,
) -> Result<(Project, Vec<BoatImportReport>, Vec<String>)> {
    build_with_mode(state, event, cancel, BoatMatchMode::default())
}

pub fn build_with_mode(
    state: &AppState,
    event: &TrackerEvent,
    cancel: &AtomicBool,
    match_mode: BoatMatchMode,
) -> Result<(Project, Vec<BoatImportReport>, Vec<String>)> {
    check(cancel)?;
    if event.boats.is_empty() {
        return Err(AppError::Internal("The tracker returned no boats".into()));
    }
    // The embedded catalogue with what was scraped since (spec.md 5.4), so a
    // boat is matched against this year's certificates too.
    let catalogue = crate::orc::catalogue(state)?;
    let mut warnings = Vec::new();
    let orr = match crate::orr::records(state) {
        Ok(r) => r,
        Err(e) => {
            warnings.push(e.to_string());
            Arc::new(Vec::new())
        }
    };
    let vessels = match crate::library::catalogue::matching_vessels(state) {
        Ok(v) => v,
        Err(e) => {
            warnings.push(e.to_string());
            Vec::new()
        }
    };
    let settings = crate::library::settings(state)?;
    state.boat_import.update(|p| {
        p.phase = "matching".into();
        p.total = event.boats.len() as u32;
        p.fraction = 0.0;
    });
    let mut orc_profiles = Vec::new();
    for id in 0..catalogue.len() as u32 {
        check(cancel)?;
        if let Some(entry) = catalogue.entry(id) {
            let mut fields = BTreeMap::new();
            fields.insert("model".into(), entry.model.clone().unwrap_or_default());
            fields.insert("builder".into(), entry.builder.clone().unwrap_or_default());
            fields.insert("sailNumber".into(), entry.sail_no.clone());
            if let Some(loa) = entry.size[0] {
                fields.insert("loa".into(), loa.to_string());
            }
            orc_profiles.push((id, Profile::from_details(&fields)));
        }
    }
    let orr_profiles: Vec<_> = orr
        .iter()
        .map(|entry| {
            let mut fields = BTreeMap::new();
            if let Some(details) = &entry.details {
                fields.extend(details.list_fields.clone());
                fields.extend(details.fields.values().map(|f| {
                    (
                        f.label.clone(),
                        f.display.clone().unwrap_or_else(|| f.value.clone()),
                    )
                }));
            }
            fields.insert("model".into(), entry.model.clone().unwrap_or_default());
            fields.insert("sailNumber".into(), entry.sail_no.clone());
            if let Some(loa) = entry.size.loa {
                fields.insert("loa".into(), loa.to_string());
            }
            Profile::from_details(&fields)
        })
        .collect();
    let identity_candidates: Vec<_> = orc_profiles
        .iter()
        .map(|(_, p)| p.clone())
        .chain(orr_profiles.iter().cloned())
        .chain(vessels.iter().map(|v| v.profile.clone()))
        .collect();
    let staging = AppState::new(state.paths.clone());
    state.with_session(|s| {
        staging.with_session(|staged| {
            staged.settings = s.settings.clone();
            Ok(())
        })
    })?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let mut boats = Vec::new();
    let mut reports = Vec::new();
    for (index, boat) in event.boats.iter().enumerate() {
        check(cancel)?;
        state.boat_import.update(|p| {
            p.current = boat.name.clone();
            p.done = index as u32;
        });
        let name = if boat.name.trim().is_empty() {
            boat.sail.clone().unwrap_or_else(|| boat.id.clone())
        } else {
            boat.name.clone()
        };
        let mut details = boat.details.clone();
        if let Some(model) = &boat.model {
            details
                .entry("model".into())
                .or_insert_with(|| model.clone());
        }
        if let Some(sail) = &boat.sail {
            details
                .entry("sailNumber".into())
                .or_insert_with(|| sail.clone());
        }
        if let Some(division) = &boat.division {
            details
                .entry("division".into())
                .or_insert_with(|| division.clone());
        }
        details.insert("trackerUrl".into(), event.event.url.clone());
        details.insert("trackerBoatId".into(), boat.id.clone());
        let profile = Profile::from_details(&details).resolve_model(&identity_candidates);
        let model = if profile.models.len() == 1 {
            profile.models.first().cloned()
        } else {
            None
        };
        if let Some(model) = &model {
            details.insert("matchedModel".into(), model.clone());
        }
        let mut project = Project::new(
            name.clone(),
            Boat {
                name: name.clone(),
                details: details.clone(),
                ..Default::default()
            },
            now,
        );
        let mut report = BoatImportReport {
            boat: name,
            details,
            model,
            polars: 0,
            tracks: 0,
            missing_tracks: 0,
            warnings: Vec::new(),
        };
        for (id, candidate) in &orc_profiles {
            check(cancel)?;
            if !match_mode.matches(&profile, candidate) {
                continue;
            }
            if let Some(record) = catalogue.record(*id) {
                if project.sources.iter().any(|source| matches!(&source.kind, SourceKind::Orc { record: existing } if pe_orc::same_certificate(existing, &record))) { continue; }
                polar_source(
                    &mut project,
                    record.name.clone(),
                    SourceKind::Orc {
                        record: Box::new(record),
                    },
                );
                report.polars += 1;
            }
        }
        let mut orr_seen = BTreeSet::new();
        for (record, candidate) in orr.iter().zip(&orr_profiles) {
            check(cancel)?;
            if match_mode.matches(&profile, candidate) && orr_seen.insert(record.key()) {
                polar_source(
                    &mut project,
                    format!("{} · ORR {} · {}", record.name, record.year, record.variant),
                    SourceKind::Orr {
                        record: Box::new(record.clone()),
                    },
                );
                report.polars += 1;
            }
        }
        let mut pending = Vec::new();
        let mut seen = BTreeSet::new();
        if !boat.fixes.is_empty() {
            pending.push(crate::trackers::pending(event, boat));
            seen.insert(track_key(event.event.tracker, &event.event.url, &boat.id));
        }
        for vessel in &vessels {
            if !match_mode.matches(&profile, &vessel.profile) {
                continue;
            }
            for hit in &vessel.tracks {
                check(cancel)?;
                let tracker = match hit.source.as_str() {
                    "YELLOWBRICK" => Some(Tracker::YellowBrick),
                    "BLUEWATER" => Some(Tracker::BlueWaterTracks),
                    "GEOVOILE" | "GEOVOILEOLD" | "OLDGEOVOILE" => Some(Tracker::Geovoile),
                    _ => None,
                };
                let key = tracker
                    .filter(|_| !hit.tracker_boat_id.is_empty() && !hit.original_url.is_empty())
                    .map(|t| track_key(t, &hit.original_url, &hit.tracker_boat_id))
                    .unwrap_or_else(|| {
                        format!(
                            "{}:{}:{}",
                            hit.competition_id, hit.storage_key, hit.participant_id
                        )
                    });
                if seen.contains(&key) {
                    continue;
                }
                match crate::library::catalogue::read_hit(&settings, hit) {
                    Ok(track) => {
                        seen.insert(key);
                        pending.push(track);
                    }
                    Err(error) => {
                        report.missing_tracks += 1;
                        report.warnings.push(error.to_string());
                    }
                }
            }
        }
        report.tracks = pending.len() as u32;
        staging.with_session(|s| {
            s.replace(OpenProject::created(project));
            Ok(())
        })?;
        crate::tracks::add_tracks(&staging, pending, Vec::new())?;
        let project = staging.with_session(|s| s.document())?;
        boats.push(project);
        reports.push(report);
        state.boat_import.update(|p| {
            p.done = (index + 1) as u32;
            p.fraction = p.done as f64 / f64::from(p.total.max(1));
        });
    }
    let mut root = boats.remove(0);
    root.name = event.title.clone();
    root.boat_tabs = boats;
    root.validate()?;
    Ok((root, reports, warnings))
}
