//! Compact, self-describing WhirlwindMatrixPolar binary format.
//!
//! Version 1 supports independent SOG, sampled-PDF, and Gaussian-PDF modes.
//! Version 2 files from the brief transition are also readable.

use pe_core::polar::{GaussianPdf, PolarGrid, WhirlwindMatrixPolar, validate_axis};
use thiserror::Error;

const MAGIC: &[u8; 8] = b"WWMPOLAR";
const VERSION: u16 = 1;
const TRANSITION_VERSION: u16 = 2;
const MODE_SOG: u16 = 1;
const MODE_PDF: u16 = 2;
const MODE_GAUSSIAN: u16 = 4;
const HEADER_LEN: usize = 8 + 2 + 2 + 6 * 4 + 2 * 4;
const MISSING: u16 = u16::MAX;
const LEVELS: f32 = u16::MAX as f32 - 1.0;
/// Bounds allocation from malformed files before decoding their compressed body.
pub const MAX_MATRIX_VALUES: usize = 16_000_000;
pub const MAX_MATRIX_FILE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum MatrixError {
    #[error("the WhirlwindMatrixPolar header is missing or invalid")]
    Header,
    #[error("the WhirlwindMatrixPolar version {0} is not supported")]
    Version(u16),
    #[error("the WhirlwindMatrixPolar modes are invalid")]
    Modes,
    #[error("the WhirlwindMatrixPolar data is too large")]
    TooLarge,
    #[error("the WhirlwindMatrixPolar wave axes must all be empty or all be populated")]
    WaveAxes,
    #[error("the WhirlwindMatrixPolar axis {0} is invalid")]
    Axis(&'static str),
    #[error("the WhirlwindMatrixPolar payload shape is invalid")]
    Shape,
    #[error("the WhirlwindMatrixPolar values are invalid")]
    Values,
    #[error("the WhirlwindMatrixPolar compressed payload is invalid")]
    Compression,
}

pub trait MatrixPolarExt {
    fn from_polar(polar: &PolarGrid) -> Result<Self, MatrixError>
    where
        Self: Sized;
    fn validate(&self) -> Result<(), MatrixError>;
    fn to_polar(&self) -> Result<PolarGrid, MatrixError>;
}

impl MatrixPolarExt for WhirlwindMatrixPolar {
    /// Construct the speed-only form of a conventional polar.
    fn from_polar(polar: &PolarGrid) -> Result<Self, MatrixError> {
        polar.validate().map_err(|_| MatrixError::Values)?;
        let sog = polar
            .bsp
            .iter()
            .flatten()
            .map(|value| value.map(|v| v as f32))
            .collect();
        let matrix = Self {
            twa_deg: polar.twa.iter().map(|v| *v as f32).collect(),
            tws_kn: polar.tws.iter().map(|v| *v as f32).collect(),
            wave_height_m: Vec::new(),
            wave_angle_deg: Vec::new(),
            wave_period_s: Vec::new(),
            sog_bins_kn: Vec::new(),
            sog: Some(sog),
            pdf: None,
            pdf_gaussians: None,
        };
        matrix.validate()?;
        Ok(matrix)
    }

    /// Validate dimensions, axis ordering and payload values.
    fn validate(&self) -> Result<(), MatrixError> {
        if self.twa_deg.is_empty() || self.tws_kn.is_empty() {
            return Err(MatrixError::Shape);
        }
        validate_f32_axis(&self.twa_deg, "TWA", 0.0, 360.0)?;
        validate_f32_axis(&self.tws_kn, "TWS", 0.0, 70.0)?;
        let wave_count = [
            self.wave_height_m.len(),
            self.wave_angle_deg.len(),
            self.wave_period_s.len(),
        ];
        if wave_count.contains(&0) && wave_count.iter().any(|n| *n != 0) {
            return Err(MatrixError::WaveAxes);
        }
        validate_f32_axis(&self.wave_height_m, "wave height", 0.0, 100.0)?;
        validate_f32_axis(&self.wave_angle_deg, "wave angle", 0.0, 360.0)?;
        validate_f32_axis(&self.wave_period_s, "wave period", 0.0, 1000.0)?;
        validate_f32_axis(&self.sog_bins_kn, "SOG bins", 0.0, 60.0)?;
        let wind_cells = checked_product(&[
            self.wave_height_m.len().max(1),
            self.wave_angle_deg.len().max(1),
            self.wave_period_s.len().max(1),
            self.twa_deg.len(),
            self.tws_kn.len(),
        ])?;
        let axis_values = self
            .twa_deg
            .len()
            .saturating_add(self.tws_kn.len())
            .saturating_add(self.wave_height_m.len())
            .saturating_add(self.wave_angle_deg.len())
            .saturating_add(self.wave_period_s.len())
            .saturating_add(self.sog_bins_kn.len());
        if axis_values > MAX_MATRIX_VALUES {
            return Err(MatrixError::TooLarge);
        }
        if self.sog.is_none() && self.pdf.is_none() && self.pdf_gaussians.is_none() {
            return Err(MatrixError::Modes);
        }
        if self.pdf.is_none() && self.pdf_gaussians.is_none() && !self.sog_bins_kn.is_empty() {
            return Err(MatrixError::Shape);
        }
        if let Some(sog) = &self.sog
            && (sog.len() != wind_cells
                || sog
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || *v < 0.0 || *v > 60.0))
        {
            return Err(MatrixError::Shape);
        }
        if let Some(pdf) = &self.pdf {
            let expected = wind_cells
                .checked_mul(self.sog_bins_kn.len())
                .ok_or(MatrixError::TooLarge)?;
            if self.sog_bins_kn.is_empty()
                || pdf.len() != expected
                || pdf.iter().any(|v| !v.is_finite() || *v < 0.0)
            {
                return Err(MatrixError::Shape);
            }
        }
        if let Some(gaussians) = &self.pdf_gaussians
            && ((self.pdf.is_none() && !self.sog_bins_kn.is_empty())
                || gaussians.len() != wind_cells
                || gaussians.iter().flatten().any(|g| {
                    !g.mean_kn.is_finite()
                        || !(0.0..=60.0).contains(&g.mean_kn)
                        || !g.std_dev_kn.is_finite()
                        || !(0.0..=60.0).contains(&g.std_dev_kn)
                }))
        {
            return Err(MatrixError::Shape);
        }
        Ok(())
    }

    /// A 2D speed projection for PolarExplorer's current blend model. It uses
    /// SOG data directly when wave-independent; otherwise it averages all
    /// wave slices. PDF-only files use the Gaussian expected speed.
    fn to_polar(&self) -> Result<PolarGrid, MatrixError> {
        self.validate()?;
        let wave_cells = self.wave_height_m.len().max(1)
            * self.wave_angle_deg.len().max(1)
            * self.wave_period_s.len().max(1);
        let twas = self.twa_deg.len();
        let twss = self.tws_kn.len();
        let mut bsp = vec![vec![None; twss]; twas];
        for (i, row) in bsp.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                let wind_index = i * twss + j;
                let value = if let Some(sog) = &self.sog {
                    let (sum, count) = (0..wave_cells).fold((0.0f64, 0usize), |(sum, count), w| {
                        match sog[w * twas * twss + wind_index] {
                            Some(v) => (sum + f64::from(v), count + 1),
                            None => (sum, count),
                        }
                    });
                    (count > 0).then_some((sum / count as f64) as f32)
                } else if let Some(gaussians) = &self.pdf_gaussians {
                    let (sum, count) = (0..wave_cells).fold((0.0f64, 0usize), |(sum, count), w| {
                        match gaussians[w * twas * twss + wind_index] {
                            Some(g) => (sum + f64::from(g.mean_kn), count + 1),
                            None => (sum, count),
                        }
                    });
                    (count > 0).then_some((sum / count as f64) as f32)
                } else if let Some(pdf) = &self.pdf {
                    let width = self.sog_bins_kn.len();
                    let (weighted, total) =
                        (0..wave_cells).fold((0.0f64, 0.0f64), |(sum, mass), w| {
                            let start = (w * twas * twss + wind_index) * width;
                            let slice = &pdf[start..start + width];
                            let p: f64 = slice.iter().map(|v| f64::from(*v)).sum();
                            let e: f64 = slice
                                .iter()
                                .zip(&self.sog_bins_kn)
                                .map(|(p, speed)| f64::from(*p) * f64::from(*speed))
                                .sum();
                            (sum + e, mass + p)
                        });
                    (total > 0.0).then_some((weighted / total) as f32)
                } else {
                    None
                };
                *cell = value.map(f64::from);
            }
        }
        Ok(PolarGrid {
            twa: self.twa_deg.iter().map(|v| f64::from(*v)).collect(),
            tws: self.tws_kn.iter().map(|v| f64::from(*v)).collect(),
            bsp,
        })
    }
}

