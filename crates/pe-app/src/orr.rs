//! The local ORR catalogue, user-started scraping and duplicate-free imports.

use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use pe_core::{Command, Source, SourceKind, orr::OrrRecord};
use serde::Serialize;
use ts_rs::TS;

use crate::{
    commands::AppState,
    error::{AppError, Result},
    orc::{OrcFilters, OrcThumbCurve},
    projects::ProjectSummary,
};

/// Signals catalogue replacement; searches refresh without changing the project.
pub const UPDATED_EVENT: &str = "orr://updated";
const BUNDLED: &str = include_str!("../../../assets/orr/catalogue.json");

/// A scrape's progress, retained while Settings is closed.
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export_to = "OrrProgress.ts")]
pub struct OrrProgress {
    /// Whether a worker is running.
    pub running: bool,
    /// Certificates processed.
    pub done: u32,
    /// Certificates in the requested valid list.
    pub total: u32,
    /// New certificate variants added by the last completed scrape.
    pub added: u32,
    /// Existing variants updated by that scrape.
    pub updated: u32,
    /// Certificates that had no usable public polar.
    pub failed: u32,
    /// Up to twenty contextual diagnostics, for the details disclosure.
    pub failures: Vec<String>,
    /// The last scrape was cancelled; the old catalogue remains intact.
    pub cancelled: bool,
    /// A job-wide failure, if any.
    pub error: Option<String>,
}

#[derive(Debug, Default)]
struct Store {
    records: Option<Arc<Vec<OrrRecord>>>,
    cancel: Option<Arc<AtomicBool>>,
    progress: OrrProgress,
}

/// Session cache and running job. Raw HTML is never written to user storage.
#[derive(Debug, Default)]
pub struct OrrCatalogue {
    store: Mutex<Store>,
}

fn lock(catalogue: &OrrCatalogue) -> Result<std::sync::MutexGuard<'_, Store>> {
    catalogue
        .store
        .lock()
        .map_err(|_| AppError::Internal("ORR catalogue lock was poisoned".into()))
}

/// Where the scraped ORR certificates are kept.
pub fn cache_path(state: &AppState) -> std::path::PathBuf {
    state.paths.config_dir.join("orr-catalogue.json")
}

