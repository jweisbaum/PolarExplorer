//! The environment of every track sample, fetched as a job (spec.md 7.5,
//! 7.7, 13).
//!
//! **Jobs.** Queued tracks share time-ordered batches and one bounded
//! chunk-download/decode pipeline; a track queued alone runs by itself.
//! Only requested positions are sampled; compressed Whirlwind chunks can be
//! reused from the disk cache, and decoded chunks share the memory budget.
//!
//! **Partial results are kept.** Every finished batch is written into the
//! project at once, under the session lock, and marks its samples
//! `env_fetched`. Cancel stops at the next chunk; whatever was written
//! stays (status "partial"), and Refetch resumes with the samples still
//! unmarked. Because each sample's values depend only on its own time and
//! place, a resumed fetch ends exactly where an uninterrupted one does.
//!
//! **Jobs belong to the project.** A task names the project it was queued
//! for; results for a project that is no longer open, or a track that has
//! been removed or changed, are dropped, and replacing the project cancels
//! its jobs (spec.md 3.3).
//!
//! Units are converted here, once, on ingest: m/s to knots, u/v to speed
//! and direction, wind "from" and current "toward" (CLAUDE.md conventions),
//! and rounded to the precision the project stores (D27), so what is in
//! memory is what a save and a load give back.

mod fleet;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use pe_core::track::{DatasetRecord, EnvMeta, EnvStatus, Sample, compass};
use pe_core::{Command, SourceId};
use pe_env::dataset::Dataset;
use pe_env::{EnvError, EnvPoint, Interval, Options, Point, Provider, Reanalysis};
use serde::Serialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::projects::ProjectSummary;
use crate::settings::DataSource;

/// Knots per metre per second (1 kn = 1852 m/h).
pub const KN_PER_MS: f64 = 3600.0 / 1852.0;

/// The event carrying [`EnvJobsStatus`] whenever a job starts, advances
/// or ends.
pub const PROGRESS_EVENT: &str = "env://progress";
/// The event saying the open project changed under a job, so the frontend
/// fetches a fresh summary.
pub const CHANGED_EVENT: &str = "env://changed";

/// The most samples one batch asks for.
const BATCH_SAMPLES: usize = 10_000;

/// The longest stretch of track time one batch covers: 72 hours, one
/// archive inner chunk's span, so a batch plans and decodes each chunk once.
const BATCH_SPAN_S: i64 = 72 * 3600;

// ------------------------------------------------------------- status

/// One track in the job queue, as the status bar and track list show it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "EnvJobTrack.ts")]
pub struct EnvJobTrack {
    /// Boat owning this source; absent only in older UI fixtures.
    #[ts(optional)]
    pub boat_id: Option<u64>,
    /// The track source.
    pub source_id: u64,
    /// Its label.
    pub label: String,
    /// `"queued"` or `"fetching"`.
    pub state: String,
    /// How much of it is done, 0–1.
    pub fraction: f64,
}

/// Every running and queued environment fetch.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export_to = "EnvJobsStatus.ts")]
pub struct EnvJobsStatus {
    /// The running track first, then the queued ones in order.
    pub tracks: Vec<EnvJobTrack>,
    /// The last fetch that failed: `[label, message]`, until the next
    /// fetch starts.
    pub failure: Option<Vec<String>>,
    /// The last fetch that had to leave a current source out (it would not
    /// open): `[label, messages]`, until the next fetch starts.
    pub warning: Option<Vec<String>>,
}

/// Where a job reports to: the frontend in the app, a recorder in tests.
pub trait JobSink: Send + Sync {
    /// The queue changed or a task advanced.
    fn progress(&self, status: &EnvJobsStatus);
    /// The open project changed.
    fn changed(&self);
}

/// One track to fetch.
#[derive(Debug, Clone)]
struct Task {
    data_source: DataSource,
    project: u64,
    source: u64,
    label: String,
    interval: Interval,
    restart: bool,
}

#[derive(Debug)]
struct Running {
    task: Task,
    fraction: f64,
    cancelled: bool,
}

impl Task {
    fn same_track(&self, other: &Self) -> bool {
        self.project == other.project && self.source == other.source
    }
}

#[derive(Debug, Default)]
struct Queue {
    waiting: VecDeque<Task>,
    running: Vec<Running>,
    cancel: Arc<AtomicBool>,
    failure: Option<Vec<String>>,
    warning: Option<Vec<String>>,
}

