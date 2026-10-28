//! Wind-only projection for consumers that cannot evaluate wave-conditioned polars.
use super::{MatrixError, MatrixPolarExt};
use pe_core::polar::{PolarGrid, WhirlwindMatrixPolar};

/// Construct a conventional symmetric polar at the original wind-speed nodes.
/// Fold both tacks to 0..180 degrees and pool available wave/tack cells equally.
/// Sampled PDFs (normalized per cell) take precedence over Gaussian PDFs, then
/// deterministic SOG. Distribution cells use the pooled inverse CDF at 0.9;
/// SOG-only cells use the linearly interpolated empirical 90th percentile.
/// Missing cells stay absent; no speed or wind-axis extrapolation is introduced.
pub fn to_p90_polar(matrix: &WhirlwindMatrixPolar) -> Result<PolarGrid, MatrixError> {
    matrix.validate()?;
    let folded: Vec<f64> = matrix
        .twa_deg
        .iter()
        .map(|a| {
            let a = f64::from(*a).rem_euclid(360.0);
            a.min(360.0 - a)
        })
        .collect();
    let mut twa = folded.clone();
    twa.sort_by(f64::total_cmp);
    twa.dedup();
    let winds = matrix.tws_kn.len();
    let cells = winds * matrix.twa_deg.len();
    let waves = matrix.wave_height_count().max(1)
        * (matrix.wave_direction_count as usize).max(1)
        * matrix.wave_period_count().max(1);
    let mut bsp = vec![vec![None; winds]; twa.len()];
    for (angle, row) in twa.iter().zip(&mut bsp) {
        for (wind, output) in row.iter_mut().enumerate() {
            let mut distributions = Vec::new();
            for (a, _) in folded
                .iter()
                .enumerate()
                .filter(|(_, value)| *value == angle)
            {
                for wave in 0..waves {
                    let i = wave * cells + a * winds + wind;
                    let pdf = matrix.pdf.as_ref().and_then(|pdf| {
                        let width = matrix.sog_bins_kn.len();
                        let weights = &pdf[i * width..(i + 1) * width];
                        let mass: f64 = weights.iter().map(|v| f64::from(*v)).sum();
                        (mass > 0.).then_some(Distribution::Sampled(
                            weights,
                            &matrix.sog_bins_kn,
                            mass,
                        ))
                    });
                    let distribution = pdf
                        .or_else(|| {
                            matrix
                                .pdf_gaussians
                                .as_ref()
                                .and_then(|values| values[i])
                                .map(|g| {
                                    Distribution::Gaussian(
                                        f64::from(g.mean_kn),
                                        f64::from(g.std_dev_kn),
                                    )
                                })
                        })
                        .or_else(|| {
                            matrix
                                .sog
                                .as_ref()
                                .and_then(|values| values[i])
                                .map(|v| Distribution::Point(f64::from(v)))
                        });
                    if let Some(distribution) = distribution {
                        distributions.push(distribution);
                    }
                }
            }
            *output = percentile(&distributions);
        }
    }
    let result = PolarGrid {
        twa,
        tws: matrix.tws_kn.iter().map(|v| f64::from(*v)).collect(),
        bsp,
    };
    result.validate().map_err(|_| MatrixError::Values)?;
    Ok(result)
}

