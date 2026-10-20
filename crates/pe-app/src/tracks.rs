//! Tracks from files, and a track's filters and derivation (spec.md 7.1,
//! 7.3, 7.4, 7.6).
//!
//! Importing is two calls, as the dialog needs. [`inspect_track_files`]
//! reads each chosen file and says what it holds: the boats of a GeoJSON
//! file, or a CSV's header, first rows and a guessed column mapping (with
//! the boats that mapping gives, when it reads). The user confirms the
//! mapping and picks boats, and [`import_track_files`] reads the files again
//! with those answers and adds one source per boat as **one** history entry.
//! Every parse runs outside the session lock, as polar files do; only the
//! id allocation and the command are inside it.

use std::path::Path;

use pe_core::command::Command;
use pe_core::source::{
    DirectionRange, Range, SampleFilters, Source, TimeWindow, WaveDirectionFilter, WaveSector,
};
use pe_core::track::{DerivationSettings, PreferValues, Track, TrackOrigin, ValueOrigin};
use pe_core::{SampleId, SourceId, SourceKind, TrackId};
use pe_tracks::csv::{CsvGuess, CsvMapping, CsvTable, SpeedUnit, guess, parse_table, read_csv};
use pe_tracks::error::MAX_FILE_BYTES;
use pe_tracks::time::TimeFormat;
use pe_tracks::{ImportReport, RawTrack, Reason, TrackFileError, build_track};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::edit;
use crate::error::{AppError, Result};
use crate::projects::ProjectSummary;

/// The history label of a one-track import: an English key to translate.
pub const IMPORT_ONE: &str = "Import track";
/// The history label of a multi-track import.
pub const IMPORT_MANY: &str = "Import tracks";
/// How many CSV rows the mapping dialog previews.
pub const PREVIEW_ROWS: usize = 8;

// ------------------------------------------------------------ summaries

/// A track's filters as the Tracks section edits them (spec.md 7.6): every
/// one, the environment's included (M9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "TrackFilters.ts")]
pub struct TrackFilters {
    /// Start of the time window, UTC epoch seconds; null for open.
    pub time_start: Option<i64>,
    /// End of the time window, UTC epoch seconds; null for open.
    pub time_end: Option<i64>,
    /// Minimum boat speed, knots.
    pub min_bsp: Option<f64>,
    /// Maximum boat speed, knots.
    pub max_bsp: Option<f64>,
    /// Manoeuvre threshold: heading change between neighbours, degrees.
    pub max_heading_change: Option<f64>,
    /// Maximum apparent wind bearing change, degrees; null disables.
    #[serde(default)]
    pub max_awa_change: Option<f64>,
    /// Maximum true wind speed change, knots; null disables.
    #[serde(default)]
    pub max_wind_speed_change: Option<f64>,
    /// Maximum true wind direction change, degrees; null disables.
    #[serde(default)]
    pub max_wind_direction_change: Option<f64>,
    /// Heading through the water kept, compass degrees clockwise from
    /// `heading_from` to `heading_to`; both null for no bound.
    #[serde(default)]
    pub heading_from: Option<f64>,
    /// See `heading_from`.
    #[serde(default)]
    pub heading_to: Option<f64>,
    /// Course over the ground kept, compass degrees clockwise.
    #[serde(default)]
    pub cog_from: Option<f64>,
    /// See `cog_from`.
    #[serde(default)]
    pub cog_to: Option<f64>,
    /// VMG range, knots: BSP × cos TWA, negative downwind.
    #[serde(default)]
    pub vmg_min: Option<f64>,
    /// See `vmg_min`.
    #[serde(default)]
    pub vmg_max: Option<f64>,
    /// True wind direction kept, compass degrees clockwise.
    #[serde(default)]
    pub twd_from: Option<f64>,
    /// See `twd_from`.
    #[serde(default)]
    pub twd_to: Option<f64>,
    /// Leave out the sample on either side of each tack and gybe.
    #[serde(default)]
    pub exclude_tacks: bool,
    /// True wind speed range, knots.
    pub tws_min: Option<f64>,
    /// See `tws_min`.
    pub tws_max: Option<f64>,
    /// True wind angle range, degrees.
    pub twa_min: Option<f64>,
    /// See `twa_min`.
    pub twa_max: Option<f64>,
    /// Significant wave height range, metres.
    pub hs_min: Option<f64>,
    /// See `hs_min`.
    pub hs_max: Option<f64>,
    /// Current speed range, knots.
    pub current_min: Option<f64>,
    /// See `current_min`.
    pub current_max: Option<f64>,
    /// The wave-direction filter: `"off"`, `"sectors"`, `"relative"` (off
    /// the bow, `wave_min`–`wave_max`) or `"absolute"` (from
    /// `wave_from` clockwise to `wave_to`).
    pub wave_mode: String,
    /// The sectors kept: `"head"`, `"bow"`, `"beam"`, `"quarter"`,
    /// `"following"`.
    pub wave_sectors: Vec<String>,
    /// Relative wave angle range, degrees 0–180.
    pub wave_min: Option<f64>,
    /// See `wave_min`.
    pub wave_max: Option<f64>,
    /// Absolute wave direction range start, degrees "from".
    pub wave_from: Option<f64>,
    /// Absolute wave direction range end.
    pub wave_to: Option<f64>,
    /// Leave out samples whose current has no tide (spec.md 7.5).
    pub exclude_no_tide: bool,
    /// Require complete wave data.
    #[serde(default)]
    pub exclude_unknown_wave: bool,
    /// Require complete current data.
    #[serde(default)]
    pub exclude_unknown_current: bool,
}

const SECTORS: [(WaveSector, &str); 5] = [
    (WaveSector::Head, "head"),
    (WaveSector::Bow, "bow"),
    (WaveSector::Beam, "beam"),
    (WaveSector::Quarter, "quarter"),
    (WaveSector::Following, "following"),
];

