#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The blend, its settings and its export end to end (spec.md 8, 12, 6),
//! through the functions the Tauri commands call.
//!
//! **The CI matrix compares these hashes** (invariant 5, plan.md M14): a
//! fixed project — an ORC certificate, an edited polar file with an excluded
//! node, a track with current-corrected samples, and a hidden source — is
//! exported in all three formats, and the SHA-256 of each file is pinned
//! here. Every target that runs the suite (macOS Intel and Apple silicon,
//! Windows x64, Linux) must produce the same bytes. Changing them is
//! changing export output (CLAUDE.md, "Changing export output"). They
//! were re-pinned in M14e only because the fixture's track now holds its
//! wind and current and derives TWA and the corrected values from them on
//! load, as every saved track does (D27); the writers did not change.

mod common;

use common::TempRoot;
use pe_app::blend::{self, BlendSettingsInput, ExportAxes};
use pe_app::{edit, projects};
use pe_core::orc::{OrcRecord, OrcSize, OrcVpp};
use pe_core::polar::PolarFileFormat;
use pe_core::source::{CellOverride, CellRef};
use pe_core::track::{Fix, Sample, Track, TrackOrigin};
use pe_core::{Boat, Colour, Command, Project, SampleId, Source, SourceKind, TrackId};
use sha2::{Digest, Sha256};

const EXPEDITION: &[u8] = include_bytes!("../../pe-polar/tests/golden/expedition.txt");

/// The pinned SHA-256 of the fixed project's exports.
const EXPEDITION_SHA256: &str = "504aba8479df506ec9461c61ea47d782e816a2d2ff7dbee7233eea499cde6d82";
const ADRENA_SHA256: &str = "735104c2a0a35d521f84feade4fe6f8d3f6b9d671906525f95b39c30ed63d220";
const CSV_SHA256: &str = "47f9fbd5fef3684b1fdb09483faaee0b0f71db543a5ba857471035a99ca1f461";

fn orc() -> OrcRecord {
    let angles = vec![52.0, 60.0, 75.0, 90.0, 110.0, 120.0, 135.0, 150.0];
    let speeds = vec![6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0];
    // Boat speeds from a formula in hundredths, so the fixture is plain
    // data: faster with wind, fastest near a beam reach.
    let bsp = angles
        .iter()
        .map(|a: &f64| {
            speeds
                .iter()
                .map(|s: &f64| {
                    let reach = 100.0 - (a - 105.0).abs();
                    let hundredths = (300.0 + s * 38.0 + reach * 1.5 - s * s * 0.6).round();
                    Some(hundredths / 100.0)
                })
                .collect()
        })
        .collect();
    OrcRecord {
        ref_no: Some("EXPORT0001".to_owned()),
        sail_no: "GBR 1".to_owned(),
        country: "GBR".to_owned(),
        name: "Export Fixture".to_owned(),
        model: Some("Fixture 40".to_owned()),
        builder: None,
        designer: None,
        year: Some(2020),
        certificate_year: Some(2025),
        size: OrcSize::default(),
        gph: Some(560.0),
        osn: None,
        vpp: OrcVpp {
            angles,
            speeds,
            bsp,
            beat_angle: vec![44.5, 42.0, 40.5, 39.0, 38.5, 38.0, 38.5],
            beat_vmg: vec![3.9, 4.6, 5.1, 5.45, 5.7, 5.85, 6.0],
            run_angle: vec![142.0, 146.0, 150.0, 160.0, 168.0, 172.0, 176.0],
            run_vmg: vec![4.1, 5.0, 5.8, 6.5, 7.1, 7.6, 8.6],
        },
    }
}