/// Encode a matrix as v1 with any combination of its three data modes.
pub fn write(matrix: &WhirlwindMatrixPolar) -> Result<Vec<u8>, MatrixError> {
    matrix.validate()?;
    let mut flags = 0;
    if matrix.sog.is_some() {
        flags |= MODE_SOG;
    }
    if matrix.pdf.is_some() {
        flags |= MODE_PDF;
    }
    if matrix.pdf_gaussians.is_some() {
        flags |= MODE_GAUSSIAN;
    }
    let counts = [
        matrix.twa_deg.len(),
        matrix.tws_kn.len(),
        matrix.wave_height_m.len(),
        matrix.wave_angle_deg.len(),
        matrix.wave_period_s.len(),
        if matrix.pdf.is_some() {
            matrix.sog_bins_kn.len()
        } else {
            0
        },
    ];
    let mut axes = Vec::with_capacity(counts.iter().sum::<usize>() * 4);
    for axis in [
        &matrix.twa_deg,
        &matrix.tws_kn,
        &matrix.wave_height_m,
        &matrix.wave_angle_deg,
        &matrix.wave_period_s,
    ] {
        for value in axis {
            axes.extend_from_slice(&value.to_le_bytes());
        }
    }
    if matrix.pdf.is_some() {
        for value in &matrix.sog_bins_kn {
            axes.extend_from_slice(&value.to_le_bytes());
        }
    }

    let sog_values = matrix.sog.as_deref().unwrap_or(&[]);
    let sog_scale = quant_scale(sog_values.iter().flatten().copied().map(f64::from));
    let pdf_values = matrix.pdf.as_deref().unwrap_or(&[]);
    let pdf_scale = quant_scale(pdf_values.iter().copied().map(f64::from));
    let gaussian_values = matrix.pdf_gaussians.as_deref().unwrap_or(&[]);
    let raw_count = sog_values
        .len()
        .checked_add(pdf_values.len())
        .ok_or(MatrixError::TooLarge)?
        .checked_add(
            gaussian_values
                .len()
                .checked_mul(2)
                .ok_or(MatrixError::TooLarge)?,
        )
        .ok_or(MatrixError::TooLarge)?;
    if raw_count > MAX_MATRIX_VALUES {
        return Err(MatrixError::TooLarge);
    }
    let mut body = Vec::with_capacity(raw_count * 2);
    if matrix.sog.is_some() {
        for value in sog_values {
            body.extend_from_slice(&quantize(value.map(f64::from), sog_scale).to_le_bytes());
        }
    }
    if matrix.pdf.is_some() {
        for value in pdf_values {
            body.extend_from_slice(&quantize(Some(f64::from(*value)), pdf_scale).to_le_bytes());
        }
    }
    if let Some(gaussians) = matrix.pdf_gaussians.as_deref() {
        for gaussian in gaussians {
            if let Some(g) = gaussian {
                body.extend_from_slice(&quantize_speed(g.mean_kn, 60.0).to_le_bytes());
                body.extend_from_slice(&quantize_speed(g.std_dev_kn, 60.0).to_le_bytes());
            } else {
                body.extend_from_slice(&MISSING.to_le_bytes());
                body.extend_from_slice(&MISSING.to_le_bytes());
            }
        }
    }
    let compressed = lz4_flex::block::compress_prepend_size(&body);
    let total = HEADER_LEN
        .checked_add(axes.len())
        .and_then(|n| n.checked_add(compressed.len()))
        .ok_or(MatrixError::TooLarge)?;
    if total > MAX_MATRIX_FILE_BYTES {
        return Err(MatrixError::TooLarge);
    }
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    for count in counts {
        out.extend_from_slice(
            &u32::try_from(count)
                .map_err(|_| MatrixError::TooLarge)?
                .to_le_bytes(),
        );
    }
    out.extend_from_slice(&sog_scale.to_le_bytes());
    out.extend_from_slice(&pdf_scale.to_le_bytes());
    out.extend_from_slice(&axes);
    out.extend_from_slice(&compressed);
    Ok(out)
}

