//! Scraping YellowBrick, Geovoile and Blue Water races into the track
//! library (asked 2026-10-04): each finished race's tracks become GeoJSON
//! files under the library's folder, and its boats records in
//! `boat-metadata.json`, so the Tracks panel's search finds them. Nothing is
//! written to a database.
//!
//! Discovery, the finished-race check and Geovoile's legs are the trackers'
//! own (`pe_trackers::library`); an ongoing, future or unverified race is
//! skipped. Retained for internal callers; Settings has no scraper controls
//! and neither startup nor shutdown schedules a scrape.

use super::LibrarySettings;
use super::catalogue::{BoatTrackHit, Metadata};
use crate::{
    commands::AppState,
    error::{AppError, Context, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{Emitter, Manager};
use ts_rs::TS;

/// The event a scrape's progress is sent on, for the status bar.
pub const PROGRESS: &str = "library://scrape";

/// How many finished races are saved to the metadata at a time: the file can
/// be hundreds of megabytes, so it is not rewritten after every race.
const SAVE_EVERY: usize = 10;

/// How often a busy answer is tried again, and the first pause; each later
/// pause doubles, up to two minutes.
const SCRAPE_RETRIES: u32 = 6;
const SCRAPE_BACKOFF: Duration = Duration::from_secs(5);

/// The least time between two requests' starts.
const REQUEST_SPACING: Duration = Duration::from_secs(1);

/// The pause between two races.
const RACE_PAUSE: Duration = Duration::from_secs(2);

/// The wait before the races a busy tracker turned away are tried again.
const BUSY_COOL_OFF: Duration = Duration::from_secs(60);

/// What a scrape is doing, or did last.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct ScrapeProgress {
    /// Whether one is running.
    pub running: bool,
    /// Whether the person started it (the status bar shows only these).
    pub manual: bool,
    /// Races looked at.
    pub done: u32,
    /// Races queued.
    pub total: u32,
    /// Boat tracks saved.
    pub tracks: u32,
    /// Races skipped as unfinished or unverified.
    pub skipped: u32,
    /// Races not fetched because the library already holds them.
    pub held: u32,
    /// Races or discoveries that failed.
    pub failed: u32,
    /// What it is on now.
    pub current: String,
    /// Whether it was cancelled.
    pub cancelled: bool,
    /// What stopped it, when something did.
    pub error: Option<String>,
    /// What went wrong along the way, the first hundred.
    pub failures: Vec<String>,
}

#[derive(Debug, Default)]
struct Store {
    progress: ScrapeProgress,
    cancel: Option<Arc<AtomicBool>>,
}

/// The scrape's state in the application.
#[derive(Debug, Default)]
pub struct ScrapeState {
    store: Mutex<Store>,
    shutdown_started: AtomicBool,
    shutdown_finished: AtomicBool,
}

impl ScrapeState {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Store>> {
        self.store
            .lock()
            .map_err(|_| AppError::Internal("The scrape's lock was poisoned".into()))
    }

    pub(crate) fn running(&self) -> bool {
        self.lock().is_ok_and(|s| s.progress.running)
    }
}

fn update<R: tauri::Runtime>(app: &tauri::AppHandle<R>, f: impl FnOnce(&mut ScrapeProgress)) {
    let state = app.state::<AppState>();
    let progress = match state.library.scrape.lock() {
        Ok(mut s) => {
            f(&mut s.progress);
            s.progress.clone()
        }
        Err(_) => return,
    };
    let _ = app.emit(PROGRESS, progress);
}

/// The scrape's progress.
#[tauri::command]
pub fn library_scrape_status(state: tauri::State<'_, AppState>) -> Result<ScrapeProgress> {
    Ok(state.library.scrape.lock()?.progress.clone())
}

/// Asks the running scrape to stop; the races it finished stay in the library.
#[tauri::command]
pub fn cancel_library_scrape(state: tauri::State<'_, AppState>) -> Result<()> {
    if let Some(c) = &state.library.scrape.lock()?.cancel {
        c.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Starts a scrape by hand, with the library settings as saved.
#[tauri::command]
pub fn start_library_scrape(app: tauri::AppHandle) -> Result<ScrapeProgress> {
    start(app, true)
}

fn start<R: tauri::Runtime>(app: tauri::AppHandle<R>, manual: bool) -> Result<ScrapeProgress> {
    let state = app.state::<AppState>();
    let settings = super::settings(&state)?;
    settings.validate()?;
    if settings.geojson_directory.is_empty() {
        return Err(AppError::Internal(
            "Choose a GeoJSON directory before scraping".into(),
        ));
    }
    if state.library.download.running() {
        return Err(AppError::Internal(
            "Wait for the metadata download to finish before scraping".into(),
        ));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = ScrapeProgress {
        running: true,
        manual,
        ..Default::default()
    };
    {
        let mut store = state.library.scrape.lock()?;
        if store.progress.running {
            return Err(AppError::Internal("A scrape is already running".into()));
        }
        store.progress = progress.clone();
        store.cancel = Some(Arc::clone(&cancel));
    }
    let _ = app.emit(PROGRESS, progress.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let result = scrape(&app, &settings, &cancel);
        let state = app.state::<AppState>();
        update(&app, |p| {
            p.running = false;
            p.current = String::new();
            p.cancelled = result.is_err() && cancel.load(Ordering::SeqCst);
            if let Err(e) = &result
                && !p.cancelled
            {
                p.error = Some(e.to_string());
            }
        });
        if let Ok(mut store) = state.library.scrape.lock() {
            store.cancel = None;
            if let Ok(bytes) = serde_json::to_vec(&store.progress) {
                let _ = pe_core::io::write_atomic(
                    &state.paths.config_dir.join("library-last-scrape.json"),
                    &bytes,
                );
            }
        }
    });
    Ok(progress)
}

fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::SeqCst) {
        Err(AppError::Tracker(pe_trackers::TrackerError::Cancelled))
    } else {
        Ok(())
    }
}

fn failure<R: tauri::Runtime>(app: &tauri::AppHandle<R>, what: &str, error: &str) {
    update(app, |p| {
        p.failed += 1;
        if p.failures.len() < 100 {
            p.failures.push(format!("{what}: {error}"));
        }
    });
}

fn queue_key(event: &pe_trackers::EventRef) -> String {
    let key = if event.tracker == pe_core::track::Tracker::YellowBrick {
        event.key.to_ascii_lowercase()
    } else {
        event.key.clone()
    };
    format!("{:?}/{key}", event.tracker)
}

/// A stable id: the same race and boat get the same records on every scrape.
fn guid(key: &str) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, key.as_bytes()).to_string()
}

