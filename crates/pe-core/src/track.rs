//! A track as plain data: where it came from, its fixes as imported, and the
//! samples derived from them with the environment found for each.
//!
//! Filled by `pe-tracks` (import, derivation, filters) and `pe-env`
//! (reanalysis). The fields are those of spec.md 7.4 and 7.5 in full, so the
//! later milestones fill them in without a schema migration.
//!
//! **Bulk data lives outside `project.json`.** `fixes` and `samples` are
//! skipped by serde here and written to `tracks/<id>.json` by [`crate::io`],
//! so the project document stays small and diffable (spec.md 4.3). Samples
//! are written by column ([`SampleColumns`]) with only what is not derived.

use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::id::{SampleId, TrackId};

/// One imported track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Names the archive entry holding this track's fixes and samples.
    pub id: TrackId,
    /// Where it came from.
    pub origin: TrackOrigin,
    /// How heading and speed are derived (spec.md 7.4).
    #[serde(default)]
    pub derivation: DerivationSettings,
    /// How this track's polar segment summarises a cell (spec.md 12.1).
    #[serde(default)]
    pub statistic: SegmentStatistic,
    /// Which reanalysis datasets supplied the samples.
    #[serde(default)]
    pub env_meta: EnvMeta,
    /// The fixes exactly as imported, sorted by time with duplicates merged
    /// (spec.md 7.4). Never rewritten afterwards (invariant 1).
    #[serde(skip)]
    pub fixes: Vec<Fix>,
    /// One sample per fix, derived and fetched.
    #[serde(skip)]
    pub samples: Vec<Sample>,
}

impl Track {
    /// A track with no fixes yet.
    pub fn new(id: TrackId, origin: TrackOrigin) -> Self {
        Self {
            id,
            origin,
            derivation: DerivationSettings::default(),
            statistic: SegmentStatistic::default(),
            env_meta: EnvMeta::default(),
            fixes: Vec::new(),
            samples: Vec::new(),
        }
    }

    /// The bulk half, as written to `tracks/<id>.json`.
    ///
    /// # Errors
    /// [`crate::CoreError::Invalid`] if a stored value is not finite.
    pub fn bulk(&self) -> crate::Result<TrackBulkRef<'_>> {
        Ok(TrackBulkRef {
            fixes: &self.fixes,
            samples: SampleColumns::of(&self.samples)?,
        })
    }
}

/// The part of a track written to its own archive entry.
#[derive(Debug, Serialize)]
pub struct TrackBulkRef<'a> {
    /// See [`Track::fixes`].
    pub fixes: &'a [Fix],
    /// See [`Track::samples`], by column.
    pub samples: SampleColumns,
}

/// [`TrackBulkRef`] as read back (schema 2 on).
#[derive(Debug, Default, Deserialize)]
pub struct TrackBulk {
    /// See [`Track::fixes`].
    #[serde(default)]
    pub fixes: Vec<Fix>,
    /// See [`Track::samples`].
    #[serde(default)]
    pub samples: SampleColumns,
}

/// A track's bulk entry as schema 1 wrote it: one object per sample with
/// every value, derived ones included. Read only, to migrate.
#[derive(Debug, Default, Deserialize)]
pub struct TrackBulkV1 {
    /// See [`Track::fixes`].
    #[serde(default)]
    pub fixes: Vec<Fix>,
    /// Every field of every sample.
    #[serde(default)]
    pub samples: Vec<Sample>,
}

impl TrackBulkV1 {
    /// The fixes and samples, the samples as schema 2 keeps them: the
    /// environment rounded to its stored precision and every derived value
    /// recomputed.
    pub fn into_parts(self) -> crate::Result<(Vec<Fix>, Vec<Sample>)> {
        let samples = self
            .samples
            .into_iter()
            .map(|mut s| {
                let fix = self.fixes.get(s.fix as usize).ok_or_else(|| {
                    crate::CoreError::Invalid(format!(
                        "sample #{} refers to fix {} of {}",
                        s.id.raw(),
                        s.fix,
                        self.fixes.len()
                    ))
                })?;
                (s.t, s.lat, s.lon) = (fix.t, fix.lat, fix.lon);
                s.quantise_env();
                s.relate();
                Ok(s)
            })
            .collect::<crate::Result<Vec<_>>>()?;
        Ok((self.fixes, samples))
    }
}