/// Decode and validate v1 files and the brief v2 transition format.
pub fn read(bytes: &[u8]) -> Result<WhirlwindMatrixPolar, MatrixError> {
    if bytes.len() > MAX_MATRIX_FILE_BYTES || bytes.len() < HEADER_LEN + 4 || &bytes[..8] != MAGIC {
        return Err(MatrixError::Header);
    }
    let version = read_u16(bytes, 8)?;
    if version != VERSION && version != TRANSITION_VERSION {
        return Err(MatrixError::Version(version));
    }
    let flags = read_u16(bytes, 10)?;
    let known_modes = if version == TRANSITION_VERSION {
        MODE_SOG | MODE_PDF
    } else {
        MODE_SOG | MODE_PDF | MODE_GAUSSIAN
    };
    if flags == 0 || flags & !known_modes != 0 {
        return Err(MatrixError::Modes);
    }
    let counts: [usize; 6] =
        std::array::from_fn(|i| read_u32(bytes, 12 + i * 4).unwrap_or(u32::MAX) as usize);
    if counts.contains(&(u32::MAX as usize)) {
        return Err(MatrixError::Header);
    }
    let sog_scale = read_f32(bytes, 36)?;
    let pdf_scale = read_f32(bytes, 40)?;
    if !sog_scale.is_finite() || sog_scale < 0.0 || !pdf_scale.is_finite() || pdf_scale < 0.0 {
        return Err(MatrixError::Values);
    }
    let axis_bytes = counts
        .iter()
        .try_fold(0usize, |sum, n| sum.checked_add(n.checked_mul(4)?))
        .ok_or(MatrixError::TooLarge)?;
    let axis_end = HEADER_LEN
        .checked_add(axis_bytes)
        .ok_or(MatrixError::TooLarge)?;
    if axis_end + 4 > bytes.len() {
        return Err(MatrixError::Header);
    }
    let mut pos = HEADER_LEN;
    let mut axes = Vec::with_capacity(6);
    for count in counts {
        let mut axis = Vec::with_capacity(count);
        for _ in 0..count {
            axis.push(read_f32(bytes, pos)?);
            pos += 4;
        }
        axes.push(axis);
    }
    let h = axes.remove(0);
    let a = axes.remove(0);
    let p = axes.remove(0);
    let th = axes.remove(0);
    let ts = axes.remove(0);
    let speed_bins = axes.remove(0);
    let wind_cells = checked_product(&[
        h.len().max(1),
        a.len().max(1),
        p.len().max(1),
        th.len(),
        ts.len(),
    ])?;
    let sog_count = if flags & MODE_SOG != 0 { wind_cells } else { 0 };
    let histogram_count = if version == VERSION && flags & MODE_PDF != 0 {
        wind_cells
            .checked_mul(speed_bins.len())
            .ok_or(MatrixError::TooLarge)?
    } else {
        0
    };
    let gaussian_mode = if version == TRANSITION_VERSION {
        flags & MODE_PDF != 0
    } else {
        flags & MODE_GAUSSIAN != 0
    };
    let gaussian_count = if gaussian_mode {
        wind_cells.checked_mul(2).ok_or(MatrixError::TooLarge)?
    } else {
        0
    };
    let raw_count = sog_count
        .checked_add(histogram_count)
        .and_then(|n| n.checked_add(gaussian_count))
        .ok_or(MatrixError::TooLarge)?;
    if raw_count > MAX_MATRIX_VALUES {
        return Err(MatrixError::TooLarge);
    }
    let expected = raw_count.checked_mul(2).ok_or(MatrixError::TooLarge)?;
    if read_u32(bytes, axis_end)? as usize != expected {
        return Err(MatrixError::Shape);
    }
    let raw = lz4_flex::block::decompress_size_prepended(&bytes[axis_end..])
        .map_err(|_| MatrixError::Compression)?;
    if raw.len() != expected {
        return Err(MatrixError::Shape);
    }
    let mut cursor = 0usize;
    let mut sog = None;
    if flags & MODE_SOG != 0 {
        let mut values = Vec::with_capacity(sog_count);
        for _ in 0..sog_count {
            let q = read_u16(&raw, cursor)?;
            cursor += 2;
            values.push((q != MISSING).then_some(q as f32 * sog_scale / LEVELS));
        }
        sog = Some(values);
    }
    let mut pdf = None;
    let mut pdf_gaussians = None;
    if version == VERSION && flags & MODE_PDF != 0 {
        let mut values = Vec::with_capacity(histogram_count);
        for _ in 0..histogram_count {
            let q = read_u16(&raw, cursor)?;
            cursor += 2;
            if q == MISSING {
                return Err(MatrixError::Values);
            }
            values.push(q as f32 * pdf_scale / LEVELS);
        }
        pdf = Some(values);
    }
    if gaussian_mode {
        let scale = if version == TRANSITION_VERSION {
            if !speed_bins.is_empty() || pdf_scale <= 0.0 || pdf_scale > 60.0 {
                return Err(MatrixError::Values);
            }
            pdf_scale
        } else {
            60.0
        };
        let mut values = Vec::with_capacity(wind_cells);
        for _ in 0..wind_cells {
            let mean = read_u16(&raw, cursor)?;
            let std_dev = read_u16(&raw, cursor + 2)?;
            cursor += 4;
            if mean == MISSING && std_dev == MISSING {
                values.push(None);
            } else if mean == MISSING || std_dev == MISSING {
                return Err(MatrixError::Values);
            } else {
                values.push(Some(GaussianPdf {
                    mean_kn: mean as f32 * scale / LEVELS,
                    std_dev_kn: std_dev as f32 * scale / LEVELS,
                }));
            }
        }
        pdf_gaussians = Some(values);
    }
    let matrix = WhirlwindMatrixPolar {
        twa_deg: th,
        tws_kn: ts,
        wave_height_m: h,
        wave_angle_deg: a,
        wave_period_s: p,
        sog_bins_kn: speed_bins,
        sog,
        pdf,
        pdf_gaussians,
    };
    matrix.validate()?;
    Ok(matrix)
}

