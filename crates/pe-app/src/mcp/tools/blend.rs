//! The blend group (spec.md 3.7): the blend's settings and filters, reading
//! any polar as a table, what stands behind a cell, editing cells, and
//! comparing two polars.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use tauri::Manager;

use super::tracks::{no_filters, patched};
use super::{BoatParams, PolarExplorer, ToolError, ToolResult, json, on_axis, typed};
use crate::blend::{BlendSettingsInput, WaveRangesInput};
use crate::commands::AppState;
use crate::compare::CompareOperand;
use crate::error::AppError;
use crate::polar_edit::{EditOp, PolarCell};
use crate::projects::ProjectSummary;

/// The difference below which two polars are taken to agree, knots, unless
/// a comparison names another: the Compare stage's own default.
const DEFAULT_THRESHOLD_KN: f64 = 0.05;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BlendSetParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The settings to change, an object holding only those: twa and tws
    /// (the output grid's axes, lists of increasing numbers: degrees 0–180,
    /// or 0–360 when asymmetric; knots 0–70), min_samples (the samples a
    /// track cell needs), n_full (the samples for full confidence),
    /// polar_statistic ("min", "median", "mean", "max", "p90"),
    /// smoothing (true/false), default_statistic ("median", "mean", "p75",
    /// "p90", "p95", "max": what a newly imported track starts with),
    /// use_corrected (correct for current), asymmetric
    /// (independent port and starboard), interpolation ("linear" or
    /// "monotone_spline").
    pub settings: Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BlendFiltersParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// Filters applied to every track after its own, an object holding
    /// only the limits to set, with the fields of track_set's `filters`.
    #[serde(default)]
    pub global: Option<Value>,
    /// Remove the global filters altogether.
    #[serde(default)]
    pub clear_global: bool,
    /// The wave ranges of the 3D view's sliders, an object holding only
    /// the ranges, and of each only the ends, to set: {"hs": {"min": 0.5,
    /// "max": 3}, "waveAngle": {"max": 90}, "wavePeriod": {"min": 4,
    /// "max": null}} (metres, degrees off the bow, seconds; null opens an
    /// end, an end left out stays as it is).
    #[serde(default)]
    pub wave_ranges: Option<Value>,
    /// Priority groups, tried in order for each cell across the tracks: a
    /// list of filter objects (the fields of track_set's `filters`, each
    /// holding only its limits). The first group with enough samples
    /// supplies the cell. An empty list turns priorities off.
    #[serde(default)]
    pub priority_groups: Option<Value>,
    /// The samples a group needs to supply a cell.
    #[serde(default)]
    pub priority_minimum: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PolarReadParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// A source's id for its polar (a track's is its polar segment);
    /// without it, the blend.
    #[serde(default)]
    pub source: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BlendCellParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The cell's true wind angle, degrees: a value of the output grid.
    pub twa: f64,
    /// The cell's true wind speed, knots: a value of the output grid.
    pub tws: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PolarEditParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The source whose overlay is edited; without it, the blend itself
    /// (a correction applied after blending).
    #[serde(default)]
    pub source: Option<u64>,
    /// What to do to the cells, an object: {"type": "type", "bsp": 7.5}
    /// sets the value (bsp null resets the cell); {"type": "scale",
    /// "percent": 3} makes the cells 3 % faster; {"type": "smooth"};
    /// {"type": "reset"} resets the cells; {"type": "reset_all"} clears
    /// every edit of the source.
    pub op: Value,
    /// The cells, a list of {"twa": 90, "tws": 12}: values on the polar's
    /// own axes, as polar_read gives them. Ignored by reset_all.
    #[serde(default)]
    pub cells: Vec<CellAt>,
    /// Calls that share a gesture name, one after another, are one undo
    /// step. Usually omitted: one call, one step.
    #[serde(default)]
    pub gesture: Option<String>,
}

