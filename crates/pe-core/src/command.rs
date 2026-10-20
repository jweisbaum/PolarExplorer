//! Document mutations, each one reversible (spec.md 4.6).
//!
//! Every change to a project goes through a [`Command`]. A command carries
//! both the new value and the value it displaced, so undo is exact rather
//! than recomputed — the VectorEffects pattern. Snapshotting the whole
//! document per edit was rejected there for cost, and here a project holds
//! hundreds of thousands of samples.
//!
//! **Apply checks the document is in the state the command was built
//! against.** A command whose `before` no longer matches refuses with
//! [`CoreError::Stale`] rather than silently writing, so a bug elsewhere
//! cannot make undo restore the wrong value.

use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{CoreError, Result};
use crate::id::{SampleId, SourceId};
use crate::project::{BlendSettings, OutputGrid, Project};
use crate::source::{
    CellOverride, CellRef, Colour, SampleFilters, Source, SourceKind, validate_edit_bsp,
    validate_weight,
};
use crate::track::{DerivationSettings, Motion, SegmentStatistic, Track};

/// One cell's edit (spec.md 10.4): the override it held before and the one
/// it holds after, `None` meaning none (the cell reads the source's value).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellEdit {
    /// TWA, degrees: a value of the editable surface's axis.
    #[serde(with = "canonical::degrees_field")]
    pub twa: f64,
    /// TWS, knots: a value of the editable surface's axis.
    #[serde(with = "canonical::knots_field")]
    pub tws: f64,
    /// The override before the edit.
    #[serde(with = "canonical::optional_knots_field")]
    pub before: Option<f64>,
    /// The override after it.
    #[serde(with = "canonical::optional_knots_field")]
    pub after: Option<f64>,
}

impl CellEdit {
    fn cell(&self) -> CellRef {
        CellRef {
            twa: self.twa,
            tws: self.tws,
        }
    }
}

/// Which edit tool made a [`Command::EditCells`] (spec.md 10.4). It names
/// the history entry, and only drags coalesce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditAction {
    /// A node dragged in the 3D view.
    Drag,
    /// A value typed in the table.
    Type,
    /// A selection scaled by a percentage.
    Scale,
    /// A selection smoothed.
    Smooth,
    /// A selection reset to the source's values.
    Reset,
    /// Every edit of the source cleared.
    ResetAll,
}

/// A reversible change to a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// Renames one boat without changing the project name.
    RenameBoat {
        /// Previous name.
        before: String,
        /// New name.
        after: String,
    },
    /// Renames the project.
    RenameProject {
        /// Previous name.
        before: String,
        /// New name.
        after: String,
    },
    /// Inserts a source at `index`. An import is this (spec.md 4.6): undo
    /// removes the source.
    AddSource {
        /// Position in the list.
        index: usize,
        /// The source, with its bulk data.
        source: Box<Source>,
    },
    /// Removes the source at `index`, keeping it for undo.
    RemoveSource {
        /// Position in the list.
        index: usize,
        /// The removed source, restored on undo.
        source: Box<Source>,
    },
    /// Changes a source's colour.
    SetSourceColour {
        /// Target source.
        source: SourceId,
        /// Previous colour.
        before: Colour,
        /// New colour.
        after: Colour,
    },
    /// Shows or hides a source.
    SetSourceVisible {
        /// Target source.
        source: SourceId,
        /// Previous state.
        before: bool,
        /// New state.
        after: bool,
    },
    /// Changes a source's blend weight. A slider drag coalesces into one entry.
    SetSourceWeight {
        /// Target source.
        source: SourceId,
        /// Previous weight.
        before: f64,
        /// New weight.
        after: f64,
    },
    /// Renames a source.
    SetSourceLabel {
        /// Target source.
        source: SourceId,
        /// Previous label.
        before: String,
        /// New label.
        after: String,
    },
    /// Moves a source within the list (display order only).
    MoveSource {
        /// Index before the move.
        from: usize,
        /// Index after the move.
        to: usize,
    },
    /// Excludes polar nodes of one ORC or file source from the blend
    /// (spec.md 10.3). `cells` are exactly the nodes this changes: none may
    /// already be excluded, so undo removes exactly them.
    ExcludeCells {
        /// Target source.
        source: SourceId,
        /// The nodes excluded, each once.
        cells: Vec<CellRef>,
    },
    /// Includes excluded polar nodes again (spec.md 10.3); the inverse of
    /// [`Command::ExcludeCells`]. Every cell must currently be excluded.
    IncludeCells {
        /// Target source.
        source: SourceId,
        /// The nodes included, each once.
        cells: Vec<CellRef>,
    },
    /// Excludes samples of one track from the blend (spec.md 10.3).
    /// `samples` are exactly the ones this changes: each is the track's,
    /// none is already excluded, so undo removes exactly them.
    ExcludeSamples {
        /// Target track source.
        source: SourceId,
        /// The samples excluded, each once.
        samples: Vec<SampleId>,
    },
    /// Includes excluded samples again; the inverse of
    /// [`Command::ExcludeSamples`]. Every sample must currently be excluded.
    IncludeSamples {
        /// Target track source.
        source: SourceId,
        /// The samples included, each once.
        samples: Vec<SampleId>,
    },
    /// Replaces a track's sample filters (spec.md 7.6).
    SetSampleFilters {
        /// Target track source.
        source: SourceId,
        /// Previous filters.
        before: Box<SampleFilters>,
        /// New filters.
        after: Box<SampleFilters>,
    },
    /// Changes how a track derives heading and speed (spec.md 7.4), with
    /// every sample's motion before and after. `pe-core` does not derive
    /// (that is `pe-tracks`), so the command carries the result both ways
    /// and undo is exact rather than recomputed.
    SetDerivation {
        /// Target track source.
        source: SourceId,
        /// Previous settings.
        before: DerivationSettings,
        /// New settings.
        after: DerivationSettings,
        /// Every sample's motion under `before`, in sample order.
        motion_before: Vec<Motion>,
        /// Every sample's motion under `after`, in sample order.
        motion_after: Vec<Motion>,
    },
    /// Edits cells of one source's editable surface (spec.md 10.4): the
    /// imported grid of a file, the VPP grid of an ORC certificate, or a
    /// track's polar segment on the output grid. Each cell's override moves
    /// from `before` to `after`, and apply refuses unless every cell holds
    /// its `before`, so undo is exact. The overrides stay sorted by cell,
    /// one per cell; with none left, the source reads exactly as imported
    /// (invariant 1).
    EditCells {
        /// Target source.
        source: SourceId,
        /// The tool that made it.
        action: EditAction,
        /// The cells changed, each once.
        cells: Vec<CellEdit>,
    },
    /// Chooses the per-cell statistic of a track's polar segment
    /// (spec.md 12.1).
    SetSegmentStatistic {
        /// Target track source.
        source: SourceId,
        /// Previous statistic.
        before: SegmentStatistic,
        /// New statistic.
        after: SegmentStatistic,
    },
    /// Chooses whether the polar is fed from current-corrected (water)
    /// values where a current exists, or ground values (spec.md 7.5, D13).
    SetUseCorrected {
        /// Previous choice.
        before: bool,
        /// New choice.
        after: bool,
    },
    /// Chooses whether the current includes Stokes drift
    /// (spec.md 7.5, Q3). Applies to the next environment fetch.
    SetStokesDrift {
        /// Previous choice.
        before: bool,
        /// New choice.
        after: bool,
    },
    /// Replaces the blend settings (spec.md 8, 12): the Blend entry's colour
    /// and visibility, and the Blend settings dialog's sample counts,
    /// smoothing, current correction, Stokes drift and default statistic.
    SetBlendSettings {
        /// Previous settings.
        before: Box<BlendSettings>,
        /// New settings.
        after: Box<BlendSettings>,
    },
    /// Replaces the output grid (spec.md 12.2). Track segments are binned
    /// onto it again; an edit on a node the new grid lacks is kept in the
    /// overlay but has nothing to apply to.
    SetOutputGrid {
        /// Previous grid.
        before: OutputGrid,
        /// New grid.
        after: OutputGrid,
    },
    /// Several commands as one history entry, e.g. a multi-file import.
    Batch {
        /// The history label.
        label: String,
        /// Applied in order, undone in reverse.
        commands: Vec<Command>,
    },
}

