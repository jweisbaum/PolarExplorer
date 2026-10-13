#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The source-list edits exposed over IPC (spec.md 8), driven through the
//! same functions the Tauri commands call. The list UI arrives in later
//! milestones; the commands are here so it only adds a view.

mod common;

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::error::AppError;
use pe_app::{edit, projects};
use pe_core::command::EditAction;
use pe_core::polar::{PolarFileFormat, PolarGrid};
use pe_core::{Colour, Command, Source, SourceKind};

/// A project with three polar-file sources, ids 1–3, saved so it is clean.
fn project_with_sources(root: &TempRoot) -> AppState {
    let app = root.state();
    projects::create(&app, "Sources".to_owned(), None, false).expect("create");
    app.with_session(|session| {
        let open = session.require_open()?;
        for label in ["A", "B", "C"] {
            let id = open.project.allocate_source_id();
            let colour = open.project.next_palette_colour();
            let source = Source::new(
                id,
                label,
                colour,
                SourceKind::PolarFile {
                    matrix: None,
                    format: PolarFileFormat::Expedition,
                    file_name: format!("{label}.txt"),
                    polar: PolarGrid::empty(vec![40.0, 90.0], vec![10.0]),
                },
            );
            let index = open.project.sources.len();
            open.apply(Command::AddSource {
                index,
                source: Box::new(source),
            })?;
        }
        // The imports are the fixture, not part of what is under test.
        open.history = pe_core::History::default();
        Ok(())
    })
    .expect("add sources");
    projects::save_as(&app, root.file("s.wpsproj")).expect("save");
    app
}

fn labels(summary: &projects::ProjectSummary) -> Vec<&str> {
    summary.sources.iter().map(|s| s.label.as_str()).collect()
}

fn ids(app: &AppState) -> Vec<u64> {
    projects::summary(app)
        .unwrap()
        .unwrap()
        .sources
        .iter()
        .map(|s| s.id)
        .collect()
}

#[test]
fn each_edit_changes_the_source_marks_dirty_and_undoes() {
    let root = TempRoot::new("source-edits");
    let app = project_with_sources(&root);
    let id = ids(&app)[1];
    let original = projects::summary(&app).unwrap().unwrap();
    assert!(!original.dirty);

    let s = edit::source_colour_set(&app, id, "#ABCDEF").unwrap();
    assert_eq!(s.sources[1].colour, "#abcdef");
    assert!(s.dirty);
    assert_eq!(s.undo_label.as_deref(), Some("Change source colour"));

    let s = edit::source_visible_set(&app, id, false).unwrap();
    assert!(!s.sources[1].visible);
    assert_eq!(s.undo_label.as_deref(), Some("Hide source"));

    let s = edit::source_weight_set(&app, id, 0.5, None).unwrap();
    assert_eq!(s.sources[1].weight, 0.5);

    let s = edit::source_label_set(&app, id, "  Sister ship ".to_owned()).unwrap();
    assert_eq!(s.sources[1].label, "Sister ship");

    let s = edit::source_move(&app, id, 0).unwrap();
    assert_eq!(labels(&s), ["Sister ship", "A", "C"]);

    let s = edit::source_remove(&app, id).unwrap();
    assert_eq!(labels(&s), ["A", "C"]);

    // Six entries; undoing all of them gives back the saved list exactly.
    for _ in 0..6 {
        edit::undo_last(&app).unwrap();
    }
    let back = projects::summary(&app).unwrap().unwrap();
    assert_eq!(back.sources, original.sources);
    assert!(!back.can_undo);
}

#[test]
fn a_change_to_the_same_value_records_nothing() {
    let root = TempRoot::new("source-noop");
    let app = project_with_sources(&root);
    let first = ids(&app)[0];
    let colour = projects::summary(&app).unwrap().unwrap().sources[0]
        .colour
        .clone();
    let s = edit::source_colour_set(&app, first, &colour).unwrap();
    let s2 = edit::source_visible_set(&app, first, true).unwrap();
    let s3 = edit::source_move(&app, first, 0).unwrap();
    for summary in [s, s2, s3] {
        assert!(!summary.dirty);
        assert!(!summary.can_undo);
    }
}

#[test]
fn a_weight_drag_is_one_undo_entry() {
    let root = TempRoot::new("source-drag");
    let app = project_with_sources(&root);
    let id = ids(&app)[0];
    for weight in [0.9, 0.8, 0.7, 0.6] {
        edit::source_weight_set(&app, id, weight, Some("drag-1")).unwrap();
    }
    let s = edit::undo_last(&app).unwrap();
    assert_eq!(s.sources[0].weight, 1.0);
    assert!(!s.can_undo, "the whole drag was one entry");
}

#[test]
fn bad_arguments_are_refused_and_change_nothing() {
    let root = TempRoot::new("source-bad");
    let app = project_with_sources(&root);
    let id = ids(&app)[0];
    assert!(matches!(
        edit::source_colour_set(&app, id, "red"),
        Err(AppError::Core(_))
    ));
    // 1 is the largest weight (spec.md 8); 1.5 was valid before schema 7.
    assert!(edit::source_weight_set(&app, id, 1.5, None).is_err());
    assert!(edit::source_weight_set(&app, id, f64::NAN, None).is_err());
    assert!(matches!(
        edit::source_label_set(&app, id, "   ".to_owned()),
        Err(AppError::BadOption { .. })
    ));
    assert!(edit::source_move(&app, id, 3).is_err());
    assert!(edit::source_remove(&app, 999).is_err());
    let s = projects::summary(&app).unwrap().unwrap();
    assert!(!s.dirty);
    assert_eq!(
        Colour::parse(&s.sources[0].colour).unwrap().as_str(),
        s.sources[0].colour
    );
}