/// A track's samples by column (spec.md 4.3, D27): what a sample has that
/// its fix does not — its id, its motion, the environment found for it and
/// which dataset supplied each part. Its time and place are its fix's, and
/// everything that relates the motion to the environment (TWA, tack, the
/// current-corrected values, the wave angle) is recomputed on load, so none
/// of it is written.
///
/// Columns of like values compress far better than objects of mixed ones:
/// at the environment's stored precision a thousand samples of wind, waves
/// and current are a few kilobytes in the file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SampleColumns {
    /// Sample ids as runs of consecutive ids: `[first, count]`.
    pub ids: Vec<[u64; 2]>,
    /// Fix indices, as runs like `ids`.
    pub fixes: Vec<[u64; 2]>,
    /// Heading over the ground, degrees (canonical precision).
    pub heading: Vec<Option<f64>>,
    /// Where each heading came from: `g` given, `d` derived, `-` none.
    pub heading_origin: String,
    /// Speed over the ground, knots (canonical precision).
    pub speed: Vec<Option<f64>>,
    /// Where each speed came from, as [`Self::heading_origin`].
    pub speed_origin: String,
    /// Whether the environment fetch has answered: `1` or `0`.
    pub fetched: String,
    /// True wind speed, knots, to 0.01.
    pub tws: Vec<Option<f64>>,
    /// True wind direction ("from"), degrees, to 0.1.
    pub twd_from: Vec<Option<f64>>,
    /// Significant wave height, metres, to 0.01.
    pub hs_m: Vec<Option<f64>>,
    /// Mean wave period in seconds; absent in schema 2 projects.
    pub wave_period_s: Vec<Option<f64>>,
    /// Mean wave direction ("from"), degrees, to 0.1.
    pub wave_from: Vec<Option<f64>>,
    /// Current speed, knots, to 0.01.
    pub current_speed: Vec<Option<f64>>,
    /// Current direction ("toward"), degrees, to 0.1.
    pub current_toward: Vec<Option<f64>>,
    /// Which dataset supplied the wind (index into the track's datasets).
    pub wind_dataset: Vec<Option<u16>>,
    /// Which dataset supplied the waves.
    pub wave_dataset: Vec<Option<u16>>,
    /// Which dataset supplied the current.
    pub current_dataset: Vec<Option<u16>>,
}

/// Runs of consecutive values: `[first, count]`.
fn runs(values: impl Iterator<Item = u64>) -> Vec<[u64; 2]> {
    let mut out: Vec<[u64; 2]> = Vec::new();
    for v in values {
        match out.last_mut() {
            Some([first, count]) if first.checked_add(*count) == Some(v) => *count += 1,
            _ => out.push([v, 1]),
        }
    }
    out
}

/// The values runs stand for, refusing more than `limit` of them.
fn unrun(runs: &[[u64; 2]], limit: usize, what: &str) -> crate::Result<Vec<u64>> {
    let total = runs.iter().fold(0u64, |t, [_, n]| t.saturating_add(*n));
    if total > limit as u64 {
        return Err(crate::CoreError::Invalid(format!(
            "the {what} column holds {total} values, more than the {limit} allowed"
        )));
    }
    Ok(runs
        .iter()
        .flat_map(|&[first, n]| (0..n).map(move |k| first.saturating_add(k)))
        .collect())
}

/// One character per origin.
fn origin_code(origin: Option<ValueOrigin>) -> char {
    match origin {
        Some(ValueOrigin::Given) => 'g',
        Some(ValueOrigin::Derived) => 'd',
        None => '-',
    }
}

fn origin_of(code: char) -> crate::Result<Option<ValueOrigin>> {
    match code {
        'g' => Ok(Some(ValueOrigin::Given)),
        'd' => Ok(Some(ValueOrigin::Derived)),
        '-' => Ok(None),
        other => Err(crate::CoreError::Invalid(format!(
            "{other:?} is not a value origin (g, d or -)"
        ))),
    }
}

/// A column of optional values rounded by `round`, refusing a non-finite
/// one (it would be written as `null` and read back as missing).
fn column(
    samples: &[Sample],
    what: &str,
    value: impl Fn(&Sample) -> Option<f64>,
    round: fn(f64) -> f64,
) -> crate::Result<Vec<Option<f64>>> {
    samples
        .iter()
        .map(|s| match value(s) {
            Some(v) if !v.is_finite() => Err(crate::CoreError::Invalid(format!(
                "sample #{} has a {what} of {v}; only finite numbers can be stored",
                s.id.raw()
            ))),
            v => Ok(v.map(round)),
        })
        .collect()
}

impl SampleColumns {
    /// The columns of `samples`.
    ///
    /// # Errors
    /// [`crate::CoreError::Invalid`] if a value is not finite.
    pub fn of(samples: &[Sample]) -> crate::Result<Self> {
        use crate::canonical::{degrees, env_degrees, env_knots, env_metres, knots};
        Ok(Self {
            ids: runs(samples.iter().map(|s| s.id.raw())),
            fixes: runs(samples.iter().map(|s| u64::from(s.fix))),
            heading: column(samples, "heading", |s| s.heading, degrees)?,
            heading_origin: samples
                .iter()
                .map(|s| origin_code(s.heading_origin))
                .collect(),
            speed: column(samples, "speed", |s| s.speed, knots)?,
            speed_origin: samples
                .iter()
                .map(|s| origin_code(s.speed_origin))
                .collect(),
            fetched: samples
                .iter()
                .map(|s| if s.env_fetched { '1' } else { '0' })
                .collect(),
            tws: column(samples, "wind speed", |s| s.tws, env_knots)?,
            twd_from: column(samples, "wind direction", |s| s.twd_from, env_degrees)?,
            hs_m: column(samples, "wave height", |s| s.hs_m, env_metres)?,
            wave_period_s: column(samples, "wave period", |s| s.wave_period_s, env_metres)?,
            wave_from: column(samples, "wave direction", |s| s.wave_from, env_degrees)?,
            current_speed: column(samples, "current speed", |s| s.current_speed, env_knots)?,
            current_toward: column(
                samples,
                "current direction",
                |s| s.current_toward,
                env_degrees,
            )?,
            wind_dataset: samples.iter().map(|s| s.wind_dataset).collect(),
            wave_dataset: samples.iter().map(|s| s.wave_dataset).collect(),
            current_dataset: samples.iter().map(|s| s.current_dataset).collect(),
        })
    }

