//! Sources and the overlays stored beside them (spec.md 4.1, 8).
//!
//! A source's data (`kind`) is immutable once imported. Everything the user
//! changes — colour, visibility, weight, label, exclusions, cell overrides,
//! filters — is stored beside it, and removing every overlay gives the source
//! back exactly (invariant 1).

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{CoreError, Result};
use crate::id::{SampleId, SourceId};
use crate::orc::OrcRecord;
use crate::polar::{PolarFileFormat, PolarGrid};
use crate::track::Track;

/// The weight a new source starts with.
pub const DEFAULT_WEIGHT: f64 = 1.0;
/// The largest weight a source may have (spec.md 8).
pub const MAX_WEIGHT: f64 = 1.0;
/// The fastest boat speed an edit may give a cell, knots: the same bound an
/// imported polar is held to (`pe_polar::MAX_SPEED_KN`, spec.md 6).
pub const MAX_EDIT_BSP_KN: f64 = 60.0;

/// The palette new sources take colours from, in order (spec.md 8).
pub const PALETTE: [&str; 16] = [
    "#4e79a7", "#f28e2b", "#e15759", "#76b7b2", "#59a14f", "#edc948", "#b07aa1", "#ff9da7",
    "#9c755f", "#bab0ac", "#1f77b4", "#d62728", "#2ca02c", "#9467bd", "#17becf", "#bcbd22",
];

/// A colour as `#rrggbb`, lower case.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Colour(String);

impl Colour {
    /// Parses `#rrggbb` (either case), storing it lower case.
    pub fn parse(text: &str) -> Result<Self> {
        let valid = text.len() == 7
            && text.starts_with('#')
            && text[1..].chars().all(|c| c.is_ascii_hexdigit());
        if valid {
            Ok(Self(text.to_ascii_lowercase()))
        } else {
            Err(CoreError::Invalid(format!(
                "{text:?} is not a colour of the form #rrggbb"
            )))
        }
    }

    /// A colour from a literal known to be lower-case `#rrggbb` (the palette,
    /// the defaults); tested in this module.
    pub(crate) fn trusted(text: &str) -> Self {
        Self(text.to_owned())
    }

    /// The colour as `#rrggbb`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Colour {
    type Error = CoreError;
    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<Colour> for String {
    fn from(value: Colour) -> Self {
        value.0
    }
}

impl std::fmt::Display for Colour {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Anything that contributes to the polar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// Stable id, allocated by the project.
    pub id: SourceId,
    /// The label shown in the source list; renamed in place.
    pub label: String,
    /// The source's colour everywhere it is drawn. Stored, never derived from
    /// list order.
    pub colour: Colour,
    /// Hidden sources are excluded from the blend and from every plot (D15).
    pub visible: bool,
    /// Blend weight in [0, 1].
    #[serde(with = "canonical::ratio_field")]
    pub weight: f64,
    /// The imported data. Immutable.
    pub kind: SourceKind,
    /// Every user change to it.
    #[serde(default)]
    pub overlay: Overlay,
}

impl Source {
    /// A visible source at the default weight with an empty overlay.
    pub fn new(id: SourceId, label: impl Into<String>, colour: Colour, kind: SourceKind) -> Self {
        Self {
            id,
            label: label.into(),
            colour,
            visible: true,
            weight: DEFAULT_WEIGHT,
            kind,
            overlay: Overlay::default(),
        }
    }

    /// The track, if this is a track source.
    pub fn track(&self) -> Option<&Track> {
        match &self.kind {
            SourceKind::Track { track } => Some(track),
            _ => None,
        }
    }

    /// The track, if this is a track source, mutably.
    pub fn track_mut(&mut self) -> Option<&mut Track> {
        match &mut self.kind {
            SourceKind::Track { track } => Some(track),
            _ => None,
        }
    }

