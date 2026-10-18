//! A polar as plain data.
//!
//! `pe-polar` owns the mathematics (interpolation, resampling, blending), but
//! a polar is also part of the document: an imported file and an ORC VPP are
//! stored as sources. `pe-polar` depends on this crate, so the data type lives
//! here and the behaviour lives there.

use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{CoreError, Result};

/// Boat speed on a TWA × TWS grid.
///
/// Rows follow `twa`, columns follow `tws`: `bsp[i][j]` is the speed at
/// `twa[i]`, `tws[j]`. A cell may be empty — a file with rows of different
/// lengths, or an ORC VPP that gives no value there — and nothing is invented
/// to fill it (spec.md 5.3).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PolarGrid {
    /// True wind angles, degrees in [0, 360], strictly increasing.
    #[serde(with = "canonical::degrees_list")]
    pub twa: Vec<f64>,
    /// True wind speeds, knots, strictly increasing.
    #[serde(with = "canonical::knots_list")]
    pub tws: Vec<f64>,
    /// Boat speed in knots, one row per TWA.
    #[serde(with = "canonical::optional_knots_rows")]
    pub bsp: Vec<Vec<Option<f64>>>,
}

/// One Gaussian approximation of a conditional boat-speed probability density.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GaussianPdf {
    /// Expected boat speed, in knots.
    pub mean_kn: f32,
    /// Standard deviation of boat speed, in knots.
    pub std_dev_kn: f32,
}

/// Complete data stored by an imported WhirlwindMatrixPolar source. The 2D
/// editor uses its derived `PolarGrid` projection while this matrix preserves
/// all wave and probability data for project save/export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhirlwindMatrixPolar {
    pub twa_deg: Vec<f32>,
    pub tws_kn: Vec<f32>,
    /// Interior height boundaries in metres. Bins include their lower bound.
    pub wave_height_cutoffs_m: Vec<f32>,
    /// 0 disables all waves; otherwise 2, 4, 8 or 20 equal bow-relative sectors.
    /// Sector i is centered at i * 360 / count, clockwise, incoming waves.
    pub wave_direction_count: u32,
    /// Interior period boundaries in seconds. Bins include their upper bound.
    pub wave_period_cutoffs_s: Vec<f32>,
    /// Boat-speed bins for the sampled PDF mode.
    pub sog_bins_kn: Vec<f32>,
    pub sog: Option<Vec<Option<f32>>>,
    /// Sampled density values per `[wave][TWA][TWS][SOG bin]` cell.
    #[serde(default)]
    pub pdf: Option<Vec<f32>>,
    /// Gaussian mean and standard deviation per `[wave][TWA][TWS]` cell.
    #[serde(default)]
    pub pdf_gaussians: Option<Vec<Option<GaussianPdf>>>,
}

impl WhirlwindMatrixPolar {
    pub fn wave_height_count(&self) -> usize {
        if self.wave_direction_count == 0 {
            0
        } else {
            self.wave_height_cutoffs_m.len() + 1
        }
    }

    pub fn wave_period_count(&self) -> usize {
        if self.wave_direction_count == 0 {
            0
        } else {
            self.wave_period_cutoffs_s.len() + 1
        }
    }
}

impl PolarGrid {
    /// An empty grid on the given axes.
    pub fn empty(twa: Vec<f64>, tws: Vec<f64>) -> Self {
        let bsp = vec![vec![None; tws.len()]; twa.len()];
        Self { twa, tws, bsp }
    }

    /// The speed at row `i`, column `j`, if the cell holds one.
    pub fn get(&self, i: usize, j: usize) -> Option<f64> {
        self.bsp.get(i)?.get(j).copied().flatten()
    }

    /// Checks the grid's shape and ranges.
    pub fn validate(&self) -> Result<()> {
        validate_axis(&self.twa, "TWA", 0.0, 360.0)?;
        validate_axis(&self.tws, "TWS", 0.0, f64::MAX)?;
        if self.bsp.len() != self.twa.len() {
            return Err(CoreError::Invalid(format!(
                "a polar has {} TWA rows but {} rows of speeds",
                self.twa.len(),
                self.bsp.len()
            )));
        }
        for (i, row) in self.bsp.iter().enumerate() {
            if row.len() != self.tws.len() {
                return Err(CoreError::Invalid(format!(
                    "polar row {i} has {} speeds for {} TWS columns",
                    row.len(),
                    self.tws.len()
                )));
            }
            for cell in row.iter().flatten() {
                if !cell.is_finite() || *cell < 0.0 {
                    return Err(CoreError::Invalid(format!(
                        "polar row {i} holds the speed {cell}, which is not a speed"
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Checks that an axis is finite, in range, and strictly increasing.
pub fn validate_axis(axis: &[f64], name: &str, min: f64, max: f64) -> Result<()> {
    for value in axis {
        if !value.is_finite() || *value < min || *value > max {
            return Err(CoreError::Invalid(format!(
                "{name} value {value} is outside {min}..={max}"
            )));
        }
    }
    if axis.windows(2).any(|w| w[0] >= w[1]) {
        return Err(CoreError::Invalid(format!(
            "the {name} axis is not strictly increasing"
        )));
    }
    Ok(())
}

/// Which format an imported polar file was read as (spec.md 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolarFileFormat {
    /// Expedition `.txt`: rows of `TWS` then `TWA BSP` pairs.
    Expedition,
    /// Adrena `.pol`: a tab-separated TWA × TWS grid.
    Adrena,
    /// A `.csv` TWA × TWS grid, semicolon- or comma-separated.
    Csv,
    /// qtVlm `.csv`: a semicolon-separated TWA × TWS table.
    QtVlm,
    /// OpenCPN Weather Routing `.pol`: a semicolon-separated TWA × TWS table.
    OpenCpn,
    /// LuckGrib `.tsv`: a tab-separated TWA × TWS table.
    LuckGrib,
    /// WhirlwindMatrixPolar portable binary matrix.
    WhirlwindMatrixPolar,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_grid_has_the_shape_of_its_axes() {
        let grid = PolarGrid::empty(vec![0.0, 90.0, 180.0], vec![6.0, 12.0]);
        grid.validate().unwrap();
        assert_eq!(grid.get(1, 1), None);
        assert_eq!(grid.get(9, 9), None);
    }

    #[test]
    fn a_ragged_or_out_of_range_grid_is_invalid() {
        let mut grid = PolarGrid::empty(vec![0.0, 90.0], vec![6.0]);
        grid.bsp[1].push(Some(3.0));
        assert!(grid.validate().is_err());

        let grid = PolarGrid::empty(vec![0.0, 361.0], vec![6.0]);
        assert!(grid.validate().is_err());

        let grid = PolarGrid::empty(vec![90.0, 45.0], vec![6.0]);
        assert!(grid.validate().is_err());

        let mut grid = PolarGrid::empty(vec![90.0], vec![6.0]);
        grid.bsp[0][0] = Some(-1.0);
        assert!(grid.validate().is_err());
    }
}
