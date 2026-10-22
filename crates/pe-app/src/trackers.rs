//! Tracker imports (spec.md 7.2): download a whole event, keep it for the
//! session, and import the boats the user picks.
//!
//! [`tracker_event`] can load only the boat list, without positions, for
//! the Add tracks dialog's Open action. Import tracks explicitly downloads
//! the event, with progress and cancellation, then [`import_tracker_boats`]
//! adds the selected boats in one undoable change. Providers with a combined
//! response defer the whole download until Import tracks. Complete events
//! remain in the session cache up to [`KEPT_FIXES`] positions.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pe_core::source::{SampleFilters, TimeWindow};
use pe_core::track::{DerivationSettings, TrackOrigin, Tracker};
use pe_core::{SampleId, TrackId};
use pe_trackers::event::PositionsFrom;
use pe_trackers::{Fetcher, Progress, TrackerClient, TrackerError, TrackerEvent};
use pe_tracks::build_track;
use serde::Serialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::tracks::{Pending, TrackImportFailure, TrackImportResult, add_tracks, count};

/// The event carrying [`TrackerProgress`] while an event downloads.
pub const PROGRESS_EVENT: &str = "tracker://progress";

/// The event carrying the downloading event's boat list, a
/// [`TrackerEventView`] with `positions` false, before its positions.
pub const LISTED_EVENT: &str = "tracker://listed";

/// How many positions the downloaded events the session keeps may hold in
/// all; the oldest event goes first, and the latest is always kept. A
/// position is 56 bytes in memory, so the Fastnet 2025 (444 boats, 714,380
/// positions) is about 40 MB, and the cap about 110 MB.
pub const KEPT_FIXES: usize = 2_000_000;

/// How many points of each boat the dialog's map preview gets.
pub const PREVIEW_POINTS: usize = 64;

