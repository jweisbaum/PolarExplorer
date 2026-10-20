//! The environment at track positions (spec.md 7.5).
//!
//! A [`Provider`] answers, for a batch of positions, the wind, waves and
//! current there. [`Reanalysis`] is the real one: it reads the Whirlwind
//! Hindsight archive. The job system in `pe-app` talks to the trait, so its
//! cancel-and-resume behaviour is tested against a fake without the network.
//!
//! **Each position's answer depends only on that position** (its time,
//! latitude and longitude) and the options: never on which other positions
//! share its batch. That is what lets a cancelled fetch resume to exactly
//! the result an uninterrupted one gives.
//!
//! Interpolation (spec.md 7.5): bilinear in space, linear in time. Wind and
//! current interpolate their u and v; wave direction interpolates as a unit
//! vector. Land (NaN) corners are left out of the stencil; if all are land
//! the value is missing. Values stay in archive units (m/s, m, degrees);
//! `pe-app` converts to knots once, on ingest into the project.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::dataset::{Cell, Dataset, corners_of, lerp_time};
use crate::error::Result;
use crate::grid::Stencil;
use crate::memory::BlockCache;
use crate::store::TimeAxis;
use crate::whirlwind::Whirlwind;

/// One track position to sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// UTC epoch seconds.
    pub t: i64,
    /// Latitude, degrees.
    pub lat: f64,
    /// Longitude, degrees, any convention.
    pub lon: f64,
}

/// How finely wind, waves and current are sampled in time (D19).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Interval {
    /// Every hour of ERA5 (the default).
    #[default]
    Hourly,
}

impl Interval {
    /// The spacing, seconds.
    pub fn seconds(self) -> i64 {
        match self {
            Self::Hourly => 3600,
        }
    }

    /// The interval of this spacing, if it is one.
    pub fn from_seconds(seconds: i64) -> Option<Self> {
        [Self::Hourly].into_iter().find(|i| i.seconds() == seconds)
    }
}

/// Which of wind, waves and current a read asks for. A fetch for a track
/// asks for all three; a GRIB export only for what the user ticked, and
/// for the current apart from the rest (spec.md 7.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parts {
    /// 10 m wind.
    pub wind: bool,
    /// Wave height and direction.
    pub waves: bool,
    /// Surface current.
    pub current: bool,
}

impl Parts {
    /// Everything: what a track's fetch reads.
    pub const ALL: Self = Self {
        wind: true,
        waves: true,
        current: true,
    };
}

impl Default for Parts {
    fn default() -> Self {
        Self::ALL
    }
}

/// What a fetch reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Time sampling.
    pub interval: Interval,
    /// The project's Stokes drift choice, kept so tracks fetched with
    /// different choices are sampled apart (Q3).
    pub stokes_drift: bool,
    /// What is read; the rest is left `None`.
    pub parts: Parts,
}

/// A horizontal vector, m/s: `u` east, `v` north. Wind "from" and current
/// "toward" are worked out by the caller (CLAUDE.md conventions).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector {
    /// Eastward, m/s.
    pub u: f64,
    /// Northward, m/s.
    pub v: f64,
    /// Where it came from.
    pub dataset: Dataset,
}

/// Waves at a position; height and direction may each be missing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Waves {
    /// Significant wave height, metres.
    pub hs: Option<f64>,
    /// Mean wave period, seconds.
    pub period_s: Option<f64>,
    /// Mean wave direction, "from", degrees in [0, 360).
    pub from: Option<f64>,
    /// Where they came from.
    pub dataset: Dataset,
}

/// Everything found at one position.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EnvPoint {
    /// 10 m wind.
    pub wind: Option<Vector>,
    /// Waves.
    pub waves: Option<Waves>,
    /// Surface current (toward).
    pub current: Option<Vector>,
}

