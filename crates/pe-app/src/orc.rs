//! The ORC polars section over IPC (spec.md 5).
//!
//! The catalogue is embedded in the binary (`pe_orc`) and decoded on first
//! use, together with the certificates scraped from ORC's service and kept
//! on this computer (spec.md 5.4), each listed once. Searching and adding
//! never touch the network; a scrape does, through `pe-trackers`, and only
//! when the person starts one or has chosen a schedule (invariant 4). A
//! search returns a light result per boat — the fields the list shows and a
//! thumbnail of three wind speeds — and Add copies the full record into the
//! project as an ORC source, one undo entry, with the next palette colour.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pe_core::source::Source;
use pe_core::{Command, SourceKind};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::projects::ProjectSummary;

/// Tells the frontend the catalogue changed: a scrape finished. Searches
/// refresh; no project changes.
pub const UPDATED_EVENT: &str = "orc://updated";

/// A scrape's progress, kept while Settings is closed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export_to = "OrcProgress.ts")]
pub struct OrcProgress {
    /// Whether a scrape is running.
    pub running: bool,
    /// Countries downloaded.
    pub done: u32,
    /// Countries ORC's service lists.
    pub total: u32,
    /// Certificates read so far.
    pub certificates: u32,
    /// Certificates the last finished scrape added to the catalogue.
    pub added: u32,
    /// Certificates it replaced with a newer reading of themselves.
    pub updated: u32,
    /// Certificates it took out because ORC no longer lists them as valid.
    pub removed: u32,
    /// Certificates and countries it could not read.
    pub failed: u32,
    /// Up to twenty of those, each with its reason.
    pub failures: Vec<String>,
    /// The last scrape was cancelled; the catalogue is as it was.
    pub cancelled: bool,
    /// What stopped the whole scrape, if anything did.
    pub error: Option<String>,
}

#[derive(Debug, Default)]
struct Store {
    /// The embedded catalogue with the scraped certificates, once loaded.
    catalogue: Option<Arc<pe_orc::Catalogue>>,
    cancel: Option<Arc<AtomicBool>>,
    progress: OrcProgress,
}

/// The catalogue as this session holds it, and the running scrape. What a
/// scrape downloads is never written anywhere: only the certificates read
/// from it are, in the catalogue's own format.
#[derive(Debug, Default)]
pub struct OrcCatalogue {
    store: Mutex<Store>,
}

fn lock(catalogue: &OrcCatalogue) -> Result<std::sync::MutexGuard<'_, Store>> {
    catalogue
        .store
        .lock()
        .map_err(|_| AppError::Internal("ORC catalogue lock was poisoned".into()))
}

/// Where the scraped certificates are kept.
pub fn cache_path(state: &AppState) -> std::path::PathBuf {
    state.paths.config_dir.join("orc-catalogue.bin")
}

/// The scraped certificates on disk. None yet is an empty list; a file that
/// cannot be read is too, so a damaged store costs the scraped certificates
/// until the next scrape writes it again and never the catalogue itself.
fn stored(state: &AppState) -> Vec<pe_orc::Scraped> {
    std::fs::read(cache_path(state))
        .ok()
        .and_then(|bytes| pe_orc::format::decode_scraped(&bytes).ok())
        .unwrap_or_default()
}

/// The history label of Add: an English key to translate.
pub const ADD_ORC: &str = "Add ORC polar";
/// The most results one search returns.
pub const MAX_LIMIT: u32 = 200;
/// The wind speeds the thumbnail draws, knots: light, medium and strong. Each
/// is matched to the nearest the certificate has.
const THUMB_TWS: [f64; 3] = [6.0, 12.0, 20.0];

/// What the catalogue is and where it came from (spec.md 5.1), for the
/// section's footer, its filters and About.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "OrcCatalogueInfo.ts")]
pub struct OrcCatalogueInfo {
    /// How many certificates it holds.
    pub records: u32,
    /// How many of them were scraped from ORC's service (spec.md 5.4).
    pub scraped: u32,
    /// When the scraped certificates were last written, UTC epoch seconds.
    #[ts(type = "number | null")]
    pub scraped_at: Option<i64>,
    /// The repository it was built from, `"jieter/orc-data"`.
    pub source: String,
    /// The commit it was built from, in full.
    pub commit: String,
    /// That commit's date, `YYYY-MM-DD`.
    pub commit_date: String,
    /// The day it was built, `YYYY-MM-DD`.
    pub build_date: String,
    /// Every country with a certificate, as three-letter codes, sorted.
    pub countries: Vec<String>,
    /// The earliest year built.
    pub year_min: Option<i32>,
    /// The latest year built.
    pub year_max: Option<i32>,
}