#[test]
fn edits_need_an_open_project() {
    let root = TempRoot::new("source-none");
    let app = root.state();
    assert!(matches!(
        edit::source_visible_set(&app, 1, false),
        Err(AppError::NoProjectOpen)
    ));
}

/// History labels are English keys the frontend translates (spec.md 3.5);
/// `rust-strings.json` is how the coverage test holds every catalogue to
/// them, so it must list exactly the labels Rust can produce.
#[test]
fn every_history_label_is_listed_for_translation() {
    let listed: Vec<String> =
        serde_json::from_str(include_str!("../../../ui/src/i18n/rust-strings.json")).unwrap();
    let source = || {
        Box::new(Source::new(
            pe_core::SourceId(1),
            "A",
            Colour::parse("#000000").unwrap(),
            SourceKind::PolarFile {
                matrix: None,
                format: PolarFileFormat::Expedition,
                file_name: "a.txt".to_owned(),
                polar: PolarGrid::empty(vec![40.0], vec![10.0]),
            },
        ))
    };
    let id = pe_core::SourceId(1);
    let black = Colour::parse("#000000").unwrap();
    let mut commands = vec![
        Command::RenameBoat {
            before: String::new(),
            after: String::new(),
        },
        Command::RenameProject {
            before: String::new(),
            after: String::new(),
        },
        Command::AddSource {
            index: 0,
            source: source(),
        },
        Command::RemoveSource {
            index: 0,
            source: source(),
        },
        Command::SetSourceColour {
            source: id,
            before: black.clone(),
            after: black,
        },
        Command::SetSourceVisible {
            source: id,
            before: false,
            after: true,
        },
        Command::SetSourceVisible {
            source: id,
            before: true,
            after: false,
        },
        Command::SetSourceWeight {
            source: id,
            before: 1.0,
            after: 1.0,
        },
        Command::SetSourceLabel {
            source: id,
            before: String::new(),
            after: String::new(),
        },
        Command::MoveSource { from: 0, to: 1 },
        // A batch of exclusions over several sources reuses these labels.
        Command::ExcludeCells {
            source: id,
            cells: Vec::new(),
        },
        Command::IncludeCells {
            source: id,
            cells: Vec::new(),
        },
        Command::ExcludeSamples {
            source: id,
            samples: Vec::new(),
        },
        Command::IncludeSamples {
            source: id,
            samples: Vec::new(),
        },
        Command::SetSampleFilters {
            source: id,
            before: Box::default(),
            after: Box::default(),
        },
        Command::SetDerivation {
            source: id,
            before: Default::default(),
            after: Default::default(),
            motion_before: Vec::new(),
            motion_after: Vec::new(),
        },
        Command::SetUseCorrected {
            before: true,
            after: false,
        },
        Command::SetStokesDrift {
            before: false,
            after: true,
        },
        Command::SetSegmentStatistic {
            source: id,
            before: Default::default(),
            after: Default::default(),
        },
        Command::SetOutputGrid {
            before: Default::default(),
            after: Default::default(),
        },
    ];
    // A blend settings change is named by what it changes (spec.md 8).
    let blend = pe_core::project::BlendSettings::default();
    for after in [
        pe_core::project::BlendSettings {
            visible: false,
            ..blend.clone()
        },
        pe_core::project::BlendSettings {
            colour: pe_core::Colour::parse("#123456").unwrap(),
            ..blend.clone()
        },
        pe_core::project::BlendSettings {
            global_filters: Some(Default::default()),
            ..blend.clone()
        },
        pe_core::project::BlendSettings {
            corrections: vec![pe_core::source::CellOverride {
                twa: 90.0,
                tws: 10.0,
                bsp: 7.0,
            }],
            ..blend.clone()
        },
        pe_core::project::BlendSettings {
            n_full: 3,
            ..blend.clone()
        },
    ] {
        let hidden = !after.visible;
        commands.push(Command::SetBlendSettings {
            before: Box::new(blend.clone()),
            after: Box::new(after.clone()),
        });
        if hidden {
            commands.push(Command::SetBlendSettings {
                before: Box::new(after),
                after: Box::new(blend.clone()),
            });
        }
    }
    // An edit is named by the tool that made it.
    commands.extend(
        [
            EditAction::Drag,
            EditAction::Type,
            EditAction::Scale,
            EditAction::Smooth,
            EditAction::Reset,
            EditAction::ResetAll,
        ]
        .map(|action| Command::EditCells {
            source: id,
            action,
            cells: Vec::new(),
        }),
    );
    let mut labels: Vec<String> = commands.iter().map(Command::label).collect();
    // Batches carry their own label; these are every one Rust builds.
    labels.extend(
        [
            pe_app::polar_files::IMPORT_ONE,
            pe_app::polar_files::IMPORT_MANY,
            pe_app::orc::ADD_ORC,
            "Add ORR polar",
            pe_app::tracks::IMPORT_ONE,
            pe_app::tracks::IMPORT_MANY,
            pe_core::command::EXCLUDE_DOTS_LABEL,
            pe_core::command::INCLUDE_DOTS_LABEL,
            // Not a history label: the blend curve's, translated as one.
            pe_app::polar_plot::BLEND_LABEL,
        ]
        .map(str::to_owned),
    );
    labels.sort();
    let mut listed_sorted = listed.clone();
    listed_sorted.sort();
    assert_eq!(labels, listed_sorted);
}

/// The source list's colour picker offers the palette new sources take
/// colours from (spec.md 8); the frontend keeps a copy of it, checked here.
#[test]
fn the_frontend_palette_is_the_core_palette() {
    let listed: Vec<String> =
        serde_json::from_str(include_str!("../../../ui/src/panels/palette.json")).unwrap();
    assert_eq!(listed, pe_core::source::PALETTE);
}
