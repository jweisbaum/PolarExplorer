#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The Compare stage over IPC (spec.md 11): each operand read onto the
//! output grid as the other views see it, the difference only where both
//! have a value, and nothing written into the project.

mod common;

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::compare::{self, CompareOperand};
use pe_app::polar_edit::{self, EditOp, PolarCell};
use pe_app::polar3d::{self, PolarNodeRef};
use pe_app::{edit, projects};
use pe_core::polar::{PolarFileFormat, PolarGrid};
use pe_core::track::{Fix, Sample, Track, TrackOrigin};
use pe_core::{Colour, Command, SampleId, Source, SourceId, SourceKind, TrackId};
use pe_polar::{CellClass, Comparison, Faster};

const FILE: CompareOperand = CompareOperand::Polar { source_id: 1 };
const TRACK: CompareOperand = CompareOperand::Segment { source_id: 2 };

/// On output-grid points: 45°, 90°, 135° × 6, 10, 14 kn.
fn file_source() -> Source {
    Source::new(
        SourceId(1),
        "File",
        Colour::parse("#4e79a7").unwrap(),
        SourceKind::PolarFile {
            matrix: None,
            format: PolarFileFormat::Adrena,
            file_name: "file.pol".to_owned(),
            polar: PolarGrid {
                twa: vec![45.0, 90.0, 135.0],
                tws: vec![6.0, 10.0, 14.0],
                bsp: vec![
                    vec![Some(4.0), Some(5.0), Some(6.0)],
                    vec![Some(5.0), Some(9.0), Some(7.0)],
                    vec![Some(4.5), None, Some(6.5)],
                ],
            },
        },
    )
}

/// Ten samples at 90° in 12 kn, BSP 6.0 … 6.9: one segment cell, the 90th
/// percentile 6.81 (rank 8.1 between 6.8 and 6.9).
fn track_source() -> Source {
    let mut track = Track::new(
        TrackId(3),
        TrackOrigin::File {
            name: "race.csv".to_owned(),
            boat_name: None,
        },
    );
    for k in 0..10u64 {
        let fix = Fix {
            tws: None,
            twd_from: None,
            t: 1_753_531_200 + k as i64 * 60,
            lat: 50.0,
            lon: -5.0,
            cog: None,
            sog: None,
        };
        let mut sample = Sample::at(SampleId(200 + k), k as u32, &fix);
        sample.twa = Some(90.0);
        sample.tws = Some(12.0);
        sample.speed = Some(6.0 + k as f64 / 10.0);
        sample.heading = Some(0.0);
        track.fixes.push(fix);
        track.samples.push(sample);
    }
    let mut source = Source::new(
        SourceId(2),
        "Track",
        Colour::parse("#e15759").unwrap(),
        SourceKind::Track {
            track: Box::new(track),
        },
    );
    source.overlay.filters.min_bsp_kn = None;
    source.overlay.filters.max_heading_change_deg = None;
    source
}

fn app(root: &TempRoot) -> AppState {
    let app = root.state();
    projects::create(&app, "Compare".to_owned(), None, false).unwrap();
    app.with_session(|session| {
        let open = session.require_open()?;
        open.project.next_id = 10_000;
        for (index, source) in [file_source(), track_source()].into_iter().enumerate() {
            open.apply(Command::AddSource {
                index,
                source: Box::new(source),
            })?;
        }
        open.history.clear();
        Ok(())
    })
    .unwrap();
    app
}

fn run(app: &AppState, a: CompareOperand, b: CompareOperand) -> pe_app::error::Result<Comparison> {
    app.with_session(|session| {
        let open = session.require_open()?;
        compare::compare_in(&open.project, &mut open.derived, a, b, 0.05)
    })
}

/// Row and column of a TWA and TWS on the default output grid.
fn at(c: &Comparison, twa: f64, tws: f64) -> (usize, usize) {
    (
        c.twa.iter().position(|v| *v == twa).unwrap(),
        c.tws.iter().position(|v| *v == tws).unwrap(),
    )
}

fn project_bytes(app: &AppState) -> Vec<u8> {
    app.with_session(|s| Ok(pe_core::io::to_bytes(&s.require_open()?.project).unwrap()))
        .unwrap()
}

/// File against track: the one segment cell is the overlap. The file is read
/// at 90° 12 kn half way between 9 and 7 kn: 8.0; Δ = 8.0 − 6.81 = 1.19.
#[test]
fn a_polar_against_a_segment_compares_only_where_both_have_a_value() {
    let root = TempRoot::new("compare-overlap");
    let app = app(&root);
    let before = project_bytes(&app);
    let c = run(&app, FILE, TRACK).unwrap();
    let (i, j) = at(&c, 90.0, 12.0);
    assert_eq!(c.a[i][j], Some(8.0));
    assert_eq!(c.b[i][j], Some(6.81));
    assert_eq!(c.delta_kn[i][j], Some(1.19));
    assert_eq!(c.overlap, 1);
    assert_eq!(c.b_only, 0);
    // Every other cell the file reaches (45°–135° × 6–14 kn on the output
    // grid, less those read from its empty 135°/10 kn node) is A only.
    let file_cells = c.a.iter().flatten().filter(|v| v.is_some()).count();
    assert_eq!(c.a_only, file_cells - 1);
    assert_eq!(c.class[i][j], CellClass::Both);
    assert_eq!(c.regions.len(), 1);
    assert_eq!(c.regions[0].faster, Faster::A);
    assert_eq!(
        (c.regions[0].first_twa_index, c.regions[0].tws_index),
        (i, j)
    );
    // Swapping the operands negates Δ and the side.
    let swapped = run(&app, TRACK, FILE).unwrap();
    assert_eq!(swapped.delta_kn[i][j], Some(-1.19));
    assert_eq!(swapped.regions[0].faster, Faster::B);
    assert_eq!(project_bytes(&app), before, "comparing writes nothing");
}

