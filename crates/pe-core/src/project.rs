//! The project: one attempt to build one polar for one boat (spec.md 2, 4.1).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{CoreError, Result};
use crate::id::{MAX_ID, ProjectId, SampleId, SourceId, TrackId};
use crate::polar::validate_axis;
use crate::source::{Colour, PALETTE, Source};
use crate::track::SegmentStatistic;

/// The document schema version this build writes.
///
/// Opening a newer version is refused; older versions migrate forward on open
/// (`io::MIGRATIONS`).
pub const SCHEMA_VERSION: u32 = 8;

/// The boat the polar is for. Free text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Boat {
    /// Boat name.
    pub name: String,
    /// Notes.
    pub notes: String,
    /// Original tracker fields and identifiers, retained for model matching.
    pub details: std::collections::BTreeMap<String, String>,
}

/// The output grid every source is resampled onto (spec.md 12.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputGrid {
    /// True wind angles, degrees in [0, 180] or [0, 360], strictly increasing.
    #[serde(with = "canonical::degrees_list")]
    pub twa: Vec<f64>,
    /// True wind speeds, knots, strictly increasing.
    #[serde(with = "canonical::knots_list")]
    pub tws: Vec<f64>,
}

/// The most values an output grid axis may have: what a polar file may hold
/// (pe-polar's `MAX_AXIS_VALUES`), so every export reads back.
pub const MAX_GRID_VALUES: usize = 512;

/// The highest wind speed an output grid may have, knots: what a polar file
/// may hold (spec.md 6).
pub const MAX_GRID_TWS_KN: f64 = 70.0;

/// The smallest step between two values of an output grid axis. Export
/// writes axes with two decimals (spec.md 6), so two values closer than
/// this would be written as one and the file would not read back
/// (plan.md M14, carried from M4).
pub const GRID_STEP: f64 = 0.01;

/// Checks one output grid axis (spec.md 12.2): at least one value, at most
/// [`MAX_GRID_VALUES`], each finite in `0..=max` with at most two decimals,
/// strictly increasing — so no two are closer than [`GRID_STEP`] and every
/// value is written exactly as it is.
pub fn validate_grid_axis(axis: &[f64], name: &str, max: f64) -> Result<()> {
    if axis.is_empty() {
        return Err(CoreError::Invalid(format!("the {name} axis has no values")));
    }
    if axis.len() > MAX_GRID_VALUES {
        return Err(CoreError::Invalid(format!(
            "the {name} axis has more than {MAX_GRID_VALUES} values"
        )));
    }
    for value in axis {
        if !value.is_finite() || *value < 0.0 || *value > max {
            return Err(CoreError::Invalid(format!(
                "{name} value {value} is outside 0..={max}"
            )));
        }
        let hundredths = value * 100.0;
        if (hundredths - hundredths.round()).abs() > 1e-6 {
            return Err(CoreError::Invalid(format!(
                "{name} value {value} has more than two decimals"
            )));
        }
    }
    for pair in axis.windows(2) {
        if pair[1] - pair[0] < GRID_STEP - 1e-9 {
            return Err(CoreError::Invalid(format!(
                "{name} values {} and {} are not increasing by at least {GRID_STEP}",
                pair[0], pair[1]
            )));
        }
    }
    Ok(())
}

impl OutputGrid {
    /// Checks both axes (see [`validate_grid_axis`]).
    pub fn validate(&self) -> Result<()> {
        validate_grid_axis(&self.twa, "TWA", 360.0)?;
        validate_grid_axis(&self.tws, "TWS", MAX_GRID_TWS_KN)
    }
}

impl Default for OutputGrid {
    fn default() -> Self {
        Self {
            twa: vec![
                0.0, 30.0, 35.0, 40.0, 45.0, 52.0, 60.0, 70.0, 75.0, 80.0, 90.0, 100.0, 110.0,
                120.0, 135.0, 150.0, 160.0, 170.0, 180.0,
            ],
            tws: vec![4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0, 25.0, 30.0],
        }
    }
}

/// How values between polar nodes are read. Linear preserves the old result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    /// Straight lines between known values.
    #[default]
    Linear,
    /// Shape-preserving piecewise cubic Hermite interpolation (PCHIP).
    MonotoneSpline,
}