fn range(min: Option<f64>, max: Option<f64>) -> Option<Range> {
    (min.is_some() || max.is_some()).then_some(Range { min, max })
}

fn bounds(range: Option<&Range>) -> (Option<f64>, Option<f64>) {
    range.map_or((None, None), |r| (r.min, r.max))
}

/// A compass sector from two bounds; none without both.
fn sector(from: Option<f64>, to: Option<f64>) -> Result<Option<DirectionRange>> {
    match (from, to) {
        (Some(from), Some(to)) => Ok(Some(DirectionRange { from, to })),
        (None, None) => Ok(None),
        _ => Err(AppError::BadOption {
            field: "Direction range",
            value: "needs both its bounds".to_owned(),
        }),
    }
}

fn sector_bounds(range: Option<&DirectionRange>) -> (Option<f64>, Option<f64>) {
    range.map_or((None, None), |r| (Some(r.from), Some(r.to)))
}

impl TrackFilters {
    /// The editable part of a track's filters.
    pub fn of(filters: &SampleFilters) -> Self {
        let window = filters.time_window.clone().unwrap_or_default();
        Self {
            time_start: window.start,
            time_end: window.end,
            min_bsp: filters.min_bsp_kn,
            max_bsp: filters.max_bsp_kn,
            max_heading_change: filters.max_heading_change_deg,
            max_awa_change: filters.max_awa_change_deg,
            max_wind_speed_change: filters.max_wind_speed_change_kn,
            max_wind_direction_change: filters.max_wind_direction_change_deg,
            heading_from: sector_bounds(filters.heading_deg.as_ref()).0,
            heading_to: sector_bounds(filters.heading_deg.as_ref()).1,
            cog_from: sector_bounds(filters.cog_deg.as_ref()).0,
            cog_to: sector_bounds(filters.cog_deg.as_ref()).1,
            vmg_min: bounds(filters.vmg_kn.as_ref()).0,
            vmg_max: bounds(filters.vmg_kn.as_ref()).1,
            twd_from: sector_bounds(filters.twd_deg.as_ref()).0,
            twd_to: sector_bounds(filters.twd_deg.as_ref()).1,
            exclude_tacks: filters.exclude_tacks,
            tws_min: bounds(filters.tws_kn.as_ref()).0,
            tws_max: bounds(filters.tws_kn.as_ref()).1,
            twa_min: bounds(filters.twa_deg.as_ref()).0,
            twa_max: bounds(filters.twa_deg.as_ref()).1,
            hs_min: bounds(filters.wave_height_m.as_ref()).0,
            hs_max: bounds(filters.wave_height_m.as_ref()).1,
            current_min: bounds(filters.current_speed_kn.as_ref()).0,
            current_max: bounds(filters.current_speed_kn.as_ref()).1,
            wave_mode: match &filters.wave_direction {
                None => "off",
                Some(WaveDirectionFilter::Sectors { .. }) => "sectors",
                Some(WaveDirectionFilter::Relative { .. }) => "relative",
                Some(WaveDirectionFilter::Cog { .. }) => "cog",
                Some(WaveDirectionFilter::Absolute { .. }) => "absolute",
            }
            .to_owned(),
            wave_sectors: match &filters.wave_direction {
                Some(WaveDirectionFilter::Sectors { sectors }) => SECTORS
                    .iter()
                    .filter(|(s, _)| sectors.contains(s))
                    .map(|(_, name)| (*name).to_owned())
                    .collect(),
                _ => Vec::new(),
            },
            wave_min: match &filters.wave_direction {
                Some(
                    WaveDirectionFilter::Relative { range } | WaveDirectionFilter::Cog { range },
                ) => range.min,
                _ => None,
            },
            wave_max: match &filters.wave_direction {
                Some(
                    WaveDirectionFilter::Relative { range } | WaveDirectionFilter::Cog { range },
                ) => range.max,
                _ => None,
            },
            wave_from: match &filters.wave_direction {
                Some(WaveDirectionFilter::Absolute { range }) => Some(range.from),
                _ => None,
            },
            wave_to: match &filters.wave_direction {
                Some(WaveDirectionFilter::Absolute { range }) => Some(range.to),
                _ => None,
            },
            exclude_no_tide: filters.exclude_no_tide,
            exclude_unknown_wave: filters.exclude_unknown_wave,
            exclude_unknown_current: filters.exclude_unknown_current,
        }
    }

    fn wave_direction(&self) -> Result<Option<WaveDirectionFilter>> {
        Ok(match self.wave_mode.as_str() {
            "off" => None,
            "sectors" => {
                let mut sectors = Vec::new();
                for name in &self.wave_sectors {
                    let (sector, _) = SECTORS.iter().find(|(_, n)| n == name).ok_or_else(|| {
                        AppError::BadOption {
                            field: "Wave sector",
                            value: name.clone(),
                        }
                    })?;
                    if !sectors.contains(sector) {
                        sectors.push(*sector);
                    }
                }
                sectors.sort();
                Some(WaveDirectionFilter::Sectors { sectors })
            }
            "relative" => Some(WaveDirectionFilter::Relative {
                range: Range {
                    min: self.wave_min,
                    max: self.wave_max,
                },
            }),
            "cog" => Some(WaveDirectionFilter::Cog {
                range: Range {
                    min: self.wave_min,
                    max: self.wave_max,
                },
            }),
            "absolute" => Some(WaveDirectionFilter::Absolute {
                range: DirectionRange {
                    from: self.wave_from.unwrap_or(0.0),
                    to: self.wave_to.unwrap_or(360.0),
                },
            }),
            other => {
                return Err(AppError::BadOption {
                    field: "Wave direction filter",
                    value: other.to_owned(),
                });
            }
        })
    }

