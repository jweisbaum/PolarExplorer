//! Derived data per source, recomputed only when that source changes
//! (spec.md 10.4, 13; plan.md M13).
//!
//! Every view reads a source through what is derived from it: a polar
//! source's grid with its edits, a track's samples placed in the polar with
//! their filter flags, and its polar segment (spec.md 12.1). Deriving a
//! segment reads every sample of the track, so doing it for every source on
//! every edit would spend the edit-to-view budget (spec.md 13) on sources
//! that did not change. Instead each source carries revisions, bumped only by
//! the changes that reach it, and the cache recomputes an entry only when its
//! revisions moved.
//!
//! **Never persisted** (invariant 2): the cache and the revisions live in the
//! open project's session state and start empty on every opening. Anything
//! that changes a project outside a command bumps everything
//! ([`Derivations::invalidate_all`]) unless it names the source it wrote
//! ([`Derivations::samples_changed`]), so a missed case costs time, never a
//! stale answer.

use std::collections::BTreeMap;
use std::sync::Arc;

use pe_core::source::{Source, SourceKind};
use pe_core::track::SegmentStatistic;
use pe_core::{Command, Project};
use pe_polar::{Polar, Segment};

use crate::wave_split::WaveSplit;

/// How far a change reaches into what is derived from one source. Each
/// level includes the ones below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Touch {
    /// Its grid as edited: overrides and excluded nodes.
    Polar,
    /// A track's segment: which samples count (filters, exclusions) and how
    /// a cell sums them up.
    Segment,
    /// A track's samples themselves: where each sits in the polar.
    Samples,
}

/// What one command changes, per source; `all` when it reaches every
/// source (the choice of corrected or ground values moves every sample).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Touches {
    /// Source id and how far.
    pub sources: Vec<(u64, Touch)>,
    /// Everything derived is stale.
    pub all: bool,
    /// Refilter every cached track without moving its observations.
    pub segments: bool,
}

/// What `command` changes. The match has no wildcard, so a new command does
/// not compile until it says what it touches.
pub fn touches(command: &Command) -> Touches {
    let mut out = Touches::default();
    collect(command, &mut out);
    out
}

fn collect(command: &Command, out: &mut Touches) {
    let mut one = |id: pe_core::SourceId, touch| out.sources.push((id.raw(), touch));
    match command {
        Command::AddSource { source, .. } | Command::RemoveSource { source, .. } => {
            one(source.id, Touch::Samples);
        }
        Command::SetDerivation { source, .. } => one(*source, Touch::Samples),
        Command::ExcludeSamples { source, .. }
        | Command::IncludeSamples { source, .. }
        | Command::SetSampleFilters { source, .. }
        | Command::SetSegmentStatistic { source, .. } => one(*source, Touch::Segment),
        Command::ExcludeCells { source, .. }
        | Command::IncludeCells { source, .. }
        | Command::EditCells { source, .. } => one(*source, Touch::Polar),
        // A new grid re-bins every track; the choice of corrected or
        // ground values moves every sample.
        Command::SetUseCorrected { .. } | Command::SetOutputGrid { .. } => out.all = true,
        Command::SetBlendSettings { before, after } => {
            if before.use_corrected != after.use_corrected || before.asymmetric != after.asymmetric
            {
                out.all = true;
            }
            if before.global_filters != after.global_filters
                || before.wave_ranges != after.wave_ranges
            {
                out.segments = true;
            }
            // The sample minimum re-bins through each track's key; the rest
            // (colour, visibility, n_full, smoothing) reaches only the
            // blend, whose key holds them.
        }
        // Colour, label, weight, visibility and order change no source's
        // own derived data; the views read them from the project directly.
        Command::RenameProject { .. }
        | Command::RenameBoat { .. }
        | Command::SetSourceColour { .. }
        | Command::SetSourceVisible { .. }
        | Command::SetSourceWeight { .. }
        | Command::SetSourceLabel { .. }
        | Command::MoveSource { .. }
        | Command::SetStokesDrift { .. } => {}
        Command::Batch { commands, .. } => {
            for command in commands {
                collect(command, out);
            }
        }
    }
}

