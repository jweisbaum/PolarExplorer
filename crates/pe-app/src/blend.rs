//! The blend, its settings and its export over IPC (spec.md 8, 12, 6).
//!
//! The rule itself is `pe_polar::blend` (one function, spec.md 12.3); this
//! module reads every visible source onto the output grid for it — a polar
//! source resampled with its edits in and every cell read from an excluded
//! node left out, a track through its segment with its per-cell counts —
//! and hands the answer to the views, the source list and export.
//!
//! **Export always recomputes** (invariant 2): [`fresh`] derives every
//! source from scratch, never from the session's cache, and the bytes it
//! writes are the writer's, which depend on nothing but the project
//! (invariant 5). A custom export grid is the blend resampled onto it
//! (bilinear, never extrapolated, spec.md 12.2): tracks are binned on the
//! project's grid, so that is the grid the blend is made on.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use pe_core::command::BLEND_SETTINGS_LABEL;
use pe_core::project::{BlendSettings, MAX_GRID_TWS_KN, OutputGrid, validate_grid_axis};
use pe_core::{Colour, Command, Project};
use pe_polar::blend::on_grid_mode;
use pe_polar::{
    Blend, BlendOptions, BlendSource, CellOrigin, Confidence, ExportProblem, Polar, PolarFileFormat,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::derived::{Derivations, Derived};
use crate::error::{AppError, Context, Result};
use crate::polar_edit::{parse_statistic, statistic_name};
use crate::projects::ProjectSummary;

/// One inclusive wave range in physical units.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "WaveRangeInput.ts")]
pub struct WaveRangeInput {
    /// Lower bound, or none.
    pub min: Option<f64>,
    /// Upper bound, or none.
    pub max: Option<f64>,
}

/// The main wave sliders, independent of the other global filters.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "WaveRangesInput.ts")]
pub struct WaveRangesInput {
    /// Significant wave height, metres.
    pub hs: WaveRangeInput,
    /// Wave angle off the bow, degrees.
    pub wave_angle: WaveRangeInput,
    /// Mean wave period, seconds.
    pub wave_period: WaveRangeInput,
}

impl WaveRangesInput {
    fn of(value: &pe_core::source::WaveRanges) -> Self {
        let range = |r: &pe_core::source::Range| WaveRangeInput {
            min: r.min,
            max: r.max,
        };
        Self {
            hs: range(&value.height_m),
            wave_angle: range(&value.angle_deg),
            wave_period: range(&value.period_s),
        }
    }
}

/// Update all wave sliders as one reversible analysis edit.
#[tauri::command]
pub fn set_wave_ranges(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    ranges: WaveRangesInput,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    wave_ranges_set(&state, ranges)
}

/// The native wave slider edit, shared with tests.
pub fn wave_ranges_set(state: &AppState, ranges: WaveRangesInput) -> Result<ProjectSummary> {
    let range = |r: WaveRangeInput| pe_core::source::Range {
        min: r.min,
        max: r.max,
    };
    let next = pe_core::source::WaveRanges {
        height_m: range(ranges.hs),
        angle_deg: range(ranges.wave_angle),
        period_s: range(ranges.wave_period),
    };
    next.validate()?;
    settings_set(state, |settings| {
        settings.wave_ranges = next;
        Ok(())
    })
}

/// The blend of `project` from each visible source's derived data.
pub fn assemble(project: &Project, derived: &BTreeMap<u64, Arc<Derived>>) -> Blend {
    let mut result = assemble_base(project, derived);
    pe_polar::edit::apply_overrides(&mut result.polar, &project.blend.corrections);
    for (i, angle) in result.polar.twa.iter().enumerate() {
        for j in 0..result.polar.tws.len() {
            if *angle == 0.0 || *angle == 360.0 {
                result.polar.bsp[i][j] = Some(0.0);
            } else if project
                .blend
                .corrections
                .iter()
                .any(|c| c.twa == *angle && c.tws == result.polar.tws[j])
            {
                result.origin[i][j] = CellOrigin::Direct;
            }
        }
    }
    result
}

/// The uncorrrected blend used as the manual editor's immutable baseline.
pub fn assemble_base(project: &Project, derived: &BTreeMap<u64, Arc<Derived>>) -> Blend {
    with_blend_sources(project, derived, |sources, options| {
        pe_polar::blend::blend_mode(
            &project.grid.twa,
            &project.grid.tws,
            sources,
            options,
            project.blend.interpolation,
        )
    })
}

/// Reads every visible source onto the output grid as the rule takes it
/// (spec.md 12.3) and hands the rule's inputs to `then`: the one assembly
/// behind the blend and behind what a cell's tooltip says stood for it, so
/// the two cannot disagree about who took part.
fn with_blend_sources<T>(
    project: &Project,
    derived: &BTreeMap<u64, Arc<Derived>>,
    then: impl FnOnce(&[BlendSource<'_>], &BlendOptions) -> T,
) -> T {
    let grid = &project.grid;
    // A track cell the person overrode counts fully (D23 ruling).
    type Read<'a> = (u64, f64, Polar, Option<(&'a Derived, Vec<Vec<bool>>)>);
    let read: Vec<Read<'_>> = project
        .sources
        .iter()
        .filter(|source| source.visible)
        .filter_map(|source| {
            let data = derived.get(&source.id.raw())?;
            let (polar, track) = if data.track.is_some() {
                // A segment is already on the output grid, its overrides in
                // and its excluded nodes empty.
                let segment = &data.blend;
                let overridden = (0..segment.twa.len())
                    .map(|i| {
                        (0..segment.tws.len())
                            .map(|j| pe_polar::edit::is_overridden(segment, &source.overlay, i, j))
                            .collect()
                    })
                    .collect();
                (data.blend.clone(), Some((&**data, overridden)))
            } else {
                (
                    on_grid_mode(
                        &data.edited,
                        &source.overlay,
                        &grid.twa,
                        &grid.tws,
                        project.blend.interpolation,
                    ),
                    None,
                )
            };
            Some((source.id.raw(), source.weight, polar, track))
        })
        .collect();
    let sources: Vec<BlendSource<'_>> = read
        .iter()
        .map(|(id, weight, polar, track)| BlendSource {
            id: *id,
            grid: polar,
            weight: *weight,
            confidence: match track {
                Some((data, overridden)) => match data.track.as_ref() {
                    Some(placed) => Confidence::Samples {
                        count: &placed.segment.count,
                        overridden,
                    },
                    None => Confidence::Full,
                },
                None => Confidence::Full,
            },
        })
        .collect();
    then(
        &sources,
        &BlendOptions {
            polar_statistic: project.blend.polar_statistic,
            n_full: project.blend.n_full,
            smoothing: project.blend.smoothing,
        },
    )
}

/// Where a blend cell's value came from (spec.md 12.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "BlendCellOrigin.ts")]
pub enum BlendCellOrigin {
    /// At least one source had a value there, or a correction holds it.
    Direct,
    /// Interpolated between other cells, or the 0° row's 0 kn.
    Filled,
    /// Nothing reaches it.
    Empty,
}

/// One source behind a blend cell's value.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "BlendContributor.ts")]
pub struct BlendContributor {
    /// The source.
    pub source_id: u64,
    /// Its boat speed in the cell, knots.
    pub bsp: f64,
    /// Effective weight after polar statistic selection or track confidence.
    pub weight: f64,
    /// Its part of the cell's total weight, 0–1.
    pub share: f64,
}

/// One cell of the blend with what stands behind it: what hovering the
/// blend shows (spec.md 9.2, 10.1). Derived, never stored (invariant 2).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "BlendCell.ts")]
pub struct BlendCell {
    /// The cell's true wind angle, degrees.
    pub twa: f64,
    /// The cell's true wind speed, knots.
    pub tws: f64,
    /// The blend's boat speed there, knots; null for an empty cell.
    pub bsp: Option<f64>,
    /// Where the value came from.
    pub origin: BlendCellOrigin,
    /// A manual correction holds the cell (spec.md 12.3): `bsp` is the
    /// correction, not the contributors' mean.
    pub corrected: bool,
    /// Each source the rule counted in the cell, in id order. Their
    /// weighted mean is the cell's value before smoothing and correction;
    /// empty for a filled or empty cell.
    pub contributors: Vec<BlendContributor>,
}

/// One blend cell and its contributors, by its indices on the output grid.
#[tauri::command]
pub fn blend_cell(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    twa_index: u32,
    tws_index: u32,
) -> Result<BlendCell> {
    let state = state.scoped(boat_context);
    blend_cell_of(&state, twa_index, tws_index)
}

/// [`blend_cell`] for one copy of a split view (spec.md 10.5): the cell of
/// that copy's blend, with the sources behind it there — each track counted
/// with only that direction's samples.
#[tauri::command]
pub fn blend_cell_split(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    twa_index: u32,
    tws_index: u32,
    split: crate::wave_split::WaveSplit,
    cell: u32,
) -> Result<BlendCell> {
    blend_cell_split_of(
        &state.scoped(boat_context),
        twa_index,
        tws_index,
        split,
        cell,
    )
}

/// [`blend_cell_split`] without a Tauri handle.
pub fn blend_cell_split_of(
    state: &AppState,
    twa_index: u32,
    tws_index: u32,
    split: crate::wave_split::WaveSplit,
    cell: u32,
) -> Result<BlendCell> {
    let split = split.checked()?;
    let cell = split.cell(cell)?;
    state.with_session(|session| {
        let open = session.require_open()?;
        let project = &open.project;
        let derived = crate::wave_split::direction_derived(
            project,
            &open.derived.visible(project),
            split,
            cell,
        );
        let blend = match open.derived.split_blends(project, split).get(cell as usize) {
            Some(Some(blend)) => blend.clone(),
            // A copy with nothing to say: every cell is empty.
            _ => assemble(project, &derived),
        };
        cell_of(project, &derived, &blend, twa_index, tws_index)
    })
}

/// [`blend_cell`] without a Tauri handle.
pub fn blend_cell_of(state: &AppState, twa_index: u32, tws_index: u32) -> Result<BlendCell> {
    state.with_session(|session| {
        let open = session.require_open()?;
        let project = &open.project;
        let derived = open.derived.visible(project);
        let blend = open.derived.blend(project);
        cell_of(project, &derived, &blend, twa_index, tws_index)
    })
}

/// One cell of `blend` and what `derived` says stood for it.
fn cell_of(
    project: &Project,
    derived: &BTreeMap<u64, Arc<Derived>>,
    blend: &pe_polar::Blend,
    twa_index: u32,
    tws_index: u32,
) -> Result<BlendCell> {
    {
        let (i, j) = (twa_index as usize, tws_index as usize);
        let (Some(twa), Some(tws)) = (project.grid.twa.get(i), project.grid.tws.get(j)) else {
            return Err(AppError::BadOption {
                field: "Blend cell",
                value: format!("({twa_index}, {tws_index})"),
            });
        };
        let (twa, tws) = (*twa, *tws);
        let terms = with_blend_sources(project, derived, |sources, options| {
            pe_polar::blend::contributions(
                &project.grid.twa,
                &project.grid.tws,
                sources,
                options,
                i,
                j,
            )
        });
        let total: f64 = terms.iter().map(|term| term.weight).sum();
        Ok(BlendCell {
            twa,
            tws,
            bsp: blend.polar.get(i, j),
            origin: match blend.origin.get(i).and_then(|row| row.get(j)) {
                Some(CellOrigin::Direct) => BlendCellOrigin::Direct,
                Some(CellOrigin::Filled) => BlendCellOrigin::Filled,
                Some(CellOrigin::Empty) | None => BlendCellOrigin::Empty,
            },
            corrected: project
                .blend
                .corrections
                .iter()
                .any(|c| c.twa == twa && c.tws == tws),
            contributors: terms
                .into_iter()
                .map(|term| BlendContributor {
                    source_id: term.id,
                    bsp: term.bsp,
                    weight: term.weight,
                    // `total` is positive: every term's weight is.
                    share: term.weight / total,
                })
                .collect(),
        })
    }
}

/// The blend derived from scratch: what export writes (invariant 2).
pub fn fresh(project: &Project) -> Blend {
    let derived = Derivations::default().visible(project);
    assemble(project, &derived)
}

/// The Blend entry and the Blend settings dialog (spec.md 8, 12).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "BlendSummary.ts")]
pub struct BlendSummary {
    /// Its colour in every plot, `#rrggbb`.
    pub colour: String,
    /// Whether it is drawn.
    pub visible: bool,
    /// Cells with direct evidence (spec.md 12.3).
    pub direct: u32,
    /// Cells filled by interpolation, and the 0° row.
    pub filled: u32,
    /// Cells nothing reaches.
    pub empty: u32,
    /// The output grid's TWA axis, degrees (spec.md 12.2).
    pub twa: Vec<f64>,
    /// The output grid's TWS axis, knots.
    pub tws: Vec<f64>,
    /// Samples a track cell needs (spec.md 12.1).
    pub min_samples: u32,
    /// Samples at which a track cell counts fully (spec.md 12.3).
    pub n_full: u32,
    /// Smoothing over the filled grid.
    pub smoothing: bool,
    /// The statistic new tracks start with: `median`, `mean`, `p75`, `p90`.
    pub default_statistic: String,
    /// `min`, `median`, `mean`, `max` or `p90` across polar sources.
    pub polar_statistic: String,
    /// Extra sample filters applied after each track's own filters.
    pub global_filters: Option<crate::tracks::TrackFilters>,
    /// Additional wave constraints from the main view.
    pub wave_ranges: WaveRangesInput,
    /// Independent port/starboard values.
    pub asymmetric: bool,
    /// `linear` or `monotone_spline`.
    pub interpolation: String,
    /// Stored manual blend corrections.
    pub correction_count: u32,
    /// Ordered fallback groups; empty disables prioritization.
    pub priority_groups: Vec<crate::tracks::TrackFilters>,
    /// Minimum pooled observations per cell for a priority group.
    pub priority_min_samples: u32,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl BlendSummary {
    /// The summary of `project` with its blend.
    pub fn of(project: &Project, blend: &Blend) -> Self {
        let coverage = blend.coverage();
        let settings = &project.blend;
        Self {
            colour: settings.colour.to_string(),
            visible: settings.visible,
            direct: count(coverage.direct),
            filled: count(coverage.filled),
            empty: count(coverage.empty),
            twa: project.grid.twa.clone(),
            tws: project.grid.tws.clone(),
            min_samples: settings.min_samples,
            n_full: settings.n_full,
            smoothing: settings.smoothing,
            default_statistic: statistic_name(settings.default_statistic).to_owned(),
            polar_statistic: polar_statistic_name(settings.polar_statistic).to_owned(),
            global_filters: settings
                .global_filters
                .as_ref()
                .map(crate::tracks::TrackFilters::of),
            wave_ranges: WaveRangesInput::of(&settings.wave_ranges),
            asymmetric: settings.asymmetric,
            interpolation: interpolation_name(settings.interpolation).to_owned(),
            correction_count: count(settings.corrections.len()),
            priority_groups: settings
                .priority_groups
                .iter()
                .map(crate::tracks::TrackFilters::of)
                .collect(),
            priority_min_samples: settings.priority_min_samples,
        }
    }
}

/// Stable wire name of the polar blend statistic.
pub fn polar_statistic_name(statistic: pe_core::project::PolarStatistic) -> &'static str {
    use pe_core::project::PolarStatistic::*;
    match statistic {
        Min => "min",
        Median => "median",
        Mean => "mean",
        Max => "max",
        P90 => "p90",
    }
}

/// Stable wire name of the interpolation rule.
pub fn interpolation_name(mode: pe_core::project::Interpolation) -> &'static str {
    match mode {
        pe_core::project::Interpolation::Linear => "linear",
        pe_core::project::Interpolation::MonotoneSpline => "monotone_spline",
    }
}