fn load(path: &Path) -> Result<Vec<OrrRecord>> {
    let bundled: Vec<OrrRecord> = serde_json::from_str(BUNDLED)
        .map_err(|e| AppError::Internal(format!("ORR bundled catalogue: {e}")))?;
    let mut records: BTreeMap<_, _> = bundled.into_iter().map(|r| (r.key(), r)).collect();
    match std::fs::read(path) {
        Ok(bytes) => {
            let cached: Vec<OrrRecord> = serde_json::from_slice(&bytes).map_err(|e| {
                AppError::Internal(format!("ORR catalogue {}: {e}", path.display()))
            })?;
            for record in cached {
                record.polar.validate()?;
                // A cache written by the former polar-only scraper must not
                // mask the complete bundled copy of the same certificate.
                if record.details.is_none()
                    && records
                        .get(&record.key())
                        .is_some_and(|r| r.details.is_some())
                {
                    continue;
                }
                records.insert(record.key(), record);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(AppError::Internal(format!(
                "ORR catalogue {}: {e}",
                path.display()
            )));
        }
    }
    Ok(records.into_values().collect())
}

/// Current local records, loading the bundled snapshot and user updates once.
pub fn records(state: &AppState) -> Result<Arc<Vec<OrrRecord>>> {
    let mut store = lock(&state.orr)?;
    if let Some(records) = &store.records {
        return Ok(Arc::clone(records));
    }
    let records = Arc::new(load(&cache_path(state))?);
    store.records = Some(Arc::clone(&records));
    Ok(records)
}

/// Replace same-key records, keep unrelated years, and write atomically.
/// Existing project sources are immutable copies and are never changed here.
pub fn merge_records(state: &AppState, incoming: Vec<OrrRecord>) -> Result<(u32, u32)> {
    let held = records(state)?;
    let mut next: BTreeMap<_, _> = held.iter().cloned().map(|r| (r.key(), r)).collect();
    let (mut added, mut updated) = (0u32, 0u32);
    for record in incoming {
        record.polar.validate()?;
        let key = record.key();
        match next.get(&key) {
            None => added += 1,
            Some(old) if *old != record => updated += 1,
            _ => {}
        }
        next.insert(key, record);
    }
    let next: Vec<_> = next.into_values().collect();
    let bytes = serde_json::to_vec(&next).map_err(|e| AppError::Internal(e.to_string()))?;
    let mut store = lock(&state.orr)?;
    if store
        .cancel
        .as_ref()
        .is_some_and(|c| c.load(Ordering::SeqCst))
    {
        return Err(AppError::Tracker(pe_trackers::TrackerError::Cancelled));
    }
    pe_core::io::write_atomic(&cache_path(state), &bytes)?;
    store.records = Some(Arc::new(next));
    // Publication is the commit boundary: a later Cancel cannot claim that
    // an already published catalogue was rolled back.
    store.cancel = None;
    Ok((added, updated))
}

/// Available local catalogue metadata; this never fetches.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "OrrCatalogueInfo.ts")]
pub struct OrrCatalogueInfo {
    /// Certificate variants in the local catalogue.
    pub records: u32,
    /// Countries derived from public sail numbers.
    pub countries: Vec<String>,
    /// Certificate years currently present.
    pub years: Vec<i32>,
}

fn country(record: &OrrRecord) -> String {
    let prefix: String = record
        .sail_no
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    if prefix.len() == 3 {
        prefix.to_ascii_uppercase()
    } else {
        String::new()
    }
}

/// Read catalogue metadata for Settings and search.
#[tauri::command]
pub fn orr_catalogue_info(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
) -> Result<OrrCatalogueInfo> {
    let state = state.scoped(boat_context);
    info(&state)
}

/// Metadata without a Tauri handle, also used by catalogue tests.
pub fn info(state: &AppState) -> Result<OrrCatalogueInfo> {
    let records = records(state)?;
    let countries: std::collections::BTreeSet<_> = records
        .iter()
        .map(country)
        .filter(|s| !s.is_empty())
        .collect();
    let years: std::collections::BTreeSet<_> = records.iter().map(|r| r.year).collect();
    Ok(OrrCatalogueInfo {
        records: records.len() as u32,
        countries: countries.into_iter().collect(),
        years: years.into_iter().collect(),
    })
}

/// Same list presentation as ORC, with a stable string key and table variant.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "OrrHit.ts")]
pub struct OrrHit {
    /// Certificate SKU plus variant.
    pub id: String,
    /// Boat name.
    pub name: String,
    /// Sail number.
    pub sail_no: String,
    /// Country prefix.
    pub country: String,
    /// Model/type.
    pub model: Option<String>,
    /// Builder from the certificate data, if published.
    pub builder: Option<String>,
    /// Year built from the certificate data, if published.
    pub year: Option<i32>,
    /// Certificate year.
    pub certificate_year: Option<i32>,
    /// Offshore or short-course table.
    pub variant: String,
    /// This certificate variant already exists in the project.
    pub in_project: bool,
    /// Representative wind curves.
    pub thumb: Vec<OrcThumbCurve>,
}

/// One page of ORR results.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "OrrSearchResult.ts")]
pub struct OrrSearchResult {
    /// Total matches across all pages.
    pub total: u32,
    /// At most the requested page size.
    pub hits: Vec<OrrHit>,
}

fn matches(text: &str, query: &str) -> bool {
    let words = pe_orc::fold::words(text);
    let compact = pe_orc::fold::compact(text);
    pe_orc::fold::words(query)
        .iter()
        .all(|q| words.iter().any(|w| w.starts_with(q)) || compact.starts_with(q))
}

fn admits(record: &OrrRecord, query: &str, filters: &pe_orc::Filters) -> bool {
    let c = country(record);
    if filters
        .country
        .as_ref()
        .is_some_and(|wanted| !wanted.eq_ignore_ascii_case(&c))
        || filters
            .year_min
            .is_some_and(|min| record.build_year().is_none_or(|y| y < min))
        || filters
            .year_max
            .is_some_and(|max| record.build_year().is_none_or(|y| y > max))
    {
        return false;
    }
    let f = &filters.fields;
    if ![
        (record.name.as_str(), f.name.as_str()),
        (record.sail_no.as_str(), f.sail_no.as_str()),
        (record.model.as_deref().unwrap_or(""), f.model.as_str()),
        (record.field("builder-bt").unwrap_or(""), f.builder.as_str()),
        (
            record.field("designer-bt").unwrap_or(""),
            f.designer.as_str(),
        ),
        (&record.year.to_string(), f.certificate_year.as_str()),
    ]
    .iter()
    .all(|(text, query)| matches(text, query))
    {
        return false;
    }
    let size = &record.size;
    let measurements = [
        size.loa,
        size.beam,
        size.draft,
        size.displacement_kg,
        size.main_area,
        size.genoa_area,
        size.spinnaker_area,
        size.asym_spinnaker_area,
        size.crew_kg,
    ];
    for (k, value) in measurements.into_iter().enumerate() {
        let (min, max) = (filters.size_min[k], filters.size_max[k]);
        if (min.is_some() || max.is_some())
            && value.is_none_or(|v| {
                !v.is_finite() || min.is_some_and(|n| v < n) || max.is_some_and(|n| v > n)
            })
        {
            return false;
        }
    }
    matches(
        &format!(
            "{} {} {} {} {} {} {} {} {}",
            record.name,
            record.sail_no,
            pe_orc::fold::compact(&record.sail_no),
            record.model.as_deref().unwrap_or(""),
            record.certificate,
            record.year,
            record.variant,
            record.field("builder-bt").unwrap_or(""),
            record.field("designer-bt").unwrap_or("")
        ),
        query,
    )
}

fn holds(project: &pe_core::Project, key: &str) -> bool {
    project
        .sources
        .iter()
        .any(|source| matches!(&source.kind, SourceKind::Orr { record } if record.key() == key))
}

/// Search the local catalogue without network access.
#[tauri::command(async)]
pub fn orr_search(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    query: String,
    filters: OrcFilters,
    limit: u32,
    offset: u32,
) -> Result<OrrSearchResult> {
    let state = state.scoped(boat_context);
    search(&state, &query, filters, limit, offset)
}

/// Local search with inclusive measurement bounds and deterministic pagination.
pub fn search(
    state: &AppState,
    query: &str,
    filters: OrcFilters,
    limit: u32,
    offset: u32,
) -> Result<OrrSearchResult> {
    let records = records(state)?;
    let filters = crate::orc::validated_filters(filters)?;
    let mut found: Vec<_> = records
        .iter()
        .filter(|r| admits(r, query, &filters))
        .collect();
    found.sort_by(|a, b| {
        b.year
            .cmp(&a.year)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.key().cmp(&b.key()))
    });
    let total = found.len() as u32;
    let hits = state.with_session(|session| {
        Ok(found
            .into_iter()
            .skip(offset as usize)
            .take(limit.clamp(1, crate::orc::MAX_LIMIT) as usize)
            .map(|r| {
                let key = r.key();
                let in_project = session
                    .require_open()
                    .ok()
                    .is_some_and(|open| holds(&open.project, &key));
                OrrHit {
                    id: key,
                    name: r.name.clone(),
                    sail_no: r.sail_no.clone(),
                    country: country(r),
                    model: r.model.clone(),
                    builder: r
                        .field("builder-bt")
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned),
                    year: r.build_year(),
                    certificate_year: Some(r.year),
                    variant: r.variant.clone(),
                    in_project,
                    thumb: crate::orc::thumbnail_grid(&r.polar),
                }
            })
            .collect())
    })?;
    Ok(OrrSearchResult { total, hits })
}

