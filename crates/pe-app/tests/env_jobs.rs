#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The environment job end to end (spec.md 7.5, 7.7), against a fake
//! provider and without the network: import queues nothing by itself, a
//! queued fetch fills every sample, a cancelled one keeps what finished and
//! resumes to exactly the uninterrupted result, a failed one is marked, a
//! removed track is left alone, and a derivation change relates the new
//! headings to the stored wind without fetching again.

mod common;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::env::{self, EnvJobsStatus, JobSink, Outcome};
use pe_app::tracks::{self, TrackFileRequest};
use pe_app::{edit, projects};
use pe_core::canonical::{env_degrees, env_knots};
use pe_core::track::{EnvStatus, Track};
use pe_env::dataset::Dataset;
use pe_env::whirlwind::testing;
use pe_env::{EnvError, EnvPoint, Options, Point, Provider, Vector, Waves};

/// 2025-07-26T12:00Z.
const NOON: i64 = 1_753_531_200;
/// A fixed clock, so the recorded fetch times of two runs compare equal.
const NOW: i64 = 1_760_000_000;

/// Exercise Tauri's async dispatch, not a direct synchronous command call.
/// A newline in the fake key fails header validation before any HTTP request.
#[test]
fn whirlwind_estimate_ipc_returns_errors_and_can_retry_without_hanging() {
    use pe_app::settings::DataSource;
    use std::time::Duration;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    let root = TempRoot::new("whirlwind-estimate-ipc");
    let (state, id) = imported(&root);
    state
        .with_session(|s| {
            s.settings.data_source = DataSource::Whirlwind;
            Ok(())
        })
        .unwrap();
    let fake =
        serde_json::json!({"access_key_id":"TEST\nINVALID", "secret_access_key":"test-secret"});
    std::fs::write(
        state.paths.config_dir.join("whirlwind-credentials.json"),
        fake.to_string(),
    )
    .unwrap();
    // The public S3 provider must initialise even with an invalid legacy file.
    // Inject invalid signing credentials explicitly to fail before HTTP.
    env::provider(&state).unwrap();
    let mut archive = DataSource::Whirlwind.archive();
    if let pe_env::whirlwind::ArchiveLocation::S3 { credentials, .. } = &mut archive.location {
        *credentials = Some(serde_json::from_value(fake).unwrap());
    }
    state.env_provider.lock().unwrap().as_mut().unwrap().1 = Arc::new(
        pe_env::Reanalysis::open(
            archive,
            Duration::from_secs(1),
            pe_env::BlockCache::new(16 << 20),
        )
        .unwrap(),
    );
    let app = mock_builder()
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![env::env_estimate])
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let invoke = || {
        let window = window.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let response = tauri::test::get_ipc_response(
                &window,
                tauri::webview::InvokeRequest {
                    cmd: "env_estimate".into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: if cfg!(windows) {
                        "http://tauri.localhost"
                    } else {
                        "tauri://localhost"
                    }
                    .parse()
                    .unwrap(),
                    body: tauri::ipc::InvokeBody::Json(
                        serde_json::json!({"boatContext":null,"sourceIds":[id],"restart":false}),
                    ),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.to_owned(),
                },
            );
            let _ = tx.send(response);
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("weather estimate must settle its IPC promise")
    };
    for _ in 0..2 {
        let error = invoke().unwrap_err();
        assert_eq!(error["kind"], "internal");
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("invalid Whirlwind credentials"),
            "{error}"
        );
    }
    assert!(state.env_jobs.status().tracks.is_empty());
}

/// One boat sailing north up 5°W from 49.5°N, a position every two hours
/// for five days: 61 positions, two 72-hour batches (36 and 25).
fn track_file(root: &TempRoot) -> String {
    track_file_at(root, NOON, 49.5, -5.0, 60, 7200)
}

/// A boat sailing north up `lon` from `lat` at 0.06° an hour (3.6 kn), a
/// position every `step` seconds from `t0`: `last + 1` positions.
fn track_file_at(root: &TempRoot, t0: i64, lat: f64, lon: f64, last: i64, step: i64) -> String {
    let features: Vec<String> = (0..=last)
        .map(|k| {
            format!(
                r#"{{"type":"Feature","geometry":{{"type":"Point","coordinates":[{lon},{}]}},"properties":{{"time":{},"boat":"Alpha"}}}}"#,
                lat + 0.06 * (step * k) as f64 / 3600.0,
                t0 + step * k
            )
        })
        .collect();
    let path = root.file("race.geojson");
    std::fs::write(
        &path,
        format!(
            r#"{{"type":"FeatureCollection","features":[{}]}}"#,
            features.join(",")
        ),
    )
    .unwrap();
    path
}

/// A project with the track imported; returns the track's source id.
fn imported(root: &TempRoot) -> (AppState, u64) {
    imported_from(root, track_file(root))
}

/// A project with the track at `path` imported; returns its source id.
fn imported_from(root: &TempRoot, path: String) -> (AppState, u64) {
    let app = root.state();
    projects::create(&app, "Env".to_owned(), None, false).unwrap();
    let result = tracks::import(
        &app,
        &[TrackFileRequest {
            path,
            mapping: None,
            boats: None,
        }],
    )
    .unwrap();
    let id = result.imported[0].source_id;
    (app, id)
}

/// Points the session's provider at `pe-env`'s fixture archive on a
/// loopback stub, so an estimate reads its indexes there.
fn stub_archive(app: &AppState) -> testing::Server {
    let server = testing::serve(testing::fixture(), 200);
    env::provider(app).unwrap();
    app.env_provider.lock().unwrap().as_mut().unwrap().1 =
        Arc::new(testing::reader_at(server.endpoint()));
    server
}