/// A track of 600 samples over the polar, every second one with a current
/// correction, all from integer arithmetic. The samples hold what a track
/// stores — motion, wind and current — and the TWA, TWS and corrected
/// values the blend reads are derived from them when the project is read
/// back (schema 2, D27): heading 000° at the boat speed, the wind from the
/// TWA, and on every second sample 0.2 kn of current setting north.
fn track() -> Track {
    let mut track = Track::new(
        TrackId(20),
        TrackOrigin::File {
            name: "race.csv".to_owned(),
            boat_name: None,
        },
    );
    for k in 0..600u32 {
        let fix = Fix {
            tws: None,
            twd_from: None,
            t: 1_750_000_000 + i64::from(k) * 60,
            lat: 50.0,
            lon: -1.0,
            cog: None,
            sog: None,
        };
        let mut sample = Sample::at(SampleId(1_000 + u64::from(k)), k, &fix);
        let twa = 30.0 + f64::from(k * 7 % 150);
        let tws = 5.0 + f64::from(k * 3 % 22) + 0.25;
        let bsp =
            (300.0 + tws * 30.0 + (twa - 30.0) * 1.2 + f64::from(k % 5) * 10.0).round() / 100.0;
        sample.heading = Some(0.0);
        sample.speed = Some(bsp);
        sample.twd_from = Some(twa);
        sample.tws = Some(tws);
        if k % 2 == 0 {
            sample.current_speed = Some(0.2);
            sample.current_toward = Some(0.0);
        }
        sample.relate();
        track.fixes.push(fix);
        track.samples.push(sample);
    }
    track
}

