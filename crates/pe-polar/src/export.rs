//! Export: a polar written as a file another program reads (spec.md 6, 12).
//!
//! The writers ([`crate::write`]) round axis values to two decimals, so two
//! distinct values closer than that — 42.001° and 42.004° — would be written
//! as one, and the file would hold the same angle twice, which every reader
//! (this one included) refuses (plan.md M14, carried from M4). Export checks
//! that first and refuses with the two values named, rather than writing a
//! file that does not read back. It also refuses a polar with nothing in it,
//! and one faster than a polar file may hold.
//!
//! Byte-deterministic (invariant 5): the text is the writer's, which depends
//! on nothing but the grid.

use pe_core::polar::{PolarFileFormat, PolarGrid};
use thiserror::Error;

use crate::format::axis_text;
use crate::{MAX_SPEED_KN, MAX_TWS_KN};

/// Which axis a problem is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// True wind angle.
    Twa,
    /// True wind speed.
    Tws,
}

impl Axis {
    /// A stable name for the interface.
    pub fn code(self) -> &'static str {
        match self {
            Self::Twa => "twa",
            Self::Tws => "tws",
        }
    }
}

/// Why a polar cannot be exported.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ExportProblem {
    /// No cell holds a value off the 0° row (which is 0 kn by definition).
    #[error("the polar has no boat speeds to export")]
    Empty,
    /// Two axis values are written the same with two decimals.
    #[error("{} values {first} and {second} would both be written as {written}", axis.code().to_uppercase())]
    AxisCollision {
        /// The axis.
        axis: Axis,
        /// The first value.
        first: f64,
        /// The next one.
        second: f64,
        /// What both would be written as.
        written: String,
    },
    /// An axis value outside what a polar file holds, or not increasing.
    #[error("{} value {value} cannot be written in a polar file", axis.code().to_uppercase())]
    OutOfRange {
        /// The axis.
        axis: Axis,
        /// The value.
        value: f64,
    },
    /// A boat speed above [`MAX_SPEED_KN`].
    #[error("the boat speed {bsp} kn at {twa}° and {tws} kn is faster than a polar holds")]
    TooFast {
        /// Where: TWA, degrees.
        twa: f64,
        /// Where: TWS, knots.
        tws: f64,
        /// The speed.
        bsp: f64,
    },
    /// The format is binary and must use [`crate::matrix::write`].
    #[error("WhirlwindMatrixPolar must be written with its binary matrix writer")]
    BinaryMatrix,
}

impl ExportProblem {
    /// A stable identifier for the interface to translate by.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::AxisCollision { .. } => "axis-collision",
            Self::OutOfRange { .. } => "out-of-range",
            Self::TooFast { .. } => "too-fast",
            Self::BinaryMatrix => "binary-matrix",
        }
    }
}

fn check_axis(values: &[f64], axis: Axis, max: f64) -> Result<(), ExportProblem> {
    for value in values {
        if !value.is_finite() || *value < 0.0 || *value > max {
            return Err(ExportProblem::OutOfRange {
                axis,
                value: *value,
            });
        }
    }
    for pair in values.windows(2) {
        let (a, b) = (axis_text(pair[0]), axis_text(pair[1]));
        if a == b {
            return Err(ExportProblem::AxisCollision {
                axis,
                first: pair[0],
                second: pair[1],
                written: a,
            });
        }
        // Written values must still be increasing, as the readers sort them.
        let (wa, wb) = (a.parse::<f64>(), b.parse::<f64>());
        if let (Ok(wa), Ok(wb)) = (wa, wb)
            && wa >= wb
        {
            return Err(ExportProblem::OutOfRange {
                axis,
                value: pair[1],
            });
        }
    }
    Ok(())
}

/// Checks a polar can be written and read back as the same grid.
pub fn check(polar: &PolarGrid) -> Result<(), ExportProblem> {
    check_axis(&polar.twa, Axis::Twa, 360.0)?;
    check_axis(&polar.tws, Axis::Tws, MAX_TWS_KN)?;
    for (i, twa) in polar.twa.iter().enumerate() {
        for (j, tws) in polar.tws.iter().enumerate() {
            let Some(bsp) = polar.get(i, j) else {
                continue;
            };
            // Rounded as written: 60.004 is written 60.00, which reads.
            if !bsp.is_finite() || bsp < 0.0 || (bsp * 100.0).round() / 100.0 > MAX_SPEED_KN {
                return Err(ExportProblem::TooFast {
                    twa: *twa,
                    tws: *tws,
                    bsp,
                });
            }
        }
    }
    // The 0° row is 0 kn by definition (spec.md 12.3): a polar with nothing
    // else has nothing to say.
    if crate::blend::has_value_off_zero_row(polar) {
        Ok(())
    } else {
        Err(ExportProblem::Empty)
    }
}

/// The file text of `polar` in `format`, after [`check`].
pub fn export(format: PolarFileFormat, polar: &PolarGrid) -> Result<String, ExportProblem> {
    if format == PolarFileFormat::WhirlwindMatrixPolar {
        return Err(ExportProblem::BinaryMatrix);
    }
    check(polar)?;
    Ok(crate::write(format, polar))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polar(twa: Vec<f64>, tws: Vec<f64>) -> PolarGrid {
        let mut grid = PolarGrid::empty(twa, tws);
        for row in &mut grid.bsp {
            for cell in row.iter_mut() {
                *cell = Some(5.0);
            }
        }
        grid
    }

    #[test]
    fn values_written_the_same_are_refused_by_name() {
        let problem = check(&polar(vec![42.001, 42.004, 90.0], vec![10.0])).unwrap_err();
        assert_eq!(
            problem,
            ExportProblem::AxisCollision {
                axis: Axis::Twa,
                first: 42.001,
                second: 42.004,
                written: "42".to_owned()
            }
        );
        assert_eq!(problem.code(), "axis-collision");
        assert_eq!(
            problem.to_string(),
            "TWA values 42.001 and 42.004 would both be written as 42"
        );
        let problem = check(&polar(vec![90.0], vec![10.001, 10.004])).unwrap_err();
        assert!(matches!(
            problem,
            ExportProblem::AxisCollision {
                axis: Axis::Tws,
                ..
            }
        ));
        check(&polar(vec![42.0, 42.01], vec![10.0])).unwrap();
    }

    #[test]
    fn an_empty_or_impossible_polar_is_refused() {
        assert_eq!(
            check(&PolarGrid::empty(vec![90.0], vec![10.0])),
            Err(ExportProblem::Empty)
        );
        // A 0° row of zeros alone is still nothing.
        let mut zeros = PolarGrid::empty(vec![0.0, 90.0], vec![10.0]);
        zeros.bsp[0][0] = Some(0.0);
        assert_eq!(check(&zeros), Err(ExportProblem::Empty));
        let mut fast = polar(vec![90.0], vec![10.0]);
        fast.bsp[0][0] = Some(61.0);
        assert_eq!(check(&fast).unwrap_err().code(), "too-fast");
        assert_eq!(
            check(&polar(vec![90.0], vec![71.0])).unwrap_err().code(),
            "out-of-range"
        );
    }
}
