//! Editing one source's polar over IPC (spec.md 10.4, 12.1).
//!
//! The editable surface is the source's own polar: the imported grid of a
//! polar file, the VPP grid of an ORC certificate, or a track's polar segment
//! on the project's output grid. Every tool — a node drag, a typed value,
//! scale, smooth, reset — becomes one [`Command::EditCells`] on the source's
//! overlay (invariant 1); Rust computes every new value, the frontend only
//! says which cells and which tool. A drag's steps share a gesture name and
//! coalesce into one undo entry; every other action is one entry.

use pe_core::command::{CellEdit, EditAction};
use pe_core::source::{MAX_EDIT_BSP_KN, Source};
use pe_core::track::SegmentStatistic;
use pe_core::{Command, SourceId, canonical};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::projects::ProjectSummary;
use crate::session::OpenProject;

/// One cell of an editable surface, by its place in the surface's axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export_to = "PolarCell.ts")]
pub struct PolarCell {
    /// Row: an index into the surface's TWA axis.
    pub twa_index: u32,
    /// Column: an index into the surface's TWS axis.
    pub tws_index: u32,
}

/// A source's editable surface, as the table editor and the 3D edit mode
/// show it (spec.md 10.4). Grids are `[twa][tws]`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "EditSurface.ts")]
pub struct EditSurface {
    /// The source.
    pub source_id: u64,
    /// `orc`, `polar_file` or `track`.
    pub kind: String,
    /// TWA axis, degrees.
    pub twa: Vec<f64>,
    /// TWS axis, knots.
    pub tws: Vec<f64>,
    /// Each cell as imported (or, for a track, as binned), knots.
    pub source: Vec<Vec<Option<f64>>>,
    /// Each cell with the edits written in, knots: what is drawn.
    pub bsp: Vec<Vec<Option<f64>>>,
    /// Which cells hold an edit.
    pub edited: Vec<Vec<bool>>,
    /// Which cells are excluded from the blend (spec.md 10.3).
    pub excluded: Vec<Vec<bool>>,
    /// A track's samples per cell (spec.md 12.1); null for a polar source.
    pub count: Option<Vec<Vec<u32>>>,
    /// A track's spread per cell, knots (sample standard deviation).
    pub spread: Option<Vec<Vec<Option<f64>>>>,
    /// A track's statistic: `min`, `median`, `mean`, `p75`, `p90` or `max`.
    pub statistic: Option<String>,
    /// The samples a track cell needs to have a value (spec.md 12.1).
    pub min_samples: u32,
    /// Every edit the source holds, including any off this grid.
    pub edit_count: u32,
}

/// What an edit does to the chosen cells (spec.md 10.4).
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export_to = "EditOp.ts")]
pub enum EditOp {
    /// One step of a node drag: the node's new speed. Steps with the same
    /// gesture name are one undo entry.
    Drag {
        /// Knots.
        bsp: f64,
    },
    /// A value typed in the table; null resets the cell to the source.
    Type {
        /// Knots, or null.
        bsp: Option<f64>,
    },
    /// Scales the cells by a percentage (+10 is 10 % faster).
    Scale {
        /// Percent.
        percent: f64,
    },
    /// Smooths the cells over their 3×3 neighbourhood.
    Smooth,
    /// Resets the cells to the source's values.
    Reset,
    /// Clears every edit of the source; the cells are ignored.
    ResetAll,
}

/// A statistic's wire name.
pub fn statistic_name(statistic: SegmentStatistic) -> &'static str {
    match statistic {
        SegmentStatistic::Min => "min",
        SegmentStatistic::Median => "median",
        SegmentStatistic::Mean => "mean",
        SegmentStatistic::P75 => "p75",
        SegmentStatistic::P90 => "p90",
        SegmentStatistic::Max => "max",
    }
}

/// A statistic from its wire name.
pub fn parse_statistic(name: &str) -> Option<SegmentStatistic> {
    match name {
        "min" => Some(SegmentStatistic::Min),
        "median" => Some(SegmentStatistic::Median),
        "mean" => Some(SegmentStatistic::Mean),
        "p75" => Some(SegmentStatistic::P75),
        "p90" => Some(SegmentStatistic::P90),
        "max" => Some(SegmentStatistic::Max),
        _ => None,
    }
}

fn source_of(open: &mut OpenProject, id: u64) -> Result<Source> {
    if id == 0 {
        let data = open.derived.visible(&open.project);
        let polar = crate::blend::assemble_base(&open.project, &data).polar;
        let mut source = Source::new(
            SourceId(0),
            "Blend",
            open.project.blend.colour.clone(),
            pe_core::source::SourceKind::PolarFile {
                format: pe_core::polar::PolarFileFormat::Expedition,
                file_name: String::new(),
                polar,
                matrix: None,
            },
        );
        source.overlay.cell_overrides = open.project.blend.corrections.clone();
        return Ok(source);
    }
    open.project
        .source(SourceId(id))
        .cloned()
        .ok_or(AppError::Core(pe_core::CoreError::MissingSource(id)))
}