/// How often progress is reported at most.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// The events downloaded this session, and the download running now.
#[derive(Debug, Default)]
pub struct TrackerSession {
    events: Mutex<Vec<Arc<TrackerEvent>>>,
    // A race in legs (Geovoile, spec.md 7.2): the page shows its *current*
    // leg, so an address pasted without one resolves to a key without a
    // leg, but the event downloads and is kept under the key the page
    // actually gave (with the leg). Without this, re-pasting the same
    // address would never hit the cache: `(tracker, requested key)` to the
    // actual key it was kept under.
    aliases: Mutex<Vec<(Tracker, String, String)>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl TrackerSession {
    pub(crate) fn cached(&self, tracker: Tracker, key: &str) -> Option<Arc<TrackerEvent>> {
        let direct = self
            .events
            .lock()
            .ok()?
            .iter()
            .find(|e| e.event.tracker == tracker && e.event.key == key)
            .cloned();
        if direct.is_some() {
            return direct;
        }
        let actual = self
            .aliases
            .lock()
            .ok()?
            .iter()
            .find(|(t, from, _)| *t == tracker && from == key)
            .map(|(_, _, to)| to.clone())?;
        self.events
            .lock()
            .ok()?
            .iter()
            .find(|e| e.event.tracker == tracker && e.event.key == actual)
            .cloned()
    }

    /// Keeps `event`, downloaded for `requested` (the key it was asked
    /// for, before the download, which may differ from `event.event.key`
    /// once a race in legs answers with the leg the page actually shows).
    /// Public for the MCP tests, which put a recorded race here rather than
    /// reach a tracker.
    pub fn keep(&self, event: Arc<TrackerEvent>, requested: &str) {
        if let Ok(mut events) = self.events.lock() {
            events.retain(|e| {
                !(e.event.tracker == event.event.tracker && e.event.key == event.event.key)
            });
            events.push(event.clone());
            let fixes = |e: &TrackerEvent| e.boats.iter().map(|b| b.fixes.len()).sum::<usize>();
            let mut total: usize = events.iter().map(|e| fixes(e)).sum();
            while events.len() > 1 && total > KEPT_FIXES {
                total -= fixes(&events.remove(0));
            }
            if let Ok(mut aliases) = self.aliases.lock() {
                // Pruned to what the kept events still hold, so an alias
                // never outlives the event it points to.
                aliases.retain(|(t, _, to)| {
                    events
                        .iter()
                        .any(|e| e.event.tracker == *t && e.event.key == *to)
                });
                if requested != event.event.key {
                    aliases
                        .retain(|(t, from, _)| !(*t == event.event.tracker && from == requested));
                    aliases.push((
                        event.event.tracker,
                        requested.to_owned(),
                        event.event.key.clone(),
                    ));
                }
            }
        }
    }

    /// Stops the running download, if any.
    pub fn cancel(&self) {
        if let Ok(slot) = self.cancel.lock()
            && let Some(flag) = slot.as_ref()
        {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

// ----------------------------------------------------------------- wire

/// A tracker's wire name.
fn tracker_of(name: &str) -> Result<Tracker> {
    match name {
        "yellowbrick" => Ok(Tracker::YellowBrick),
        "geovoile" => Ok(Tracker::Geovoile),
        "bluewater" => Ok(Tracker::BlueWaterTracks),
        other => Err(AppError::BadOption {
            field: "Tracker",
            value: other.to_owned(),
        }),
    }
}

fn tracker_name(tracker: Tracker) -> &'static str {
    match tracker {
        Tracker::YellowBrick => "yellowbrick",
        Tracker::Geovoile => "geovoile",
        Tracker::BlueWaterTracks => "bluewater",
    }
}

pub(crate) fn client_of(tracker: Tracker) -> Result<Box<dyn TrackerClient>> {
    #[cfg(feature = "webdriver")]
    if let Some(client) = automation_client(tracker) {
        return Ok(client);
    }
    pe_trackers::event::client(tracker).ok_or_else(|| AppError::BadOption {
        field: "Tracker",
        value: pe_trackers::event::name(tracker).to_owned(),
    })
}

/// YellowBrick served by the UX suite's local fixture server (D25): the
/// dialog is driven end to end with recorded responses and no live network.
///
/// Only with the WebDriver feature, only for YellowBrick, and only for a
/// loopback `http://127.0.0.1:<port>` origin, so even a test build cannot be
/// pointed at a host invariant 4 does not allow.
#[cfg(feature = "webdriver")]
fn automation_client(tracker: Tracker) -> Option<Box<dyn TrackerClient>> {
    if tracker != Tracker::YellowBrick {
        return None;
    }
    let origin = std::env::var("PE_DRIVER_YELLOWBRICK").ok()?;
    let port = origin.strip_prefix("http://127.0.0.1:")?;
    if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(Box::new(pe_trackers::yellowbrick::YellowBrick::at(
        &origin, &origin,
    )))
}

/// How far a download is (spec.md 7.7).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackerProgress.ts")]
pub struct TrackerProgress {
    /// 0–1.
    pub fraction: f64,
    /// Bytes of the response being read.
    pub bytes: f64,
    /// Whether the fallback format is being read (YellowBrick's KML).
    pub fallback: bool,
}

/// One boat of a downloaded event, as the dialog's table shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackerBoatRow.ts")]
pub struct TrackerBoatRow {
    /// The tracker's id for the boat.
    pub id: String,
    /// Boat name.
    pub name: String,
    /// Sail number.
    pub sail: Option<String>,
    /// Model or class.
    pub model: Option<String>,
    /// Division.
    pub division: Option<String>,
    /// Its classes: each of the groups the division joins (asked 2026-10-05).
    pub classes: Vec<String>,
    /// The tracker's status (`RACING`, `FINISHED`, `RETIRED`…).
    pub status: Option<String>,
    /// Positions.
    pub fixes: u32,
    /// First position, UTC epoch seconds.
    pub first: Option<i64>,
    /// Last position.
    pub last: Option<i64>,
    /// A few positions for the map preview: `[lon, lat, lon, lat, …]`,
    /// rounded to 1e-4° (about 10 m), which keeps the payload small.
    pub preview: Vec<f64>,
}

/// A downloaded event, as the dialog shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackerEventView.ts")]
pub struct TrackerEventView {
    /// `"yellowbrick"`, `"geovoile"` or `"bluewater"`.
    pub tracker: String,
    /// The tracker's key for the event.
    pub key: String,
    /// The canonical address.
    pub url: String,
    /// Title.
    pub title: String,
    /// Event start, UTC epoch seconds.
    pub start: Option<i64>,
    /// Event end.
    pub stop: Option<i64>,
    /// Whether the positions came from the fallback format.
    pub fallback: bool,
    /// For one leg of a race sailed in legs: which, from 1.
    pub leg: Option<u32>,
    /// And how many legs the race has.
    pub legs: Option<u32>,
    /// Whether this came from the session's memory rather than a download.
    pub cached: bool,
    /// Whether the positions are in. False for the boat list sent ahead of
    /// them ([`LISTED_EVENT`]): every boat's `fixes` is 0, its `first`,
    /// `last` and `preview` empty.
    pub positions: bool,
    /// Every boat.
    pub boats: Vec<TrackerBoatRow>,
}

/// The boat list sent ahead of the positions ([`LISTED_EVENT`]), with the
/// key of the download it belongs to: the dialog names each download it
/// starts, and a listing for any other (an earlier address, a closed
/// dialog's download still finishing) is ignored (M17a).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackerListed.ts")]
pub struct TrackerListed {
    /// The key the dialog gave [`tracker_event`] for this download.
    pub download: String,
    /// The event, `positions` false.
    pub event: TrackerEventView,
}

/// Up to [`PREVIEW_POINTS`] evenly spaced positions, the last included.
fn preview(fixes: &[pe_core::track::Fix]) -> Vec<f64> {
    let n = fixes.len();
    if n == 0 {
        return Vec::new();
    }
    let take = n.min(PREVIEW_POINTS);
    (0..take)
        .map(|k| {
            if take == 1 {
                0
            } else {
                k * (n - 1) / (take - 1)
            }
        })
        .flat_map(|i| [round4(fixes[i].lon), round4(fixes[i].lat)])
        .collect()
}

/// To 1e-4°: the preview is 600 units wide, and the shortest decimal of the
/// rounded value is what the IPC's JSON carries.
fn round4(value: f64) -> f64 {
    (value * 1e4).round() / 1e4
}

impl TrackerEventView {
    fn of(event: &TrackerEvent, cached: bool) -> Self {
        Self::with(event, cached, true)
    }

    /// The boat list a tracker sends ahead of the positions.
    fn listing(event: &TrackerEvent) -> Self {
        Self::with(event, false, false)
    }

    fn with(event: &TrackerEvent, cached: bool, positions: bool) -> Self {
        Self {
            tracker: tracker_name(event.event.tracker).to_owned(),
            key: event.event.key.clone(),
            url: event.event.url.clone(),
            title: event.title.clone(),
            start: event.start,
            stop: event.stop,
            fallback: event.positions_from == PositionsFrom::Fallback,
            leg: event.leg.map(|(leg, _)| leg),
            legs: event.leg.map(|(_, legs)| legs),
            cached,
            positions,
            boats: event
                .boats
                .iter()
                .map(|b| TrackerBoatRow {
                    id: b.id.clone(),
                    name: b.name.clone(),
                    sail: b.sail.clone(),
                    model: b.model.clone(),
                    division: b.division.clone(),
                    classes: pe_trackers::event::classes(
                        event.event.tracker,
                        b.division.as_deref(),
                    ),
                    status: b.status.clone(),
                    fixes: count(b.fixes.len()),
                    first: b.fixes.first().map(|f| f.t),
                    last: b.fixes.last().map(|f| f.t),
                    preview: preview(&b.fixes),
                })
                .collect(),
        }
    }
}

// ------------------------------------------------------------- download

/// Downloads (or recalls) the event `input` names with `client`, reporting
/// progress to `on_progress`. `refresh` downloads again even when the
/// session has it. Returns at once on Cancel; a download that is still
/// waiting on the network then finishes on its own and is dropped.
///
/// # Errors
/// [`AppError::Tracker`] for an address the tracker does not serve, a
/// failed or cancelled download, or a response that did not decode.
pub fn download_with(
    state: &AppState,
    client: Arc<dyn TrackerClient>,
    input: &str,
    refresh: bool,
    on_progress: impl FnMut(TrackerProgress),
) -> Result<TrackerEventView> {
    download_listed(state, client, input, refresh, on_progress, |_| {})
}

/// [`download_with`], also passing the boat list to `on_listed` as soon as
/// the tracker gives it, before the positions (at most once; never for an
/// event recalled from the session, nor for a tracker that answers
/// everything at once).
///
/// # Errors
/// As [`download_with`].
pub fn download_listed(
    state: &AppState,
    client: Arc<dyn TrackerClient>,
    input: &str,
    refresh: bool,
    on_progress: impl FnMut(TrackerProgress),
    on_listed: impl FnMut(&TrackerEventView),
) -> Result<TrackerEventView> {
    read_event(state, client, input, refresh, false, on_progress, on_listed)
}

/// Open the picker without downloading any boat positions.
pub fn list_with(
    state: &AppState,
    client: Arc<dyn TrackerClient>,
    input: &str,
    refresh: bool,
    on_progress: impl FnMut(TrackerProgress),
) -> Result<TrackerEventView> {
    read_event(state, client, input, refresh, true, on_progress, |_| {})
}

fn read_event(
    state: &AppState,
    client: Arc<dyn TrackerClient>,
    input: &str,
    refresh: bool,
    list_only: bool,
    mut on_progress: impl FnMut(TrackerProgress),
    mut on_listed: impl FnMut(&TrackerEventView),
) -> Result<TrackerEventView> {
    let event = client.resolve(input)?;
    if !refresh && let Some(cached) = state.trackers.cached(event.tracker, &event.key) {
        return Ok(TrackerEventView::of(&cached, true));
    }
    let timeout = crate::settings::DEFAULT_TIMEOUT_S;
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut slot) = state.trackers.cancel.lock() {
        // A new download replaces (and stops) any earlier one.
        if let Some(old) = slot.replace(Arc::clone(&cancel)) {
            old.store(true, Ordering::SeqCst);
        }
    }
    let fetcher = Fetcher::new(
        pe_trackers::event::name(event.tracker),
        Duration::from_secs(u64::from(timeout.max(1))),
        Arc::clone(&cancel),
    )?;
    let (tx, rx) = std::sync::mpsc::channel::<Message>();
    let worker = {
        let tx = tx.clone();
        let event = event.clone();
        std::thread::Builder::new()
            .name("tracker-download".to_owned())
            .spawn(move || {
                let listed_tx = tx.clone();
                let result = if list_only {
                    client
                        .list(&event, &fetcher, &mut |p| {
                            let _ = tx.send(Message::Progress(p));
                        })
                        .map(|listing| {
                            listing.unwrap_or_else(|| TrackerEvent {
                                title: event.key.clone(),
                                event: event.clone(),
                                start: None,
                                stop: None,
                                boats: Vec::new(),
                                positions_from: PositionsFrom::Primary,
                                leg: None,
                            })
                        })
                } else {
                    client.fetch_listed(
                        &event,
                        &fetcher,
                        &mut |p| {
                            let _ = tx.send(Message::Progress(p));
                        },
                        &mut |boats| {
                            let _ = listed_tx.send(Message::Listed(Box::new(boats)));
                        },
                    )
                };
                let _ = tx.send(Message::Done(Box::new(result)));
            })
            .map_err(|e| AppError::Internal(format!("no download thread: {e}")))?
    };
    drop(tx);
    let mut last = Instant::now() - PROGRESS_EVERY;
    let mut last_step = u32::MAX;
    let outcome = loop {
        if cancel.load(Ordering::SeqCst) {
            break Err(TrackerError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Message::Listed(boats)) => {
                on_listed(&TrackerEventView::listing(&boats));
            }
            Ok(Message::Progress(p)) => {
                if p.step != last_step || last.elapsed() >= PROGRESS_EVERY {
                    last = Instant::now();
                    last_step = p.step;
                    on_progress(TrackerProgress {
                        fraction: p.fraction(),
                        bytes: p.bytes as f64,
                        fallback: p.fallback,
                    });
                }
            }
            Ok(Message::Done(result)) => break *result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break Err(TrackerError::Network("the download stopped".to_owned()));
            }
        }
    };
    if let Ok(mut slot) = state.trackers.cancel.lock()
        && slot.as_ref().is_some_and(|f| Arc::ptr_eq(f, &cancel))
    {
        *slot = None;
    }
    if outcome.is_ok() {
        let _ = worker.join();
    }
    let downloaded = Arc::new(outcome?);
    if list_only {
        return Ok(TrackerEventView::listing(&downloaded));
    }
    state.trackers.keep(Arc::clone(&downloaded), &event.key);
    Ok(TrackerEventView::of(&downloaded, false))
}

/// What the download's worker says.
enum Message {
    Progress(Progress),
    Listed(Box<TrackerEvent>),
    Done(Box<pe_trackers::Result<TrackerEvent>>),
}

/// Resolves an event address, reading only metadata when `list_only`, or
/// downloading tracks otherwise. Complete cached events need no requests.
/// During a full download the boat list goes
/// ahead as [`LISTED_EVENT`] when the tracker gives it first.
///
/// Generic over the Tauri runtime so the MCP service's tools, and their
/// tests on a mock application, call this very command.
#[tauri::command]
pub async fn tracker_event<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    tracker: String,
    url: String,
    refresh: bool,
    download: String,
    list_only: Option<bool>,
) -> Result<TrackerEventView> {
    use tauri::{Emitter, Manager};
    let tracker = tracker_of(&tracker)?;
    let client: Arc<dyn TrackerClient> = Arc::from(client_of(tracker)?);
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        read_event(
            &state,
            client,
            &url,
            refresh,
            list_only.unwrap_or(false),
            |p| {
                let _ = app.emit(PROGRESS_EVENT, &p);
            },
            |listing| {
                let _ = app.emit(
                    LISTED_EVENT,
                    TrackerListed {
                        download: download.clone(),
                        event: listing.clone(),
                    },
                );
            },
        )
    })
    .await
    .map_err(|e| AppError::Internal(format!("the download task failed: {e}")))?
}