/// A source's revisions: each moves when a change reaches that level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceRevs {
    /// Moves with [`Touch::Samples`].
    pub samples: u64,
    /// Moves with [`Touch::Segment`] and above.
    pub segment: u64,
    /// Moves with every touch.
    pub polar: u64,
}

/// A track's samples as the polar views place them, and its segment.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackDerived {
    /// Each sample's (TWA °, TWS kn, BSP kn), `None` without wind, in sample
    /// order (spec.md 7.5: water-relative where the project says so).
    pub points: Vec<Option<(f64, f64, f64)>>,
    /// Whether the filters take each sample out, in sample order.
    pub filtered: Vec<bool>,
    /// The polar segment on the output grid (spec.md 12.1).
    pub segment: Segment,
}

/// What one source's views read.
#[derive(Debug, Clone, PartialEq)]
pub struct Derived {
    /// The editable surface before edits: the imported or VPP grid, or the
    /// track's segment (spec.md 10.4).
    pub base: Polar,
    /// Samples and segment, for a track.
    pub track: Option<Arc<TrackDerived>>,
    /// `base` with the edits written in: what the 3D view and the table show.
    pub edited: Polar,
    /// `edited` with excluded nodes emptied: what the blend reads
    /// (spec.md 12.3) and the 2D curves draw.
    pub blend: Polar,
}

/// What a track's derived data depends on besides its own revisions.
#[derive(Debug, Clone, PartialEq)]
struct TrackKey {
    epoch: u64,
    samples: u64,
    segment: u64,
    use_corrected: bool,
    min_samples: u32,
    statistic: SegmentStatistic,
    priority_revision: u64,
    twa: Vec<f64>,
    tws: Vec<f64>,
}

#[derive(Debug, Clone)]
struct Entry {
    base_key: Option<TrackKey>,
    base_epoch: u64,
    edit_key: (u64, u64),
    value: Arc<Derived>,
}

/// What the blend was made from: equal keys give the same blend. Each
/// source's derived data is held by its `Arc`, so it cannot be freed and
/// its address reused while the key compares it.
#[derive(Debug, Clone)]
struct BlendKey {
    grid: pe_core::project::OutputGrid,
    n_full: u32,
    polar_statistic: pe_core::project::PolarStatistic,
    smoothing: bool,
    interpolation: pe_core::project::Interpolation,
    corrections: Vec<pe_core::source::CellOverride>,
    sources: Vec<(u64, u64, Arc<Derived>)>,
}

impl PartialEq for BlendKey {
    fn eq(&self, other: &Self) -> bool {
        self.grid == other.grid
            && self.n_full == other.n_full
            && self.polar_statistic == other.polar_statistic
            && self.smoothing == other.smoothing
            && self.interpolation == other.interpolation
            && self.corrections == other.corrections
            && self.sources.len() == other.sources.len()
            && self
                .sources
                .iter()
                .zip(&other.sources)
                .all(|(a, b)| a.0 == b.0 && a.1 == b.1 && Arc::ptr_eq(&a.2, &b.2))
    }
}

type PriorityFlags = Arc<BTreeMap<u64, Vec<bool>>>;
/// The blend of each copy of a split view, `None` where a copy has none.
pub type SplitBlends = Arc<Vec<Option<pe_polar::Blend>>>;

/// The revisions and the cache of one opening of a project.
#[derive(Debug, Default)]
pub struct Derivations {
    /// Unique to this opening of the project (its first revision), so two
    /// openings — whose counters and ids both start again — never share a
    /// samples key.
    nonce: u64,
    counter: u64,
    /// Moves when everything is stale.
    epoch: u64,
    revs: BTreeMap<u64, SourceRevs>,
    cache: BTreeMap<u64, Entry>,
    blend: Option<(BlendKey, Arc<pe_polar::Blend>)>,
    /// The blends of a split view's copies (spec.md 10.5), for the split
    /// they were made for, kept on the blend's own terms.
    split: Option<(BlendKey, WaveSplit, SplitBlends)>,
    priority: Option<(crate::priority::Key, u64, PriorityFlags)>,
    /// Entries computed since the opening, for tests of what recomputes.
    pub computed: u64,
    /// Blends computed since the opening.
    pub blends: u64,
}