/// The fixed project, read back through its own file format, as a person's
/// saved project would be.
fn fixed_project() -> Project {
    let mut project = Project::new("Export fixture", Boat::default(), 1_700_000_000);
    let colour = |c: &str| Colour::parse(c).unwrap();
    let mut file = Source::new(
        pe_core::SourceId(2),
        "Expedition",
        colour("#f28e2b"),
        SourceKind::PolarFile {
            format: PolarFileFormat::Expedition,
            file_name: "expedition.txt".to_owned(),
            polar: pe_polar::read(EXPEDITION).unwrap().polar,
        },
    );
    file.weight = 0.75;
    file.overlay.cell_overrides = vec![CellOverride {
        twa: 90.0,
        tws: 16.0,
        bsp: 8.6,
    }];
    file.overlay.excluded_cells = vec![CellRef {
        twa: 110.0,
        tws: 10.0,
    }];
    let mut track = Source::new(
        pe_core::SourceId(3),
        "Track",
        colour("#e15759"),
        SourceKind::Track {
            track: Box::new(track()),
        },
    );
    track.overlay.filters.min_bsp_kn = None;
    track.overlay.filters.max_heading_change_deg = None;
    let mut hidden = Source::new(
        pe_core::SourceId(4),
        "Hidden",
        colour("#76b7b2"),
        SourceKind::PolarFile {
            format: PolarFileFormat::Expedition,
            file_name: "hidden.txt".to_owned(),
            polar: pe_polar::read(b"10 90 20\n").unwrap().polar,
        },
    );
    hidden.visible = false;
    hidden.weight = 1.0;
    project.sources = vec![
        Source::new(
            pe_core::SourceId(1),
            "ORC",
            colour("#4e79a7"),
            SourceKind::Orc {
                record: Box::new(orc()),
            },
        ),
        file,
        track,
        hidden,
    ];
    project.next_id = 10_000;
    pe_core::io::from_bytes(&pe_core::io::to_bytes(&project).unwrap()).unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn exports(project: &Project) -> [(&'static str, Vec<u8>); 3] {
    ["expedition", "adrena", "csv"].map(|f| (f, blend::export_bytes(project, f, None).unwrap()))
}

/// The pinned hashes (see the module documentation). Every export also
/// reads back as a polar.
#[test]
fn the_fixed_project_exports_the_same_bytes_on_every_platform() {
    let project = fixed_project();
    let pinned = [EXPEDITION_SHA256, ADRENA_SHA256, CSV_SHA256];
    let exports = exports(&project);
    let hashes: Vec<String> = exports.iter().map(|(_, bytes)| sha256(bytes)).collect();
    for (((format, bytes), hash), pinned) in exports.iter().zip(&hashes).zip(pinned) {
        let parsed = pe_polar::read(bytes).unwrap();
        assert!(pe_polar::cell_count(&parsed.polar) > 100, "{format}");
        assert_eq!(
            hash,
            pinned,
            "{format} export changed (every hash: {hashes:?}):\n{}",
            String::from_utf8_lossy(bytes)
        );
    }
}

/// Reordering the list (display only, spec.md 8) and saving and reloading
/// change no byte; a hidden source changes nothing either.
#[test]
fn list_order_saving_and_hidden_sources_change_no_byte() {
    let project = fixed_project();
    let reference = exports(&project);
    let mut reordered = project.clone();
    reordered.sources.reverse();
    assert_eq!(exports(&reordered), reference);
    let reloaded = pe_core::io::from_bytes(&pe_core::io::to_bytes(&project).unwrap()).unwrap();
    assert_eq!(exports(&reloaded), reference);
    let mut without_hidden = project.clone();
    without_hidden.sources.pop();
    assert_eq!(exports(&without_hidden), reference);
}

fn open_fixed(root: &TempRoot) -> pe_app::commands::AppState {
    let app = root.state();
    projects::create(&app, "Export".to_owned(), None, false).unwrap();
    let fixed = fixed_project();
    app.with_session(|session| {
        let open = session.require_open()?;
        for (index, source) in fixed.sources.iter().enumerate() {
            let mut source = source.clone();
            source.id = open.project.allocate_source_id();
            if let SourceKind::Track { track } = &mut source.kind {
                track.id = open.project.allocate_track_id();
                for sample in &mut track.samples {
                    sample.id = open.project.allocate_sample_id();
                }
            }
            open.apply(Command::AddSource {
                index,
                source: Box::new(source),
            })?;
        }
        Ok(())
    })
    .unwrap();
    app
}

/// Export writes the file, recomputed from the sources: a change made
/// behind the session's cache (no command, no revision) is still what is
/// written (invariant 2).
#[test]
fn export_writes_the_file_and_always_recomputes() {
    let root = TempRoot::new("export-write");
    let app = open_fixed(&root);
    let path = root.file("blend.pol");
    let written = blend::export_to(&app, &path, "adrena", None).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(written.bytes as usize, bytes.len());
    assert_eq!(
        bytes,
        blend::export_bytes(&fixed_project(), "adrena", None).unwrap()
    );

    let summary_before = projects::summary(&app).unwrap().unwrap().blend;
    app.with_session(|session| {
        let open = session.require_open()?;
        // The polar file's own grid, rewritten without a command: the
        // session's cache has no way to know.
        if let SourceKind::PolarFile { polar, .. } = &mut open.project.sources[1].kind {
            for cell in polar.bsp.iter_mut().flatten().flatten() {
                *cell += 1.0;
            }
        }
        Ok(())
    })
    .unwrap();
    blend::export_to(&app, &path, "adrena", None).unwrap();
    assert_ne!(std::fs::read(&path).unwrap(), bytes, "recomputed");
    // The views' cache has not seen it; export did not use the cache.
    assert_eq!(
        projects::summary(&app).unwrap().unwrap().blend,
        summary_before
    );
}

/// The preview shows the grid and the text; a custom grid is the blend
/// resampled; a grid that would not read back is refused with the values
/// named, and nothing is written.
#[test]
fn the_preview_offers_the_project_grid_or_a_custom_one() {
    let root = TempRoot::new("export-preview");
    let app = open_fixed(&root);
    let preview = blend::preview(&app, "expedition", None).unwrap();
    assert!(preview.problem.is_none());
    assert_eq!(preview.twa.len(), 19);
    assert_eq!(preview.tws.len(), 10);
    let origin = preview.origin.unwrap();
    assert_eq!(origin[0][0], "filled", "the 0° row");
    assert!(origin.iter().flatten().any(|o| o == "direct"));
    assert_eq!(
        preview.text.as_bytes(),
        blend::export_bytes(&fixed_project(), "expedition", None).unwrap()
    );

    let axes = ExportAxes {
        twa: vec![0.0, 45.0, 90.0, 135.0, 180.0],
        tws: vec![8.0, 12.0, 16.0],
    };
    let custom = blend::preview(&app, "csv", Some(&axes)).unwrap();
    assert!(custom.origin.is_none());
    assert_eq!(custom.bsp.len(), 5);
    let project = blend::preview(&app, "csv", None).unwrap();
    // 90° at 12 kn is a node of both grids: the same value.
    assert_eq!(custom.bsp[2][1], project.bsp[10][4]);
    assert!(custom.text.starts_with("TWA\\TWS;8;12;16\n"));
    // The 0° row is 0 kn and no anchor: 45° is read between the project
    // grid's 40° and 45° rows, never from 0°.
    assert_eq!(custom.bsp[0], vec![Some(0.0); 3]);
    assert_eq!(custom.bsp[1][1], project.bsp[4][4]);

    let colliding = ExportAxes {
        twa: vec![40.0, 40.004],
        tws: vec![10.0],
    };
    assert!(blend::preview(&app, "csv", Some(&colliding)).is_err());
    let path = root.file("never.csv");
    assert!(blend::export_to(&app, &path, "csv", Some(&colliding)).is_err());
    assert!(!std::path::Path::new(&path).exists());
    assert!(blend::preview(&app, "gpx", None).is_err());
}

/// A project grid that would write two axis values as one — possible only
/// in a file edited by hand, since the Blend settings refuse it — is
/// refused at export with the two values named (plan.md M14, from M4).
#[test]
fn a_colliding_project_grid_is_refused_at_export_with_its_values_named() {
    let mut project = fixed_project();
    project.grid.twa = vec![0.0, 42.001, 42.004, 90.0, 180.0];
    let preview = blend::preview_of(&project, "adrena", None).unwrap();
    let problem = preview.problem.unwrap();
    assert_eq!(problem.code, "axis-collision");
    assert_eq!(problem.axis.as_deref(), Some("twa"));
    assert_eq!(
        (problem.first, problem.second),
        (Some(42.001), Some(42.004))
    );
    assert_eq!(problem.written.as_deref(), Some("42"));
    assert!(preview.text.is_empty());
    let refused = blend::export_bytes(&project, "adrena", None).unwrap_err();
    assert_eq!(refused.kind(), "export-refused");
    assert!(refused.to_string().contains("42.001 and 42.004"));
}

fn input(summary: &pe_app::blend::BlendSummary) -> BlendSettingsInput {
    BlendSettingsInput {
        asymmetric: summary.asymmetric,
        interpolation: summary.interpolation.clone(),
        twa: summary.twa.clone(),
        tws: summary.tws.clone(),
        min_samples: summary.min_samples,
        n_full: summary.n_full,
        smoothing: summary.smoothing,
        default_statistic: summary.default_statistic.clone(),
        polar_statistic: Some(summary.polar_statistic.clone()),
        use_corrected: true,
    }
}

#[test]
fn asymmetric_blend_corrections_are_independent_undoable_and_saved_as_overlays() {
    use pe_app::polar_edit::PolarCell;
    use pe_app::polar_edit::{self, EditOp};
    let root = TempRoot::new("asymmetric-corrections");
    let app = open_fixed(&root);
    let before = projects::summary(&app).unwrap().unwrap();
    let original_sources = app
        .with_session(|s| Ok(s.require_open()?.project.sources.clone()))
        .unwrap();
    let mut settings = input(&before.blend);
    settings.asymmetric = true;
    settings.interpolation = "monotone_spline".into();
    settings.twa = vec![0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0, 360.0];
    settings.tws = vec![6.0, 12.0];
    let changed = blend::blend_settings_set(&app, settings).unwrap();
    assert!(changed.blend.asymmetric);
    assert_eq!(changed.blend.interpolation, "monotone_spline");
    let unedited = polar_edit::edit_surface(&app, 0).unwrap();
    let port = PolarCell {
        twa_index: 6,
        tws_index: 1,
    };
    let corrected =
        polar_edit::polar_edit(&app, 0, &EditOp::Type { bsp: Some(12.34) }, &[port], None).unwrap();
    assert_eq!(corrected.undo_label.as_deref(), Some("Correct the blend"));
    assert_eq!(corrected.blend.correction_count, 1);
    let surface = polar_edit::edit_surface(&app, 0).unwrap();
    assert_eq!(surface.bsp[6][1], Some(12.34));
    assert_eq!(
        surface.bsp[2][1], unedited.bsp[2][1],
        "starboard is independent"
    );
    assert_eq!(
        surface.source, unedited.source,
        "the baseline is always recomputed without corrections"
    );
    edit::undo_last(&app).unwrap();
    assert_eq!(polar_edit::edit_surface(&app, 0).unwrap().bsp, unedited.bsp);
    edit::redo_next(&app).unwrap();
    let preview = blend::preview(&app, "adrena", None).unwrap();
    let imported = pe_polar::read(preview.text.as_bytes()).unwrap().polar;
    assert_eq!(imported.twa, changed.blend.twa);
    assert_eq!(imported.bsp[6][1], Some(12.34));
    assert_eq!(imported.bsp[0], [Some(0.0), Some(0.0)]);
    assert_eq!(imported.bsp[8], [Some(0.0), Some(0.0)]);
    app.with_session(|s| {
        let project = &s.require_open()?.project;
        assert_eq!(project.sources, original_sources);
        let bytes = pe_core::io::to_bytes(project)?;
        let loaded = pe_core::io::from_bytes(&bytes)?;
        assert_eq!(loaded.blend.corrections.len(), 1);
        assert_eq!(pe_core::io::to_bytes(&loaded)?, bytes);
        assert_eq!(blend::fresh(&loaded), blend::fresh(project));
        Ok(())
    })
    .unwrap();
    polar_edit::polar_edit(&app, 0, &EditOp::ResetAll, &[], None).unwrap();
    assert_eq!(polar_edit::edit_surface(&app, 0).unwrap().bsp, unedited.bsp);
}

#[test]
fn priority_groups_round_trip_and_undo_without_changing_track_filters() {
    use pe_core::source::{Range, SampleFilters};
    let root = TempRoot::new("priority-save");
    let app = open_fixed(&root);
    let before = app
        .with_session(|s| Ok(s.require_open()?.project.clone()))
        .unwrap();
    let group = SampleFilters {
        wave_height_m: Some(Range {
            min: None,
            max: Some(1.5),
        }),
        ..SampleFilters::default()
    };
    let groups = vec![
        pe_app::tracks::TrackFilters::of(&group),
        pe_app::tracks::TrackFilters::of(&SampleFilters::default()),
    ];
    let changed = blend::priority_filters_set(&app, groups, 3).unwrap();
    assert_eq!(changed.blend.priority_groups.len(), 2);
    app.with_session(|s| {
        let project = &s.require_open()?.project;
        assert_eq!(project.sources, before.sources);
        let bytes = pe_core::io::to_bytes(project)?;
        assert_eq!(
            pe_core::io::to_bytes(&pe_core::io::from_bytes(&bytes)?)?,
            bytes
        );
        Ok(())
    })
    .unwrap();
    edit::undo_last(&app).unwrap();
    app.with_session(|s| {
        assert_eq!(s.require_open()?.project, before);
        Ok(())
    })
    .unwrap();
}

/// The dialog applies as one undo entry; the Blend entry's switch and
/// colour are entries of their own; undo restores each exactly; a bad grid
/// or count is refused and changes nothing.
#[test]
fn blend_settings_are_undoable_and_validated() {
    let root = TempRoot::new("blend-settings");
    let app = open_fixed(&root);
    let before = projects::summary(&app).unwrap().unwrap();
    assert!(before.blend.direct > 0 && before.blend.filled > 0);

    let mut change = input(&before.blend);
    change.tws = vec![6.0, 10.0, 14.0, 20.0];
    change.n_full = 10;
    change.smoothing = true;
    change.default_statistic = "median".to_owned();
    let after = blend::blend_settings_set(&app, change).unwrap();
    assert_eq!(after.undo_label.as_deref(), Some("Change blend settings"));
    assert_eq!(after.blend.tws, [6.0, 10.0, 14.0, 20.0]);
    assert_eq!(after.blend.default_statistic, "median");
    assert_eq!(
        after.blend.direct + after.blend.filled + after.blend.empty,
        18 * 4,
        "the 0° row counts in no coverage"
    );
    let undone = edit::undo_last(&app).unwrap();
    assert_eq!(undone.blend, before.blend);

    let hidden = blend::blend_visible_set(&app, false).unwrap();
    assert_eq!(hidden.undo_label.as_deref(), Some("Hide blend"));
    assert!(!hidden.blend.visible);
    let coloured = blend::blend_colour_set(&app, "#ff8800").unwrap();
    assert_eq!(coloured.undo_label.as_deref(), Some("Change blend colour"));
    assert_eq!(coloured.blend.colour, "#ff8800");
    assert!(blend::blend_colour_set(&app, "orange").is_err());
    edit::undo_last(&app).unwrap();
    assert_eq!(
        edit::undo_last(&app).unwrap().blend,
        before.blend,
        "both undone"
    );

    let revision = projects::summary(&app).unwrap().unwrap().revision;
    let mut bad = input(&before.blend);
    bad.twa = vec![0.0, 45.0, 45.004];
    assert!(blend::blend_settings_set(&app, bad).is_err());
    let mut bad = input(&before.blend);
    bad.n_full = 0;
    assert!(blend::blend_settings_set(&app, bad).is_err());
    let mut bad = input(&before.blend);
    bad.default_statistic = "p99".to_owned();
    assert!(blend::blend_settings_set(&app, bad).is_err());
    assert_eq!(projects::summary(&app).unwrap().unwrap().revision, revision);
    // Applying what is already there records nothing.
    let same = blend::blend_settings_set(&app, input(&before.blend)).unwrap();
    assert_eq!(same.revision, revision);
}

/// With no visible source the blend is its 0° row alone: no coverage, and
/// export refuses it as empty rather than writing a file of zeros (review
/// round 1).
#[test]
fn a_blend_of_no_visible_source_is_refused_as_empty() {
    let mut project = fixed_project();
    for source in &mut project.sources {
        source.visible = false;
    }
    let preview = blend::preview_of(&project, "adrena", None).unwrap();
    assert_eq!(preview.problem.unwrap().code, "empty");
    let refused = blend::export_bytes(&project, "expedition", None).unwrap_err();
    assert_eq!(refused.kind(), "export-refused");
    let coverage = blend::fresh(&project).coverage();
    assert_eq!((coverage.direct, coverage.filled), (0, 0));

    let root = TempRoot::new("export-empty");
    let app = root.state();
    projects::create(&app, "Empty".to_owned(), None, false).unwrap();
    let summary = projects::summary(&app).unwrap().unwrap().blend;
    assert_eq!((summary.direct, summary.filled, summary.empty), (0, 0, 180));
    assert!(blend::export_to(&app, &root.file("x.txt"), "expedition", None).is_err());
}

/// A track cell the person overrode counts with confidence 1, whatever its
/// sample count (D23 ruling): an override on a cell with no samples moves
/// the blend there.
#[test]
fn an_overridden_track_cell_counts_fully() {
    let mut project = fixed_project();
    project
        .sources
        .retain(|s| s.label == "Track" || s.label == "ORC");
    let (i, j) = (10, 4); // 90°, 12 kn
    assert_eq!((project.grid.twa[i], project.grid.tws[j]), (90.0, 12.0));
    // Every sample out: the track has no count anywhere.
    let track = project
        .sources
        .iter_mut()
        .find(|s| s.label == "Track")
        .unwrap();
    let ids: Vec<SampleId> = track
        .track()
        .unwrap()
        .samples
        .iter()
        .map(|s| s.id)
        .collect();
    track.overlay.excluded_samples = ids;
    let orc_only = blend::fresh(&project).polar.bsp[i][j].unwrap();
    let track = project
        .sources
        .iter_mut()
        .find(|s| s.label == "Track")
        .unwrap();
    track.overlay.cell_overrides = vec![CellOverride {
        twa: 90.0,
        tws: 12.0,
        bsp: 20.0,
    }];
    let vouched = blend::fresh(&project).polar.bsp[i][j].unwrap();
    assert!(
        (vouched - (orc_only + 20.0) / 2.0).abs() < 1e-6,
        "{vouched} vs {orc_only}"
    );
}

/// A track imported after the default statistic changed starts with it.
#[test]
fn a_new_track_takes_the_default_statistic() {
    let root = TempRoot::new("blend-default-statistic");
    let app = root.state();
    projects::create(&app, "Stat".to_owned(), None, false).unwrap();
    let summary = projects::summary(&app).unwrap().unwrap();
    let mut change = input(&summary.blend);
    change.default_statistic = "p75".to_owned();
    blend::blend_settings_set(&app, change).unwrap();
    let path = root.file("track.geojson");
    let features: Vec<String> = (0..3)
        .map(|k| {
            format!(
                r#"{{"type":"Feature","geometry":{{"type":"Point","coordinates":[-1.{k},50]}},"properties":{{"time":{}}}}}"#,
                1_753_531_200 + 600 * k
            )
        })
        .collect();
    std::fs::write(
        &path,
        format!(
            r#"{{"type":"FeatureCollection","features":[{}]}}"#,
            features.join(",")
        ),
    )
    .unwrap();
    pe_app::tracks::import(
        &app,
        &[pe_app::tracks::TrackFileRequest {
            path,
            mapping: None,
            boats: None,
        }],
    )
    .unwrap();
    app.with_session(|session| {
        let open = session.require_open()?;
        let track = open.project.sources[0].track().unwrap();
        assert_eq!(track.statistic, pe_core::track::SegmentStatistic::P75);
        Ok(())
    })
    .unwrap();
}

#[test]
fn polar_blend_settings_survive_save_undo_and_hidden_tracks() {
    use pe_core::project::PolarStatistic;
    let root = TempRoot::new("polar-statistic");
    let app = open_fixed(&root);
    let before = projects::summary(&app).unwrap().unwrap();
    assert_eq!(before.blend.polar_statistic, "mean");
    let mut settings = input(&before.blend);
    settings.polar_statistic = Some("max".into());
    let changed = blend::blend_settings_set(&app, settings).unwrap();
    assert_eq!(changed.blend.polar_statistic, "max");
    let mut legacy_input = input(&changed.blend);
    legacy_input.polar_statistic = None;
    assert_eq!(
        blend::blend_settings_set(&app, legacy_input)
            .unwrap()
            .blend
            .polar_statistic,
        "max"
    );
    app.with_session(|s| {
        let mut project = s.require_open()?.project.clone();
        for source in &mut project.sources {
            if matches!(source.kind, SourceKind::Track { .. }) {
                source.visible = false;
            }
        }
        let loaded = pe_core::io::from_bytes(&pe_core::io::to_bytes(&project)?)?;
        assert_eq!(loaded.blend.polar_statistic, PolarStatistic::Max);
        let original = blend::fresh(&project);
        let mut cache = pe_app::derived::Derivations::default();
        assert_eq!(*cache.blend(&project), original);
        project.blend.polar_statistic = PolarStatistic::Min;
        assert_eq!(*cache.blend(&project), blend::fresh(&project));
        assert_ne!(*cache.blend(&project), original);
        project.blend.polar_statistic = PolarStatistic::Max;
        assert_eq!(*cache.blend(&project), original);
        project.blend.min_samples = 1_000_000;
        project.blend.n_full = 1_000_000;
        project.blend.default_statistic = pe_core::track::SegmentStatistic::Median;
        assert_eq!(blend::fresh(&project), original);
        project.sources.retain(|source| source.visible);
        assert_eq!(blend::fresh(&project), original);
        Ok(())
    })
    .unwrap();
    edit::undo_last(&app).unwrap();
    assert_eq!(
        projects::summary(&app)
            .unwrap()
            .unwrap()
            .blend
            .polar_statistic,
        "mean"
    );
    edit::redo_next(&app).unwrap();
    assert_eq!(
        projects::summary(&app)
            .unwrap()
            .unwrap()
            .blend
            .polar_statistic,
        "max"
    );
    let mut invalid = input(&before.blend);
    invalid.polar_statistic = Some("p75".into());
    assert!(blend::blend_settings_set(&app, invalid).is_err());
}