fn competition_id(event: &pe_trackers::EventRef) -> String {
    key_competition_id(
        pe_trackers::library::source(event.tracker),
        &queue_key(event),
    )
}

/// The event id the scraper gives a race's event: from its source and its
/// catalogue key, so the metadata download can tell scraped records apart.
pub(super) fn event_id(source: &str, event_key: &str) -> String {
    guid(&format!("polarexplorer/library/{source}/{event_key}/event"))
}

fn key_competition_id(source: &str, key: &str) -> String {
    guid(&format!("polarexplorer/library/{source}/{key}/race"))
}

/// The leg a race's name gives: "Défi Azimut 2019 - Leg 2" (the snapshot's
/// names) or "Défi Azimut (2/2)" (Geovoile's own).
fn leg_from_name(name: &str) -> Option<u32> {
    let lower = name.to_lowercase();
    if let Some(at) = lower.rfind("leg ") {
        let digits: String = lower[at + 4..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(n) = digits.parse() {
            return Some(n);
        }
    }
    let open = name.rfind('(')?;
    let (n, _) = name[open + 1..].split_once('/')?;
    n.trim().parse().ok()
}

/// The race keys a search record stands for. A Geovoile race in legs is
/// read as its legs (`…?leg=2`), but an address without one shows a leg too,
/// which the record's name says; and any leg stands for the race's own
/// address, so holding one leg holds the race (asked 2026-10-05).
fn keys_of(source: &str, url: &str, name: &str) -> Vec<String> {
    let Ok(e) = pe_trackers::library::resolve_source(source, url) else {
        return Vec::new();
    };
    let key = queue_key(&e);
    let mut keys = vec![key.clone()];
    if e.tracker == pe_core::track::Tracker::Geovoile {
        match key.split_once("?leg=") {
            Some((base, _)) => keys.push(base.to_owned()),
            None => {
                if let Some(leg) = leg_from_name(name) {
                    keys.push(format!("{key}?leg={leg}"));
                }
            }
        }
    }
    keys
}

fn scrape<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    settings: &LibrarySettings,
    cancel: &Arc<AtomicBool>,
) -> Result<()> {
    let state = app.state::<AppState>();
    let timeout = crate::settings::DEFAULT_TIMEOUT_S;
    // Patient with a tracker that answers a burst with 503s (asked
    // 2026-10-05): requests a second apart, and a busy answer tried again
    // after 5, 10, 20, 40, 80 and 120 s, longer when the server asks.
    let fetcher = pe_trackers::Fetcher::new(
        "The track library",
        Duration::from_secs(u64::from(timeout)),
        Arc::clone(cancel),
    )?
    .with_retries(SCRAPE_RETRIES)
    .with_backoff(SCRAPE_BACKOFF)
    .with_spacing(REQUEST_SPACING);
    update(app, |p| p.current = "Reading the boat metadata".into());
    let mut metadata = super::catalogue::read_metadata(&state, settings)?;
    let root = Path::new(&settings.geojson_directory);
    std::fs::create_dir_all(root).doing("create", root.display())?;
    let mut unsaved = usize::from(drop_duplicates(&mut metadata, root) > 0);
    let mut held = held(&metadata);
    let mut urls = BTreeMap::new();
    let mut yellowbrick_races = BTreeMap::new();
    let explicit: Vec<_> = settings
        .scrape_urls
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if explicit.is_empty() {
        for tracker in DOWNLOAD_ORDER {
            check(cancel)?;
            let name = pe_trackers::event::name(tracker);
            let fetcher = fetcher.for_tracker(name);
            update(app, |p| p.current = format!("Discovering {name}"));
            let discovered: Result<Vec<String>> = if tracker == pe_core::track::Tracker::YellowBrick
            {
                discover_yellowbrick(app, settings, &metadata, &fetcher, &mut yellowbrick_races)
            } else {
                pe_trackers::library::discover(&fetcher, tracker).map_err(Into::into)
            };
            match discovered {
                Ok(found) => {
                    for u in found {
                        if let Ok(e) = pe_trackers::library::resolve(&u) {
                            urls.entry(queue_key(&e)).or_insert((u, e));
                        }
                    }
                }
                Err(e) => failure(app, &format!("{name} discovery"), &e.to_string()),
            }
        }
    } else {
        for u in explicit {
            let e = pe_trackers::library::resolve(u)?;
            urls.insert(queue_key(&e), (u.to_owned(), e));
        }
    }
    // A race the library holds is never fetched again (asked 2026-10-05).
    let mut queue: Vec<_> = urls.into_values().collect();
    in_download_order(&mut queue);
    let before = queue.len();
    queue.retain(|(_, e)| !held.contains(&queue_key(e)));
    let already = (before - queue.len()) as u32;
    update(app, |p| p.held += already);
    let mut cursor = 0;
    // Races a busy tracker turned away, tried once more at the end.
    let mut later = Vec::new();
    let mut second_round = false;
    let outcome: Result<()> = (|| {
        loop {
            if cursor == queue.len() {
                if second_round || later.is_empty() {
                    break;
                }
                second_round = true;
                let n = later.len();
                update(app, |p| {
                    p.current = format!("Waiting before trying {n} busy races again");
                });
                pause(cancel, BUSY_COOL_OFF)?;
                queue.append(&mut later);
            }
            check(cancel)?;
            let (original, event) = queue[cursor].clone();
            update(app, |p| {
                p.current = original.clone();
                p.done = cursor as u32;
                p.total = queue.len() as u32;
            });
            let key = queue_key(&event);
            let result: Result<()> = (|| {
                // A Geovoile leg queued during this scrape may be held by now.
                if held.contains(&key) {
                    update(app, |p| p.held += 1);
                    return Ok(());
                }
                let client = crate::trackers::client_of(event.tracker)?;
                let fetcher = fetcher.for_tracker(pe_trackers::event::name(event.tracker));
                let catalogue = yellowbrick_races.get(&key).and_then(Option::as_ref);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs() as i64);
                let fetched = match client.fetch_for_scrape(&event, &fetcher, &mut |_| {}, now) {
                    Ok(fetched) => fetched,
                    Err(pe_trackers::TrackerError::Unavailable { .. }) if !second_round => {
                        later.push((original.clone(), event.clone()));
                        return Ok(());
                    }
                    Err(e) => return Err(e.into()),
                };
                use pe_trackers::library::completion::ScrapeFetch;
                let legs = match &fetched {
                    ScrapeFetch::Finished(event) => event.leg,
                    ScrapeFetch::Skipped { legs } => *legs,
                };
                if let Some((_, legs)) = legs {
                    for leg in 1..=legs {
                        let site = pe_trackers::geovoile::site(&original)?;
                        let url = format!(
                            "{}?leg={leg}",
                            site.url().split('?').next().unwrap_or_default()
                        );
                        let e = client.resolve(&url)?;
                        let leg_key = queue_key(&e);
                        if held.contains(&leg_key) {
                            continue;
                        }
                        if !queue
                            .iter()
                            .chain(later.iter())
                            .any(|(_, old)| queue_key(old) == leg_key)
                        {
                            queue.push((url, e));
                        }
                    }
                }
                let ScrapeFetch::Finished(event) = fetched else {
                    update(app, |p| p.skipped += 1);
                    return Ok(());
                };
                // An address without a leg showed one: the library has it already?
                let shown = queue_key(&event.event);
                if shown != key && held.contains(&shown) {
                    update(app, |p| p.held += 1);
                    return Ok(());
                }
                // Recorded under the leg it is, so the next scrape knows it.
                let recorded = match event.leg {
                    Some((leg, _)) if shown != key => format!(
                        "{}?leg={leg}",
                        pe_trackers::geovoile::site(&original)?
                            .url()
                            .split('?')
                            .next()
                            .unwrap_or_default()
                    ),
                    _ => original.clone(),
                };
                let count = save_race(&mut metadata, root, &event, &recorded, catalogue)?;
                held.extend(keys_of(
                    pe_trackers::library::source(event.event.tracker),
                    &recorded,
                    &event.title,
                ));
                held.insert(key.clone());
                update(app, |p| p.tracks += count as u32);
                unsaved += 1;
                if unsaved >= SAVE_EVERY {
                    update(app, |p| p.current = "Saving the boat metadata".into());
                    super::catalogue::write_metadata(&state, settings, &metadata)?;
                    unsaved = 0;
                }
                Ok(())
            })();
            if let Err(e) = result {
                check(cancel)?;
                failure(app, &original, &e.to_string());
            }
            cursor += 1;
            // A breath between races, so a long scrape never bursts.
            if cursor < queue.len() {
                pause(cancel, RACE_PAUSE)?;
            }
        }
        Ok(())
    })();
    // Every race finished so far stays in the library, cancelled or not.
    if unsaved > 0 {
        update(app, |p| p.current = "Saving the boat metadata".into());
        super::catalogue::write_metadata(&state, settings, &metadata)?;
    }
    update(app, |p| {
        p.done = cursor as u32;
        p.total = queue.len() as u32;
    });
    outcome
}

