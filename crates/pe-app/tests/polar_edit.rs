#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Editing one source (spec.md 10.4) and track polar segments (spec.md
//! 12.1) over IPC, through the functions the Tauri commands call: every
//! tool is an undoable overlay change, every view reads it at once, and
//! removing every edit gives the source back byte for byte (invariant 1).

mod common;

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::polar_edit::{self, EditOp, PolarCell};
use pe_app::{edit, polar_plot, polar3d, projects};
use pe_core::polar::{PolarFileFormat, PolarGrid};
use pe_core::track::{Fix, Sample, Track, TrackOrigin};
use pe_core::{Colour, Command, SampleId, Source, SourceId, SourceKind, TrackId};

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

/// A track whose samples sit at 90° in 12 kn, BSP 6.0, 6.1, … 6.9 (ten
/// samples), plus three at 60° in 8 kn (below the minimum of five).
fn track_source(id: u64) -> Source {
    let mut track = Track::new(
        TrackId(id + 1),
        TrackOrigin::File {
            name: "race.csv".to_owned(),
            boat_name: None,
        },
    );
    let place = |k: u64| -> (f64, f64, f64) {
        if k < 10 {
            (90.0, 12.0, 6.0 + k as f64 / 10.0)
        } else {
            (60.0, 8.0, 5.0)
        }
    };
    for k in 0..13u64 {
        let fix = Fix {
            tws: None,
            twd_from: None,
            t: 1_753_531_200 + k as i64 * 60,
            lat: 50.0,
            lon: -5.0,
            cog: None,
            sog: None,
        };
        let (twa, tws, bsp) = place(k);
        let mut sample = Sample::at(SampleId(id * 100 + k), k as u32, &fix);
        sample.twa = Some(twa);
        sample.tws = Some(tws);
        sample.speed = Some(bsp);
        sample.heading = Some(0.0);
        track.fixes.push(fix);
        track.samples.push(sample);
    }
    Source::new(
        SourceId(id),
        "Track",
        Colour::parse("#e15759").unwrap(),
        SourceKind::Track {
            track: Box::new(track),
        },
    )
}

fn app(root: &TempRoot) -> AppState {
    let app = root.state();
    projects::create(&app, "Edit".to_owned(), None, false).unwrap();
    app.with_session(|session| {
        let open = session.require_open()?;
        open.project.next_id = 10_000;
        open.apply(Command::AddSource {
            index: 0,
            source: Box::new(file_source()),
        })?;
        open.apply(Command::AddSource {
            index: 1,
            source: Box::new(track_source(2)),
        })?;
        open.history.clear();
        Ok(())
    })
    .unwrap();
    app
}

fn cell(i: u32, j: u32) -> PolarCell {
    PolarCell {
        twa_index: i,
        tws_index: j,
    }
}

fn history_len(app: &AppState) -> usize {
    app.with_session(|s| Ok(s.require_open()?.history.cursor()))
        .unwrap()
}

fn project_bytes(app: &AppState) -> Vec<u8> {
    app.with_session(|s| Ok(pe_core::io::to_bytes(&s.require_open()?.project).unwrap()))
        .unwrap()
}

fn surface(app: &AppState, id: u64) -> polar_edit::EditSurface {
    polar_edit::edit_surface(app, id).unwrap()
}

#[test]
fn a_drag_is_one_undo_entry_and_undo_puts_the_node_back() {
    let root = TempRoot::new("edit-drag");
    let app = app(&root);
    for bsp in [9.2, 9.4, 9.55] {
        polar_edit::polar_edit(&app, 1, &EditOp::Drag { bsp }, &[cell(1, 1)], Some("g1")).unwrap();
    }
    assert_eq!(history_len(&app), 1);
    let summary = projects::summary(&app).unwrap().unwrap();
    assert_eq!(summary.undo_label.as_deref(), Some("Move polar node"));
    assert_eq!(summary.sources[0].edits, 1);
    let edited = surface(&app, 1);
    assert_eq!(edited.bsp[1][1], Some(9.55));
    assert_eq!(edited.source[1][1], Some(9.0));
    assert!(edited.edited[1][1] && !edited.edited[0][0]);

    // A second drag is a second entry.
    polar_edit::polar_edit(
        &app,
        1,
        &EditOp::Drag { bsp: 8.0 },
        &[cell(1, 1)],
        Some("g2"),
    )
    .unwrap();
    assert_eq!(history_len(&app), 2);
    edit::undo_last(&app).unwrap();
    edit::undo_last(&app).unwrap();
    assert_eq!(surface(&app, 1).bsp[1][1], Some(9.0));
    assert_eq!(surface(&app, 1).edit_count, 0);
}