/// Values that depend only on a position's time and place, as the real
/// archive's do; the current differs north and south of 49.8°N.
fn truth(p: &Point) -> EnvPoint {
    let hours = (p.t - NOON) as f64 / 3600.0;
    EnvPoint {
        wind: Some(Vector {
            u: 5.0 + 0.1 * hours,
            v: -3.0 + 0.01 * p.lat,
            dataset: Dataset::Hindsight,
        }),
        waves: Some(Waves {
            period_s: Some(8.5),
            hs: Some(1.0 + 0.01 * p.lon),
            from: Some((p.t / 60).rem_euclid(360) as f64),
            dataset: Dataset::Hindsight,
        }),
        current: Some(if p.lat > 49.8 {
            Vector {
                u: 0.2,
                v: 0.1,
                dataset: Dataset::Hindsight,
            }
        } else {
            Vector {
                u: 0.1,
                v: -0.1,
                dataset: Dataset::Hindsight,
            }
        }),
    }
}

/// The fake archive. `cancel_on` makes the user press Cancel while the
/// call with that number is reading; `fail_on` makes that call's read
/// fail.
struct Fake<'a> {
    calls: AtomicUsize,
    points: AtomicUsize,
    app: Option<&'a AppState>,
    cancel_on: Option<usize>,
    fail_on: Option<usize>,
    /// Said once, as a real provider says a tier would not open.
    warning: Mutex<Option<String>>,
}

impl<'a> Fake<'a> {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            points: AtomicUsize::new(0),
            app: None,
            cancel_on: None,
            fail_on: None,
            warning: Mutex::new(None),
        }
    }
}

impl Provider for Fake<'_> {
    fn sample(
        &self,
        points: &[Point],
        options: &Options,
        cancel: &Arc<AtomicBool>,
    ) -> pe_env::Result<Vec<EnvPoint>> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if Some(call) == self.cancel_on
            && let Some(app) = self.app
        {
            // The user cancels mid-read; the reader sees the flag at its
            // next chunk.
            app.env_jobs.cancel(None);
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(EnvError::Cancelled);
        }
        if Some(call) == self.fail_on {
            return Err(EnvError::OutOfRange("the archive answered 500".to_owned()));
        }
        self.points.fetch_add(points.len(), Ordering::SeqCst);
        // Stokes drift adds to the current, as it does in the real store.
        let stokes = if options.stokes_drift { 1.0 } else { 0.0 };
        Ok(points
            .iter()
            .map(|p| {
                let mut env = truth(p);
                if let Some(c) = &mut env.current {
                    c.u += stokes;
                }
                env
            })
            .collect())
    }

    fn take_warnings(&self) -> Vec<String> {
        self.warning.lock().unwrap().take().into_iter().collect()
    }
}

/// Records what the frontend would be sent.
#[derive(Default)]
struct Sink {
    progress: Mutex<Vec<EnvJobsStatus>>,
    changed: AtomicUsize,
}

impl JobSink for Sink {
    fn progress(&self, status: &EnvJobsStatus) {
        self.progress.lock().unwrap().push(status.clone());
    }
    fn changed(&self) {
        self.changed.fetch_add(1, Ordering::SeqCst);
    }
}

/// Runs the queue dry.
fn drain(app: &AppState, provider: &dyn Provider, sink: &Sink) -> Vec<Outcome> {
    drain_at(app, provider, sink, NOW)
}

/// Runs the queue dry with the clock at `now`.
fn drain_at(app: &AppState, provider: &dyn Provider, sink: &Sink, now: i64) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    while let Some(outcome) = env::run_next(app, provider, sink, now, false) {
        outcomes.push(outcome);
    }
    outcomes
}

fn track(app: &AppState, id: u64) -> Track {
    app.with_session(|session| {
        let open = session.require_open()?;
        Ok(open
            .project
            .source(pe_core::SourceId(id))
            .unwrap()
            .track()
            .unwrap()
            .clone())
    })
    .unwrap()
}