/// The trackers in the order their races download (asked 2026-10-05).
const DOWNLOAD_ORDER: [pe_core::track::Tracker; 3] = [
    pe_core::track::Tracker::BlueWaterTracks,
    pe_core::track::Tracker::YellowBrick,
    pe_core::track::Tracker::Geovoile,
];

/// Puts the queue in [`DOWNLOAD_ORDER`], each tracker's races keeping theirs.
fn in_download_order(queue: &mut [(String, pe_trackers::EventRef)]) {
    queue.sort_by_key(|(_, e)| DOWNLOAD_ORDER.iter().position(|t| *t == e.tracker));
}

/// Waits `time`, ending at once on cancel.
fn pause(cancel: &AtomicBool, time: Duration) -> Result<()> {
    let until = std::time::Instant::now() + time;
    while std::time::Instant::now() < until {
        check(cancel)?;
        std::thread::sleep(Duration::from_millis(50).min(until - std::time::Instant::now()));
    }
    Ok(())
}

/// YellowBrick catalogue races the library already holds codes for: by
/// catalogue id (the event's `scrapedOriginalId`), the race addresses.
fn yellowbrick_codes(metadata: &Metadata) -> BTreeMap<String, Vec<String>> {
    let events: BTreeMap<&str, &str> = metadata
        .tables
        .get("CalendarEvents")
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|e| e["source"] == "YELLOWBRICK")
        .filter_map(|e| Some((e["id"].as_str()?, e["scrapedOriginalId"].as_str()?)))
        .collect();
    let mut codes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for unit in metadata
        .tables
        .get("CompetitionUnits")
        .map_or(&[][..], Vec::as_slice)
    {
        if let (Some(event), Some(url)) = (
            unit["calendarEventId"].as_str(),
            unit["scrapedUrl"].as_str(),
        ) && let Some(id) = events.get(event)
        {
            codes
                .entry((*id).to_owned())
                .or_default()
                .push(url.to_owned());
        }
    }
    codes
}