impl Derivations {
    /// The derivations of one opening; `nonce` must differ between openings
    /// (the session passes the opening's fresh revision).
    pub fn for_opening(nonce: u64) -> Self {
        Self {
            nonce,
            ..Self::default()
        }
    }

    /// Drops the cached entries of sources no longer in the project, so a
    /// removed source's samples are not held in memory.
    pub fn prune(&mut self, project: &Project) {
        let present = |id: u64| project.sources.iter().any(|s| s.id.raw() == id);
        self.cache.retain(|id, _| present(*id));
        if self
            .blend
            .as_ref()
            .is_some_and(|(key, _)| key.sources.iter().any(|(id, ..)| !present(*id)))
        {
            self.blend = None;
        }
        if self
            .split
            .as_ref()
            .is_some_and(|(key, ..)| key.sources.iter().any(|(id, ..)| !present(*id)))
        {
            self.split = None;
        }
    }

    /// How many sources have a cached entry.
    pub fn cached(&self) -> usize {
        self.cache.len()
    }

    fn next(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }

    /// Records a change that reached `id` as far as `touch`.
    pub fn bump(&mut self, id: u64, touch: Touch) {
        let rev = self.next();
        let revs = self.revs.entry(id).or_default();
        revs.polar = rev;
        if touch >= Touch::Segment {
            revs.segment = rev;
        }
        if touch >= Touch::Samples {
            revs.samples = rev;
        }
    }

    /// Records what a command changed.
    pub fn record(&mut self, touches: &Touches) {
        if touches.all {
            self.invalidate_all();
        } else if touches.segments {
            let tracks: Vec<_> = self
                .cache
                .iter()
                .filter(|(_, entry)| entry.value.track.is_some())
                .map(|(id, _)| *id)
                .collect();
            for id in tracks {
                self.bump(id, Touch::Segment);
            }
        }
        for (id, touch) in &touches.sources {
            self.bump(*id, *touch);
        }
    }

    /// Everything derived is stale: a change the caller cannot place.
    pub fn invalidate_all(&mut self) {
        self.epoch = self.next();
    }

    /// A track's samples were written outside a command (the environment
    /// fetch).
    pub fn samples_changed(&mut self, id: u64) {
        self.bump(id, Touch::Samples);
    }

    /// The revisions of `id`.
    pub fn revs(&self, id: u64) -> SourceRevs {
        self.revs.get(&id).copied().unwrap_or_default()
    }