    /// The filters this edit gives. Every filter is edited here, so none of
    /// `_current` carries over; the argument stays so a caller cannot forget
    /// what it replaces.
    pub fn applied_to(&self, _current: &SampleFilters) -> Result<SampleFilters> {
        let window = TimeWindow {
            start: self.time_start,
            end: self.time_end,
        };
        let filters = SampleFilters {
            time_window: (window.start.is_some() || window.end.is_some()).then_some(window),
            min_bsp_kn: self.min_bsp,
            max_bsp_kn: self.max_bsp,
            max_heading_change_deg: self.max_heading_change,
            max_awa_change_deg: self.max_awa_change,
            max_wind_speed_change_kn: self.max_wind_speed_change,
            max_wind_direction_change_deg: self.max_wind_direction_change,
            heading_deg: sector(self.heading_from, self.heading_to)?,
            cog_deg: sector(self.cog_from, self.cog_to)?,
            vmg_kn: range(self.vmg_min, self.vmg_max),
            twd_deg: sector(self.twd_from, self.twd_to)?,
            exclude_tacks: self.exclude_tacks,
            tws_kn: range(self.tws_min, self.tws_max),
            twa_deg: range(self.twa_min, self.twa_max),
            wave_height_m: range(self.hs_min, self.hs_max),
            current_speed_kn: range(self.current_min, self.current_max),
            wave_direction: self.wave_direction()?,
            exclude_no_tide: self.exclude_no_tide,
            exclude_unknown_wave: self.exclude_unknown_wave,
            exclude_unknown_current: self.exclude_unknown_current,
        };
        filters.validate()?;
        Ok(filters)
    }
}

/// A track source as the Tracks section lists it (spec.md 7.1).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackSummary.ts")]
pub struct TrackSummary {
    /// `"file"` or `"tracker"`.
    pub origin: String,
    /// Boat name, when known.
    pub boat_name: Option<String>,
    /// The event's title (trackers) or the file name (files).
    pub event_title: String,
    /// First fix, UTC epoch seconds; null for a track with no fixes.
    pub start: Option<i64>,
    /// Last fix, UTC epoch seconds.
    pub end: Option<i64>,
    /// Samples (one per fix).
    pub samples: u32,
    /// Samples the filters take out.
    pub filtered: u32,
    /// Samples excluded by hand.
    pub excluded: u32,
    /// Samples the blend uses: neither excluded nor filtered out.
    pub used: u32,
    /// Samples with a place in the polar: wind found (M9).
    pub with_wind: u32,
    /// `"not_fetched"`, `"ready"`, `"partial"` or `"failed"` (spec.md 7.1).
    pub env_status: String,
    /// Samples the environment fetch has answered for.
    pub env_fetched: u32,
    /// `"hourly"` or `"three_hourly"`: how the last fetch sampled wind and
    /// waves; null before any.
    pub env_interval: Option<String>,
    /// Samples whose current came from a dataset marked without tide.
    pub no_tide: u32,
    /// Longest gap a central difference may span, seconds.
    pub max_gap_s: i64,
    /// The given or the derived heading first: `"given"` or `"derived"`.
    pub prefer_heading: String,
    /// The given or the derived speed first: `"given"` or `"derived"`.
    pub prefer_speed: String,
    /// Use downloaded weather even when the track supplies wind.
    pub downloaded_wind_only: bool,
    /// Fixes containing both supplied true wind speed and direction.
    pub supplied_wind: u32,
    /// Fixes that give a heading (a course over the ground).
    pub supplied_heading: u32,
    /// Fixes that give a speed (over the ground).
    pub supplied_speed: u32,
    /// The editable filters.
    pub filters: TrackFilters,
    /// Whether any environment filter (wind, waves, current) is set.
    pub environment_filters: bool,
}

/// Counts that fit the wire.
pub(crate) fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl TrackSummary {
    /// The summary of a track source, from its samples as placed and
    /// filtered (`crate::derived`).
    pub fn of(source: &Source, track: &Track, placed: &crate::derived::TrackDerived) -> Self {
        let (origin, boat_name, event_title) = match &track.origin {
            TrackOrigin::Tracker {
                event_title,
                boat_name,
                ..
            } => ("tracker", Some(boat_name.clone()), event_title.clone()),
            TrackOrigin::File { name, boat_name } => ("file", boat_name.clone(), name.clone()),
        };
        let filters = &source.overlay.filters;
        let excluded = &source.overlay.excluded_samples;
        let (mut filtered, mut hand, mut used, mut with_wind) = (0, 0, 0, 0);
        let no_tide = track
            .samples
            .iter()
            .filter(|s| {
                s.current_dataset
                    .and_then(|i| track.env_meta.datasets.get(usize::from(i)))
                    .is_some_and(|d| d.has_tide == Some(false))
            })
            .count();
        for ((sample, out), point) in track
            .samples
            .iter()
            .zip(&placed.filtered)
            .zip(&placed.points)
        {
            let is_excluded = excluded.binary_search(&sample.id).is_ok();
            filtered += usize::from(*out);
            hand += usize::from(is_excluded);
            used += usize::from(!out && !is_excluded);
            with_wind += usize::from(point.is_some());
        }
        Self {
            origin: origin.to_owned(),
            boat_name,
            event_title,
            start: track.fixes.first().map(|f| f.t),
            end: track.fixes.last().map(|f| f.t),
            samples: count(track.samples.len()),
            filtered: count(filtered),
            excluded: count(hand),
            used: count(used),
            with_wind: count(with_wind),
            env_status: match track.env_meta.status {
                pe_core::track::EnvStatus::NotFetched => "not_fetched",
                pe_core::track::EnvStatus::Ready => "ready",
                pe_core::track::EnvStatus::Partial => "partial",
                pe_core::track::EnvStatus::Failed => "failed",
            }
            .to_owned(),
            env_fetched: count(track.samples.iter().filter(|s| s.env_fetched).count()),
            env_interval: track
                .env_meta
                .interval_s
                .map(|s| if s == 3600 { "hourly" } else { "three_hourly" }.to_owned()),
            no_tide: count(no_tide),
            max_gap_s: track.derivation.max_gap_s,
            downloaded_wind_only: track.derivation.downloaded_wind_only,
            supplied_wind: count(
                track
                    .fixes
                    .iter()
                    .filter(|f| f.tws.zip(f.twd_from).is_some())
                    .count(),
            ),
            supplied_heading: count(track.fixes.iter().filter(|f| f.cog.is_some()).count()),
            supplied_speed: count(track.fixes.iter().filter(|f| f.sog.is_some()).count()),
            prefer_heading: prefer_name(track.derivation.prefer_heading).to_owned(),
            prefer_speed: prefer_name(track.derivation.prefer_speed).to_owned(),
            filters: TrackFilters::of(filters),
            environment_filters: filters.wave_height_m.is_some()
                || filters.wave_direction.is_some()
                || filters.current_speed_kn.is_some()
                || filters.tws_kn.is_some()
                || filters.twa_deg.is_some()
                || filters.max_awa_change_deg.is_some()
                || filters.max_wind_speed_change_kn.is_some()
                || filters.max_wind_direction_change_deg.is_some()
                || filters.exclude_no_tide
                || filters.exclude_unknown_wave
                || filters.exclude_unknown_current
                || filters.twd_deg.is_some()
                || filters.vmg_kn.is_some()
                || filters.exclude_tacks,
        }
    }
}