/// The races the library holds, by queue key: every search record's race
/// address, the snapshot's and the scraper's own alike.
fn held(metadata: &Metadata) -> BTreeSet<String> {
    let races: BTreeSet<(&str, &str, &str)> = metadata
        .tracks
        .iter()
        .map(|t| {
            (
                t.source.as_str(),
                t.original_url.as_str(),
                t.event_name.as_str(),
            )
        })
        .collect();
    races
        .into_iter()
        .flat_map(|(source, url, name)| keys_of(source, url, name))
        .collect()
}

/// Removes the scraper's own copy of a race the library also holds from
/// elsewhere (a SYRF snapshot): the records, and the track files it wrote.
/// Answers how many races it removed.
pub(super) fn drop_duplicates(metadata: &mut Metadata, root: &Path) -> usize {
    let mut races: BTreeMap<String, (BTreeSet<String>, String)> = BTreeMap::new();
    for t in &metadata.tracks {
        for key in keys_of(&t.source, &t.original_url, &t.event_name) {
            races
                .entry(key.clone())
                .or_insert_with(|| (BTreeSet::new(), key_competition_id(&t.source, &key)))
                .0
                .insert(t.competition_id.clone());
        }
    }
    let ours: BTreeSet<String> = races
        .into_values()
        .filter(|(ids, mine)| ids.len() > 1 && ids.contains(mine))
        .map(|(_, mine)| mine)
        .collect();
    if ours.is_empty() {
        return 0;
    }
    for t in metadata
        .tracks
        .iter()
        .filter(|t| ours.contains(&t.competition_id))
    {
        if let Ok(path) = safe_file(root, &t.storage_key) {
            let _ = std::fs::remove_file(path);
        }
    }
    metadata
        .tracks
        .retain(|t| !ours.contains(&t.competition_id));
    let units = metadata
        .tables
        .entry("CompetitionUnits".to_owned())
        .or_default();
    units.retain(|u| !u["id"].as_str().is_some_and(|id| ours.contains(id)));
    ours.len()
}

