//! A track's polar segment (spec.md 12.1): its samples binned onto the
//! project's output grid.
//!
//! Each (TWA, TWS, BSP) point goes to the grid node nearest it on each axis
//! (port and starboard folded first): a node's bin runs halfway to each
//! neighbour, and as far again past the first and last node as the half-step
//! beside them, so a sample well beyond the axis is left out rather than
//! piled onto its end. A point exactly halfway goes to the upper node.
//!
//! **Nothing is binned into a 0° TWA node.** The 0° row is 0 kn by
//! definition (spec.md 12.3): a boat does not sail head to wind, and samples
//! nearest 0° are noise (tacks, the wind a little off). They are dropped,
//! not moved to the next node.
//!
//! Per cell the segment keeps every sample's count and the spread of their
//! boat speeds (for the blend weight and the tooltips), and a value — the
//! track's statistic of those speeds — only where there are at least the
//! minimum number of samples; below that the cell is empty.
//!
//! Derived, never stored (invariant 2): the caller hands in the samples that
//! pass the filters and are not excluded, and nothing here writes project
//! data. Deterministic: speeds are sorted with a total order and summed in
//! the order given, so the same samples give the same bits everywhere.

use pe_core::track::SegmentStatistic;

use crate::Polar;
use crate::grid::angle_on_axis;

/// A polar segment: the statistic grid, and what it was made from.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// The statistic of each cell with enough samples; empty elsewhere. Axes
    /// are the grid the samples were binned onto.
    pub polar: Polar,
    /// Samples binned into each cell, `[twa][tws]`, including cells below the
    /// minimum.
    pub count: Vec<Vec<u32>>,
    /// The sample standard deviation of each cell's boat speeds, knots, where
    /// it has two samples or more.
    pub spread: Vec<Vec<Option<f64>>>,
}

/// Which node of `axis` a value belongs to, or `None` beyond the axis's
/// outer half-steps. An axis of one value takes everything.
pub fn bin_index(axis: &[f64], x: f64) -> Option<usize> {
    if !x.is_finite() || axis.is_empty() {
        return None;
    }
    let n = axis.len();
    if n == 1 {
        return Some(0);
    }
    let first_half = (axis[1] - axis[0]) / 2.0;
    let last_half = (axis[n - 1] - axis[n - 2]) / 2.0;
    if x < axis[0] - first_half || x > axis[n - 1] + last_half {
        return None;
    }
    // The first node whose upper boundary lies beyond x.
    let upper = (0..n - 1).find(|&i| x < (axis[i] + axis[i + 1]) / 2.0);
    Some(upper.unwrap_or(n - 1))
}

/// The statistic of some speeds, sorted ascending (spec.md 12.1).
/// Percentiles interpolate linearly between the two nearest ranks.
pub fn statistic(sorted: &[f64], statistic: SegmentStatistic) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let quantile = |p: f64| {
        let h = (sorted.len() - 1) as f64 * p;
        let lo = h.floor() as usize;
        let hi = (lo + 1).min(sorted.len() - 1);
        sorted[lo] + (h - lo as f64) * (sorted[hi] - sorted[lo])
    };
    Some(match statistic {
        SegmentStatistic::Min => sorted[0],
        SegmentStatistic::Median => quantile(0.5),
        SegmentStatistic::Mean => sorted.iter().sum::<f64>() / sorted.len() as f64,
        SegmentStatistic::P75 => quantile(0.75),
        SegmentStatistic::P90 => quantile(0.9),
        SegmentStatistic::Max => sorted[sorted.len() - 1],
    })
}

/// The sample standard deviation of some speeds, `None` for fewer than two.
pub fn spread(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let squares: f64 = values.iter().map(|v| (v - mean) * (v - mean)).sum();
    Some((squares / (n - 1.0)).sqrt())
}

