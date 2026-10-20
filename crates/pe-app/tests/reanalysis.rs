#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The reanalysis archive chosen among Hindsight Sync's (spec.md 3.4, 7.5):
//! probing and scanning its loopback API, choosing an archive, keeping its
//! location (credentials included) out of the settings and out of IPC, and
//! fetching from it whether Hindsight Sync is running or not. Hindsight
//! Sync is a stub on loopback serving recorded listings.

mod common;

use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use common::{Stub, TempRoot, stub};
use pe_app::commands::AppState;
use pe_app::env::{self, EnvJobsStatus, JobSink, Outcome};
use pe_app::error::AppError;
use pe_app::reanalysis::{self, ArchiveStorage};
use pe_app::settings::{self, Settings};
use pe_app::{projects, tracks};
use pe_core::track::DatasetRecord;
use pe_env::dataset::Dataset;
use pe_env::{EnvPoint, Options, Point, Provider, Vector};
use serde_json::{Value, json};

/// 2025-07-26T12:00Z.
const NOON: i64 = 1_753_531_200;
const NOW: i64 = 1_760_000_000;
const NO_ARCHIVE: &str = "Choose a reanalysis archive in Settings before fetching weather.";
const LOCATION_MISSING: &str =
    "The remembered archive's location is missing. Press Scan in Settings.";
const HINT: &str = "Press Scan in Settings to refresh the archive's credentials.";

/// A port nothing listens on.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn keys(secret: &str) -> Value {
    json!({"access_key_id": "AKIDTESTKEY", "secret_access_key": secret, "session_token": null})
}

fn cloud(id: &str, location: Value) -> Value {
    json!({"id": id, "name": format!("{id} archive"), "kind": "reanalysis", "storage": "cloud",
        "uri": format!("r2://{id}/routing"), "mode": "global",
        "start_datetime": "2000-01-01T00:00:00Z", "stop_datetime": "2025-12-31T23:00:00Z",
        "chunk_degrees": 5, "chunk_hours": 72, "products": ["wind10m", "waves"],
        "parameters": [], "layout_version": 3, "location": location})
}

/// A path-style S3 location at `endpoint`.
fn s3(endpoint: &str, credentials: Value) -> Value {
    json!({"protocol": "s3", "provider": "r2", "bucket": "whirlwind-hindsight",
        "prefix": "hindsight-routing", "endpoint_url": endpoint, "region": "auto",
        "force_path_style": true, "credentials": credentials})
}

/// Hindsight Sync's two answers: an R2 archive, one whose credentials are
/// missing, one in `errors`, and a local copy of a cloud archive.
fn listing(secret: &str) -> Vec<(&'static str, u16, String)> {
    let mut no_key = s3("https://acct.r2.cloudflarestorage.invalid", Value::Null);
    no_key["credential_error"] = json!("no key for this bucket");
    let cloud_body = json!({"api_version": 1,
        "archives": [
            cloud("r2", s3("https://acct.r2.cloudflarestorage.invalid", keys(secret))),
            cloud("nokey", no_key)],
        "errors": [{"id": "broken", "storage": "cloud", "error": "bad metadata"}]});
    let mut local = cloud("copy", json!({"protocol": "file", "path": "/data/copy"}));
    local["storage"] = json!("local");
    local["uri"] = json!("file:///data/copy");
    local["source_uri"] = json!("s3://weather/source");
    local["stop_datetime"] = Value::Null;
    let local_body = json!({"api_version": 1, "archives": [local], "errors": []});
    vec![
        ("/v1/archives/cloud/reanalysis", 200, cloud_body.to_string()),
        ("/v1/archives/local/reanalysis", 200, local_body.to_string()),
    ]
}

/// An application whose saved port is the stub's, after one probe.
fn probed(root: &TempRoot, server: &Stub) -> AppState {
    let app = root.state();
    reanalysis::port_set(&app, server.port).unwrap();
    app
}

fn settings_text(app: &AppState) -> String {
    std::fs::read_to_string(app.paths.settings_file()).unwrap()
}

fn location_text(app: &AppState) -> String {
    std::fs::read_to_string(reanalysis::location_path(&app.paths)).unwrap()
}