fn stale(what: &str) -> CoreError {
    CoreError::Stale(what.to_owned())
}

fn source_mut(project: &mut Project, id: SourceId) -> Result<&mut Source> {
    project
        .source_mut(id)
        .ok_or(CoreError::MissingSource(id.raw()))
}

/// Replaces `field` with `to` if it currently holds `from`.
fn swap<T: PartialEq + Clone>(field: &mut T, from: &T, to: &T, what: &str) -> Result<()> {
    if field != from {
        return Err(stale(what));
    }
    *field = to.clone();
    Ok(())
}

impl Command {
    /// Applies the change.
    pub fn apply(&mut self, project: &mut Project) -> Result<()> {
        self.run(project, true)
    }

    /// Reverses the change.
    pub fn undo(&mut self, project: &mut Project) -> Result<()> {
        self.run(project, false)
    }

    fn run(&mut self, project: &mut Project, forward: bool) -> Result<()> {
        match self {
            Self::RenameBoat { before, after } => {
                let (from, to) = if forward {
                    (&*before, &*after)
                } else {
                    (&*after, &*before)
                };
                if forward && to.trim().is_empty() {
                    return Err(CoreError::Invalid("a boat needs a name".to_owned()));
                }
                swap(&mut project.boat.name, from, to, "boat name")
            }
            Self::RenameProject { before, after } => {
                let (from, to) = if forward {
                    (&*before, &*after)
                } else {
                    (&*after, &*before)
                };
                if to.trim().is_empty() {
                    return Err(CoreError::Invalid("a project needs a name".to_owned()));
                }
                swap(&mut project.name, from, to, "project name")
            }
            Self::AddSource { index, source } => {
                if forward {
                    insert(project, *index, source)
                } else {
                    **source = remove(project, *index, source.id)?;
                    Ok(())
                }
            }
            Self::RemoveSource { index, source } => {
                if forward {
                    // What was really there is what undo restores, overlay
                    // and all.
                    **source = remove(project, *index, source.id)?;
                    Ok(())
                } else {
                    insert(project, *index, source)
                }
            }
            Self::SetSourceColour {
                source,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (&*before, &*after)
                } else {
                    (&*after, &*before)
                };
                swap(
                    &mut source_mut(project, *source)?.colour,
                    from,
                    to,
                    "colour",
                )
            }
            Self::SetSourceVisible {
                source,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (*before, *after)
                } else {
                    (*after, *before)
                };
                swap(
                    &mut source_mut(project, *source)?.visible,
                    &from,
                    &to,
                    "visibility",
                )
            }
            Self::SetSourceWeight {
                source,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (*before, *after)
                } else {
                    (*after, *before)
                };
                validate_weight(to)?;
                swap(
                    &mut source_mut(project, *source)?.weight,
                    &from,
                    &to,
                    "weight",
                )
            }
            Self::SetSourceLabel {
                source,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (&*before, &*after)
                } else {
                    (&*after, &*before)
                };
                swap(&mut source_mut(project, *source)?.label, from, to, "label")
            }
            Self::MoveSource { from, to } => {
                let (from, to) = if forward { (*from, *to) } else { (*to, *from) };
                let len = project.sources.len();
                for index in [from, to] {
                    if index >= len {
                        return Err(CoreError::IndexOutOfBounds { index, len });
                    }
                }
                let moved = project.sources.remove(from);
                project.sources.insert(to, moved);
                Ok(())
            }
            Self::ExcludeCells { source, cells } => {
                let target = polar_source_mut(project, *source)?;
                let list = &mut target.overlay.excluded_cells;
                if forward {
                    add_cells(list, cells)
                } else {
                    remove_cells(list, cells)
                }
            }
            Self::IncludeCells { source, cells } => {
                let target = polar_source_mut(project, *source)?;
                let list = &mut target.overlay.excluded_cells;
                if forward {
                    remove_cells(list, cells)
                } else {
                    add_cells(list, cells)
                }
            }
            Self::ExcludeSamples { source, samples } => {
                let target = track_source_mut(project, *source)?;
                if forward {
                    add_samples(target, samples)
                } else {
                    remove_samples(target, samples)
                }
            }
            Self::IncludeSamples { source, samples } => {
                let target = track_source_mut(project, *source)?;
                if forward {
                    remove_samples(target, samples)
                } else {
                    add_samples(target, samples)
                }
            }
            Self::SetSampleFilters {
                source,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (&**before, &**after)
                } else {
                    (&**after, &**before)
                };
                to.validate()?;
                let target = track_source_mut(project, *source)?;
                swap(&mut target.overlay.filters, from, to, "sample filters")
            }
            Self::SetDerivation {
                source,
                before,
                after,
                motion_before,
                motion_after,
            } => {
                let (from, to, motion_from, motion_to) = if forward {
                    (&*before, &*after, &*motion_before, &*motion_after)
                } else {
                    (&*after, &*before, &*motion_after, &*motion_before)
                };
                to.validate()?;
                let target = track_source_mut(project, *source)?;
                let label = target.label.clone();
                let track = target
                    .track_mut()
                    .ok_or_else(|| CoreError::Invalid(format!("{label} is not a track")))?;
                set_derivation(track, from, to, motion_from, motion_to)
            }
            Self::EditCells { source, cells, .. } => {
                let target = source_mut(project, *source)?;
                edit_cells(&mut target.overlay.cell_overrides, cells, forward)
            }
            Self::SetSegmentStatistic {
                source,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (*before, *after)
                } else {
                    (*after, *before)
                };
                let target = track_source_mut(project, *source)?;
                let label = target.label.clone();
                let track = target
                    .track_mut()
                    .ok_or_else(|| CoreError::Invalid(format!("{label} is not a track")))?;
                swap(&mut track.statistic, &from, &to, "segment statistic")
            }
            Self::SetUseCorrected { before, after } => {
                let (from, to) = if forward {
                    (*before, *after)
                } else {
                    (*after, *before)
                };
                swap(
                    &mut project.blend.use_corrected,
                    &from,
                    &to,
                    "current correction",
                )
            }
            Self::SetStokesDrift { before, after } => {
                let (from, to) = if forward {
                    (*before, *after)
                } else {
                    (*after, *before)
                };
                swap(
                    &mut project.blend.include_stokes_drift,
                    &from,
                    &to,
                    "Stokes drift",
                )
            }
            Self::SetBlendSettings { before, after } => {
                let (from, to) = if forward {
                    (&**before, &**after)
                } else {
                    (&**after, &**before)
                };
                to.validate()?;
                swap(&mut project.blend, from, to, "blend settings")
            }
            Self::SetOutputGrid { before, after } => {
                let (from, to) = if forward {
                    (&*before, &*after)
                } else {
                    (&*after, &*before)
                };
                to.validate()?;
                swap(&mut project.grid, from, to, "output grid")
            }
            Self::Batch { commands, .. } => {
                if forward {
                    for (done, command) in commands.iter_mut().enumerate() {
                        if let Err(err) = command.apply(project) {
                            // Leave the document as it was: a half-applied
                            // batch would be an entry describing nothing the
                            // user did.
                            for earlier in commands.iter_mut().take(done).rev() {
                                earlier.undo(project)?;
                            }
                            return Err(err);
                        }
                    }
                } else {
                    for (done, command) in commands.iter_mut().rev().enumerate() {
                        if let Err(err) = command.undo(project) {
                            let len = commands.len();
                            for later in commands.iter_mut().skip(len - done) {
                                later.apply(project)?;
                            }
                            return Err(err);
                        }
                    }
                }
                Ok(())
            }
        }
    }

    /// Folds `next` into this command, for a coalesced gesture: the entry
    /// keeps its first `before` and takes the newest `after`, so one undo
    /// returns to where the gesture started. Returns whether it merged.
    pub fn merge(&mut self, next: &Self) -> bool {
        match (self, next) {
            (
                Self::SetSourceWeight { source, after, .. },
                Self::SetSourceWeight {
                    source: next_source,
                    after: next_after,
                    ..
                },
            ) if source == next_source => {
                *after = *next_after;
                true
            }
            (
                Self::SetSourceColour { source, after, .. },
                Self::SetSourceColour {
                    source: next_source,
                    after: next_after,
                    ..
                },
            ) if source == next_source => {
                *after = next_after.clone();
                true
            }
            (
                Self::EditCells {
                    source,
                    action: EditAction::Drag,
                    cells,
                },
                Self::EditCells {
                    source: next_source,
                    action: EditAction::Drag,
                    cells: next_cells,
                },
            ) if source == next_source
                && cells.len() == next_cells.len()
                && cells
                    .iter()
                    .zip(next_cells)
                    .all(|(a, b)| a.cell().order(&b.cell()) == std::cmp::Ordering::Equal) =>
            {
                // One drag is one entry: its first `before`, its last `after`.
                for (cell, next) in cells.iter_mut().zip(next_cells) {
                    cell.after = next.after;
                }
                true
            }
            _ => false,
        }
    }

    /// The history label: English text the UI translates as a key
    /// (spec.md 3.5).
    pub fn label(&self) -> String {
        match self {
            Self::RenameBoat { .. } => "Rename boat",
            Self::RenameProject { .. } => "Rename project",
            Self::AddSource { .. } => "Add source",
            Self::RemoveSource { .. } => "Remove source",
            Self::SetSourceColour { .. } => "Change source colour",
            Self::SetSourceVisible { after: true, .. } => "Show source",
            Self::SetSourceVisible { after: false, .. } => "Hide source",
            Self::SetSourceWeight { .. } => "Change source weight",
            Self::SetSourceLabel { .. } => "Rename source",
            Self::MoveSource { .. } => "Reorder sources",
            Self::ExcludeCells { .. } => EXCLUDE_NODES_LABEL,
            Self::IncludeCells { .. } => INCLUDE_NODES_LABEL,
            Self::ExcludeSamples { .. } => EXCLUDE_SAMPLES_LABEL,
            Self::IncludeSamples { .. } => INCLUDE_SAMPLES_LABEL,
            Self::SetSampleFilters { .. } => "Change sample filters",
            Self::SetDerivation { .. } => "Change heading and speed derivation",
            Self::EditCells { action, .. } => match action {
                EditAction::Drag => "Move polar node",
                EditAction::Type => "Type polar value",
                EditAction::Scale => "Scale polar cells",
                EditAction::Smooth => "Smooth polar cells",
                EditAction::Reset => "Reset polar cells",
                EditAction::ResetAll => "Reset all edits",
            },
            Self::SetSegmentStatistic { .. } => "Change segment statistic",
            Self::SetUseCorrected { .. } => "Change current correction",
            Self::SetStokesDrift { .. } => "Change Stokes drift",
            Self::SetBlendSettings { before, after } => blend_label(before, after),
            Self::SetOutputGrid { .. } => "Change the output grid",
            Self::Batch { label, .. } => return label.clone(),
        }
        .to_owned()
    }
}