/// The search's filters and its per-field queries (spec.md 5.2). A field
/// query left empty is no condition.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, TS)]
#[serde(default)]
#[ts(export_to = "OrcFilters.ts")]
pub struct OrcFilters {
    /// Earliest year built, inclusive.
    pub year_min: Option<i32>,
    /// Latest year built, inclusive.
    pub year_max: Option<i32>,
    /// A three-letter country code.
    pub country: Option<String>,
    /// Words that must start words of the boat name.
    pub name: String,
    /// Words that must start words of the sail number (`GBR1124`, `GBR 1124`
    /// and `GBR/1124` alike).
    pub sail_no: String,
    /// Words that must start words of the type or model.
    pub model: String,
    /// Words that must start words of the builder.
    pub builder: String,
    /// Words that must start words of the designer.
    pub designer: String,
    /// The start of the certificate year.
    pub certificate_year: String,
    /// Minimum LOA, beam, draft, displacement, main, genoa, spinnaker,
    /// asymmetric spinnaker and crew weight, in metres, kg and m².
    pub size_min: Vec<Option<f64>>,
    /// Upper measurement bounds in the same order.
    pub size_max: Vec<Option<f64>>,
}

/// One wind speed's curve in a result's thumbnail: the points the polar has
/// at that speed, by increasing angle.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "OrcThumbCurve.ts")]
pub struct OrcThumbCurve {
    /// Wind speed, knots.
    pub tws: f64,
    /// True wind angles, degrees.
    pub twa: Vec<f64>,
    /// Boat speeds, knots, one per angle.
    pub bsp: Vec<f64>,
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "OrcHit.ts")]
pub struct OrcHit {
    /// Catalogue id, for [`orc_add`]. Valid for this build only.
    pub id: u32,
    /// Boat name.
    pub name: String,
    /// Sail number as shown; empty when there is none.
    pub sail_no: String,
    /// Three-letter country code.
    pub country: String,
    /// Type or model.
    pub model: Option<String>,
    /// Builder.
    pub builder: Option<String>,
    /// Year built.
    pub year: Option<i32>,
    /// Year of the certificate, when known.
    pub certificate_year: Option<i32>,
    /// ORC's reference number of the certificate, for one that was scraped
    /// (spec.md 5.4): what tells two valid certificates of one boat apart.
    pub ref_no: Option<String>,
    /// Whether the open project already holds this certificate.
    pub in_project: bool,
    /// Light, medium and strong wind curves for the thumbnail.
    pub thumb: Vec<OrcThumbCurve>,
}

/// A search's answer.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "OrcSearchResult.ts")]
pub struct OrcSearchResult {
    /// How many certificates matched in all.
    pub total: u32,
    /// The best of them, best first.
    pub hits: Vec<OrcHit>,
}

fn build(scraped: &[pe_orc::Scraped]) -> Result<Arc<pe_orc::Catalogue>> {
    pe_orc::Catalogue::with_scraped(pe_orc::EMBEDDED, scraped)
        .map(Arc::new)
        .map_err(|e| AppError::Internal(e.to_string()))
}

/// The catalogue: the embedded one and what was scraped, loaded once.
pub fn catalogue(state: &AppState) -> Result<Arc<pe_orc::Catalogue>> {
    let mut store = lock(&state.orc)?;
    if let Some(catalogue) = &store.catalogue {
        return Ok(Arc::clone(catalogue));
    }
    let catalogue = build(&stored(state))?;
    store.catalogue = Some(Arc::clone(&catalogue));
    Ok(catalogue)
}