/// Statistic across visible polar sources, independent of track statistics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolarStatistic {
    /// Lowest speed.
    Min,
    /// Middle speed, interpolating between the two central values.
    Median,
    /// Source-weighted mean, preserving the original blend rule.
    #[default]
    Mean,
    /// Highest speed.
    Max,
    /// Linearly interpolated 90th percentile.
    P90,
}

/// How the blend is computed and shown (spec.md 7.5, 8, 12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlendSettings {
    /// Samples a track cell needs before it has a value (spec.md 12.1).
    /// Clamped into range on load, so a hand-edited file still opens.
    #[serde(deserialize_with = "clamped_samples")]
    pub min_samples: u32,
    /// Samples at which a track cell reaches full confidence (spec.md 12.3).
    /// Clamped into range on load.
    #[serde(deserialize_with = "clamped_samples")]
    pub n_full: u32,
    /// Smooth the filled grid (off by default).
    pub smoothing: bool,
    /// Feed the polar from current-corrected values where a current exists
    /// (spec.md 7.5, D13).
    pub use_corrected: bool,
    /// Include Stokes drift in the global merged current (spec.md 7.5.1).
    pub include_stokes_drift: bool,
    /// The blend's colour in every plot.
    pub colour: Colour,
    /// Whether the blend is drawn.
    pub visible: bool,
    /// The per-cell statistic a newly imported track starts with (spec.md
    /// 12.1); each track keeps its own after that.
    pub default_statistic: SegmentStatistic,
    /// How polar sources are combined in each cell.
    pub polar_statistic: PolarStatistic,
    /// Additional track filters, edited in the global 3D view. Never a time window.
    pub global_filters: Option<crate::source::SampleFilters>,
    /// Additional wave ranges from the main view, also applied to the blend.
    pub wave_ranges: crate::source::WaveRanges,
    /// Preserve independent port and starboard values, using a 0–360° grid.
    pub asymmetric: bool,
    /// Interpolation for resampling sources, filling holes and exporting.
    pub interpolation: Interpolation,
    /// Manual corrections applied after the derived blend, never a saved blend.
    pub corrections: Vec<crate::source::CellOverride>,
    /// Highest priority first; an empty list disables per-cell fallback.
    pub priority_groups: Vec<crate::source::SampleFilters>,
    /// Minimum pooled samples needed to choose a group for one TWA/TWS cell.
    #[serde(deserialize_with = "clamped_samples")]
    pub priority_min_samples: u32,
}

/// The most samples a cell may be asked to need, or to reach full
/// confidence at: far beyond any real track, small enough to stay sane.
pub const MAX_SAMPLE_SETTING: u32 = 1_000_000;

/// The blend's colour in a new project: a rose that stands out on every
/// bundled theme, light and dark (contrast 2.7–4.7 against their
/// backgrounds), and is not the feature-search flash colour. Older projects
/// keep the white they were saved with; the interface outlines a blend
/// colour too close to the background (plan.md M14 review).
pub const DEFAULT_BLEND_COLOUR: &str = "#e0457b";

/// A sample count read from a file, clamped into 1..=[`MAX_SAMPLE_SETTING`]:
/// a hand-edited 0 opens as 1 rather than refusing the whole project.
/// Saving still validates.
fn clamped_samples<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<u32, D::Error> {
    let value = u64::deserialize(d)?;
    Ok(u32::try_from(value.clamp(1, u64::from(MAX_SAMPLE_SETTING))).unwrap_or(MAX_SAMPLE_SETTING))
}

impl BlendSettings {
    /// Checks the sample counts: at least one, at most
    /// [`MAX_SAMPLE_SETTING`].
    pub fn validate(&self) -> Result<()> {
        self.wave_ranges.validate()?;
        if let Some(filters) = &self.global_filters {
            filters.validate()?;
            if filters.time_window.is_some() {
                return Err(CoreError::Invalid(
                    "global filters cannot set start or end times".to_owned(),
                ));
            }
        }
        crate::source::validate_overrides(&self.corrections, "blend")?;
        if self.priority_groups.len() > 16 {
            return Err(CoreError::Invalid(
                "at most 16 priority groups are supported".to_owned(),
            ));
        }
        for filters in &self.priority_groups {
            filters.validate()?;
            if filters.time_window.is_some() {
                return Err(CoreError::Invalid(
                    "priority groups cannot set start or end times".to_owned(),
                ));
            }
        }
        for (name, value) in [
            ("minimum samples per cell", self.min_samples),
            (
                "minimum samples per priority group",
                self.priority_min_samples,
            ),
            ("samples for full confidence", self.n_full),
        ] {
            if value == 0 || value > MAX_SAMPLE_SETTING {
                return Err(CoreError::Invalid(format!(
                    "the {name} must be 1 to {MAX_SAMPLE_SETTING}, not {value}"
                )));
            }
        }
        Ok(())
    }
}

