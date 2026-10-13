//! Polar grids and the math over them.
//!
//! The polar grid type and its interpolation, the Expedition and Adrena
//! readers and writers, binning track samples into polar segments, blending
//! weighted sources, and the comparison surfaces (spec.md 6, 11, 12).
//!
//! The grid itself is plain data in `pe_core` ([`Polar`]), because an
//! imported polar is part of the document; everything it *does* lives here.
//!
//! The blend is derived, never stored (invariant 2): nothing here writes
//! project data, and export always recomputes from sources plus overlays.
//! Export bytes are pinned by golden files, because they must be identical on
//! every platform (invariant 5).

pub mod blend;
pub mod compare;
pub mod edit;
pub mod export;
pub mod format;
pub mod grid;
pub mod matrix;
pub mod orc;
pub mod segment;
pub mod source;
pub mod spline;

mod build;
mod expedition;
mod table;

pub use blend::{Blend, BlendOptions, BlendSource, CellOrigin, Confidence, Coverage, blend};
pub use compare::{CellClass, CompareError, Comparison, Faster, Region, compare};
pub use export::{ExportProblem, export};
pub use format::{PolarError, Reason, detect, read, read_as, write};
pub use grid::{cell_count, fold_twa, interpolate, interpolate_from, resample};
pub use matrix::MatrixPolarExt;
pub use orc::vpp_to_polar;
pub use pe_core::polar::{GaussianPdf, WhirlwindMatrixPolar};
pub use pe_core::polar::{PolarFileFormat, PolarGrid as Polar};
pub use segment::{Segment, bin};
pub use source::{blend_input, edited_polar, source_polar, with_overlay};

/// The fastest a boat may go in an imported polar, knots (spec.md 6).
/// Anything faster is a format error, not a polar. The wind speed axis has
/// its own, higher bound: [`MAX_TWS_KN`].
pub const MAX_SPEED_KN: f64 = 60.0;

/// The fastest a wind speed axis value may be in an imported polar, knots
/// (spec.md 6). Real Adrena/ORC-style grids carry a TWS axis out to 70 kn
/// (a gale, not a boat speed), which [`MAX_SPEED_KN`] alone would refuse.
pub const MAX_TWS_KN: f64 = 70.0;