/// A storage key under `root`, never through traversal or a symlink out of it.
fn safe_file(root: &Path, key: &str) -> Result<std::path::PathBuf> {
    if Path::new(key)
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(AppError::Internal("Unsafe track storage key".into()));
    }
    let path = root.join(key);
    let canonical_root = root.canonicalize().doing("read", root.display())?;
    let mut ancestor = path.as_path();
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| AppError::Internal("Invalid track path".into()))?;
    }
    if !ancestor
        .canonicalize()
        .doing("read", ancestor.display())?
        .starts_with(&canonical_root)
    {
        return Err(AppError::Internal(
            "Track storage key points outside the GeoJSON directory".into(),
        ));
    }
    Ok(path)
}

fn iso(t: Option<i64>) -> Option<String> {
    t.and_then(|t| chrono::DateTime::from_timestamp(t, 0).map(|d| d.to_rfc3339()))
}

/// The mean of points, `[lon, lat]`.
fn mean(points: &[[f64; 2]]) -> Option<[f64; 2]> {
    if points.is_empty() {
        return None;
    }
    let n = points.len() as f64;
    let (x, y) = points
        .iter()
        .fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
    Some([x / n, y / n])
}

fn point(p: Option<[f64; 2]>) -> Value {
    p.map_or(
        Value::Null,
        |p| json!({"type": "Point", "coordinates": [p[0], p[1]]}),
    )
}

