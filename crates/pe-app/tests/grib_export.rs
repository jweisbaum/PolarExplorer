#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The reanalysis GRIB export end to end (spec.md 7.8): a track's area and
//! hours read through the real provider from the Whirlwind fixture archive,
//! served over HTTP by `pe-env`'s loopback S3 stub, which honours `Range`
//! as S3 does, and written as a `.grib2` file.
//!
//! **The golden file pins the bytes** (invariant 5, CLAUDE.md "Changing
//! export output"): `crates/pe-grib/tests/golden/reanalysis.grib2`, whose
//! SHA-256 is pinned below. Every CI target must write the same bytes, and
//! CI reads the file with ecCodes (`tools/check-grib.sh`). Rewrite it with
//! `PE_BLESS=1 cargo test -p pe-app --test grib_export` only when the
//! output is meant to change, and say why in the commit.
//!
//! # The archive served
//!
//! `pe_env::whirlwind::testing::fixture()`: hourly from 2000-01-01T00Z on a
//! grid of 30° rows (60N, 30N, 0, 30S) and 45° columns, its float16 values
//! depending only on the hour `h` and the latitude: u10 = 2h + (60 − lat) /
//! 30, v10 = 4, wave height 2 + h, wave direction 350° at even hours and
//! 10° at odd ones, current (0.5, −0.25). Everything is linear in latitude,
//! so the bilinear regridding onto the 0.25° nodes gives the formula
//! exactly. The shards from 02Z on are withheld: that hour is missing in
//! the archive and must be written as missing.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use common::TempRoot;
use pe_app::grib::{self, GribExportStatus, GribSink, Outcome, Plan};
use pe_env::whirlwind::testing;
use pe_env::{Interval, Reanalysis};
use pe_grib::reader::{Decoded, decode_all};
use sha2::{Digest, Sha256};

/// The pinned SHA-256 of the golden export.
const GOLDEN_SHA256: &str = "8c007d05b68065e5b5f6bb61284db4ce58486346ab0eee478d3324a536c7d93c";

/// 2000-01-01T00:00Z, the fixture archive's first hour.
const T0: i64 = 946_684_800;

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pe-grib/tests/golden/reanalysis.grib2")
}

// ------------------------------------------------------------- archive

/// The fixture archive on a loopback stub, without the time shards from
/// 02Z on.
fn archive() -> testing::Server {
    let mut objects = testing::fixture();
    objects.retain(|key, _| !key.starts_with("data/c/1/"));
    testing::serve(objects, 200)
}

fn provider(server: &testing::Server) -> Reanalysis {
    testing::reader_at(server.endpoint())
}

// -------------------------------------------------------------- tests

/// Three fixes from 00:30 to 01:30Z: the area 43–47.75N × 82W–77W (20
/// rows, 21 columns), hours 00, 01 and 02Z.
fn plan(waves: bool, current: bool) -> Plan {
    let fixes = [
        (T0 + 1800, 45.0, -80.0),
        (T0 + 3600, 45.3, -79.6),
        (T0 + 5400, 45.6, -79.2),
    ];
    Plan::of(&fixes, waves, current, Interval::Hourly, false).unwrap()
}

fn tolerance(values: &[f32]) -> f32 {
    let finite = values.iter().copied().filter(|v| v.is_finite());
    let (lo, hi) = finite.fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), v| {
        (a.min(v), b.max(v))
    });
    ((hi - lo) / 65535.0).max(1e-6) * 1.5
}

/// The value at the node `(lat, lon)` of a decoded message.
fn at(m: &Decoded, lat: f64, lon: f64) -> f32 {
    let j = ((f64::from(m.la1) / 1e6 - lat) / 0.25).round() as usize;
    let west = f64::from(m.lo1) / 1e6;
    let i = ((lon.rem_euclid(360.0) - west).rem_euclid(360.0) / 0.25).round() as usize;
    m.values[j * m.ni as usize + i]
}