    /// The samples these columns hold, placed on `fixes`, the environment
    /// rounded as stored and every derived value recomputed.
    ///
    /// # Errors
    /// [`crate::CoreError::Invalid`] naming the column if the columns
    /// disagree in length, a sample names a fix that is not there, or an
    /// origin or fetched flag is not one of its codes.
    pub fn into_samples(self, fixes: &[Fix]) -> crate::Result<Vec<Sample>> {
        use crate::canonical::{degrees, knots};
        // A column may not claim more samples than there are fixes: each
        // fix has at most one sample.
        let ids = unrun(&self.ids, fixes.len(), "ids")?;
        let fix_index = unrun(&self.fixes, fixes.len(), "fixes")?;
        let n = ids.len();
        let check = |what: &str, len: usize| {
            if len == n {
                Ok(())
            } else {
                Err(crate::CoreError::Invalid(format!(
                    "the {what} column has {len} values for {n} samples"
                )))
            }
        };
        check("fixes", fix_index.len())?;
        check("heading", self.heading.len())?;
        check("heading_origin", self.heading_origin.chars().count())?;
        check("speed", self.speed.len())?;
        check("speed_origin", self.speed_origin.chars().count())?;
        check("fetched", self.fetched.chars().count())?;
        check("tws", self.tws.len())?;
        check("twd_from", self.twd_from.len())?;
        check("hs_m", self.hs_m.len())?;
        if !self.wave_period_s.is_empty() {
            check("wave_period_s", self.wave_period_s.len())?;
        }
        check("wave_from", self.wave_from.len())?;
        check("current_speed", self.current_speed.len())?;
        check("current_toward", self.current_toward.len())?;
        check("wind_dataset", self.wind_dataset.len())?;
        check("wave_dataset", self.wave_dataset.len())?;
        check("current_dataset", self.current_dataset.len())?;
        let mut heading_origin = self.heading_origin.chars();
        let mut speed_origin = self.speed_origin.chars();
        let mut fetched = self.fetched.chars();
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            let index = fix_index[k];
            let fix = usize::try_from(index)
                .ok()
                .and_then(|i| fixes.get(i))
                .ok_or_else(|| {
                    crate::CoreError::Invalid(format!(
                        "sample #{} refers to fix {index} of {}",
                        ids[k],
                        fixes.len()
                    ))
                })?;
            let mut s = Sample::at(
                crate::id::SampleId(ids[k]),
                u32::try_from(index).unwrap_or(u32::MAX),
                fix,
            );
            s.heading = self.heading[k].map(degrees);
            s.heading_origin = origin_of(heading_origin.next().unwrap_or('-'))?;
            s.speed = self.speed[k].map(knots);
            s.speed_origin = origin_of(speed_origin.next().unwrap_or('-'))?;
            s.env_fetched = match fetched.next() {
                Some('1') => true,
                Some('0') | None => false,
                Some(other) => {
                    return Err(crate::CoreError::Invalid(format!(
                        "{other:?} is not a fetched flag (1 or 0)"
                    )));
                }
            };
            s.tws = self.tws[k];
            s.twd_from = self.twd_from[k];
            s.hs_m = self.hs_m[k];
            s.wave_from = self.wave_from[k];
            s.wave_period_s = self.wave_period_s.get(k).copied().flatten();
            s.current_speed = self.current_speed[k];
            s.current_toward = self.current_toward[k];
            s.wind_dataset = self.wind_dataset[k];
            s.wave_dataset = self.wave_dataset[k];
            s.current_dataset = self.current_dataset[k];
            s.quantise_env();
            s.relate();
            out.push(s);
        }
        Ok(out)
    }
}

/// Which tracker a track was scraped from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tracker {
    /// YellowBrick (`yb.tl`).
    YellowBrick,
    /// Geovoile (`*.geovoile.com`).
    Geovoile,
    /// Blue Water Tracks.
    BlueWaterTracks,
}

/// Where a track came from (spec.md 4.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrackOrigin {
    /// One boat of a tracker event.
    Tracker {
        /// Which tracker.
        tracker: Tracker,
        /// The event URL the user pasted.
        event_url: String,
        /// The event's title as the tracker gives it.
        event_title: String,
        /// The tracker's own id for the boat.
        boat_id: String,
        /// Boat name.
        boat_name: String,
        /// Sail number, if the tracker gives one.
        #[serde(default)]
        sail_no: Option<String>,
        /// Boat model or class, if the tracker gives one (M10). Left out
        /// of the file when absent, so earlier projects save unchanged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        /// The division the tracker lists the boat in (M10), e.g. "IRC 2".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        division: Option<String>,
        /// The race start as the tracker gives it, UTC epoch seconds. The
        /// default start of the time-window filter (spec.md 7.6).
        #[serde(default)]
        race_start: Option<i64>,
        /// The race finish as the tracker gives it, UTC epoch seconds.
        #[serde(default)]
        race_finish: Option<i64>,
    },
    /// A GeoJSON or CSV file.
    File {
        /// The file name it was imported from.
        name: String,
        /// The boat name, when the file has several boats.
        #[serde(default)]
        boat_name: Option<String>,
    },
}