// --------------------------------------------------------------- import

/// A column mapping as the CSV dialog shows and edits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "CsvMappingInput.ts")]
pub struct CsvMappingInput {
    /// Time column, 0-based; null when not chosen.
    pub time: Option<u32>,
    /// Latitude column.
    pub lat: Option<u32>,
    /// Longitude column.
    pub lon: Option<u32>,
    /// Heading or COG column; optional.
    pub heading: Option<u32>,
    /// SOG or boat speed column; optional.
    pub speed: Option<u32>,
    /// Supplied true wind speed column; optional.
    #[serde(default)]
    pub tws: Option<u32>,
    /// Supplied true wind direction column; optional.
    #[serde(default)]
    pub twd: Option<u32>,
    /// Boat column; optional.
    pub boat: Option<u32>,
    /// `"auto"`, `"iso"`, `"epoch_s"`, `"epoch_ms"` or `"custom"`.
    pub time_format: String,
    /// The pattern when `time_format` is `"custom"`.
    pub custom_format: String,
    /// `"kn"`, `"ms"`, `"kmh"` or `"mph"`.
    pub speed_unit: String,
    /// Supplied wind unit, independent of boat speed: kn, ms, kmh or mph.
    #[serde(default)]
    pub wind_speed_unit: String,
}

fn index(value: Option<u32>) -> Option<usize> {
    value.map(|v| v as usize)
}

impl CsvMappingInput {
    fn of(g: &CsvGuess) -> Self {
        let col = |v: Option<usize>| v.map(count);
        Self {
            time: col(g.time),
            lat: col(g.lat),
            lon: col(g.lon),
            heading: col(g.heading),
            speed: col(g.speed),
            tws: col(g.tws),
            twd: col(g.twd),
            boat: col(g.boat),
            time_format: match &g.time_format {
                None | Some(TimeFormat::Auto) => "auto",
                Some(TimeFormat::Iso8601) => "iso",
                Some(TimeFormat::EpochSeconds) => "epoch_s",
                Some(TimeFormat::EpochMillis) => "epoch_ms",
                Some(TimeFormat::Custom(_)) => "custom",
            }
            .to_owned(),
            custom_format: String::new(),
            speed_unit: match g.speed_unit {
                SpeedUnit::Knots => "kn",
                SpeedUnit::MetresPerSecond => "ms",
                SpeedUnit::KilometresPerHour => "kmh",
                SpeedUnit::MilesPerHour => "mph",
            }
            .to_owned(),
            wind_speed_unit: match g.wind_speed_unit {
                SpeedUnit::Knots => "kn",
                SpeedUnit::MetresPerSecond => "ms",
                SpeedUnit::KilometresPerHour => "kmh",
                SpeedUnit::MilesPerHour => "mph",
            }
            .to_owned(),
        }
    }

    /// The mapping, once time, latitude and longitude are all chosen.
    fn mapping(&self) -> std::result::Result<CsvMapping, Reason> {
        let (Some(time), Some(lat), Some(lon)) = (self.time, self.lat, self.lon) else {
            return Err(Reason::MissingColumn(0));
        };
        let time_format = match self.time_format.as_str() {
            "iso" => TimeFormat::Iso8601,
            "epoch_s" => TimeFormat::EpochSeconds,
            "epoch_ms" => TimeFormat::EpochMillis,
            "custom" => TimeFormat::Custom(self.custom_format.clone()),
            _ => TimeFormat::Auto,
        };
        let speed_unit = match self.speed_unit.as_str() {
            "ms" => SpeedUnit::MetresPerSecond,
            "kmh" => SpeedUnit::KilometresPerHour,
            "mph" => SpeedUnit::MilesPerHour,
            _ => SpeedUnit::Knots,
        };
        Ok(CsvMapping {
            time: time as usize,
            lat: lat as usize,
            lon: lon as usize,
            heading: index(self.heading),
            speed: index(self.speed),
            tws: index(self.tws),
            twd: index(self.twd),
            wind_speed_unit: match self.wind_speed_unit.as_str() {
                "ms" => SpeedUnit::MetresPerSecond,
                "kmh" => SpeedUnit::KilometresPerHour,
                "mph" => SpeedUnit::MilesPerHour,
                _ => SpeedUnit::Knots,
            },
            boat: index(self.boat),
            time_format,
            speed_unit,
        })
    }
}