fn export_to(dir: &Path, name: &str, plan: &Plan) -> (Vec<u8>, grib::Summary) {
    let server = archive();
    let p = provider(&server);
    let path = dir.join(name);
    let outcome = grib::export(
        &p,
        plan,
        &path,
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .unwrap();
    let Outcome::Done(summary) = outcome else {
        panic!("{outcome:?}");
    };
    // Shards are read by their index and inner chunk ranges, never whole.
    let requests = server.requests.lock().unwrap().clone();
    assert!(
        !requests.is_empty()
            && requests
                .iter()
                .all(|(key, range)| key.starts_with("data/c/") && range.starts_with("bytes=")),
        "{requests:?}"
    );
    (std::fs::read(&path).unwrap(), summary)
}

/// The acceptance test: every message decodes, with the right times,
/// grid, parameters and values, and the bytes are the golden file's.
#[test]
fn a_track_exports_wind_waves_and_current_to_the_golden_bytes() {
    let root = TempRoot::new("grib-golden");
    let plan = plan(true, true);
    assert_eq!((plan.grid.ni, plan.grid.nj), (21, 20));
    let (bytes, summary) = export_to(&root.0, "race.grib2", &plan);
    assert_eq!(summary.messages, 18);
    assert_eq!(summary.bytes, bytes.len() as u64);
    // 02Z is not in the archive: its 6 messages are empty.
    assert_eq!(summary.empty_messages, 6);

    let decoded = decode_all(&bytes);
    assert_eq!(decoded.len(), 18);
    let order = [
        (0, 2, 2),
        (0, 2, 3),
        (10, 0, 3),
        (10, 0, 14),
        (10, 1, 2),
        (10, 1, 3),
    ];
    for (k, m) in decoded.iter().enumerate() {
        assert_eq!(
            (m.discipline, m.category, m.number),
            order[k % 6],
            "message {k}"
        );
        assert_eq!(m.forecast_hour, (k / 6) as u32, "message {k}");
        let r = m.reference_time;
        assert_eq!((r.year, r.month, r.day, r.hour), (2000, 1, 1, 0));
        assert_eq!((m.la1, m.la2), (47_750_000, 43_000_000));
        assert_eq!((m.lo1, m.lo2), (278_000_000, 283_000_000));
        assert_eq!((m.di, m.dj), (250_000, 250_000));
        // 16 bits, or none for an empty or constant field.
        let finite: Vec<f32> = m.values.iter().copied().filter(|x| x.is_finite()).collect();
        assert!(
            m.bits == 16 || (m.bits == 0 && finite.windows(2).all(|w| w[0] == w[1])),
            "message {k}"
        );
    }

    for (hour, base) in [(0usize, 0), (1, 6)] {
        let h = hour as f32;
        let (u, v) = (&decoded[base], &decoded[base + 1]);
        let (hs, dir) = (&decoded[base + 2], &decoded[base + 3]);
        let (cu, cv) = (&decoded[base + 4], &decoded[base + 5]);
        for (lat, lon) in [(43.0, -82.0), (45.0, -80.0), (47.75, -77.0), (46.5, -78.25)] {
            // Wind: linear in latitude, so exact on every node.
            let want_u = 2.0 * h + (60.0 - lat as f32) / 30.0;
            assert!(
                (at(u, lat, lon) - want_u).abs() <= tolerance(&u.values),
                "{hour} {lat} {lon}: {}",
                at(u, lat, lon)
            );
            assert!((at(v, lat, lon) - 4.0).abs() <= tolerance(&v.values));
            assert!((at(hs, lat, lon) - (2.0 + h)).abs() <= tolerance(&hs.values));
            let want_dir = if hour == 0 { 350.0 } else { 10.0 };
            assert!((at(dir, lat, lon) - want_dir).abs() <= tolerance(&dir.values));
            assert!((at(cu, lat, lon) - 0.5).abs() <= tolerance(&cu.values));
            assert!((at(cv, lat, lon) + 0.25).abs() <= tolerance(&cv.values));
        }
    }
    // Nothing at 02Z.
    for (k, m) in decoded.iter().enumerate().skip(12) {
        assert!(m.values.iter().all(|x| x.is_nan()), "message {k}");
    }

    if std::env::var("PE_BLESS").is_ok_and(|v| v == "1") {
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        std::fs::write(golden_path(), &bytes).unwrap();
    }
    let golden = std::fs::read(golden_path()).expect("the golden file");
    assert!(bytes == golden, "the export differs from the golden file");
    let sha = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(sha, GOLDEN_SHA256, "the pinned hash");
}

/// The preview counts the archive chunks the export reads, and once they
/// are read, counts them as held.
#[test]
fn the_preview_counts_the_chunks_an_export_reads() {
    let root = TempRoot::new("grib-preview");
    let server = archive();
    let p = provider(&server);
    let plan = plan(true, true);
    let before = grib::preview_of(7, "Alpha", &plan, &p).unwrap();
    assert_eq!((before.source_id, before.label.as_str()), (7, "Alpha"));
    assert_eq!((before.ni, before.nj, before.times), (21, 20, 3));
    assert!(before.wind_bytes > 0);
    assert_eq!(before.cached_bytes, 0);
    let path = root.0.join("race.grib2");
    grib::export(
        &p,
        &plan,
        &path,
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .unwrap();
    let after = grib::preview_of(7, "Alpha", &plan, &p).unwrap();
    assert_eq!(after.wind_bytes, 0);
    assert_eq!(after.cached_bytes, before.wind_bytes);
}

/// A plan over the budget (217 rows × 297 columns × 4 hours, 257,796
/// positions) is thinned for the preview, yet counts exactly the chunks
/// the full plan needs.
#[test]
fn a_thinned_preview_counts_every_chunk_of_the_full_plan() {
    let server = testing::serve(testing::fixture(), 200);
    let p = provider(&server);
    let plan = Plan::of(
        &[(T0, 0.0, -120.0), (T0 + 3 * 3600, 50.0, -50.0)],
        true,
        true,
        Interval::Hourly,
        false,
    )
    .unwrap();
    assert_eq!(
        (plan.grid.nj, plan.grid.ni, plan.times.len()),
        (217, 297, 4)
    );
    let full: Vec<pe_env::Point> = plan
        .times
        .iter()
        .flat_map(|&t| {
            plan.grid
                .points()
                .map(move |(lon, lat)| pe_env::Point { t, lat, lon })
        })
        .collect();
    assert!(full.len() > 200_000);
    let expected = p.estimate(&full).unwrap().hourly_bytes;
    assert!(expected > 0);
    let preview = grib::preview_of(1, "Wide", &plan, &p).unwrap();
    assert_eq!(preview.wind_bytes, expected);
}

/// Wind alone is the first two messages of every hour, with no waves or
/// current read, and the same values as among the rest.
#[test]
fn wind_alone_writes_two_messages_an_hour() {
    let root = TempRoot::new("grib-wind");
    let (bytes, summary) = export_to(&root.0, "wind.grib2", &plan(false, false));
    assert_eq!(summary.messages, 6);
    let all = std::fs::read(golden_path()).unwrap();
    let full = decode_all(&all);
    let wind = decode_all(&bytes);
    for (k, m) in wind.iter().enumerate() {
        let same = &full[(k / 2) * 6 + k % 2];
        assert_eq!(m.values.len(), same.values.len());
        for (a, b) in m.values.iter().zip(&same.values) {
            assert!(a == b || (a.is_nan() && b.is_nan()));
        }
    }
}

/// A cancelled export writes nothing: what was at the path stays, and no
/// temporary file (the export's or the current's) is left.
#[test]
fn a_cancelled_export_leaves_the_path_as_it_was() {
    let root = TempRoot::new("grib-cancel");
    let server = archive();
    let p = provider(&server);
    let path = root.0.join("race.grib2");
    std::fs::write(&path, b"an earlier file").unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    // Cancelled as soon as the first part is done.
    let outcome = grib::export(&p, &plan(true, true), &path, &cancel, &mut |f| {
        if f > 0.0 {
            flag.store(true, Ordering::SeqCst);
        }
    })
    .unwrap();
    assert_eq!(outcome, Outcome::Cancelled);
    assert_eq!(std::fs::read(&path).unwrap(), b"an earlier file");
    let left: Vec<_> = std::fs::read_dir(&root.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

/// A panic mid-export (a bug) unwinds through the file and the spool,
/// which delete their temporaries; what was at the path stays (M17a). It
/// panics on the progress after the last hour's messages are written:
/// the current is still spooled and every message is in the temporary,
/// the commit not yet made.
#[test]
fn a_panic_mid_export_leaves_no_temporary() {
    let root = TempRoot::new("grib-panic-mid");
    let server = archive();
    let p = provider(&server);
    let path = root.0.join("race.grib2");
    std::fs::write(&path, b"an earlier file").unwrap();
    let temp = pe_grib::file::temp_path_for(&path);
    let spool = root.0.join("race.grib2.current.tmp");
    let mut seen = false;
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        grib::export(
            &p,
            &plan(true, true),
            &path,
            &Arc::new(AtomicBool::new(false)),
            &mut |f| {
                if f >= 1.0 {
                    seen = temp.exists() && spool.exists();
                    panic!("a bug mid-export");
                }
            },
        )
    }));
    assert!(unwound.is_err(), "the export panicked");
    assert!(seen, "with the temporary and the spool on disk");
    assert_eq!(std::fs::read(&path).unwrap(), b"an earlier file");
    let left: Vec<_> = std::fs::read_dir(&root.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

struct Quiet;

impl GribSink for Quiet {
    fn progress(&self, _status: &GribExportStatus) {}
}

/// A project holding one track, 00:30 to 01:30Z.
fn project_with_track(root: &TempRoot) -> (pe_app::commands::AppState, u64) {
    use pe_app::tracks::{self, TrackFileRequest};
    let app = root.state();
    pe_app::projects::create(&app, "Grib".to_owned(), None, false).unwrap();
    let features: Vec<String> = [(T0 + 1800, 45.0, -80.0), (T0 + 5400, 45.6, -79.2)]
        .iter()
        .map(|(t, lat, lon)| {
            format!(
                r#"{{"type":"Feature","geometry":{{"type":"Point","coordinates":[{lon},{lat}]}},"properties":{{"time":{t},"boat":"Alpha"}}}}"#
            )
        })
        .collect();
    let file = root.file("race.geojson");
    std::fs::write(
        &file,
        format!(
            r#"{{"type":"FeatureCollection","features":[{}]}}"#,
            features.join(",")
        ),
    )
    .unwrap();
    let imported = tracks::import(
        &app,
        &[TrackFileRequest {
            path: file,
            mapping: None,
            boats: None,
        }],
    )
    .unwrap();
    (app, imported.imported[0].source_id)
}

/// Through the app: the plan comes from the open project's track, the job
/// refuses a second export while one runs, and its status ends "done"
/// with the file's size.
#[test]
fn the_job_exports_a_project_track_once_at_a_time() {
    let root = TempRoot::new("grib-job");
    let (app, id) = project_with_track(&root);

    let (planned, label) = grib::plan_for(&app, id, "hourly", true, false).unwrap();
    assert_eq!(label, "Alpha");
    assert_eq!(planned.times, vec![T0, T0 + 3600, T0 + 7200]);
    assert!(grib::plan_for(&app, id, "weekly", true, false).is_err());

    let out = root.file("alpha.grib2");
    let (plan, cancel) = grib::begin(&app, id, &out, "hourly", true, false).unwrap();
    assert_eq!(app.grib_jobs.status().state, "running");
    assert!(
        grib::begin(&app, id, &out, "hourly", true, false).is_err(),
        "one at a time"
    );

    let server = archive();
    let p = provider(&server);
    let status = grib::run(&app, &p, &|| p.net_totals().1, &plan, &out, &cancel, &Quiet);
    assert_eq!(status.state, "done", "{status:?}");
    assert_eq!(status.messages, 12);
    assert!(status.downloaded_bytes > 0);
    assert_eq!(status.bytes, std::fs::metadata(&out).unwrap().len());
    assert_eq!(app.grib_jobs.status(), status);
    // Done, so another may start.
    assert!(grib::begin(&app, id, &out, "three_hourly", false, false).is_err());
    assert!(grib::begin(&app, id, &out, "hourly", false, false).is_ok());
}

/// What a real track's GRIB export costs (plan.md M16): live, never in the
/// default suite (`#[ignore]`, `PE_TEST_LIVE=1`).
///
/// ```text
/// PE_TEST_LIVE=1 cargo test -p pe-app --test grib_export live -- --ignored --nocapture
/// ```
///
/// A Fastnet 2025 boat of about `PE_GRIB_DAYS` days (default 2) is
/// imported from YellowBrick and its reanalysis exported through the real
/// archive: wind and waves, hourly, and the current if `PE_GRIB_CURRENT=1`.
/// It prints the dialog's estimate, the bytes downloaded, the file's size
/// and the seconds, and decodes every message with the test reader. If
/// ecCodes' `grib_ls` is on the PATH, it must read the file too.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
#[allow(clippy::print_stdout, reason = "the figures are the point")]
fn live_a_fastnet_boat_exports_to_grib() {
    if std::env::var("PE_TEST_LIVE").as_deref() != Ok("1") {
        return;
    }
    let root = TempRoot::new("grib-live");
    let app = root.state();
    pe_app::projects::create(&app, "Grib".to_owned(), None, false).unwrap();
    let view = pe_app::trackers::download_listed(
        &app,
        Arc::new(pe_trackers::yellowbrick::YellowBrick::default()),
        "yb.tl/fastnet2025",
        true,
        |_| {},
        |_| {},
    )
    .unwrap();
    let days: i64 = std::env::var("PE_GRIB_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);
    let current = std::env::var("PE_GRIB_CURRENT").as_deref() == Ok("1");
    let boat = view
        .boats
        .iter()
        .filter(|b| b.fixes > 0)
        .filter_map(|b| Some((b.last? - b.first?, b)))
        .min_by_key(|(span, _)| (span - days * 86_400).abs())
        .map(|(_, b)| b.id.clone())
        .unwrap();
    let imported = pe_app::trackers::import_boats(
        &app,
        pe_core::track::Tracker::YellowBrick,
        &view.key,
        &[boat],
    )
    .unwrap();
    let id = imported.imported[0].source_id;
    let (plan, label) = grib::plan_for(&app, id, "hourly", true, current).unwrap();
    let provider = pe_app::env::provider(&app).unwrap();
    let preview = grib::preview_of(id, &label, &plan, &provider).unwrap();
    let out = root.file("live.grib2");
    let (_, b0) = provider.net_totals();
    let start = std::time::Instant::now();
    let outcome = grib::export(
        provider.as_ref(),
        &plan,
        Path::new(&out),
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .unwrap();
    let seconds = start.elapsed().as_secs_f64();
    let (_, b1) = provider.net_totals();
    let Outcome::Done(summary) = outcome else {
        panic!("{outcome:?}");
    };
    let bytes = std::fs::read(&out).unwrap();
    let decoded = decode_all(&bytes);
    assert_eq!(decoded.len() as u64, plan.messages());
    println!(
        "M16 | {label}: {} × {} points, {} hours, {} messages | estimate {:.1} MB download \
         ({:.1} MB held), file ~{:.2} MB | downloaded {:.1} MB in \
         {seconds:.1} s | file {:.2} MB, {} empty messages",
        plan.grid.ni,
        plan.grid.nj,
        plan.times.len(),
        summary.messages,
        preview.wind_bytes as f64 / 1e6,
        preview.cached_bytes as f64 / 1e6,
        preview.file_bytes as f64 / 1e6,
        (b1 - b0) as f64 / 1e6,
        summary.bytes as f64 / 1e6,
        summary.empty_messages,
    );
    let listed = std::process::Command::new("grib_ls").arg(&out).output();
    match listed {
        Ok(result) => {
            assert!(result.status.success(), "grib_ls failed");
            let text = String::from_utf8_lossy(&result.stdout);
            let last = text
                .lines()
                .rev()
                .find(|l| l.contains("messages"))
                .unwrap_or("");
            println!("M16 | grib_ls: {last}");
        }
        Err(_) => println!("M16 | grib_ls is not installed; ecCodes did not check the file"),
    }
    std::fs::copy(&out, std::env::temp_dir().join("pe-grib-live.grib2")).unwrap();
}

/// A provider whose archive will not answer: prepare fails, and so does
/// every read; or, with `panics`, a read panics (a bug).
struct Broken {
    panics: bool,
}

impl pe_env::Provider for Broken {
    fn sample(
        &self,
        points: &[pe_env::Point],
        _options: &pe_env::Options,
        _cancel: &Arc<AtomicBool>,
    ) -> pe_env::Result<Vec<pe_env::EnvPoint>> {
        assert!(!self.panics, "a bug in a read");
        let _ = points;
        Err(pe_env::EnvError::Open("the archive is down".to_owned()))
    }

    fn prepare(
        &self,
        _points: &[pe_env::Point],
        _options: &pe_env::Options,
        _cancel: &Arc<AtomicBool>,
    ) -> pe_env::Result<()> {
        if self.panics {
            return Ok(());
        }
        Err(pe_env::EnvError::Open("the archive is down".to_owned()))
    }
}

/// Review round 1: a prepare that fails without a cancel is a failure,
/// not a cancel, and names why; nothing is written.
#[test]
fn a_failing_archive_fails_the_export_rather_than_cancelling_it() {
    let root = TempRoot::new("grib-broken");
    let path = root.0.join("race.grib2");
    let err = grib::export(
        &Broken { panics: false },
        &plan(true, false),
        &path,
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .expect_err("a failure");
    assert!(err.to_string().contains("the archive is down"), "{err}");
    assert!(!path.exists());
    assert!(!pe_grib::file::temp_path_for(&path).exists());
    // Cancelled while it failed: a cancel.
    let cancelled = grib::export(
        &Broken { panics: false },
        &plan(true, false),
        &path,
        &Arc::new(AtomicBool::new(true)),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(cancelled, Outcome::Cancelled);
}

/// Review round 1: an export thread that panics leaves the job "failed",
/// not "running" for ever, and no file.
#[test]
fn a_panicking_export_ends_failed() {
    let root = TempRoot::new("grib-panic");
    let (app, id) = project_with_track(&root);
    let out = root.file("alpha.grib2");
    let (plan, cancel) = grib::begin(&app, id, &out, "hourly", false, false).unwrap();
    let joined = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                grib::run(
                    &app,
                    &Broken { panics: true },
                    &|| 0,
                    &plan,
                    &out,
                    &cancel,
                    &Quiet,
                )
            })
            .join()
    });
    assert!(joined.is_err(), "the read panicked");
    let status = app.grib_jobs.status();
    assert_eq!(status.state, "failed", "{status:?}");
    assert!(!Path::new(&out).exists());
    assert!(grib::begin(&app, id, &out, "hourly", false, false).is_ok());
}