    /// Names the samples section of a 3D scene: equal keys mean the same
    /// visible sources in the same order, each track's samples placed as
    /// before, so only their flags can differ. FNV-1a over plain numbers:
    /// nothing here iterates a hash map. Kept to 53 bits, so the frontend
    /// holds it exactly as a number and names it back.
    pub fn samples_key(&self, project: &Project) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |value: u64| {
            for byte in value.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        };
        mix(self.nonce);
        mix(self.epoch);
        mix(u64::from(project.blend.use_corrected));
        for source in project.sources.iter().filter(|s| s.visible) {
            mix(source.id.raw());
            if source.track().is_some() {
                mix(self.revs(source.id.raw()).samples);
            }
        }
        hash & ((1 << 53) - 1)
    }

    fn priority_masks(&mut self, project: &Project) -> (u64, Option<PriorityFlags>) {
        if project.blend.priority_groups.is_empty() {
            return (0, None);
        }
        let key = crate::priority::Key::of(project, self.epoch, |id| self.revs(id).segment);
        if let Some((held, revision, masks)) = &self.priority
            && *held == key
        {
            return (*revision, Some(Arc::clone(masks)));
        }
        let masks = Arc::new(crate::priority::filter_flags(project));
        let revision = self.next();
        self.priority = Some((key, revision, Arc::clone(&masks)));
        (revision, Some(masks))
    }

    /// What `source` derives to, from the cache when nothing it depends on
    /// has moved.
    pub fn get(&mut self, project: &Project, source: &Source) -> Arc<Derived> {
        let (priority_revision, priority) = self.priority_masks(project);
        let id = source.id.raw();
        let revs = self.revs(id);
        let base_key = source.track().map(|track| TrackKey {
            epoch: self.epoch,
            samples: revs.samples,
            segment: revs.segment,
            use_corrected: project.blend.use_corrected,
            min_samples: project.blend.min_samples,
            statistic: track.statistic,
            priority_revision,
            twa: project.grid.twa.clone(),
            tws: project.grid.tws.clone(),
        });
        let edit_key = (self.epoch, revs.polar);
        let cached = self.cache.get(&id);
        let base_fresh = cached
            .is_some_and(|entry| entry.base_key == base_key && entry.base_epoch == self.epoch);
        if let Some(entry) = cached
            && base_fresh
            && entry.edit_key == edit_key
        {
            return Arc::clone(&entry.value);
        }
        self.computed += 1;
        let (base, track) = match cached {
            Some(entry) if base_fresh => (entry.value.base.clone(), entry.value.track.clone()),
            _ => derive_base_with_filters(
                project,
                source,
                priority.as_deref().and_then(|m| m.get(&id)),
            ),
        };
        let edited = pe_polar::with_overlay(base.clone(), &source.overlay, false);
        let blend = pe_polar::with_overlay(edited.clone(), &source.overlay, true);
        let value = Arc::new(Derived {
            base,
            track,
            edited,
            blend,
        });
        self.cache.insert(
            id,
            Entry {
                base_key,
                base_epoch: self.epoch,
                edit_key,
                value: Arc::clone(&value),
            },
        );
        value
    }

    /// The blend (spec.md 12.3), from the cache when no visible source,
    /// weight, grid or blend setting it reads has moved. For the views;
    /// export recomputes from scratch (invariant 2, [`crate::blend::fresh`]).
    pub fn blend(&mut self, project: &Project) -> Arc<pe_polar::Blend> {
        let (key, derived) = self.blend_key(project);
        if let Some((held, value)) = &self.blend
            && *held == key
        {
            return Arc::clone(value);
        }
        self.blends += 1;
        let value = Arc::new(crate::blend::assemble(project, &derived));
        self.blend = Some((key, Arc::clone(&value)));
        value
    }

    /// The blend of each copy of a split view (spec.md 10.5,
    /// [`crate::wave_split::blends`]), kept while the blend would be: a
    /// change that leaves the blend alone leaves these alone.
    pub fn split_blends(&mut self, project: &Project, split: WaveSplit) -> SplitBlends {
        let (key, derived) = self.blend_key(project);
        if let Some((held, held_split, value)) = &self.split
            && *held == key
            && *held_split == split
        {
            return Arc::clone(value);
        }
        let value = Arc::new(crate::wave_split::blends(project, &derived, split));
        self.split = Some((key, split, Arc::clone(&value)));
        value
    }

    /// What the blend is made from, and the derived data it reads.
    fn blend_key(&mut self, project: &Project) -> (BlendKey, BTreeMap<u64, Arc<Derived>>) {
        let derived = self.visible(project);
        let key = BlendKey {
            grid: project.grid.clone(),
            n_full: project.blend.n_full,
            polar_statistic: project.blend.polar_statistic,
            smoothing: project.blend.smoothing,
            interpolation: project.blend.interpolation,
            corrections: project.blend.corrections.clone(),
            sources: project
                .sources
                .iter()
                .filter(|s| s.visible)
                .filter_map(|s| {
                    let data = derived.get(&s.id.raw())?;
                    Some((s.id.raw(), s.weight.to_bits(), Arc::clone(data)))
                })
                .collect(),
        };
        // By id: the list order is display only (spec.md 8), and the blend
        // sums in id order, so reordering must not recompute it.
        let mut key = key;
        key.sources.sort_by_key(|(id, ..)| *id);
        (key, derived)
    }

    /// Every visible source's derived data, by id.
    pub fn visible(&mut self, project: &Project) -> BTreeMap<u64, Arc<Derived>> {
        project
            .sources
            .iter()
            .filter(|s| s.visible)
            .map(|s| (s.id.raw(), self.get(project, s)))
            .collect()
    }
}

