//! Tracks and their samples.
//!
//! The track model's behaviour: GeoJSON and CSV import, heading and speed
//! derivation from successive fixes, and the sample filters (spec.md 7). A
//! track's raw fixes are stored exactly as imported (invariant 1); derived
//! values and filters sit beside them.
//!
//! No network: tracker clients live in `pe-trackers`, which depends on this
//! crate and never the other way round.

pub mod csv;
pub mod daytime;
pub mod derive;
pub mod error;
pub mod filter;
pub mod geo;
pub mod geojson;
pub mod gpx;
pub mod time;

use pe_core::track::{DerivationSettings, Fix, Motion, Sample, Track, TrackOrigin, ValueOrigin};
use pe_core::{SampleId, TrackId};

pub use derive::{NormaliseReport, derive, normalise};
pub use error::{Reason, TrackFileError};
pub use filter::{boat_speed, filtered_out, polar_point, through_water};

/// The most fixes one file may hold: a year of ten-second fixes.
pub const MAX_FIXES: usize = 5_000_000;
/// The fastest speed a track may give, knots. Anything above is a unit
/// mistake, not a boat.
pub const MAX_GIVEN_SPEED_KN: f64 = 100.0;

/// One boat's fixes as a file gave them: not yet sorted or merged.
#[derive(Debug, Clone, PartialEq)]
pub struct RawTrack {
    /// The boat's name, when the file names one.
    pub boat: Option<String>,
    /// The fixes, in file order.
    pub fixes: Vec<Fix>,
}

/// A latitude and longitude on Earth, with the longitude folded into
/// [-180, 180). Longitudes written 0–360 are accepted.
pub(crate) fn position(lat: f64, lon: f64) -> Option<(f64, f64)> {
    (lat.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && lon.is_finite()
        && (-180.0..=360.0).contains(&lon))
    .then(|| (lat, geo::wrap_lon(lon)))
}

/// A given heading, folded into [0, 360); anything past a full turn either
/// way is refused.
pub(crate) fn check_heading(value: Option<f64>) -> Result<Option<f64>, Reason> {
    match value {
        None => Ok(None),
        Some(v) if v.is_finite() && (-360.0..=360.0).contains(&v) => Ok(Some(geo::wrap_360(v))),
        Some(v) => Err(Reason::OutOfRange(v)),
    }
}

/// A given speed, knots.
pub(crate) fn check_speed(value: Option<f64>) -> Result<Option<f64>, Reason> {
    match value {
        None => Ok(None),
        Some(v) if v.is_finite() && (0.0..=MAX_GIVEN_SPEED_KN).contains(&v) => Ok(Some(v)),
        Some(v) => Err(Reason::OutOfRange(v)),
    }
}

/// What importing one track found, for the import summary (spec.md 7.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Fixes kept.
    pub fixes: usize,
    /// Fixes that were out of time order and were sorted.
    pub out_of_order: usize,
    /// Fixes merged into another at the same time.
    pub duplicates: usize,
    /// Samples whose heading the track gave.
    pub heading_given: usize,
    /// Samples whose heading was derived.
    pub heading_derived: usize,
    /// Samples whose speed the track gave.
    pub speed_given: usize,
    /// Samples whose speed was derived.
    pub speed_derived: usize,
}