/// Stops the running event download.
#[tauri::command]
pub fn cancel_tracker_event(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
) -> Result<()> {
    let state = state.scoped(boat_context);
    state.trackers.cancel();
    Ok(())
}

// --------------------------------------------------------------- import

/// The track of one boat, ready for its ids.
pub(crate) fn pending(event: &TrackerEvent, boat: &pe_trackers::TrackerBoat) -> Pending {
    let label = if boat.name.is_empty() {
        boat.sail.clone().unwrap_or_else(|| boat.id.clone())
    } else {
        boat.name.clone()
    };
    let origin = TrackOrigin::Tracker {
        tracker: event.event.tracker,
        event_url: event.event.url.clone(),
        event_title: event.title.clone(),
        boat_id: boat.id.clone(),
        boat_name: label.clone(),
        sail_no: boat.sail.clone(),
        model: boat.model.clone(),
        division: boat.division.clone(),
        race_start: boat.start,
        race_finish: boat.finish,
    };
    let mut next = 0u64;
    let (track, report) = build_track(
        TrackId(0),
        origin,
        boat.fixes.clone(),
        DerivationSettings::default(),
        || {
            next += 1;
            SampleId(next)
        },
    );
    // The default time window is the boat's start and finish, so pre-start
    // and post-finish motoring is out (spec.md 7.6).
    let mut filters = SampleFilters::default();
    if boat.start.is_some() || boat.finish.is_some() {
        filters.time_window = Some(TimeWindow {
            start: boat.start,
            end: boat
                .finish
                .filter(|end| boat.start.is_none_or(|start| *end > start)),
        });
    }
    Pending {
        file: event.title.clone(),
        label,
        track,
        report,
        filters,
    }
}

