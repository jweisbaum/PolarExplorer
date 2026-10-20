//! The archives PolarExplorer reads (spec.md 7.5), and the cell arithmetic
//! the sampler shares with the Whirlwind reader.
//!
//! Every archive is the same Whirlwind Hindsight v3 store, published at one
//! of three endpoints; each is its own dataset so a sample records where its
//! values came from, and so cached chunks are never shared between them.

use std::collections::BTreeMap;

use crate::grid::Stencil;

/// A dataset: one Zarr store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dataset {
    /// Whirlwind Hindsight v3 wind, waves and tidal surface current.
    WhirlwindHindsight,
    /// The same combined-parameter archive on Cloudflare R2.
    WhirlwindR2,
    /// The same combined-parameter archive on Tigris.
    WhirlwindTigris,
}

impl Dataset {
    /// Every dataset.
    pub const ALL: [Self; 3] = [
        Self::WhirlwindHindsight,
        Self::WhirlwindR2,
        Self::WhirlwindTigris,
    ];

    /// The name recorded on each sample (`DatasetRecord::name`), and the
    /// chunk cache namespace.
    pub fn id(self) -> &'static str {
        match self {
            Self::WhirlwindHindsight => "whirlwind-hindsight",
            Self::WhirlwindR2 => "whirlwind-hindsight-r2",
            Self::WhirlwindTigris => "whirlwind-hindsight-tigris",
        }
    }

    /// Where it is published.
    pub fn url(self) -> &'static str {
        match self {
            Self::WhirlwindHindsight => crate::whirlwind::URL,
            Self::WhirlwindR2 => crate::whirlwind::Source::R2.url(),
            Self::WhirlwindTigris => crate::whirlwind::Source::Tigris.url(),
        }
    }

    /// The version recorded on each sample.
    pub fn version(self) -> &'static str {
        "hindsight-v3"
    }

    /// Whether this dataset's current includes tides. The Whirlwind
    /// current is tidal; the flag stays per dataset so an archive without
    /// tides can be marked, and filtered, should one be added.
    pub fn has_tide(self) -> Option<bool> {
        Some(true)
    }

    /// The dataset whose [`Self::id`] this is.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.id() == id)
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
    fn dataset_ids_are_distinct_and_urls_are_https() {
        let ids: std::collections::BTreeSet<_> = Dataset::ALL.iter().map(|d| d.id()).collect();
        assert_eq!(ids.len(), Dataset::ALL.len());
        for d in Dataset::ALL {
            assert!(d.url().starts_with("https://"), "{d:?}");
            assert!(!d.version().is_empty());
            assert_eq!(Dataset::from_id(d.id()), Some(d));
        }
    }
}
