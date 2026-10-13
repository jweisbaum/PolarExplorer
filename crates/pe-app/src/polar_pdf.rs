//! Shared, derived Gaussian PDFs for both polar views. A source contributes
//! exactly the weight/statistic it contributes to the blend. Track variability
//! is retained, so a long track does not acquire extra weight just by having
//! more observations after reaching the blend's confidence cap.

use std::collections::BTreeMap;
use std::sync::Arc;

use pe_core::Project;
use pe_polar::{BlendOptions, BlendSource, Confidence, Polar};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::derived::{Derived, TrackDerived};
use crate::error::{AppError, Result};
use crate::wave_split::{WaveSplit, sample_bearing};

/// View-only restrictions; the project and its source overlays stay intact.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export_to = "PolarPdfOptions.ts")]
pub struct PolarPdfOptions {
    pub samples: bool,
    pub nodes: bool,
    pub filtered: bool,
    pub excluded: bool,
    pub source: Option<u64>,
    pub split: Option<WaveSplit>,
}

/// One Gaussian on the output wind grid. Percentiles describe this same
/// weighted Gaussian, truncated to the valid boat-speed range.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "PolarPdfCell.ts")]
pub struct PolarPdfCell {
    pub wind_cell: usize,
    pub copy: u32,
    pub twa: f64,
    pub tws: f64,
    pub count: u32,
    pub mean: f64,
    pub stddev: f64,
    pub mass: f64,
    pub percentiles: [f64; 5],
}

/// Axes travel with their cells so an old reply can never use a newer grid.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "PolarPdfResult.ts")]
pub struct PolarPdfResult {
    pub twa: Vec<f64>,
    pub tws: Vec<f64>,
    pub cells: Vec<PolarPdfCell>,
}

/// A compact result, independent of the (potentially very large) dot packet.
#[tauri::command]
pub async fn polar_pdf(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    options: PolarPdfOptions,
) -> Result<PolarPdfResult> {
    let state = state.scoped(boat_context);
    if let Some(split) = options.split {
        split.checked()?;
    }
    tauri::async_runtime::spawn_blocking(move || {
        state.with_session(|session| {
            let open = session.require_open()?;
            let derived = open.derived.visible(&open.project);
            Ok(distributions(&open.project, &derived, &options))
        })
    })
    .await
    .map_err(|error| AppError::Internal(format!("Computing polar PDFs: {error}")))?
}

fn distributions(
    project: &Project,
    derived: &BTreeMap<u64, Arc<Derived>>,
    options: &PolarPdfOptions,
) -> PolarPdfResult {
    let mut cells = Vec::new();
    for copy in 0..options.split.map_or(1, |split| split.count) {
        let shown = view_sources(project, derived, options, copy);
        cells.extend(fit(project, &shown, options.excluded, copy));
    }
    PolarPdfResult {
        twa: project.grid.twa.clone(),
        tws: project.grid.tws.clone(),
        cells,
    }
}

fn view_sources(
    project: &Project,
    derived: &BTreeMap<u64, Arc<Derived>>,
    options: &PolarPdfOptions,
    copy: u32,
) -> BTreeMap<u64, Arc<Derived>> {
    project
        .sources
        .iter()
        .filter_map(|source| {
            if !source.visible
                || source.weight <= 0.0
                || options.source.is_some_and(|id| id != source.id.raw())
            {
                return None;
            }
            let data = derived.get(&source.id.raw())?;
            let Some(track) = source.track() else {
                return options.nodes.then(|| (source.id.raw(), Arc::clone(data)));
            };
            if !options.samples {
                return None;
            }
            if options.split.is_none() && !options.filtered && !options.excluded {
                return Some((source.id.raw(), Arc::clone(data)));
            }
            let placed = data.track.as_ref()?;
            let points = track
                .samples
                .iter()
                .zip(&placed.points)
                .zip(&placed.filtered)
                .filter(|((sample, _), filtered)| {
                    (options.filtered || !**filtered)
                        && (options.excluded
                            || source
                                .overlay
                                .excluded_samples
                                .binary_search(&sample.id)
                                .is_err())
                        && !pe_tracks::filter::wave_ranges_exclude(
                            sample,
                            &project.blend.wave_ranges,
                            project.blend.use_corrected,
                        )
                        && options.split.is_none_or(|split| {
                            split.bucket(sample_bearing(project, sample)) == Some(copy)
                        })
                })
                .filter_map(|((_, point), _)| *point);
            let segment = pe_polar::bin(
                points,
                &project.grid.twa,
                &project.grid.tws,
                track.statistic,
                if project.blend.priority_groups.is_empty() {
                    project.blend.min_samples
                } else {
                    1
                },
            );
            let base = segment.polar.clone();
            let mut overlay = source.overlay.clone();
            if options.excluded {
                overlay.excluded_cells.clear();
            }
            let edited = pe_polar::with_overlay(base.clone(), &overlay, false);
            let blend = pe_polar::with_overlay(edited.clone(), &overlay, true);
            Some((
                source.id.raw(),
                Arc::new(Derived {
                    base,
                    edited,
                    blend,
                    track: Some(Arc::new(TrackDerived {
                        points: Vec::new(),
                        filtered: Vec::new(),
                        segment,
                    })),
                }),
            ))
        })
        .collect()
}