/// The job queue of environment fetches.
#[derive(Debug, Default)]
pub struct EnvJobs {
    queue: Mutex<Queue>,
    wake: Condvar,
}

impl EnvJobs {
    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        // A poisoned queue is still a queue: every field is valid on its own.
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The queue as the frontend shows it.
    pub fn status(&self) -> EnvJobsStatus {
        let queue = self.lock();
        let mut tracks = Vec::new();
        for running in &queue.running {
            let task = &running.task;
            tracks.push(EnvJobTrack {
                boat_id: Some(task.project),
                source_id: task.source,
                label: task.label.clone(),
                state: "fetching".to_owned(),
                fraction: running.fraction,
            });
        }
        tracks.extend(queue.waiting.iter().map(|task| EnvJobTrack {
            boat_id: Some(task.project),
            source_id: task.source,
            label: task.label.clone(),
            state: "queued".to_owned(),
            fraction: 0.0,
        }));
        EnvJobsStatus {
            tracks,
            failure: queue.failure.clone(),
            warning: queue.warning.clone(),
        }
    }

    /// Whether anything is running or waiting.
    pub fn busy(&self) -> bool {
        let queue = self.lock();
        !queue.running.is_empty() || !queue.waiting.is_empty()
    }

    fn enqueue(&self, tasks: Vec<Task>) {
        let mut queue = self.lock();
        for task in tasks {
            // Asking again for a track already waiting replaces its request.
            queue
                .waiting
                .retain(|t| t.project != task.project || t.source != task.source);
            queue.waiting.push_back(task);
        }
        queue.failure = None;
        queue.warning = None;
        drop(queue);
        self.wake.notify_all();
    }

    /// Cancels the fetches of `sources`, or every fetch for `None`: queued
    /// ones are dropped, the running one stops at its next chunk and keeps
    /// what it finished.
    pub fn cancel(&self, sources: Option<&[u64]>) {
        self.cancel_boat(None, sources);
    }