/// A cell named by its values.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
pub struct CellAt {
    /// True wind angle, degrees.
    pub twa: f64,
    /// True wind speed, knots.
    pub tws: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompareParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// Operand A, an object: {"kind": "blend"}, {"kind": "polar",
    /// "source_id": 1} (an ORC, ORR or file source's polar) or {"kind":
    /// "segment", "source_id": 2} (a track's polar segment).
    pub a: Value,
    /// Operand B, as A.
    pub b: Value,
    /// A difference under this, knots, counts as agreement (0.05 when
    /// omitted).
    #[serde(default)]
    pub threshold_kn: Option<f64>,
}

/// The open boat's summary, or the refusal for none.
fn summary(state: &AppState) -> crate::error::Result<ProjectSummary> {
    crate::projects::summary(state)?.ok_or(AppError::NoProjectOpen)
}

/// The Blend settings as the dialog would send them unchanged, as JSON, for
/// a patch to be written over.
fn current_settings(summary: &ProjectSummary) -> Value {
    let blend = &summary.blend;
    serde_json::json!({
        "twa": blend.twa,
        "tws": blend.tws,
        "min_samples": blend.min_samples,
        "n_full": blend.n_full,
        "smoothing": blend.smoothing,
        "default_statistic": blend.default_statistic,
        "polar_statistic": blend.polar_statistic,
        "use_corrected": summary.use_corrected,
        "stokes_drift": summary.stokes_drift,
        "asymmetric": blend.asymmetric,
        "interpolation": blend.interpolation,
    })
}

/// `patch` written over `base`, both objects.
fn merged(field: &str, mut base: Value, patch: Value) -> std::result::Result<Value, ToolError> {
    let patch: serde_json::Map<String, Value> = typed(field, patch)?;
    super::patch_over(field, &mut base, patch)?;
    Ok(base)
}