/// One timestamped position, as imported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fix {
    /// UTC epoch seconds.
    pub t: i64,
    /// Latitude, degrees.
    #[serde(with = "canonical::degrees_field")]
    pub lat: f64,
    /// Longitude, degrees in [-180, 180).
    #[serde(with = "canonical::degrees_field")]
    pub lon: f64,
    /// Course over ground the track supplied, degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub cog: Option<f64>,
    /// Speed over ground (or boat speed) the track supplied, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub sog: Option<f64>,
    /// Supplied true wind speed over ground, knots; never replaced by weather.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "canonical::optional_knots_field"
    )]
    pub tws: Option<f64>,
    /// Supplied true wind direction, meteorological "from", degrees.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "canonical::optional_degrees_field"
    )]
    pub twd_from: Option<f64>,
}

impl Fix {
    /// Complete supplied wind at the precision used by the project file.
    /// Raw imported values remain untouched; derived samples must agree before
    /// and after saving and reopening, including converted wind-speed units.
    pub fn supplied_wind(&self) -> Option<(f64, f64)> {
        self.tws
            .zip(self.twd_from)
            .map(|(speed, direction)| (canonical::knots(speed), canonical::degrees(direction)))
    }
}

/// Whether a value came with the track or was computed from neighbours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueOrigin {
    /// Supplied by the tracker or file.
    Given,
    /// Derived from neighbouring fixes (spec.md 7.4).
    Derived,
}

/// Which side the wind is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tack {
    /// Wind over the port side.
    Port,
    /// Wind over the starboard side.
    Starboard,
}

/// One track position with its derived motion and the environment found for
/// it (spec.md 2, 7.4, 7.5). A dot in the plots.
///
/// **Raw** values are ground-relative, as the tracker and the reanalysis give
/// them. **Corrected** values are water-relative, with the current taken out
/// (spec.md 7.5, D13); they are present only where a current was found. A
/// project setting chooses which feed the polar. Everything the pipeline may
/// fail to find is optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// Rebuilt from the immutable fix; the downloaded wind remains in `tws`/`twd_from`.
    #[serde(skip)]
    pub supplied_wind: Option<(f64, f64)>,
    /// Rebuilt from the track's derivation overlay.
    #[serde(skip)]
    pub downloaded_wind_only: bool,
    /// Named by exclusions in the overlay.
    pub id: SampleId,
    /// Index of the fix this sample is for.
    pub fix: u32,
    /// UTC epoch seconds (the fix's).
    pub t: i64,
    /// Latitude, degrees (the fix's).
    #[serde(with = "canonical::degrees_field")]
    pub lat: f64,
    /// Longitude, degrees (the fix's).
    #[serde(with = "canonical::degrees_field")]
    pub lon: f64,

    // --- Motion over the ground (spec.md 7.4) ---
    /// Heading over the ground, degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub heading: Option<f64>,
    /// Whether [`Self::heading`] was given or derived.
    #[serde(default)]
    pub heading_origin: Option<ValueOrigin>,
    /// Speed over the ground, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub speed: Option<f64>,
    /// Whether [`Self::speed`] was given or derived.
    #[serde(default)]
    pub speed_origin: Option<ValueOrigin>,

    // --- Wind, raw (ground-relative) ---
    /// True wind speed over the ground, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub tws: Option<f64>,
    /// True wind direction, meteorological "from", degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub twd_from: Option<f64>,
    /// True wind angle off the ground heading, degrees in [0, 180].
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub twa: Option<f64>,
    /// Tack from the ground heading and wind.
    #[serde(default)]
    pub tack: Option<Tack>,

    // --- Water-relative, current removed (spec.md 7.5, D13) ---
    /// Boat speed through the water, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub bsp_corrected: Option<f64>,
    /// Heading through the water, degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub heading_corrected: Option<f64>,
    /// True wind speed over the water, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub tws_corrected: Option<f64>,
    /// True wind direction over the water, "from", degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub twd_from_corrected: Option<f64>,
    /// True wind angle through the water, degrees in [0, 180].
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub twa_corrected: Option<f64>,
    /// Tack through the water.
    #[serde(default)]
    pub tack_corrected: Option<Tack>,

    // --- Waves ---
    /// Significant wave height, metres.
    #[serde(default, with = "canonical::optional_metres_field")]
    pub hs_m: Option<f64>,
    /// Mean wave period, seconds.
    #[serde(default, with = "canonical::optional_ratio_field")]
    pub wave_period_s: Option<f64>,
    /// Mean wave direction, "from", degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub wave_from: Option<f64>,
    /// Angle between the bow and where the waves come from, degrees in
    /// [0, 180]: 0 head seas, 180 following.
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub wave_angle: Option<f64>,

    // --- Current ---
    /// Surface current speed, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub current_speed: Option<f64>,
    /// Surface current direction, oceanographic "toward", degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub current_toward: Option<f64>,

    // --- Provenance: indices into the track's `env_meta.datasets` ---
    /// Which dataset supplied the wind.
    #[serde(default)]
    pub wind_dataset: Option<u16>,
    /// Which dataset supplied the waves.
    #[serde(default)]
    pub wave_dataset: Option<u16>,
    /// Which dataset supplied the current.
    #[serde(default)]
    pub current_dataset: Option<u16>,
    /// Whether the environment fetch has answered for this sample (even if
    /// it found nothing there, as over land for waves or before an
    /// archive's first hour). What a Refetch resumes from: samples without
    /// it are the ones still to fetch (spec.md 7.7).
    #[serde(default)]
    pub env_fetched: bool,
}