/// Changes the blend settings with `change`, as one undoable entry.
fn settings_set(
    state: &AppState,
    change: impl FnOnce(&mut BlendSettings) -> Result<()>,
) -> Result<ProjectSummary> {
    crate::edit::apply(state, |project| {
        let before = project.blend.clone();
        let mut after = before.clone();
        change(&mut after)?;
        Ok((after != before).then(|| Command::SetBlendSettings {
            before: Box::new(before),
            after: Box::new(after),
        }))
    })
}

/// Sets the second filter layer; null disables it without touching track filters.
#[tauri::command]
pub fn set_global_filters(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    filters: Option<crate::tracks::TrackFilters>,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    global_filters_set(&state, filters)
}

/// Global filtering without a Tauri handle, for lifecycle tests.
pub fn global_filters_set(
    state: &AppState,
    filters: Option<crate::tracks::TrackFilters>,
) -> Result<ProjectSummary> {
    let filters = filters
        .map(|f| f.applied_to(&pe_core::source::SampleFilters::default()))
        .transpose()?;
    settings_set(state, |settings| {
        settings.global_filters = filters;
        settings.validate()?;
        Ok(())
    })
}

/// Configure ordered per-cell fallback groups as one undoable overlay change.
#[tauri::command]
pub fn set_priority_filters(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    groups: Vec<crate::tracks::TrackFilters>,
    minimum: u32,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    priority_filters_set(&state, groups, minimum)
}

