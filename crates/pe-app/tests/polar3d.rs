#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The 3D polar view over IPC (spec.md 10): the scene Rust assembles, and
//! excluding and including polar nodes as undoable changes.

mod common;

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::edit;
use pe_app::polar3d::{self, PolarNodeRef, scene_of};
use pe_app::projects;
use pe_core::polar::{PolarFileFormat, PolarGrid};
use pe_core::{Colour, Command, Source, SourceId, SourceKind};

fn file_source(id: u64, label: &str, polar: PolarGrid) -> Source {
    Source::new(
        SourceId(id),
        label,
        Colour::parse("#4e79a7").unwrap(),
        SourceKind::PolarFile {
            matrix: None,
            format: PolarFileFormat::Expedition,
            file_name: format!("{label}.txt"),
            polar,
        },
    )
}

/// Two file polars: A (3 × 2, one empty cell) and B (2 × 1).
fn two_polars(root: &TempRoot) -> AppState {
    let app = root.state();
    projects::create(&app, "3D".to_owned(), None, false).unwrap();
    app.with_session(|session| {
        let open = session.require_open()?;
        let a = file_source(
            1,
            "A",
            PolarGrid {
                twa: vec![40.0, 90.0, 150.0],
                tws: vec![6.0, 12.0],
                bsp: vec![
                    vec![Some(4.0), Some(6.0)],
                    vec![Some(6.0), None],
                    vec![Some(5.0), Some(9.0)],
                ],
            },
        );
        let b = file_source(
            2,
            "B",
            PolarGrid {
                twa: vec![45.0, 135.0],
                tws: vec![10.0],
                bsp: vec![vec![Some(5.0)], vec![Some(7.0)]],
            },
        );
        open.apply(Command::AddSource {
            index: 0,
            source: Box::new(a),
        })?;
        open.apply(Command::AddSource {
            index: 1,
            source: Box::new(b),
        })?;
        // A clean history, so the tests count only their own entries.
        open.history.clear();
        Ok(())
    })
    .unwrap();
    app
}

fn scene(app: &AppState) -> polar3d::Scene {
    app.with_session(|session| Ok(scene_of(&session.require_open()?.project)))
        .unwrap()
}

fn node(source_id: u64, twa_index: u32, tws_index: u32) -> PolarNodeRef {
    PolarNodeRef {
        source_id,
        twa_index,
        tws_index,
    }
}

#[test]
fn every_visible_polar_gives_its_nodes_and_its_surface_over_its_own_axes() {
    let root = TempRoot::new("scene");
    let app = two_polars(&root);
    let scene = scene(&app);
    assert_eq!(scene.sources.len(), 2);
    assert_eq!(scene.sources[0].colour, 0x4e79a7);
    // A has 5 cells with a value (one empty), B has 2.
    assert_eq!(scene.nodes.len(), 7);
    let a_nodes: Vec<(f32, f32, f32)> = scene
        .nodes
        .iter()
        .filter(|n| n.source == 0)
        .map(|n| (n.twa, n.tws, n.bsp))
        .collect();
    assert_eq!(
        a_nodes,
        [
            (40.0, 6.0, 4.0),
            (40.0, 12.0, 6.0),
            (90.0, 6.0, 6.0),
            (150.0, 6.0, 5.0),
            (150.0, 12.0, 9.0)
        ]
    );
    assert!(scene.nodes.iter().all(|n| !n.excluded));
    assert!(scene.samples.is_empty());
    let surface = &scene.surfaces[0];
    assert_eq!(surface.twa, [40.0, 90.0, 150.0]);
    assert_eq!(surface.tws, [6.0, 12.0]);
    // The empty cell is a hole, not a zero.
    assert!(surface.bsp[3].is_nan());
    // Two sources' surfaces, then the blend's on the output grid.
    assert_eq!(scene.surfaces.len(), 3);
    assert_eq!(scene.surfaces[2].source, polar3d::BLEND_SOURCE);
    assert_eq!(scene.surfaces[2].tws.len(), 10);

    edit::source_visible_set(&app, 2, false).unwrap();
    let scene = self::scene(&app);
    assert_eq!(scene.sources.len(), 1);
    assert_eq!(scene.surfaces.len(), 2);
    assert_eq!(scene.nodes.len(), 5);

    // A hidden blend is not drawn (spec.md 8).
    pe_app::blend::blend_visible_set(&app, false).unwrap();
    assert_eq!(self::scene(&app).surfaces.len(), 1);
}