    /// Checks the source's own rules.
    pub fn validate(&self) -> Result<()> {
        validate_weight(self.weight)?;
        validate_cells(&self.overlay.excluded_cells, &self.label)?;
        validate_overrides(&self.overlay.cell_overrides, &self.label)?;
        match &self.kind {
            SourceKind::Orc { record } => {
                let vpp = &record.vpp;
                if vpp.bsp.len() != vpp.angles.len()
                    || vpp.bsp.iter().any(|row| row.len() != vpp.speeds.len())
                {
                    return Err(CoreError::Invalid(format!(
                        "the ORC table of {} does not match its axes",
                        self.label
                    )));
                }
            }
            SourceKind::Orr { record } => record.polar.validate()?,
            SourceKind::PolarFile { polar, .. } => polar.validate()?,
            SourceKind::Track { track } => {
                track.derivation.validate()?;
                self.overlay.filters.validate()?;
                if self
                    .overlay
                    .excluded_samples
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
                {
                    return Err(CoreError::Invalid(format!(
                        "the excluded samples of {} are not in order, or one is listed twice",
                        self.label
                    )));
                }
                for fix in &track.fixes {
                    if !(-90.0..=90.0).contains(&fix.lat) || !(-180.0..=180.0).contains(&fix.lon) {
                        return Err(CoreError::Invalid(format!(
                            "track {} has a fix at {}, {} which is not on Earth",
                            self.label, fix.lat, fix.lon
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Checks a blend weight.
pub fn validate_weight(weight: f64) -> Result<()> {
    if weight.is_finite() && (0.0..=MAX_WEIGHT).contains(&weight) {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!(
            "weight {weight} is outside 0..={MAX_WEIGHT}"
        )))
    }
}

/// Checks an `excluded_cells` list: every cell on the axes' ranges, and the
/// list strictly in [`CellRef::order`]. The commands that write it keep it
/// that way, which is what makes their undo exact (spec.md 4.6).
pub fn validate_cells(cells: &[CellRef], label: &str) -> Result<()> {
    for cell in cells {
        cell.validate()?;
    }
    if cells
        .windows(2)
        .any(|pair| pair[0].order(&pair[1]) != Ordering::Less)
    {
        return Err(CoreError::Invalid(format!(
            "the excluded polar nodes of {label} are not in order, or one is listed twice"
        )));
    }
    Ok(())
}

/// Checks a `cell_overrides` list: every cell on the axes' ranges with a
/// boat speed in 0..=[`MAX_EDIT_BSP_KN`], and the list strictly in
/// [`CellRef::order`], one override per cell, as [`crate::Command::EditCells`]
/// keeps it.
pub fn validate_overrides(overrides: &[CellOverride], label: &str) -> Result<()> {
    for edit in overrides {
        edit.cell().validate()?;
        validate_edit_bsp(edit.bsp)?;
    }
    if overrides
        .windows(2)
        .any(|pair| pair[0].cell().order(&pair[1].cell()) != Ordering::Less)
    {
        return Err(CoreError::Invalid(format!(
            "the polar edits of {label} are not in order, or one cell is edited twice"
        )));
    }
    Ok(())
}

/// Checks a boat speed an edit gives a cell.
pub fn validate_edit_bsp(bsp: f64) -> Result<()> {
    if bsp.is_finite() && (0.0..=MAX_EDIT_BSP_KN).contains(&bsp) {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!(
            "{bsp} kn is not a boat speed a polar cell can hold (0 to {MAX_EDIT_BSP_KN} kn)"
        )))
    }
}

/// What a source is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceKind {
    /// An ORC certificate, copied from the catalogue.
    Orc {
        /// The record as the catalogue held it.
        record: Box<OrcRecord>,
    },
    /// A public ORR certificate, copied from the user's local catalogue.
    Orr {
        /// The immutable certificate variant and boat-speed grid.
        record: Box<crate::orr::OrrRecord>,
    },
    /// An imported Expedition or Adrena polar, parsed at import.
    PolarFile {
        /// How it was read.
        format: PolarFileFormat,
        /// The file name it came from.
        file_name: String,
        /// The grid as parsed.
        polar: PolarGrid,
        /// Full WhirlwindMatrixPolar payload, when the source was imported
        /// from that matrix format. `polar` is its 2D editor/blend projection.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        matrix: Option<Box<crate::polar::WhirlwindMatrixPolar>>,
    },
    /// A track.
    Track {
        /// The track; its bulk data lives in `tracks/<id>.json`.
        track: Box<Track>,
    },
}

impl SourceKind {
    /// A stable name for the kind, for IPC and labels.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Orc { .. } => "orc",
            Self::Orr { .. } => "orr",
            Self::PolarFile { .. } => "polar_file",
            Self::Track { .. } => "track",
        }
    }
}

/// Every user change stored beside one source (spec.md 4.1).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Overlay {
    /// Polar edits (spec.md 10.4), applied before blending.
    pub cell_overrides: Vec<CellOverride>,
    /// Polar nodes excluded from the blend (spec.md 10.3).
    pub excluded_cells: Vec<CellRef>,
    /// Track dots removed from the blend (spec.md 10.3), sorted.
    pub excluded_samples: Vec<SampleId>,
    /// Sample filters; track sources only (spec.md 7.6).
    pub filters: SampleFilters,
}

impl Overlay {
    /// Whether the polar node at (`twa`, `tws`) is excluded from the blend
    /// (spec.md 10.3).
    pub fn is_cell_excluded(&self, twa: f64, tws: f64) -> bool {
        let probe = CellRef { twa, tws };
        self.excluded_cells
            .binary_search_by(|cell| cell.order(&probe))
            .is_ok()
    }