impl Sample {
    /// A sample at a fix with nothing derived or fetched yet.
    pub fn at(id: SampleId, fix_index: u32, fix: &Fix) -> Self {
        Self {
            id,
            supplied_wind: fix.supplied_wind(),
            downloaded_wind_only: false,
            fix: fix_index,
            t: fix.t,
            lat: fix.lat,
            lon: fix.lon,
            heading: None,
            heading_origin: None,
            speed: None,
            speed_origin: None,
            tws: None,
            twd_from: None,
            twa: None,
            tack: None,
            bsp_corrected: None,
            heading_corrected: None,
            tws_corrected: None,
            twd_from_corrected: None,
            twa_corrected: None,
            tack_corrected: None,
            hs_m: None,
            wave_period_s: None,
            wave_from: None,
            wave_angle: None,
            current_speed: None,
            current_toward: None,
            wind_dataset: None,
            wave_dataset: None,
            current_dataset: None,
            env_fetched: false,
        }
    }
}

/// A sample's motion over the ground: heading and speed with where each
/// came from (spec.md 7.4). What re-deriving a track rewrites, and all it
/// rewrites; [`crate::Command::SetDerivation`] carries it both ways.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    /// Heading over the ground, degrees in [0, 360).
    pub heading: Option<f64>,
    /// Whether the heading was given or derived.
    pub heading_origin: Option<ValueOrigin>,
    /// Speed over the ground, knots.
    pub speed: Option<f64>,
    /// Whether the speed was given or derived.
    pub speed_origin: Option<ValueOrigin>,
}

impl Sample {
    /// This sample's motion.
    pub fn motion(&self) -> Motion {
        Motion {
            heading: self.heading,
            heading_origin: self.heading_origin,
            speed: self.speed,
            speed_origin: self.speed_origin,
        }
    }

    /// Replaces this sample's motion.
    pub fn set_motion(&mut self, motion: Motion) {
        self.heading = motion.heading;
        self.heading_origin = motion.heading_origin;
        self.speed = motion.speed;
        self.speed_origin = motion.speed_origin;
    }

    /// Rounds the environment to the precision it is stored at (spec.md
    /// 7.5, [`crate::canonical::ENV_KNOT_PLACES`]), so what is in memory is
    /// exactly what a save and a load give back. The fetch calls it when it
    /// stores what it found, before relating it to the motion.
    pub fn quantise_env(&mut self) {
        use crate::canonical::{env_degrees, env_knots, env_metres};
        self.tws = self.tws.map(env_knots);
        self.twd_from = self.twd_from.map(env_degrees);
        self.hs_m = self.hs_m.map(env_metres);
        self.wave_from = self.wave_from.map(env_degrees);
        self.wave_period_s = self.wave_period_s.map(env_metres);
        self.current_speed = self.current_speed.map(env_knots);
        self.current_toward = self.current_toward.map(env_degrees);
    }

    /// Forgets everything the environment fetch stored for this sample —
    /// wind, waves and current, raw and corrected, and which datasets
    /// supplied them — as a Refetch that starts over does, so no value of
    /// an earlier fetch survives next to the new one.
    pub fn clear_env(&mut self) {
        self.tws = None;
        self.twd_from = None;
        self.hs_m = None;
        self.wave_from = None;
        self.wave_period_s = None;
        self.current_speed = None;
        self.current_toward = None;
        self.wind_dataset = None;
        self.wave_dataset = None;
        self.current_dataset = None;
        self.env_fetched = false;
        self.relate();
    }

    /// Effective wind speed. Supplied wind requires both speed and direction;
    /// incomplete pairs fall back together, never mixing two observations.
    pub fn wind_speed(&self) -> Option<f64> {
        if !self.downloaded_wind_only
            && let Some((speed, _)) = self.supplied_wind
        {
            Some(speed)
        } else {
            self.tws
        }
    }

    /// Effective meteorological wind direction over ground.
    pub fn wind_direction(&self) -> Option<f64> {
        if !self.downloaded_wind_only
            && let Some((_, direction)) = self.supplied_wind
        {
            Some(direction)
        } else {
            self.twd_from
        }
    }

    /// Signed apparent wind bearing clockwise from the bow, in [0, 360).
    /// Air relative to the boat is true air velocity minus boat velocity;
    /// subtracting the same current from both cancels exactly.
    pub fn apparent_wind_angle(&self, use_corrected: bool) -> Option<f64> {
        let (heading, speed) = self.heading.zip(self.speed)?;
        let (wind, from) = self.wind_speed().zip(self.wind_direction())?;
        // Work in the boat's ground-heading frame. The apparent "from"
        // vector is true wind "from" plus boat velocity; this needs only
        // one rotation instead of rotating both global vectors.
        let angle = (from - heading).to_radians();
        let (east, north) = (wind * angle.sin(), wind * angle.cos() + speed);
        let bow = if use_corrected {
            self.heading_corrected.unwrap_or(heading)
        } else {
            heading
        };
        (east.hypot(north) > 1e-9).then(|| {
            canonical::degrees((east.atan2(north).to_degrees() + heading - bow).rem_euclid(360.0))
        })
    }