fn editable_data(
    open: &mut OpenProject,
    source: &Source,
) -> std::sync::Arc<crate::derived::Derived> {
    if source.id.raw() != 0 {
        return open.derived.get(&open.project, source);
    }
    let base = pe_polar::source_polar(source).unwrap_or_default();
    let edited = pe_polar::with_overlay(base.clone(), &source.overlay, false);
    std::sync::Arc::new(crate::derived::Derived {
        base,
        blend: edited.clone(),
        edited,
        track: None,
    })
}

/// Reuse the cell-edit tools for the blend, storing only the correction overlay.
fn edit_command(
    open: &OpenProject,
    source_id: u64,
    action: EditAction,
    cells: Vec<CellEdit>,
) -> Result<Command> {
    if source_id != 0 {
        return Ok(Command::EditCells {
            source: SourceId(source_id),
            action,
            cells,
        });
    }
    let before = open.project.blend.clone();
    let mut after = before.clone();
    for edit in cells {
        if (edit.twa == 0.0 || edit.twa == 360.0) && edit.after.is_some_and(|v| v != 0.0) {
            return Err(bad("Boat speed", "head-to-wind cells must remain zero"));
        }
        let probe = pe_core::source::CellRef {
            twa: edit.twa,
            tws: edit.tws,
        };
        let index = after
            .corrections
            .binary_search_by(|c| c.cell().order(&probe));
        match (index, edit.after) {
            (Ok(i), None) => {
                after.corrections.remove(i);
            }
            (Ok(i), Some(bsp)) => after.corrections[i].bsp = bsp,
            (Err(i), Some(bsp)) => after.corrections.insert(
                i,
                pe_core::source::CellOverride {
                    twa: edit.twa,
                    tws: edit.tws,
                    bsp,
                },
            ),
            (Err(_), None) => {}
        }
    }
    after.validate()?;
    Ok(Command::SetBlendSettings {
        before: Box::new(before),
        after: Box::new(after),
    })
}

/// The editable surface of source `source_id` in the open project; 0 is the blend.
#[tauri::command]
pub fn polar_edit_surface(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    source_id: u64,
) -> Result<EditSurface> {
    let state = state.scoped(boat_context);
    edit_surface(&state, source_id)
}

/// [`polar_edit_surface`] without a Tauri handle.
pub fn edit_surface(state: &AppState, source_id: u64) -> Result<EditSurface> {
    state.with_session(|session| {
        let open = session.require_open()?;
        let source = source_of(open, source_id)?;
        let derived = editable_data(open, &source);
        let grid = &derived.edited;
        let flags = |f: &dyn Fn(f64, f64) -> bool| -> Vec<Vec<bool>> {
            grid.twa
                .iter()
                .map(|twa| grid.tws.iter().map(|tws| f(*twa, *tws)).collect())
                .collect()
        };
        let overlay = &source.overlay;
        let track = source.track();
        Ok(EditSurface {
            source_id,
            kind: source.kind.name().to_owned(),
            twa: grid.twa.clone(),
            tws: grid.tws.clone(),
            source: derived.base.bsp.clone(),
            bsp: grid.bsp.clone(),
            edited: flags(&|twa, tws| overlay.override_at(twa, tws).is_some()),
            excluded: flags(&|twa, tws| overlay.is_cell_excluded(twa, tws)),
            count: derived.track.as_ref().map(|t| t.segment.count.clone()),
            spread: derived.track.as_ref().map(|t| t.segment.spread.clone()),
            statistic: track.map(|t| statistic_name(t.statistic).to_owned()),
            min_samples: open.project.blend.min_samples,
            edit_count: u32::try_from(overlay.cell_overrides.len()).unwrap_or(u32::MAX),
        })
    })
}

/// Edits cells of one source's editable surface (spec.md 10.4) as one
/// undoable change. `gesture` names a drag: its steps coalesce.
#[tauri::command]
pub fn edit_polar(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    source_id: u64,
    op: EditOp,
    cells: Vec<PolarCell>,
    gesture: Option<String>,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    polar_edit(&state, source_id, &op, &cells, gesture.as_deref())
}

fn bad(field: &'static str, value: impl Into<String>) -> AppError {
    AppError::BadOption {
        field,
        value: value.into(),
    }
}

/// A speed the user gave, checked and canonical.
fn speed(bsp: f64) -> Result<f64> {
    if bsp.is_finite() && (0.0..=MAX_EDIT_BSP_KN).contains(&bsp) {
        Ok(canonical::knots(bsp))
    } else {
        Err(bad("Boat speed", format!("{bsp}")))
    }
}

/// A cell by row and column, and the override it is to hold.
type Target = ((usize, usize), Option<f64>);