/// A file that did not import (or inspect), and why.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackImportFailure.ts")]
pub struct TrackImportFailure {
    /// The file name, without its folder.
    pub file: String,
    /// 1-based line, when known.
    pub line: Option<u32>,
    /// 1-based column, when known.
    pub column: Option<u32>,
    /// 0-based GeoJSON feature, when that is the location.
    pub feature: Option<u32>,
    /// A stable code the interface translates: one of `pe_tracks`'
    /// `Reason::code`s, `"unreadable"`, `"no-mapping"` or `"no-boat"`.
    pub reason: String,
    /// The English explanation, for the tooltip.
    pub message: String,
}

impl TrackImportFailure {
    fn of(file: &str, error: &TrackFileError) -> Self {
        Self {
            file: file.to_owned(),
            line: error.line.map(count),
            column: error.column.map(count),
            feature: error.feature.map(count),
            reason: error.reason.code().to_owned(),
            message: format!("{file}, {error}"),
        }
    }

    pub(crate) fn simple(file: &str, reason: &str, message: String) -> Self {
        Self {
            file: file.to_owned(),
            line: None,
            column: None,
            feature: None,
            reason: reason.to_owned(),
            message,
        }
    }
}

/// One boat a file holds.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackBoatPreview.ts")]
pub struct TrackBoatPreview {
    /// The boat's name; empty when the file does not name one.
    pub name: String,
    /// Positions.
    pub fixes: u32,
    /// Earliest time, UTC epoch seconds.
    pub start: i64,
    /// Latest time, UTC epoch seconds.
    pub end: i64,
}

/// A CSV's first rows and a proposed mapping, for the mapping dialog.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "CsvPreview.ts")]
pub struct CsvPreview {
    /// Header names.
    pub header: Vec<String>,
    /// The first rows' cells.
    pub rows: Vec<Vec<String>>,
    /// How many data rows the file has.
    pub row_count: u32,
    /// The proposed (or, on a re-inspection, the given) mapping.
    pub mapping: CsvMappingInput,
}

/// What one chosen file holds (spec.md 7.3).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackFileInspection.ts")]
pub struct TrackFileInspection {
    /// The path as chosen.
    pub path: String,
    /// The file name.
    pub file: String,
    /// `"gpx"`, `"geojson"` or `"csv"`.
    pub kind: String,
    /// The boats it holds, in the order they first appear; empty when it
    /// did not read.
    pub boats: Vec<TrackBoatPreview>,
    /// For a CSV, the preview and mapping.
    pub csv: Option<CsvPreview>,
    /// Why it did not read, when it did not.
    pub failure: Option<TrackImportFailure>,
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn read_text(path: &Path, file: &str) -> std::result::Result<String, TrackImportFailure> {
    let unreadable = |why: String| {
        TrackImportFailure::simple(file, "unreadable", format!("Could not read {file}: {why}"))
    };
    let size = std::fs::metadata(path)
        .map_err(|e| unreadable(e.to_string()))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(TrackImportFailure::of(
            file,
            &TrackFileError::whole(Reason::TooLarge),
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| unreadable(e.to_string()))?;
    String::from_utf8(bytes).or_else(|e| {
        // Latin-1 is common in older logger exports; every byte is a char.
        let bytes = e.into_bytes();
        if bytes.contains(&0) {
            Err(TrackImportFailure::of(
                file,
                &TrackFileError::whole(Reason::NotText),
            ))
        } else {
            Ok(bytes.iter().map(|b| char::from(*b)).collect())
        }
    })
}

/// Whether a file is GeoJSON: by extension, else by its first character.
fn is_geojson(path: &Path, text: &str) -> bool {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "geojson" | "json" => true,
        "csv" | "txt" | "tsv" => false,
        _ => text.trim_start().starts_with('{'),
    }
}

/// GPX is identified by its extension or root tag, since exports commonly use
/// a namespace-prefixed `<gpx>` root and may have any file name extension.
fn is_gpx(path: &Path, text: &str) -> bool {
    path.extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("gpx"))
        || text.trim_start().starts_with("<?xml") && text.contains("<gpx")
        || text.trim_start().starts_with("<gpx")
}

fn boats_of(raw: &[RawTrack]) -> Vec<TrackBoatPreview> {
    raw.iter()
        .map(|track| TrackBoatPreview {
            name: track.boat.clone().unwrap_or_default(),
            fixes: count(track.fixes.len()),
            start: track.fixes.iter().map(|f| f.t).min().unwrap_or(0),
            end: track.fixes.iter().map(|f| f.t).max().unwrap_or(0),
        })
        .collect()
}

fn no_mapping(file: &str) -> TrackImportFailure {
    TrackImportFailure::simple(
        file,
        "no-mapping",
        format!("{file}: choose the time, latitude and longitude columns"),
    )
}

/// The raw tracks a file gives with a mapping (for CSV), or its failure.
fn read_file(
    file: &str,
    text: &str,
    table: Option<&CsvTable>,
    mapping: Option<&CsvMappingInput>,
) -> std::result::Result<Vec<RawTrack>, TrackImportFailure> {
    if let Some(table) = table {
        let mapping = mapping.ok_or_else(|| no_mapping(file))?;
        let mapping = mapping.mapping().map_err(|_| no_mapping(file))?;
        return read_csv(table, &mapping).map_err(|e| TrackImportFailure::of(file, &e));
    }
    if is_gpx(Path::new(file), text) {
        return pe_tracks::gpx::read_gpx(text.as_bytes())
            .map_err(|e| TrackImportFailure::of(file, &e));
    }
    pe_tracks::geojson::read_geojson(text.as_bytes()).map_err(|e| TrackImportFailure::of(file, &e))
}

