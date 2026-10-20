#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! What the weather for one real track costs (plan.md M14e, D27): live,
//! never in the default suite (`#[ignore]`, `PE_TEST_LIVE=1`).
//!
//! ```text
//! PE_TEST_LIVE=1 cargo test -p pe-app --test weather_download -- --ignored --nocapture
//! ```
//!
//! A Fastnet 2025 boat of about five days is imported from YellowBrick and
//! its weather fetched hourly through the real archive. It prints the
//! pre-flight estimate, the bytes downloaded, the seconds, and what the
//! track adds to the project file; then a second boat of the race, which
//! reads the first one's chunks from memory.

mod common;

use std::sync::Arc;
use std::time::Instant;

use common::TempRoot;
use pe_app::env::{self, EnvJobsStatus, JobSink, Outcome};
use pe_app::{projects, trackers};
use pe_core::track::Tracker;

fn live() -> bool {
    std::env::var("PE_TEST_LIVE").is_ok_and(|v| v == "1")
}

struct Quiet;

impl JobSink for Quiet {
    fn progress(&self, _status: &EnvJobsStatus) {}
    fn changed(&self) {}
}

/// Compressed and plain size of `name` in a project archive.
fn entry_size(bytes: &[u8], name: &str) -> (u64, u64) {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let file = archive.by_name(name).unwrap();
    (file.compressed_size(), file.size())
}

#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn a_fastnet_boat_costs_megabytes_to_fetch_and_kilobytes_to_keep() {
    if !live() {
        return;
    }
    let root = TempRoot::new("weather-download");
    let app = root.state();
    common::choose_live_archive(&app);
    projects::create(&app, "Weather".to_owned(), None, false).unwrap();
    let view = trackers::download_listed(
        &app,
        Arc::new(pe_trackers::yellowbrick::YellowBrick::default()),
        "yb.tl/fastnet2025",
        true,
        |_| {},
        |_| {},
    )
    .unwrap();
    // The boats whose positions span closest to `PE_WEATHER_DAYS` days
    // (default 5), `PE_WEATHER_BOATS` of them (default 2).
    let number = |name: &str, default: i64| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let days = number("PE_WEATHER_DAYS", 5);
    let count = number("PE_WEATHER_BOATS", 2) as usize;
    let mut boats: Vec<_> = view
        .boats
        .iter()
        .filter(|b| b.fixes > 0)
        .filter_map(|b| Some((b.last? - b.first?, b)))
        .collect();
    boats.sort_by_key(|(span, _)| (span - days * 86_400).abs());
    let chosen: Vec<String> = boats
        .iter()
        .take(count)
        .map(|(_, b)| b.id.clone())
        .collect();
    let imported = trackers::import_boats(&app, Tracker::YellowBrick, &view.key, &chosen).unwrap();
    let ids: Vec<u64> = imported.imported.iter().map(|i| i.source_id).collect();
    let provider = env::provider(&app).unwrap();

    for (n, &id) in ids.iter().enumerate() {
        let estimate = env::estimate_for(&app, &[id], false).unwrap();
        let (r0, b0) = provider.net_totals();
        env::queue_fetch(&app, &[id], "hourly", false).unwrap();
        let start = Instant::now();
        let outcome = env::run_next(&app, provider.as_ref(), &Quiet, 1_760_000_000, false);
        let seconds = start.elapsed().as_secs_f64();
        assert_eq!(outcome, Some(Outcome::Done));
        let (r1, b1) = provider.net_totals();

        let summary = projects::summary(&app).unwrap().unwrap();
        let track = summary
            .sources
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.track.clone())
            .unwrap();
        let project = app
            .with_session(|s| Ok(s.open.as_ref().unwrap().project.clone()))
            .unwrap();
        let source = project.source(pe_core::SourceId(id)).unwrap();
        let t = source.track().unwrap();
        let span_h = (t.fixes.last().unwrap().t - t.fixes[0].t) as f64 / 3600.0;
        let entry = pe_core::io::track_entry(t.id);
        let (stored, plain) = entry_size(&pe_core::io::to_bytes(&project).unwrap(), &entry);
        let mut bare = project.clone();
        for s in &mut bare.sources {
            if let Some(track) = s.track_mut() {
                for sample in &mut track.samples {
                    sample.clear_env();
                }
            }
        }
        let (no_env, _) = entry_size(&pe_core::io::to_bytes(&bare).unwrap(), &entry);
        println!(
            "M14e | boat {} {:?}: {} fixes over {span_h:.0} h, {} with wind | estimate {:.1} MB hourly \
             ({:.1} MB already in memory), stored ~{} B | downloaded {:.1} MB in {} \
             requests, {seconds:.1} s | tracks/{}.json {} B deflated \
             ({} B plain), of which the environment {} B ({:.1} B per sample) | memory {:.0} MB",
            n + 1,
            source.label,
            t.fixes.len(),
            track.with_wind,
            estimate.hourly_bytes as f64 / 1e6,
            estimate.cached_bytes as f64 / 1e6,
            estimate.stored_bytes,
            (b1 - b0) as f64 / 1e6,
            r1 - r0,
            t.id.raw(),
            stored,
            plain,
            stored - no_env,
            (stored - no_env) as f64 / t.samples.len() as f64,
            provider.memory().size() as f64 / 1e6,
        );
        assert_eq!(track.env_status, "ready");
        assert!(track.with_wind > 0);
    }
}