fn upsert(metadata: &mut Metadata, table: &str, row: Value) {
    let rows = metadata.tables.entry(table.to_owned()).or_default();
    let id = row["id"].clone();
    match rows.iter_mut().find(|r| r["id"] == id) {
        Some(old) => *old = row,
        None => rows.push(row),
    }
}

/// Saves one finished race into the library: each boat's track as a GeoJSON
/// file the import reads, and the race, its event, its boats and one search
/// record per track into `metadata` (replacing the race's own earlier ones).
/// Answers how many tracks were saved.
fn save_race(
    metadata: &mut Metadata,
    root: &Path,
    event: &pe_trackers::TrackerEvent,
    original_url: &str,
    catalogue: Option<&pe_trackers::library::yellowbrick::Race>,
) -> Result<usize> {
    let source = pe_trackers::library::source(event.event.tracker);
    let cid = competition_id(&event.event);
    let family = event
        .event
        .key
        .split('?')
        .next()
        .unwrap_or(&event.event.key);
    let event_key = catalogue.map_or(family, |r| r.id.as_str());
    let eid = event_id(source, event_key);
    let event_name = catalogue.map_or_else(|| event.title.clone(), |r| r.title.clone());
    upsert(
        metadata,
        "CalendarEvents",
        json!({"id": eid, "name": event_name, "source": source, "scrapedOriginalId": event_key}),
    );
    let firsts: Vec<[f64; 2]> = event
        .boats
        .iter()
        .filter_map(|b| b.fixes.first())
        .map(|f| [f.lon, f.lat])
        .collect();
    let lasts: Vec<[f64; 2]> = event
        .boats
        .iter()
        .filter_map(|b| b.fixes.last())
        .map(|f| [f.lon, f.lat])
        .collect();
    upsert(
        metadata,
        "CompetitionUnits",
        json!({"id": cid, "name": event.title, "calendarEventId": eid, "scrapedUrl": original_url, "scrapedOriginalId": event.event.key,
            "startTime": iso(event.start), "endTime": iso(event.stop), "isCompleted": true,
            "approximateStartLocation": point(mean(&firsts)), "approximateEndLocation": point(mean(&lasts))}),
    );
    metadata.tracks.retain(|t| t.competition_id != cid);
    let mut count = 0;
    for boat in &event.boats {
        if boat.fixes.is_empty() {
            continue;
        }
        let sail = boat.sail.clone().unwrap_or_default();
        // A vessel is one boat across races: by name and sail number.
        let vid = guid(&format!(
            "polarexplorer/library/{source}/vessel/{}|{}",
            boat.name.trim(),
            sail.trim()
        ));
        let pid = guid(&format!(
            "polarexplorer/library/{source}/{}/participant/{}",
            queue_key(&event.event),
            boat.id
        ));
        let storage = format!("individual-tracks/{cid}/vessel/provided/{pid}.geojson");
        let path = safe_file(root, &storage)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).doing("create", parent.display())?;
        }
        let coordinates: Vec<Value> = boat
            .fixes
            .iter()
            .map(|f| json!([f.lon, f.lat, 0, f.t * 1000, f.sog, f.cog]))
            .collect();
        let feature = json!({"type": "Feature",
            "properties": {"vesselParticipantId": pid, "competitionUnitId": cid, "detail": {"lon": 0, "lat": 1, "elevation": 2, "time": 3, "sog": 4, "cog": 5}},
            "geometry": {"type": "LineString", "coordinates": coordinates}});
        pe_core::io::write_atomic(
            &path,
            &serde_json::to_vec(&feature).doing("encode", "track")?,
        )?;
        // Every field the tracker gives the boat is searchable.
        let details: serde_json::Map<String, Value> = boat
            .details
            .iter()
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        upsert(
            metadata,
            "Vessels",
            json!({"id": vid, "vesselId": boat.id, "publicName": boat.name, "sailNumber": boat.sail, "model": boat.model,
                "source": source, "details": details}),
        );
        metadata.tracks.push(BoatTrackHit {
            id: format!("{cid}/{pid}"),
            vessel_id: vid,
            participant_id: pid,
            competition_id: cid.clone(),
            boat_name: boat.name.clone(),
            sail_number: sail,
            model: boat.model.clone().unwrap_or_default(),
            source: source.to_owned(),
            event_name: event_name.clone(),
            original_url: original_url.to_owned(),
            start: iso(boat.start.or(event.start)),
            end: iso(boat.finish.or(event.stop)),
            tracker_boat_id: boat.id.clone(),
            storage_key: storage,
            file_available: true,
        });
        count += 1;
    }
    Ok(count)
}