/// Inspects one file; `mapping` re-reads a CSV with the user's mapping.
pub fn inspect(path: &str, mapping: Option<&CsvMappingInput>) -> TrackFileInspection {
    let p = Path::new(path);
    let file = file_name(p);
    let mut out = TrackFileInspection {
        path: path.to_owned(),
        file: file.clone(),
        kind: "csv".to_owned(),
        boats: Vec::new(),
        csv: None,
        failure: None,
    };
    let text = match read_text(p, &file) {
        Ok(text) => text,
        Err(failure) => {
            out.failure = Some(failure);
            return out;
        }
    };
    if is_gpx(p, &text) {
        out.kind = "gpx".to_owned();
        match read_file(&file, &text, None, None) {
            Ok(raw) => out.boats = boats_of(&raw),
            Err(failure) => out.failure = Some(failure),
        }
        return out;
    }
    if is_geojson(p, &text) {
        out.kind = "geojson".to_owned();
        match read_file(&file, &text, None, None) {
            Ok(raw) => out.boats = boats_of(&raw),
            Err(failure) => out.failure = Some(failure),
        }
        return out;
    }
    let table = match parse_table(&text) {
        Ok(table) => table,
        Err(e) => {
            out.failure = Some(TrackImportFailure::of(&file, &e));
            return out;
        }
    };
    let mapping = mapping
        .cloned()
        .unwrap_or_else(|| CsvMappingInput::of(&guess(&table)));
    match read_file(&file, &text, Some(&table), Some(&mapping)) {
        Ok(raw) => out.boats = boats_of(&raw),
        Err(failure) => out.failure = Some(failure),
    }
    out.csv = Some(CsvPreview {
        header: table.header.clone(),
        rows: table
            .rows
            .iter()
            .take(PREVIEW_ROWS)
            .map(|r| r.cells.clone())
            .collect(),
        row_count: count(table.rows.len()),
        mapping,
    });
    out
}

/// Reads each chosen file and says what it holds (spec.md 7.3).
#[tauri::command]
pub async fn inspect_track_files(paths: Vec<String>) -> Result<Vec<TrackFileInspection>> {
    Ok(paths.iter().map(|p| inspect(p, None)).collect())
}

/// Re-reads one CSV with the user's column mapping.
#[tauri::command]
pub async fn inspect_csv_track(
    path: String,
    mapping: CsvMappingInput,
) -> Result<TrackFileInspection> {
    Ok(inspect(&path, Some(&mapping)))
}

/// One file to import, with the answers the dialog collected.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export_to = "TrackFileRequest.ts")]
pub struct TrackFileRequest {
    /// The path.
    pub path: String,
    /// For a CSV, the confirmed mapping.
    pub mapping: Option<CsvMappingInput>,
    /// The boats to import, by name (empty for the unnamed one); null for
    /// every boat in the file.
    pub boats: Option<Vec<String>>,
}

/// One track an import added.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackImportLine.ts")]
pub struct TrackImportLine {
    /// The track source it became, for the environment fetch that follows.
    pub source_id: u64,
    /// The file it came from.
    pub file: String,
    /// The source's label.
    pub label: String,
    /// Positions kept.
    pub fixes: u32,
    /// Positions that were out of time order and were sorted.
    pub out_of_order: u32,
    /// Positions merged into another at the same time.
    pub duplicates: u32,
    /// Headings the file gave.
    pub heading_given: u32,
    /// Headings derived from neighbours.
    pub heading_derived: u32,
    /// Speeds the file gave.
    pub speed_given: u32,
    /// Speeds derived from neighbours.
    pub speed_derived: u32,
}

/// What an import did (spec.md 7.3, 7.4).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackImportResult.ts")]
pub struct TrackImportResult {
    /// The project after the import.
    pub project: ProjectSummary,
    /// Every track added, in order.
    pub imported: Vec<TrackImportLine>,
    /// The files (or boats) that did not import.
    pub failures: Vec<TrackImportFailure>,
}

/// A track read and derived outside the lock, waiting for its ids.
pub(crate) struct Pending {
    /// The file (or, for a tracker, the event) it came from.
    pub file: String,
    /// The source's label.
    pub label: String,
    /// The track, with placeholder ids.
    pub track: Track,
    /// What building it found.
    pub report: ImportReport,
    /// The source's starting filters: the defaults, and for a tracker boat
    /// its start and finish as the time window (spec.md 7.6).
    pub filters: SampleFilters,
}

fn file_stem(file: &str) -> String {
    Path::new(file)
        .file_stem()
        .map_or_else(|| file.to_owned(), |s| s.to_string_lossy().into_owned())
}

/// Reads one requested file into pending tracks.
fn prepare(request: &TrackFileRequest) -> std::result::Result<Vec<Pending>, TrackImportFailure> {
    let path = Path::new(&request.path);
    let file = file_name(path);
    let text = read_text(path, &file)?;
    let raw = if is_gpx(path, &text) || is_geojson(path, &text) {
        read_file(&file, &text, None, None)?
    } else {
        let table = parse_table(&text).map_err(|e| TrackImportFailure::of(&file, &e))?;
        let mapping = request
            .mapping
            .clone()
            .unwrap_or_else(|| CsvMappingInput::of(&guess(&table)));
        read_file(&file, &text, Some(&table), Some(&mapping))?
    };
    let chosen: Vec<RawTrack> = raw
        .into_iter()
        .filter(|t| {
            request
                .boats
                .as_ref()
                .is_none_or(|boats| boats.contains(&t.boat.clone().unwrap_or_default()))
        })
        .collect();
    if chosen.is_empty() {
        return Err(TrackImportFailure::simple(
            &file,
            "no-boat",
            format!("{file}: no boat was chosen"),
        ));
    }
    Ok(chosen
        .into_iter()
        .map(|raw| {
            // A boat is named after itself; an unnamed one after its file.
            let label = raw.boat.clone().unwrap_or_else(|| file_stem(&file));
            let origin = TrackOrigin::File {
                name: file.clone(),
                boat_name: raw.boat.clone(),
            };
            let mut next = 0u64;
            let (track, report) = build_track(
                TrackId(0),
                origin,
                raw.fixes,
                DerivationSettings::default(),
                || {
                    next += 1;
                    SampleId(next)
                },
            );
            Pending {
                file: file.clone(),
                label,
                track,
                report,
                filters: SampleFilters::default(),
            }
        })
        .collect())
}