#[test]
fn a_fetch_fills_every_sample_in_project_units() {
    let root = TempRoot::new("env-fill");
    let (app, id) = imported(&root);
    assert_eq!(track(&app, id).env_meta.status, EnvStatus::NotFetched);
    let queued = env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    assert_eq!(queued.tracks.len(), 1);
    assert_eq!(queued.tracks[0].state, "queued");

    let sink = Sink::default();
    let fake = Fake::new();
    assert_eq!(drain(&app, &fake, &sink), vec![Outcome::Done]);
    let t = track(&app, id);
    assert_eq!(t.env_meta.status, EnvStatus::Ready);
    assert_eq!(t.env_meta.interval_s, Some(3600));
    assert!(t.samples.iter().all(|s| s.env_fetched));
    // Five days in 72-hour batches: two calls, every sample once.
    assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
    assert_eq!(fake.points.load(Ordering::SeqCst), 61);
    // Dataset name and version per sample (invariant 3).
    let names: Vec<(&str, Option<bool>)> = t
        .env_meta
        .datasets
        .iter()
        .map(|d| (d.name.as_str(), d.has_tide))
        .collect();
    // One record for the archive every part came from.
    assert_eq!(
        names,
        [("s3://whirlwind-hindsight/hindsight-routing", Some(true))]
    );
    assert!(t.env_meta.datasets.iter().all(|d| d.fetched_at == NOW));
    let first = &t.samples[0];
    assert_eq!(first.wind_dataset, Some(0));
    assert_eq!(first.current_dataset, Some(0));
    // u = 5, v = -3 + 0.495 m/s: speed √(u² + v²) × 3600/1852 kn, kept
    // to 0.01 kn (D27).
    let (u, v) = (5.0f64, -3.0 + 0.01 * 49.5);
    assert_eq!(first.tws.unwrap(), env_knots(u.hypot(v) * 3600.0 / 1852.0));
    // The wind blows toward the east-south-east, so it comes from the
    // west-north-west; kept to 0.1°.
    let from = first.twd_from.unwrap();
    assert_eq!(
        from,
        env_degrees(270.0 + (v.abs() / u).atan().to_degrees()),
        "{from}"
    );
    // Related to the motion: the boat heads north, so TWA is the angle to it.
    assert!((first.twa.unwrap() - (360.0 - from)).abs() < 1e-9);
    assert!(first.bsp_corrected.is_some(), "a current was found");
    // Progress reached the frontend, and so did each written batch.
    assert_eq!(sink.changed.load(Ordering::SeqCst), 3);
    let last = sink.progress.lock().unwrap().last().cloned().unwrap();
    assert!(last.tracks.is_empty());
    // With wind, the samples have their place in the polar.
    let summary = projects::summary(&app).unwrap().unwrap();
    let alpha = summary.sources[0].track.clone().unwrap();
    assert_eq!(alpha.env_status, "ready");
    assert_eq!(alpha.env_fetched, 61);
    assert_eq!(alpha.with_wind, 61);
}

/// Runs a fetch on a fresh import, cancelling while the second batch reads
/// when `cancel`; returns the application and the track's id.
fn fetched(root: &TempRoot, cancel: bool) -> (AppState, u64) {
    let (app, id) = imported(root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let outcome = {
        let fake = Fake {
            app: Some(&app),
            cancel_on: cancel.then_some(2),
            ..Fake::new()
        };
        drain(&app, &fake, &Sink::default())
    };
    let expected = if cancel {
        Outcome::Cancelled
    } else {
        Outcome::Done
    };
    assert_eq!(outcome, vec![expected]);
    (app, id)
}

/// Two JSON values equal, numbers to within `tol`.
fn same_json(a: &serde_json::Value, b: &serde_json::Value, tol: f64) -> bool {
    use serde_json::Value;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() <= tol
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same_json(x, y, tol))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| same_json(v, w, tol)))
        }
        _ => a == b,
    }
}

/// The M9 acceptance: a cancelled job leaves a consistent project that
/// resumes to the same result as an uninterrupted one.
#[test]
fn a_cancelled_fetch_resumes_to_the_uninterrupted_result() {
    let root_a = TempRoot::new("env-straight");
    let (a, id_a) = fetched(&root_a, false);
    let straight = track(&a, id_a);

    // Cancelled while reading its second batch.
    let root_b = TempRoot::new("env-resumed");
    let (b, id_b) = fetched(&root_b, true);
    let partial = track(&b, id_b);
    assert_eq!(partial.env_meta.status, EnvStatus::Partial);
    // The first batch (0–70 h, 36 samples) finished; nothing after it.
    assert_eq!(partial.samples.iter().filter(|s| s.env_fetched).count(), 36);
    assert!(
        partial.samples[36..]
            .iter()
            .all(|s| !s.env_fetched && s.tws.is_none())
    );
    // What finished is exactly what the uninterrupted run found.
    assert_eq!(partial.samples[..36], straight.samples[..36]);

    // Refetch resumes the rest, and only the rest, to exactly the same
    // project data.
    env::queue_fetch(&b, &[id_b], "hourly", false).unwrap();
    let resuming = Fake::new();
    assert_eq!(drain(&b, &resuming, &Sink::default()), vec![Outcome::Done]);
    assert_eq!(
        resuming.points.load(Ordering::SeqCst),
        61 - 36,
        "only the rest"
    );
    let resumed = track(&b, id_b);
    assert_eq!(resumed.samples, straight.samples);
    assert_eq!(resumed.env_meta, straight.env_meta);
}

/// The cancelled project is consistent on disk: saved and reopened it is
/// partial, and a Refetch after reopening completes it. The file holds
/// every value to its canonical precision (knots to 1e-6, degrees to
/// 1e-9), so the result matches the uninterrupted run to that precision.
#[test]
fn a_cancelled_fetch_saves_as_partial_and_resumes_after_reopening() {
    let root_a = TempRoot::new("env-straight-disk");
    let (a, id_a) = fetched(&root_a, false);
    let root_b = TempRoot::new("env-resumed-disk");
    let (b, id_b) = fetched(&root_b, true);
    let path = root_b.file("b.wpsproj");
    projects::save_as(&b, path.clone()).unwrap();
    projects::open(&b, path, false).unwrap();
    let reopened = track(&b, id_b);
    assert_eq!(reopened.env_meta.status, EnvStatus::Partial);
    assert_eq!(
        reopened.samples.iter().filter(|s| s.env_fetched).count(),
        36
    );
    let summary = projects::summary(&b).unwrap().unwrap();
    assert_eq!(
        summary.sources[0].track.as_ref().unwrap().env_status,
        "partial"
    );

    env::queue_fetch(&b, &[id_b], "hourly", false).unwrap();
    assert_eq!(
        drain(&b, &Fake::new(), &Sink::default()),
        vec![Outcome::Done]
    );
    let (ta, tb) = (track(&a, id_a), track(&b, id_b));
    assert_eq!(tb.env_meta, ta.env_meta);
    let json = |t: &Track| serde_json::to_value(&t.samples).unwrap();
    assert!(same_json(&json(&ta), &json(&tb), 1e-5));
}