    /// Cancel only a named boat's jobs; source ids are local to each boat.
    pub fn cancel_boat(&self, boat: Option<u64>, sources: Option<&[u64]>) {
        let mut queue = self.lock();
        let named = |task: &Task| {
            boat.is_none_or(|id| id == task.project)
                && sources.is_none_or(|s| s.contains(&task.source))
        };
        queue.waiting.retain(|t| !named(t));
        for running in &mut queue.running {
            if named(&running.task) {
                running.cancelled = true;
            }
        }
        if queue.running.iter().all(|r| r.cancelled) {
            queue.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Whirlwind jobs for the same archive share one bounded fleet batch.
    fn take(&self, block: bool) -> Option<(Vec<Task>, Arc<AtomicBool>)> {
        let mut queue = self.lock();
        loop {
            if queue.running.is_empty()
                && let Some(task) = queue.waiting.pop_front()
            {
                let mut tasks = vec![task.clone()];
                let mut kept = VecDeque::new();
                while let Some(next) = queue.waiting.pop_front() {
                    if tasks.len() < 128
                        && next.data_source == task.data_source
                        && next.interval == task.interval
                    {
                        tasks.push(next);
                    } else {
                        kept.push_back(next);
                    }
                }
                queue.waiting = kept;
                queue.cancel = Arc::new(AtomicBool::new(false));
                queue.running = tasks
                    .iter()
                    .cloned()
                    .map(|task| Running {
                        task,
                        fraction: 0.0,
                        cancelled: false,
                    })
                    .collect();
                return Some((tasks, Arc::clone(&queue.cancel)));
            }
            if !block {
                return None;
            }
            queue = self.wake.wait(queue).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn cancelled(&self, task: &Task) -> bool {
        self.lock()
            .running
            .iter()
            .find(|r| r.task.same_track(task))
            .is_none_or(|r| r.cancelled)
    }

    fn advance(&self, task: &Task, fraction: f64) {
        if let Some(r) = self
            .lock()
            .running
            .iter_mut()
            .find(|r| r.task.same_track(task))
        {
            r.fraction = fraction;
        }
    }

    fn warn(&self, label: &str, warnings: std::collections::BTreeSet<String>) {
        if !warnings.is_empty() {
            let text = warnings.into_iter().collect::<Vec<_>>().join("; ");
            self.lock().warning = Some(vec![label.to_owned(), text]);
        }
    }

    fn finish(&self, task: &Task, failure: Option<Vec<String>>) {
        let mut queue = self.lock();
        if !queue.running.iter().any(|r| r.task.same_track(task)) {
            return;
        }
        queue.running.retain(|r| !r.task.same_track(task));
        if failure.is_some() {
            queue.failure = failure;
        }
        if queue.running.iter().all(|r| r.cancelled) {
            queue.cancel.store(true, Ordering::SeqCst);
        }
        self.wake.notify_all();
    }
}

// -------------------------------------------------------------- ingest

/// The version recorded with each archive, until Hindsight Sync supplies
/// its layout version (Task 4).
const ARCHIVE_VERSION: &str = "hindsight-v3";

/// The index of the archive `identity` in a track's dataset list, adding it (with the
/// fetch time) the first time a sample refers to it.
fn record(meta: &mut EnvMeta, identity: &str, now: i64) -> Option<u16> {
    let found = meta
        .datasets
        .iter()
        .position(|d| d.name == identity && d.version == ARCHIVE_VERSION);
    let index = found.unwrap_or_else(|| {
        meta.datasets.push(DatasetRecord {
            name: identity.to_owned(),
            version: ARCHIVE_VERSION.to_owned(),
            fetched_at: now,
            has_tide: Dataset::Hindsight.has_tide(),
        });
        meta.datasets.len() - 1
    });
    u16::try_from(index).ok()
}

/// Writes what was found at one sample, in project units, and relates it
/// to the sample's motion.
/// `identity` names the archive the values came from (`ArchiveRef::identity`).
fn ingest(sample: &mut Sample, meta: &mut EnvMeta, env: &EnvPoint, identity: &str, now: i64) {
    let (tws, twd, wind_ds) = match env.wind {
        Some(w) => {
            let speed = w.u.hypot(w.v);
            // Wind is named for where it comes from: opposite its vector.
            let from = (speed > 1e-9).then(|| compass(-w.u, -w.v));
            (Some(speed * KN_PER_MS), from, record(meta, identity, now))
        }
        None => (None, None, None),
    };
    sample.tws = tws;
    sample.twd_from = twd;
    sample.wind_dataset = wind_ds;
    match env.waves {
        Some(w) => {
            sample.hs_m = w.hs;
            sample.wave_period_s = w.period_s;
            sample.wave_from = w.from;
            sample.wave_dataset = record(meta, identity, now);
        }
        None => {
            sample.hs_m = None;
            sample.wave_period_s = None;
            sample.wave_from = None;
            sample.wave_dataset = None;
        }
    }
    match env.current {
        Some(c) => {
            let speed = c.u.hypot(c.v);
            sample.current_speed = Some(speed * KN_PER_MS);
            // A current is named for where it goes.
            sample.current_toward = Some(compass(c.u, c.v));
            sample.current_dataset = record(meta, identity, now);
        }
        None => {
            sample.current_speed = None;
            sample.current_toward = None;
            sample.current_dataset = None;
        }
    }
    sample.env_fetched = true;
    sample.quantise_env();
    sample.relate();
}

/// The status a track's samples add up to (spec.md 7.1).
fn status_of(samples: &[Sample], failed: bool) -> EnvStatus {
    let fetched = samples.iter().filter(|s| s.env_fetched).count();
    if !samples.is_empty() && fetched == samples.len() {
        EnvStatus::Ready
    } else if fetched > 0 {
        EnvStatus::Partial
    } else if failed {
        EnvStatus::Failed
    } else {
        EnvStatus::NotFetched
    }
}

// ------------------------------------------------------------- running

/// The track's identity when a task started, checked before every write:
/// a result is only ever written into the track it was fetched for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Target {
    project: u64,
    source: u64,
    track: u64,
    samples: usize,
}

/// Runs `f` on the track `target` names, if it is still open and
/// unchanged.
fn with_track<T>(
    state: &AppState,
    target: Target,
    f: impl FnOnce(&mut pe_core::track::Track) -> T,
) -> Result<Option<T>> {
    let scoped = state.scoped(Some(target.project));
    let state = &scoped;
    state
        .with_session(|session| {
            let Some(open) = session.open.as_mut() else {
                return Ok(None);
            };
            if open.project.id.raw() != target.project {
                return Ok(None);
            }
            let Some(track) = open
                .project
                .source_mut(SourceId(target.source))
                .and_then(|s| s.track_mut())
            else {
                return Ok(None);
            };
            if track.id.raw() != target.track || track.samples.len() != target.samples {
                return Ok(None);
            }
            let out = f(track);
            open.touch_samples(target.source);
            Ok(Some(out))
        })
        .or_else(|error| match error {
            AppError::NoProjectOpen => Ok(None),
            other => Err(other),
        })
}

/// How one task ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Every sample has its environment.
    Done,
    /// Cancelled; what finished is kept.
    Cancelled,
    /// A read failed; what finished is kept.
    Failed(String),
    /// The project or track went away; nothing more was written.
    Gone,
}

type Prepared = (Target, Vec<(usize, Point)>, bool);

fn gather(state: &AppState, task: &Task, now: i64) -> Result<Option<Prepared>> {
    let scoped = state.scoped(Some(task.project));
    let state = &scoped;
    // Gather what is still to fetch, starting over when asked or when the
    // interval or the Stokes choice changed: a track is never a mix.
    state
        .with_session(|session| {
            let Some(open) = session.open.as_mut() else {
                return Ok(None);
            };
            if open.project.id.raw() != task.project {
                return Ok(None);
            }
            let stokes = open.project.blend.include_stokes_drift;
            let Some(track) = open
                .project
                .source_mut(SourceId(task.source))
                .and_then(|s| s.track_mut())
            else {
                return Ok(None);
            };
            let seconds = task.interval.seconds();
            let meta = &track.env_meta;
            let identity = task.data_source.archive().identity;
            let source_changed = different_source(meta, &identity);
            let restart = task.restart
                || source_changed
                || meta.interval_s.is_some_and(|s| s != seconds)
                || meta.stokes_drift.is_some_and(|s| s != stokes);
            let changed =
                restart || meta.interval_s != Some(seconds) || meta.stokes_drift != Some(stokes);
            if restart {
                // Nothing of the earlier fetch survives: not a value, not a
                // dataset record (with its fetch time), so a cancel part way
                // through leaves a track that is partly fetched, never mixed.
                for sample in &mut track.samples {
                    sample.clear_env();
                }
                track.env_meta.datasets.clear();
                track.env_meta.status = EnvStatus::NotFetched;
            }
            track.env_meta.interval_s = Some(seconds);
            track.env_meta.stokes_drift = Some(stokes);
            // Keep source identity even when every requested cell is missing.
            record(&mut track.env_meta, &identity, now);
            let todo: Vec<(usize, Point)> = track
                .samples
                .iter()
                .enumerate()
                .filter(|(_, s)| !s.env_fetched)
                .map(|(k, s)| {
                    (
                        k,
                        Point {
                            t: s.t,
                            lat: s.lat,
                            lon: s.lon,
                        },
                    )
                })
                .collect();
            let target = Target {
                project: task.project,
                source: task.source,
                track: track.id.raw(),
                samples: track.samples.len(),
            };
            if changed {
                open.touch_samples(task.source);
            }
            Ok(Some((target, todo, stokes)))
        })
        .or_else(|error| match error {
            AppError::NoProjectOpen => Ok(None),
            other => Err(other),
        })
}

/// Runs one independently requested track.
fn run(
    state: &AppState,
    task: &Task,
    provider: &dyn Provider,
    sink: &dyn JobSink,
    jobs: &EnvJobs,
    cancel: &Arc<AtomicBool>,
    now: i64,
) -> Result<Outcome> {
    let gathered = gather(state, task, now)?;
    let Some((target, todo, stokes)) = gathered else {
        return Ok(Outcome::Gone);
    };
    let options = Options {
        interval: task.interval,
        stokes_drift: stokes,
        parts: pe_env::Parts::ALL,
    };
    let identity = task.data_source.archive().identity;
    let total = todo.len().max(1);
    let mut done = 0;
    let mut outcome = Outcome::Done;
    let mut warnings = std::collections::BTreeSet::new();
    // The heads of every chunk the track needs, side by side, before the
    // batches read their blocks (M14e).
    let all: Vec<Point> = todo.iter().map(|(_, p)| *p).collect();
    if let Err(error) = provider.prepare(&all, &options, cancel) {
        outcome = if cancel.load(Ordering::SeqCst) || matches!(error, EnvError::Cancelled) {
            Outcome::Cancelled
        } else {
            Outcome::Failed(error.to_string())
        };
    }
    for batch in batches_with_limit(&todo, BATCH_SPAN_S, BATCH_SAMPLES) {
        if outcome != Outcome::Done {
            break;
        }
        if cancel.load(Ordering::SeqCst) {
            outcome = Outcome::Cancelled;
            break;
        }
        let points: Vec<Point> = batch.iter().map(|(_, p)| *p).collect();
        let found = match provider.sample(&points, &options, cancel) {
            Ok(found) => found,
            Err(EnvError::Cancelled) => {
                outcome = Outcome::Cancelled;
                break;
            }
            Err(_) if cancel.load(Ordering::SeqCst) => {
                outcome = Outcome::Cancelled;
                break;
            }
            Err(err) => {
                outcome = Outcome::Failed(err.to_string());
                break;
            }
        };
        warnings.extend(provider.take_warnings());
        let written = with_track(state, target, |track| {
            for ((k, _), env) in batch.iter().zip(&found) {
                if let Some(sample) = track.samples.get_mut(*k) {
                    ingest(sample, &mut track.env_meta, env, &identity, now);
                }
            }
            track.env_meta.status = status_of(&track.samples, false);
        })?;
        if written.is_none() {
            outcome = Outcome::Gone;
            break;
        }
        done += batch.len();
        jobs.advance(task, done as f64 / total as f64);
        sink.changed();
        sink.progress(&jobs.status());
    }
    if outcome != Outcome::Gone {
        let failed = matches!(outcome, Outcome::Failed(_));
        with_track(state, target, |track| {
            track.env_meta.status = status_of(&track.samples, failed);
        })?;
        sink.changed();
    }
    jobs.warn(&task.label, warnings);
    Ok(outcome)
}

/// Consecutive runs of `todo` (in time order, as samples are) covering at
/// most `span_s` of track time and `limit` samples each.
fn batches_with_limit(
    todo: &[(usize, Point)],
    span_s: i64,
    limit: usize,
) -> Vec<&[(usize, Point)]> {
    let mut out = Vec::new();
    let mut start = 0;
    for k in 1..=todo.len() {
        let split =
            k == todo.len() || k - start >= limit || todo[k].1.t - todo[start].1.t >= span_s;
        if split {
            out.push(&todo[start..k]);
            start = k;
        }
    }
    out
}

/// Runs the next queued task, if there is one; returns its outcome.
/// `block` waits for one to be queued.
pub fn run_next(
    state: &AppState,
    provider: &dyn Provider,
    sink: &dyn JobSink,
    now: i64,
    block: bool,
) -> Option<Outcome> {
    let jobs = &state.env_jobs;
    let (tasks, cancel) = jobs.take(block)?;
    Some(run_tasks(state, &tasks, provider, sink, &cancel, now))
}

fn run_tasks(
    state: &AppState,
    tasks: &[Task],
    provider: &dyn Provider,
    sink: &dyn JobSink,
    cancel: &Arc<AtomicBool>,
    now: i64,
) -> Outcome {
    let jobs = &state.env_jobs;
    sink.progress(&jobs.status());
    let outcome = if tasks.len() > 1 {
        fleet::run(state, tasks, provider, sink, cancel, now)
    } else {
        run(state, &tasks[0], provider, sink, jobs, cancel, now)
    }
    .unwrap_or_else(|err| Outcome::Failed(err.to_string()));
    for task in tasks {
        let failure = match &outcome {
            Outcome::Failed(message) => Some(vec![task.label.clone(), message.clone()]),
            _ => None,
        };
        jobs.finish(task, failure);
    }
    sink.progress(&jobs.status());
    outcome
}

// ----------------------------------------------------------- provider

/// What the provider was built from; a settings change builds a new one.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderKey {
    data_source: DataSource,
    weather_cache: crate::weather_cache::WeatherCacheSettings,
    memory_bytes: u64,
    timeout_s: u64,
    concurrency: u32,
}

/// The reanalysis provider for the current settings, kept while they do not
/// change so the opened archives are reused across tracks and jobs.
pub fn provider(state: &AppState) -> Result<Arc<Reanalysis>> {
    let source = state.with_session(|s| Ok(s.settings.data_source))?;
    provider_for_source(state, source)
}

fn provider_for_source(state: &AppState, data_source: DataSource) -> Result<Arc<Reanalysis>> {
    let key = state.with_session(|session| {
        let s = &session.settings;
        Ok(ProviderKey {
            data_source,
            weather_cache: s.weather_cache.clone(),
            memory_bytes: s.weather_memory_bytes(),
            timeout_s: u64::from(s.network.timeout_s),
            concurrency: s.network.concurrency,
        })
    })?;
    let mut slot = state
        .env_provider
        .lock()
        .map_err(|_| AppError::Internal("the reanalysis provider lock was poisoned".to_owned()))?;
    if let Some((k, p)) = slot.as_ref()
        && *k == key
    {
        return Ok(Arc::clone(p));
    }
    let timeout = std::time::Duration::from_secs(key.timeout_s);
    let memory = pe_env::BlockCache::new(key.memory_bytes);
    let reanalysis = Arc::new(
        Reanalysis::open(key.data_source.archive(), timeout, memory)
            .map_err(|e| AppError::Internal(e.to_string()))?
            .with_whirlwind_cache(crate::weather_cache::open(state, &key.weather_cache)?),
    );
    *slot = Some((key, Arc::clone(&reanalysis)));
    Ok(reanalysis)
}

/// The frontend, as a [`JobSink`].
struct Frontend(tauri::AppHandle);

impl JobSink for Frontend {
    fn progress(&self, status: &EnvJobsStatus) {
        use tauri::Emitter;
        let _ = self.0.emit(PROGRESS_EVENT, status);
    }
    fn changed(&self) {
        use tauri::Emitter;
        let _ = self.0.emit(CHANGED_EVENT, ());
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Starts the runner thread for the life of the process.
pub fn start(app: tauri::AppHandle) {
    let _ = std::thread::Builder::new()
        .name("environment".to_owned())
        .spawn(move || {
            use tauri::Manager;
            let sink = Frontend(app.clone());
            loop {
                let state = app.state::<AppState>();
                // Wait for work before building the provider: nothing is
                // opened before someone asks for weather.
                let (tasks, cancel) = match state.env_jobs.take(true) {
                    Some(next) => next,
                    None => continue,
                };
                match provider_for_source(&state, tasks[0].data_source) {
                    Ok(provider) => {
                        run_tasks(&state, &tasks, provider.as_ref(), &sink, &cancel, now());
                    }
                    Err(err) => {
                        for task in &tasks {
                            state
                                .env_jobs
                                .finish(task, Some(vec![task.label.clone(), err.to_string()]));
                        }
                        sink.progress(&state.env_jobs.status());
                    }
                }
            }
        });
}

// ------------------------------------------------------------------ IPC

/// The chosen interval by name.
fn interval(name: &str) -> Result<Interval> {
    match name {
        "hourly" => Ok(Interval::Hourly),
        other => Err(AppError::BadOption {
            field: "Sampling interval",
            value: other.to_owned(),
        }),
    }
}

/// What [`positions`] finds.
struct Gathered {
    project: u64,
    labels: Vec<(u64, String)>,
    points: Vec<Point>,
}

/// Whether a track's weather came from any archive but `identity`: then a
/// fetch starts it over, so a track never mixes archives.
fn different_source(meta: &EnvMeta, identity: &str) -> bool {
    meta.datasets.is_empty() || meta.datasets.iter().any(|d| d.name != identity)
}

/// The positions of the named tracks still to fetch (all of them when
/// `restart`), each track's label, and the open project's id.
fn positions(state: &AppState, sources: &[u64], restart: bool) -> Result<Gathered> {
    state.with_session(|session| {
        let identity = session.settings.data_source.archive().identity;
        let open = session.require_open()?;
        open.ensure_editable()?;
        let mut labels = Vec::new();
        let mut points = Vec::new();
        for &id in sources {
            let source = open
                .project
                .source(SourceId(id))
                .ok_or(AppError::Core(pe_core::CoreError::MissingSource(id)))?;
            let track = source.track().ok_or_else(|| AppError::BadOption {
                field: "Track",
                value: source.label.clone(),
            })?;
            let source_changed = different_source(&track.env_meta, &identity);
            let interval_changed = track
                .env_meta
                .interval_s
                .is_some_and(|s| s != Interval::Hourly.seconds());
            labels.push((id, source.label.clone()));
            points.extend(
                track
                    .samples
                    .iter()
                    .filter(|s| restart || !s.env_fetched || source_changed || interval_changed)
                    .map(|s| Point {
                        t: s.t,
                        lat: s.lat,
                        lon: s.lon,
                    }),
            );
        }
        Ok(Gathered {
            project: open.project.id.raw(),
            labels,
            points,
        })
    })
}

/// What a fetch would download, before it starts (spec.md 13, D19, D27).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "EnvEstimate.ts")]
pub struct EnvEstimate {
    /// Samples to fetch.
    pub samples: u32,
    /// Bytes to download sampling hourly.
    pub hourly_bytes: u64,
    /// Bytes of the hourly fetch already downloaded this session.
    pub cached_bytes: u64,
    /// About how much the project file grows by: the values stored per
    /// sample, compressed.
    pub stored_bytes: u64,
}

/// About what one fetched sample adds to the project file, bytes: its wind,
/// waves and current at their stored precision, by column and deflated
/// (measured on real tracks, M14e: see plan.md).
pub const STORED_BYTES_PER_SAMPLE: u64 = 8;

/// [`env_estimate`] without a Tauri handle, through the session's
/// provider (what it already holds is not counted).
pub fn estimate_for(state: &AppState, sources: &[u64], restart: bool) -> Result<EnvEstimate> {
    let points = positions(state, sources, restart)?.points;
    let e = provider(state)?
        .estimate(&points)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(EnvEstimate {
        samples: u32::try_from(points.len()).unwrap_or(u32::MAX),
        hourly_bytes: e.hourly_bytes,
        cached_bytes: e.hourly_cached_bytes,
        stored_bytes: points.len() as u64 * STORED_BYTES_PER_SAMPLE,
    })
}

/// The expected download for fetching the named tracks' environment.
#[tauri::command]
pub async fn env_estimate(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    source_ids: Vec<u64>,
    restart: bool,
) -> Result<EnvEstimate> {
    let state = state.scoped(boat_context);
    // Tauri's `command(async)` dispatches even a synchronous function on an
    // async worker. Whirlwind drives its own runtime while opening metadata
    // and reading indexes: doing that there panics, leaving the IPC promise
    // unanswered and the busy indicator running. Provider replacement also
    // drops that runtime, so keep the entire operation on a blocking worker.
    tauri::async_runtime::spawn_blocking(move || estimate_for(&state, &source_ids, restart))
        .await
        .map_err(|e| AppError::Internal(format!("Weather estimate worker failed: {e}")))?
}

/// [`start_env_fetch`] without a Tauri handle: queues one task per track.
pub fn queue_fetch(
    state: &AppState,
    sources: &[u64],
    interval_name: &str,
    restart: bool,
) -> Result<EnvJobsStatus> {
    let interval = interval(interval_name)?;
    let data_source = state.with_session(|s| Ok(s.settings.data_source))?;
    let Gathered {
        project, labels, ..
    } = positions(state, sources, restart)?;
    state.env_jobs.enqueue(
        labels
            .into_iter()
            .map(|(source, label)| Task {
                data_source,
                project,
                source,
                label,
                interval,
                restart,
            })
            .collect(),
    );
    Ok(state.env_jobs.status())
}

/// Fetches the environment of the named tracks: what is missing, or all
/// of it again with `restart`. Only `"hourly"` sampling is accepted.
///
/// Generic over the Tauri runtime so the MCP service's tools, and their
/// tests on a mock application, call this very command.
#[tauri::command]
pub fn start_env_fetch<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    source_ids: Vec<u64>,
    interval: String,
    restart: bool,
) -> Result<EnvJobsStatus> {
    let state = state.scoped(boat_context);
    let status = queue_fetch(&state, &source_ids, &interval, restart)?;
    use tauri::Emitter;
    let _ = app.emit(PROGRESS_EVENT, &status);
    Ok(status)
}

/// Cancels the fetches of the named tracks, or all of them for null.
/// Generic over the Tauri runtime, as [`start_env_fetch`].
#[tauri::command]
pub fn cancel_env_fetch<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    source_ids: Option<Vec<u64>>,
) -> Result<EnvJobsStatus> {
    let state = state.scoped(boat_context);
    state
        .env_jobs
        .cancel_boat(boat_context, source_ids.as_deref());
    let status = state.env_jobs.status();
    use tauri::Emitter;
    let _ = app.emit(PROGRESS_EVENT, &status);
    Ok(status)
}

/// The job queue now (for a frontend that just loaded).
#[tauri::command]
pub fn env_jobs(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
) -> Result<EnvJobsStatus> {
    let state = state.scoped(boat_context);
    Ok(state.env_jobs.status())
}

/// Chooses whether the polar uses current-corrected values (undoable).
#[tauri::command]
pub fn set_use_corrected(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    on: bool,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    use_corrected_set(&state, on)
}

/// [`set_use_corrected`] without a Tauri handle.
pub fn use_corrected_set(state: &AppState, on: bool) -> Result<ProjectSummary> {
    crate::edit::apply(state, |project| {
        let before = project.blend.use_corrected;
        Ok((before != on).then_some(Command::SetUseCorrected { before, after: on }))
    })
}

/// Chooses whether the global merged current includes Stokes drift
/// (undoable; applies to the next fetch). No control sets it any more
/// (removed at the user's request 2026-10-02, and a loaded project has it
/// off): the fetch's own tests drive it.
pub fn stokes_drift_set(state: &AppState, on: bool) -> Result<ProjectSummary> {
    crate::edit::apply(state, |project| {
        let before = project.blend.include_stokes_drift;
        Ok((before != on).then_some(Command::SetStokesDrift { before, after: on }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t: i64) -> (usize, Point) {
        (
            0,
            Point {
                t,
                lat: 0.0,
                lon: 0.0,
            },
        )
    }

    #[test]
    fn batches_split_by_span_and_count() {
        let todo: Vec<_> = (0..10).map(|k| at(k * 3600)).collect();
        let split = |todo: &[(usize, Point)], span: i64| -> Vec<usize> {
            batches_with_limit(todo, span, 400)
                .iter()
                .map(|b| b.len())
                .collect()
        };
        assert_eq!(split(&todo, 3 * 3600), vec![3, 3, 3, 1]);
        let dense: Vec<_> = (0..1000).map(at).collect();
        assert_eq!(split(&dense, 3 * 3600), vec![400, 400, 200]);
        assert!(split(&[], 3600).is_empty());
    }

    /// Hand-computed: u = −3, v = −4 m/s is 5 m/s blowing toward the
    /// south-west, so from 36.87° (north-east); 5 m/s = 9.7192 kn. A
    /// current of u = 1, v = 0 m/s sets toward 090° at 1.9438 kn. Each is
    /// kept to 0.01 kn or 0.1°.
    #[test]
    fn ingest_converts_units_and_senses() {
        let fix = pe_core::track::Fix {
            tws: None,
            twd_from: None,
            t: 0,
            lat: 0.0,
            lon: 0.0,
            cog: None,
            sog: None,
        };
        const IDENTITY: &str = "s3://whirlwind-hindsight/hindsight-routing";
        let mut sample = Sample::at(pe_core::SampleId(1), 0, &fix);
        let mut meta = EnvMeta::default();
        let env = EnvPoint {
            wind: Some(pe_env::Vector {
                u: -3.0,
                v: -4.0,
                dataset: Dataset::Hindsight,
            }),
            waves: None,
            current: Some(pe_env::Vector {
                u: 1.0,
                v: 0.0,
                dataset: Dataset::Hindsight,
            }),
        };
        ingest(&mut sample, &mut meta, &env, IDENTITY, 42);
        // Kept at the stored precision (D27): 9.72 kn, 36.9°, 1.94 kn.
        assert_eq!(sample.tws, Some(9.72));
        assert_eq!(sample.twd_from, Some(36.9));
        assert_eq!(sample.current_toward, Some(90.0));
        assert_eq!(sample.current_speed, Some(1.94));
        assert_eq!(sample.wind_dataset, Some(0));
        // One archive, one record, whatever part a value is.
        assert_eq!(sample.current_dataset, Some(0));
        assert!(sample.env_fetched);
        assert_eq!(
            meta.datasets[0].has_tide,
            Some(true),
            "the Hindsight current includes the tide"
        );
        assert_eq!(meta.datasets[0].name, IDENTITY);
        assert_eq!(meta.datasets[0].version, "hindsight-v3");
        assert_eq!(meta.datasets[0].fetched_at, 42);
        // The same archive again is the same record.
        ingest(&mut sample, &mut meta, &env, IDENTITY, 99);
        assert_eq!(meta.datasets.len(), 1);
        assert_eq!(meta.datasets[0].fetched_at, 42);
    }
}