enum Distribution<'a> {
    Point(f64),
    Gaussian(f64, f64),
    Sampled(&'a [f32], &'a [f32], f64),
}
impl Distribution<'_> {
    fn upper(&self) -> f64 {
        match self {
            Self::Point(v) => *v,
            Self::Gaussian(mean, sigma) => mean + 10. * sigma,
            Self::Sampled(_, bins, _) => bins.last().map_or(0., |&v| f64::from(v)),
        }
    }
    fn cdf(&self, speed: f64) -> f64 {
        match self {
            Self::Point(v) => {
                if speed >= *v {
                    1.
                } else {
                    0.
                }
            }
            Self::Gaussian(mean, sigma) if *sigma == 0. => {
                if speed >= *mean {
                    1.
                } else {
                    0.
                }
            }
            Self::Gaussian(mean, sigma) => normal_cdf((speed - mean) / sigma),
            Self::Sampled(weights, bins, mass) => {
                weights
                    .iter()
                    .zip(*bins)
                    .take_while(|(_, bin)| f64::from(**bin) <= speed)
                    .map(|(weight, _)| f64::from(*weight))
                    .sum::<f64>()
                    / mass
            }
        }
    }
}
fn percentile(values: &[Distribution<'_>]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let points: Option<Vec<_>> = values
        .iter()
        .map(|d| match d {
            Distribution::Point(v) => Some(*v),
            _ => None,
        })
        .collect();
    if let Some(mut points) = points {
        points.sort_by(f64::total_cmp);
        let rank = 0.9 * (points.len() - 1) as f64;
        let lo = rank.floor() as usize;
        let hi = rank.ceil() as usize;
        return Some(points[lo] + (points[hi] - points[lo]) * rank.fract());
    }
    let mut low = 0.;
    let mut high = values.iter().map(Distribution::upper).fold(0., f64::max);
    for _ in 0..64 {
        let mid = (low + high) / 2.;
        let cdf = values.iter().map(|d| d.cdf(mid)).sum::<f64>() / values.len() as f64;
        if cdf < 0.9 {
            low = mid;
        } else {
            high = mid;
        }
    }
    Some(high)
}

// Standard normal CDF; absolute error < 7.5e-8, with no platform libm dependency.
fn normal_cdf(x: f64) -> f64 {
    let t = 1. / (1. + 0.2316419 * x.abs());
    let tail = (-0.5 * x * x).exp() / (2. * std::f64::consts::PI).sqrt()
        * t
        * (0.319381530
            + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
    if x >= 0. { 1. - tail } else { tail }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pe_core::polar::GaussianPdf;
    fn matrix() -> WhirlwindMatrixPolar {
        WhirlwindMatrixPolar {
            twa_deg: vec![0., 90., 180., 270., 360.],
            tws_kn: vec![10., 20.],
            wave_height_cutoffs_m: vec![],
            wave_direction_count: 2,
            wave_period_cutoffs_s: vec![],
            sog_bins_kn: vec![],
            sog: Some(
                (0..20)
                    .map(|i| Some(if i < 10 { 4. } else { 8. }))
                    .collect(),
            ),
            pdf: None,
            pdf_gaussians: None,
        }
    }
    #[test]
    fn folds_tacks_and_uses_p90_without_changing_wind_nodes_or_missing_cells() {
        let mut m = matrix();
        for i in [4, 14] {
            m.sog.as_mut().unwrap()[i] = None;
        }
        let grid = to_p90_polar(&m).unwrap();
        assert_eq!(grid.twa, [0., 90., 180.]);
        assert_eq!(grid.tws, [10., 20.]);
        assert_eq!(grid.bsp[0], [Some(8.), Some(8.)]);
        assert_eq!(grid.bsp[2], [None, Some(7.6)]);
        assert_eq!(m.twa_deg.len(), 5);
    }
    #[test]
    fn sampled_distributions_are_normalized_per_wave_and_take_precedence() {
        let mut m = matrix();
        m.sog_bins_kn = vec![3., 9., 15.];
        m.pdf = Some(
            (0..20)
                .flat_map(|i| {
                    if i < 10 {
                        [95., 0., 5.]
                    } else {
                        [0.05, 0.80, 0.15]
                    }
                })
                .collect(),
        );
        let grid = to_p90_polar(&m).unwrap();
        assert!((grid.bsp[1][0].unwrap() - 15.).abs() < 1e-6);
        m.pdf.as_mut().unwrap().fill(0.);
        assert_eq!(to_p90_polar(&m).unwrap().bsp[2][0], Some(7.6));
    }
    #[test]
    fn gaussian_quantiles_and_degenerate_distributions() {
        let mut m = matrix();
        m.sog = None;
        m.pdf_gaussians = Some(vec![
            Some(GaussianPdf {
                mean_kn: 6.,
                std_dev_kn: 2.
            });
            20
        ]);
        assert!((to_p90_polar(&m).unwrap().bsp[1][0].unwrap() - 8.56310313).abs() < 1e-5);
        m.pdf_gaussians.as_mut().unwrap().fill(Some(GaussianPdf {
            mean_kn: 6.,
            std_dev_kn: 0.,
        }));
        assert_eq!(to_p90_polar(&m).unwrap().bsp[1][0], Some(6.));
        m.pdf_gaussians.as_mut().unwrap().fill(None);
        assert_eq!(to_p90_polar(&m).unwrap().bsp[1][0], None);
        m.tws_kn.reverse();
        assert!(to_p90_polar(&m).is_err());
    }
}