/// Anything that can say what the environment was at track positions.
pub trait Provider: Send + Sync {
    /// The environment at each of `points`, in order.
    ///
    /// # Errors
    /// [`crate::EnvError::Cancelled`] once `cancel` is set; a read error
    /// otherwise. Nothing partial is returned: a batch is all or nothing.
    fn sample(
        &self,
        points: &[Point],
        options: &Options,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Vec<EnvPoint>>;

    /// Called once with every position a fetch will ask for, before its
    /// batches, so the provider can start what they share (the archive's
    /// metadata). Nothing it does changes a
    /// batch's answer.
    ///
    /// # Errors
    /// [`crate::EnvError::Cancelled`] once `cancel` is set.
    fn prepare(
        &self,
        _points: &[Point],
        _options: &Options,
        _cancel: &Arc<AtomicBool>,
    ) -> Result<()> {
        Ok(())
    }

    /// What the last calls had to leave out and why, each said once;
    /// emptied by the call.
    fn take_warnings(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The real provider: the Whirlwind archive of spec.md 7.5.
///
/// The opened archive is kept for the life of the provider: opening costs
/// metadata requests, which a job over several tracks would otherwise pay
/// per track.
#[derive(Debug)]
pub struct Reanalysis {
    whirlwind: Whirlwind,
    /// What this session has downloaded: every track of every event reads
    /// through it, so the second boat of a race costs little.
    memory: Arc<BlockCache>,
}

/// Positions placed on a variable, and the corner values they need.
type Placed = (Vec<Option<Stamp>>, BTreeMap<Cell, f32>);

/// A position placed on a variable's grid and time axis.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Stamp {
    pub(crate) stencil: Stencil,
    pub(crate) a: u64,
    pub(crate) b: u64,
    pub(crate) w: f64,
}

/// The steps either side of `t` on `axis`, taking only every `every`
/// seconds (aligned on UTC midnight), and the weight of the later one.
/// Falls back to every step where the coarse steps are not on the axis
/// (its end, or an axis whose spacing does not divide `every`).
pub(crate) fn bracket_every(axis: &TimeAxis, t: i64, every: i64) -> Option<(u64, u64, f64)> {
    if every <= axis.step || every % axis.step != 0 {
        return axis.bracket(t);
    }
    let ta = t - t.rem_euclid(every);
    if ta == t {
        return axis.bracket(t);
    }
    let coarse = match (axis.bracket(ta), axis.bracket(ta + every)) {
        (Some((a, a2, _)), Some((b, b2, _))) if a == a2 && b == b2 => {
            Some((a, b, (t - ta) as f64 / every as f64))
        }
        _ => None,
    };
    coarse.or_else(|| axis.bracket(t))
}

/// Interpolates one side's corners, NaN corners left out.
fn side(stencil: &Stencil, corners: [f32; 4]) -> Option<f64> {
    stencil.interpolate(corners)
}

/// A scalar at each placed position.
pub(crate) fn scalar_of((stamps, values): &Placed) -> Vec<Option<f64>> {
    stamps
        .iter()
        .map(|stamp| {
            let s = stamp.as_ref()?;
            let first = side(&s.stencil, corners_of(values, s.a, &s.stencil));
            if s.a == s.b {
                return first;
            }
            let second = side(&s.stencil, corners_of(values, s.b, &s.stencil));
            lerp_time(first, second, s.w)
        })
        .collect()
}

/// A direction in degrees at each placed position, interpolated as a unit
/// vector so 350° and 10° average to 0°, not 180°.
pub(crate) fn direction_of((stamps, values): &Placed) -> Vec<Option<f64>> {
    let unit = |s: &Stamp, step: u64| -> Option<(f64, f64)> {
        let corners = corners_of(values, step, &s.stencil);
        let sin = corners.map(|d| (f64::from(d).to_radians().sin()) as f32);
        let cos = corners.map(|d| (f64::from(d).to_radians().cos()) as f32);
        Some((side(&s.stencil, sin)?, side(&s.stencil, cos)?))
    };
    stamps
        .iter()
        .map(|stamp| {
            let s = stamp.as_ref()?;
            // On a grid node at an archive hour the direction is the
            // archive's, read as it is rather than through sin, cos and
            // atan2, whose last bit differs between platforms' maths
            // libraries: a GRIB export of these nodes is byte-identical
            // everywhere (invariant 5).
            if s.a == s.b && s.stencil.lat_frac == 0.0 && s.stencil.lon_frac == 0.0 {
                let node = corners_of(values, s.a, &s.stencil)[0];
                return node.is_finite().then(|| f64::from(node).rem_euclid(360.0));
            }
            let first = unit(s, s.a);
            let (x, y) = if s.a == s.b {
                first?
            } else {
                let second = unit(s, s.b);
                (
                    lerp_time(first.map(|f| f.0), second.map(|f| f.0), s.w)?,
                    lerp_time(first.map(|f| f.1), second.map(|f| f.1), s.w)?,
                )
            };
            if x.hypot(y) < 1e-9 {
                return None;
            }
            Some(x.atan2(y).to_degrees().rem_euclid(360.0))
        })
        .collect()
}

impl Reanalysis {
    /// The Whirlwind archive, using its own bounded async pool.
    pub fn whirlwind(
        source: crate::whirlwind::Source,
        credentials: Option<crate::whirlwind::Credentials>,
        timeout: Duration,
        memory: Arc<BlockCache>,
    ) -> Result<Self> {
        let whirlwind = Whirlwind::new(
            source,
            credentials,
            timeout,
            Arc::clone(&memory),
            crate::whirlwind::CONCURRENCY,
        )?;
        Ok(Self::from_reader(whirlwind, memory))
    }

    /// A provider over an already built reader sharing `memory`.
    pub(crate) fn from_reader(whirlwind: Whirlwind, memory: Arc<BlockCache>) -> Self {
        Self { whirlwind, memory }
    }

    /// Persist validated Whirlwind chunks across routes and sessions.
    pub fn with_whirlwind_cache(mut self, cache: Arc<crate::whirlwind::DiskCache>) -> Self {
        self.whirlwind = self.whirlwind.with_disk_cache(cache);
        self
    }

    /// Download accounting for `points`: the exact compressed sizes of the
    /// chunks they need, from the archive's indexes.
    ///
    /// # Errors
    /// A metadata or index read failing.
    pub fn estimate(&self, points: &[Point]) -> Result<Estimate> {
        self.whirlwind.estimate(points)
    }

    /// `(requests, bytes)` sent to and received from the archive so far.
    pub fn net_totals(&self) -> (u64, u64) {
        self.whirlwind.net_totals()
    }

    /// What it keeps of its downloads.
    pub fn memory(&self) -> &Arc<BlockCache> {
        &self.memory
    }
}

impl Provider for Reanalysis {
    fn sample(
        &self,
        points: &[Point],
        options: &Options,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Vec<EnvPoint>> {
        self.whirlwind.sample(points, options, cancel)
    }

    fn prepare(&self, points: &[Point], options: &Options, cancel: &Arc<AtomicBool>) -> Result<()> {
        self.whirlwind.prepare(points, options, cancel)
    }
}

/// What a fetch is expected to download (spec.md 13, D19, D27).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Estimate {
    /// Bytes to download sampling hourly, chunks already held left out.
    pub hourly_bytes: u64,
    /// Bytes of the hourly fetch already held in memory or on disk.
    pub hourly_cached_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis() -> TimeAxis {
        TimeAxis {
            first: 0,
            step: 3600,
            len: 48,
        }
    }

    #[test]
    fn hourly_brackets_use_adjacent_hours() {
        assert_eq!(
            bracket_every(&axis(), 4 * 3600 + 1800, 3600),
            Some((4, 5, 0.5))
        );
        assert_eq!(bracket_every(&axis(), 6 * 3600, 3600), Some((6, 6, 0.0)));
        assert_eq!(
            bracket_every(&axis(), 46 * 3600 + 1800, 3600),
            Some((46, 47, 0.5))
        );
        assert_eq!(bracket_every(&axis(), -1, 3600), None);
    }

    #[test]
    fn intervals_round_trip_through_seconds() {
        let i = Interval::Hourly;
        assert_eq!(Interval::from_seconds(i.seconds()), Some(i));
        assert_eq!(Interval::from_seconds(7200), None);
        assert_eq!(Interval::from_seconds(10_800), None);
    }
}