#[test]
fn every_tool_is_one_entry_and_reset_all_restores_the_source_byte_for_byte() {
    let root = TempRoot::new("edit-tools");
    let app = app(&root);
    let original = project_bytes(&app);

    // Two typed values: one entry each.
    polar_edit::polar_edit(
        &app,
        1,
        &EditOp::Type { bsp: Some(4.25) },
        &[cell(0, 0)],
        None,
    )
    .unwrap();
    polar_edit::polar_edit(
        &app,
        1,
        &EditOp::Type { bsp: Some(6.2) },
        &[cell(2, 1)],
        None,
    )
    .unwrap();
    assert_eq!(history_len(&app), 2);
    // Typing into the hole fills it: the user's value.
    assert_eq!(surface(&app, 1).bsp[2][1], Some(6.2));

    // Scale +10 % over three cells: one entry, each × 1.1 of its edited value.
    polar_edit::polar_edit(
        &app,
        1,
        &EditOp::Scale { percent: 10.0 },
        &[cell(0, 0), cell(0, 1), cell(1, 1)],
        None,
    )
    .unwrap();
    assert_eq!(history_len(&app), 3);
    let scaled = surface(&app, 1);
    assert!((scaled.bsp[0][0].unwrap() - 4.675).abs() < 1e-9);
    assert!((scaled.bsp[0][1].unwrap() - 5.5).abs() < 1e-9);
    assert!((scaled.bsp[1][1].unwrap() - 9.9).abs() < 1e-9);

    // Smooth, then reset two cells: one entry each.
    polar_edit::polar_edit(&app, 1, &EditOp::Smooth, &[cell(1, 1)], None).unwrap();
    assert_eq!(history_len(&app), 4);
    assert!(surface(&app, 1).bsp[1][1].unwrap() < 9.9);
    polar_edit::polar_edit(&app, 1, &EditOp::Reset, &[cell(0, 0), cell(1, 1)], None).unwrap();
    assert_eq!(history_len(&app), 5);
    let reset = surface(&app, 1);
    assert_eq!(reset.bsp[0][0], Some(4.0));
    assert!(!reset.edited[1][1]);
    let summary = projects::summary(&app).unwrap().unwrap();
    assert_eq!(summary.undo_label.as_deref(), Some("Reset polar cells"));

    // Reset all: one entry, and the project is byte for byte as it was.
    polar_edit::polar_edit(&app, 1, &EditOp::ResetAll, &[], None).unwrap();
    assert_eq!(history_len(&app), 6);
    assert_eq!(project_bytes(&app), original);
    // Undo brings every edit back; a reset of nothing records nothing.
    edit::undo_last(&app).unwrap();
    assert_eq!(surface(&app, 1).edit_count, 2);
    edit::redo_next(&app).unwrap();
    polar_edit::polar_edit(&app, 1, &EditOp::ResetAll, &[], None).unwrap();
    assert_eq!(history_len(&app), 6);
}

#[test]
fn bad_edits_are_refused_and_change_nothing() {
    let root = TempRoot::new("edit-bad");
    let app = app(&root);
    let before = project_bytes(&app);
    let bad: [(EditOp, Vec<PolarCell>); 5] = [
        (EditOp::Type { bsp: Some(-1.0) }, vec![cell(0, 0)]),
        (EditOp::Type { bsp: Some(70.0) }, vec![cell(0, 0)]),
        (
            EditOp::Type { bsp: Some(5.0) },
            vec![cell(0, 0), cell(0, 1)],
        ),
        (EditOp::Drag { bsp: 5.0 }, vec![cell(3, 0)]),
        (EditOp::Scale { percent: -100.0 }, vec![cell(0, 0)]),
    ];
    for (op, cells) in bad {
        assert!(
            polar_edit::polar_edit(&app, 1, &op, &cells, None).is_err(),
            "{op:?}"
        );
    }
    assert!(polar_edit::polar_edit(&app, 99, &EditOp::Smooth, &[cell(0, 0)], None).is_err());
    assert_eq!(project_bytes(&app), before);
    assert_eq!(history_len(&app), 0);
}