/// A project with one 61-position track; returns its source id.
fn with_track(root: &TempRoot, app: &AppState) -> u64 {
    let features: Vec<Value> = (0..=60)
        .map(|k| {
            json!({"type": "Feature",
                "geometry": {"type": "Point", "coordinates": [-5.0, 49.5 + 0.12 * f64::from(k)]},
                "properties": {"time": NOON + 7200 * i64::from(k), "boat": "Alpha"}})
        })
        .collect();
    let path = root.file("race.geojson");
    std::fs::write(
        &path,
        json!({"type": "FeatureCollection", "features": features}).to_string(),
    )
    .unwrap();
    projects::create(app, "Weather".to_owned(), None, false).unwrap();
    tracks::import(
        app,
        &[tracks::TrackFileRequest {
            path,
            mapping: None,
            boats: None,
        }],
    )
    .unwrap()
    .imported[0]
        .source_id
}

/// A constant wind everywhere.
struct Fake;

impl Provider for Fake {
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
                    u: 5.0,
                    v: 2.0,
                    dataset: Dataset::Hindsight,
                }),
                ..EnvPoint::default()
            })
            .collect())
    }
}

struct Quiet;

impl JobSink for Quiet {
    fn progress(&self, _: &EnvJobsStatus) {}
    fn changed(&self) {}
}

fn drain(app: &AppState, provider: &dyn Provider) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    while let Some(outcome) = env::run_next(app, provider, &Quiet, NOW, false) {
        outcomes.push(outcome);
    }
    outcomes
}

fn datasets(app: &AppState, id: u64) -> Vec<DatasetRecord> {
    app.with_session(|session| {
        Ok(session
            .require_open()?
            .project
            .source(pe_core::SourceId(id))
            .unwrap()
            .track()
            .unwrap()
            .env_meta
            .datasets
            .clone())
    })
    .unwrap()
}

#[test]
fn a_probe_lists_archives_marks_connected_and_remembers_the_port() {
    let root = TempRoot::new("reanalysis-probe");
    let server = stub(listing("first-secret"));
    let app = root.state();
    assert!(!settings::current(&app).unwrap().reanalysis.connected_once);
    reanalysis::port_set(&app, server.port).unwrap();
    let found = reanalysis::probe_now(&app).unwrap();
    assert!(found.detected);
    assert_eq!(found.port, server.port);
    let ids: Vec<_> = found.archives.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, ["r2", "copy"], "cloud first, then this computer's");
    assert_eq!(found.archives[0].storage, ArchiveStorage::Cloud);
    assert_eq!(found.archives[0].place, "acct.r2.cloudflarestorage.invalid");
    assert_eq!(found.archives[1].storage, ArchiveStorage::Local);
    assert_eq!(found.archives[1].place, "/data/copy");
    let unavailable: Vec<_> = found
        .unavailable
        .iter()
        .map(|u| (u.id.as_str(), u.reason.as_str()))
        .collect();
    assert_eq!(
        unavailable,
        [
            ("nokey", "no key for this bucket"),
            ("broken", "bad metadata")
        ]
    );
    // Saved: what a restart reads.
    let saved = Settings::load(&app.paths.settings_file()).reanalysis;
    assert!(saved.connected_once);
    assert_eq!(saved.port, server.port);
    assert_eq!(saved.archive, None, "a probe chooses nothing");
}