impl Default for BlendSettings {
    fn default() -> Self {
        Self {
            min_samples: 5,
            n_full: 30,
            smoothing: false,
            use_corrected: true,
            include_stokes_drift: false,
            colour: Colour::trusted(DEFAULT_BLEND_COLOUR),
            visible: true,
            default_statistic: SegmentStatistic::default(),
            polar_statistic: PolarStatistic::default(),
            global_filters: None,
            wave_ranges: crate::source::WaveRanges::default(),
            asymmetric: false,
            interpolation: Interpolation::Linear,
            corrections: Vec::new(),
            priority_groups: Vec::new(),
            priority_min_samples: 5,
        }
    }
}

/// A whole project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// The schema this document follows.
    pub schema_version: u32,
    /// Identity, independent of the file name.
    pub id: ProjectId,
    /// Display name.
    pub name: String,
    /// When it was created, UTC epoch seconds.
    pub created: i64,
    /// The boat.
    #[serde(default)]
    pub boat: Boat,
    /// The output grid.
    #[serde(default)]
    pub grid: OutputGrid,
    /// Blend settings.
    #[serde(default)]
    pub blend: BlendSettings,
    /// Every source, in the order the user sees them.
    #[serde(default)]
    pub sources: Vec<Source>,
    /// The next id to allocate. Ids are never reused.
    pub next_id: u64,
    /// Additional independent boats, in tab order. Only the root document
    /// has tabs; each child uses its own ids, grid and source overlays.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boat_tabs: Vec<Project>,
}