/// Spec.md 12.1: the segment keeps count and spread per cell, a cell below
/// five samples is empty, the statistic is the track's own (p90 by
/// default), and excluded or filtered samples are left out.
#[test]
fn a_track_segment_bins_its_samples_with_its_statistic() {
    let root = TempRoot::new("edit-segment");
    let app = app(&root);
    let segment = surface(&app, 2);
    assert_eq!(segment.kind, "track");
    assert_eq!(segment.statistic.as_deref(), Some("p90"));
    assert_eq!(segment.min_samples, 5);
    let i = segment.twa.iter().position(|v| *v == 90.0).unwrap();
    let j = segment.tws.iter().position(|v| *v == 12.0).unwrap();
    let (i60, j8) = (
        segment.twa.iter().position(|v| *v == 60.0).unwrap(),
        segment.tws.iter().position(|v| *v == 8.0).unwrap(),
    );
    let count = segment.count.as_ref().unwrap();
    assert_eq!(count[i][j], 10);
    assert_eq!(count[i60][j8], 3);
    // 6.0 … 6.9: the 90th percentile at rank 8.1 is 6.81.
    assert!((segment.bsp[i][j].unwrap() - 6.81).abs() < 1e-9);
    assert_eq!(segment.bsp[i60][j8], None, "three samples are below five");
    // Their spread: the sample standard deviation of 6.0 … 6.9. The mean is
    // 6.45; the squares sum to (0.45² + 0.35² + … + 0.45²) = 0.825, over 9
    // is 11/120, whose square root is 0.302765035409749…
    let spread = segment.spread.as_ref().unwrap()[i][j].unwrap();
    assert!((spread - 0.302_765_035_409_749).abs() < 1e-12, "{spread}");

    // The median instead, undoably.
    let summary = polar_edit::segment_statistic_set(&app, 2, "median").unwrap();
    assert_eq!(
        summary.undo_label.as_deref(),
        Some("Change segment statistic")
    );
    assert!((surface(&app, 2).bsp[i][j].unwrap() - 6.45).abs() < 1e-9);
    assert!(polar_edit::segment_statistic_set(&app, 2, "p99").is_err());
    assert!(polar_edit::segment_statistic_set(&app, 1, "mean").is_err());

    // Excluding six samples leaves four: the cell is empty.
    let ids: Vec<u64> = (0..6).map(|k| 200 + k).collect();
    polar3d::excluded_set(&app, &[], &ids, true).unwrap();
    let fewer = surface(&app, 2);
    assert_eq!(fewer.count.as_ref().unwrap()[i][j], 4);
    assert_eq!(fewer.bsp[i][j], None);

    // An edit on the segment is an override on the track's overlay, on the
    // output grid, and it holds the cell whatever the samples say.
    polar_edit::polar_edit(
        &app,
        2,
        &EditOp::Type { bsp: Some(7.0) },
        &[cell(i as u32, j as u32)],
        None,
    )
    .unwrap();
    let edited = surface(&app, 2);
    assert_eq!(edited.bsp[i][j], Some(7.0));
    assert_eq!(edited.source[i][j], None);
    assert!(edited.edited[i][j]);
}

