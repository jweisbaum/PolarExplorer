//! Document edits and undo/redo over IPC (spec.md 4.6).
//!
//! Every change goes through a `pe_core::Command` on the open project's
//! history, which bumps the revision and sets `dirty` (spec.md 3.3). Later
//! milestones add their edits here.

use pe_core::{Colour, Command, SourceId};

use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::projects::ProjectSummary;

/// Applies the command `build` makes from the open project, and returns the
/// new summary. `build` returns `None` for a change that changes nothing,
/// which records no history entry and leaves the project clean.
pub fn apply(
    state: &AppState,
    build: impl FnOnce(&pe_core::Project) -> Result<Option<Command>>,
) -> Result<ProjectSummary> {
    state.with_session(|session| {
        let open = session.require_open()?;
        open.ensure_editable()?;
        if let Some(command) = build(&open.project)? {
            open.apply(command)?;
        }
        Ok(ProjectSummary::of(open))
    })
}

/// Reverses the most recent change.
#[tauri::command]
pub fn undo(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    undo_last(&state)
}

/// [`undo`] without a Tauri handle.
pub fn undo_last(state: &AppState) -> Result<ProjectSummary> {
    state.with_session(|session| {
        let open = session.require_open()?;
        open.undo()?;
        Ok(ProjectSummary::of(open))
    })
}

/// Reapplies the most recently undone change.
#[tauri::command]
pub fn redo(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    redo_next(&state)
}

/// [`redo`] without a Tauri handle.
pub fn redo_next(state: &AppState) -> Result<ProjectSummary> {
    state.with_session(|session| {
        let open = session.require_open()?;
        open.redo()?;
        Ok(ProjectSummary::of(open))
    })
}

/// Renames the open project (spec.md 3.2: click the name to rename).
#[tauri::command]
pub fn rename_project(state: tauri::State<'_, AppState>, name: String) -> Result<ProjectSummary> {
    rename(&state, name)
}

/// [`rename_project`] without a Tauri handle.
pub fn rename(state: &AppState, name: String) -> Result<ProjectSummary> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::BadOption {
            field: "Project name",
            value: name,
        });
    }
    apply(state, |project| {
        Ok((project.name != name).then(|| Command::RenameProject {
            before: project.name.clone(),
            after: name,
        }))
    })
}

/// The open project's source with `id`, or a `core` error naming it.
fn source_of(project: &pe_core::Project, id: u64) -> Result<&pe_core::Source> {
    project
        .source(SourceId(id))
        .ok_or(AppError::Core(pe_core::CoreError::MissingSource(id)))
}

/// Changes a source's colour (spec.md 8). `colour` is `#rrggbb`.
#[tauri::command]
pub fn set_source_colour(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: u64,
    colour: String,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    source_colour_set(&state, id, &colour)
}

/// [`set_source_colour`] without a Tauri handle.
pub fn source_colour_set(state: &AppState, id: u64, colour: &str) -> Result<ProjectSummary> {
    let after = Colour::parse(colour)?;
    apply(state, |project| {
        let before = source_of(project, id)?.colour.clone();
        Ok((before != after).then_some(Command::SetSourceColour {
            source: SourceId(id),
            before,
            after,
        }))
    })
}

/// Shows or hides a source. Hidden sources leave the blend and every plot
/// (spec.md 8, D15).
#[tauri::command]
pub fn set_source_visible(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: u64,
    visible: bool,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    source_visible_set(&state, id, visible)
}

/// [`set_source_visible`] without a Tauri handle.
pub fn source_visible_set(state: &AppState, id: u64, visible: bool) -> Result<ProjectSummary> {
    apply(state, |project| {
        let before = source_of(project, id)?.visible;
        Ok((before != visible).then_some(Command::SetSourceVisible {
            source: SourceId(id),
            before,
            after: visible,
        }))
    })
}

/// Replace the visible source set in one undoable change (bulk selection/solo).
#[tauri::command]
pub fn set_visible_sources(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    ids: Vec<u64>,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    apply(&state, |project| {
        let ids: std::collections::BTreeSet<_> = ids.into_iter().collect();
        for id in &ids {
            source_of(project, *id)?;
        }
        let commands: Vec<_> = project
            .sources
            .iter()
            .filter_map(|source| {
                let visible = ids.contains(&source.id.raw());
                (source.visible != visible).then_some(Command::SetSourceVisible {
                    source: source.id,
                    before: source.visible,
                    after: visible,
                })
            })
            .collect();
        Ok((!commands.is_empty()).then_some(Command::Batch {
            label: "Change source visibility".to_owned(),
            commands,
        }))
    })
}

/// Changes a source's blend weight, 0–1. `gesture` names a slider drag: every
/// call carrying the same name, one after another, is one undo entry.
#[tauri::command]
pub fn set_source_weight(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: u64,
    weight: f64,
    gesture: Option<String>,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    source_weight_set(&state, id, weight, gesture.as_deref())
}

/// [`set_source_weight`] without a Tauri handle.
pub fn source_weight_set(
    state: &AppState,
    id: u64,
    weight: f64,
    gesture: Option<&str>,
) -> Result<ProjectSummary> {
    pe_core::source::validate_weight(weight)?;
    state.with_session(|session| {
        let open = session.require_open()?;
        let before = source_of(&open.project, id)?.weight;
        if before != weight {
            let command = Command::SetSourceWeight {
                source: SourceId(id),
                before,
                after: weight,
            };
            match gesture {
                Some(key) => open.apply_coalesced(command, &format!("weight:{id}:{key}"))?,
                None => open.apply(command)?,
            }
        }
        Ok(ProjectSummary::of(open))
    })
}

/// Renames a source.
#[tauri::command]
pub fn set_source_label(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: u64,
    label: String,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    source_label_set(&state, id, label)
}

/// [`set_source_label`] without a Tauri handle.
pub fn source_label_set(state: &AppState, id: u64, label: String) -> Result<ProjectSummary> {
    let label = label.trim().to_owned();
    if label.is_empty() {
        return Err(AppError::BadOption {
            field: "Source label",
            value: label,
        });
    }
    apply(state, |project| {
        let before = source_of(project, id)?.label.clone();
        Ok((before != label).then_some(Command::SetSourceLabel {
            source: SourceId(id),
            before,
            after: label,
        }))
    })
}

/// Moves a source to position `to` in the list (display order only).
#[tauri::command]
pub fn move_source(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: u64,
    to: usize,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    source_move(&state, id, to)
}

/// [`move_source`] without a Tauri handle.
pub fn source_move(state: &AppState, id: u64, to: usize) -> Result<ProjectSummary> {
    apply(state, |project| {
        let from = project
            .source_index(SourceId(id))
            .ok_or(AppError::Core(pe_core::CoreError::MissingSource(id)))?;
        let len = project.sources.len();
        if to >= len {
            return Err(AppError::Core(pe_core::CoreError::IndexOutOfBounds {
                index: to,
                len,
            }));
        }
        Ok((from != to).then_some(Command::MoveSource { from, to }))
    })
}

/// Removes a source (undoable: undo puts it back, overlays and all).
#[tauri::command]
pub fn remove_source(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: u64,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    source_remove(&state, id)
}

/// [`remove_source`] without a Tauri handle.
pub fn source_remove(state: &AppState, id: u64) -> Result<ProjectSummary> {
    apply(state, |project| {
        let index = project
            .source_index(SourceId(id))
            .ok_or(AppError::Core(pe_core::CoreError::MissingSource(id)))?;
        let source = project.sources[index].clone();
        Ok(Some(Command::RemoveSource {
            index,
            source: Box::new(source),
        }))
    })
}