    /// The boat speed the user gave the cell at (`twa`, `tws`), if they
    /// edited it (spec.md 10.4).
    pub fn override_at(&self, twa: f64, tws: f64) -> Option<f64> {
        let probe = CellRef { twa, tws };
        self.cell_overrides
            .binary_search_by(|edit| edit.cell().order(&probe))
            .ok()
            .and_then(|at| self.cell_overrides.get(at))
            .map(|edit| edit.bsp)
    }

    /// Whether the overlay changes nothing.
    pub fn is_empty(&self) -> bool {
        self.cell_overrides.is_empty()
            && self.excluded_cells.is_empty()
            && self.excluded_samples.is_empty()
            && self.filters == SampleFilters::default()
    }
}

/// One edited cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellOverride {
    /// TWA, degrees.
    #[serde(with = "canonical::degrees_field")]
    pub twa: f64,
    /// TWS, knots.
    #[serde(with = "canonical::knots_field")]
    pub tws: f64,
    /// The boat speed the user gave it, knots.
    #[serde(with = "canonical::knots_field")]
    pub bsp: f64,
}

impl CellOverride {
    /// The cell it edits.
    pub fn cell(&self) -> CellRef {
        CellRef {
            twa: self.twa,
            tws: self.tws,
        }
    }
}

/// A cell named by its axis values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellRef {
    /// TWA, degrees.
    #[serde(with = "canonical::degrees_field")]
    pub twa: f64,
    /// TWS, knots.
    #[serde(with = "canonical::knots_field")]
    pub tws: f64,
}

impl CellRef {
    /// The order an overlay keeps its cells in: by TWA, then by TWS.
    pub fn order(&self, other: &Self) -> Ordering {
        self.twa
            .total_cmp(&other.twa)
            .then(self.tws.total_cmp(&other.tws))
    }

    /// Checks the cell is on the polar's axes' ranges.
    pub fn validate(&self) -> Result<()> {
        if self.twa.is_finite()
            && (0.0..=360.0).contains(&self.twa)
            && self.tws.is_finite()
            && self.tws >= 0.0
        {
            Ok(())
        } else {
            Err(CoreError::Invalid(format!(
                "({}°, {} kn) is not a polar node",
                self.twa, self.tws
            )))
        }
    }
}