#[test]
fn a_failed_fetch_keeps_what_finished_and_says_so() {
    let root = TempRoot::new("env-fail");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let failing = Fake {
        fail_on: Some(2),
        ..Fake::new()
    };
    let sink = Sink::default();
    let outcomes = drain(&app, &failing, &sink);
    assert!(matches!(&outcomes[..], [Outcome::Failed(m)] if m.contains("500")));
    assert_eq!(track(&app, id).env_meta.status, EnvStatus::Partial);
    let status = app.env_jobs.status();
    assert_eq!(status.failure.as_ref().unwrap()[0], "Alpha");

    // Failing at once leaves nothing: "failed".
    let root = TempRoot::new("env-fail-first");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let failing = Fake {
        fail_on: Some(1),
        ..Fake::new()
    };
    drain(&app, &failing, &Sink::default());
    assert_eq!(track(&app, id).env_meta.status, EnvStatus::Failed);
}

/// Undoing the import while its fetch waits leaves nothing to write into;
/// the fetch ends without touching the project.
#[test]
fn a_track_removed_under_its_fetch_is_left_alone() {
    let root = TempRoot::new("env-gone");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    edit::undo_last(&app).unwrap();
    let fake = Fake::new();
    assert_eq!(drain(&app, &fake, &Sink::default()), vec![Outcome::Gone]);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    // Redo brings the track back as it was: not fetched.
    edit::redo_next(&app).unwrap();
    assert_eq!(track(&app, id).env_meta.status, EnvStatus::NotFetched);
}

/// Refetch of a ready track starts over; a derivation
/// change afterwards relates the new headings to the stored wind without
/// asking the archive again (M8 carry).
#[test]
fn refetch_starts_over_and_rederiving_needs_no_fetch() {
    let root = TempRoot::new("env-refetch");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    drain(&app, &Fake::new(), &Sink::default());
    let coarse = Fake::new();
    env::queue_fetch(&app, &[id], "hourly", true).unwrap();
    assert_eq!(drain(&app, &coarse, &Sink::default()), vec![Outcome::Done]);
    assert_eq!(
        coarse.points.load(Ordering::SeqCst),
        61,
        "every sample again"
    );
    assert_eq!(track(&app, id).env_meta.interval_s, Some(3600));

    let before = track(&app, id);
    let calls = coarse.calls.load(Ordering::SeqCst);
    tracks::track_derivation_set(&app, id, 300, "derived", "derived").unwrap();
    let after = track(&app, id);
    assert_eq!(
        coarse.calls.load(Ordering::SeqCst),
        calls,
        "nothing fetched"
    );
    // A 5-minute gap leaves the 10-minute fixes with no derived heading, so
    // no TWA; the stored wind is untouched.
    assert!(
        after
            .samples
            .iter()
            .all(|s| s.heading.is_none() && s.twa.is_none())
    );
    assert!(
        after
            .samples
            .iter()
            .zip(&before.samples)
            .all(|(a, b)| a.tws == b.tws)
    );
    edit::undo_last(&app).unwrap();
    assert_eq!(track(&app, id).samples, before.samples);
}

/// Replacing the project cancels its queued fetches (spec.md 3.3).
#[test]
fn replacing_the_project_cancels_its_jobs() {
    let root = TempRoot::new("env-replace");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    assert!(app.env_jobs.busy());
    projects::create(&app, "Other".to_owned(), None, true).unwrap();
    assert!(!app.env_jobs.busy());
    assert_eq!(drain(&app, &Fake::new(), &Sink::default()), vec![]);
}

/// Hand-counted: 13 positions from 2000-01-01T00Z to 02Z at 45N 80W
/// (the fixture archive's first hours) need the stencil rows 60N–30N and
/// columns 90W–45W, one inner chunk of each of the two time shards. Each
/// inner chunk is 2 hours × 7 parameters × 2 × 2 float16 values (112
/// bytes) stored as one raw Zstandard block, 12 bytes of framing: 124.
#[test]
fn estimates_hourly_download_and_rejects_other_intervals() {
    let root = TempRoot::new("env-estimate");
    let (app, id) = imported_from(
        &root,
        track_file_at(&root, 946_684_800, 45.0, -80.0, 12, 600),
    );
    let _server = stub_archive(&app);
    let e = env::estimate_for(&app, &[id], false).unwrap();
    assert_eq!(e.samples, 13);
    assert_eq!(e.hourly_bytes, 2 * 124);
    assert_eq!(e.cached_bytes, 0);
    assert!(env::queue_fetch(&app, &[id], "three_hourly", false).is_err());
    assert!(app.env_jobs.status().tracks.is_empty());
    // What the project grows by: bytes per sample, not megabytes.
    assert_eq!(e.stored_bytes, 13 * env::STORED_BYTES_PER_SAMPLE);
    assert!(env::queue_fetch(&app, &[id], "weekly", false).is_err());
}