/// An excluded node empties the cells read from it, as in the blend; an
/// override on the segment is what the segment says.
#[test]
fn operands_are_read_through_their_overlays() {
    let root = TempRoot::new("compare-overlay");
    let app = app(&root);
    polar_edit::polar_edit(
        &app,
        2,
        &EditOp::Type { bsp: Some(8.5) },
        &[{
            let c = run(&app, FILE, TRACK).unwrap();
            let (i, j) = at(&c, 90.0, 12.0);
            PolarCell {
                twa_index: i as u32,
                tws_index: j as u32,
            }
        }],
        None,
    )
    .unwrap();
    let c = run(&app, FILE, TRACK).unwrap();
    let (i, j) = at(&c, 90.0, 12.0);
    assert_eq!(c.delta_kn[i][j], Some(-0.5));

    polar3d::excluded_set(
        &app,
        &[PolarNodeRef {
            source_id: 1,
            twa_index: 1,
            tws_index: 1,
        }],
        &[],
        true,
    )
    .unwrap();
    let c = run(&app, FILE, TRACK).unwrap();
    assert_eq!(c.a[i][j], None, "read from the excluded 90°/10 kn node");
    assert_eq!((c.overlap, c.b_only), (0, 1));
    assert!(c.kn.mean_abs.is_none());
    edit::undo_last(&app).unwrap();
    assert_eq!(run(&app, FILE, TRACK).unwrap().overlap, 1);
}

/// The blend is the one the other views draw; a polar against itself is the
/// same everywhere; a hidden source can still be compared.
#[test]
fn the_blend_and_hidden_sources_are_operands() {
    let root = TempRoot::new("compare-blend");
    let app = app(&root);
    let blend = app
        .with_session(|s| {
            let open = s.require_open()?;
            Ok(open.derived.blend(&open.project).polar.clone())
        })
        .unwrap();
    let c = run(&app, CompareOperand::Blend, FILE).unwrap();
    assert_eq!(c.a, blend.bsp);
    let (i, _) = at(&c, 0.0, 6.0);
    assert!(c.class[i].iter().all(|k| *k == CellClass::ZeroRow));

    let same = run(&app, FILE, FILE).unwrap();
    assert!(same.delta_kn.iter().flatten().flatten().all(|d| *d == 0.0));
    assert!(same.regions.is_empty());

    edit::source_visible_set(&app, 1, false).unwrap();
    let hidden = run(&app, FILE, TRACK).unwrap();
    assert_eq!(hidden.overlap, 1);
}

#[test]
fn a_wrong_or_missing_operand_is_refused() {
    let root = TempRoot::new("compare-refused");
    let app = app(&root);
    let kind = |e: pe_app::error::AppError| e.kind();
    assert_eq!(
        kind(run(&app, CompareOperand::Segment { source_id: 1 }, FILE).unwrap_err()),
        "bad-option"
    );
    assert_eq!(
        kind(run(&app, CompareOperand::Polar { source_id: 2 }, FILE).unwrap_err()),
        "bad-option"
    );
    assert!(run(&app, CompareOperand::Polar { source_id: 99 }, FILE).is_err());
    let bad = app.with_session(|s| {
        let open = s.require_open()?;
        compare::compare_in(&open.project, &mut open.derived, FILE, FILE, -1.0)
    });
    assert!(bad.is_err());
}

/// The packed answer starts with the documented header.
#[test]
fn the_command_answers_a_packet() {
    let root = TempRoot::new("compare-packet");
    let app = app(&root);
    let bytes = compare::compare_bytes(&app, FILE, TRACK, 0.05).unwrap();
    assert_eq!(&bytes[0..4], b"PECM");
    let word = |k: usize| u32::from_le_bytes(bytes[k * 4..k * 4 + 4].try_into().unwrap());
    assert_eq!((word(2), word(3)), (19, 10), "the default output grid");
    assert_eq!(word(5), 1);
}

/// Compare reads a polar source with the project's interpolation rule, as
/// the blend does (spec.md 7.6, 11). With the track hidden the blend is the
/// file alone, so the file against the blend differs nowhere, between the
/// file's own nodes included. Read linearly, the file's 45°–90°–135° peak
/// at 10 kn would sit below the spline the blend drew.
#[test]
fn a_polar_operand_follows_the_projects_interpolation() {
    let root = TempRoot::new("compare-spline");
    let app = app(&root);
    edit::source_visible_set(&app, 2, false).unwrap();
    app.with_session(|session| {
        let open = session.require_open()?;
        let before = open.project.blend.clone();
        let mut after = before.clone();
        after.interpolation = pe_core::project::Interpolation::MonotoneSpline;
        open.apply(Command::SetBlendSettings {
            before: Box::new(before),
            after: Box::new(after),
        })?;
        Ok(())
    })
    .unwrap();

    let c = run(&app, FILE, CompareOperand::Blend).unwrap();
    // Between the file's nodes: 60° at 10 kn has a value on both sides.
    let (i, j) = at(&c, 60.0, 10.0);
    assert_eq!(c.class[i][j], CellClass::Both);
    assert!(c.overlap > 8, "cells between the file's nodes are compared");
    let mut differing = Vec::new();
    for (i, row) in c.delta_kn.iter().enumerate() {
        for (j, delta) in row.iter().enumerate() {
            if let Some(delta) = delta.filter(|d| *d != 0.0) {
                differing.push((c.twa[i], c.tws[j], delta));
            }
        }
    }
    assert!(
        differing.is_empty(),
        "the file read as the blend reads it, but (TWA, TWS, Δ) differ: {differing:?}"
    );
}