/// Builds a track from a file's fixes: sorted and merged, one sample per
/// fix with its heading and speed, each sample id from `next_sample`
/// (the project's allocator).
pub fn build_track(
    id: TrackId,
    origin: TrackOrigin,
    raw: Vec<Fix>,
    settings: DerivationSettings,
    mut next_sample: impl FnMut() -> SampleId,
) -> (Track, ImportReport) {
    let (fixes, normalised) = normalise(raw);
    let motion = derive(&fixes, &settings);
    let mut track = Track::new(id, origin);
    track.derivation = settings;
    track.samples = fixes
        .iter()
        .zip(&motion)
        .enumerate()
        .map(|(i, (fix, motion))| {
            let mut sample = Sample::at(next_sample(), u32::try_from(i).unwrap_or(u32::MAX), fix);
            sample.set_motion(*motion);
            sample.downloaded_wind_only = track.derivation.downloaded_wind_only;
            sample.relate();
            sample
        })
        .collect();
    track.fixes = fixes;
    let count = |f: &dyn Fn(&Motion) -> bool| motion.iter().filter(|m| f(m)).count();
    let report = ImportReport {
        fixes: track.fixes.len(),
        out_of_order: normalised.out_of_order,
        duplicates: normalised.duplicates,
        heading_given: count(&|m| m.heading_origin == Some(ValueOrigin::Given)),
        heading_derived: count(&|m| m.heading_origin == Some(ValueOrigin::Derived)),
        speed_given: count(&|m| m.speed_origin == Some(ValueOrigin::Given)),
        speed_derived: count(&|m| m.speed_origin == Some(ValueOrigin::Derived)),
    };
    (track, report)
}

/// Supplied wind uses the same knot convention as weather and polars.
pub(crate) fn check_wind_speed(
    value: Option<f64>,
) -> std::result::Result<Option<f64>, error::Reason> {
    if let Some(value) = value
        && (!value.is_finite() || !(0.0..=200.0).contains(&value))
    {
        return Err(error::Reason::OutOfRange(value));
    }
    Ok(value.map(pe_core::canonical::knots))
}

/// Every sample's motion under new derivation settings (spec.md 7.4:
/// editable later, undoable), in sample order. The caller wraps it in a
/// `Command::SetDerivation`.
pub fn rederive(track: &Track, settings: &DerivationSettings) -> Vec<Motion> {
    let motion = derive(&track.fixes, settings);
    track
        .samples
        .iter()
        .map(|sample| motion.get(sample.fix as usize).copied().unwrap_or_default())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(t: i64, lon: f64) -> Fix {
        Fix {
            tws: None,
            twd_from: None,
            t,
            lat: 0.0,
            lon,
            cog: None,
            sog: None,
        }
    }

    #[test]
    fn a_track_is_built_sorted_with_a_sample_per_fix() {
        let raw = vec![fix(1200, 0.02), fix(0, 0.0), fix(600, 0.01), fix(600, 0.01)];
        let mut next = 100;
        let (track, report) = build_track(
            TrackId(7),
            TrackOrigin::File {
                name: "a.csv".to_owned(),
                boat_name: None,
            },
            raw,
            DerivationSettings::default(),
            || {
                next += 1;
                SampleId(next)
            },
        );
        assert_eq!(track.fixes.len(), 3);
        assert_eq!(
            track.samples.iter().map(|s| s.id.raw()).collect::<Vec<_>>(),
            [101, 102, 103]
        );
        assert_eq!(
            track.samples.iter().map(|s| s.fix).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(track.samples[2].t, 1200);
        assert_eq!(report.out_of_order, 1);
        assert_eq!(report.duplicates, 1);
        assert_eq!(report.heading_derived, 3);
        assert_eq!(report.speed_given, 0);

        let wider = DerivationSettings {
            max_gap_s: 300,
            ..DerivationSettings::default()
        };
        let motion = rederive(&track, &wider);
        assert!(motion.iter().all(|m| *m == Motion::default()));
    }

    #[test]
    fn positions_are_checked_and_folded() {
        assert_eq!(position(10.0, 190.0), Some((10.0, -170.0)));
        assert_eq!(position(10.0, 180.0), Some((10.0, -180.0)));
        assert_eq!(position(91.0, 0.0), None);
        assert_eq!(position(f64::NAN, 0.0), None);
        assert_eq!(check_heading(Some(-90.0)), Ok(Some(270.0)));
        assert!(check_heading(Some(400.0)).is_err());
        assert!(check_speed(Some(-1.0)).is_err());
        assert!(check_speed(Some(150.0)).is_err());
    }
}

/// Native SYRF race archive reader.
pub mod syrf;