/// Writes a scrape into the store, each certificate once, and what ORC no
/// longer lists for `year` withdrawn (spec.md 5.4); then publishes the
/// catalogue with it. Sources already in a project are copies and are never
/// touched.
pub fn merge_records(
    state: &AppState,
    harvest: pe_trackers::orc::Harvest,
    year: i32,
) -> Result<pe_orc::Merged> {
    let incoming: Vec<pe_orc::Scraped> = harvest
        .records
        .iter()
        .filter_map(|record| {
            Some(pe_orc::Scraped {
                ref_no: record.ref_no.clone()?,
                entry: pe_orc::Entry::from_record(record)?,
                withdrawn: false,
            })
        })
        .collect();
    let mut scraped = stored(state);
    let merged = pe_orc::merge_scraped(&mut scraped, incoming, &harvest.listed, year);
    let bytes =
        pe_orc::format::encode_scraped(&scraped).map_err(|e| AppError::Internal(e.to_string()))?;
    let catalogue = build(&scraped)?;
    let mut store = lock(&state.orc)?;
    if store
        .cancel
        .as_ref()
        .is_some_and(|cancel| cancel.load(Ordering::SeqCst))
    {
        return Err(AppError::Tracker(pe_trackers::TrackerError::Cancelled));
    }
    pe_core::io::write_atomic(&cache_path(state), &bytes)?;
    store.catalogue = Some(catalogue);
    // Published: a Cancel from here on cannot claim it was rolled back.
    store.cancel = None;
    Ok(merged)
}

/// The catalogue's size, provenance, countries and years.
#[tauri::command(async)]
pub fn orc_catalogue_info(state: tauri::State<'_, AppState>) -> Result<OrcCatalogueInfo> {
    info(&state)
}

/// [`orc_catalogue_info`], also for tests.
pub fn info(state: &AppState) -> Result<OrcCatalogueInfo> {
    let catalogue = catalogue(state)?;
    let provenance = catalogue.provenance();
    let years = catalogue.year_range();
    Ok(OrcCatalogueInfo {
        records: u32::try_from(catalogue.certificates()).unwrap_or(u32::MAX),
        scraped: u32::try_from(catalogue.scraped()).unwrap_or(u32::MAX),
        scraped_at: crate::catalogues::written_at(&cache_path(state)),
        source: provenance.source.clone(),
        commit: provenance.commit.clone(),
        commit_date: provenance.commit_date.clone(),
        build_date: provenance.build_date.clone(),
        countries: catalogue.countries().to_vec(),
        year_min: years.map(|(min, _)| min),
        year_max: years.map(|(_, max)| max),
    })
}

/// The thumbnail curves of a record.
fn thumbnail(record: &pe_core::orc::OrcRecord) -> Vec<OrcThumbCurve> {
    let polar = pe_polar::vpp_to_polar(&record.vpp);
    thumbnail_grid(&polar)
}

/// Thumbnails use the same three representative winds for ORC and ORR grids.
pub(crate) fn thumbnail_grid(polar: &pe_polar::Polar) -> Vec<OrcThumbCurve> {
    let mut columns: Vec<usize> = THUMB_TWS
        .iter()
        .filter_map(|want| {
            (0..polar.tws.len()).min_by(|&a, &b| {
                (polar.tws[a] - want)
                    .abs()
                    .total_cmp(&(polar.tws[b] - want).abs())
            })
        })
        .collect();
    columns.dedup();
    columns
        .into_iter()
        .map(|j| {
            let (twa, bsp) = polar
                .twa
                .iter()
                .zip(&polar.bsp)
                .filter_map(|(twa, row)| row[j].map(|bsp| (*twa, bsp)))
                .unzip();
            OrcThumbCurve {
                tws: polar.tws[j],
                twa,
                bsp,
            }
        })
        .collect()
}

/// Searches the catalogue (spec.md 5.2).
#[tauri::command(async)]
pub fn orc_search(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    query: String,
    filters: OrcFilters,
    limit: u32,
    offset: Option<u32>,
) -> Result<OrcSearchResult> {
    let state = state.scoped(boat_context);
    search_page(&state, &query, filters, limit, offset.unwrap_or(0))
}