/// Old projects keep their weather until asked to fetch; resuming a coarse
/// partial fetch then replaces it completely at hourly resolution.
#[test]
fn a_legacy_three_hourly_track_is_estimated_and_refetched_hourly() {
    let root = TempRoot::new("env-legacy-interval");
    let (app, id) = fetched(&root, false);
    app.with_session(|s| {
        let track = s
            .open
            .as_mut()
            .unwrap()
            .project
            .source_mut(pe_core::SourceId(id))
            .unwrap()
            .track_mut()
            .unwrap();
        track.env_meta.interval_s = Some(10_800);
        track.env_meta.status = EnvStatus::Partial;
        track.samples[0].clear_env();
        Ok(())
    })
    .unwrap();
    let _server = stub_archive(&app);
    assert_eq!(env::estimate_for(&app, &[id], false).unwrap().samples, 61);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let fake = Fake::new();
    assert_eq!(drain(&app, &fake, &Sink::default()), vec![Outcome::Done]);
    assert_eq!(fake.points.load(Ordering::SeqCst), 61);
    assert_eq!(track(&app, id).env_meta.interval_s, Some(3600));
}

/// Correcting for current and Stokes drift are project settings, each one
/// undo.
#[test]
fn current_correction_and_stokes_drift_are_undoable_settings() {
    let root = TempRoot::new("env-toggles");
    let (app, _) = imported(&root);
    let summary = projects::summary(&app).unwrap().unwrap();
    assert!(summary.use_corrected && !summary.stokes_drift);
    let off = env::use_corrected_set(&app, false).unwrap();
    assert!(!off.use_corrected);
    assert_eq!(off.undo_label.as_deref(), Some("Change current correction"));
    let on = env::stokes_drift_set(&app, true).unwrap();
    assert!(on.stokes_drift);
    edit::undo_last(&app).unwrap();
    let back = edit::undo_last(&app).unwrap();
    assert!(back.use_corrected && !back.stokes_drift);
}

/// Toggling Stokes drift between a cancelled fetch and its Refetch starts
/// over, so no track mixes currents with and without it, and the track
/// records which it has (review round 1).
#[test]
fn a_stokes_change_between_cancel_and_refetch_starts_over() {
    let root = TempRoot::new("env-stokes");
    let (app, id) = fetched(&root, true);
    assert_eq!(track(&app, id).env_meta.stokes_drift, Some(false));
    env::stokes_drift_set(&app, true).unwrap();
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let again = Fake::new();
    assert_eq!(drain(&app, &again, &Sink::default()), vec![Outcome::Done]);
    assert_eq!(
        again.points.load(Ordering::SeqCst),
        61,
        "every sample again"
    );
    let t = track(&app, id);
    assert_eq!(t.env_meta.stokes_drift, Some(true));
    // Every current carries the drift: none is left from the first fetch.
    let expected = |s: &pe_core::track::Sample| {
        let c = truth(&Point {
            t: s.t,
            lat: s.lat,
            lon: s.lon,
        })
        .current
        .unwrap();
        env_knots((c.u + 1.0).hypot(c.v) * 3600.0 / 1852.0)
    };
    assert!(
        t.samples
            .iter()
            .all(|s| s.current_speed.unwrap() == expected(s))
    );
}

/// A restart that is cancelled leaves a track partly fetched at the new
/// settings and otherwise empty: no value, dataset record or fetch time of
/// the earlier fetch survives beside the new ones (review round 1).
#[test]
fn a_cancelled_restart_never_mixes_two_fetches() {
    let root = TempRoot::new("env-restart-cancel");
    let (app, id) = fetched(&root, false);
    const LATER: i64 = NOW + 86_400;
    env::queue_fetch(&app, &[id], "hourly", true).unwrap();
    let cancelling = Fake {
        app: Some(&app),
        cancel_on: Some(2),
        ..Fake::new()
    };
    assert_eq!(
        drain_at(&app, &cancelling, &Sink::default(), LATER),
        vec![Outcome::Cancelled]
    );
    let t = track(&app, id);
    assert_eq!(t.env_meta.status, EnvStatus::Partial);
    assert_eq!(t.env_meta.interval_s, Some(3600));
    let (fetched, rest): (Vec<_>, Vec<_>) = t.samples.iter().partition(|s| s.env_fetched);
    assert!(!fetched.is_empty() && !rest.is_empty());
    for s in &rest {
        assert_eq!(
            (
                s.tws,
                s.twa,
                s.hs_m,
                s.current_speed,
                s.bsp_corrected,
                s.wind_dataset,
                s.current_dataset
            ),
            (None, None, None, None, None, None, None)
        );
    }
    // Only the new fetch's records, each one used.
    assert!(t.env_meta.datasets.iter().all(|d| d.fetched_at == LATER));
    let used: std::collections::BTreeSet<u16> = fetched
        .iter()
        .flat_map(|s| [s.wind_dataset, s.wave_dataset, s.current_dataset])
        .flatten()
        .collect();
    assert_eq!(used.len(), t.env_meta.datasets.len());
}

/// A provider's warning (a current source left out) reaches the job's
/// status with the track's name, and the fetch still completes.
#[test]
fn a_provider_warning_reaches_the_status() {
    let root = TempRoot::new("env-warning");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let warning = Fake::new();
    *warning.warning.lock().unwrap() = Some("the current source x could not be opened".to_owned());
    assert_eq!(drain(&app, &warning, &Sink::default()), vec![Outcome::Done]);
    let status = app.env_jobs.status();
    assert_eq!(
        status.warning,
        Some(vec![
            "Alpha".to_owned(),
            "the current source x could not be opened".to_owned()
        ])
    );
    assert!(status.failure.is_none());
}