/// The command without a Tauri handle for lifecycle tests.
pub fn priority_filters_set(
    state: &AppState,
    groups: Vec<crate::tracks::TrackFilters>,
    minimum: u32,
) -> Result<ProjectSummary> {
    let groups = groups
        .into_iter()
        .map(|f| f.applied_to(&pe_core::source::SampleFilters::default()))
        .collect::<Result<Vec<_>>>()?;
    settings_set(state, |settings| {
        settings.priority_groups = groups;
        settings.priority_min_samples = minimum;
        settings.validate()?;
        Ok(())
    })
}

/// Shows or hides the blend in every plot (spec.md 8). Export is unchanged.
#[tauri::command]
pub fn set_blend_visible(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    visible: bool,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    blend_visible_set(&state, visible)
}

/// [`set_blend_visible`] without a Tauri handle.
pub fn blend_visible_set(state: &AppState, visible: bool) -> Result<ProjectSummary> {
    settings_set(state, |settings| {
        settings.visible = visible;
        Ok(())
    })
}

/// Changes the blend's colour (spec.md 8). `colour` is `#rrggbb`.
#[tauri::command]
pub fn set_blend_colour(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    colour: String,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    blend_colour_set(&state, &colour)
}

/// [`set_blend_colour`] without a Tauri handle.
pub fn blend_colour_set(state: &AppState, colour: &str) -> Result<ProjectSummary> {
    let colour = Colour::parse(colour)?;
    settings_set(state, |settings| {
        settings.colour = colour;
        Ok(())
    })
}