/// [`edit_polar`] without a Tauri handle.
pub fn polar_edit(
    state: &AppState,
    source_id: u64,
    op: &EditOp,
    cells: &[PolarCell],
    gesture: Option<&str>,
) -> Result<ProjectSummary> {
    state.with_session(|session| {
        let open = session.require_open()?;
        let source = source_of(open, source_id)?;
        let derived = editable_data(open, &source);
        let grid = &derived.edited;
        let overlay = &source.overlay;
        // The chosen cells, checked against the surface, each once.
        let mut chosen: Vec<(usize, usize)> = Vec::with_capacity(cells.len());
        for cell in cells {
            let (i, j) = (cell.twa_index as usize, cell.tws_index as usize);
            if i >= grid.twa.len() || j >= grid.tws.len() {
                return Err(bad(
                    "Polar cell",
                    format!("row {i} column {j} of {}", source.label),
                ));
            }
            chosen.push((i, j));
        }
        chosen.sort_unstable();
        chosen.dedup();
        let single = || match chosen.as_slice() {
            [one] => Ok(*one),
            _ => Err(bad("Polar cell", "a drag or a typed value edits one cell")),
        };
        let (action, targets): (EditAction, Vec<Target>) = match op {
            EditOp::Drag { bsp } => (EditAction::Drag, vec![(single()?, Some(speed(*bsp)?))]),
            EditOp::Type { bsp } => {
                let value = bsp.map(speed).transpose()?;
                (EditAction::Type, vec![(single()?, value)])
            }
            EditOp::Scale { percent } => {
                if !percent.is_finite() || *percent <= -100.0 || *percent > 1000.0 {
                    return Err(bad("Scale", format!("{percent} %")));
                }
                let out = pe_polar::edit::scaled(grid, &chosen, *percent);
                (
                    EditAction::Scale,
                    out.into_iter()
                        .map(|(cell, v)| (cell, Some(canonical::knots(v))))
                        .collect(),
                )
            }
            EditOp::Smooth => (
                EditAction::Smooth,
                // Neighbours as the blend reads them: an excluded node is
                // empty for this source (spec.md 10.3) and takes no part.
                pe_polar::edit::smoothed(grid, &derived.blend, &chosen)
                    .into_iter()
                    .map(|(cell, v)| (cell, Some(canonical::knots(v))))
                    .collect(),
            ),
            EditOp::Reset => (
                EditAction::Reset,
                chosen.iter().map(|cell| (*cell, None)).collect(),
            ),
            EditOp::ResetAll => {
                // Every override, on this grid or not.
                let edits: Vec<CellEdit> = overlay
                    .cell_overrides
                    .iter()
                    .map(|o| CellEdit {
                        twa: o.twa,
                        tws: o.tws,
                        before: Some(o.bsp),
                        after: None,
                    })
                    .collect();
                if !edits.is_empty() {
                    let command = edit_command(open, source_id, EditAction::ResetAll, edits)?;
                    open.apply(command)?;
                }
                return Ok(ProjectSummary::of(open));
            }
        };
        let edits: Vec<CellEdit> = targets
            .into_iter()
            .filter_map(|((i, j), after)| {
                let (twa, tws) = (*grid.twa.get(i)?, *grid.tws.get(j)?);
                let before = overlay.override_at(twa, tws);
                (before != after).then_some(CellEdit {
                    twa,
                    tws,
                    before,
                    after,
                })
            })
            .collect();
        if edits.is_empty() {
            return Ok(ProjectSummary::of(open));
        }
        let command = edit_command(open, source_id, action, edits)?;
        match (action, gesture) {
            (EditAction::Drag, Some(key)) => {
                open.apply_coalesced(command, &format!("edit:{source_id}:{key}"))?;
            }
            _ => open.apply(command)?,
        }
        Ok(ProjectSummary::of(open))
    })
}

/// Chooses the per-cell statistic of a track's polar segment (spec.md
/// 12.1): `min`, `median`, `mean`, `p75`, `p90` or `max`. Undoable.
#[tauri::command]
pub fn set_segment_statistic(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    source_id: u64,
    statistic: String,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    segment_statistic_set(&state, source_id, &statistic)
}

/// [`set_segment_statistic`] without a Tauri handle.
pub fn segment_statistic_set(
    state: &AppState,
    source_id: u64,
    statistic: &str,
) -> Result<ProjectSummary> {
    let after = parse_statistic(statistic).ok_or_else(|| bad("Statistic", statistic))?;
    crate::edit::apply(state, |project| {
        let source = project
            .source(SourceId(source_id))
            .ok_or(AppError::Core(pe_core::CoreError::MissingSource(source_id)))?;
        let track = source
            .track()
            .ok_or_else(|| bad("Statistic", format!("{} is not a track", source.label)))?;
        Ok(
            (track.statistic != after).then_some(Command::SetSegmentStatistic {
                source: SourceId(source_id),
                before: track.statistic,
                after,
            }),
        )
    })
}