/// [`import_tracker_boats`] without a Tauri handle.
pub fn import_boats(
    state: &AppState,
    tracker: Tracker,
    key: &str,
    boats: &[String],
) -> Result<TrackImportResult> {
    state.with_session(|session| session.require_open().map(|_| ()))?;
    let event = state
        .trackers
        .cached(tracker, key)
        .ok_or_else(|| AppError::Doing {
            doing: "import from",
            what: key.to_owned(),
            why: "the event is no longer downloaded in this session; open it again".to_owned(),
        })?;
    let mut pendings = Vec::new();
    let mut failures = Vec::new();
    for id in boats {
        let Some(boat) = event.boat(id) else {
            failures.push(TrackImportFailure::simple(
                id,
                "no-boat",
                format!("{}: no boat {id}", event.title),
            ));
            continue;
        };
        if boat.fixes.is_empty() {
            failures.push(TrackImportFailure::simple(
                &boat.name,
                "no-positions",
                format!(
                    "{}: the tracker has no positions for {}",
                    event.title, boat.name
                ),
            ));
            continue;
        }
        pendings.push(pending(&event, boat));
    }
    add_tracks(state, pendings, failures)
}

/// Imports the chosen boats of a downloaded event, one track source each,
/// as one undoable change (spec.md 7.2).
#[tauri::command]
pub async fn import_tracker_boats(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    tracker: String,
    key: String,
    boats: Vec<String>,
) -> Result<TrackImportResult> {
    let state = state.scoped(boat_context);
    import_boats(&state, tracker_of(&tracker)?, &key, &boats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_keep_the_ends_and_stay_small() {
        let fix = |t: i64| pe_core::track::Fix {
            tws: None,
            twd_from: None,
            t,
            lat: t as f64,
            lon: -(t as f64),
            cog: None,
            sog: None,
        };
        let fixes: Vec<_> = (0..1000).map(fix).collect();
        let p = preview(&fixes);
        assert_eq!(p.len(), 2 * PREVIEW_POINTS);
        assert_eq!(&p[..2], &[0.0, 0.0]);
        assert_eq!(&p[p.len() - 2..], &[-999.0, 999.0]);
        assert_eq!(preview(&fixes[..1]), vec![0.0, 0.0]);
        assert!(preview(&[]).is_empty());
    }

    #[test]
    fn tracker_names_cross_the_wire() {
        for t in [
            Tracker::YellowBrick,
            Tracker::Geovoile,
            Tracker::BlueWaterTracks,
        ] {
            assert_eq!(tracker_of(tracker_name(t)).expect("known"), t);
        }
        assert!(tracker_of("nope").is_err());
    }

    fn event(key: &str, fixes: usize) -> Arc<TrackerEvent> {
        let fix = pe_core::track::Fix {
            tws: None,
            twd_from: None,
            t: 0,
            lat: 0.0,
            lon: 0.0,
            cog: None,
            sog: None,
        };
        Arc::new(TrackerEvent {
            event: pe_trackers::EventRef {
                tracker: Tracker::YellowBrick,
                key: key.to_owned(),
                url: String::new(),
            },
            title: String::new(),
            start: None,
            stop: None,
            boats: vec![pe_trackers::TrackerBoat {
                details: Default::default(),
                id: "1".to_owned(),
                name: String::new(),
                sail: None,
                model: None,
                division: None,
                status: None,
                start: None,
                finish: None,
                fixes: vec![fix; fixes],
            }],
            positions_from: PositionsFrom::Primary,
            leg: None,
        })
    }

    /// The session keeps events up to a total number of positions, not a
    /// number of events: many small events stay, a huge one alone stays.
    #[test]
    fn the_session_cache_is_capped_by_positions() {
        let session = TrackerSession::default();
        for k in 0..10 {
            let key = format!("small{k}");
            session.keep(event(&key, 1000), &key);
        }
        assert!(
            session.cached(Tracker::YellowBrick, "small0").is_some(),
            "ten small events fit"
        );
        session.keep(event("big", KEPT_FIXES - 5000), "big");
        assert!(session.cached(Tracker::YellowBrick, "small4").is_none());
        assert!(session.cached(Tracker::YellowBrick, "small5").is_some());
        assert!(session.cached(Tracker::YellowBrick, "big").is_some());
        // Over the cap on its own: kept, everything older dropped.
        session.keep(event("huge", KEPT_FIXES + 1), "huge");
        assert!(session.cached(Tracker::YellowBrick, "huge").is_some());
        assert!(session.cached(Tracker::YellowBrick, "big").is_none());
        assert!(session.cached(Tracker::YellowBrick, "small9").is_none());
    }

    /// A race in legs (Geovoile, spec.md 7.2): the address pasted names no
    /// leg, but the page shows one, so the event downloads and is kept
    /// under a key with the leg. Re-pasting the same, leg-less address
    /// must still find it, or every reopen would download again (M11
    /// review carry).
    #[test]
    fn a_multi_leg_event_is_found_by_the_key_it_was_asked_for() {
        let session = TrackerSession::default();
        let requested = "x.geovoile.com/2024/".to_owned();
        let actual = "x.geovoile.com/2024/?leg=2".to_owned();
        session.keep(event(&actual, 10), &requested);
        assert!(
            session.cached(Tracker::YellowBrick, &requested).is_some(),
            "found by the key it was asked for"
        );
        assert!(
            session.cached(Tracker::YellowBrick, &actual).is_some(),
            "still found by its own key too"
        );
        // A later download under a different actual key replaces the
        // alias, so the requested key never points at a stale event.
        let other = "x.geovoile.com/2024/?leg=3".to_owned();
        session.keep(event(&other, 10), &requested);
        let found = session
            .cached(Tracker::YellowBrick, &requested)
            .expect("still found");
        assert_eq!(found.event.key, other);
    }
}