/// Import a certificate without duplicating an existing source.
#[tauri::command]
pub fn orr_add(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: String,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    add(&state, &id)
}

/// Import exactly once; a repeat returns the current project without an edit.
pub fn add(state: &AppState, id: &str) -> Result<ProjectSummary> {
    let records = records(state)?;
    let record = records
        .iter()
        .find(|r| r.key() == id)
        .ok_or_else(|| AppError::BadOption {
            field: "ORR certificate",
            value: id.into(),
        })?;
    state.with_session(|session| {
        let open = session.require_open()?;
        open.ensure_editable()?;
        if holds(&open.project, id) {
            return Ok(ProjectSummary::of(open));
        }
        let variant = if record.variant == "short_course" {
            "Short Course"
        } else {
            "Offshore"
        };
        let label = format!("{} · ORR {} · {variant}", record.name, record.year);
        let source = Source::new(
            open.project.allocate_source_id(),
            label,
            open.project.next_palette_colour(),
            SourceKind::Orr {
                record: Box::new(record.clone()),
            },
        );
        let index = open.project.sources.len();
        open.apply(Command::Batch {
            label: "Add ORR polar".into(),
            commands: vec![Command::AddSource {
                index,
                source: Box::new(source),
            }],
        })?;
        Ok(ProjectSummary::of(open))
    })
}