/// Imports tracks from files, one source per boat, as one undoable change.
#[tauri::command]
pub async fn import_track_files(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    files: Vec<TrackFileRequest>,
) -> Result<TrackImportResult> {
    let state = state.scoped(boat_context);
    import(&state, &files)
}

/// [`import_track_files`] without a Tauri handle.
pub fn import(state: &AppState, files: &[TrackFileRequest]) -> Result<TrackImportResult> {
    state.with_session(|session| session.require_open().map(|_| ()))?;

    let mut pending = Vec::new();
    let mut failures = Vec::new();
    for request in files {
        match prepare(request) {
            Ok(tracks) => pending.extend(tracks),
            Err(failure) => failures.push(failure),
        }
    }

    add_tracks(state, pending, failures)
}

/// Adds read tracks as one source each, with the next palette colours, as
/// **one** history entry, and says what was added.
pub(crate) fn add_tracks(
    state: &AppState,
    pending: Vec<Pending>,
    failures: Vec<TrackImportFailure>,
) -> Result<TrackImportResult> {
    add_tracks_to_project(state, pending, failures, None)
}

/// A database import reads disk outside the lock. Check the destination and
/// duplicates under the same lock that commits the sources.
pub(crate) fn add_tracks_to_project(
    state: &AppState,
    pending: Vec<Pending>,
    failures: Vec<TrackImportFailure>,
    expected_project: Option<pe_core::ProjectId>,
) -> Result<TrackImportResult> {
    state.with_session(|session| {
        let open = session.require_open()?;
        open.ensure_editable()?;
        if let Some(expected) = expected_project {
            if open.project.id != expected {
                return Err(AppError::Internal(
                    "The project changed during import".into(),
                ));
            }
            if pending.iter().any(|p| {
                open.project.sources.iter().any(
                    |s| matches!(&s.kind,SourceKind::Track{track} if track.origin==p.track.origin),
                )
            }) {
                return Err(AppError::Internal(
                    "This boat track is already in the project".into(),
                ));
            }
        }
        let mut imported = Vec::new();
        if !pending.is_empty() {
            let project = &mut open.project;
            let ids: u64 = pending
                .iter()
                .map(|p| 2 + p.track.samples.len() as u64)
                .sum();
            project.reserve_ids(ids)?;
            let colours = project.next_palette_colours(pending.len());
            let start = project.sources.len();
            let mut commands = Vec::with_capacity(pending.len());
            for (k, (mut p, colour)) in pending.into_iter().zip(colours).enumerate() {
                let source_id = project.allocate_source_id();
                p.track.id = project.allocate_track_id();
                // A new track starts with the project's default statistic
                // (Blend settings, spec.md 12.1); it keeps its own after.
                p.track.statistic = project.blend.default_statistic;
                for sample in &mut p.track.samples {
                    sample.id = project.allocate_sample_id();
                }
                let r = p.report;
                imported.push(TrackImportLine {
                    source_id: source_id.raw(),
                    file: p.file,
                    label: p.label.clone(),
                    fixes: count(r.fixes),
                    out_of_order: count(r.out_of_order),
                    duplicates: count(r.duplicates),
                    heading_given: count(r.heading_given),
                    heading_derived: count(r.heading_derived),
                    speed_given: count(r.speed_given),
                    speed_derived: count(r.speed_derived),
                });
                let mut source = Source::new(
                    source_id,
                    p.label,
                    colour,
                    SourceKind::Track {
                        track: Box::new(p.track),
                    },
                );
                source.overlay.filters = p.filters;
                commands.push(Command::AddSource {
                    index: start + k,
                    source: Box::new(source),
                });
            }
            let label = if commands.len() == 1 {
                IMPORT_ONE
            } else {
                IMPORT_MANY
            };
            open.apply(Command::Batch {
                label: label.to_owned(),
                commands,
            })?;
        }
        Ok(TrackImportResult {
            project: ProjectSummary::of(open),
            imported,
            failures,
        })
    })
}

// ---------------------------------------------------------------- edits

fn track_source(project: &pe_core::Project, id: u64) -> Result<(&Source, &Track)> {
    let source = project
        .source(SourceId(id))
        .ok_or(AppError::Core(pe_core::CoreError::MissingSource(id)))?;
    let track = source.track().ok_or_else(|| AppError::BadOption {
        field: "Track",
        value: source.label.clone(),
    })?;
    Ok((source, track))
}

/// Changes a track's sample filters (spec.md 7.6), undoably.
#[tauri::command]
pub fn set_track_filters(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: u64,
    filters: TrackFilters,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    track_filters_set(&state, id, &filters)
}

/// [`set_track_filters`] without a Tauri handle.
pub fn track_filters_set(
    state: &AppState,
    id: u64,
    filters: &TrackFilters,
) -> Result<ProjectSummary> {
    edit::apply(state, |project| {
        let (source, _) = track_source(project, id)?;
        let before = source.overlay.filters.clone();
        let after = filters.applied_to(&before)?;
        Ok((before != after).then(|| Command::SetSampleFilters {
            source: SourceId(id),
            before: Box::new(before),
            after: Box::new(after),
        }))
    })
}

fn prefer_name(prefer: PreferValues) -> &'static str {
    match prefer {
        PreferValues::Given => "given",
        PreferValues::Derived => "derived",
    }
}

fn prefer_value(field: &'static str, name: &str) -> Result<PreferValues> {
    match name {
        "given" => Ok(PreferValues::Given),
        "derived" => Ok(PreferValues::Derived),
        other => Err(AppError::BadOption {
            field,
            value: other.to_owned(),
        }),
    }
}