/// What the Blend settings dialog applies (spec.md 12).
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export_to = "BlendSettingsInput.ts")]
pub struct BlendSettingsInput {
    /// Output grid TWA axis, degrees.
    pub twa: Vec<f64>,
    /// Output grid TWS axis, knots.
    pub tws: Vec<f64>,
    /// Samples a track cell needs.
    pub min_samples: u32,
    /// Samples at which a track cell counts fully.
    pub n_full: u32,
    /// Smooth the filled grid.
    pub smoothing: bool,
    /// `median`, `mean`, `p75` or `p90`.
    pub default_statistic: String,
    /// `min`, `median`, `mean`, `max` or `p90` across polar sources.
    #[serde(default)]
    pub polar_statistic: Option<String>,
    /// Feed the polar from current-corrected values.
    pub use_corrected: bool,
    /// Independent port/starboard values.
    #[serde(default)]
    pub asymmetric: bool,
    /// `linear` or `monotone_spline`; absent in older callers means linear.
    #[serde(default)]
    pub interpolation: String,
}

/// Applies the Blend settings dialog as one undoable entry: the output grid
/// and the settings together, or whichever of them changed.
#[tauri::command]
pub fn set_blend_settings(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    settings: BlendSettingsInput,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    blend_settings_set(&state, settings)
}