/// A closed range; either end may be open.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Range {
    /// Lower bound, if any.
    #[serde(default, with = "canonical::optional_ratio_field")]
    pub min: Option<f64>,
    /// Upper bound, if any.
    #[serde(default, with = "canonical::optional_ratio_field")]
    pub max: Option<f64>,
}

/// A range of directions in degrees, clockwise from `from` to `to`; it may
/// wrap through north (`from` 300, `to` 60).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectionRange {
    /// Start, degrees.
    #[serde(with = "canonical::degrees_field")]
    pub from: f64,
    /// End, degrees.
    #[serde(with = "canonical::degrees_field")]
    pub to: f64,
}

/// Where waves come from relative to the bow (spec.md 7.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveSector {
    /// Head seas.
    Head,
    /// On the bow.
    Bow,
    /// On the beam.
    Beam,
    /// On the quarter.
    Quarter,
    /// Following seas.
    Following,
}

/// The wave-direction filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WaveDirectionFilter {
    /// Keep samples whose waves come from one of these sectors.
    Sectors {
        /// The sectors kept.
        sectors: Vec<WaveSector>,
    },
    /// Keep samples whose wave angle off the bow is in this range, degrees
    /// in [0, 180].
    Relative {
        /// The range kept.
        range: Range,
    },
    /// Wave angle relative to course over ground, without current correction.
    Cog {
        /// The range kept, degrees in [0, 180].
        range: Range,
    },
    /// Keep samples whose waves come from this compass range.
    Absolute {
        /// The "from" directions kept.
        range: DirectionRange,
    },
}

/// A track source's sample filters (spec.md 7.6). Every bound is optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SampleFilters {
    /// Significant wave height, metres.
    pub wave_height_m: Option<Range>,
    /// Wave direction.
    pub wave_direction: Option<WaveDirectionFilter>,
    /// Current speed, knots.
    pub current_speed_kn: Option<Range>,
    /// True wind speed, knots.
    pub tws_kn: Option<Range>,
    /// True wind angle, degrees.
    pub twa_deg: Option<Range>,
    /// Time window, UTC epoch seconds. Import sets it to the race start and
    /// finish when the tracker gives them.
    pub time_window: Option<TimeWindow>,
    /// Minimum boat speed, knots.
    #[serde(with = "canonical::optional_knots_field")]
    pub min_bsp_kn: Option<f64>,
    /// Maximum boat speed, knots.
    #[serde(with = "canonical::optional_knots_field")]
    pub max_bsp_kn: Option<f64>,
    /// Exclude fixes whose heading changes by more than this between
    /// neighbours (manoeuvres), degrees.
    #[serde(with = "canonical::optional_degrees_field")]
    pub max_heading_change_deg: Option<f64>,
    /// Largest allowed change of apparent wind bearing, degrees.
    #[serde(with = "canonical::optional_degrees_field")]
    pub max_awa_change_deg: Option<f64>,
    /// Largest allowed change of true wind speed, knots.
    #[serde(with = "canonical::optional_knots_field")]
    pub max_wind_speed_change_kn: Option<f64>,
    /// Largest allowed change of true wind direction, degrees.
    #[serde(with = "canonical::optional_degrees_field")]
    pub max_wind_direction_change_deg: Option<f64>,
    /// Heading through the water kept, compass degrees clockwise from
    /// `from` to `to` (the course over the ground where no current is
    /// corrected for).
    pub heading_deg: Option<DirectionRange>,
    /// Course over the ground kept, compass degrees clockwise.
    pub cog_deg: Option<DirectionRange>,
    /// Speed made good to windward kept, knots: BSP × cos TWA, so negative
    /// downwind.
    pub vmg_kn: Option<Range>,
    /// True wind direction kept, compass degrees clockwise.
    pub twd_deg: Option<DirectionRange>,
    /// Leave out the sample on either side of a tack or a gybe: where the
    /// wind crosses the bow or the stern between neighbours (spec.md 7.6).
    pub exclude_tacks: bool,
    /// Exclude samples whose current came from a tier without tides
    /// (spec.md 7.5.1).
    pub exclude_no_tide: bool,
    /// Require both wave height and wave direction.
    pub exclude_unknown_wave: bool,
    /// Require both current speed and direction; calm current is known.
    pub exclude_unknown_current: bool,
}

