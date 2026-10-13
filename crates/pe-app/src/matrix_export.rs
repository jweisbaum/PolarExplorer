//! Wave-conditioned exports, derived from the same admitted evidence and blend
//! rules as the views. No raw sources or project settings are changed.

use std::{collections::BTreeMap, sync::Arc};

use pe_core::{Project, source::SourceKind, track::Sample};
use pe_polar::{GaussianPdf, MatrixPolarExt, Polar, WhirlwindMatrixPolar};
use serde::Deserialize;
use ts_rs::TS;

use crate::{
    blend::ExportAxes,
    derived::Derived,
    error::{AppError, Result},
};

/// Nonnegative bins separated by cutoffs. The final bin is open-ended.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export_to = "MatrixAxisBins.ts")]
pub struct MatrixAxisBins {
    pub cutoffs: Vec<f64>,
}

impl MatrixAxisBins {
    fn count(&self) -> u32 {
        self.cutoffs.len() as u32 + 1
    }

    fn validate(&self, maximum: f64) -> Result<()> {
        if self.cutoffs.len() > 63
            || self
                .cutoffs
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.0 || *v >= maximum)
            || self.cutoffs.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(invalid(
                "Use at most 63 strictly increasing positive wave cutoffs below the axis limit",
            ));
        }
        distinct_centers(&self.centers(maximum))
    }

    /// Finite bins use midpoints. The open tail extends half the preceding
    /// bin width beyond its cutoff (bounded by the file axis limit).
    fn centers(&self, maximum: f64) -> Vec<f32> {
        let mut lower = 0.0;
        let mut width = 0.0;
        let mut centers = Vec::with_capacity(self.cutoffs.len() + 1);
        for upper in &self.cutoffs {
            centers.push(((lower + upper) / 2.0) as f32);
            width = upper - lower;
            lower = *upper;
        }
        centers.push((lower + width / 2.0).min(maximum) as f32);
        centers
    }

    fn bucket(&self, value: f64, upper_inclusive: bool) -> Option<u32> {
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        Some(self.cutoffs.partition_point(|cut| {
            if upper_inclusive {
                *cut < value
            } else {
                *cut <= value
            }
        }) as u32)
    }
}

fn distinct_centers(centers: &[f32]) -> Result<()> {
    if centers.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid(
            "Wave bin coordinates must remain distinct at float32 precision",
        ));
    }
    Ok(())
}

/// Consecutive, unwrapped direction edges spanning one full turn.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export_to = "MatrixDirectionBins.ts")]
pub struct MatrixDirectionBins {
    pub edges: Vec<f64>,
}

impl MatrixDirectionBins {
    fn count(&self) -> u32 {
        self.edges.len().saturating_sub(1) as u32
    }

    fn validate(&self) -> Result<()> {
        if !(2..=65).contains(&self.edges.len())
            || self.edges.iter().any(|v| !v.is_finite())
            || !(-360.0..360.0).contains(&self.edges[0])
            || (self.edges[self.edges.len() - 1] - self.edges[0] - 360.0).abs() > 1e-9
            || self.edges.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(invalid(
                "Use 2–65 increasing wave direction boundaries spanning exactly 360 degrees, starting between -360 and 360",
            ));
        }
        distinct_centers(&self.centers())
    }

    fn center(pair: &[f64]) -> f32 {
        // Normalize after narrowing too, since float32 can round 359.99999 to 360.
        (((pair[0] + pair[1]) / 2.0).rem_euclid(360.0) as f32).rem_euclid(360.0)
    }

    fn centers(&self) -> Vec<f32> {
        let mut centers: Vec<_> = self.edges.windows(2).map(Self::center).collect();
        centers.sort_by(f32::total_cmp);
        centers
    }

    fn bucket(&self, value: f64) -> Option<u32> {
        if !value.is_finite() {
            return None;
        }
        let unwrapped = self.edges[0] + (value - self.edges[0]).rem_euclid(360.0);
        let pair = self
            .edges
            .windows(2)
            .find(|p| unwrapped >= p[0] && unwrapped < p[1])?;
        let center = Self::center(pair);
        // Payload order follows the sorted [0,360) axis, even when its first
        // sector wraps through zero or custom boundaries start behind the bow.
        Some(
            self.edges
                .windows(2)
                .filter(|p| Self::center(p) < center)
                .count() as u32,
        )
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(default)]
#[ts(export_to = "MatrixWaveBins.ts")]
pub struct MatrixWaveBins {
    /// Significant wave height in metres; lower-inclusive bins.
    pub height: MatrixAxisBins,
    /// Clockwise incoming wave direction relative to the selected bow, 0–360°.
    pub angle: MatrixDirectionBins,
    /// Wave period in seconds; upper-inclusive bins.
    pub period: MatrixAxisBins,
}

