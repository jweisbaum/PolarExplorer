#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Edit-to-view at the spec.md 13 scale, and the 2D plot's "all" mode with
//! 50 tracks of 10,000 fixes (plan.md M13, carried from M7 and M8).
//!
//! Ignored by default (they build large projects); run them in a release
//! build to record the numbers:
//!
//! ```text
//! CARGO_INCREMENTAL=0 cargo test --release -p pe-app --test perf_edit -- --ignored --nocapture
//! ```
//!
//! Each measures what Rust does between an edit and every open view having
//! its data: the command and the summary it returns, the 3D scene (flags
//! only, since no sample moved), the 2D curves and dots, and the table.
//! The IPC transfer and the frontend's own work are measured in the page.

mod common;

use std::time::{Duration, Instant};

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::compare::{self, CompareOperand};
use pe_app::polar_edit::{self, EditOp, PolarCell};
use pe_app::{polar_plot, polar3d, projects};
use pe_core::polar::{PolarFileFormat, PolarGrid};
use pe_core::track::{Fix, Sample, Track, TrackOrigin};
use pe_core::{Colour, Command, SampleId, Source, SourceId, SourceKind, TrackId};
use serde::Serialize;

/// A 19 × 10 polar on the default output grid, a smooth made-up shape.
fn polar_source(id: u64) -> Source {
    let twa: Vec<f64> = vec![
        0.0, 30.0, 35.0, 40.0, 45.0, 52.0, 60.0, 70.0, 75.0, 80.0, 90.0, 100.0, 110.0, 120.0,
        135.0, 150.0, 160.0, 170.0, 180.0,
    ];
    let tws: Vec<f64> = vec![4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0, 25.0, 30.0];
    let bsp = twa
        .iter()
        .map(|a| {
            tws.iter()
                .map(|w| Some((w.sqrt() * 2.2 * (a.to_radians() / 1.4).sin().abs()).min(20.0)))
                .collect()
        })
        .collect();
    Source::new(
        SourceId(id),
        format!("polar {id}"),
        Colour::parse("#4e79a7").unwrap(),
        SourceKind::PolarFile {
            matrix: None,
            format: PolarFileFormat::Adrena,
            file_name: format!("{id}.pol"),
            polar: PolarGrid { twa, tws, bsp },
        },
    )
}

/// A track of `n` samples with wind, spread over the polar.
fn track_source(id: u64, n: u64) -> Source {
    let mut track = Track::new(
        TrackId(id + 1),
        TrackOrigin::File {
            name: format!("{id}.csv"),
            boat_name: None,
        },
    );
    for k in 0..n {
        let fix = Fix {
            tws: None,
            twd_from: None,
            t: 1_753_531_200 + k as i64 * 30,
            lat: 50.0 + k as f64 * 1e-4,
            lon: -5.0,
            cog: None,
            sog: None,
        };
        let x = (k * 7919 + id * 104_729) as f64;
        let mut sample = Sample::at(SampleId(id * 1_000_000 + k), k as u32, &fix);
        sample.twa = Some(30.0 + x % 150.0);
        sample.tws = Some(4.0 + (x * 0.37) % 24.0);
        sample.speed = Some(3.0 + (x * 0.13) % 8.0);
        sample.heading = Some(10.0);
        sample.hs_m = Some(1.0 + (x * 0.01) % 2.0);
        track.fixes.push(fix);
        track.samples.push(sample);
    }
    Source::new(
        SourceId(id),
        format!("track {id}"),
        Colour::parse("#e15759").unwrap(),
        SourceKind::Track {
            track: Box::new(track),
        },
    )
}