    /// Recomputes everything that relates the boat's motion to the stored
    /// environment (spec.md 7.5, D13): TWA and tack over the ground, the
    /// current-corrected motion and wind, and the wave angle off the bow.
    ///
    /// Reads only the stored motion and environment, so a change of
    /// derivation settings recomputes these without fetching anything
    /// (M8 carry), and the environment fetch calls it once it has stored
    /// what it found. Plain vector arithmetic on the stored values, which is
    /// why it lives with the data rather than in `pe-tracks` or `pe-env`.
    ///
    /// - Wind over the ground: TWA is the angle between the heading and
    ///   where the wind comes from, 0–180°; the wind on the starboard side
    ///   is starboard tack.
    /// - With a current: the boat's velocity through the water is its
    ///   ground velocity minus the current (leeway ignored), and the wind
    ///   over the water is the wind minus the current. Without one, every
    ///   corrected value is empty.
    /// - The wave angle is measured off the bow, which points along the
    ///   heading through the water where there is a current and the ground
    ///   heading otherwise: 0° head seas, 180° following.
    pub fn relate(&mut self) {
        self.twa = self
            .heading
            .zip(self.wind_direction())
            .map(|(h, w)| angle_off(h, w));
        self.tack = self
            .heading
            .zip(self.wind_direction())
            .and_then(|(h, w)| tack_of(h, w));
        self.bsp_corrected = None;
        self.heading_corrected = None;
        self.tws_corrected = None;
        self.twd_from_corrected = None;
        self.twa_corrected = None;
        self.tack_corrected = None;
        if let (Some(cs), Some(ct)) = (self.current_speed, self.current_toward) {
            let (ce, cn) = toward(cs, ct);
            if let (Some(h), Some(s)) = (self.heading, self.speed) {
                let (ge, gn) = toward(s, h);
                let (we, wn) = (ge - ce, gn - cn);
                let bsp = we.hypot(wn);
                self.bsp_corrected = Some(bsp);
                // A boat stopped in the water has no heading through it.
                self.heading_corrected = (bsp > 1e-9).then(|| compass(we, wn));
            }
            if let (Some(tws), Some(from)) = (self.wind_speed(), self.wind_direction()) {
                let (ae, an) = toward(tws, from + 180.0);
                let (re, rn) = (ae - ce, an - cn);
                let speed = re.hypot(rn);
                self.tws_corrected = Some(speed);
                self.twd_from_corrected =
                    (speed > 1e-9).then(|| (compass(re, rn) + 180.0).rem_euclid(360.0));
            }
            let pair = self.heading_corrected.zip(self.twd_from_corrected);
            self.twa_corrected = pair.map(|(h, w)| angle_off(h, w));
            self.tack_corrected = pair.and_then(|(h, w)| tack_of(h, w));
        }
        let bow = self.heading_corrected.or(self.heading);
        self.wave_angle = bow.zip(self.wave_from).map(|(h, w)| angle_off(h, w));
        // Everything above went through the platform's libm (sin, cos,
        // atan2, hypot), which may differ in the last bit between targets.
        // These values feed the blend and export (invariant 5) and are now
        // recomputed on every load, so they are rounded to canonical
        // precision, where one-ULP differences vanish.
        use crate::canonical::{degrees, knots};
        for angle in [
            &mut self.twa,
            &mut self.heading_corrected,
            &mut self.twd_from_corrected,
            &mut self.twa_corrected,
            &mut self.wave_angle,
        ] {
            *angle = angle.map(degrees);
        }
        for speed in [&mut self.bsp_corrected, &mut self.tws_corrected] {
            *speed = speed.map(knots);
        }
        // 359.9999999996 rounds to 360: keep directions in [0, 360).
        for direction in [&mut self.heading_corrected, &mut self.twd_from_corrected] {
            if *direction == Some(360.0) {
                *direction = Some(0.0);
            }
        }
    }
}

/// East and north components of `speed` toward compass `direction`.
fn toward(speed: f64, direction: f64) -> (f64, f64) {
    let r = direction.to_radians();
    (speed * r.sin(), speed * r.cos())
}

/// The compass direction a vector points toward, degrees in [0, 360).
pub fn compass(east: f64, north: f64) -> f64 {
    let d = east.atan2(north).to_degrees().rem_euclid(360.0);
    // rem_euclid can round a tiny negative up to exactly 360.
    if d >= 360.0 { 0.0 } else { d }
}

/// The unsigned angle between two compass directions, [0, 180].
fn angle_off(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    if d > 180.0 { 360.0 - d } else { d }
}

/// The tack for a heading and a wind "from": wind over the starboard side
/// (coming from 0–180° clockwise of the bow) is starboard tack. Head to
/// wind or dead downwind is neither.
fn tack_of(heading: f64, from: f64) -> Option<Tack> {
    let r = (from - heading).rem_euclid(360.0);
    if r > 0.0 && r < 180.0 {
        Some(Tack::Starboard)
    } else if r > 180.0 && r < 360.0 {
        Some(Tack::Port)
    } else {
        None
    }
}

/// Which value to use when a fix has both a given and a derived one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreferValues {
    /// The tracker's or file's own value, where present.
    #[default]
    Given,
    /// Always the derived one.
    Derived,
}