/// The history label of a change to the blend settings made in the dialog,
/// also used for a batch of it with a new output grid.
pub const BLEND_SETTINGS_LABEL: &str = "Change blend settings";

/// Names a blend settings change by what it changes: the Blend entry's
/// switch and colour have their own entries (spec.md 8).
fn blend_label(before: &BlendSettings, after: &BlendSettings) -> &'static str {
    let only = |f: fn(&mut BlendSettings, &BlendSettings)| {
        let mut probe = before.clone();
        f(&mut probe, after);
        probe == *after
    };
    if before.visible != after.visible && only(|b, a| b.visible = a.visible) {
        if after.visible {
            "Show blend"
        } else {
            "Hide blend"
        }
    } else if before.colour != after.colour && only(|b, a| b.colour = a.colour.clone()) {
        "Change blend colour"
    } else if before.corrections != after.corrections
        && only(|b, a| b.corrections = a.corrections.clone())
    {
        "Correct the blend"
    } else if before.global_filters != after.global_filters
        && only(|b, a| b.global_filters = a.global_filters.clone())
    {
        "Change global point filters"
    } else {
        BLEND_SETTINGS_LABEL
    }
}

/// The history label of an exclusion, also used for a batch of them over
/// several sources.
pub const EXCLUDE_NODES_LABEL: &str = "Exclude polar nodes";
/// The history label of an inclusion.
pub const INCLUDE_NODES_LABEL: &str = "Include polar nodes";

/// The history label of a sample exclusion.
pub const EXCLUDE_SAMPLES_LABEL: &str = "Exclude samples";
/// The history label of a sample inclusion.
pub const INCLUDE_SAMPLES_LABEL: &str = "Include samples";
/// The history label of an exclusion over polar nodes and samples together.
pub const EXCLUDE_DOTS_LABEL: &str = "Exclude dots";
/// The history label of an inclusion over polar nodes and samples together.
pub const INCLUDE_DOTS_LABEL: &str = "Include dots";