/// [`orc_search`] without a Tauri handle.
pub fn search(
    state: &AppState,
    query: &str,
    filters: OrcFilters,
    limit: u32,
) -> Result<OrcSearchResult> {
    search_page(state, query, filters, limit, 0)
}

/// Validate the shared measurement filters for both certificate catalogues.
pub(crate) fn validated_filters(filters: OrcFilters) -> Result<pe_orc::Filters> {
    let mut minima = [None; 9];
    let mut maxima = [None; 9];
    if filters.size_min.len() > 9 || filters.size_max.len() > 9 {
        return Err(AppError::BadOption {
            field: "Measurements",
            value: "too many bounds".to_owned(),
        });
    }
    for k in 0..9 {
        minima[k] = filters.size_min.get(k).copied().flatten();
        maxima[k] = filters.size_max.get(k).copied().flatten();
        if minima[k]
            .into_iter()
            .chain(maxima[k])
            .any(|v| !v.is_finite() || v < 0.0)
            || minima[k].zip(maxima[k]).is_some_and(|(min, max)| min > max)
        {
            return Err(AppError::BadOption {
                field: "Measurements",
                value: "bounds must be nonnegative and increasing".to_owned(),
            });
        }
    }
    Ok(pe_orc::Filters {
        size_min: minima,
        size_max: maxima,
        year_min: filters.year_min,
        year_max: filters.year_max,
        country: filters.country.filter(|c| !c.trim().is_empty()),
        fields: pe_orc::Fields {
            name: filters.name,
            sail_no: filters.sail_no,
            model: filters.model,
            builder: filters.builder,
            designer: filters.designer,
            certificate_year: filters.certificate_year,
        },
    })
}

/// Stable pages over the full ranked result set; only this page's thumbnails
/// are built, so paging does not transfer the whole catalogue to the UI.
pub fn search_page(
    state: &AppState,
    query: &str,
    filters: OrcFilters,
    limit: u32,
    offset: u32,
) -> Result<OrcSearchResult> {
    let catalogue = catalogue(state)?;
    let filters = validated_filters(filters)?;
    let limit = limit.clamp(1, MAX_LIMIT) as usize;
    let offset = (offset as usize).min(catalogue.len());
    let hits = catalogue.search(query, &filters, offset.saturating_add(limit));
    let records: Vec<(u32, pe_core::orc::OrcRecord)> = hits
        .ids
        .iter()
        .skip(offset)
        .take(limit)
        .filter_map(|id| catalogue.record(*id).map(|record| (*id, record)))
        .collect();
    // Which are already in the project; no project open means none are.
    let present: Vec<bool> = state.with_session(|session| {
        Ok(match session.require_open() {
            Ok(open) => records
                .iter()
                .map(|(_, record)| holds(&open.project, record))
                .collect(),
            Err(_) => vec![false; records.len()],
        })
    })?;
    Ok(OrcSearchResult {
        total: u32::try_from(hits.total).unwrap_or(u32::MAX),
        hits: records
            .into_iter()
            .zip(present)
            .map(|((id, record), in_project)| OrcHit {
                id,
                thumb: thumbnail(&record),
                name: record.name,
                sail_no: record.sail_no,
                country: record.country,
                model: record.model,
                builder: record.builder,
                year: record.year,
                certificate_year: record.certificate_year,
                ref_no: record.ref_no,
                in_project,
            })
            .collect(),
    })
}

/// Whether `project` already has `record` as a source.
fn holds(project: &pe_core::Project, record: &pe_core::orc::OrcRecord) -> bool {
    project.sources.iter().any(|source| match &source.kind {
        SourceKind::Orc { record: held } => pe_orc::same_certificate(held, record),
        _ => false,
    })
}

/// A source's label for a certificate: its name, else its model, else its
/// sail number.
fn label(record: &pe_core::orc::OrcRecord) -> String {
    [
        record.name.as_str(),
        record.model.as_deref().unwrap_or(""),
        record.sail_no.as_str(),
    ]
    .into_iter()
    .find(|text| !text.trim().is_empty())
    .unwrap_or("ORC")
    .trim()
    .to_owned()
}