/// Place port observations on the second half of an asymmetric polar.
pub fn sample_point(project: &Project, sample: &pe_core::track::Sample) -> Option<(f64, f64, f64)> {
    let corrected = project.blend.use_corrected;
    let (mut angle, wind, bsp) = pe_tracks::polar_point(sample, corrected)?;
    let tack = if corrected && sample.twa_corrected.is_some() {
        sample.tack_corrected
    } else {
        sample.tack
    };
    if project.blend.asymmetric && tack == Some(pe_core::track::Tack::Port) {
        angle = 360.0 - angle;
    }
    Some((angle, wind, bsp))
}

/// Shared filter flags for every view, summary and export.
pub fn filtered_samples(
    project: &Project,
    source: &Source,
    track: &pe_core::track::Track,
) -> Vec<bool> {
    if !project.blend.priority_groups.is_empty() {
        return crate::priority::filter_flags(project)
            .remove(&source.id.raw())
            .unwrap_or_default();
    }
    hard_filters(project, source, track)
}

/// Individual and global exclusions, before the priority selector runs.
pub(crate) fn hard_filters(
    project: &Project,
    source: &Source,
    track: &pe_core::track::Track,
) -> Vec<bool> {
    let corrected = project.blend.use_corrected;
    let mut flags = pe_tracks::filtered_out(track, &source.overlay.filters, corrected);
    if let Some(global) = &project.blend.global_filters {
        for (out, extra) in flags
            .iter_mut()
            .zip(pe_tracks::filtered_out(track, global, corrected))
        {
            *out |= extra;
        }
    }
    for (out, sample) in flags.iter_mut().zip(&track.samples) {
        *out |=
            pe_tracks::filter::wave_ranges_exclude(sample, &project.blend.wave_ranges, corrected);
    }
    flags
}