impl Default for SampleFilters {
    fn default() -> Self {
        Self {
            wave_height_m: None,
            wave_direction: None,
            current_speed_kn: None,
            tws_kn: None,
            twa_deg: None,
            time_window: None,
            min_bsp_kn: Some(1.0),
            max_bsp_kn: None,
            max_heading_change_deg: Some(30.0),
            max_awa_change_deg: None,
            max_wind_speed_change_kn: None,
            max_wind_direction_change_deg: None,
            heading_deg: None,
            cog_deg: None,
            vmg_kn: None,
            twd_deg: None,
            exclude_tacks: false,
            exclude_no_tide: false,
            exclude_unknown_wave: false,
            exclude_unknown_current: false,
        }
    }
}

fn check_range(range: &Option<Range>, what: &str, lo: f64, hi: f64) -> Result<()> {
    let Some(range) = range else { return Ok(()) };
    for value in [range.min, range.max].into_iter().flatten() {
        if !value.is_finite() || value < lo || value > hi {
            return Err(CoreError::Invalid(format!(
                "the {what} filter bound {value} is outside {lo} to {hi}"
            )));
        }
    }
    if let (Some(min), Some(max)) = (range.min, range.max)
        && min > max
    {
        return Err(CoreError::Invalid(format!(
            "the {what} filter runs from {min} down to {max}"
        )));
    }
    Ok(())
}

impl SampleFilters {
    /// Checks every bound is a number in its quantity's range and every
    /// range runs upwards.
    pub fn validate(&self) -> Result<()> {
        check_range(&self.wave_height_m, "wave height", 0.0, 100.0)?;
        check_range(&self.current_speed_kn, "current speed", 0.0, 100.0)?;
        check_range(&self.tws_kn, "wind speed", 0.0, 200.0)?;
        check_range(&self.twa_deg, "wind angle", 0.0, 180.0)?;
        let bsp = Some(Range {
            min: self.min_bsp_kn,
            max: self.max_bsp_kn,
        });
        check_range(&bsp, "boat speed", 0.0, 100.0)?;
        if let Some(limit) = self.max_heading_change_deg
            && !(limit.is_finite() && limit > 0.0 && limit <= 180.0)
        {
            return Err(CoreError::Invalid(format!(
                "a manoeuvre threshold of {limit}° is outside 0° to 180°"
            )));
        }
        for (threshold, name, upper) in [
            (self.max_awa_change_deg, "apparent wind change", 180.0),
            (
                self.max_wind_direction_change_deg,
                "wind direction change",
                180.0,
            ),
            (self.max_wind_speed_change_kn, "wind speed change", 200.0),
        ] {
            check_range(
                &threshold.map(|v| Range {
                    min: Some(v),
                    max: None,
                }),
                name,
                0.0,
                upper,
            )?;
        }
        if let Some(window) = &self.time_window
            && let (Some(start), Some(end)) = (window.start, window.end)
            && start > end
        {
            return Err(CoreError::Invalid(
                "the time window ends before it starts".to_owned(),
            ));
        }
        // VMG runs from the full speed to windward to the full speed downwind.
        check_range(&self.vmg_kn, "VMG", -100.0, 100.0)?;
        for (range, what) in [
            (&self.heading_deg, "heading"),
            (&self.cog_deg, "course over ground"),
            (&self.twd_deg, "wind direction"),
        ] {
            if let Some(range) = range {
                for value in [range.from, range.to] {
                    if !(value.is_finite() && (0.0..=360.0).contains(&value)) {
                        return Err(CoreError::Invalid(format!(
                            "the {what} filter bound {value} is outside 0° to 360°"
                        )));
                    }
                }
            }
        }
        match &self.wave_direction {
            Some(WaveDirectionFilter::Relative { range } | WaveDirectionFilter::Cog { range }) => {
                check_range(&Some(range.clone()), "wave angle", 0.0, 180.0)?;
            }
            Some(WaveDirectionFilter::Absolute { range }) => {
                for value in [range.from, range.to] {
                    if !(value.is_finite() && (0.0..=360.0).contains(&value)) {
                        return Err(CoreError::Invalid(format!(
                            "the wave direction {value}° is outside 0° to 360°"
                        )));
                    }
                }
            }
            Some(WaveDirectionFilter::Sectors { .. }) | None => {}
        }
        Ok(())
    }
}

