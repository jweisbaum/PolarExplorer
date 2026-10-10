//! The blend (spec.md 12.3): every visible source's polar on the output
//! grid, combined cell by cell into the one polar that is exported.
//!
//! **The blending rule lives in [`blend`] and nowhere else**, so it can be
//! changed without touching a view. Polars use the selected statistic:
//! a source-weighted mean, min, max, or linearly interpolated median/p90
//! over positive-weight speeds. Their combined weight is retained when
//! mixing with tracks. The default mean preserves the original rule:
//!
//! ```text
//! blend = Σ wᵢ · bspᵢ / Σ wᵢ,   wᵢ = source weight × confidence
//! ```
//!
//! Confidence is 1 for a polar source and `min(1, n / n_full)` for a track
//! segment, `n` being the cell's sample count (plan.md §6 Q4); a track cell
//! the person overrode counts with confidence 1, whatever its count — they
//! vouched for it (D23, controller ruling). Cells no
//! source reaches are then filled, in order: along TWA within the same TWS,
//! then along TWS, each only **between** two known values (nothing is
//! extrapolated, spec.md 12.2); the 0° row is 0 kn by definition and takes
//! no part in either (as in binning, D22). Optional smoothing runs over the
//! filled grid. Every cell remembers whether it had direct evidence or was
//! filled, which the source list shows as the blend's coverage; the 0° row,
//! 0 kn by definition, is no evidence and counts in neither.
//!
//! **Derived, never stored** (invariant 2). **Deterministic** (invariant 5):
//! sources are summed in id order — never list order, which the person
//! reorders for display — every step is plain IEEE arithmetic, and each
//! value is rounded to the canonical knot precision, so the same project
//! gives the same bits on every platform.

use pe_core::canonical;
use pe_core::source::Overlay;

use crate::Polar;
use crate::grid::interpolate_from;

/// How close to an axis value counts as on it.
const ON_AXIS: f64 = 1e-9;

/// How sure a source is of each of its cells.
#[derive(Debug, Clone, Copy)]
pub enum Confidence<'a> {
    /// A polar source (ORC, file): 1 everywhere.
    Full,
    /// A track segment on the output grid, `[twa][tws]`.
    Samples {
        /// Each cell's sample count.
        count: &'a [Vec<u32>],
        /// Whether the person overrode the cell: then it counts fully.
        overridden: &'a [Vec<bool>],
    },
}

/// One visible source, read onto the output grid.
#[derive(Debug, Clone, Copy)]
pub struct BlendSource<'a> {
    /// Its id: the order sources are summed in.
    pub id: u64,
    /// Its polar on the output grid's axes, overrides in and exclusions out.
    pub grid: &'a Polar,
    /// Its weight, 0–1 (spec.md 8).
    pub weight: f64,
    /// How sure it is of each cell.
    pub confidence: Confidence<'a>,
}

/// The settings the rule reads (spec.md 12.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlendOptions {
    /// Samples at which a track cell reaches full confidence.
    pub n_full: u32,
    /// Statistic across polar sources before combining with tracks.
    pub polar_statistic: pe_core::project::PolarStatistic,
    /// Smooth the filled grid.
    pub smoothing: bool,
}

/// Where a cell's value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellOrigin {
    /// At least one source had a value there.
    Direct,
    /// Interpolated from other cells, or the 0° row's 0 kn.
    Filled,
    /// Nothing reaches it.
    Empty,
}

/// How much of the blend is evidence (spec.md 12.3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Coverage {
    /// Cells with direct evidence.
    pub direct: usize,
    /// Cells filled.
    pub filled: usize,
    /// Cells left empty.
    pub empty: usize,
}

/// The blended polar and where each cell came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Blend {
    /// The polar on the output grid.
    pub polar: Polar,
    /// Each cell's origin, `[twa][tws]`.
    pub origin: Vec<Vec<CellOrigin>>,
}