/// Adds a certificate to the project as an ORC source (spec.md 5.3). A
/// certificate the project already holds is refused with kind
/// `"orc-duplicate"` unless `allow_duplicate`: the frontend asks, then calls
/// again.
#[tauri::command(async)]
pub fn orc_add(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: u32,
    allow_duplicate: bool,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    add(&state, id, allow_duplicate)
}

/// [`orc_add`] without a Tauri handle.
pub fn add(state: &AppState, id: u32, allow_duplicate: bool) -> Result<ProjectSummary> {
    state.with_session(|session| session.require_open().map(|_| ()))?;
    let record = catalogue(state)?
        .record(id)
        .ok_or_else(|| AppError::BadOption {
            field: "ORC record",
            value: id.to_string(),
        })?;
    state.with_session(|session| {
        let open = session.require_open()?;
        open.ensure_editable()?;
        if !allow_duplicate && holds(&open.project, &record) {
            return Err(AppError::DuplicateCertificate {
                name: label(&record),
            });
        }
        let colour = open.project.next_palette_colour();
        let source_id = open.project.allocate_source_id();
        let source = Source::new(
            source_id,
            label(&record),
            colour,
            SourceKind::Orc {
                record: Box::new(record),
            },
        );
        let index = open.project.sources.len();
        open.apply(Command::Batch {
            label: ADD_ORC.to_owned(),
            commands: vec![Command::AddSource {
                index,
                source: Box::new(source),
            }],
        })?;
        Ok(ProjectSummary::of(open))
    })
}

/// The scrape's progress, without fetching anything.
#[tauri::command]
pub fn orc_scrape_status(state: tauri::State<'_, AppState>) -> Result<OrcProgress> {
    status(&state)
}

/// [`orc_scrape_status`] without a Tauri handle.
pub fn status(state: &AppState) -> Result<OrcProgress> {
    Ok(lock(&state.orc)?.progress.clone())
}

/// Whether a scrape is running.
pub fn scraping(state: &AppState) -> bool {
    lock(&state.orc).is_ok_and(|store| store.progress.running)
}

/// Asks the running scrape to stop. The catalogue stays as it was: a scrape
/// is stored whole or not at all.
#[tauri::command]
pub fn cancel_orc_scrape(state: tauri::State<'_, AppState>) -> Result<()> {
    cancel(&state)
}

