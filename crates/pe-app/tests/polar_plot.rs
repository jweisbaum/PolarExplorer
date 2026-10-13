#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The 2D polar plot over IPC (spec.md 9.2), driven through the function the
//! Tauri command calls, with a real ORC certificate alongside a polar file.

mod common;

use common::TempRoot;
use pe_app::orc::{self, OrcFilters};
use pe_app::polar_plot::plot;
use pe_app::projects;
use pe_core::polar::{PolarFileFormat, PolarGrid};
use pe_core::{Command, Source, SourceKind};

fn project_with_orc_and_file(root: &TempRoot) -> pe_app::commands::AppState {
    let app = root.state();
    projects::create(&app, "Plot".to_owned(), None, false).unwrap();
    let hit = orc::search(&app, "GBR/1124", OrcFilters::default(), 1)
        .unwrap()
        .hits
        .remove(0);
    orc::add(&app, hit.id, false).unwrap();
    app.with_session(|session| {
        let open = session.require_open()?;
        let id = open.project.allocate_source_id();
        let colour = open.project.next_palette_colour();
        let source = Source::new(
            id,
            "File",
            colour,
            SourceKind::PolarFile {
                matrix: None,
                format: PolarFileFormat::Expedition,
                file_name: "file.txt".to_owned(),
                polar: PolarGrid {
                    twa: vec![60.0, 120.0],
                    tws: vec![10.0],
                    bsp: vec![vec![Some(7.0)], vec![Some(9.0)]],
                },
            },
        );
        let index = open.project.sources.len();
        open.apply(Command::AddSource {
            index,
            source: Box::new(source),
        })
    })
    .unwrap();
    app
}

#[test]
fn a_real_orc_certificate_gives_a_curve_at_its_own_wind_speeds() {
    let root = TempRoot::new("plot-orc");
    let app = project_with_orc_and_file(&root);
    // 10 kn is a certificate wind speed and the file source's only one.
    let result = plot(&app, Some(10.0)).unwrap();
    assert_eq!(result.curves.len(), 2);
    let orc_curve = result
        .curves
        .iter()
        .find(|c| c.label == "Eratosthenes")
        .unwrap();
    assert_eq!(orc_curve.tws, 10.0);
    assert!(
        orc_curve.points.len() >= 8,
        "the eight ORC angles plus beat and run"
    );
    assert!(orc_curve.points.windows(2).all(|w| w[0].twa < w[1].twa));
    let file_curve = result.curves.iter().find(|c| c.label == "File").unwrap();
    assert_eq!(
        file_curve.points,
        vec![
            pe_app::polar_plot::PolarCurvePoint {
                twa: 60.0,
                bsp: 7.0
            },
            pe_app::polar_plot::PolarCurvePoint {
                twa: 120.0,
                bsp: 9.0
            },
        ]
    );
}

#[test]
fn hiding_a_source_drops_it_from_the_plot_and_the_domain() {
    let root = TempRoot::new("plot-hide");
    let app = project_with_orc_and_file(&root);
    let file_id = projects::summary(&app)
        .unwrap()
        .unwrap()
        .sources
        .iter()
        .find(|s| s.label == "File")
        .unwrap()
        .id;
    pe_app::edit::source_visible_set(&app, file_id, false).unwrap();
    let result = plot(&app, None).unwrap();
    assert!(result.curves.iter().all(|c| c.label != "File"));
    // Only the ORC certificate's own axis is left: 10 kn is no longer in range.
    assert_ne!(result.tws_max, Some(10.0));
}

#[test]
fn the_plot_needs_an_open_project() {
    let root = TempRoot::new("plot-none");
    let app = root.state();
    assert!(matches!(
        plot(&app, None),
        Err(pe_app::error::AppError::NoProjectOpen)
    ));
}

#[test]
fn without_tracks_there_are_no_dots_and_the_blend_is_between_the_sources() {
    let root = TempRoot::new("plot-placeholders");
    let app = project_with_orc_and_file(&root);
    let result = plot(&app, Some(10.0)).unwrap();
    assert!(
        pe_app::polar_plot::dots(&app, Some(10.0), false)
            .unwrap()
            .is_empty(),
        "no track exists"
    );
    // The blend at 10 kn, 60°: the mean of the certificate and the file's
    // 7.0 kn, both at weight 1 (spec.md 12.3).
    assert_eq!(result.blend.len(), 1);
    let at_60 = |points: &[pe_app::polar_plot::PolarCurvePoint]| {
        points.iter().find(|p| p.twa == 60.0).map(|p| p.bsp)
    };
    let orc = result
        .curves
        .iter()
        .find(|c| c.label != "File")
        .and_then(|c| at_60(&c.points))
        .unwrap();
    let blend = at_60(&result.blend[0].points).unwrap();
    assert!((blend - (orc + 7.0) / 2.0).abs() < 1e-6, "{blend} vs {orc}");
}