/// Whether an angle is the 0° row: 0 kn by definition, no evidence.
fn is_zero_row(twa: f64) -> bool {
    twa.abs() <= ON_AXIS || (twa - 360.0).abs() <= ON_AXIS
}

/// Whether `polar` holds a value anywhere off the 0° row: a blend that does
/// not has nothing to say (its 0° zeros are definition, not evidence).
pub fn has_value_off_zero_row(polar: &Polar) -> bool {
    polar
        .twa
        .iter()
        .zip(&polar.bsp)
        .any(|(twa, row)| !is_zero_row(*twa) && row.iter().any(Option::is_some))
}

/// The blend read onto other axes (a custom export grid, spec.md 12.4):
/// bilinear, never extrapolated, and — as in the fill (D23) — the 0° row is
/// no anchor: it is left out of the reading, and an output 0° row is 0 kn.
pub fn resample_blend(blend: &Polar, twa: &[f64], tws: &[f64]) -> Polar {
    resample_blend_mode(blend, twa, tws, pe_core::project::Interpolation::Linear)
}

/// Resample with the project's interpolation rule, retaining the zero-row rule.
pub fn resample_blend_mode(
    blend: &Polar,
    twa: &[f64],
    tws: &[f64],
    mode: pe_core::project::Interpolation,
) -> Polar {
    // Retain the axis (including 360°) so a sparse asymmetric grid cannot
    // accidentally become a half-circle source and mirror its other side.
    let mut without_zero = blend.clone();
    for (angle, row) in without_zero.twa.iter().zip(&mut without_zero.bsp) {
        if is_zero_row(*angle) {
            row.fill(None);
        }
    }
    let reader = crate::spline::Interpolator::new(&without_zero, mode);
    let mut out = Polar {
        twa: twa.to_vec(),
        tws: tws.to_vec(),
        bsp: twa
            .iter()
            .map(|a| tws.iter().map(|s| reader.at(*a, *s)).collect())
            .collect(),
    };
    for (angle, row) in out.twa.iter().zip(out.bsp.iter_mut()) {
        if is_zero_row(*angle) {
            row.iter_mut().for_each(|cell| *cell = Some(0.0));
        }
    }
    out
}

#[cfg(test)]
mod directional_regressions {
    use super::*;
    #[test]
    fn a_sparse_full_circle_never_mirrors_or_anchors_on_headwind() {
        let polar = Polar {
            twa: vec![0.0, 45.0, 90.0, 360.0],
            tws: vec![10.0],
            bsp: vec![
                vec![Some(0.0)],
                vec![Some(4.0)],
                vec![Some(6.0)],
                vec![Some(0.0)],
            ],
        };
        for mode in [
            pe_core::project::Interpolation::Linear,
            pe_core::project::Interpolation::MonotoneSpline,
        ] {
            let sampled =
                resample_blend_mode(&polar, &[0.0, 20.0, 67.5, 270.0, 360.0], &[10.0], mode);
            assert_eq!(
                sampled.bsp,
                vec![
                    vec![Some(0.0)],
                    vec![None],
                    vec![Some(5.0)],
                    vec![None],
                    vec![Some(0.0)]
                ]
            );
        }
    }
}

impl Blend {
    /// Counts the cells by origin, off the 0° row.
    pub fn coverage(&self) -> Coverage {
        let mut out = Coverage::default();
        let rows = self.polar.twa.iter().zip(&self.origin);
        for origin in rows
            .filter(|(twa, _)| !is_zero_row(**twa))
            .flat_map(|(_, row)| row)
        {
            match origin {
                CellOrigin::Direct => out.direct += 1,
                CellOrigin::Filled => out.filled += 1,
                CellOrigin::Empty => out.empty += 1,
            }
        }
        out
    }
}