#[test]
fn boat_jobs_with_equal_source_ids_are_independent() {
    let root = TempRoot::new("boat-env-isolation");
    let (app, id) = imported(&root);
    let first = projects::summary(&app).unwrap().unwrap().id;
    let second = pe_app::boats::add(&app, "Sister ship".into()).unwrap().id;
    let child = app.scoped(Some(second));
    let imported = tracks::import(
        &child,
        &[TrackFileRequest {
            path: track_file(&root),
            mapping: None,
            boats: None,
        }],
    )
    .unwrap();
    assert_eq!(id, imported.imported[0].source_id);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let queued = env::queue_fetch(&child, &[id], "hourly", false).unwrap();
    assert_eq!(queued.tracks.len(), 2);
    assert_eq!(
        queued.tracks.iter().map(|j| j.boat_id).collect::<Vec<_>>(),
        [Some(first), Some(second)]
    );
    app.env_jobs.cancel_boat(Some(first), Some(&[id]));
    let outcomes = drain(&app, &Fake::new(), &Sink::default());
    assert!(outcomes.contains(&Outcome::Done));
    assert!(!track(&app, id).samples.iter().any(|s| s.env_fetched));
    assert!(track(&child, id).samples.iter().all(|s| s.env_fetched));
}

#[test]
fn changing_data_source_replaces_existing_values_and_keeps_queued_choice() {
    use pe_app::settings::{self, DataSource};
    struct Whirlwind(Dataset);
    impl Provider for Whirlwind {
        fn sample(
            &self,
            points: &[Point],
            _: &Options,
            _: &Arc<AtomicBool>,
        ) -> pe_env::Result<Vec<EnvPoint>> {
            Ok(points
                .iter()
                .map(|_| EnvPoint {
                    wind: Some(Vector {
                        u: 12.,
                        v: 3.,
                        dataset: self.0,
                    }),
                    ..EnvPoint::default()
                })
                .collect())
        }
    }
    let root = TempRoot::new("source-switch");
    let (app, id) = imported(&root);
    let sink = Sink::default();
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    assert_eq!(
        drain(&app, &Whirlwind(Dataset::Hindsight), &sink),
        vec![Outcome::Done]
    );
    // Existing values are from S3, so switching to R2 must replace even a
    // ready track without requiring an explicit restart.
    settings::data_source_set(&app, DataSource::WhirlwindR2).unwrap();
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    // A preference change after queueing must not change that queued request.
    settings::data_source_set(&app, DataSource::Whirlwind).unwrap();
    assert_eq!(
        drain(&app, &Whirlwind(Dataset::Hindsight), &sink),
        vec![Outcome::Done]
    );
    let fetched = track(&app, id);
    assert!(
        fetched
            .env_meta
            .datasets
            .iter()
            .all(|d| d.name == "r2://whirlwind-hindsight/hindsight-routing")
    );
    assert!(fetched.samples.iter().all(|s| s.env_fetched));
    for (source, identity) in [
        (
            DataSource::WhirlwindR2,
            "r2://whirlwind-hindsight/hindsight-routing",
        ),
        (
            DataSource::WhirlwindTigris,
            "tigris://whirlwind-hindsight/hindsight-routing",
        ),
        (
            DataSource::Whirlwind,
            "s3://whirlwind-hindsight/hindsight-routing",
        ),
    ] {
        settings::data_source_set(&app, source).unwrap();
        // A ready route must be fetched again when its storage source changes.
        assert!(
            !env::queue_fetch(&app, &[id], "hourly", false)
                .unwrap()
                .tracks
                .is_empty()
        );
        assert_eq!(
            drain(&app, &Whirlwind(Dataset::Hindsight), &sink),
            vec![Outcome::Done]
        );
        assert!(
            track(&app, id)
                .env_meta
                .datasets
                .iter()
                .all(|d| d.name == identity)
        );
    }
}

#[test]
fn a_prepare_failure_is_reported_as_failure_not_cancellation() {
    struct Broken;
    impl Provider for Broken {
        fn sample(
            &self,
            _: &[Point],
            _: &Options,
            _: &Arc<AtomicBool>,
        ) -> pe_env::Result<Vec<EnvPoint>> {
            panic!("a failed prepare must not sample");
        }
        fn prepare(&self, _: &[Point], _: &Options, _: &Arc<AtomicBool>) -> pe_env::Result<()> {
            Err(EnvError::Open("S3 answered 403".into()))
        }
    }
    let root = TempRoot::new("prepare-failure");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    assert!(
        matches!(drain(&app,&Broken,&Sink::default()).as_slice(),[Outcome::Failed(message)] if message.contains("403"))
    );
    assert_eq!(track(&app, id).env_meta.status, EnvStatus::Failed);
}

fn imported_fleet(root: &TempRoot, boats: usize, samples: usize) -> (AppState, Vec<u64>) {
    imported_fleet_dates(root, boats, samples, 0)
}
fn imported_fleet_dates(
    root: &TempRoot,
    boats: usize,
    samples: usize,
    date_spacing: i64,
) -> (AppState, Vec<u64>) {
    let app = root.state();
    projects::create(&app, "Fleet weather".into(), None, false).unwrap();
    app.with_session(|s| {
        s.settings.data_source = pe_app::settings::DataSource::WhirlwindR2;
        Ok(())
    })
    .unwrap();
    let features: Vec<_> = (0..boats).flat_map(|boat| (0..samples).map(move |k| {
        serde_json::json!({"type":"Feature", "geometry":{"type":"Point","coordinates":[-5.0, 50.0 + boat as f64 * 0.001]},
            "properties":{"time":NOON + k as i64 * 600 + date_spacing * boat as i64, "boat":format!("Fleet {boat}")}})
    })).collect();
    let path = root.file("fleet.geojson");
    std::fs::write(
        &path,
        serde_json::json!({"type":"FeatureCollection","features":features}).to_string(),
    )
    .unwrap();
    let result = tracks::import(
        &app,
        &[TrackFileRequest {
            path,
            mapping: None,
            boats: None,
        }],
    )
    .unwrap();
    let ids = result.imported.iter().map(|s| s.source_id).collect();
    (app, ids)
}

