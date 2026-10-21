//! The archive kind PolarExplorer reads (spec.md 7.5), and the cell
//! arithmetic the sampler shares with the Whirlwind reader.
//!
//! Which archive a value came from is its `ArchiveRef::identity`; the dataset
//! only says what kind of store it is.

use std::collections::BTreeMap;

use crate::grid::Stencil;

/// A kind of reanalysis store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dataset {
    /// A Hindsight Sync archive: wind, waves and tidal surface current.
    Hindsight,
}

impl Dataset {
    /// A short name for the kind.
    pub fn id(self) -> &'static str {
        match self {
            Self::Hindsight => "hindsight",
        }
    }

    /// Whether this dataset's current includes tides. The Hindsight current
    /// is tidal; the flag stays per dataset so an archive without tides can
    /// be marked, and filtered, should one be added.
    pub fn has_tide(self) -> Option<bool> {
        Some(true)
    }
}

/// One grid cell of a variable at one time step: `(step, lat index, lon
/// index)`.
pub type Cell = (u64, usize, usize);

/// The four cells of a stencil at one step, in [`Stencil::interpolate`]'s
/// corner order.
pub fn corner_cells(step: u64, stencil: &Stencil) -> [Cell; 4] {
    [
        (step, stencil.lat[0], stencil.lon[0]),
        (step, stencil.lat[0], stencil.lon[1]),
        (step, stencil.lat[1], stencil.lon[0]),
        (step, stencil.lat[1], stencil.lon[1]),
    ]
}

/// The four corner values of a stencil at one step, NaN where unread.
pub fn corners_of(values: &BTreeMap<Cell, f32>, step: u64, stencil: &Stencil) -> [f32; 4] {
    corner_cells(step, stencil).map(|cell| values.get(&cell).copied().unwrap_or(f32::NAN))
}

/// Linear interpolation in time between the values at the steps either side
/// (weight `w` on the later). One side missing (an unwritten hour): the
/// nearer side only, and nothing if the missing side is the nearer.
pub fn lerp_time(first: Option<f64>, second: Option<f64>, w: f64) -> Option<f64> {
    match (first, second) {
        (Some(x), Some(y)) => Some(x + (y - x) * w),
        (Some(x), None) if w <= 0.5 => Some(x),
        (None, Some(y)) if w >= 0.5 => Some(y),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hindsight_dataset_is_named_and_tidal() {
        assert_eq!(Dataset::Hindsight.id(), "hindsight");
        assert_eq!(Dataset::Hindsight.has_tide(), Some(true));
    }
}