impl Project {
    /// A new, empty project with default grid and blend settings.
    pub fn new(name: impl Into<String>, boat: Boat, created: i64) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id: ProjectId::fresh(),
            name: name.into(),
            created,
            boat,
            grid: OutputGrid::default(),
            blend: BlendSettings::default(),
            sources: Vec::new(),
            next_id: 1,
            boat_tabs: Vec::new(),
        }
    }

    /// How many ids are left before [`MAX_ID`].
    pub fn ids_left(&self) -> u64 {
        (MAX_ID + 1).saturating_sub(self.next_id)
    }

    /// Checks `count` more ids can be allocated without passing [`MAX_ID`].
    /// Every import calls this before allocating, so no id the project hands
    /// out is ever beyond what the frontend can hold exactly.
    pub fn reserve_ids(&self, count: u64) -> Result<()> {
        if count <= self.ids_left() {
            Ok(())
        } else {
            Err(CoreError::Invalid(format!(
                "the project has only {} ids left, and this needs {count}",
                self.ids_left()
            )))
        }
    }

    fn allocate(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    /// A source id no source has had.
    pub fn allocate_source_id(&mut self) -> SourceId {
        SourceId(self.allocate())
    }

    /// A track id no track has had.
    pub fn allocate_track_id(&mut self) -> TrackId {
        TrackId(self.allocate())
    }

    /// A sample id no sample has had.
    pub fn allocate_sample_id(&mut self) -> SampleId {
        SampleId(self.allocate())
    }

    /// The source with this id.
    pub fn source(&self, id: SourceId) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    /// The source with this id, mutably.
    pub fn source_mut(&mut self, id: SourceId) -> Option<&mut Source> {
        self.sources.iter_mut().find(|s| s.id == id)
    }

    /// Where the source with this id sits in the list.
    pub fn source_index(&self, id: SourceId) -> Option<usize> {
        self.sources.iter().position(|s| s.id == id)
    }

    /// The first palette colour no source uses, or the palette continued by
    /// count when all sixteen are taken (spec.md 8).
    pub fn next_palette_colour(&self) -> Colour {
        let mut colours = self.next_palette_colours(1);
        colours.pop().unwrap_or_else(|| Colour::trusted(PALETTE[0]))
    }

    /// The colours `count` sources added one after another would take: each
    /// the first palette colour neither the project nor an earlier one of
    /// them uses. A multi-file import colours its files with this before any
    /// of them is in the list.
    pub fn next_palette_colours(&self, count: usize) -> Vec<Colour> {
        let mut used: BTreeSet<&str> = self.sources.iter().map(|s| s.colour.as_str()).collect();
        (0..count)
            .map(|k| {
                let pick = PALETTE
                    .iter()
                    .find(|c| !used.contains(**c))
                    .copied()
                    .unwrap_or(PALETTE[(self.sources.len() + k) % PALETTE.len()]);
                used.insert(pick);
                Colour::trusted(pick)
            })
            .collect()
    }

    /// Checks every rule the document must hold before it is written.
    pub fn validate(&self) -> Result<()> {
        self.check(true)
    }

    /// [`Self::validate`] without the bulk track data, for a document read
    /// back from `project.json` alone (which has no fixes or samples).
    pub fn validate_document(&self) -> Result<()> {
        self.check(false)
    }

    fn check(&self, bulk: bool) -> Result<()> {
        let mut boats = BTreeSet::from([self.id]);
        for tab in &self.boat_tabs {
            if !tab.boat_tabs.is_empty() || !boats.insert(tab.id) {
                return Err(CoreError::Invalid(
                    "boat tabs must be flat and have unique identities".to_owned(),
                ));
            }
            if tab.schema_version != SCHEMA_VERSION {
                return Err(CoreError::Invalid(
                    "a boat tab has an unsupported schema version".to_owned(),
                ));
            }
            tab.check(bulk)?;
        }
        if self.name.trim().is_empty() {
            return Err(CoreError::Invalid("the project has no name".to_owned()));
        }
        validate_axis(
            &self.grid.twa,
            "output TWA",
            0.0,
            if self.blend.asymmetric { 360.0 } else { 180.0 },
        )?;
        validate_axis(&self.grid.tws, "output TWS", 0.0, f64::MAX)?;
        self.blend.validate()?;

        if self.next_id > MAX_ID + 1 {
            return Err(CoreError::Invalid(format!(
                "the next id {} is beyond the largest id {MAX_ID}",
                self.next_id
            )));
        }
        let mut ids = BTreeSet::new();
        let mut claim = |raw: u64, what: &str| -> Result<()> {
            if raw >= self.next_id {
                return Err(CoreError::Invalid(format!(
                    "{what} #{raw} is not below the next id {}",
                    self.next_id
                )));
            }
            if !ids.insert(raw) {
                return Err(CoreError::Invalid(format!("id #{raw} is used twice")));
            }
            Ok(())
        };
        for source in &self.sources {
            claim(source.id.raw(), "source")?;
            source.validate()?;
            if let Some(track) = source.track() {
                claim(track.id.raw(), "track")?;
                if !bulk {
                    continue;
                }
                for sample in &track.samples {
                    claim(sample.id.raw(), "sample")?;
                    if sample.fix as usize >= track.fixes.len() {
                        return Err(CoreError::Invalid(format!(
                            "sample #{} refers to fix {} of {}",
                            sample.id.raw(),
                            sample.fix,
                            track.fixes.len()
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polar::{PolarFileFormat, PolarGrid};
    use crate::source::SourceKind;

    fn polar_source(project: &mut Project) -> Source {
        let id = project.allocate_source_id();
        let colour = project.next_palette_colour();
        Source::new(
            id,
            "file",
            colour,
            SourceKind::PolarFile {
                format: PolarFileFormat::Expedition,
                file_name: "a.txt".to_owned(),
                polar: PolarGrid::empty(vec![45.0], vec![10.0]),
            },
        )
    }

    #[test]
    fn a_new_project_has_the_spec_defaults() {
        let p = Project::new("Fastnet", Boat::default(), 0);
        assert_eq!(p.grid.tws.len(), 10);
        assert_eq!(p.grid.twa.len(), 19);
        assert_eq!(p.blend.min_samples, 5);
        assert_eq!(p.blend.n_full, 30);
        assert!(p.blend.use_corrected);
        p.validate().unwrap();
    }

    /// The output grid editor's rules (spec.md 12.2, plan.md M14): two
    /// decimals at most, strictly increasing by at least 0.01, in range.
    #[test]
    fn an_output_grid_axis_must_write_back_exactly() {
        OutputGrid::default().validate().unwrap();
        validate_grid_axis(&[0.0, 42.5, 42.51, 180.0], "TWA", 180.0).unwrap();
        for bad in [
            vec![],
            vec![40.0, 40.0],
            vec![45.0, 40.0],
            vec![40.0, 40.005],
            vec![40.125],
            vec![-1.0],
            vec![181.0],
            vec![f64::NAN],
        ] {
            assert!(
                validate_grid_axis(&bad, "TWA", 180.0).is_err(),
                "{bad:?} should be refused"
            );
        }
        assert!(validate_grid_axis(&[71.0], "TWS", MAX_GRID_TWS_KN).is_err());
        let long: Vec<f64> = (0..=MAX_GRID_VALUES).map(|k| k as f64 / 100.0).collect();
        assert!(validate_grid_axis(&long, "TWA", 180.0).is_err());
    }

    /// A hand-edited file with out-of-range sample counts opens, clamped.
    #[test]
    fn out_of_range_sample_counts_are_clamped_on_load() {
        let blend: BlendSettings =
            serde_json::from_str(r#"{"min_samples": 0, "n_full": 99999999999}"#).unwrap();
        assert_eq!((blend.min_samples, blend.n_full), (1, MAX_SAMPLE_SETTING));
        blend.validate().unwrap();
        let blend: BlendSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(blend, BlendSettings::default());
        assert_eq!(blend.colour.as_str(), DEFAULT_BLEND_COLOUR);
    }

    #[test]
    fn blend_sample_counts_are_at_least_one() {
        let mut blend = BlendSettings::default();
        blend.validate().unwrap();
        blend.n_full = 0;
        assert!(blend.validate().is_err());
        blend.n_full = 30;
        blend.min_samples = MAX_SAMPLE_SETTING + 1;
        assert!(blend.validate().is_err());
    }

    #[test]
    fn ids_are_allocated_upwards_and_never_reused() {
        let mut p = Project::new("P", Boat::default(), 0);
        let a = p.allocate_source_id();
        let b = p.allocate_track_id();
        assert!(b.raw() > a.raw());
        assert_eq!(p.next_id, 3);
    }

    #[test]
    fn new_sources_take_the_next_unused_palette_colour() {
        let mut p = Project::new("P", Boat::default(), 0);
        let first = polar_source(&mut p);
        assert_eq!(first.colour.as_str(), PALETTE[0]);
        p.sources.push(first);
        let second = polar_source(&mut p);
        assert_eq!(second.colour.as_str(), PALETTE[1]);
    }

    /// A batch takes the colours the same sources added one by one would:
    /// skipping any a source already has, and never twice the same.
    #[test]
    fn a_batch_of_sources_takes_distinct_unused_colours() {
        let mut p = Project::new("P", Boat::default(), 0);
        let mut first = polar_source(&mut p);
        first.colour = Colour::trusted(PALETTE[1]);
        p.sources.push(first);
        let picked: Vec<String> = p
            .next_palette_colours(3)
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(picked, [PALETTE[0], PALETTE[2], PALETTE[3]]);
        assert_eq!(p.next_palette_colours(17).len(), 17);
        assert_eq!(p.next_palette_colour().as_str(), PALETTE[0]);
    }

    /// Ids never pass 2^53 − 1, the largest integer a JavaScript number
    /// holds exactly, so every id survives the trip to the frontend.
    #[test]
    fn ids_stay_within_what_javascript_holds_exactly() {
        assert_eq!(MAX_ID, 9_007_199_254_740_991);
        assert_eq!(MAX_ID as f64 as u64, MAX_ID);
        let mut p = Project::new("P", Boat::default(), 0);
        p.reserve_ids(1000).unwrap();
        p.next_id = MAX_ID;
        p.reserve_ids(1).unwrap();
        assert!(p.reserve_ids(2).is_err());
        assert_eq!(p.allocate_sample_id().raw(), MAX_ID);
        assert_eq!(p.ids_left(), 0);
        assert!(p.reserve_ids(1).is_err());
        p.validate().unwrap();
        p.next_id = MAX_ID + 2;
        assert!(p.validate().is_err());
    }

    #[test]
    fn duplicate_or_unallocated_ids_are_invalid() {
        let mut p = Project::new("P", Boat::default(), 0);
        let s = polar_source(&mut p);
        p.sources.push(s.clone());
        p.validate().unwrap();
        p.sources.push(s);
        assert!(p.validate().is_err());

        let mut p = Project::new("P", Boat::default(), 0);
        let mut s = polar_source(&mut p);
        s.id = SourceId(99);
        p.sources.push(s);
        assert!(p.validate().is_err());
    }
}