/// Changes how a track derives heading and speed (spec.md 7.4), undoably.
/// `prefer_heading` and `prefer_speed` are `"given"` or `"derived"`.
#[tauri::command]
pub fn set_track_derivation(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: u64,
    max_gap_s: i64,
    prefer_heading: String,
    prefer_speed: String,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    track_derivation_set(&state, id, max_gap_s, &prefer_heading, &prefer_speed)
}

/// [`set_track_derivation`] without a Tauri handle.
pub fn track_derivation_set(
    state: &AppState,
    id: u64,
    max_gap_s: i64,
    prefer_heading: &str,
    prefer_speed: &str,
) -> Result<ProjectSummary> {
    let after = DerivationSettings {
        max_gap_s,
        downloaded_wind_only: false,
        prefer_heading: prefer_value("Prefer heading", prefer_heading)?,
        prefer_speed: prefer_value("Prefer speed", prefer_speed)?,
    };
    after.validate()?;
    edit::apply(state, |project| {
        let (_, track) = track_source(project, id)?;
        let after = DerivationSettings {
            downloaded_wind_only: track.derivation.downloaded_wind_only,
            ..after.clone()
        };
        if track.derivation == after {
            return Ok(None);
        }
        Ok(Some(Command::SetDerivation {
            source: SourceId(id),
            before: track.derivation.clone(),
            after: after.clone(),
            motion_before: track.samples.iter().map(|s| s.motion()).collect(),
            motion_after: pe_tracks::rederive(track, &after),
        }))
    })
}

/// Select supplied wind where available, or downloaded weather only.
#[tauri::command]
pub fn set_track_wind(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: u64,
    downloaded_only: bool,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    track_wind_set(&state, id, downloaded_only)
}

/// Undoable wind choice; fetched data and original fixes remain intact.
pub fn track_wind_set(state: &AppState, id: u64, downloaded_only: bool) -> Result<ProjectSummary> {
    edit::apply(state, |project| {
        let (_, track) = track_source(project, id)?;
        if track.derivation.downloaded_wind_only == downloaded_only {
            return Ok(None);
        }
        let mut after = track.derivation.clone();
        after.downloaded_wind_only = downloaded_only;
        let motion = track.samples.iter().map(|s| s.motion()).collect::<Vec<_>>();
        Ok(Some(Command::SetDerivation {
            source: SourceId(id),
            before: track.derivation.clone(),
            after,
            motion_before: motion.clone(),
            motion_after: motion,
        }))
    })
}

// -------------------------------------------------------- hover details

/// One sample as the map's hover shows it (spec.md 9.1).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "SampleDetails.ts")]
pub struct SampleDetails {
    /// The track source.
    pub source_id: u64,
    /// The sample.
    pub sample_id: u64,
    /// UTC epoch seconds.
    pub t: i64,
    /// Latitude, degrees.
    pub lat: f64,
    /// Longitude, degrees.
    pub lon: f64,
    /// Heading over the ground, degrees.
    pub heading: Option<f64>,
    /// `"given"` or `"derived"`.
    pub heading_origin: Option<String>,
    /// Speed over the ground, knots.
    pub speed: Option<f64>,
    /// `"given"` or `"derived"`.
    pub speed_origin: Option<String>,
    /// Boat speed the polar uses, knots (through the water where a current
    /// correction exists).
    pub bsp: Option<f64>,
    /// True wind speed, knots.
    pub tws: Option<f64>,
    /// True wind angle, degrees.
    pub twa: Option<f64>,
    /// Significant wave height, metres.
    pub hs: Option<f64>,
    /// Current speed, knots.
    pub current_speed: Option<f64>,
    /// Current direction, "toward", degrees.
    pub current_toward: Option<f64>,
    /// Whether the filters take it out.
    pub filtered: bool,
    /// Whether it is excluded by hand.
    pub excluded: bool,
}

fn origin_name(origin: Option<ValueOrigin>) -> Option<String> {
    origin.map(|o| {
        match o {
            ValueOrigin::Given => "given",
            ValueOrigin::Derived => "derived",
        }
        .to_owned()
    })
}

/// One sample's details, for the map's hover.
#[tauri::command]
pub fn sample_details(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    source_id: u64,
    sample_id: u64,
) -> Result<SampleDetails> {
    let state = state.scoped(boat_context);
    details(&state, source_id, sample_id)
}

/// [`sample_details`] without a Tauri handle.
pub fn details(state: &AppState, source_id: u64, sample_id: u64) -> Result<SampleDetails> {
    state.with_session(|session| {
        let open = session.require_open()?;
        let use_corrected = open.project.blend.use_corrected;
        let (source, track) = track_source(&open.project, source_id)?;
        let k = track
            .samples
            .iter()
            .position(|s| s.id == SampleId(sample_id))
            .ok_or_else(|| AppError::BadOption {
                field: "Sample",
                value: sample_id.to_string(),
            })?;
        let s = &track.samples[k];
        let point = pe_tracks::polar_point(s, use_corrected);
        let filtered = crate::derived::filtered_samples(&open.project, source, track)
            .get(k)
            .copied()
            .unwrap_or(false);
        Ok(SampleDetails {
            source_id,
            sample_id,
            t: s.t,
            lat: s.lat,
            lon: s.lon,
            heading: s.heading,
            heading_origin: origin_name(s.heading_origin),
            speed: s.speed,
            speed_origin: origin_name(s.speed_origin),
            bsp: pe_tracks::boat_speed(s, use_corrected),
            tws: point.map(|p| p.1).or(s.wind_speed()),
            twa: point.map(|p| p.0).or(s.twa),
            hs: s.hs_m,
            current_speed: s.current_speed,
            current_toward: s.current_toward,
            filtered,
            excluded: source.overlay.excluded_samples.binary_search(&s.id).is_ok(),
        })
    })
}