/// [`set_blend_settings`] without a Tauri handle.
pub fn blend_settings_set(state: &AppState, input: BlendSettingsInput) -> Result<ProjectSummary> {
    let statistic = parse_statistic(&input.default_statistic).ok_or(AppError::BadOption {
        field: "Default statistic",
        value: input.default_statistic.clone(),
    })?;
    let polar_statistic = input
        .polar_statistic
        .as_deref()
        .map(|value| {
            use pe_core::project::PolarStatistic::*;
            match value {
                "min" => Ok(Min),
                "median" => Ok(Median),
                "mean" => Ok(Mean),
                "max" => Ok(Max),
                "p90" => Ok(P90),
                _ => Err(AppError::BadOption {
                    field: "Polar statistic",
                    value: value.to_owned(),
                }),
            }
        })
        .transpose()?;
    let interpolation = match input.interpolation.as_str() {
        "" | "linear" => pe_core::project::Interpolation::Linear,
        "monotone_spline" => pe_core::project::Interpolation::MonotoneSpline,
        _ => {
            return Err(AppError::BadOption {
                field: "Interpolation",
                value: input.interpolation.clone(),
            });
        }
    };
    validate_grid_axis(
        &input.twa,
        "TWA",
        if input.asymmetric { 360.0 } else { 180.0 },
    )?;
    let grid = OutputGrid {
        twa: input.twa.clone(),
        tws: input.tws.clone(),
    };
    grid.validate()?;
    crate::edit::apply(state, |project| {
        let before = project.blend.clone();
        let after = BlendSettings {
            min_samples: input.min_samples,
            n_full: input.n_full,
            smoothing: input.smoothing,
            use_corrected: input.use_corrected,
            default_statistic: statistic,
            polar_statistic: polar_statistic.unwrap_or(before.polar_statistic),
            asymmetric: input.asymmetric,
            interpolation,
            ..before.clone()
        };
        after.validate()?;
        let mut commands = Vec::new();
        if project.grid != grid {
            commands.push(Command::SetOutputGrid {
                before: project.grid.clone(),
                after: grid,
            });
        }
        if after != before {
            commands.push(Command::SetBlendSettings {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        Ok(match commands.len() {
            0 => None,
            1 => commands.pop(),
            _ => Some(Command::Batch {
                label: BLEND_SETTINGS_LABEL.to_owned(),
                commands,
            }),
        })
    })
}

/// The axes an export is written on, when not the project's output grid.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export_to = "ExportAxes.ts")]
pub struct ExportAxes {
    /// TWA values, degrees.
    pub twa: Vec<f64>,
    /// TWS values, knots.
    pub tws: Vec<f64>,
}

/// Why an export would be refused (see `pe_polar::export`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "ExportProblemView.ts")]
pub struct ExportProblemView {
    /// `empty`, `axis-collision`, `out-of-range` or `too-fast`.
    pub code: String,
    /// `twa` or `tws`, for an axis problem.
    pub axis: Option<String>,
    /// The first value of a collision, or the value out of range.
    pub first: Option<f64>,
    /// The second value of a collision.
    pub second: Option<f64>,
    /// What both values of a collision would be written as.
    pub written: Option<String>,
    /// Where a boat speed is too fast: TWA, TWS and the speed.
    pub cell: Option<(f64, f64, f64)>,
}

impl From<&ExportProblem> for ExportProblemView {
    fn from(problem: &ExportProblem) -> Self {
        let mut view = Self {
            code: problem.code().to_owned(),
            axis: None,
            first: None,
            second: None,
            written: None,
            cell: None,
        };
        match problem {
            ExportProblem::Empty => {}
            ExportProblem::AxisCollision {
                axis,
                first,
                second,
                written,
            } => {
                view.axis = Some(axis.code().to_owned());
                view.first = Some(*first);
                view.second = Some(*second);
                view.written = Some(written.clone());
            }
            ExportProblem::OutOfRange { axis, value } => {
                view.axis = Some(axis.code().to_owned());
                view.first = Some(*value);
            }
            ExportProblem::TooFast { twa, tws, bsp } => view.cell = Some((*twa, *tws, *bsp)),
        }
        view
    }
}

/// What the export dialog shows before saving (spec.md 12): the grid that
/// would be written and the file's text, or why it cannot be.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "ExportPreview.ts")]
pub struct ExportPreview {
    /// `expedition`, `adrena` or `csv`.
    pub format: String,
    /// TWA axis, degrees.
    pub twa: Vec<f64>,
    /// TWS axis, knots.
    pub tws: Vec<f64>,
    /// Boat speed per cell, `[twa][tws]`.
    pub bsp: Vec<Vec<Option<f64>>>,
    /// Per cell on the project's grid: `direct`, `filled` or `empty`; null
    /// on a custom grid, which is the blend resampled.
    pub origin: Option<Vec<Vec<String>>>,
    /// The file as it would be written; empty when refused.
    pub text: String,
    /// Why it would be refused.
    pub problem: Option<ExportProblemView>,
}

fn parse_format(format: &str) -> Result<PolarFileFormat> {
    match format {
        "expedition" => Ok(PolarFileFormat::Expedition),
        "adrena" => Ok(PolarFileFormat::Adrena),
        "csv" => Ok(PolarFileFormat::Csv),
        other => Err(AppError::BadOption {
            field: "Export format",
            value: other.to_owned(),
        }),
    }
}

fn format_name(format: PolarFileFormat) -> &'static str {
    match format {
        PolarFileFormat::Expedition => "expedition",
        PolarFileFormat::Adrena => "adrena",
        PolarFileFormat::Csv => "csv",
    }
}

fn origin_name(origin: CellOrigin) -> &'static str {
    match origin {
        CellOrigin::Direct => "direct",
        CellOrigin::Filled => "filled",
        CellOrigin::Empty => "empty",
    }
}

/// The grid an export writes: the blend on the project grid, or resampled
/// onto custom axes (which must pass the output grid's own rules).
pub fn export_grid(project: &Project, axes: Option<&ExportAxes>) -> Result<(Blend, Option<Polar>)> {
    let blend = fresh(project);
    let Some(axes) = axes else {
        return Ok((blend, None));
    };
    validate_grid_axis(&axes.twa, "TWA", 360.0)?;
    validate_grid_axis(&axes.tws, "TWS", MAX_GRID_TWS_KN)?;
    let polar = pe_polar::blend::resample_blend_mode(
        &blend.polar,
        &axes.twa,
        &axes.tws,
        project.blend.interpolation,
    );
    Ok((blend, Some(polar)))
}

/// The preview of an export of `project` (pure; no session).
pub fn preview_of(
    project: &Project,
    format: &str,
    axes: Option<&ExportAxes>,
) -> Result<ExportPreview> {
    let format = parse_format(format)?;
    let (blend, custom) = export_grid(project, axes)?;
    let origin = custom.is_none().then(|| {
        blend
            .origin
            .iter()
            .map(|row| row.iter().map(|o| origin_name(*o).to_owned()).collect())
            .collect()
    });
    let polar = custom.unwrap_or(blend.polar);
    let (text, problem) = match pe_polar::export(format, &polar) {
        Ok(text) => (text, None),
        Err(problem) => (String::new(), Some(ExportProblemView::from(&problem))),
    };
    Ok(ExportPreview {
        format: format_name(format).to_owned(),
        twa: polar.twa,
        tws: polar.tws,
        bsp: polar.bsp,
        origin,
        text,
        problem,
    })
}

/// The bytes an export of `project` writes (pure; no session): recomputed
/// from the sources, refused with the problem named.
pub fn export_bytes(project: &Project, format: &str, axes: Option<&ExportAxes>) -> Result<Vec<u8>> {
    let format = parse_format(format)?;
    let (blend, custom) = export_grid(project, axes)?;
    let polar = custom.unwrap_or(blend.polar);
    pe_polar::export(format, &polar)
        .map(String::into_bytes)
        .map_err(|problem| AppError::Export {
            code: problem.code(),
            message: problem.to_string(),
        })
}

/// The export dialog's preview (spec.md 12).
#[tauri::command]
pub fn export_preview(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    format: String,
    axes: Option<ExportAxes>,
) -> Result<ExportPreview> {
    let state = state.scoped(boat_context);
    preview(&state, &format, axes.as_ref())
}

/// [`export_preview`] without a Tauri handle.
pub fn preview(state: &AppState, format: &str, axes: Option<&ExportAxes>) -> Result<ExportPreview> {
    state.with_session(|session| preview_of(&session.require_open()?.project, format, axes))
}

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "ExportResult.ts")]
pub struct ExportResult {
    /// The file written.
    pub path: String,
    /// Its size.
    pub bytes: u32,
}

/// Writes the blend to `path` in `format` (spec.md 6, 12), recomputed from
/// the sources (invariant 2), on the project's grid or `axes`.
#[tauri::command]
pub fn export_polar(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    path: String,
    format: String,
    axes: Option<ExportAxes>,
) -> Result<ExportResult> {
    let state = state.scoped(boat_context);
    export_to(&state, &path, &format, axes.as_ref())
}

/// [`export_polar`] without a Tauri handle.
pub fn export_to(
    state: &AppState,
    path: &str,
    format: &str,
    axes: Option<&ExportAxes>,
) -> Result<ExportResult> {
    let bytes = state
        .with_session(|session| export_bytes(&session.require_open()?.project, format, axes))?;
    let target = PathBuf::from(path);
    pe_core::io::write_atomic(&target, &bytes).doing("write the polar to", path)?;
    Ok(ExportResult {
        path: path.to_owned(),
        bytes: count(bytes.len()),
    })
}