#[test]
fn choosing_writes_display_details_to_settings_and_the_location_to_its_own_file() {
    let root = TempRoot::new("reanalysis-choose");
    let server = stub(listing("first-secret"));
    let app = probed(&root, &server);
    let chosen = reanalysis::archive_set(&app, Some("r2".to_owned())).unwrap();
    let archive = chosen.reanalysis.archive.clone().unwrap();
    assert_eq!(archive.id, "r2");
    assert_eq!(archive.name, "r2 archive");
    assert_eq!(archive.place, "acct.r2.cloudflarestorage.invalid");
    assert_eq!((archive.chunk_degrees, archive.chunk_hours), (5, 72));
    assert_eq!(archive.products, ["wind10m", "waves"]);
    assert_eq!(archive.stop.as_deref(), Some("2025-12-31T23:00:00Z"));

    let saved = settings_text(&app);
    for wanted in [
        r#""id": "r2""#,
        r#""name": "r2 archive""#,
        r#""place": "acct.r2.cloudflarestorage.invalid""#,
        r#""chunk_degrees": 5"#,
        r#""chunk_hours": 72"#,
    ] {
        assert!(saved.contains(wanted), "{wanted} in {saved}");
    }
    for secret in [
        "access_key_id",
        "secret_access_key",
        "AKIDTESTKEY",
        "first-secret",
    ] {
        assert!(!saved.contains(secret), "{secret} in settings.json");
    }
    let location = location_text(&app);
    for wanted in [
        "access_key_id",
        "secret_access_key",
        "first-secret",
        r#""id": "r2""#,
    ] {
        assert!(location.contains(wanted), "{wanted} in {location}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(reanalysis::location_path(&app.paths))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    // A restart reads the same choice.
    assert_eq!(settings::current(&root.state()).unwrap(), chosen);
    assert_eq!(
        reanalysis::captured(&root.state()).unwrap().identity(),
        "r2://r2/routing"
    );

    // Clearing the choice removes the location with it.
    let cleared = reanalysis::archive_set(&app, None).unwrap();
    assert_eq!(cleared.reanalysis.archive, None);
    assert!(!reanalysis::location_path(&app.paths).exists());
}

#[test]
fn an_unlisted_or_credential_less_archive_is_refused() {
    let root = TempRoot::new("reanalysis-refused");
    // Before any probe there is nothing to choose from.
    let fresh = root.state();
    let before = reanalysis::archive_set(&fresh, Some("r2".to_owned())).unwrap_err();
    assert!(matches!(
        before,
        AppError::BadOption {
            field: "Reanalysis archive",
            ..
        }
    ));

    let server = stub(listing("first-secret"));
    let app = probed(&root, &server);
    let ghost = reanalysis::archive_set(&app, Some("ghost".to_owned())).unwrap_err();
    assert_eq!(ghost.kind(), "bad-option");
    assert_eq!(
        ghost.to_string(),
        r#"Reanalysis archive cannot be "ghost"."#
    );
    let nokey = reanalysis::archive_set(&app, Some("nokey".to_owned())).unwrap_err();
    assert_eq!(nokey.kind(), "doing");
    assert_eq!(
        nokey.to_string(),
        "Could not choose nokey archive: no key for this bucket"
    );
    let broken = reanalysis::archive_set(&app, Some("broken".to_owned())).unwrap_err();
    assert_eq!(broken.to_string(), "Could not choose broken: bad metadata");
    // Nothing was chosen or written.
    assert_eq!(settings::current(&app).unwrap().reanalysis.archive, None);
    assert!(!reanalysis::location_path(&app.paths).exists());
}

#[test]
fn a_later_probe_refreshes_the_chosen_locations_credentials() {
    let root = TempRoot::new("reanalysis-rotate");
    let server = stub(listing("first-secret"));
    let app = probed(&root, &server);
    reanalysis::archive_set(&app, Some("r2".to_owned())).unwrap();
    assert!(location_text(&app).contains("first-secret"));
    // Hindsight Sync rotates the key.
    *server.routes.lock().unwrap() = listing("second-secret");
    assert!(reanalysis::probe_now(&app).unwrap().detected);
    let location = location_text(&app);
    assert!(location.contains("second-secret"), "{location}");
    assert!(!location.contains("first-secret"), "{location}");
    assert_eq!(
        reanalysis::captured(&app).unwrap().archive.id,
        "r2",
        "still the same choice"
    );
}

#[test]
fn scan_tries_the_saved_port_then_the_default_then_the_sibling() {
    let root = TempRoot::new("reanalysis-scan");
    // On the saved port something answers, but not Hindsight Sync.
    let stranger = stub(vec![]);
    let app = probed(&root, &stranger);
    assert!(!settings::current(&app).unwrap().reanalysis.connected_once);
    let server = stub(listing("first-secret"));
    let found = reanalysis::scan_with(&app, Some(server.port)).unwrap();
    if found.port == pe_env::hindsight::DEFAULT_PORT {
        // A real Hindsight Sync on this computer answered on its default
        // port, which comes before the sibling's: the order held.
        assert!(found.detected);
        return;
    }
    assert!(found.detected);
    assert_eq!(found.port, server.port);
    assert_eq!(found.archives.len(), 2);
    assert!(
        stranger
            .seen
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.ends_with("/cloud/reanalysis")),
        "the saved port was asked first"
    );
    let saved = Settings::load(&app.paths.settings_file()).reanalysis;
    assert_eq!(saved.port, server.port, "the port that answered is saved");
    assert!(saved.connected_once);

    // Nothing anywhere: not detected, the saved port kept, and the
    // archives last heard still listed.
    let gone = closed_port();
    reanalysis::port_set(&app, gone).unwrap();
    let none = reanalysis::scan_with(&app, None).unwrap();
    assert!(!none.detected);
    assert_eq!(none.port, gone);
    assert_eq!(none.archives.len(), 2);
    assert_eq!(settings::current(&app).unwrap().reanalysis.port, gone);
}

#[test]
fn not_running_keeps_the_remembered_archive_usable() {
    let root = TempRoot::new("reanalysis-not-running");
    let server = stub(listing("first-secret"));
    let app = probed(&root, &server);
    reanalysis::archive_set(&app, Some("r2".to_owned())).unwrap();
    // Hindsight Sync quits; the person points the port at nothing.
    let gone = closed_port();
    let saved = reanalysis::port_set(&app, gone).unwrap();
    assert_eq!(saved.reanalysis.port, gone);
    let found = reanalysis::probe_now(&app).unwrap();
    assert!(!found.detected);
    assert_eq!(found.port, gone);
    let captured = reanalysis::captured(&app).unwrap();
    assert_eq!(captured.identity(), "r2://r2/routing");
    assert_eq!(captured.version(), "hindsight-layout-3");
    // A restart, with Hindsight Sync still not running, reads it too.
    let restarted = root.state();
    assert!(!reanalysis::probe_now(&restarted).unwrap().detected);
    assert_eq!(
        reanalysis::captured(&restarted).unwrap().identity(),
        "r2://r2/routing"
    );
    assert!(
        settings::current(&restarted)
            .unwrap()
            .reanalysis
            .connected_once
    );
}

#[test]
fn fetch_without_an_archive_is_refused_in_words() {
    let root = TempRoot::new("reanalysis-none");
    let app = root.state();
    let id = with_track(&root, &app);
    let refused = env::queue_fetch(&app, &[id], "hourly", false).unwrap_err();
    let wire = serde_json::to_value(&refused).unwrap();
    assert_eq!(wire["kind"], "no-reanalysis-archive");
    assert_eq!(wire["message"], NO_ARCHIVE);
    assert!(app.env_jobs.status().tracks.is_empty());
    // The GRIB export and the estimate read through the same provider.
    assert_eq!(env::provider(&app).unwrap_err().to_string(), NO_ARCHIVE);
    assert_eq!(
        env::estimate_for(&app, &[id], false)
            .unwrap_err()
            .to_string(),
        NO_ARCHIVE
    );

    // A remembered archive whose location file is gone.
    let server = stub(listing("first-secret"));
    reanalysis::port_set(&app, server.port).unwrap();
    reanalysis::archive_set(&app, Some("r2".to_owned())).unwrap();
    std::fs::remove_file(reanalysis::location_path(&app.paths)).unwrap();
    let missing = env::queue_fetch(&app, &[id], "hourly", false).unwrap_err();
    assert_eq!(missing.kind(), "no-reanalysis-archive");
    assert_eq!(missing.to_string(), LOCATION_MISSING);
}

#[test]
fn provenance_is_the_identity_and_layout_version() {
    let root = TempRoot::new("reanalysis-provenance");
    let server = stub(listing("first-secret"));
    let app = probed(&root, &server);
    let id = with_track(&root, &app);
    reanalysis::archive_set(&app, Some("copy".to_owned())).unwrap();
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    assert_eq!(drain(&app, &Fake), [Outcome::Done]);
    assert_eq!(
        datasets(&app, id),
        [DatasetRecord {
            name: "s3://weather/source".to_owned(),
            version: "hindsight-layout-3".to_owned(),
            fetched_at: NOW,
            has_tide: Some(true),
        }],
        "a local copy is recorded as the archive it copies, never its folder"
    );
}

#[test]
fn the_ipc_settings_json_never_holds_a_secret() {
    let root = TempRoot::new("reanalysis-ipc");
    let server = stub(listing("first-secret"));
    let app = probed(&root, &server);
    reanalysis::archive_set(&app, Some("r2".to_owned())).unwrap();
    let ipc = serde_json::to_string(&settings::current(&app).unwrap()).unwrap();
    assert!(ipc.contains(r#""id":"r2""#), "{ipc}");
    for secret in [
        "AKIDTESTKEY",
        "first-secret",
        "access_key_id",
        "secret_access_key",
        "session_token",
        "credentials",
    ] {
        assert!(!ipc.contains(secret), "{secret} crosses IPC: {ipc}");
    }
    let scan = serde_json::to_string(&reanalysis::probe_now(&app).unwrap()).unwrap();
    assert!(!scan.contains("first-secret") && !scan.contains("AKIDTESTKEY"));
}

#[test]
fn a_forbidden_read_fails_the_fetch_with_the_scan_hint() {
    let root = TempRoot::new("reanalysis-forbidden");
    // The bucket refuses every read, as R2 does a revoked key.
    let bucket = stub(vec![(
        "",
        403,
        "<Error><Code>AccessDenied</Code></Error>".to_owned(),
    )]);
    let endpoint = format!("http://127.0.0.1:{}", bucket.port);
    let cloud_body = json!({"api_version": 1,
        "archives": [cloud("locked", s3(&endpoint, keys("revoked")))], "errors": []});
    let server = stub(vec![(
        "/v1/archives/cloud/reanalysis",
        200,
        cloud_body.to_string(),
    )]);
    let app = probed(&root, &server);
    let id = with_track(&root, &app);
    reanalysis::archive_set(&app, Some("locked".to_owned())).unwrap();
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let provider = env::provider(&app).unwrap();
    let outcomes = drain(&app, provider.as_ref());
    let [Outcome::Failed(message)] = outcomes.as_slice() else {
        panic!("{outcomes:?}");
    };
    assert!(message.contains("403"), "{message}");
    assert!(message.ends_with(&format!(" {HINT}")), "{message}");
    assert!(!message.contains("revoked"), "no secret in a message");
    // The bucket was really asked.
    assert!(!bucket.seen.lock().unwrap().is_empty());
}

/// A choice whose settings cannot be saved leaves the earlier location in
/// place, owner-only.
#[test]
fn a_failed_choice_puts_the_earlier_location_back_owner_only() {
    let root = TempRoot::new("reanalysis-rollback");
    let server = stub(listing("first-secret"));
    let app = probed(&root, &server);
    reanalysis::archive_set(&app, Some("r2".to_owned())).unwrap();
    let before = location_text(&app);
    // settings.json becomes a folder: the next save cannot replace it.
    let settings_file = app.paths.settings_file();
    std::fs::remove_file(&settings_file).unwrap();
    std::fs::create_dir(&settings_file).unwrap();
    assert!(reanalysis::archive_set(&app, Some("copy".to_owned())).is_err());
    assert_eq!(location_text(&app), before, "the earlier location is back");
    assert_eq!(
        settings::current(&app)
            .unwrap()
            .reanalysis
            .archive
            .unwrap()
            .id,
        "r2"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(reanalysis::location_path(&app.paths))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let temp = reanalysis::location_path(&app.paths).with_file_name("reanalysis-archive.json.tmp");
    assert!(!temp.exists(), "no temporary left holding a secret");
}

/// The commands run on a tokio worker: Hindsight Sync's blocking client must
/// be made and dropped off it, or the call panics and never answers.
#[tokio::test(flavor = "multi_thread")]
async fn the_commands_answer_from_a_tokio_worker() {
    let root = TempRoot::new("reanalysis-worker");
    let server = stub(listing("first-secret"));
    let app = root.state();
    // Each body on a worker of a multi-thread runtime, as Tauri runs them.
    let worker = app.clone();
    let port = server.port;
    let saved = tokio::spawn(async move { reanalysis::set_port(worker, port).await })
        .await
        .expect("setting the port answers rather than panicking")
        .unwrap();
    assert_eq!(saved.reanalysis.port, server.port);
    let found = tokio::spawn(reanalysis::probe(app.clone()))
        .await
        .expect("the probe answers rather than panicking")
        .unwrap();
    assert!(found.detected);
    assert_eq!(found.port, server.port);
    let scanned = tokio::spawn(reanalysis::scan(app.clone()))
        .await
        .expect("the scan answers rather than panicking")
        .unwrap();
    assert!(scanned.detected);
    assert_eq!(scanned.port, server.port);
}