impl Default for MatrixWaveBins {
    fn default() -> Self {
        Self {
            height: MatrixAxisBins {
                cutoffs: vec![1.0, 3.0, 5.0],
            },
            angle: MatrixDirectionBins {
                edges: vec![-45.0, 45.0, 135.0, 225.0, 315.0],
            },
            period: MatrixAxisBins { cutoffs: vec![5.0] },
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(default)]
#[ts(export_to = "MatrixExportOptions.ts")]
pub struct MatrixExportOptions {
    pub sog: bool,
    pub sampled_pdf: bool,
    pub gaussian_pdf: bool,
    /// None exports a wave-independent matrix. Otherwise all three axes exist.
    pub waves: Option<MatrixWaveBins>,
    /// Equal-width histogram bins over 0–60 knots, stored at their centers.
    pub sog_bins: u32,
}

impl Default for MatrixExportOptions {
    fn default() -> Self {
        Self {
            sog: true,
            sampled_pdf: true,
            gaussian_pdf: true,
            waves: Some(MatrixWaveBins::default()),
            sog_bins: 120,
        }
    }
}

fn invalid(value: &str) -> AppError {
    AppError::BadOption {
        field: "WhirlwindMatrixPolar export",
        value: value.to_owned(),
    }
}

impl MatrixExportOptions {
    pub(crate) fn validate(&self, wind_cells: usize) -> Result<usize> {
        if !self.sog && !self.sampled_pdf && !self.gaussian_pdf {
            return Err(invalid("Select at least one matrix data mode"));
        }
        if self.sampled_pdf && !(2..=512).contains(&self.sog_bins) {
            return Err(invalid("Histogram speed bins must be between 2 and 512"));
        }
        let waves = if let Some(w) = &self.waves {
            w.height.validate(100.0)?;
            w.angle.validate()?;
            w.period.validate(1000.0)?;
            (w.height.count() as usize) * (w.angle.count() as usize) * (w.period.count() as usize)
        } else {
            1
        };
        let per_cell = usize::from(self.sog)
            + if self.sampled_pdf {
                self.sog_bins as usize
            } else {
                0
            }
            + 2 * usize::from(self.gaussian_pdf);
        let count = waves
            .checked_mul(wind_cells)
            .and_then(|n| n.checked_mul(per_cell));
        if waves > 4096 || count.is_none_or(|n| n > pe_polar::matrix::MAX_MATRIX_VALUES) {
            return Err(invalid(
                "The selected matrix exceeds 4096 wave combinations or 16000000 stored values; reduce its bins or wind grid",
            ));
        }
        Ok(waves)
    }
}

fn wave_values(project: &Project, sample: &Sample) -> Option<[f64; 3]> {
    let heading = if project.blend.use_corrected {
        sample.heading_corrected.or(sample.heading)
    } else {
        sample.heading
    }?;
    Some([
        sample.hs_m?,
        (sample.wave_from? - heading).rem_euclid(360.0),
        sample.wave_period_s?,
    ])
}

fn in_cell(bins: &MatrixWaveBins, cell: [u32; 3], values: [f64; 3]) -> bool {
    bins.height.bucket(values[0], false) == Some(cell[0])
        && bins.angle.bucket(values[1]) == Some(cell[1])
        && bins.period.bucket(values[2], true) == Some(cell[2])
}

fn sources_for(
    project: &Project,
    wave: Option<(&MatrixWaveBins, [u32; 3])>,
) -> BTreeMap<u64, Arc<Derived>> {
    let masks = crate::priority::filter_flags_where(project, |sample| {
        wave.is_none_or(|(bins, cell)| {
            wave_values(project, sample).is_some_and(|values| in_cell(bins, cell, values))
        })
    });
    project
        .sources
        .iter()
        .filter(|source| source.visible && source.weight > 0.0)
        .map(|source| {
            let (mut base, track) = crate::derived::derive_base_with_filters(
                project,
                source,
                masks.get(&source.id.raw()),
            );
            // An imported wave matrix is conditioned by its stored SOG coordinates.
            // A certificate or ordinary 2D polar is wave-independent and contributes
            // to every wave condition as the same baseline.
            if let (
                Some((bins, cell)),
                SourceKind::PolarFile {
                    matrix: Some(matrix),
                    ..
                },
            ) = (wave, &source.kind)
                && !matrix.wave_height_m.is_empty()
            {
                base = Polar::empty(
                    matrix.twa_deg.iter().map(|v| f64::from(*v)).collect(),
                    matrix.tws_kn.iter().map(|v| f64::from(*v)).collect(),
                );
                let mut sums = vec![0.0; matrix.twa_deg.len() * matrix.tws_kn.len()];
                let mut counts = vec![0u32; sums.len()];
                if let Some(sog) = &matrix.sog {
                    let mut wave_index = 0;
                    for h in &matrix.wave_height_m {
                        for d in &matrix.wave_angle_deg {
                            for p in &matrix.wave_period_s {
                                if in_cell(
                                    bins,
                                    cell,
                                    [f64::from(*h), f64::from(*d), f64::from(*p)],
                                ) {
                                    for k in 0..sums.len() {
                                        if let Some(v) = sog[wave_index * sums.len() + k] {
                                            sums[k] += f64::from(v);
                                            counts[k] += 1;
                                        }
                                    }
                                }
                                wave_index += 1;
                            }
                        }
                    }
                }
                for (k, v) in base.bsp.iter_mut().flatten().enumerate() {
                    if counts[k] > 0 {
                        *v = Some(sums[k] / f64::from(counts[k]));
                    }
                }
                base = pe_polar::grid::directional(&base, project.blend.asymmetric);
            }
            let edited = pe_polar::with_overlay(base.clone(), &source.overlay, false);
            let blend = pe_polar::with_overlay(edited.clone(), &source.overlay, true);
            (
                source.id.raw(),
                Arc::new(Derived {
                    base,
                    edited,
                    blend,
                    track,
                }),
            )
        })
        .collect()
}

fn resample(project: &Project, polar: &Polar, axes: Option<&ExportAxes>) -> Polar {
    axes.map_or_else(
        || polar.clone(),
        |axes| {
            pe_polar::blend::resample_blend_mode(
                polar,
                &axes.twa,
                &axes.tws,
                project.blend.interpolation,
            )
        },
    )
}

/// Gaussian parameters use the same weighted moments/statistics as the views.
/// Resampling moments retains disagreement between neighboring distributions.
fn gaussians(
    project: &Project,
    sources: &BTreeMap<u64, Arc<Derived>>,
    axes: Option<&ExportAxes>,
) -> Vec<Option<GaussianPdf>> {
    let cells = crate::polar_pdf::fit(project, sources, false, 0);
    let mut mean = Polar::empty(project.grid.twa.clone(), project.grid.tws.clone());
    let mut second = mean.clone();
    for cell in cells {
        let (i, j) = (
            cell.wind_cell / project.grid.tws.len(),
            cell.wind_cell % project.grid.tws.len(),
        );
        mean.bsp[i][j] = Some(cell.mean);
        second.bsp[i][j] = Some(cell.mean * cell.mean + cell.stddev * cell.stddev);
    }
    let (mean, second) = (
        resample(project, &mean, axes),
        resample(project, &second, axes),
    );
    mean.twa
        .iter()
        .zip(&mean.bsp)
        .zip(&second.bsp)
        .flat_map(|((angle, row), squares)| {
            row.iter().zip(squares).map(move |(m, s)| {
                if *angle == 0.0 || *angle == 360.0 {
                    return None;
                }
                m.zip(*s).map(|(m, s)| GaussianPdf {
                    mean_kn: m as f32,
                    std_dev_kn: (s - m * m).max(0.25).sqrt() as f32,
                })
            })
        })
        .collect()
}

/// Histogram shape from admitted samples, weighted by the blend's effective
/// source contributions. Track residuals are centered on the selected statistic
/// (including node edits), matching the Gaussian calculation's location rule.
fn histograms(
    project: &Project,
    sources: &BTreeMap<u64, Arc<Derived>>,
    axes: Option<&ExportAxes>,
    bins: usize,
) -> Vec<f32> {
    let ni = project.grid.twa.len();
    let nj = project.grid.tws.len();
    let width = 60.0 / bins as f64;
    let raw: BTreeMap<u64, Vec<Vec<f64>>> = project
        .sources
        .iter()
        .filter_map(|source| {
            let track = source.track()?;
            let derived = sources.get(&source.id.raw())?.track.as_ref()?;
            let mut cells = vec![Vec::new(); ni * nj];
            for ((sample, point), filtered) in track
                .samples
                .iter()
                .zip(&derived.points)
                .zip(&derived.filtered)
            {
                if *filtered
                    || source
                        .overlay
                        .excluded_samples
                        .binary_search(&sample.id)
                        .is_ok()
                {
                    continue;
                }
                let Some((a, w, v)) = *point else { continue };
                let Some(a) = pe_polar::grid::angle_on_axis(&project.grid.twa, a) else {
                    continue;
                };
                if let (Some(i), Some(j)) = (
                    pe_polar::segment::bin_index(&project.grid.twa, a),
                    pe_polar::segment::bin_index(&project.grid.tws, w),
                ) {
                    cells[i * nj + j].push(v);
                }
            }
            Some((source.id.raw(), cells))
        })
        .collect();
    crate::blend::with_blend_sources_view(project, sources, false, |inputs, options| {
        let mut grids =
            vec![Polar::empty(project.grid.twa.clone(), project.grid.tws.clone()); bins];
        for i in 0..ni {
            for j in 0..nj {
                if project.grid.twa[i] == 0.0 || project.grid.twa[i] == 360.0 {
                    continue;
                }
                let terms = pe_polar::blend::contributions(
                    &project.grid.twa,
                    &project.grid.tws,
                    inputs,
                    options,
                    i,
                    j,
                );
                let total: f64 = terms.iter().map(|t| t.weight).sum();
                if total <= 0.0 {
                    continue;
                }
                let mut probabilities = vec![0.0; bins];
                let mut deposit = |speed: f64, weight: f64| {
                    let b = ((speed.clamp(0.0, 60.0) / width).floor() as usize).min(bins - 1);
                    probabilities[b] += weight / total;
                };
                for term in terms {
                    let speeds = raw
                        .get(&term.id)
                        .map(|v| v[i * nj + j].as_slice())
                        .unwrap_or(&[]);
                    if speeds.is_empty() {
                        deposit(term.bsp, term.weight);
                    } else {
                        let center = speeds.iter().sum::<f64>() / speeds.len() as f64;
                        for speed in speeds {
                            deposit(speed + term.bsp - center, term.weight / speeds.len() as f64);
                        }
                    }
                }
                for (b, value) in probabilities.into_iter().enumerate() {
                    grids[b].bsp[i][j] = Some(value);
                }
            }
        }
        let options = pe_polar::BlendOptions {
            polar_statistic: pe_core::project::PolarStatistic::Mean,
            ..*options
        };
        let grids: Vec<_> = grids
            .into_iter()
            .map(|grid| {
                let filled = pe_polar::blend::blend_mode(
                    &project.grid.twa,
                    &project.grid.tws,
                    &[pe_polar::BlendSource {
                        id: 0,
                        grid: &grid,
                        weight: 1.0,
                        confidence: pe_polar::Confidence::Full,
                    }],
                    &options,
                    project.blend.interpolation,
                );
                resample(project, &filled.polar, axes)
            })
            .collect();
        let before = resample(
            project,
            &crate::blend::assemble_base(project, sources).polar,
            axes,
        );
        let after = resample(
            project,
            &crate::blend::assemble(project, sources).polar,
            axes,
        );
        let out_count = after.twa.len() * after.tws.len();
        let mut out = vec![0.0; out_count * bins];
        for k in 0..out_count {
            let (i, j) = (k / after.tws.len(), k % after.tws.len());
            let shift = after
                .get(i, j)
                .zip(before.get(i, j))
                .map_or(0.0, |(a, b)| a - b);
            let mut values = vec![0.0; bins];
            for (b, grid) in grids.iter().enumerate() {
                let mass = grid.get(i, j).unwrap_or(0.0).max(0.0);
                let at = (b as f64 + shift / width).clamp(0.0, (bins - 1) as f64);
                let lo = at.floor() as usize;
                let hi = (lo + 1).min(bins - 1);
                let f = at - lo as f64;
                values[lo] += mass * (1.0 - f);
                values[hi] += mass * f;
            }
            let sum: f64 = values.iter().sum();
            if sum > 0.0 {
                for (b, value) in values.into_iter().enumerate() {
                    out[k * bins + b] = (value / sum / width) as f32;
                }
            }
        }
        out
    })
}

pub(crate) fn build(
    project: &Project,
    axes: Option<&ExportAxes>,
    options: &MatrixExportOptions,
) -> Result<WhirlwindMatrixPolar> {
    if let Some(axes) = axes {
        pe_core::project::validate_grid_axis(&axes.twa, "TWA", 360.0)?;
        pe_core::project::validate_grid_axis(&axes.tws, "TWS", 70.0)?;
    }
    let (twa, tws) = axes.map_or((&project.grid.twa, &project.grid.tws), |a| (&a.twa, &a.tws));
    let wave_count = options.validate(twa.len() * tws.len())?;
    // Histogram interpolation temporarily holds one field per speed bin on
    // the source wind grid, even when the requested output grid is smaller.
    if options.sampled_pdf
        && project
            .grid
            .twa
            .len()
            .checked_mul(project.grid.tws.len())
            .and_then(|n| n.checked_mul(options.sog_bins as usize))
            .is_none_or(|n| n > pe_polar::matrix::MAX_MATRIX_VALUES)
    {
        return Err(invalid(
            "The histogram on the project wind grid is too large; reduce its speed bins or wind grid",
        ));
    }
    let (heights, angles, periods) = options
        .waves
        .as_ref()
        .map_or((vec![], vec![], vec![]), |w| {
            (
                w.height.centers(100.0),
                w.angle.centers(),
                w.period.centers(1000.0),
            )
        });
    let mut matrix = WhirlwindMatrixPolar {
        twa_deg: twa.iter().map(|v| *v as f32).collect(),
        tws_kn: tws.iter().map(|v| *v as f32).collect(),
        wave_height_m: heights,
        wave_angle_deg: angles,
        wave_period_s: periods,
        sog_bins_kn: if options.sampled_pdf {
            (0..options.sog_bins)
                .map(|i| ((f64::from(i) + 0.5) * 60.0 / f64::from(options.sog_bins)) as f32)
                .collect()
        } else {
            vec![]
        },
        sog: options.sog.then(Vec::new),
        pdf: options.sampled_pdf.then(Vec::new),
        pdf_gaussians: options.gaussian_pdf.then(Vec::new),
    };
    let mut populated = false;
    for wave_index in 0..wave_count {
        let wave = options.waves.as_ref().map(|w| {
            let p = wave_index as u32 % w.period.count();
            let d = (wave_index as u32 / w.period.count()) % w.angle.count();
            let h = wave_index as u32 / (w.period.count() * w.angle.count());
            (w, [h, d, p])
        });
        let derived = sources_for(project, wave);
        let polar = resample(
            project,
            &crate::blend::assemble(project, &derived).polar,
            axes,
        );
        populated |= pe_polar::blend::has_value_off_zero_row(&polar);
        if let Some(sog) = &mut matrix.sog {
            sog.extend(polar.bsp.iter().flatten().map(|v| v.map(|v| v as f32)));
        }
        if let Some(pdf) = &mut matrix.pdf {
            pdf.extend(histograms(
                project,
                &derived,
                axes,
                options.sog_bins as usize,
            ));
        }
        if let Some(gaussian) = &mut matrix.pdf_gaussians {
            gaussian.extend(gaussians(project, &derived, axes));
        }
    }
    if !populated {
        return Err(AppError::Export {
            code: "empty",
            message: "The selected wave conditions have no blend data".into(),
        });
    }
    matrix.validate().map_err(|e| invalid(&e.to_string()))?;
    Ok(matrix)
}