/// Moves each cell's override from `before` to `after` (forward) or back.
/// Checks everything first, so a refusal changes nothing.
fn edit_cells(list: &mut Vec<CellOverride>, cells: &[CellEdit], forward: bool) -> Result<()> {
    let refs: Vec<CellRef> = cells.iter().map(CellEdit::cell).collect();
    // The same checks as an exclusion: at least one, valid, each once.
    check_cells(&refs, "an edit")?;
    let find = |list: &[CellOverride], cell: &CellRef| {
        list.binary_search_by(|held| held.cell().order(cell))
    };
    for edit in cells {
        let (from, to) = if forward {
            (edit.before, edit.after)
        } else {
            (edit.after, edit.before)
        };
        if let Some(value) = to {
            validate_edit_bsp(value)?;
        }
        let held = find(list, &edit.cell())
            .ok()
            .and_then(|at| list.get(at))
            .map(|o| o.bsp);
        if held != from {
            return Err(stale("the polar edits"));
        }
    }
    for edit in cells {
        let to = if forward { edit.after } else { edit.before };
        let cell = edit.cell();
        match (find(list, &cell), to) {
            (Ok(at), Some(bsp)) => {
                if let Some(held) = list.get_mut(at) {
                    held.bsp = bsp;
                }
            }
            (Ok(at), None) => {
                list.remove(at);
            }
            (Err(at), Some(bsp)) => list.insert(
                at,
                CellOverride {
                    twa: cell.twa,
                    tws: cell.tws,
                    bsp,
                },
            ),
            (Err(_), None) => {}
        }
    }
    Ok(())
}

/// A track source.
fn track_source_mut(project: &mut Project, id: SourceId) -> Result<&mut Source> {
    let source = source_mut(project, id)?;
    if !matches!(source.kind, SourceKind::Track { .. }) {
        return Err(CoreError::Invalid(format!(
            "{} is not a track; it has no samples",
            source.label
        )));
    }
    Ok(source)
}

/// Checks `samples` is a non-empty set of the track's own samples, each
/// named once, and returns them sorted.
fn checked_samples(source: &Source, samples: &[SampleId]) -> Result<Vec<SampleId>> {
    if samples.is_empty() {
        return Err(CoreError::Invalid(
            "an exclusion names at least one sample".to_owned(),
        ));
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(CoreError::Invalid(
            "an exclusion names the same sample twice".to_owned(),
        ));
    }
    let mut own: Vec<SampleId> = source
        .track()
        .map(|track| track.samples.iter().map(|s| s.id).collect())
        .unwrap_or_default();
    own.sort_unstable();
    if let Some(stranger) = sorted.iter().find(|id| own.binary_search(id).is_err()) {
        return Err(CoreError::Invalid(format!(
            "sample {stranger} is not one of {}'s",
            source.label
        )));
    }
    Ok(sorted)
}

/// Adds `samples` to the source's sorted exclusion list; refuses (changing
/// nothing) if any is already there.
fn add_samples(source: &mut Source, samples: &[SampleId]) -> Result<()> {
    let sorted = checked_samples(source, samples)?;
    let list = &mut source.overlay.excluded_samples;
    if sorted.iter().any(|id| list.binary_search(id).is_ok()) {
        return Err(stale("the excluded samples"));
    }
    // A merge, not an insert per id: a lasso can take a hundred thousand.
    list.extend(sorted);
    list.sort_unstable();
    Ok(())
}

/// Removes `samples` from the source's sorted exclusion list; refuses
/// (changing nothing) if any is not there.
fn remove_samples(source: &mut Source, samples: &[SampleId]) -> Result<()> {
    let sorted = checked_samples(source, samples)?;
    let list = &mut source.overlay.excluded_samples;
    if sorted.iter().any(|id| list.binary_search(id).is_err()) {
        return Err(stale("the excluded samples"));
    }
    list.retain(|id| sorted.binary_search(id).is_err());
    Ok(())
}

/// Moves a track from one derivation to another, checking it is in the
/// first before writing the second.
fn set_derivation(
    track: &mut Track,
    from: &DerivationSettings,
    to: &DerivationSettings,
    motion_from: &[Motion],
    motion_to: &[Motion],
) -> Result<()> {
    if track.derivation != *from
        || motion_from.len() != track.samples.len()
        || motion_to.len() != track.samples.len()
        || track
            .samples
            .iter()
            .zip(motion_from)
            .any(|(sample, motion)| sample.motion() != *motion)
    {
        return Err(stale("the track's heading and speed"));
    }
    track.derivation = to.clone();
    // What relates the motion to the stored environment follows it, from
    // the stored values, so the angles never go stale and nothing is
    // fetched again (M8 carry).
    for (sample, motion) in track.samples.iter_mut().zip(motion_to) {
        sample.set_motion(*motion);
        sample.downloaded_wind_only = to.downloaded_wind_only;
        sample.relate();
    }
    Ok(())
}

/// A source whose polar nodes can be excluded: an ORC or file polar. A
/// track's positions are excluded one sample at a time instead (spec.md 10.3).
fn polar_source_mut(project: &mut Project, id: SourceId) -> Result<&mut Source> {
    let source = source_mut(project, id)?;
    if matches!(source.kind, SourceKind::Track { .. }) {
        return Err(CoreError::Invalid(format!(
            "{} is a track; its positions are excluded sample by sample, not as polar nodes",
            source.label
        )));
    }
    Ok(source)
}

/// Checks `cells` is a non-empty set of valid cells, each named once.
fn check_cells(cells: &[CellRef], what: &str) -> Result<()> {
    if cells.is_empty() {
        return Err(CoreError::Invalid(format!(
            "{what} names at least one polar node"
        )));
    }
    let mut sorted: Vec<&CellRef> = cells.iter().collect();
    sorted.sort_by(|a, b| a.order(b));
    for cell in &sorted {
        cell.validate()?;
    }
    if sorted
        .windows(2)
        .any(|pair| pair[0].order(pair[1]) == std::cmp::Ordering::Equal)
    {
        return Err(CoreError::Invalid(format!(
            "{what} names the same polar node twice"
        )));
    }
    Ok(())
}

/// Adds `cells` to a sorted list, keeping it sorted; refuses (changing
/// nothing) if any is already there.
fn add_cells(list: &mut Vec<CellRef>, cells: &[CellRef]) -> Result<()> {
    check_cells(cells, "an exclusion")?;
    if cells
        .iter()
        .any(|cell| list.binary_search_by(|held| held.order(cell)).is_ok())
    {
        return Err(stale("the excluded polar nodes"));
    }
    for cell in cells {
        let at = list.partition_point(|held| held.order(cell) == std::cmp::Ordering::Less);
        list.insert(at, cell.clone());
    }
    Ok(())
}

/// Removes `cells` from a sorted list; refuses (changing nothing) if any is
/// not there.
fn remove_cells(list: &mut Vec<CellRef>, cells: &[CellRef]) -> Result<()> {
    check_cells(cells, "an exclusion")?;
    if cells
        .iter()
        .any(|cell| list.binary_search_by(|held| held.order(cell)).is_err())
    {
        return Err(stale("the excluded polar nodes"));
    }
    list.retain(|held| {
        !cells
            .iter()
            .any(|cell| held.order(cell) == std::cmp::Ordering::Equal)
    });
    Ok(())
}