#[tool_router(router = tool_router_blend, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "The blend's settings and coverage: its output grid (twa, tws), interpolation, smoothing, the samples a track cell needs and for full confidence, the global filters, wave ranges and priority groups, how many cells have direct evidence, were filled or are empty, and how many manual corrections it holds."
    )]
    async fn blend_status(&self, Parameters(p): Parameters<BoatParams>) -> ToolResult {
        let summary = self
            .run("blend_status", move |app| {
                summary(&app.state::<AppState>().scoped(p.boat))
            })
            .await?;
        json(&summary.blend)
    }

    #[tool(
        description = "Changes the blend's settings: give only the ones to change. One undo step. A new grid re-bins every track; interpolation applies to how sources are read, how gaps are filled, the plots and the export."
    )]
    async fn blend_set(&self, Parameters(p): Parameters<BlendSetParams>) -> ToolResult {
        let boat = p.boat;
        let current = self
            .run("blend_set", move |app| {
                summary(&app.state::<AppState>().scoped(boat))
            })
            .await?;
        let settings: BlendSettingsInput = typed(
            "settings",
            merged("settings", current_settings(&current), p.settings)?,
        )?;
        let summary = self
            .write("blend_set", false, move |app| {
                crate::blend::set_blend_settings(app.state(), boat, settings)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Sets the filters over every track of the boat: global filters (applied after each track's own), the wave ranges, and priority groups; give the ones to change. Each is one undo step."
    )]
    async fn blend_filters_set(&self, Parameters(p): Parameters<BlendFiltersParams>) -> ToolResult {
        let priorities = p.priority_groups.is_some() || p.priority_minimum.is_some();
        if p.global.is_none() && !p.clear_global && p.wave_ranges.is_none() && !priorities {
            return Err(ToolError::Refused(
                "there is nothing to change: give global, clear_global, wave_ranges, priority_groups or priority_minimum"
                    .to_owned(),
            ));
        }
        let boat = p.boat;
        let current = self
            .run("blend_filters_set", move |app| {
                summary(&app.state::<AppState>().scoped(boat))
            })
            .await?
            .blend;
        // Each part is a patch over what the boat has, or over "nothing
        // filtered" where it has none.
        let global = p
            .global
            .map(|patch| {
                let base = match &current.global_filters {
                    Some(filters) => filters.clone(),
                    None => no_filters()?,
                };
                patched("global", &base, patch)
            })
            .transpose()?;
        let wave_ranges: Option<WaveRangesInput> = p
            .wave_ranges
            .map(|patch| {
                let base = serde_json::to_value(&current.wave_ranges)
                    .map_err(|e| ToolError::Refused(format!("wave_ranges: {e}")))?;
                typed("wave_ranges", merged("wave_ranges", base, patch)?)
            })
            .transpose()?;
        let groups = p
            .priority_groups
            .map(|raw| {
                let patches: Vec<Value> = typed("priority_groups", raw)?;
                let blank = no_filters()?;
                patches
                    .into_iter()
                    .map(|patch| patched("priority_groups", &blank, patch))
                    .collect::<std::result::Result<Vec<_>, _>>()
            })
            .transpose()?;
        let clear_global = p.clear_global;
        let minimum = p.priority_minimum;
        let summary = self
            .write("blend_filters_set", false, move |app| {
                let state = || app.state::<AppState>();
                let mut last = None;
                if clear_global {
                    last = Some(crate::blend::set_global_filters(state(), boat, None)?);
                }
                if let Some(filters) = global {
                    last = Some(crate::blend::set_global_filters(
                        state(),
                        boat,
                        Some(filters),
                    )?);
                }
                if let Some(ranges) = wave_ranges {
                    last = Some(crate::blend::set_wave_ranges(state(), boat, ranges)?);
                }
                if priorities {
                    last = Some(crate::blend::set_priority_filters(
                        state(),
                        boat,
                        groups.unwrap_or(current.priority_groups),
                        minimum.unwrap_or(current.priority_min_samples),
                    )?);
                }
                last.ok_or(AppError::Internal(
                    "blend_filters_set had nothing to apply".to_owned(),
                ))
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "A polar as a table: `twa` and `tws` axes and `bsp[twa][tws]` in knots (null for an empty cell), as edited. For a source: `source` holds the imported values, `edited` and `excluded` flag cells, and a track's segment adds each cell's sample `count`, `spread` and its `statistic`. Without `source`: the blend on the output grid."
    )]
    async fn polar_read(&self, Parameters(p): Parameters<PolarReadParams>) -> ToolResult {
        let of_blend = p.source.is_none();
        let surface = self
            .run("polar_read", move |app| {
                // Source 0 is the blend, as the Correct blend editor reads it.
                crate::polar_edit::polar_edit_surface(app.state(), p.boat, p.source.unwrap_or(0))
            })
            .await?;
        let mut table = serde_json::to_value(&surface).map_err(|e| {
            ToolError::Refused(format!("the polar could not be written as JSON: {e}"))
        })?;
        // The editor reads the blend as a source of its own, under a kind
        // that is the editor's business; a client asked for the blend.
        if of_blend {
            table["kind"] = Value::from("blend");
        }
        json(&table)
    }

    #[tool(
        description = "One cell of the blend and what stands behind it: its `bsp` (knots; null when empty), `origin` (direct: sources had values there; filled: interpolated between neighbouring cells, or the 0° row; empty), whether a manual correction holds it (`corrected`), and `contributors`: each source the blending rule counted in the cell with its own bsp, the weight it had there (its weight times its confidence) and its `share` of the total. Read-only."
    )]
    async fn blend_cell(&self, Parameters(p): Parameters<BlendCellParams>) -> ToolResult {
        let boat = p.boat;
        let grid = self
            .run("blend_cell", move |app| {
                summary(&app.state::<AppState>().scoped(boat))
            })
            .await?
            .blend;
        let twa = on_axis("TWA", &grid.twa, p.twa)?;
        let tws = on_axis("TWS", &grid.tws, p.tws)?;
        let cell = self
            .run("blend_cell", move |app| {
                crate::blend::blend_cell(app.state(), boat, twa, tws)
            })
            .await?;
        json(&cell)
    }

    #[tool(
        description = "Edits cells of a polar as an overlay: on a source (its imported values are kept, and reset brings them back), or with no `source` on the blend itself as a correction applied after blending. One undo step. The 0° row stays 0."
    )]
    async fn polar_edit(&self, Parameters(p): Parameters<PolarEditParams>) -> ToolResult {
        let op: EditOp = typed("op", p.op)?;
        let boat = p.boat;
        let source = p.source.unwrap_or(0);
        // Cells are named by their values; the command takes indices on the
        // surface's own axes.
        let cells = if p.cells.is_empty() {
            Vec::new()
        } else {
            let surface = self
                .run("polar_edit", move |app| {
                    crate::polar_edit::polar_edit_surface(app.state(), boat, source)
                })
                .await?;
            p.cells
                .iter()
                .map(|cell| {
                    Ok(PolarCell {
                        twa_index: on_axis("TWA", &surface.twa, cell.twa)?,
                        tws_index: on_axis("TWS", &surface.tws, cell.tws)?,
                    })
                })
                .collect::<std::result::Result<Vec<_>, ToolError>>()?
        };
        let gesture = p.gesture;
        let summary = self
            .write("polar_edit", false, move |app| {
                crate::polar_edit::edit_polar(app.state(), boat, source, op, cells, gesture)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Compares two polars on the output grid, A against B: the axes, `a`, `b`, `delta_kn` (A − B) and `delta_pct` (of B) as [twa][tws] tables (null where not both have a value), how many cells `overlap` or only one covers, the mean and `largest` difference, and `regions`: for each wind speed the runs of angles where one is faster by more than the threshold. Read-only."
    )]
    async fn compare(&self, Parameters(p): Parameters<CompareParams>) -> ToolResult {
        let a: CompareOperand = typed("a", p.a)?;
        let b: CompareOperand = typed("b", p.b)?;
        let threshold = p.threshold_kn.unwrap_or(DEFAULT_THRESHOLD_KN);
        let boat = p.boat;
        let compared = self
            .run("compare", move |app| {
                app.state::<AppState>().scoped(boat).with_session(|session| {
                    let open = session.require_open()?;
                    // The Compare stage's own computation, unpacked.
                    let c = crate::compare::compare_in(
                        &open.project,
                        &mut open.derived,
                        a,
                        b,
                        threshold,
                    )?;
                    let at = |i: usize, j: usize| (c.twa.get(i).copied(), c.tws.get(j).copied());
                    let largest = c.kn.max_abs.map(|(_, i, j)| {
                        let (twa, tws) = at(i, j);
                        serde_json::json!({
                            "twa": twa,
                            "tws": tws,
                            "delta_kn": c.delta_kn.get(i).and_then(|row| row.get(j)).copied().flatten(),
                        })
                    });
                    let regions: Vec<Value> = c
                        .regions
                        .iter()
                        .map(|region| {
                            serde_json::json!({
                                "tws": c.tws.get(region.tws_index),
                                "twa_from": c.twa.get(region.first_twa_index),
                                "twa_to": c.twa.get(region.last_twa_index),
                                "faster": match region.faster {
                                    pe_polar::Faster::A => "a",
                                    pe_polar::Faster::B => "b",
                                },
                            })
                        })
                        .collect();
                    Ok(serde_json::json!({
                        "twa": c.twa,
                        "tws": c.tws,
                        "a": c.a,
                        "b": c.b,
                        "delta_kn": c.delta_kn,
                        "delta_pct": c.delta_pct,
                        "overlap": c.overlap,
                        "a_only": c.a_only,
                        "b_only": c.b_only,
                        "threshold_kn": c.threshold_kn,
                        "mean_abs_kn": c.kn.mean_abs,
                        "mean_abs_pct": c.pct.mean_abs,
                        "range_kn": c.kn.range.map(|(low, high)| [low, high]),
                        "largest": largest,
                        "regions": regions,
                    }))
                })
            })
            .await?;
        json(&compared)
    }
}