pub(crate) fn fit(
    project: &Project,
    derived: &BTreeMap<u64, Arc<Derived>>,
    include_excluded: bool,
    copy: u32,
) -> Vec<PolarPdfCell> {
    crate::blend::with_blend_sources_view(project, derived, include_excluded, |sources, options| {
        let (twa, tws) = (&project.grid.twa, &project.grid.tws);
        let base =
            pe_polar::blend::blend_mode(twa, tws, sources, options, project.blend.interpolation);
        let mut corrected = base.polar.clone();
        pe_polar::edit::apply_overrides(&mut corrected, &project.blend.corrections);
        let mut second = Polar::empty(twa.clone(), tws.clone());
        let mut counts = vec![vec![0u32; tws.len()]; twa.len()];
        for (i, row_counts) in counts.iter_mut().enumerate() {
            for (j, count) in row_counts.iter_mut().enumerate() {
                let terms = pe_polar::blend::contributions(twa, tws, sources, options, i, j);
                let mut weight = 0.0;
                let mut moment = 0.0;
                for term in terms {
                    let (n, variance) = derived
                        .get(&term.id)
                        .and_then(|data| data.track.as_ref())
                        .map_or((1, 0.0), |track| {
                            let n = track.segment.count[i][j];
                            let sample_variance = track.segment.spread[i][j].unwrap_or(0.0).powi(2);
                            (
                                n,
                                sample_variance * f64::from(n.saturating_sub(1))
                                    / f64::from(n.max(1)),
                            )
                        });
                    *count = count.saturating_add(n);
                    weight += term.weight;
                    moment += term.weight * (variance + term.bsp * term.bsp);
                }
                if weight > 0.0 {
                    second.bsp[i][j] = Some(moment / weight);
                }
            }
        }
        // Carry E[BSP²] through exactly the blend's interpolation and smoothing
        // passes. Var = E[BSP²] - E[BSP]² preserves both within-track variability
        // and disagreement between sources/neighbours. Manual corrections then
        // translate the distribution without spuriously increasing its variance.
        let moments = pe_polar::blend::blend_mode(
            twa,
            tws,
            &[BlendSource {
                id: 0,
                grid: &second,
                weight: 1.0,
                confidence: Confidence::Full,
            }],
            &BlendOptions {
                polar_statistic: pe_core::project::PolarStatistic::Mean,
                ..*options
            },
            project.blend.interpolation,
        );
        let mut cells = Vec::new();
        for (i, angle) in twa.iter().enumerate() {
            if *angle == 0.0 || *angle == 360.0 {
                continue;
            }
            for (j, wind) in tws.iter().enumerate() {
                let (Some(location), Some(moment)) =
                    (base.polar.get(i, j), moments.polar.get(i, j))
                else {
                    continue;
                };
                let mean = corrected
                    .get(i, j)
                    .unwrap_or(location)
                    .clamp(0.0, pe_polar::MAX_SPEED_KN);
                let stddev = (moment - location * location).max(0.25).sqrt();
                let lower = normal_cdf(-mean / stddev);
                let mass = normal_cdf((pe_polar::MAX_SPEED_KN - mean) / stddev) - lower;
                let percentiles = [0.1, 0.25, 0.5, 0.75, 0.9].map(|p| {
                    let (mut lo, mut hi) = (0.0, pe_polar::MAX_SPEED_KN);
                    for _ in 0..40 {
                        let mid = (lo + hi) / 2.0;
                        if normal_cdf((mid - mean) / stddev) < lower + p * mass {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    (lo + hi) / 2.0
                });
                cells.push(PolarPdfCell {
                    wind_cell: i * tws.len() + j,
                    copy,
                    twa: *angle,
                    tws: *wind,
                    count: counts[i][j],
                    mean,
                    stddev,
                    mass,
                    percentiles,
                });
            }
        }
        cells
    })
}

fn normal_cdf(value: f64) -> f64 {
    let x = value.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let erf = 1.0
        - (((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t)
            * (-x * x).exp();
    0.5 * (1.0 + value.signum() * erf)
}