/// A polar source's grid read onto the output axes for the blend: bilinear,
/// never extrapolated (spec.md 12.2), with its overrides already written in
/// (`edited`), and **every output cell read from an excluded node empty**
/// (spec.md 12.3: "exclusions remove the cell"). Reading across the
/// excluded node from its neighbours instead would put back, in part, the
/// very value the person took out.
pub fn on_grid(edited: &Polar, overlay: &Overlay, twa: &[f64], tws: &[f64]) -> Polar {
    on_grid_mode(
        edited,
        overlay,
        twa,
        tws,
        pe_core::project::Interpolation::Linear,
    )
}

/// Preserve excluded-node coverage while using only included nodes for spline slopes.
pub fn on_grid_mode(
    edited: &Polar,
    overlay: &Overlay,
    twa: &[f64],
    tws: &[f64],
    mode: pe_core::project::Interpolation,
) -> Polar {
    let clean = crate::with_overlay(edited.clone(), overlay, true);
    let reader = crate::spline::Interpolator::new(&clean, mode);
    let excluded = |(i, j): (usize, usize)| match (edited.twa.get(i), edited.tws.get(j)) {
        (Some(a), Some(s)) => overlay.is_cell_excluded(*a, *s),
        _ => false,
    };
    let bsp = twa
        .iter()
        .map(|angle| {
            tws.iter()
                .map(|speed| {
                    let (value, corners) = interpolate_from(edited, *angle, *speed)?;
                    if corners.iter().any(|node| excluded(*node)) {
                        None
                    } else if mode == pe_core::project::Interpolation::Linear {
                        Some(value)
                    } else {
                        reader.at(*angle, *speed)
                    }
                })
                .collect()
        })
        .collect();
    Polar {
        twa: twa.to_vec(),
        tws: tws.to_vec(),
        bsp,
    }
}

/// The weight a source gives cell (`i`, `j`).
fn weight_at(source: &BlendSource<'_>, i: usize, j: usize, n_full: u32) -> f64 {
    let confidence = match source.confidence {
        Confidence::Full => 1.0,
        Confidence::Samples { count, overridden } => {
            if overridden
                .get(i)
                .and_then(|row| row.get(j))
                .copied()
                .unwrap_or(false)
            {
                return source.weight;
            }
            let n = count
                .get(i)
                .and_then(|row| row.get(j))
                .copied()
                .unwrap_or(0);
            (f64::from(n) / f64::from(n_full.max(1))).min(1.0)
        }
    };
    source.weight * confidence
}

/// One source's part in a cell's direct value: a term of the weighted mean.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contribution {
    /// The source.
    pub id: u64,
    /// Its boat speed in the cell, knots.
    pub bsp: f64,
    /// Effective weight after polar selection or track sample confidence.
    pub weight: f64,
}

/// The sources the rule sums, in the order it sums them: those on the
/// output grid's axes, by id.
fn summed<'a, 'b>(
    sources: &'b [BlendSource<'a>],
    ni: usize,
    nj: usize,
) -> Vec<&'b BlendSource<'a>> {
    let mut ordered: Vec<&BlendSource<'_>> = sources
        .iter()
        .filter(|s| s.grid.twa.len() == ni && s.grid.tws.len() == nj)
        .collect();
    ordered.sort_by_key(|s| s.id);
    ordered
}