/// How heading and speed are derived for a track (spec.md 7.4). Heading
/// and speed are chosen apart (asked 2026-10-02): a file may give a sound
/// course and a poor speed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DerivationSettings {
    /// Longest gap between neighbours a central difference may span, seconds.
    /// Beyond it the fix gets no derived values.
    pub max_gap_s: i64,
    /// The given or the derived heading first.
    pub prefer_heading: PreferValues,
    /// The given or the derived speed first.
    pub prefer_speed: PreferValues,
    /// Ignore supplied wind and use downloaded weather only when selected.
    pub downloaded_wind_only: bool,
}

/// The longest maximum gap offered, seconds: a central difference across
/// more than a day says nothing about how the boat was sailing.
pub const MAX_GAP_LIMIT_S: i64 = 24 * 3600;

impl DerivationSettings {
    /// Checks the settings are ones the derivation can use.
    pub fn validate(&self) -> crate::Result<()> {
        if (1..=MAX_GAP_LIMIT_S).contains(&self.max_gap_s) {
            Ok(())
        } else {
            Err(crate::CoreError::Invalid(format!(
                "a maximum gap of {} s is outside 1 s to {MAX_GAP_LIMIT_S} s",
                self.max_gap_s
            )))
        }
    }
}

impl Default for DerivationSettings {
    fn default() -> Self {
        Self {
            max_gap_s: 3 * 3600,
            prefer_heading: PreferValues::Given,
            prefer_speed: PreferValues::Given,
            downloaded_wind_only: false,
        }
    }
}

/// The per-cell statistic of a track's polar segment (spec.md 12.1, D18).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentStatistic {
    /// Lowest speed.
    Min,
    /// Median.
    Median,
    /// Mean.
    Mean,
    /// 75th percentile.
    P75,
    /// 90th percentile: a polar describes good sailing, not average sailing.
    #[default]
    P90,
    /// Highest speed.
    Max,
}

/// Which reanalysis a track's samples came from (spec.md 4.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvMeta {
    /// Every dataset any sample refers to, by index.
    pub datasets: Vec<DatasetRecord>,
    /// How far the environment fetch got.
    pub status: EnvStatus,
    /// The wind and wave sampling interval of the last fetch, seconds (3600
    /// hourly; 10800 in older projects); `None` before any fetch. A Refetch
    /// at another interval starts over rather than mixing the two.
    #[serde(default)]
    pub interval_s: Option<i64>,
    /// Whether the last fetch was asked for Stokes drift in the current
    /// (spec.md 7.5); `None` before any fetch. Like the interval,
    /// a Refetch with the other choice starts over, so a track never mixes
    /// currents with and without it.
    #[serde(default)]
    pub stokes_drift: Option<bool>,
}

/// One dataset a track's environment was taken from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetRecord {
    /// Dataset name: the archive's identity, e.g. its `s3://…` URI (spec.md 7.5).
    pub name: String,
    /// Dataset version as recorded by the archive.
    pub version: String,
    /// When it was fetched, UTC epoch seconds.
    pub fetched_at: i64,
    /// Whether this dataset's current includes tides. `None` for datasets
    /// that are not currents; a reanalysis archive's current includes tide
    /// (`Some(true)`, spec.md 7.5). `Some(false)` is kept for a future
    /// source without one, which the "leave out currents
    /// without tide" filter can still remove.
    #[serde(default)]
    pub has_tide: Option<bool>,
}