/// Every edit shows in every open view at once (spec.md 10.4): the 3D
/// nodes and surfaces (with the edited flag), the 2D curves, and in edit
/// mode on a track, its segment as its surface.
#[test]
fn an_edit_reaches_the_3d_scene_and_the_2d_curves() {
    let root = TempRoot::new("edit-views");
    let app = app(&root);
    polar_edit::polar_edit(
        &app,
        1,
        &EditOp::Type { bsp: Some(8.0) },
        &[cell(1, 1)],
        None,
    )
    .unwrap();
    let bytes = polar3d::scene_bytes(&app).unwrap();
    let word = |b: &[u8], i: usize| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
    assert_eq!(word(&bytes, 1), polar3d::SCENE_VERSION);
    let nodes = word(&bytes, 3) as usize;
    assert_eq!(nodes, 8, "the file's eight cells with a value");
    let sources = word(&bytes, 2) as usize;
    let at = 12 + sources * 4;
    // Node 4 is (90°, 10 kn): TWA-major order over the file's cells.
    let bsp = f32::from_bits(word(&bytes, at + 4 * 3 + 2));
    assert_eq!(bsp, 8.0);
    let flags = word(&bytes, at + nodes * 5 + 4);
    assert_eq!(flags, polar3d::FLAG_EDITED);

    let curves = polar_plot::plot(&app, Some(10.0)).unwrap().curves;
    let file = curves.iter().find(|c| c.label == "File").unwrap();
    assert_eq!(file.points[1].bsp, 8.0);

    // In edit mode on the track, its segment joins the scene.
    let plain = polar3d::scene_bytes_for(&app, None, None).unwrap();
    let focused = polar3d::scene_bytes_for(&app, Some(2), None).unwrap();
    assert_eq!(word(&plain, 5), 2, "the file's surface and the blend's");
    assert_eq!(word(&focused, 5), 3, "and the track's segment");
    assert_eq!(word(&focused, 3), 8 + 1, "and its one cell with a value");
}

/// The samples section travels only when samples moved (M7 carry): an edit
/// or an exclusion sends the flags alone; a change of corrected or ground
/// values sends everything.
#[test]
fn only_the_flags_travel_when_no_sample_moved() {
    let root = TempRoot::new("edit-delta");
    let app = app(&root);
    let word = |b: &[u8], i: usize| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
    let key = |b: &[u8]| u64::from(word(b, 8)) | (u64::from(word(b, 9)) << 32);
    let full = polar3d::scene_bytes_for(&app, None, None).unwrap();
    assert_eq!(word(&full, 10), polar3d::SAMPLES_FULL);
    assert_eq!(word(&full, 4), 13, "every sample has its wind");
    let held = key(&full);

    polar_edit::polar_edit(
        &app,
        1,
        &EditOp::Type { bsp: Some(8.0) },
        &[cell(1, 1)],
        None,
    )
    .unwrap();
    polar3d::excluded_set(&app, &[], &[201], true).unwrap();
    let delta = polar3d::scene_bytes_for(&app, None, Some(held)).unwrap();
    assert_eq!(word(&delta, 10), polar3d::SAMPLES_FLAGS_ONLY);
    assert_eq!(key(&delta), held);
    assert!(delta.len() < full.len());
    let fresh = polar3d::scene_bytes_for(&app, None, None).unwrap();
    // The delta's nodes and surfaces are the full scene's; its flags are the
    // full scene's flags.
    let (s, n, m) = (word(&fresh, 2) as usize, word(&fresh, 3) as usize, 13);
    let nodes_end = 12 + s * 4 + n * 6;
    assert_eq!(fresh[48..nodes_end * 4], delta[48..nodes_end * 4]);
    // Thirteen columns of samples come before their flags (layout version 5).
    let flags_at = nodes_end + m * 13;
    assert_eq!(
        fresh[flags_at * 4..(flags_at + m) * 4],
        delta[nodes_end * 4..(nodes_end + m) * 4]
    );
    // Sample 201 is excluded. It was sailed at 12:01 UTC at 5°W, 11:41
    // local solar time: morning, band 1, which rides in the same flags.
    assert_eq!(
        word(&delta, nodes_end + 1),
        polar3d::FLAG_EXCLUDED | (1 << polar3d::FLAG_BAND_SHIFT)
    );

    // Corrected or ground values move every sample.
    pe_app::env::use_corrected_set(&app, false).unwrap();
    let moved = polar3d::scene_bytes_for(&app, None, Some(held)).unwrap();
    assert_eq!(word(&moved, 10), polar3d::SAMPLES_FULL);
    assert_ne!(key(&moved), held);
}