pub(crate) fn derive_base_with_filters(
    project: &Project,
    source: &Source,
    flags: Option<&Vec<bool>>,
) -> (Polar, Option<Arc<TrackDerived>>) {
    match &source.kind {
        SourceKind::Orc { .. } | SourceKind::Orr { .. } | SourceKind::PolarFile { .. } => (
            pe_polar::grid::directional(
                &pe_polar::source_polar(source).unwrap_or_default(),
                project.blend.asymmetric,
            ),
            None,
        ),
        SourceKind::Track { track } => {
            let points: Vec<_> = track
                .samples
                .iter()
                .map(|s| sample_point(project, s))
                .collect();
            let filtered = flags
                .cloned()
                .unwrap_or_else(|| hard_filters(project, source, track));
            let excluded = &source.overlay.excluded_samples;
            // Samples that pass the filters and are not excluded (spec.md
            // 12.1).
            let used = track
                .samples
                .iter()
                .zip(&points)
                .zip(&filtered)
                .filter(|((sample, _), out)| !**out && excluded.binary_search(&sample.id).is_err())
                .filter_map(|((_, point), _)| *point);
            let segment = pe_polar::bin(
                used,
                &project.grid.twa,
                &project.grid.tws,
                track.statistic,
                if project.blend.priority_groups.is_empty() {
                    project.blend.min_samples
                } else {
                    1
                },
            );
            let base = segment.polar.clone();
            (
                base,
                Some(Arc::new(TrackDerived {
                    points,
                    filtered,
                    segment,
                })),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use pe_core::command::{CellEdit, EditAction};
    use pe_core::polar::{PolarFileFormat, PolarGrid};
    use pe_core::track::{Sample, Track, TrackOrigin};
    use pe_core::{Boat, Colour, SampleId, SourceId, TrackId};

    use super::*;

    fn file(id: u64) -> Source {
        Source::new(
            SourceId(id),
            "a.pol",
            Colour::parse("#4e79a7").unwrap(),
            SourceKind::PolarFile {
                format: PolarFileFormat::Adrena,
                file_name: "a.pol".to_owned(),
                polar: PolarGrid {
                    twa: vec![52.0, 90.0],
                    tws: vec![6.0, 12.0],
                    bsp: vec![vec![Some(5.0), Some(6.5)], vec![Some(6.0), Some(8.0)]],
                },
            },
        )
    }

    /// A track of `n` samples at 90° in 12 kn, BSP 7 + k/100.
    fn track(id: u64, n: u64) -> Source {
        let mut track = Track::new(
            TrackId(id + 1000),
            TrackOrigin::File {
                name: "t.csv".to_owned(),
                boat_name: None,
            },
        );
        for k in 0..n {
            let fix = pe_core::track::Fix {
                tws: None,
                twd_from: None,
                t: k as i64 * 60,
                lat: 0.0,
                lon: 0.0,
                cog: None,
                sog: None,
            };
            let mut sample = Sample::at(SampleId(id * 10_000 + k), k as u32, &fix);
            sample.tws = Some(12.0);
            sample.twa = Some(90.0);
            sample.speed = Some(7.0 + k as f64 / 100.0);
            track.samples.push(sample);
            track.fixes.push(fix);
        }
        let mut source = Source::new(
            SourceId(id),
            "t",
            Colour::parse("#e15759").unwrap(),
            SourceKind::Track {
                track: Box::new(track),
            },
        );
        // No filters: every sample counts.
        source.overlay.filters.min_bsp_kn = None;
        source.overlay.filters.max_heading_change_deg = None;
        source
    }

    fn project() -> Project {
        let mut project = Project::new("P", Boat::default(), 0);
        project.sources = vec![file(1), track(2, 10)];
        project.next_id = 100_000;
        project
    }

    #[test]
    fn a_track_segment_is_binned_from_its_samples() {
        let project = project();
        let mut derivations = Derivations::default();
        let derived = derivations.get(&project, &project.sources[1]);
        let segment = &derived.track.as_ref().unwrap().segment;
        let i = project.grid.twa.iter().position(|v| *v == 90.0).unwrap();
        let j = project.grid.tws.iter().position(|v| *v == 12.0).unwrap();
        assert_eq!(segment.count[i][j], 10);
        // The 90th percentile of 7.00..7.09: rank 8.1 → 7.081.
        let value = derived.base.bsp[i][j].unwrap();
        assert!((value - 7.081).abs() < 1e-9, "{value}");
        assert_eq!(derived.edited, derived.base);
    }

    /// An edit to one source recomputes only that source; the track's
    /// segment is not binned again for a polar edit, nor the polar for a
    /// track's exclusion.
    #[test]
    fn a_change_recomputes_only_the_source_it_reaches() {
        let mut project = project();
        let mut derivations = Derivations::default();
        let (file_source, track_source) = (project.sources[0].clone(), project.sources[1].clone());
        let first_track = derivations.get(&project, &track_source);
        derivations.get(&project, &file_source);
        assert_eq!(derivations.computed, 2);
        derivations.get(&project, &track_source);
        assert_eq!(derivations.computed, 2, "nothing moved");

        let mut edit = Command::EditCells {
            source: SourceId(1),
            action: EditAction::Type,
            cells: vec![CellEdit {
                twa: 90.0,
                tws: 12.0,
                before: None,
                after: Some(8.4),
            }],
        };
        edit.apply(&mut project).unwrap();
        derivations.record(&touches(&edit));
        let file_after = derivations.get(&project, &project.sources[0].clone());
        assert_eq!(file_after.edited.bsp[1][1], Some(8.4));
        let track_again = derivations.get(&project, &track_source);
        assert_eq!(derivations.computed, 3, "only the edited source");
        assert!(Arc::ptr_eq(&first_track, &track_again));

        // An override on the track's segment keeps its binned samples.
        let mut track_edit = Command::EditCells {
            source: SourceId(2),
            action: EditAction::Type,
            cells: vec![CellEdit {
                twa: 90.0,
                tws: 12.0,
                before: None,
                after: Some(9.0),
            }],
        };
        track_edit.apply(&mut project).unwrap();
        derivations.record(&touches(&track_edit));
        let edited_track = derivations.get(&project, &project.sources[1].clone());
        assert!(Arc::ptr_eq(
            edited_track.track.as_ref().unwrap(),
            first_track.track.as_ref().unwrap()
        ));
        let i = project.grid.twa.iter().position(|v| *v == 90.0).unwrap();
        let j = project.grid.tws.iter().position(|v| *v == 12.0).unwrap();
        assert_eq!(edited_track.edited.bsp[i][j], Some(9.0));
        assert_ne!(edited_track.base.bsp[i][j], Some(9.0));

        // Excluding samples re-bins the track.
        let ids: Vec<SampleId> = project.sources[1]
            .track()
            .unwrap()
            .samples
            .iter()
            .take(6)
            .map(|s| s.id)
            .collect();
        let mut exclude = Command::ExcludeSamples {
            source: SourceId(2),
            samples: ids,
        };
        exclude.apply(&mut project).unwrap();
        derivations.record(&touches(&exclude));
        let fewer = derivations.get(&project, &project.sources[1].clone());
        assert_eq!(fewer.track.as_ref().unwrap().segment.count[i][j], 4);
        // Four samples are below the minimum of five: the cell is empty in
        // the segment, and the override still holds it.
        assert_eq!(fewer.base.bsp[i][j], None);
        assert_eq!(fewer.edited.bsp[i][j], Some(9.0));
    }

    /// Two openings of the same project never share a samples key, though
    /// their counters and ids start again: a view holding the first
    /// opening's scene must not take the second's flags onto it.
    #[test]
    fn two_openings_of_one_project_have_different_samples_keys() {
        let project = project();
        let first = Derivations::for_opening(1_000);
        let second = Derivations::for_opening(1_001);
        assert_ne!(first.samples_key(&project), second.samples_key(&project));
        assert_eq!(
            first.samples_key(&project),
            Derivations::for_opening(1_000).samples_key(&project)
        );
    }

    /// The blend is kept until something it reads moves: a weight, a
    /// source's data, the grid or a blend setting — not a colour.
    #[test]
    fn the_blend_is_recomputed_only_when_what_it_reads_moves() {
        let mut project = project();
        let mut derivations = Derivations::default();
        let first = derivations.blend(&project);
        assert!(Arc::ptr_eq(&first, &derivations.blend(&project)));
        assert_eq!(derivations.blends, 1);
        project.sources[0].colour = Colour::parse("#000000").unwrap();
        derivations.blend(&project);
        assert_eq!(derivations.blends, 1, "a colour is not blended");
        project.sources.reverse();
        derivations.blend(&project);
        assert_eq!(derivations.blends, 1, "the list order is display only");
        project.sources.reverse();
        project.sources[0].weight = 0.5;
        derivations.blend(&project);
        assert_eq!(derivations.blends, 2);
        project.blend.smoothing = true;
        derivations.blend(&project);
        assert_eq!(derivations.blends, 3);
        derivations.bump(1, Touch::Polar);
        derivations.blend(&project);
        assert_eq!(derivations.blends, 4);
        project.sources.remove(1);
        derivations.prune(&project);
        derivations.blend(&project);
        assert_eq!(derivations.blends, 5);
    }

    /// A removed source's entry is dropped.
    #[test]
    fn a_removed_source_leaves_the_cache() {
        let mut project = project();
        let mut derivations = Derivations::default();
        derivations.visible(&project);
        assert_eq!(derivations.cached(), 2);
        project.sources.remove(1);
        derivations.prune(&project);
        assert_eq!(derivations.cached(), 1);
    }

    /// The samples key moves when samples move or the visible list changes,
    /// and not for an edit or an exclusion.
    #[test]
    fn the_samples_key_follows_sample_positions_only() {
        let mut project = project();
        let mut derivations = Derivations::default();
        let key = derivations.samples_key(&project);
        derivations.bump(2, Touch::Segment);
        derivations.bump(1, Touch::Polar);
        assert_eq!(derivations.samples_key(&project), key);
        derivations.samples_changed(2);
        let moved = derivations.samples_key(&project);
        assert_ne!(moved, key);
        project.sources[0].visible = false;
        assert_ne!(derivations.samples_key(&project), moved);
        project.sources[0].visible = true;
        derivations.invalidate_all();
        assert_ne!(derivations.samples_key(&project), moved);
    }
}