/// Bins `points` — (TWA °, TWS kn, BSP kn), in any order — onto the grid
/// `twa` × `tws`. A cell needs `min_samples` (at least one) to have a value.
pub fn bin(
    points: impl IntoIterator<Item = (f64, f64, f64)>,
    twa: &[f64],
    tws: &[f64],
    stat: SegmentStatistic,
    min_samples: u32,
) -> Segment {
    let (ni, nj) = (twa.len(), tws.len());
    let mut cells: Vec<Vec<f64>> = vec![Vec::new(); ni * nj];
    for (angle, wind, bsp) in points {
        let Some(angle) = angle_on_axis(twa, angle) else {
            continue;
        };
        if !bsp.is_finite() || bsp < 0.0 {
            continue;
        }
        let (Some(i), Some(j)) = (bin_index(twa, angle), bin_index(tws, wind)) else {
            continue;
        };
        if twa
            .get(i)
            .is_some_and(|node| *node == 0.0 || *node == 360.0)
        {
            continue;
        }
        if let Some(cell) = cells.get_mut(i * nj + j) {
            cell.push(bsp);
        }
    }
    let need = min_samples.max(1) as usize;
    let mut polar = Polar::empty(twa.to_vec(), tws.to_vec());
    let mut count = vec![vec![0u32; nj]; ni];
    let mut spreads = vec![vec![None; nj]; ni];
    for (k, values) in cells.iter_mut().enumerate() {
        let (i, j) = (k / nj, k % nj);
        count[i][j] = u32::try_from(values.len()).unwrap_or(u32::MAX);
        // Sorted before anything is summed, so the order samples came in
        // cannot change a bit of the answer.
        values.sort_by(f64::total_cmp);
        spreads[i][j] = spread(values);
        if values.len() >= need {
            polar.bsp[i][j] = statistic(values, stat);
        }
    }
    Segment {
        polar,
        count,
        spread: spreads,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Option<f64>, b: f64) {
        let a = a.unwrap_or(f64::NAN);
        assert!((a - b).abs() < 1e-12, "{a} != {b}");
    }

    /// Bins by hand on the axis 40, 52, 60: 40's bin is [34, 46), 52's
    /// [46, 56), 60's [56, 64].
    #[test]
    fn a_value_goes_to_the_nearest_node_and_halfway_goes_up() {
        let axis = [40.0, 52.0, 60.0];
        assert_eq!(bin_index(&axis, 33.9), None);
        assert_eq!(bin_index(&axis, 34.0), Some(0));
        assert_eq!(bin_index(&axis, 45.9), Some(0));
        assert_eq!(bin_index(&axis, 46.0), Some(1));
        assert_eq!(bin_index(&axis, 55.9), Some(1));
        assert_eq!(bin_index(&axis, 56.0), Some(2));
        assert_eq!(bin_index(&axis, 64.0), Some(2));
        assert_eq!(bin_index(&axis, 64.1), None);
        assert_eq!(bin_index(&axis, f64::NAN), None);
        assert_eq!(bin_index(&[10.0], 99.0), Some(0));
        assert_eq!(bin_index(&[], 10.0), None);
    }

    /// Hand-computed on 1..=10: the median is 5.5, the mean 5.5, the 75th
    /// percentile at rank 6.75 is 7.75, the 90th at rank 8.1 is 9.1.
    #[test]
    fn the_statistics_match_hand_computed_values() {
        let values: Vec<f64> = (1..=10).map(f64::from).collect();
        close(statistic(&values, SegmentStatistic::Median), 5.5);
        close(statistic(&values, SegmentStatistic::Mean), 5.5);
        close(statistic(&values, SegmentStatistic::P75), 7.75);
        close(statistic(&values, SegmentStatistic::P90), 9.1);
        close(statistic(&[4.0], SegmentStatistic::P90), 4.0);
        assert_eq!(statistic(&[], SegmentStatistic::Mean), None);
        // 2, 4, 4, 4, 5, 5, 7, 9: mean 5, squares 32, over 7.
        close(
            spread(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]),
            (32.0f64 / 7.0).sqrt(),
        );
        assert_eq!(spread(&[3.0]), None);
    }

    /// Port and starboard fold together, a cell below the minimum is empty
    /// but keeps its count, and samples beyond the grid are left out.
    #[test]
    fn samples_bin_onto_the_grid_with_counts_and_a_minimum() {
        let twa = [45.0, 90.0];
        let tws = [8.0, 12.0];
        let mut points = Vec::new();
        // Five at 90°/12 kn, two of them on port (270° folds to 90°).
        for (k, bsp) in [6.0, 7.0, 8.0, 9.0, 10.0].into_iter().enumerate() {
            let angle = if k < 2 { 270.0 } else { 88.0 };
            points.push((angle, 11.5, bsp));
        }
        // Two at 45°/8 kn: below a minimum of 5.
        points.push((44.0, 8.2, 5.0));
        points.push((316.0, 7.9, 5.4));
        // Beyond the TWS axis (bins end at 14 kn) and a speed that is none.
        points.push((90.0, 14.5, 9.0));
        points.push((90.0, 12.0, f64::NAN));
        let segment = bin(points, &twa, &tws, SegmentStatistic::Median, 5);
        assert_eq!(segment.count, vec![vec![2, 0], vec![0, 5]]);
        assert_eq!(segment.polar.bsp[0][0], None);
        close(segment.polar.bsp[1][1], 8.0);
        assert_eq!(segment.polar.bsp[1][0], None);
        close(segment.spread[0][0], (0.08f64).sqrt());
        close(segment.spread[1][1], (2.5f64).sqrt());
        segment.polar.validate().unwrap();

        // The 90th percentile of 6..10 is 9.6; a lower minimum fills 45°.
        let p90 = bin(
            [(90.0, 12.0, 8.0), (90.0, 12.0, 6.0), (90.0, 12.0, 10.0)],
            &twa,
            &tws,
            SegmentStatistic::P90,
            0,
        );
        close(p90.polar.bsp[1][1], 9.6);
    }

    /// Samples nearest the 0° node are dropped, not moved to 30°: the 0° row
    /// is 0 kn by definition (spec.md 12.3). On the axis 0, 30, 60 the 0°
    /// bin is [-15, 15).
    #[test]
    fn nothing_is_binned_into_the_zero_degree_row() {
        let twa = [0.0, 30.0, 60.0];
        let tws = [10.0];
        let points = [
            (5.0, 10.0, 3.0),
            (14.9, 10.0, 3.0),
            (355.0, 10.0, 3.0),
            (15.0, 10.0, 4.0),
        ];
        let segment = bin(points, &twa, &tws, SegmentStatistic::Mean, 1);
        assert_eq!(segment.count, vec![vec![0], vec![1], vec![0]]);
        assert_eq!(segment.polar.bsp[0][0], None);
        close(segment.polar.bsp[1][0], 4.0);
    }

    /// The same samples in another order give the same bits.
    #[test]
    fn binning_does_not_depend_on_sample_order() {
        let points: Vec<(f64, f64, f64)> = (0..200)
            .map(|k| {
                let k = f64::from(k);
                (
                    40.0 + (k * 7.3) % 140.0,
                    5.0 + (k * 3.1) % 20.0,
                    4.0 + (k * 0.37) % 6.0,
                )
            })
            .collect();
        let grid_twa = [45.0, 60.0, 90.0, 120.0, 150.0, 180.0];
        let grid_tws = [6.0, 10.0, 14.0, 20.0];
        let forward = bin(
            points.clone(),
            &grid_twa,
            &grid_tws,
            SegmentStatistic::P75,
            2,
        );
        let backward = bin(
            points.into_iter().rev(),
            &grid_twa,
            &grid_tws,
            SegmentStatistic::P75,
            2,
        );
        assert_eq!(forward, backward);
    }
}