/// How far a track's environment fetch got (spec.md 7.1). "Fetching" is not
/// stored: a job is not project data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvStatus {
    /// Nothing fetched yet.
    #[default]
    NotFetched,
    /// Every sample has its environment.
    Ready,
    /// Some samples have it; a Refetch resumes.
    Partial,
    /// The fetch failed with nothing to show.
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Sample {
        let fix = Fix {
            tws: None,
            twd_from: None,
            t: 0,
            lat: 50.0,
            lon: -5.0,
            cog: None,
            sog: None,
        };
        Sample::at(SampleId(1), 0, &fix)
    }

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    /// No current: TWA off the ground heading, tack by the side the wind is
    /// on, nothing corrected; waves from astern are following seas.
    #[test]
    fn without_a_current_only_ground_values_are_related() {
        let mut s = sample();
        s.heading = Some(0.0);
        s.speed = Some(6.0);
        s.tws = Some(10.0);
        s.twd_from = Some(45.0);
        s.wave_from = Some(180.0);
        s.relate();
        assert!(close(s.twa, 45.0));
        assert_eq!(s.tack, Some(Tack::Starboard));
        assert_eq!(s.bsp_corrected, None);
        assert_eq!(s.twa_corrected, None);
        assert!(close(s.wave_angle, 180.0));
        s.heading = Some(90.0);
        s.twd_from = Some(0.0);
        s.relate();
        assert!(close(s.twa, 90.0));
        assert_eq!(s.tack, Some(Tack::Port));
    }

    /// Hand-computed (D13): heading 0° at 6 kn in 1 kn of current toward
    /// 090°. Through the water (-1, 6): BSP √37, heading 360 − atan(1/6) =
    /// 350.537677792°. Wind 10 kn from 0°, i.e. (0, −10) toward, minus the
    /// current: (−1, −10), √101 kn from atan(1/10) = 5.710593137°. TWA
    /// through the water 5.710593137 + 9.462322208 = 15.172915345°,
    /// starboard. Waves from 350.537677792° are dead ahead of the bow.
    #[test]
    fn a_current_is_taken_out_of_the_motion_and_the_wind() {
        let mut s = sample();
        s.heading = Some(0.0);
        s.speed = Some(6.0);
        s.tws = Some(10.0);
        s.twd_from = Some(0.0);
        s.current_speed = Some(1.0);
        s.current_toward = Some(90.0);
        s.wave_from = Some(350.537_677_792);
        s.relate();
        // Speeds are kept to a micro-knot (canonical).
        assert!((s.bsp_corrected.unwrap() - 37f64.sqrt()).abs() <= 5e-7);
        assert!(close(s.heading_corrected, 350.537_677_791_974_9));
        assert!((s.tws_corrected.unwrap() - 101f64.sqrt()).abs() <= 5e-7);
        assert!(close(s.twd_from_corrected, 5.710_593_137_499_643));
        assert!(close(s.twa_corrected, 15.172_915_345_524_7));
        assert_eq!(s.tack_corrected, Some(Tack::Starboard));
        assert!(s.wave_angle.is_some_and(|a| a < 1e-6), "{:?}", s.wave_angle);
        // Ground values are untouched by the current.
        assert!(close(s.twa, 0.0));
        assert_eq!(s.tack, None, "head to wind is neither tack");
    }

    /// Invariant 5, review round 1: what `relate` derives through libm is
    /// rounded to canonical precision, so a last-bit difference between
    /// platforms' sin, cos or atan2 cannot reach the blend or an export.
    #[test]
    fn related_values_are_canonical() {
        use crate::canonical::{degrees, knots};
        let mut s = sample();
        s.heading = Some(12.345_678_9);
        s.speed = Some(6.543_21);
        s.tws = Some(14.37);
        s.twd_from = Some(211.3);
        s.current_speed = Some(1.23);
        s.current_toward = Some(77.7);
        s.wave_from = Some(190.1);
        s.relate();
        for v in [
            s.twa,
            s.heading_corrected,
            s.twd_from_corrected,
            s.twa_corrected,
            s.wave_angle,
        ] {
            let v = v.expect("derived");
            assert_eq!(v.to_bits(), degrees(v).to_bits(), "{v}");
            assert!((0.0..360.0).contains(&v));
        }
        for v in [s.bsp_corrected, s.tws_corrected] {
            let v = v.expect("derived");
            assert_eq!(v.to_bits(), knots(v).to_bits(), "{v}");
        }
    }

    /// Clearing leaves nothing of the fetch: no raw, corrected or
    /// provenance value, and not fetched.
    #[test]
    fn clearing_the_environment_leaves_only_the_motion() {
        let mut s = sample();
        s.heading = Some(0.0);
        s.speed = Some(6.0);
        s.tws = Some(10.0);
        s.twd_from = Some(0.0);
        s.hs_m = Some(1.0);
        s.wave_from = Some(10.0);
        s.current_speed = Some(1.0);
        s.current_toward = Some(90.0);
        s.wind_dataset = Some(0);
        s.wave_dataset = Some(1);
        s.current_dataset = Some(2);
        s.env_fetched = true;
        s.relate();
        s.clear_env();
        let mut bare = sample();
        bare.heading = Some(0.0);
        bare.speed = Some(6.0);
        assert_eq!(s, bare);
    }

    /// Without a heading there is no angle, and a missing speed leaves the
    /// water track unknown while the wind over the water is still known.
    #[test]
    fn missing_motion_leaves_angles_empty() {
        let mut s = sample();
        s.tws = Some(10.0);
        s.twd_from = Some(0.0);
        s.current_speed = Some(1.0);
        s.current_toward = Some(90.0);
        s.wave_from = Some(0.0);
        s.relate();
        assert_eq!((s.twa, s.tack, s.wave_angle), (None, None, None));
        assert_eq!(s.bsp_corrected, None);
        assert!((s.tws_corrected.unwrap() - 101f64.sqrt()).abs() <= 5e-7);
        assert_eq!(s.twa_corrected, None);
    }

    /// A tracker origin saved before M10 has no model or division; it reads
    /// with neither and writes back byte for byte. One with them keeps them.
    #[test]
    fn a_tracker_origin_without_model_or_division_saves_unchanged() {
        let old = r#"{"type":"tracker","tracker":"yellow_brick","event_url":"u","event_title":"T","boat_id":"1","boat_name":"B","sail_no":null,"race_start":1,"race_finish":null}"#;
        let origin: TrackOrigin = serde_json::from_str(old).expect("reads");
        assert_eq!(serde_json::to_string(&origin).expect("writes"), old);
        let TrackOrigin::Tracker {
            tracker,
            event_url,
            event_title,
            boat_id,
            boat_name,
            sail_no,
            race_start,
            race_finish,
            ..
        } = origin
        else {
            panic!("a tracker origin");
        };
        let new = TrackOrigin::Tracker {
            tracker,
            event_url,
            event_title,
            boat_id,
            boat_name,
            sail_no,
            model: Some("IMOCA60".to_owned()),
            division: Some("IRC 2".to_owned()),
            race_start,
            race_finish,
        };
        let json = serde_json::to_string(&new).expect("writes");
        assert!(
            json.contains(r#""model":"IMOCA60","division":"IRC 2""#),
            "{json}"
        );
        assert_eq!(
            serde_json::from_str::<TrackOrigin>(&json).expect("reads"),
            new
        );
    }
}