fn validate_f32_axis(
    axis: &[f32],
    name: &'static str,
    min: f32,
    max: f32,
) -> Result<(), MatrixError> {
    let values: Vec<f64> = axis.iter().map(|v| f64::from(*v)).collect();
    validate_axis(&values, name, f64::from(min), f64::from(max))
        .map_err(|_| MatrixError::Axis(name))
}

fn checked_product(values: &[usize]) -> Result<usize, MatrixError> {
    let product = values
        .iter()
        .try_fold(1usize, |n, value| n.checked_mul(*value))
        .ok_or(MatrixError::TooLarge)?;
    if product > MAX_MATRIX_VALUES {
        return Err(MatrixError::TooLarge);
    }
    Ok(product)
}

fn quant_scale(values: impl Iterator<Item = f64>) -> f32 {
    let maximum = values.fold(0.0f64, f64::max);
    if maximum == 0.0 {
        1.0
    } else {
        let scale = (maximum / f64::from(LEVELS)) as f32;
        if scale == 0.0 { maximum as f32 } else { scale }
    }
}

fn quantize(value: Option<f64>, scale: f32) -> u16 {
    let Some(value) = value else {
        return MISSING;
    };
    if value == 0.0 {
        return 0;
    }
    ((value / f64::from(scale))
        .round()
        .clamp(0.0, f64::from(LEVELS))) as u16
}

fn quantize_speed(value: f32, max_speed: f32) -> u16 {
    ((f64::from(value) / f64::from(max_speed)) * f64::from(LEVELS))
        .round()
        .clamp(0.0, f64::from(LEVELS)) as u16
}

fn read_u16(bytes: &[u8], pos: usize) -> Result<u16, MatrixError> {
    let bytes: [u8; 2] = bytes
        .get(pos..pos + 2)
        .ok_or(MatrixError::Header)?
        .try_into()
        .map_err(|_| MatrixError::Header)?;
    Ok(u16::from_le_bytes(bytes))
}
fn read_u32(bytes: &[u8], pos: usize) -> Result<u32, MatrixError> {
    let bytes: [u8; 4] = bytes
        .get(pos..pos + 4)
        .ok_or(MatrixError::Header)?
        .try_into()
        .map_err(|_| MatrixError::Header)?;
    Ok(u32::from_le_bytes(bytes))
}
fn read_f32(bytes: &[u8], pos: usize) -> Result<f32, MatrixError> {
    Ok(f32::from_le_bytes(read_u32(bytes, pos)?.to_le_bytes()))
}