fn discover_yellowbrick<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    settings: &LibrarySettings,
    metadata: &Metadata,
    fetcher: &pe_trackers::Fetcher,
    races: &mut BTreeMap<String, Option<pe_trackers::library::yellowbrick::Race>>,
) -> Result<Vec<String>> {
    use pe_trackers::library::yellowbrick::{Credentials, discover};
    let credentials = (!settings.yellowbrick_user_key.trim().is_empty()).then_some(Credentials {
        user_key: &settings.yellowbrick_user_key,
        device_id: &settings.yellowbrick_device_id,
    });
    let known = yellowbrick_codes(metadata);
    let report = discover(fetcher, credentials.as_ref(), &known, &mut |message| {
        update(app, |p| p.current = message)
    })?;
    for race in &report.races {
        for url in &race.urls {
            if let Ok(event) = pe_trackers::library::resolve_source("YELLOWBRICK", url) {
                races
                    .entry(queue_key(&event))
                    .and_modify(|old| {
                        // Reused codes under different catalogue ids are ambiguous;
                        // keep the track without inventing a parent.
                        if old.as_ref().is_some_and(|r| r.id != race.id) {
                            *old = None;
                        }
                    })
                    .or_insert_with(|| Some(race.clone()));
            }
        }
    }
    update(app, |p| {
        p.failures.extend(report.warnings.iter().take(20).cloned());
        if report.unresolved() > 0 {
            p.failed += report.unresolved() as u32;
            p.failures.push(format!(
                "YellowBrick: {} catalogue races without codes were not queued. Check the YellowBrick credentials in Settings or add race URLs.",
                report.unresolved()
            ));
        }
    });
    Ok(report.urls())
}

/// Quitting waits for an already running scrape; it never starts one.
pub fn shutdown<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let state = app.state::<AppState>();
    let scrape = &state.library.scrape;
    if scrape.shutdown_finished.load(Ordering::SeqCst) {
        return false;
    }
    let running = scrape.running();
    if !running {
        return false;
    }
    if scrape.shutdown_started.swap(true, Ordering::SeqCst) {
        return true;
    }
    let revision = state
        .with_session(|s| Ok(s.open.as_ref().map(|o| (o.project.id, o.revision))))
        .ok()
        .flatten();
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        while app.state::<AppState>().library.scrape.running() {
            std::thread::sleep(Duration::from_millis(100));
        }
        let state = app.state::<AppState>();
        state
            .library
            .scrape
            .shutdown_finished
            .store(true, Ordering::SeqCst);
        let current = state
            .with_session(|s| Ok(s.open.as_ref().map(|o| (o.project.id, o.revision))))
            .ok()
            .flatten();
        if current != revision {
            state.exit_allowed.store(false, Ordering::SeqCst);
        }
        crate::quit::request(&app);
    });
    true
}

#[cfg(test)]
mod tests;