#[test]
fn the_packed_scene_starts_with_its_header() {
    let root = TempRoot::new("bytes");
    let app = two_polars(&root);
    let bytes = polar3d::scene_bytes(&app).unwrap();
    assert_eq!(&bytes[0..4], b"PE3D");
    assert_eq!(bytes[12], 7, "node count");
}

#[test]
fn excluding_nodes_of_two_sources_is_one_undoable_entry() {
    let root = TempRoot::new("exclude");
    let app = two_polars(&root);
    // Scenes hold NaN for empty cells, so they compare as bytes.
    let before = polar3d::scene_bytes(&app).unwrap();
    let summary = polar3d::excluded_set(
        &app,
        &[node(1, 2, 1), node(2, 0, 0), node(1, 0, 0)],
        &[],
        true,
    )
    .unwrap();
    assert!(summary.can_undo);
    assert_eq!(summary.undo_label.as_deref(), Some("Exclude polar nodes"));
    let excluded: Vec<(u32, f32, f32)> = scene(&app)
        .nodes
        .iter()
        .filter(|n| n.excluded)
        .map(|n| (n.source, n.twa, n.tws))
        .collect();
    assert_eq!(
        excluded,
        [(0, 40.0, 6.0), (0, 150.0, 12.0), (1, 45.0, 10.0)]
    );
    let history_len = app
        .with_session(|s| Ok(s.require_open()?.history.entries().len()))
        .unwrap();
    assert_eq!(history_len, 1);

    // The overlay holds exactly the axis values, in order.
    let cells = app
        .with_session(|s| {
            Ok(s.require_open()?.project.sources[0]
                .overlay
                .excluded_cells
                .iter()
                .map(|c| (c.twa, c.tws))
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert_eq!(cells, [(40.0, 6.0), (150.0, 12.0)]);

    edit::undo_last(&app).unwrap();
    assert_eq!(polar3d::scene_bytes(&app).unwrap(), before);
    edit::redo_next(&app).unwrap();
    assert_eq!(scene(&app).nodes.iter().filter(|n| n.excluded).count(), 3);
}

#[test]
fn including_restores_the_nodes_and_the_source_exactly() {
    let root = TempRoot::new("include");
    let app = two_polars(&root);
    let source = || {
        app.with_session(|s| Ok(s.require_open()?.project.sources[0].clone()))
            .unwrap()
    };
    let original = source();
    polar3d::excluded_set(&app, &[node(1, 0, 0), node(1, 0, 1)], &[], true).unwrap();
    // Including a mix of excluded and not-excluded nodes touches only the
    // excluded ones.
    let summary = polar3d::excluded_set(
        &app,
        &[node(1, 0, 0), node(1, 0, 1), node(1, 2, 0)],
        &[],
        false,
    )
    .unwrap();
    assert_eq!(summary.undo_label.as_deref(), Some("Include polar nodes"));
    assert_eq!(source(), original);
}

#[test]
fn a_selection_that_changes_nothing_records_nothing() {
    let root = TempRoot::new("noop");
    let app = two_polars(&root);
    let summary = polar3d::excluded_set(&app, &[node(1, 0, 0)], &[], false).unwrap();
    assert!(!summary.can_undo);
    // Sample ids that name no sample are refused, changing nothing (the
    // track pipeline is tested in tests/tracks.rs).
    assert!(polar3d::excluded_set(&app, &[], &[5, 6], true).is_err());
    assert!(!projects::summary(&app).unwrap().unwrap().can_undo);
}

#[test]
fn a_node_outside_the_grid_or_without_a_value_is_refused() {
    let root = TempRoot::new("bad");
    let app = two_polars(&root);
    for bad in [node(1, 3, 0), node(1, 0, 2), node(1, 1, 1), node(99, 0, 0)] {
        assert!(
            polar3d::excluded_set(&app, &[node(2, 0, 0), bad], &[], true).is_err(),
            "{bad:?}"
        );
    }
    // A refusal changes nothing, not even the good node beside it.
    assert!(scene(&app).nodes.iter().all(|n| !n.excluded));
}

#[test]
fn a_track_has_no_polar_nodes_to_exclude() {
    let root = TempRoot::new("track");
    let app = two_polars(&root);
    app.with_session(|session| {
        let open = session.require_open()?;
        let origin = pe_core::track::TrackOrigin::File {
            name: "race.csv".to_owned(),
            boat_name: None,
        };
        let track = Source::new(
            SourceId(3),
            "Track",
            Colour::parse("#e15759").unwrap(),
            SourceKind::Track {
                track: Box::new(pe_core::track::Track::new(pe_core::TrackId(4), origin)),
            },
        );
        open.apply(Command::AddSource {
            index: 2,
            source: Box::new(track),
        })
    })
    .unwrap();
    assert!(polar3d::excluded_set(&app, &[node(3, 0, 0)], &[], true).is_err());
    let scene = scene(&app);
    assert_eq!(scene.sources.len(), 3);
    assert_eq!(scene.sources[2].kind, polar3d::KIND_TRACK);
    // Two polar surfaces and the blend; the track has none of its own.
    assert_eq!(scene.surfaces.len(), 3);
}

/// One instant, 12:00 UTC on 2025-07-26 (1,753,531,200 s), at four
/// longitudes: 02:00, 11:40, 12:40 and 18:00 local solar time (15° an
/// hour). The 3D scene, its flags-only form and the 2D dots all carry the
/// band (spec.md 9.2, 10.2).
#[test]
fn samples_carry_their_band_of_the_local_solar_day() {
    use pe_core::track::{Fix, Sample, Track, TrackOrigin};
    use pe_tracks::daytime::DayBand;

    let root = TempRoot::new("day-band");
    let app = two_polars(&root);
    app.with_session(|session| {
        let open = session.require_open()?;
        let mut track = Track::new(
            pe_core::TrackId(4),
            TrackOrigin::File {
                name: "race.csv".to_owned(),
                boat_name: None,
            },
        );
        for (k, lon) in [-150.0, -5.0, 10.0, 90.0].into_iter().enumerate() {
            let fix = Fix {
                tws: None,
                twd_from: None,
                t: 1_753_531_200,
                lat: 50.0,
                lon,
                cog: None,
                sog: None,
            };
            let mut sample = Sample::at(pe_core::SampleId(200 + k as u64), k as u32, &fix);
            sample.twa = Some(90.0);
            sample.tws = Some(12.0);
            sample.speed = Some(6.0);
            sample.heading = Some(0.0);
            track.fixes.push(fix);
            track.samples.push(sample);
        }
        let mut source = Source::new(
            SourceId(3),
            "Track",
            Colour::parse("#e15759").unwrap(),
            SourceKind::Track {
                track: Box::new(track),
            },
        );
        source.overlay.filters.min_bsp_kn = None;
        source.overlay.filters.max_heading_change_deg = None;
        open.apply(Command::AddSource {
            index: 2,
            source: Box::new(source),
        })?;
        Ok(())
    })
    .unwrap();

    let expected = [
        DayBand::Night,
        DayBand::Morning,
        DayBand::Afternoon,
        DayBand::Evening,
    ];
    let full = scene(&app);
    assert_eq!(
        full.samples.iter().map(|s| s.band).collect::<Vec<_>>(),
        expected
    );

    let flags_only = app
        .with_session(|session| {
            let open = session.require_open()?;
            let derived = open.derived.visible(&open.project);
            Ok(polar3d::scene_with(
                &open.project,
                &derived,
                None,
                None,
                true,
            ))
        })
        .unwrap();
    assert_eq!(
        flags_only
            .samples
            .iter()
            .map(|s| s.band)
            .collect::<Vec<_>>(),
        expected
    );

    let dots = pe_app::polar_plot::dots(&app, None, false).unwrap();
    assert_eq!(dots.iter().map(|d| d.band).collect::<Vec<_>>(), expected);
}