/// [`cancel_orc_scrape`] without a Tauri handle.
pub fn cancel(state: &AppState) -> Result<()> {
    if let Some(cancel) = &lock(&state.orc)?.cancel {
        cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Starts a scrape of ORC's service (spec.md 5.4): every country's valid
/// certificates of the current year. Only this command and the schedule the
/// person chose in Settings start one (invariant 4). Generic over the Tauri
/// runtime so the MCP service's `orc_refresh` calls this same command.
#[tauri::command]
pub fn start_orc_scrape<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<OrcProgress> {
    use tauri::{Emitter, Manager};
    // Loaded before the worker starts, so a search during the scrape does
    // not wait behind it for the first decode.
    catalogue(&state)?;
    let cancel = Arc::new(AtomicBool::new(false));
    let timeout = state.with_session(|s| Ok(s.settings.network.timeout_s))?;
    {
        let mut store = lock(&state.orc)?;
        if store.progress.running {
            return Err(AppError::Internal(
                "an ORC scrape is already running".into(),
            ));
        }
        store.cancel = Some(Arc::clone(&cancel));
        store.progress = OrcProgress {
            running: true,
            ..Default::default()
        };
    }
    let year = crate::catalogues::current_year();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let scrape = |year: i32| {
            pe_trackers::Fetcher::new(
                "ORC",
                Duration::from_secs(u64::from(timeout)),
                Arc::clone(&cancel),
            )
            .and_then(|fetcher| {
                pe_trackers::orc::scrape(
                    &fetcher,
                    year,
                    &mut |done, total, certificates| {
                        if let Ok(mut store) = lock(&state.orc) {
                            store.progress.done = u32::try_from(done).unwrap_or(u32::MAX);
                            store.progress.total = u32::try_from(total).unwrap_or(u32::MAX);
                            store.progress.certificates =
                                u32::try_from(certificates).unwrap_or(u32::MAX);
                        }
                    },
                    &mut |what, why| {
                        if let Ok(mut store) = lock(&state.orc) {
                            store.progress.failed += 1;
                            if store.progress.failures.len() < 20 {
                                store.progress.failures.push(format!("{what}: {why}"));
                            }
                        }
                    },
                )
            })
        };
        // The service holds one year's certificates. In the first days of a
        // year that may still be last year's.
        let result = scrape(year)
            .and_then(|harvest| {
                if harvest.records.is_empty() {
                    scrape(year - 1).map(|harvest| (harvest, year - 1))
                } else {
                    Ok((harvest, year))
                }
            })
            .map_err(AppError::Tracker)
            .and_then(|(harvest, year)| {
                if harvest.records.is_empty() {
                    return Err(AppError::Internal(
                        "ORC's service answered no certificates".into(),
                    ));
                }
                merge_records(&state, harvest, year)
            });
        if let Ok(mut store) = lock(&state.orc) {
            store.progress.running = false;
            store.progress.cancelled = cancel.load(Ordering::SeqCst);
            match result {
                Ok(merged) => {
                    store.progress.added = merged.added;
                    store.progress.updated = merged.updated;
                    store.progress.removed = merged.withdrawn;
                }
                Err(error) if !store.progress.cancelled => {
                    store.progress.error = Some(error.to_string());
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

    fn temp_state(label: &str) -> (AppState, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "pe-orc-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let state = AppState::new(crate::paths::AppPaths::in_directory(&root).unwrap());
        (state, root)
    }

    /// A certificate as the scraper hands it over, with a table to two
    /// decimals.
    fn scraped(ref_no: &str, sail: &str, name: &str, year: i32) -> pe_core::orc::OrcRecord {
        pe_core::orc::OrcRecord {
            ref_no: Some(ref_no.to_owned()),
            sail_no: sail.to_owned(),
            country: "ZZZ".to_owned(),
            name: name.to_owned(),
            model: Some("Test 30".to_owned()),
            year: Some(2010),
            certificate_year: Some(year),
            gph: Some(600.0),
            vpp: pe_core::orc::OrcVpp {
                angles: vec![52.0, 90.0],
                speeds: vec![6.0, 12.0],
                bsp: vec![vec![Some(5.37), Some(6.9)], vec![Some(6.02), Some(7.71)]],
                beat_angle: vec![42.6, 38.9],
                beat_vmg: vec![3.54, 4.97],
                run_angle: vec![142.8, 152.3],
                run_vmg: vec![3.87, 6.37],
            },
            ..Default::default()
        }
    }

    /// A scrape's result: the records, and every one of them on its
    /// country's list.
    fn harvest(records: Vec<pe_core::orc::OrcRecord>) -> pe_trackers::orc::Harvest {
        let mut listed =
            std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
        for record in &records {
            listed
                .entry(record.country.clone())
                .or_default()
                .extend(record.ref_no.clone());
        }
        pe_trackers::orc::Harvest { records, listed }
    }

    fn counts(added: u32, updated: u32, withdrawn: u32) -> pe_orc::Merged {
        pe_orc::Merged {
            added,
            updated,
            withdrawn,
        }
    }

    #[test]
    fn scraped_certificates_join_the_catalogue_once_and_survive_a_restart() {
        let (state, root) = temp_state("merge");
        let embedded = info(&state).unwrap().records;
        assert_eq!(info(&state).unwrap().scraped, 0);
        assert_eq!(info(&state).unwrap().scraped_at, None);

        // One boat with two valid certificates, as ORC issues them (crewed
        // and double-handed), and another boat.
        let mut double_handed = scraped("Z1DH", "ZZZ 1", "Xqzephyr", 2026);
        double_handed.size.crew_kg = Some(200.0);
        let first = vec![
            scraped("Z1", "ZZZ 1", "Xqzephyr", 2026),
            double_handed,
            scraped("Z2", "ZZZ 2", "Zenith", 2026),
        ];
        assert_eq!(
            merge_records(&state, harvest(first.clone()), 2026).unwrap(),
            counts(3, 0, 0)
        );
        // The same scrape again stores nothing twice and changes nothing.
        assert_eq!(
            merge_records(&state, harvest(first), 2026).unwrap(),
            counts(0, 0, 0)
        );
        assert_eq!(info(&state).unwrap().records, embedded + 3);
        assert_eq!(info(&state).unwrap().scraped, 3);
        let found = search(&state, "xqzephyr", OrcFilters::default(), 10).unwrap();
        assert_eq!(found.total, 2, "both of the boat's certificates");
        let references: Vec<_> = found.hits.iter().map(|h| h.ref_no.clone()).collect();
        assert!(
            references.contains(&Some("Z1".to_owned())),
            "{references:?}"
        );
        assert!(
            references.contains(&Some("Z1DH".to_owned())),
            "{references:?}"
        );
        let hit = found
            .hits
            .iter()
            .find(|h| h.ref_no.as_deref() == Some("Z1"))
            .unwrap();
        assert_eq!(hit.certificate_year, Some(2026));
        let id = hit.id;

        // Another session over the same folder reads them back, under the
        // same ids.
        let again = AppState::new(crate::paths::AppPaths::in_directory(&root).unwrap());
        let reread = catalogue(&again).unwrap();
        assert_eq!(info(&again).unwrap().records, embedded + 3);
        let record = reread.record(id).unwrap();
        assert_eq!(record.name, "Xqzephyr");
        assert_eq!(record.ref_no.as_deref(), Some("Z1"));
        assert_eq!(record.vpp.bsp[1][1], Some(7.71));
        assert!(info(&again).unwrap().scraped_at.is_some());

        // A record with no reference, or a table finer than the catalogue
        // keeps, is not stored.
        let mut nameless = scraped("Z9", "ZZZ 9", "Nameless", 2026);
        nameless.ref_no = None;
        let mut fine = scraped("Z8", "ZZZ 8", "Fine", 2026);
        fine.vpp.bsp[0][0] = Some(5.371);
        let unusable = pe_trackers::orc::Harvest {
            records: vec![nameless, fine],
            listed: Default::default(),
        };
        assert_eq!(
            merge_records(&again, unusable, 2026).unwrap(),
            counts(0, 0, 0)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_certificate_orc_took_back_leaves_the_catalogue_and_moves_no_id() {
        let (state, root) = temp_state("withdrawn");
        let embedded = info(&state).unwrap().records;
        merge_records(
            &state,
            harvest(vec![
                scraped("Z1", "ZZZ 1", "Xqzephyr", 2026),
                scraped("Z2", "ZZZ 2", "Xqzenith", 2026),
                scraped("Z3", "ZZZ 3", "Xqzulu", 2026),
            ]),
            2026,
        )
        .unwrap();
        let id_of = |state: &AppState, name: &str| {
            search(state, name, OrcFilters::default(), 10).unwrap().hits[0].id
        };
        let zulu = id_of(&state, "xqzulu");

        // Zenith's certificate was issued again under a new number.
        assert_eq!(
            merge_records(
                &state,
                harvest(vec![
                    scraped("Z1", "ZZZ 1", "Xqzephyr", 2026),
                    scraped("Z2B", "ZZZ 2", "Xqzenith", 2026),
                    scraped("Z3", "ZZZ 3", "Xqzulu", 2026),
                ]),
                2026
            )
            .unwrap(),
            counts(1, 0, 1)
        );
        assert_eq!(info(&state).unwrap().records, embedded + 3);
        let zenith = search(&state, "xqzenith", OrcFilters::default(), 10).unwrap();
        assert_eq!(zenith.total, 1, "listed once, under its new number");
        assert_eq!(zenith.hits[0].ref_no.as_deref(), Some("Z2B"));
        // The certificates after the withdrawn one are where they were.
        assert_eq!(id_of(&state, "xqzulu"), zulu);
        // And the withdrawn one's old id adds nothing by mistake.
        let old = catalogue(&state).unwrap();
        let withdrawn = (0..u32::try_from(old.len()).unwrap())
            .rev()
            .find(|id| old.record(*id).is_none())
            .expect("the withdrawn place");
        crate::projects::create(&state, "Withdrawn".to_owned(), None, false).unwrap();
        assert!(matches!(
            add(&state, withdrawn, false),
            Err(AppError::BadOption {
                field: "ORC record",
                ..
            })
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_scraped_twin_of_an_embedded_certificate_is_one_entry() {
        let (state, root) = temp_state("twin");
        let embedded = catalogue(&state).unwrap();
        let before = info(&state).unwrap().records;
        let hit = search(&state, "GBR 1124", OrcFilters::default(), 1)
            .unwrap()
            .hits[0]
            .clone();
        assert_eq!(hit.ref_no, None, "the embedded catalogue has no numbers");
        let mut twin = embedded.record(hit.id).unwrap();
        twin.ref_no = Some("T1".to_owned());
        twin.gph = Some(123.4);
        // And the same boat's certificate of another year, which is another
        // certificate.
        let mut later = twin.clone();
        later.ref_no = Some("T2".to_owned());
        later.certificate_year = twin.certificate_year.map(|year| year + 1);
        let year = twin.certificate_year.unwrap();
        assert_eq!(
            merge_records(&state, harvest(vec![twin, later]), year).unwrap(),
            counts(2, 0, 0)
        );

        let merged = catalogue(&state).unwrap();
        assert_eq!(
            info(&state).unwrap().records,
            before + 1,
            "the twin took its twin's place"
        );
        let record = merged.record(hit.id).unwrap();
        assert_eq!(record.ref_no.as_deref(), Some("T1"));
        assert_eq!(record.gph, Some(123.4));
        let found = search(&state, "GBR 1124", OrcFilters::default(), 10).unwrap();
        let same_boat: Vec<_> = found
            .hits
            .iter()
            .filter(|h| h.name == hit.name && h.sail_no == hit.sail_no)
            .map(|h| h.certificate_year)
            .collect();
        assert_eq!(
            same_boat.len(),
            2,
            "one per certificate year: {same_boat:?}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_cancelled_scrape_publishes_nothing_and_a_damaged_store_costs_only_itself() {
        let (state, root) = temp_state("cancel");
        let before = info(&state).unwrap().records;
        lock(&state.orc).unwrap().cancel = Some(Arc::new(AtomicBool::new(true)));
        assert!(matches!(
            merge_records(
                &state,
                harvest(vec![scraped("Z1", "ZZZ 1", "Xqzephyr", 2026)]),
                2026
            ),
            Err(AppError::Tracker(pe_trackers::TrackerError::Cancelled))
        ));
        assert_eq!(info(&state).unwrap().records, before);
        assert!(!cache_path(&state).exists());

        std::fs::write(cache_path(&state), b"not a store").unwrap();
        let again = AppState::new(crate::paths::AppPaths::in_directory(&root).unwrap());
        assert_eq!(info(&again).unwrap().records, before);
        // The next scrape writes it whole again.
        assert_eq!(
            merge_records(
                &again,
                harvest(vec![scraped("Z1", "ZZZ 1", "Xqzephyr", 2026)]),
                2026
            )
            .unwrap(),
            counts(1, 0, 0)
        );
        assert_eq!(info(&again).unwrap().records, before + 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_thumbnail_has_light_medium_and_strong_wind() {
        let (state, root) = temp_state("thumb");
        let catalogue = catalogue(&state).unwrap();
        let _ = std::fs::remove_dir_all(root);
        let hits = catalogue.search("GBR 1124", &pe_orc::Filters::default(), 1);
        let record = catalogue.record(hits.ids[0]).unwrap();
        let thumb = thumbnail(&record);
        assert_eq!(
            thumb.iter().map(|c| c.tws).collect::<Vec<_>>(),
            vec![6.0, 12.0, 20.0]
        );
        // Eight ORC angles plus that wind speed's beat and run angles.
        let light = &thumb[0];
        assert_eq!(light.twa.len(), 10);
        assert_eq!(light.twa[0], 47.1);
        assert!(light.twa.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_label_falls_back_from_name_to_model_to_sail() {
        let mut record = pe_core::orc::OrcRecord {
            name: " ".to_owned(),
            model: Some("Swan 112".to_owned()),
            sail_no: "GBR 1124".to_owned(),
            ..Default::default()
        };
        assert_eq!(label(&record), "Swan 112");
        record.model = None;
        assert_eq!(label(&record), "GBR 1124");
    }
}