fn insert(project: &mut Project, index: usize, source: &Source) -> Result<()> {
    let len = project.sources.len();
    if index > len {
        return Err(CoreError::IndexOutOfBounds { index, len });
    }
    if project.source(source.id).is_some() {
        return Err(stale("a source with that id is already in the project"));
    }
    source.validate()?;
    project.sources.insert(index, source.clone());
    Ok(())
}

fn remove(project: &mut Project, index: usize, id: SourceId) -> Result<Source> {
    let len = project.sources.len();
    match project.sources.get(index) {
        None => Err(CoreError::IndexOutOfBounds { index, len }),
        Some(at) if at.id != id => Err(stale("the source list")),
        Some(_) => Ok(project.sources.remove(index)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::fixtures;
    use crate::polar::{PolarFileFormat, PolarGrid};

    /// Every variant, by name. The match has no wildcard, so a new variant
    /// does not compile until it is named here — and then
    /// `every_command_has_an_undo_inverse` fails until it has an example.
    fn variant(command: &Command) -> &'static str {
        match command {
            Command::RenameBoat { .. } => "RenameBoat",
            Command::RenameProject { .. } => "RenameProject",
            Command::AddSource { .. } => "AddSource",
            Command::RemoveSource { .. } => "RemoveSource",
            Command::SetSourceColour { .. } => "SetSourceColour",
            Command::SetSourceVisible { .. } => "SetSourceVisible",
            Command::SetSourceWeight { .. } => "SetSourceWeight",
            Command::SetSourceLabel { .. } => "SetSourceLabel",
            Command::MoveSource { .. } => "MoveSource",
            Command::ExcludeCells { .. } => "ExcludeCells",
            Command::IncludeCells { .. } => "IncludeCells",
            Command::ExcludeSamples { .. } => "ExcludeSamples",
            Command::IncludeSamples { .. } => "IncludeSamples",
            Command::SetSampleFilters { .. } => "SetSampleFilters",
            Command::SetDerivation { .. } => "SetDerivation",
            Command::EditCells { .. } => "EditCells",
            Command::SetSegmentStatistic { .. } => "SetSegmentStatistic",
            Command::SetUseCorrected { .. } => "SetUseCorrected",
            Command::SetStokesDrift { .. } => "SetStokesDrift",
            Command::SetBlendSettings { .. } => "SetBlendSettings",
            Command::SetOutputGrid { .. } => "SetOutputGrid",
            Command::Batch { .. } => "Batch",
        }
    }
    const VARIANTS: usize = 22;

    fn edit(twa: f64, tws: f64, before: Option<f64>, after: Option<f64>) -> CellEdit {
        CellEdit {
            twa,
            tws,
            before,
            after,
        }
    }

    fn cell(twa: f64, tws: f64) -> CellRef {
        CellRef { twa, tws }
    }

    fn new_polar(project: &mut Project) -> Source {
        let id = project.allocate_source_id();
        let colour = project.next_palette_colour();
        Source::new(
            id,
            "new.pol",
            colour,
            SourceKind::PolarFile {
                format: PolarFileFormat::Adrena,
                file_name: "new.pol".to_owned(),
                polar: PolarGrid::empty(vec![60.0], vec![8.0]),
                matrix: None,
            },
        )
    }

    /// One example of every command, each built against `project`.
    fn every_command(project: &mut Project) -> Vec<Command> {
        let first = project.sources[0].id;
        // A node already excluded, for the inclusion to include.
        project.sources[1].overlay.excluded_cells = vec![cell(0.0, 6.0), cell(45.0, 12.0)];
        let track = project.sources[2].clone();
        let track_id = track.id;
        let sample_ids: Vec<SampleId> = track
            .track()
            .unwrap()
            .samples
            .iter()
            .map(|s| s.id)
            .collect();
        // One sample already excluded, for the inclusion to include.
        project.sources[2].overlay.excluded_samples = vec![sample_ids[1]];
        let motion_before: Vec<Motion> = track
            .track()
            .unwrap()
            .samples
            .iter()
            .map(|s| s.motion())
            .collect();
        let motion_after: Vec<Motion> = motion_before
            .iter()
            .map(|m| Motion {
                speed: Some(6.5),
                speed_origin: Some(crate::track::ValueOrigin::Derived),
                ..*m
            })
            .collect();
        let added = new_polar(project);
        let also = new_polar(project);
        vec![
            Command::RenameBoat {
                before: project.boat.name.clone(),
                after: "New boat name".to_owned(),
            },
            Command::RenameProject {
                before: "Fixture".to_owned(),
                after: "Fastnet 2025".to_owned(),
            },
            Command::AddSource {
                index: 1,
                source: Box::new(added),
            },
            Command::RemoveSource {
                index: 2,
                source: Box::new(track),
            },
            Command::SetSourceColour {
                source: first,
                before: project.sources[0].colour.clone(),
                after: Colour::parse("#123456").unwrap(),
            },
            Command::SetSourceVisible {
                source: first,
                before: true,
                after: false,
            },
            Command::SetSourceWeight {
                source: first,
                before: 1.0,
                after: 0.75,
            },
            Command::SetSourceLabel {
                source: first,
                before: "ORC".to_owned(),
                after: "Sister ship".to_owned(),
            },
            Command::MoveSource { from: 0, to: 2 },
            Command::ExcludeCells {
                source: first,
                cells: vec![cell(150.0, 6.0), cell(52.0, 12.0)],
            },
            Command::IncludeCells {
                source: project.sources[1].id,
                cells: vec![cell(45.0, 12.0)],
            },
            Command::ExcludeSamples {
                source: track_id,
                samples: vec![sample_ids[2], sample_ids[0]],
            },
            Command::IncludeSamples {
                source: track_id,
                samples: vec![sample_ids[1]],
            },
            Command::SetSampleFilters {
                source: track_id,
                before: Box::new(SampleFilters::default()),
                after: Box::new(SampleFilters {
                    min_bsp_kn: Some(2.0),
                    max_heading_change_deg: None,
                    ..SampleFilters::default()
                }),
            },
            Command::SetDerivation {
                source: track_id,
                before: DerivationSettings::default(),
                after: DerivationSettings {
                    downloaded_wind_only: false,
                    max_gap_s: 600,
                    prefer_heading: crate::track::PreferValues::Derived,
                    prefer_speed: crate::track::PreferValues::Given,
                },
                motion_before,
                motion_after,
            },
            Command::EditCells {
                source: first,
                action: EditAction::Type,
                cells: vec![edit(52.0, 6.0, None, Some(6.1))],
            },
            Command::SetSegmentStatistic {
                source: track_id,
                before: SegmentStatistic::P90,
                after: SegmentStatistic::Median,
            },
            Command::SetUseCorrected {
                before: true,
                after: false,
            },
            Command::SetStokesDrift {
                before: false,
                after: true,
            },
            Command::SetBlendSettings {
                before: Box::new(project.blend.clone()),
                after: Box::new(BlendSettings {
                    n_full: 12,
                    smoothing: true,
                    default_statistic: SegmentStatistic::Median,
                    ..project.blend.clone()
                }),
            },
            Command::SetOutputGrid {
                before: project.grid.clone(),
                after: OutputGrid {
                    twa: vec![0.0, 45.0, 90.0, 135.0, 180.0],
                    tws: vec![6.0, 12.5, 20.0],
                },
            },
            Command::Batch {
                label: "Import polar files".to_owned(),
                commands: vec![
                    Command::AddSource {
                        index: 3,
                        source: Box::new(also),
                    },
                    Command::SetSourceVisible {
                        source: first,
                        before: true,
                        after: false,
                    },
                ],
            },
        ]
    }

    /// Spec 4.6 and the testing rules: every command has an exact inverse.
    /// Apply changes the document, undo restores it exactly, and redo gives
    /// exactly the applied state again.
    #[test]
    fn every_command_has_an_undo_inverse() {
        let mut template = fixtures::project();
        let commands = every_command(&mut template);
        let names: BTreeSet<&str> = commands.iter().map(variant).collect();
        assert_eq!(names.len(), VARIANTS, "an example per variant: {names:?}");

        for mut command in commands {
            let mut project = template.clone();
            let before = project.clone();
            command.apply(&mut project).unwrap();
            assert_ne!(project, before, "{} changed nothing", variant(&command));
            let applied = project.clone();

            command.undo(&mut project).unwrap();
            assert_eq!(
                project,
                before,
                "{} did not undo exactly",
                variant(&command)
            );

            command.apply(&mut project).unwrap();
            assert_eq!(
                project,
                applied,
                "{} did not redo exactly",
                variant(&command)
            );
            assert!(!command.label().is_empty());
        }
    }

    /// The inverse also holds through a save and a reload, where the bulk
    /// track data travels separately.
    #[test]
    fn removing_a_track_and_undoing_restores_its_samples() {
        let mut project = fixtures::project();
        let before = project.clone();
        let source = project.sources[2].clone();
        let mut command = Command::RemoveSource {
            index: 2,
            source: Box::new(source),
        };
        command.apply(&mut project).unwrap();
        assert_eq!(project.sources.len(), 2);
        command.undo(&mut project).unwrap();
        assert_eq!(project, before);
        assert_eq!(project.sources[2].track().unwrap().samples.len(), 3);
    }

    /// The Blend entry's switch and colour name their own history entries;
    /// anything else is a blend settings change. A grid the editor would
    /// refuse is refused here too.
    #[test]
    fn blend_settings_commands_name_what_they_change_and_refuse_bad_values() {
        let project = fixtures::project();
        let base = project.blend.clone();
        let with = |f: fn(&mut BlendSettings)| {
            let mut after = base.clone();
            f(&mut after);
            Command::SetBlendSettings {
                before: Box::new(base.clone()),
                after: Box::new(after),
            }
        };
        assert_eq!(with(|b| b.visible = false).label(), "Hide blend");
        assert_eq!(
            with(|b| b.colour = Colour::trusted("#123456")).label(),
            "Change blend colour"
        );
        assert_eq!(
            with(|b| {
                b.visible = false;
                b.n_full = 3;
            })
            .label(),
            BLEND_SETTINGS_LABEL
        );
        let mut bad = with(|b| b.min_samples = 0);
        assert!(bad.apply(&mut project.clone()).is_err());

        let mut grid = Command::SetOutputGrid {
            before: project.grid.clone(),
            after: OutputGrid {
                twa: vec![40.0, 40.004],
                tws: vec![10.0],
            },
        };
        let mut target = project.clone();
        assert!(grid.apply(&mut target).is_err());
        assert_eq!(target, project);
    }

    #[test]
    fn a_command_built_against_another_state_is_refused() {
        let mut project = fixtures::project();
        let first = project.sources[0].id;
        let mut stale = Command::SetSourceLabel {
            source: first,
            before: "not the label".to_owned(),
            after: "x".to_owned(),
        };
        assert!(matches!(
            stale.apply(&mut project),
            Err(CoreError::Stale(_))
        ));
        let mut missing = Command::SetSourceVisible {
            source: SourceId(9999),
            before: true,
            after: false,
        };
        assert!(matches!(
            missing.apply(&mut project),
            Err(CoreError::MissingSource(9999))
        ));
        let mut wrong_index = Command::RemoveSource {
            index: 0,
            source: Box::new(project.sources[1].clone()),
        };
        assert!(wrong_index.apply(&mut project).is_err());
        let mut out_of_range = Command::MoveSource { from: 0, to: 7 };
        assert!(matches!(
            out_of_range.apply(&mut project),
            Err(CoreError::IndexOutOfBounds { index: 7, len: 3 })
        ));
    }

    #[test]
    fn invalid_values_are_refused() {
        let mut project = fixtures::project();
        let first = project.sources[0].id;
        let mut heavy = Command::SetSourceWeight {
            source: first,
            before: 1.0,
            after: 1.5,
        };
        assert!(heavy.apply(&mut project).is_err());
        let mut blank = Command::RenameProject {
            before: "Fixture".to_owned(),
            after: "  ".to_owned(),
        };
        assert!(blank.apply(&mut project).is_err());
        let duplicate = project.sources[0].clone();
        let mut twice = Command::AddSource {
            index: 0,
            source: Box::new(duplicate),
        };
        assert!(twice.apply(&mut project).is_err());
    }

    /// A batch that fails part-way leaves the document as it was.
    #[test]
    fn a_failing_batch_rolls_back() {
        let mut project = fixtures::project();
        let before = project.clone();
        let first = project.sources[0].id;
        let mut batch = Command::Batch {
            label: "Both".to_owned(),
            commands: vec![
                Command::SetSourceVisible {
                    source: first,
                    before: true,
                    after: false,
                },
                Command::SetSourceVisible {
                    source: SourceId(9999),
                    before: true,
                    after: false,
                },
            ],
        };
        assert!(batch.apply(&mut project).is_err());
        assert_eq!(project, before);
    }

    #[test]
    fn weight_and_colour_drags_merge_but_other_sources_do_not() {
        let project = fixtures::project();
        let (a, b) = (project.sources[0].id, project.sources[1].id);
        let mut first = Command::SetSourceWeight {
            source: a,
            before: 1.0,
            after: 0.8,
        };
        assert!(first.merge(&Command::SetSourceWeight {
            source: a,
            before: 0.8,
            after: 0.6,
        }));
        assert_eq!(
            first,
            Command::SetSourceWeight {
                source: a,
                before: 1.0,
                after: 0.6
            }
        );
        assert!(!first.merge(&Command::SetSourceWeight {
            source: b,
            before: 1.0,
            after: 0.5,
        }));
        assert!(!first.merge(&Command::MoveSource { from: 0, to: 1 }));
    }

    /// Excluding keeps the list sorted whatever order the nodes came in, and
    /// the inclusion of some of them is undone back to the same list.
    #[test]
    fn exclusions_keep_their_order_and_include_undoes_exactly() {
        let mut project = fixtures::project();
        let original = project.clone();
        let orc = project.sources[0].id;
        let mut exclude = Command::ExcludeCells {
            source: orc,
            cells: vec![cell(150.0, 6.0), cell(52.0, 12.0), cell(52.0, 6.0)],
        };
        exclude.apply(&mut project).unwrap();
        assert_eq!(
            project.sources[0].overlay.excluded_cells,
            vec![cell(52.0, 6.0), cell(52.0, 12.0), cell(150.0, 6.0)]
        );
        assert!(project.sources[0].overlay.is_cell_excluded(52.0, 12.0));
        assert!(!project.sources[0].overlay.is_cell_excluded(90.0, 12.0));
        project.validate().unwrap();

        let excluded = project.clone();
        let mut include = Command::IncludeCells {
            source: orc,
            cells: vec![cell(52.0, 12.0)],
        };
        include.apply(&mut project).unwrap();
        assert_eq!(
            project.sources[0].overlay.excluded_cells,
            vec![cell(52.0, 6.0), cell(150.0, 6.0)]
        );
        include.undo(&mut project).unwrap();
        assert_eq!(project, excluded);

        // Undoing the exclusion gives the source back exactly (invariant 1).
        exclude.undo(&mut project).unwrap();
        assert_eq!(project, original);
        assert!(project.sources[0].overlay.is_empty());
    }

    #[test]
    fn a_node_excluded_twice_or_included_while_not_excluded_is_refused() {
        let mut project = fixtures::project();
        let orc = project.sources[0].id;
        Command::ExcludeCells {
            source: orc,
            cells: vec![cell(90.0, 6.0)],
        }
        .apply(&mut project)
        .unwrap();
        let before = project.clone();

        let mut again = Command::ExcludeCells {
            source: orc,
            cells: vec![cell(52.0, 6.0), cell(90.0, 6.0)],
        };
        assert!(matches!(
            again.apply(&mut project),
            Err(CoreError::Stale(_))
        ));
        let mut absent = Command::IncludeCells {
            source: orc,
            cells: vec![cell(90.0, 6.0), cell(90.0, 12.0)],
        };
        assert!(matches!(
            absent.apply(&mut project),
            Err(CoreError::Stale(_))
        ));
        // A refusal changes nothing, not even the nodes it could have taken.
        assert_eq!(project, before);

        for cells in [
            vec![],
            vec![cell(52.0, 12.0), cell(52.0, 12.0)],
            vec![cell(361.0, 12.0)],
            vec![cell(f64::NAN, 12.0)],
            vec![cell(52.0, -1.0)],
        ] {
            let mut bad = Command::ExcludeCells { source: orc, cells };
            assert!(matches!(
                bad.apply(&mut project),
                Err(CoreError::Invalid(_))
            ));
        }
        let mut track = Command::ExcludeCells {
            source: project.sources[2].id,
            cells: vec![cell(90.0, 12.0)],
        };
        assert!(matches!(
            track.apply(&mut project),
            Err(CoreError::Invalid(_))
        ));
        assert_eq!(project, before);
    }

    /// Excluded samples stay sorted, and refusals change nothing.
    #[test]
    fn sample_exclusions_keep_their_order_and_refuse_strangers() {
        let mut project = fixtures::project();
        project.sources[2].overlay.excluded_samples.clear();
        let original = project.clone();
        let track = project.sources[2].id;
        let ids: Vec<SampleId> = project.sources[2]
            .track()
            .unwrap()
            .samples
            .iter()
            .map(|s| s.id)
            .collect();
        let mut exclude = Command::ExcludeSamples {
            source: track,
            samples: vec![ids[2], ids[0]],
        };
        exclude.apply(&mut project).unwrap();
        assert_eq!(
            project.sources[2].overlay.excluded_samples,
            vec![ids[0], ids[2]]
        );
        project.validate().unwrap();
        let before = project.clone();
        for samples in [
            vec![],
            vec![ids[1], ids[1]],
            vec![SampleId(999_999)],
            vec![ids[0], ids[1]],
        ] {
            let mut bad = Command::ExcludeSamples {
                source: track,
                samples,
            };
            assert!(bad.apply(&mut project).is_err());
        }
        let mut absent = Command::IncludeSamples {
            source: track,
            samples: vec![ids[1]],
        };
        assert!(matches!(
            absent.apply(&mut project),
            Err(CoreError::Stale(_))
        ));
        let mut polar = Command::ExcludeSamples {
            source: project.sources[0].id,
            samples: vec![ids[1]],
        };
        assert!(matches!(
            polar.apply(&mut project),
            Err(CoreError::Invalid(_))
        ));
        assert_eq!(project, before);
        exclude.undo(&mut project).unwrap();
        assert_eq!(project, original);
    }

    /// A derivation change recomputes what relates the motion to the stored
    /// wind (M8 carry): with the wind from 270° a heading of 45° is TWA 135°
    /// port, a heading of 90° is dead downwind (TWA 180°, no tack), and undo
    /// gives back 135° port. Nothing is fetched: the wind is as stored.
    #[test]
    fn a_derivation_change_relates_the_new_heading_to_the_stored_wind() {
        let mut project = fixtures::project();
        let original = project.clone();
        let source = project.sources[2].id;
        let track = project.sources[2].track().unwrap();
        let before: Vec<Motion> = track.samples.iter().map(|s| s.motion()).collect();
        let after: Vec<Motion> = before
            .iter()
            .map(|m| Motion {
                heading: Some(90.0),
                ..*m
            })
            .collect();
        let mut command = Command::SetDerivation {
            source,
            before: DerivationSettings::default(),
            after: DerivationSettings {
                max_gap_s: 600,
                ..DerivationSettings::default()
            },
            motion_before: before,
            motion_after: after,
        };
        command.apply(&mut project).unwrap();
        let samples = &project.sources[2].track().unwrap().samples;
        assert!(
            samples
                .iter()
                .all(|s| s.twa == Some(180.0) && s.tack.is_none())
        );
        assert!(
            samples.iter().all(|s| s.tws == Some(14.2)),
            "the wind is as stored"
        );
        command.undo(&mut project).unwrap();
        assert!(project == original, "undo restores the related values");
    }

    /// A derivation change refuses when the samples no longer hold what it
    /// was built against, and invalid filters are refused.
    #[test]
    fn stale_derivations_and_invalid_filters_are_refused() {
        let mut project = fixtures::project();
        let track = project.sources[2].id;
        let n = project.sources[2].track().unwrap().samples.len();
        let mut stale_motion = Command::SetDerivation {
            source: track,
            before: DerivationSettings::default(),
            after: DerivationSettings::default(),
            motion_before: vec![Motion::default(); n],
            motion_after: vec![Motion::default(); n],
        };
        assert!(matches!(
            stale_motion.apply(&mut project),
            Err(CoreError::Stale(_))
        ));
        let mut bad_gap = Command::SetDerivation {
            source: track,
            before: DerivationSettings::default(),
            after: DerivationSettings {
                max_gap_s: 0,
                ..DerivationSettings::default()
            },
            motion_before: vec![],
            motion_after: vec![],
        };
        assert!(matches!(
            bad_gap.apply(&mut project),
            Err(CoreError::Invalid(_))
        ));
        let mut bad_filters = Command::SetSampleFilters {
            source: track,
            before: Box::new(SampleFilters::default()),
            after: Box::new(SampleFilters {
                min_bsp_kn: Some(-1.0),
                ..SampleFilters::default()
            }),
        };
        assert!(matches!(
            bad_filters.apply(&mut project),
            Err(CoreError::Invalid(_))
        ));
    }

    /// Exclusions survive a save and reload byte for byte, and a list out of
    /// order is refused rather than silently reordered.
    #[test]
    fn exclusions_round_trip_through_a_project_file() {
        let mut project = fixtures::project();
        Command::ExcludeCells {
            source: project.sources[1].id,
            cells: vec![cell(90.0, 6.0), cell(45.0, 6.0), cell(45.0, 12.0)],
        }
        .apply(&mut project)
        .unwrap();
        let bytes = crate::io::to_bytes(&project).unwrap();
        let loaded = crate::io::from_bytes(&bytes).unwrap();
        assert_eq!(loaded, project);
        assert_eq!(crate::io::to_bytes(&loaded).unwrap(), bytes);

        let mut unsorted = project.clone();
        unsorted.sources[1].overlay.excluded_cells.swap(0, 2);
        assert!(unsorted.validate().is_err());
        assert!(crate::io::to_bytes(&unsorted).is_err());
    }

    /// Invariant 1 and the M13 acceptance: edits are overlays, kept sorted
    /// one per cell, and clearing every one of them gives the source back
    /// byte for byte through a project file.
    #[test]
    fn removing_every_edit_restores_the_source_byte_for_byte() {
        let mut project = fixtures::project();
        let original = crate::io::to_bytes(&project).unwrap();
        let orc = project.sources[0].id;
        let mut first = Command::EditCells {
            source: orc,
            action: EditAction::Type,
            cells: vec![
                edit(90.0, 12.0, None, Some(8.5)),
                edit(52.0, 6.0, None, Some(5.25)),
            ],
        };
        first.apply(&mut project).unwrap();
        let overlay = &project.sources[0].overlay;
        assert_eq!(
            overlay
                .cell_overrides
                .iter()
                .map(|o| (o.twa, o.tws, o.bsp))
                .collect::<Vec<_>>(),
            [(52.0, 6.0, 5.25), (90.0, 12.0, 8.5)]
        );
        assert_eq!(overlay.override_at(90.0, 12.0), Some(8.5));
        assert_eq!(overlay.override_at(90.0, 6.0), None);
        project.validate().unwrap();

        // Changing one and then resetting both, as the tools would.
        Command::EditCells {
            source: orc,
            action: EditAction::Scale,
            cells: vec![edit(52.0, 6.0, Some(5.25), Some(5.5))],
        }
        .apply(&mut project)
        .unwrap();
        let edited = crate::io::to_bytes(&project).unwrap();
        assert_ne!(edited, original);
        Command::EditCells {
            source: orc,
            action: EditAction::ResetAll,
            cells: vec![
                edit(52.0, 6.0, Some(5.5), None),
                edit(90.0, 12.0, Some(8.5), None),
            ],
        }
        .apply(&mut project)
        .unwrap();
        assert!(project.sources[0].overlay.cell_overrides.is_empty());
        assert_eq!(crate::io::to_bytes(&project).unwrap(), original);
    }

    /// An edit built against other overrides refuses and changes nothing;
    /// speeds a polar cannot hold are refused.
    #[test]
    fn stale_or_impossible_edits_are_refused() {
        let mut project = fixtures::project();
        let orc = project.sources[0].id;
        Command::EditCells {
            source: orc,
            action: EditAction::Type,
            cells: vec![edit(52.0, 6.0, None, Some(5.0))],
        }
        .apply(&mut project)
        .unwrap();
        let before = project.clone();
        for cells in [
            // The second is stale: it has an override of 5.0, not none.
            vec![
                edit(90.0, 6.0, None, Some(7.0)),
                edit(52.0, 6.0, None, Some(6.0)),
            ],
            vec![edit(90.0, 6.0, None, Some(-1.0))],
            vec![edit(90.0, 6.0, None, Some(61.0))],
            vec![edit(90.0, 6.0, None, Some(f64::NAN))],
            vec![
                edit(90.0, 6.0, None, Some(1.0)),
                edit(90.0, 6.0, None, Some(2.0)),
            ],
            vec![edit(361.0, 6.0, None, Some(1.0))],
            vec![],
        ] {
            let mut bad = Command::EditCells {
                source: orc,
                action: EditAction::Type,
                cells,
            };
            assert!(bad.apply(&mut project).is_err());
            assert_eq!(project, before);
        }
        // An unsorted list read from a file is refused, not reordered.
        let mut unsorted = project.clone();
        unsorted.sources[0].overlay.cell_overrides.insert(
            0,
            CellOverride {
                twa: 90.0,
                tws: 6.0,
                bsp: 7.0,
            },
        );
        assert!(unsorted.validate().is_err());
    }

    /// A drag is one history entry: every step of it merges into the first,
    /// keeping its `before`. Other tools and other cells never merge.
    #[test]
    fn a_drag_coalesces_and_other_edits_do_not() {
        let project = fixtures::project();
        let orc = project.sources[0].id;
        let drag = |before, after| Command::EditCells {
            source: orc,
            action: EditAction::Drag,
            cells: vec![edit(52.0, 6.0, before, after)],
        };
        let mut first = drag(None, Some(6.0));
        assert!(first.merge(&drag(Some(6.0), Some(6.3))));
        assert_eq!(first, drag(None, Some(6.3)));
        let other_cell = Command::EditCells {
            source: orc,
            action: EditAction::Drag,
            cells: vec![edit(90.0, 6.0, None, Some(6.0))],
        };
        assert!(!first.merge(&other_cell));
        let typed = Command::EditCells {
            source: orc,
            action: EditAction::Type,
            cells: vec![edit(52.0, 6.0, Some(6.3), Some(7.0))],
        };
        assert!(!first.merge(&typed));

        let mut history = crate::History::default();
        let mut project = project;
        let original = project.clone();
        history
            .push_coalesced(&mut project, drag(None, Some(6.0)), "drag-1")
            .unwrap();
        history
            .push_coalesced(&mut project, drag(Some(6.0), Some(6.4)), "drag-1")
            .unwrap();
        assert_eq!(history.entries().len(), 1);
        assert_eq!(project.sources[0].overlay.override_at(52.0, 6.0), Some(6.4));
        history.undo(&mut project).unwrap();
        assert_eq!(project, original);
    }

    #[test]
    fn a_polar_source_has_no_segment_statistic() {
        let mut project = fixtures::project();
        let mut command = Command::SetSegmentStatistic {
            source: project.sources[0].id,
            before: SegmentStatistic::P90,
            after: SegmentStatistic::Mean,
        };
        assert!(matches!(
            command.apply(&mut project),
            Err(CoreError::Invalid(_))
        ));
    }
}