/// Additional wave constraints from the main view, independent of other filters.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WaveRanges {
    /// Significant wave height, metres.
    pub height_m: Range,
    /// Wave angle off the selected bow, degrees 0–180.
    pub angle_deg: Range,
    /// Mean wave period, seconds.
    pub period_s: Range,
}

impl WaveRanges {
    /// Validate every saved range before applying an overlay.
    pub fn validate(&self) -> Result<()> {
        check_range(&Some(self.height_m.clone()), "wave height", 0.0, 100.0)?;
        check_range(&Some(self.angle_deg.clone()), "wave angle", 0.0, 180.0)?;
        check_range(&Some(self.period_s.clone()), "wave period", 0.0, 1000.0)
    }
}

/// A time window, UTC epoch seconds; either end may be open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Start.
    #[serde(default)]
    pub start: Option<i64>,
    /// End.
    #[serde(default)]
    pub end: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_checked_and_lower_cased() {
        assert_eq!(Colour::parse("#A1B2C3").unwrap().as_str(), "#a1b2c3");
        for bad in ["a1b2c3", "#abc", "#gggggg", "#a1b2c3d", ""] {
            assert!(Colour::parse(bad).is_err(), "{bad}");
        }
        assert!(serde_json::from_str::<Colour>("\"red\"").is_err());
        for colour in PALETTE.into_iter().chain(["#ffffff"]) {
            assert_eq!(Colour::parse(colour).unwrap(), Colour::trusted(colour));
        }
    }

    #[test]
    fn the_default_filters_match_the_spec() {
        let filters = SampleFilters::default();
        assert_eq!(filters.min_bsp_kn, Some(1.0));
        assert_eq!(filters.max_heading_change_deg, Some(30.0));
        assert!(Overlay::default().is_empty());
    }

    #[test]
    fn filters_out_of_range_or_upside_down_are_refused() {
        SampleFilters::default().validate().unwrap();
        let bad = [
            SampleFilters {
                min_bsp_kn: Some(5.0),
                max_bsp_kn: Some(4.0),
                ..SampleFilters::default()
            },
            SampleFilters {
                min_bsp_kn: Some(f64::NAN),
                ..SampleFilters::default()
            },
            SampleFilters {
                max_heading_change_deg: Some(0.0),
                ..SampleFilters::default()
            },
            SampleFilters {
                time_window: Some(TimeWindow {
                    start: Some(10),
                    end: Some(5),
                }),
                ..SampleFilters::default()
            },
            SampleFilters {
                twa_deg: Some(Range {
                    min: Some(0.0),
                    max: Some(190.0),
                }),
                ..SampleFilters::default()
            },
        ];
        for filters in bad {
            assert!(filters.validate().is_err(), "{filters:?}");
        }
    }

    #[test]
    fn weights_outside_zero_to_one_are_refused() {
        validate_weight(0.0).unwrap();
        validate_weight(1.0).unwrap();
        assert!(validate_weight(1.01).is_err());
        assert!(validate_weight(2.0).is_err());
        assert!(validate_weight(-0.1).is_err());
        assert!(validate_weight(f64::NAN).is_err());
    }
}