/// Smoothing reads neighbours as the blend does: a node excluded from the
/// blend takes no part. The file's (0, 0) is 4 with neighbours 5 (0, 1),
/// 5 (1, 0) and 9 (1, 1); with (1, 1) excluded, 4·4 + 2·5 + 2·5 = 36 over 8
/// is 4.5 (it would be 45 over 9 = 5 with it).
#[test]
fn smoothing_leaves_out_nodes_excluded_from_the_blend() {
    let root = TempRoot::new("edit-smooth-excluded");
    let app = app(&root);
    polar3d::excluded_set(
        &app,
        &[polar3d::PolarNodeRef {
            source_id: 1,
            twa_index: 1,
            tws_index: 1,
        }],
        &[],
        true,
    )
    .unwrap();
    polar_edit::polar_edit(&app, 1, &EditOp::Smooth, &[cell(0, 0)], None).unwrap();
    assert_eq!(surface(&app, 1).bsp[0][0], Some(4.5));
}

/// Undo and redo of an exclusion and of a statistic change re-bin the
/// segment each time (the derived cache follows the history both ways).
#[test]
fn undo_and_redo_rebin_the_segment() {
    let root = TempRoot::new("edit-undo-rebin");
    let app = app(&root);
    let at = |s: &polar_edit::EditSurface| {
        let i = s.twa.iter().position(|v| *v == 90.0).unwrap();
        let j = s.tws.iter().position(|v| *v == 12.0).unwrap();
        (s.count.as_ref().unwrap()[i][j], s.bsp[i][j])
    };
    let original = at(&surface(&app, 2));
    assert_eq!(original.0, 10);

    polar3d::excluded_set(&app, &[], &[200, 201, 202], true).unwrap();
    let excluded = at(&surface(&app, 2));
    // 6.3 … 6.9: the 90th percentile at rank 5.4 is 6.84.
    assert_eq!(excluded.0, 7);
    assert!((excluded.1.unwrap() - 6.84).abs() < 1e-9);
    edit::undo_last(&app).unwrap();
    assert_eq!(at(&surface(&app, 2)), original);
    edit::redo_next(&app).unwrap();
    assert_eq!(at(&surface(&app, 2)), excluded);
    edit::undo_last(&app).unwrap();

    polar_edit::segment_statistic_set(&app, 2, "mean").unwrap();
    let mean = at(&surface(&app, 2));
    assert!((mean.1.unwrap() - 6.45).abs() < 1e-9);
    edit::undo_last(&app).unwrap();
    assert_eq!(at(&surface(&app, 2)), original);
    edit::redo_next(&app).unwrap();
    assert_eq!(at(&surface(&app, 2)), mean);
}

/// Removing a source drops its derived data; undo brings the source back.
#[test]
fn a_removed_source_leaves_the_derived_cache() {
    let root = TempRoot::new("edit-prune");
    let app = app(&root);
    surface(&app, 1);
    surface(&app, 2);
    let cached = || {
        app.with_session(|s| Ok(s.require_open()?.derived.cached()))
            .unwrap()
    };
    assert_eq!(cached(), 2);
    edit::source_remove(&app, 2).unwrap();
    assert_eq!(cached(), 1);
    edit::undo_last(&app).unwrap();
    assert_eq!(
        surface(&app, 2)
            .count
            .as_ref()
            .unwrap()
            .iter()
            .flatten()
            .sum::<u32>(),
        13
    );
}

/// Opening the same project again gives a new samples key, so a view that
/// names the old one gets the whole scene, never flags onto old positions.
#[test]
fn reopening_a_project_changes_the_samples_key() {
    let root = TempRoot::new("edit-reopen");
    let app = app(&root);
    let word = |b: &[u8], i: usize| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
    let key = |b: &[u8]| u64::from(word(b, 8)) | (u64::from(word(b, 9)) << 32);
    let path = root.file("edit.wpsproj");
    projects::save_as(&app, path.clone()).unwrap();
    let first = polar3d::scene_bytes_for(&app, None, None).unwrap();
    projects::open(&app, path, true).unwrap();
    let again = polar3d::scene_bytes_for(&app, None, Some(key(&first))).unwrap();
    assert_eq!(word(&again, 10), polar3d::SAMPLES_FULL);
    assert_ne!(key(&again), key(&first));
}