/// Read the background scrape state.
#[tauri::command]
pub fn orr_scrape_status(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
) -> Result<OrrProgress> {
    let state = state.scoped(boat_context);
    status(&state)
}

/// Read progress without fetching or requiring an open project.
pub fn status(state: &AppState) -> Result<OrrProgress> {
    Ok(lock(&state.orr)?.progress.clone())
}

/// Request cancellation, preserving the previously saved catalogue.
#[tauri::command]
pub fn cancel_orr_scrape(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
) -> Result<()> {
    let state = state.scoped(boat_context);
    cancel(&state)
}

/// [`cancel_orr_scrape`] without a Tauri handle.
pub fn cancel(state: &AppState) -> Result<()> {
    if let Some(cancel) = &lock(&state.orr)?.cancel {
        cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Whether a scrape is running.
pub fn scraping(state: &AppState) -> bool {
    lock(&state.orr).is_ok_and(|store| store.progress.running)
}

/// Start a background scrape only on this explicit command. Generic over
/// the Tauri runtime so the MCP service's `orr_refresh` calls this same
/// command, and its tests can through a mock application.
#[tauri::command]
pub fn start_orr_scrape<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    year: i32,
) -> Result<OrrProgress> {
    let state = state.scoped(boat_context);
    use tauri::{Emitter, Manager};
    if !(2018..=2100).contains(&year) {
        return Err(AppError::BadOption {
            field: "ORR year",
            value: year.to_string(),
        });
    }
    records(&state)?;
    let cancel = Arc::new(AtomicBool::new(false));
    let timeout = state.with_session(|s| Ok(s.settings.network.timeout_s))?;
    {
        let mut store = lock(&state.orr)?;
        if store.progress.running {
            return Err(AppError::Internal(
                "an ORR scrape is already running".into(),
            ));
        }
        store.cancel = Some(Arc::clone(&cancel));
        store.progress = OrrProgress {
            running: true,
            ..Default::default()
        };
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = pe_trackers::Fetcher::new(
            "ORR",
            Duration::from_secs(u64::from(timeout)),
            Arc::clone(&cancel),
        )
        .and_then(|fetcher| {
            pe_trackers::orr::scrape(
                &fetcher,
                year,
                &mut |done, total| {
                    if let Ok(mut store) = lock(&state.orr) {
                        store.progress.done = done as u32;
                        store.progress.total = total as u32;
                    }
                },
                &mut |certificate, error| {
                    if let Ok(mut store) = lock(&state.orr) {
                        store.progress.failed += 1;
                        if store.progress.failures.len() < 20 {
                            store
                                .progress
                                .failures
                                .push(format!("{certificate}: {error}"));
                        }
                    }
                },
            )
        })
        .map_err(AppError::Tracker)
        .and_then(|records| merge_records(&state, records));
        if let Ok(mut store) = lock(&state.orr) {
            store.progress.running = false;
            store.progress.cancelled = cancel.load(Ordering::SeqCst);
            match result {
                Ok((added, updated)) => {
                    store.progress.added = added;
                    store.progress.updated = updated;
                }
                Err(error) if !store.progress.cancelled => {
                    store.progress.error = Some(error.to_string())
                }
                Err(_) => {}
            }
            store.cancel = None;
        }
        let _ = app.emit(UPDATED_EVENT, ());
    });
    status(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_never_publishes_a_partial_catalogue() {
        let root = std::env::temp_dir().join(format!("pe-orr-cancel-{}", std::process::id()));
        let state = AppState::new(crate::paths::AppPaths::in_directory(&root).unwrap());
        let before = records(&state).unwrap();
        let mut incoming = before[0].clone();
        incoming.name = "Must not publish".into();
        lock(&state.orr).unwrap().cancel = Some(Arc::new(AtomicBool::new(true)));
        assert!(matches!(
            merge_records(&state, vec![incoming]),
            Err(AppError::Tracker(pe_trackers::TrackerError::Cancelled))
        ));
        assert_eq!(*records(&state).unwrap(), *before);
        assert!(!cache_path(&state).exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