fn project(root: &TempRoot, polars: u64, tracks: u64, samples: u64) -> AppState {
    let app = root.state();
    projects::create(&app, "Perf".to_owned(), None, false).unwrap();
    app.with_session(|session| {
        let open = session.require_open()?;
        open.project.next_id = 1_000_000_000;
        let mut index = 0;
        for id in 1..=polars {
            open.apply(Command::AddSource {
                index,
                source: Box::new(polar_source(id)),
            })?;
            index += 1;
        }
        for t in 0..tracks {
            open.apply(Command::AddSource {
                index,
                source: Box::new(track_source(100 + t, samples)),
            })?;
            index += 1;
        }
        Ok(())
    })
    .unwrap();
    app
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn key_of(bytes: &[u8]) -> u64 {
    let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    u64::from(word(8)) | (u64::from(word(9)) << 32)
}

/// What every open view asks for after an edit, and how long each took.
fn views(app: &AppState, held: u64, tws: f64, focus: u64) -> Vec<(&'static str, f64, usize)> {
    let mut out = Vec::new();
    let t = Instant::now();
    let scene = polar3d::scene_bytes_for(app, Some(focus), Some(held)).unwrap();
    out.push(("3D scene", ms(t.elapsed()), scene.len()));
    let t = Instant::now();
    let plot = polar_plot::plot(app, Some(tws)).unwrap();
    let json = serde_json::to_vec(&plot).unwrap();
    out.push(("2D curves (JSON)", ms(t.elapsed()), json.len()));
    let t = Instant::now();
    let dots = polar_plot::dots_bytes(app, Some(tws), false).unwrap();
    out.push(("2D dots", ms(t.elapsed()), dots.len()));
    let t = Instant::now();
    let surface = polar_edit::edit_surface(app, focus).unwrap();
    let json = serde_json::to_vec(&surface).unwrap();
    out.push(("table", ms(t.elapsed()), json.len()));
    // The Compare stage: the edited source against the blend (spec.md 11).
    let operand = if focus >= 100 {
        CompareOperand::Segment { source_id: focus }
    } else {
        CompareOperand::Polar { source_id: focus }
    };
    let t = Instant::now();
    let compared = compare::compare_bytes(app, operand, CompareOperand::Blend, 0.05).unwrap();
    out.push(("compare", ms(t.elapsed()), compared.len()));
    out
}

fn report(what: &str, edit: f64, summary_bytes: usize, views: &[(&str, f64, usize)]) -> f64 {
    let total = edit + views.iter().map(|v| v.1).sum::<f64>();
    println!("{what}: edit + summary {edit:.1} ms ({summary_bytes} B)");
    for (name, time, bytes) in views {
        println!("  {name}: {time:.1} ms, {bytes} B");
    }
    println!("  total {total:.1} ms");
    total
}

/// Spec.md 13: edit to every open view < 100 ms at 20 sources and 200,000
/// samples.
#[test]
#[ignore = "a large project; run in a release build to record numbers"]
fn edit_to_every_view_at_two_hundred_thousand_samples() {
    let root = TempRoot::new("perf-edit");
    let app = project(&root, 20, 20, 10_000);

    let t = Instant::now();
    let full = polar3d::scene_bytes_for(&app, None, None).unwrap();
    println!(
        "cold full scene (every segment derived): {:.1} ms, {} B",
        ms(t.elapsed()),
        full.len()
    );
    let t = Instant::now();
    let warm = polar3d::scene_bytes_for(&app, None, None).unwrap();
    println!(
        "warm full scene: {:.1} ms, {} B",
        ms(t.elapsed()),
        warm.len()
    );
    let held = key_of(&full);

    // A table edit on a polar source.
    let t = Instant::now();
    let summary = polar_edit::polar_edit(
        &app,
        3,
        &EditOp::Type { bsp: Some(7.5) },
        &[PolarCell {
            twa_index: 10,
            tws_index: 4,
        }],
        None,
    )
    .unwrap();
    let summary_bytes = serde_json::to_vec(&summary).unwrap().len();
    let edit = ms(t.elapsed());
    let polar_total = report(
        "polar source edit",
        edit,
        summary_bytes,
        &views(&app, held, 12.0, 3),
    );

    // One step of a drag on a track's segment: the samples are not binned
    // again.
    let t = Instant::now();
    let summary = polar_edit::polar_edit(
        &app,
        105,
        &EditOp::Drag { bsp: 6.5 },
        &[PolarCell {
            twa_index: 10,
            tws_index: 4,
        }],
        Some("perf"),
    )
    .unwrap();
    let summary_bytes = serde_json::to_vec(&summary).unwrap().len();
    let edit = ms(t.elapsed());
    let segment_total = report(
        "track segment drag step",
        edit,
        summary_bytes,
        &views(&app, held, 12.0, 105),
    );

    // Excluding 1,000 samples of one track: that track is binned again.
    let ids: Vec<u64> = (0..1000).map(|k| 105 * 1_000_000 + k * 7).collect();
    let t = Instant::now();
    let summary = polar3d::excluded_set(&app, &[], &ids, true).unwrap();
    let summary_bytes = serde_json::to_vec(&summary).unwrap().len();
    let edit = ms(t.elapsed());
    let exclude_total = report(
        "exclude 1,000 samples",
        edit,
        summary_bytes,
        &views(&app, held, 12.0, 105),
    );

    // The blend alone after an edit (spec.md 13: under 50 ms at 20 sources
    // and 200k samples), every segment already derived; and an export,
    // which derives everything from scratch (invariant 2).
    let (blend_ms, export_ms) = app
        .with_session(|session| {
            let open = session.require_open()?;
            open.derived.bump(3, pe_app::derived::Touch::Polar);
            let t = Instant::now();
            open.derived.blend(&open.project);
            let blend_ms = ms(t.elapsed());
            let t = Instant::now();
            pe_app::blend::export_bytes(&open.project, "adrena", None)?;
            Ok((blend_ms, ms(t.elapsed())))
        })
        .unwrap();
    println!("blend after an edit: {blend_ms:.1} ms; export from scratch: {export_ms:.1} ms");

    if !cfg!(debug_assertions) {
        assert!(blend_ms < 50.0, "the blend took {blend_ms} ms");
        for total in [polar_total, segment_total, exclude_total] {
            assert!(total < 100.0, "{total} ms is over the 100 ms budget");
        }
    }
}

/// The old wire shape of a dot: one JSON object each.
#[derive(Serialize)]
struct JsonDot {
    source_id: u64,
    sample_id: u64,
    twa: f64,
    tws: f64,
    bsp: f64,
    filtered: bool,
    excluded: bool,
}

/// The M8 carry: "all" mode with 50 tracks × 10,000 fixes, JSON objects
/// (the M8 shape) against the packed dots.
#[test]
#[ignore = "a large project; run in a release build to record numbers"]
fn plot_all_mode_with_fifty_tracks_of_ten_thousand_fixes() {
    let root = TempRoot::new("perf-all");
    let app = project(&root, 0, 50, 10_000);
    // Warm the cache, as the scene or the summary already has.
    polar_plot::dots(&app, None, false).unwrap();

    let t = Instant::now();
    let dots = polar_plot::dots(&app, None, false).unwrap();
    let gather = ms(t.elapsed());
    let t = Instant::now();
    let json: Vec<JsonDot> = dots
        .iter()
        .map(|d| JsonDot {
            source_id: d.source_id,
            sample_id: d.sample_id,
            twa: d.twa,
            tws: d.tws,
            bsp: d.bsp,
            filtered: d.filtered,
            excluded: d.excluded,
        })
        .collect();
    let text = serde_json::to_vec(&json).unwrap();
    let json_time = ms(t.elapsed());
    let t = Instant::now();
    let packed = polar_plot::pack_dots(&dots);
    let pack_time = ms(t.elapsed());
    println!(
        "all mode, {} dots: gather {gather:.1} ms; JSON objects {json_time:.1} ms, {} B; packed {pack_time:.1} ms, {} B",
        dots.len(),
        text.len(),
        packed.len()
    );
    assert_eq!(dots.len(), 500_000);
    assert!(packed.len() * 4 < text.len());
}

/// Compare the existing hard-filter pass with per-cell priorities on 200k points.
#[test]
#[ignore = "large project; run explicitly for analysis performance"]
fn priority_filter_pass_at_two_hundred_thousand_samples() {
    use pe_core::source::{Range, SampleFilters};
    let root = TempRoot::new("perf-priority");
    let app = project(&root, 0, 20, 10_000);
    app.with_session(|session| {
        let open = session.require_open()?;
        let start = Instant::now();
        let hard = pe_app::priority::filter_flags(&open.project);
        let baseline = ms(start.elapsed());
        let all = SampleFilters {
            min_bsp_kn: None,
            max_heading_change_deg: None,
            ..Default::default()
        };
        open.project.blend.priority_groups = vec![
            SampleFilters {
                wave_height_m: Some(Range {
                    min: None,
                    max: Some(1.5),
                }),
                ..all.clone()
            },
            all,
        ];
        let start = Instant::now();
        let selected = pe_app::priority::filter_flags(&open.project);
        let priority = ms(start.elapsed());
        assert_eq!(hard.values().map(Vec::len).sum::<usize>(), 200_000);
        assert_eq!(selected.values().map(Vec::len).sum::<usize>(), 200_000);
        assert!(selected.values().flatten().any(|flag| *flag));
        println!(
            "200,000 points: hard filters {baseline:.1} ms; two priority groups {priority:.1} ms"
        );
        assert!(priority < 100.0, "priority selection {priority:.1} ms");
        Ok(())
    })
    .unwrap();
}

/// Exercise previous/next point changes alongside the existing filter pass.
#[test]
#[ignore = "timed analysis pass; run alone after other checks"]
fn change_filters_at_two_hundred_thousand_samples() {
    use pe_core::source::SampleFilters;
    let root = TempRoot::new("perf-changes");
    let app = project(&root, 0, 20, 10_000);
    app.with_session(|session| {
        let open = session.require_open()?;
        for source in &mut open.project.sources {
            if let Some(track) = source.track_mut() {
                for (k, s) in track.samples.iter_mut().enumerate() {
                    s.heading = Some((k * 7 % 360) as f64);
                    s.twd_from = Some((k % 360) as f64);
                    s.relate();
                }
            }
        }
        let start = Instant::now();
        let baseline = pe_app::priority::filter_flags(&open.project);
        let baseline_ms = ms(start.elapsed());
        open.project.blend.global_filters = Some(SampleFilters {
            max_heading_change_deg: Some(20.0), max_awa_change_deg: Some(20.0),
            max_wind_speed_change_kn: Some(2.0), max_wind_direction_change_deg: Some(20.0),
            min_bsp_kn: None, ..Default::default()
        });
        let start = Instant::now();
        let changed = pe_app::priority::filter_flags(&open.project);
        let changed_ms = ms(start.elapsed());
        assert_eq!(baseline.values().map(Vec::len).sum::<usize>(), 200_000);
        assert_eq!(changed.values().map(Vec::len).sum::<usize>(), 200_000);
        assert!(changed.values().flatten().all(|flag| *flag));
        println!("200,000 observations: original filters {baseline_ms:.1} ms; all four change thresholds, previous/next points {changed_ms:.1} ms");
        assert!(changed_ms < 100.0, "change filters took {changed_ms:.1} ms");
        // A second pass must evaluate all four metrics: steady heading/wind,
        // with varying boat speed and therefore varying apparent wind angle.
        for source in &mut open.project.sources {
            if let Some(track) = source.track_mut() {
                for s in &mut track.samples {
                    s.heading = Some(90.0);
                    s.twd_from = Some(30.0);
                    s.tws = Some(12.0);
                    s.relate();
                }
            }
        }
        let start = Instant::now();
        let steady = pe_app::priority::filter_flags(&open.project);
        let steady_ms = ms(start.elapsed());
        assert!(steady.values().flatten().all(|flag| !*flag));
        println!("200,000 steady observations: all four change thresholds, previous/next points {steady_ms:.1} ms");
        assert!(steady_ms < 100.0, "steady change filters took {steady_ms:.1} ms");
        Ok(())
    }).unwrap();
}