struct FleetProvider<'a> {
    app: &'a AppState,
    calls: Mutex<Vec<Vec<Point>>>,
    cancel_source: Option<u64>,
    cancel_on: Option<usize>,
    fail_north: bool,
}
impl<'a> FleetProvider<'a> {
    fn new(app: &'a AppState) -> Self {
        Self {
            app,
            calls: Mutex::new(Vec::new()),
            cancel_source: None,
            cancel_on: None,
            fail_north: false,
        }
    }
}
impl Provider for FleetProvider<'_> {
    fn sample(
        &self,
        points: &[Point],
        options: &Options,
        cancel: &Arc<AtomicBool>,
    ) -> pe_env::Result<Vec<EnvPoint>> {
        let call = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(points.to_vec());
            calls.len()
        };
        assert_eq!(options.interval, pe_env::Interval::Hourly);
        assert!(points.len() <= 10_000);
        assert!(points.windows(2).all(|p| p[0].t <= p[1].t));
        for lat in points
            .iter()
            .map(|p| p.lat.to_bits())
            .collect::<std::collections::BTreeSet<_>>()
        {
            let same_route: Vec<_> = points.iter().filter(|p| p.lat.to_bits() == lat).collect();
            assert!(same_route.last().unwrap().t - same_route[0].t < 72 * 3600);
        }
        if call == 1
            && let Some(id) = self.cancel_source
        {
            self.app.env_jobs.cancel(Some(&[id]));
        }
        if self.cancel_on == Some(call) {
            self.app.env_jobs.cancel(None);
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(EnvError::Cancelled);
        }
        if self.fail_north && points.iter().any(|p| p.lat > 50.0015) {
            return Err(EnvError::OutOfRange("broken northern chunk".into()));
        }
        Ok(points.iter().map(truth).collect())
    }
}

#[test]
fn whirlwind_fleet_combines_tracks_and_scatter_preserves_each_samples_weather() {
    let root = TempRoot::new("env-fleet");
    let (app, ids) = imported_fleet(&root, 20, 61);
    env::queue_fetch(&app, &ids, "hourly", false).unwrap();
    let provider = FleetProvider::new(&app);
    let sink = Sink::default();
    assert_eq!(drain(&app, &provider, &sink), [Outcome::Done]);
    assert_eq!(provider.calls.lock().unwrap().len(), 1);
    assert_eq!(provider.calls.lock().unwrap()[0].len(), 20 * 61);
    assert!(
        sink.progress.lock().unwrap().iter().any(|s| s
            .tracks
            .iter()
            .filter(|t| t.state == "fetching")
            .count()
            == 20)
    );
    assert!(
        sink.changed.load(Ordering::SeqCst) <= 2,
        "updates are per batch, not per boat"
    );
    for id in ids {
        let track = track(&app, id);
        assert_eq!(track.env_meta.status, EnvStatus::Ready);
        assert_eq!(track.env_meta.datasets.len(), 1);
        for sample in track.samples {
            let expected = truth(&Point {
                t: sample.t,
                lat: sample.lat,
                lon: sample.lon,
            })
            .wind
            .unwrap();
            assert_eq!(
                sample.tws,
                Some(env_knots(expected.u.hypot(expected.v) * env::KN_PER_MS))
            );
            assert!(sample.env_fetched);
        }
    }
    assert!(!app.env_jobs.busy());
}

#[test]
fn cancelling_one_fleet_track_does_not_cancel_shared_downloads_for_the_others() {
    let root = TempRoot::new("env-fleet-cancel-one");
    let (app, ids) = imported_fleet(&root, 3, 61);
    env::queue_fetch(&app, &ids, "hourly", false).unwrap();
    let provider = FleetProvider {
        cancel_source: Some(ids[0]),
        ..FleetProvider::new(&app)
    };
    assert_eq!(
        drain(&app, &provider, &Sink::default()),
        [Outcome::Cancelled]
    );
    assert_eq!(track(&app, ids[0]).env_meta.status, EnvStatus::NotFetched);
    assert!(track(&app, ids[0]).samples.iter().all(|s| !s.env_fetched));
    for id in &ids[1..] {
        assert_eq!(track(&app, *id).env_meta.status, EnvStatus::Ready);
    }
    assert!(!app.env_jobs.busy());
}

#[test]
fn cancelled_fleet_keeps_completed_batches_and_resumes_only_missing_samples() {
    let root = TempRoot::new("env-fleet-resume");
    let (app, ids) = imported_fleet(&root, 3, 500);
    env::queue_fetch(&app, &ids, "hourly", false).unwrap();
    let provider = FleetProvider {
        cancel_on: Some(2),
        ..FleetProvider::new(&app)
    };
    assert_eq!(
        drain(&app, &provider, &Sink::default()),
        [Outcome::Cancelled]
    );
    for id in &ids {
        let t = track(&app, *id);
        assert_eq!(t.env_meta.status, EnvStatus::Partial);
        assert_eq!(t.samples.iter().filter(|s| s.env_fetched).count(), 432);
    }
    env::queue_fetch(&app, &ids, "hourly", false).unwrap();
    let provider = FleetProvider::new(&app);
    assert_eq!(drain(&app, &provider, &Sink::default()), [Outcome::Done]);
    assert_eq!(
        provider
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(Vec::len)
            .sum::<usize>(),
        3 * 68
    );
    for id in ids {
        assert_eq!(track(&app, id).env_meta.status, EnvStatus::Ready);
    }
}