/// The terms of cell (`i`, `j`)'s weighted mean: every source with a value
/// there and a positive weight, in summing order. The one place the rule
/// decides who takes part, shared by [`blend_mode`] and [`contributions`].
fn terms<'b>(
    ordered: &'b [&'b BlendSource<'_>],
    i: usize,
    j: usize,
    options: &BlendOptions,
) -> std::vec::IntoIter<Contribution> {
    use pe_core::project::PolarStatistic;
    let mut result = Vec::new();
    let mut polars = Vec::new();
    for source in ordered {
        let Some(bsp) = source.grid.get(i, j) else {
            continue;
        };
        let weight = weight_at(source, i, j, options.n_full);
        if weight <= 0.0 {
            continue;
        }
        if options.polar_statistic != PolarStatistic::Mean
            && matches!(source.confidence, Confidence::Full)
        {
            polars.push(result.len());
        }
        result.push(Contribution {
            id: source.id,
            bsp,
            weight,
        });
    }
    if options.polar_statistic != PolarStatistic::Mean {
        polars.sort_by(|a, b| {
            result[*a]
                .bsp
                .total_cmp(&result[*b].bsp)
                .then_with(|| result[*a].id.cmp(&result[*b].id))
        });
        if !polars.is_empty() {
            let total: f64 = polars.iter().map(|index| result[*index].weight).sum();
            let fraction = match options.polar_statistic {
                PolarStatistic::Min => 0.0,
                PolarStatistic::Median => 0.5,
                PolarStatistic::Max => 1.0,
                PolarStatistic::P90 => 0.9,
                PolarStatistic::Mean => unreachable!(),
            };
            let rank = fraction * (polars.len() - 1) as f64;
            let lo = rank.floor() as usize;
            let hi = rank.ceil() as usize;
            for index in &polars {
                result[*index].weight = 0.0;
            }
            result[polars[lo]].weight += total * (1.0 - rank.fract());
            result[polars[hi]].weight += total * rank.fract();
            result.retain(|term| term.weight > 0.0);
        }
    }
    result.into_iter()
}

/// What stands behind cell (`i`, `j`)'s direct value (spec.md 12.3): each
/// source the rule counted there, with its speed and the weight it had.
/// Empty for a cell with no direct evidence (filled, or nothing at all) and
/// for a cell off the grid. Their weighted mean is the cell's value before
/// smoothing and before any manual correction.
pub fn contributions(
    twa: &[f64],
    tws: &[f64],
    sources: &[BlendSource<'_>],
    options: &BlendOptions,
    i: usize,
    j: usize,
) -> Vec<Contribution> {
    if i >= twa.len() || j >= tws.len() {
        return Vec::new();
    }
    let ordered = summed(sources, twa.len(), tws.len());
    terms(&ordered, i, j, options).collect()
}

/// **The blending rule** (spec.md 12.3; see the module documentation), on
/// the output grid `twa` × `tws`. A source whose grid is not on those axes
/// takes no part.
pub fn blend(
    twa: &[f64],
    tws: &[f64],
    sources: &[BlendSource<'_>],
    options: &BlendOptions,
) -> Blend {
    blend_mode(
        twa,
        tws,
        sources,
        options,
        pe_core::project::Interpolation::Linear,
    )
}

/// Blend with the selected interpolation for the two ordered filling passes.
pub fn blend_mode(
    twa: &[f64],
    tws: &[f64],
    sources: &[BlendSource<'_>],
    options: &BlendOptions,
    mode: pe_core::project::Interpolation,
) -> Blend {
    let (ni, nj) = (twa.len(), tws.len());
    let ordered = summed(sources, ni, nj);
    let zero_row: Vec<bool> = twa.iter().map(|a| is_zero_row(*a)).collect();

    // Direct evidence: the weighted mean of every source with a value.
    let mut value: Vec<Vec<Option<f64>>> = vec![vec![None; nj]; ni];
    let mut origin = vec![vec![CellOrigin::Empty; nj]; ni];
    for i in 0..ni {
        for j in 0..nj {
            let (mut sum, mut weights) = (0.0, 0.0);
            for term in terms(&ordered, i, j, options) {
                sum += term.weight * term.bsp;
                weights += term.weight;
            }
            if weights > 0.0 {
                origin[i][j] = CellOrigin::Direct;
                if !zero_row[i] {
                    value[i][j] = Some(sum / weights);
                }
            }
        }
    }

    // Fill along TWA within each TWS, between known values only.
    let direct = value.clone();
    for j in 0..nj {
        let known: Vec<usize> = (0..ni).filter(|&i| direct[i][j].is_some()).collect();
        let curve = crate::spline::Curve::new(
            known
                .iter()
                .filter_map(|i| Some((twa[*i], direct[*i][j]?)))
                .collect(),
            mode,
        );
        for pair in known.windows(2) {
            let (lo, hi) = (pair[0], pair[1]);
            for i in lo + 1..hi {
                value[i][j] = curve.at(twa[i]);
                origin[i][j] = CellOrigin::Filled;
            }
        }
    }
    // Then along TWS within each TWA, from what the first step left.
    for i in (0..ni).filter(|&i| !zero_row[i]) {
        let row = value[i].clone();
        let known: Vec<usize> = (0..nj).filter(|&j| row[j].is_some()).collect();
        let curve = crate::spline::Curve::new(
            known
                .iter()
                .filter_map(|j| Some((tws[*j], row[*j]?)))
                .collect(),
            mode,
        );
        for pair in known.windows(2) {
            let (lo, hi) = (pair[0], pair[1]);
            for j in lo + 1..hi {
                value[i][j] = curve.at(tws[j]);
                origin[i][j] = CellOrigin::Filled;
            }
        }
    }
    // The 0° row is 0 kn: a boat does not sail head to wind.
    for i in (0..ni).filter(|&i| zero_row[i]) {
        for j in 0..nj {
            value[i][j] = Some(0.0);
            if origin[i][j] == CellOrigin::Empty {
                origin[i][j] = CellOrigin::Filled;
            }
        }
    }

    if options.smoothing {
        value = smooth(&value, &zero_row);
    }

    let bsp = value
        .into_iter()
        .map(|row| row.into_iter().map(|v| v.map(canonical::knots)).collect())
        .collect();
    Blend {
        polar: Polar {
            twa: twa.to_vec(),
            tws: tws.to_vec(),
            bsp,
        },
        origin,
    }
}

/// The 3×3 binomial kernel, as the edit tool's smooth (D22).
const KERNEL: [[f64; 3]; 3] = [[1.0, 2.0, 1.0], [2.0, 4.0, 2.0], [1.0, 2.0, 1.0]];

/// Every cell with a value smoothed over its neighbours with a value; the
/// 0° row stays 0 kn and takes no part. Empty cells stay empty.
fn smooth(grid: &[Vec<Option<f64>>], zero_row: &[bool]) -> Vec<Vec<Option<f64>>> {
    let at = |i: usize, j: usize| -> Option<f64> {
        if zero_row.get(i).copied().unwrap_or(true) {
            return None;
        }
        grid.get(i)?.get(j).copied().flatten()
    };
    grid.iter()
        .enumerate()
        .map(|(i, row)| {
            row.iter()
                .enumerate()
                .map(|(j, cell)| {
                    let own = (*cell)?;
                    if zero_row[i] {
                        return Some(own);
                    }
                    let (mut sum, mut weight) = (0.0, 0.0);
                    for (di, kernel_row) in KERNEL.iter().enumerate() {
                        for (dj, w) in kernel_row.iter().enumerate() {
                            let (Some(ni), Some(nj)) =
                                ((i + di).checked_sub(1), (j + dj).checked_sub(1))
                            else {
                                continue;
                            };
                            if let Some(v) = at(ni, nj) {
                                sum += w * v;
                                weight += w;
                            }
                        }
                    }
                    Some(if weight > 0.0 { sum / weight } else { own })
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use pe_core::source::CellRef;

    use super::*;

    const TWA: [f64; 5] = [0.0, 45.0, 90.0, 135.0, 180.0];
    const TWS: [f64; 3] = [6.0, 10.0, 14.0];

    fn grid(cells: &[(usize, usize, f64)]) -> Polar {
        let mut polar = Polar::empty(TWA.to_vec(), TWS.to_vec());
        for (i, j, v) in cells {
            polar.bsp[*i][*j] = Some(*v);
        }
        polar
    }

    fn options() -> BlendOptions {
        BlendOptions {
            polar_statistic: Default::default(),
            n_full: 30,
            smoothing: false,
        }
    }

    fn polar(id: u64, grid: &Polar, weight: f64) -> BlendSource<'_> {
        BlendSource {
            id,
            grid,
            weight,
            confidence: Confidence::Full,
        }
    }

    /// A single visible source blends to itself where it has values
    /// (plan.md testing strategy: blend invariants).
    #[test]
    fn a_single_source_blends_to_itself() {
        let full: Vec<(usize, usize, f64)> = (1..5)
            .flat_map(|i| (0..3).map(move |j| (i, j, 4.0 + i as f64 + j as f64 * 0.5)))
            .collect();
        let g = grid(&full);
        let out = blend(&TWA, &TWS, &[polar(1, &g, 1.3)], &options());
        for (i, j, v) in &full {
            assert_eq!(out.polar.bsp[*i][*j], Some(*v));
            assert_eq!(out.origin[*i][*j], CellOrigin::Direct);
        }
        assert_eq!(out.polar.bsp[0], vec![Some(0.0); 3]);
        assert_eq!(
            out.coverage(),
            Coverage {
                direct: 12,
                filled: 0,
                empty: 0
            }
        );
    }

    /// Hand-computed: weights 1 and 3 over 6 and 8 give (6 + 24) / 4 = 7.5;
    /// scaling every weight by the same factor changes nothing.
    #[test]
    fn the_weighted_mean_is_scale_free() {
        let a = grid(&[(2, 1, 6.0)]);
        let b = grid(&[(2, 1, 8.0)]);
        let once = blend(
            &TWA,
            &TWS,
            &[polar(1, &a, 0.5), polar(2, &b, 1.5)],
            &options(),
        );
        assert_eq!(once.polar.bsp[2][1], Some(7.5));
        let twice = blend(
            &TWA,
            &TWS,
            &[polar(1, &a, 1.0), polar(2, &b, 3.0)],
            &options(),
        );
        assert_eq!(once, twice);
        // Order does not matter: summed by id.
        let swapped = blend(
            &TWA,
            &TWS,
            &[polar(2, &b, 1.5), polar(1, &a, 0.5)],
            &options(),
        );
        assert_eq!(once, swapped);
    }

    /// A track cell counts n / n_full: 15 of 30 samples at 9 kn beside a
    /// polar at 6 kn gives (6 + 0.5 × 9) / 1.5 = 7. At 60 samples it is
    /// capped at 1: (6 + 9) / 2 = 7.5. A zero weight takes no part.
    #[test]
    fn a_track_cell_counts_by_its_samples() {
        let p = grid(&[(2, 1, 6.0)]);
        let t = grid(&[(2, 1, 9.0)]);
        let mut count = vec![vec![0u32; 3]; 5];
        count[2][1] = 15;
        let none = vec![vec![false; 3]; 5];
        fn track<'a>(
            grid: &'a Polar,
            count: &'a [Vec<u32>],
            overridden: &'a [Vec<bool>],
        ) -> BlendSource<'a> {
            BlendSource {
                id: 2,
                grid,
                weight: 1.0,
                confidence: Confidence::Samples { count, overridden },
            }
        }
        let out = blend(
            &TWA,
            &TWS,
            &[polar(1, &p, 1.0), track(&t, &count, &none)],
            &options(),
        );
        assert_eq!(out.polar.bsp[2][1], Some(7.0));
        count[2][1] = 60;
        let out = blend(
            &TWA,
            &TWS,
            &[polar(1, &p, 1.0), track(&t, &count, &none)],
            &options(),
        );
        assert_eq!(out.polar.bsp[2][1], Some(7.5));
        let out = blend(
            &TWA,
            &TWS,
            &[polar(1, &p, 0.0), track(&t, &count, &none)],
            &options(),
        );
        assert_eq!(out.polar.bsp[2][1], Some(9.0));

        // An overridden track cell counts fully (D23 ruling), even with no
        // samples at all: (6 + 9) / 2 = 7.5.
        count[2][1] = 0;
        let mut vouched = none.clone();
        vouched[2][1] = true;
        let both = [polar(1, &p, 1.0), track(&t, &count, &vouched)];
        assert_eq!(
            blend(&TWA, &TWS, &both, &options()).polar.bsp[2][1],
            Some(7.5)
        );
        let plain = [polar(1, &p, 1.0), track(&t, &count, &none)];
        assert_eq!(
            blend(&TWA, &TWS, &plain, &options()).polar.bsp[2][1],
            Some(6.0),
            "not overridden: no samples, no weight"
        );
    }

    /// What the blend tooltip shows (spec.md 10.1): each source with a
    /// value and a positive weight in the cell, in id order, with the
    /// weight the rule gave it. A polar at weight 0.5 and 6 kn, a track at
    /// weight 1 with 15 of 30 samples (so 0.5) and 9 kn: equal shares, and
    /// the blend is their mean, 7.5 kn.
    #[test]
    fn a_cells_contributions_are_the_terms_of_its_weighted_mean() {
        let p = grid(&[(2, 1, 6.0)]);
        let t = grid(&[(2, 1, 9.0)]);
        let silent = grid(&[(3, 1, 5.0)]);
        let muted = grid(&[(2, 1, 4.0)]);
        let mut count = vec![vec![0u32; 3]; 5];
        count[2][1] = 15;
        let none = vec![vec![false; 3]; 5];
        let track = BlendSource {
            id: 2,
            grid: &t,
            weight: 1.0,
            confidence: Confidence::Samples {
                count: &count,
                overridden: &none,
            },
        };
        // Given out of id order, with a source that has no value in the
        // cell (5) and one at weight 0 (9): neither takes part.
        let sources = [
            polar(9, &muted, 0.0),
            track,
            polar(5, &silent, 1.0),
            polar(1, &p, 0.5),
        ];
        let terms = contributions(&TWA, &TWS, &sources, &options(), 2, 1);
        assert_eq!(
            terms,
            vec![
                Contribution {
                    id: 1,
                    bsp: 6.0,
                    weight: 0.5
                },
                Contribution {
                    id: 2,
                    bsp: 9.0,
                    weight: 0.5
                },
            ]
        );
        assert_eq!(
            blend(&TWA, &TWS, &sources, &options()).polar.bsp[2][1],
            Some(7.5)
        );

        // An overridden track cell counts with its whole weight (D23).
        let mut vouched = none.clone();
        vouched[2][1] = true;
        let sources = [
            polar(1, &p, 0.5),
            BlendSource {
                confidence: Confidence::Samples {
                    count: &count,
                    overridden: &vouched,
                },
                ..track
            },
        ];
        let terms = contributions(&TWA, &TWS, &sources, &options(), 2, 1);
        assert_eq!(
            terms.iter().map(|c| c.weight).collect::<Vec<_>>(),
            [0.5, 1.0]
        );

        // A cell no source reaches, a cell off the grid, and a source on
        // other axes: nothing.
        assert!(contributions(&TWA, &TWS, &sources, &options(), 1, 0).is_empty());
        assert!(contributions(&TWA, &TWS, &sources, &options(), 9, 9).is_empty());
        let other = Polar::empty(vec![0.0, 90.0], vec![10.0]);
        assert!(contributions(&TWA, &TWS, &[polar(1, &other, 1.0)], &options(), 1, 0).is_empty());
    }

    /// Nothing off the 0° row is nothing: the 0° zeros are definition.
    #[test]
    fn a_blend_of_nothing_has_no_value_and_no_coverage() {
        let out = blend(&TWA, &TWS, &[], &options());
        assert!(!has_value_off_zero_row(&out.polar));
        assert_eq!(
            out.coverage(),
            Coverage {
                direct: 0,
                filled: 0,
                empty: 12
            }
        );
        assert_eq!(out.polar.bsp[0], vec![Some(0.0); 3]);
    }

    /// Custom axes read the blend without the 0° row as an anchor: 20° lies
    /// between 0° and 45°, so it is empty; the output 0° row is 0 kn; 67.5°
    /// between 45° (5) and 90° (7) is 6.
    #[test]
    fn a_custom_grid_does_not_anchor_on_the_zero_row() {
        let g = grid(&[(1, 0, 5.0), (2, 0, 7.0)]);
        let out = blend(&TWA, &TWS, &[polar(1, &g, 1.0)], &options());
        let custom = resample_blend(&out.polar, &[0.0, 20.0, 67.5], &[6.0]);
        assert_eq!(
            custom.bsp,
            vec![vec![Some(0.0)], vec![None], vec![Some(6.0)]]
        );
    }

    /// Filled along TWA first, then along TWS, never beyond known values:
    /// 45° and 135° at 6 kn fill 90° at 6 kn (by TWA, halfway: 5.5); 90° at
    /// 14 kn and that fill 90° at 10 kn (by TWS: 6.5); 180° has nothing
    /// below or beside it and stays empty.
    #[test]
    fn empty_cells_fill_along_twa_then_tws_without_extrapolating() {
        let g = grid(&[(1, 0, 5.0), (3, 0, 6.0), (2, 2, 7.5)]);
        let out = blend(&TWA, &TWS, &[polar(1, &g, 1.0)], &options());
        assert_eq!(out.polar.bsp[2][0], Some(5.5));
        assert_eq!(out.origin[2][0], CellOrigin::Filled);
        assert_eq!(out.polar.bsp[2][1], Some(6.5));
        assert_eq!(out.origin[2][1], CellOrigin::Filled);
        assert_eq!(out.polar.bsp[4], vec![None, None, None]);
        assert_eq!(out.origin[4][0], CellOrigin::Empty);
        // 45° has one value; nothing to interpolate beyond it.
        assert_eq!(out.polar.bsp[1][1], None);
        // The 0° row is 0 kn and filled.
        assert_eq!(out.polar.bsp[0], vec![Some(0.0); 3]);
        assert_eq!(out.origin[0][0], CellOrigin::Filled);
    }

    /// A polar source is resampled onto the output grid, and an output cell
    /// read from an excluded node is empty rather than read across it.
    #[test]
    fn an_excluded_node_removes_every_cell_read_from_it() {
        let source = Polar {
            twa: vec![45.0, 90.0, 135.0],
            tws: vec![6.0, 14.0],
            bsp: vec![
                vec![Some(5.0), Some(7.0)],
                vec![Some(6.0), Some(8.0)],
                vec![Some(5.5), Some(7.5)],
            ],
        };
        let mut overlay = Overlay::default();
        let clean = on_grid(&source, &overlay, &TWA, &TWS);
        assert_eq!(clean.bsp[2], vec![Some(6.0), Some(7.0), Some(8.0)]);
        assert_eq!(clean.bsp[0], vec![None, None, None]);
        overlay.excluded_cells = vec![CellRef {
            twa: 90.0,
            tws: 6.0,
        }];
        let out = on_grid(&source, &overlay, &TWA, &TWS);
        // 90° at 6 kn is the node; 90° at 10 kn reads it; 14 kn does not.
        assert_eq!(out.bsp[2], vec![None, None, Some(8.0)]);
        // 45° at 6 kn is its own node.
        assert_eq!(out.bsp[1][0], Some(5.0));
    }

    /// Smoothing is off by default; on, a spike is pulled toward its
    /// neighbours and the 0° row stays 0 kn.
    #[test]
    fn smoothing_evens_the_filled_grid() {
        let cells: Vec<(usize, usize, f64)> = (1..5)
            .flat_map(|i| (0..3).map(move |j| (i, j, if (i, j) == (2, 1) { 10.0 } else { 6.0 })))
            .collect();
        let g = grid(&cells);
        let smoothed = blend(
            &TWA,
            &TWS,
            &[polar(1, &g, 1.0)],
            &BlendOptions {
                polar_statistic: Default::default(),
                n_full: 30,
                smoothing: true,
            },
        );
        // Centre: (4 × 10 + 12 × 6) / 16 = 7.
        assert_eq!(smoothed.polar.bsp[2][1], Some(7.0));
        assert_eq!(smoothed.polar.bsp[0], vec![Some(0.0); 3]);
    }
}