#[test]
fn a_bad_chunk_fails_only_its_fleet_track_and_other_tracks_finish() {
    let root = TempRoot::new("env-fleet-isolate-failure");
    let (app, ids) = imported_fleet(&root, 3, 61);
    env::queue_fetch(&app, &ids, "hourly", false).unwrap();
    let provider = FleetProvider {
        fail_north: true,
        ..FleetProvider::new(&app)
    };
    assert!(
        matches!(&drain(&app, &provider, &Sink::default())[..], [Outcome::Failed(message)] if message.contains("broken northern chunk"))
    );
    assert_eq!(provider.calls.lock().unwrap().len(), 4);
    for id in &ids[..2] {
        assert_eq!(track(&app, *id).env_meta.status, EnvStatus::Ready);
    }
    assert_eq!(track(&app, ids[2]).env_meta.status, EnvStatus::Failed);
    assert_eq!(app.env_jobs.status().failure.unwrap()[0], "Fleet 2");
    assert!(!app.env_jobs.busy());
}

#[test]
fn fleet_batch_sample_cap_is_shared_across_all_tracks() {
    let root = TempRoot::new("env-fleet-bounded");
    let (app, ids) = imported_fleet(&root, 30, 400);
    env::queue_fetch(&app, &ids, "hourly", false).unwrap();
    let provider = FleetProvider::new(&app);
    assert_eq!(drain(&app, &provider, &Sink::default()), [Outcome::Done]);
    assert_eq!(
        provider
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        [10_000, 2_000]
    );
    for id in ids {
        assert_eq!(track(&app, id).env_meta.status, EnvStatus::Ready);
    }
}

#[test]
fn fleet_keeps_equal_source_ids_in_different_boat_tabs_separate() {
    let root = TempRoot::new("env-fleet-tabs");
    let (app, id) = imported(&root);
    app.with_session(|s| {
        s.settings.data_source = pe_app::settings::DataSource::WhirlwindR2;
        Ok(())
    })
    .unwrap();
    let first = projects::summary(&app).unwrap().unwrap().id;
    let second = pe_app::boats::add(&app, "Sister ship".into()).unwrap().id;
    let child = app.scoped(Some(second));
    let added = tracks::import(
        &child,
        &[TrackFileRequest {
            path: track_file(&root),
            mapping: None,
            boats: None,
        }],
    )
    .unwrap();
    assert_eq!(id, added.imported[0].source_id);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    env::queue_fetch(&child, &[id], "hourly", false).unwrap();
    struct CancelBoat<'a> {
        first: u64,
        source: u64,
        inner: FleetProvider<'a>,
    }
    impl Provider for CancelBoat<'_> {
        fn sample(
            &self,
            points: &[Point],
            options: &Options,
            cancel: &Arc<AtomicBool>,
        ) -> pe_env::Result<Vec<EnvPoint>> {
            self.inner
                .app
                .env_jobs
                .cancel_boat(Some(self.first), Some(&[self.source]));
            self.inner.sample(points, options, cancel)
        }
    }
    let provider = CancelBoat {
        first,
        source: id,
        inner: FleetProvider::new(&app),
    };
    assert_eq!(
        drain(&app, &provider, &Sink::default()),
        [Outcome::Cancelled]
    );
    // The first shared batch: each boat's first 72 hours, 36 samples.
    assert_eq!(provider.inner.calls.lock().unwrap()[0].len(), 72);
    assert!(track(&app, id).samples.iter().all(|s| !s.env_fetched));
    assert_eq!(track(&child, id).env_meta.status, EnvStatus::Ready);
    assert!(app.env_jobs.status().failure.is_none());
}

#[test]
fn fleet_tracks_from_different_race_dates_share_one_download_pipeline() {
    let root = TempRoot::new("env-fleet-dates");
    let (app, ids) = imported_fleet_dates(&root, 3, 61, 365 * 86400);
    env::queue_fetch(&app, &ids, "hourly", false).unwrap();
    let provider = FleetProvider::new(&app);
    assert_eq!(drain(&app, &provider, &Sink::default()), [Outcome::Done]);
    let calls = provider.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].len(), 183);
    assert!(calls[0].last().unwrap().t - calls[0][0].t > 2 * 365 * 86400);
    for id in ids {
        assert_eq!(track(&app, id).env_meta.status, EnvStatus::Ready);
    }
}

#[test]
fn a_shared_archive_error_fails_the_fleet_without_repeating_it_for_each_boat() {
    let root = TempRoot::new("env-fleet-archive-error");
    let (app, ids) = imported_fleet(&root, 20, 61);
    env::queue_fetch(&app, &ids, "hourly", false).unwrap();
    struct Forbidden(AtomicUsize);
    impl Provider for Forbidden {
        fn sample(
            &self,
            _: &[Point],
            _: &Options,
            _: &Arc<AtomicBool>,
        ) -> pe_env::Result<Vec<EnvPoint>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(EnvError::Open("Whirlwind S3 answered 403".into()))
        }
    }
    let provider = Forbidden(AtomicUsize::new(0));
    assert!(matches!(
        &drain(&app, &provider, &Sink::default())[..],
        [Outcome::Failed(_)]
    ));
    assert_eq!(provider.0.load(Ordering::SeqCst), 1);
    for id in ids {
        assert_eq!(track(&app, id).env_meta.status, EnvStatus::Failed);
    }
    assert!(!app.env_jobs.busy());
}
