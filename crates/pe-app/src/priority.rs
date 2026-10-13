//! Ordered filter groups choose evidence independently in each TWA/TWS cell.
//! Pool counts across visible tracks, then retain exactly one group's matching
//! observations. The result is a derived mask, never persisted in a project.

use std::collections::BTreeMap;

use pe_core::{Project, project::OutputGrid, source::SampleFilters};

/// Every dependency of a pooled selection. Revisions avoid comparing raw tracks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Key {
    epoch: u64,
    grid: OutputGrid,
    corrected: bool,
    asymmetric: bool,
    global: Option<SampleFilters>,
    waves: pe_core::source::WaveRanges,
    groups: Vec<SampleFilters>,
    minimum: u32,
    sources: Vec<(u64, u64, bool)>,
}

impl Key {
    pub(crate) fn of(project: &Project, epoch: u64, revision: impl Fn(u64) -> u64) -> Self {
        let mut sources: Vec<_> = project
            .sources
            .iter()
            .filter(|s| s.track().is_some())
            .map(|s| {
                (
                    s.id.raw(),
                    revision(s.id.raw()),
                    s.visible && s.weight > 0.0,
                )
            })
            .collect();
        sources.sort_unstable();
        Self {
            epoch,
            grid: project.grid.clone(),
            corrected: project.blend.use_corrected,
            asymmetric: project.blend.asymmetric,
            global: project.blend.global_filters.clone(),
            waves: project.blend.wave_ranges.clone(),
            groups: project.blend.priority_groups.clone(),
            minimum: project.blend.priority_min_samples,
            sources,
        }
    }
}

struct TrackFlags {
    id: u64,
    hard: Vec<bool>,
    cells: Vec<Option<usize>>,
    groups: Vec<Vec<bool>>,
}

/// Shared masks for the map, track details, plots and blend. An empty group
/// list has the exact previous individual/global filtering behavior.
pub fn filter_flags(project: &Project) -> BTreeMap<u64, Vec<bool>> {
    filter_flags_where(project, |_| true)
}

/// Re-evaluate priority groups inside a wave export cell before binning.
pub(crate) fn filter_flags_where(
    project: &Project,
    admitted: impl Fn(&pe_core::track::Sample) -> bool,
) -> BTreeMap<u64, Vec<bool>> {
    let groups = &project.blend.priority_groups;
    let columns = project.grid.tws.len();
    let mut counts = vec![vec![0u32; project.grid.twa.len() * columns]; groups.len()];
    let tracks: Vec<_> = project
        .sources
        .iter()
        .filter_map(|source| {
            let track = source.track()?;
            let mut hard = crate::derived::hard_filters(project, source, track);
            for (flag, sample) in hard.iter_mut().zip(&track.samples) {
                *flag |= !admitted(sample);
            }
            if groups.is_empty() {
                return Some(TrackFlags {
                    id: source.id.raw(),
                    hard,
                    cells: Vec::new(),
                    groups: Vec::new(),
                });
            }
            let cells: Vec<_> = track
                .samples
                .iter()
                .map(|sample| {
                    let (angle, wind, _) = crate::derived::sample_point(project, sample)?;
                    let i = pe_polar::segment::bin_index(&project.grid.twa, angle)?;
                    let j = pe_polar::segment::bin_index(&project.grid.tws, wind)?;
                    let angle = project.grid.twa[i];
                    (angle != 0.0 && angle != 360.0).then_some(i * columns + j)
                })
                .collect();
            let masks: Vec<_> = groups
                .iter()
                .map(|filters| pe_tracks::filtered_out(track, filters, project.blend.use_corrected))
                .collect();
            if source.visible && source.weight > 0.0 {
                for (k, sample) in track.samples.iter().enumerate() {
                    if hard[k]
                        || source
                            .overlay
                            .excluded_samples
                            .binary_search(&sample.id)
                            .is_ok()
                    {
                        continue;
                    }
                    let Some(cell) = cells[k] else {
                        continue;
                    };
                    for (count, mask) in counts.iter_mut().zip(&masks) {
                        if !mask[k] {
                            count[cell] = count[cell].saturating_add(1);
                        }
                    }
                }
            }
            Some(TrackFlags {
                id: source.id.raw(),
                hard,
                cells,
                groups: masks,
            })
        })
        .collect();
    let chosen: Vec<_> = (0..project.grid.twa.len() * columns)
        .map(|cell| {
            counts
                .iter()
                .position(|count| count[cell] >= project.blend.priority_min_samples)
        })
        .collect();
    tracks
        .into_iter()
        .map(|mut track| {
            if !groups.is_empty() {
                for (k, flag) in track.hard.iter_mut().enumerate() {
                    *flag |= track.cells[k]
                        .and_then(|cell| chosen[cell])
                        .is_none_or(|group| track.groups[group][k]);
                }
            }
            (track.id, track.hard)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pe_core::{
        Boat, Colour, SampleId, SourceId, TrackId,
        source::{Range, Source, SourceKind},
        track::{Fix, Sample, Track, TrackOrigin},
    };

    fn source(id: u64, values: &[(f64, f64, f64)]) -> Source {
        let mut track = Track::new(
            TrackId(id + 100),
            TrackOrigin::File {
                name: "priority.csv".into(),
                boat_name: None,
            },
        );
        for (k, (angle, speed, wave)) in values.iter().enumerate() {
            let fix = Fix {
                tws: None,
                twd_from: None,
                t: k as i64 * 60,
                lat: 0.0,
                lon: 0.0,
                cog: None,
                sog: None,
            };
            let mut sample = Sample::at(SampleId(id * 1000 + k as u64), k as u32, &fix);
            sample.twa = Some(*angle);
            sample.tws = Some(10.0);
            sample.speed = Some(*speed);
            sample.hs_m = Some(*wave);
            track.samples.push(sample);
            track.fixes.push(fix);
        }
        Source::new(
            SourceId(id),
            "track",
            Colour::parse("#112233").unwrap(),
            SourceKind::Track {
                track: Box::new(track),
            },
        )
    }

    #[test]
    fn choose_per_cell_pool_tracks_and_never_duplicate_overlapping_groups() {
        let mut project = Project::new("priorities", Boat::default(), 0);
        project.grid = OutputGrid {
            twa: vec![45.0, 90.0, 135.0],
            tws: vec![10.0],
        };
        project.blend.priority_min_samples = 2;
        let all = SampleFilters {
            min_bsp_kn: None,
            max_heading_change_deg: None,
            ..SampleFilters::default()
        };
        let calm = SampleFilters {
            wave_height_m: Some(Range {
                min: Some(0.0),
                max: Some(1.0),
            }),
            ..all.clone()
        };
        project.blend.priority_groups = vec![calm, all];
        project.sources = vec![
            source(1, &[(45.0, 4.0, 0.5), (90.0, 5.0, 0.5), (135.0, 5.0, 3.0)]),
            source(2, &[(45.0, 6.0, 0.5), (45.0, 20.0, 3.0), (90.0, 7.0, 3.0)]),
        ];
        // 45°: two calm across tracks choose group 1, dropping the rough outlier.
        // 90°: only one calm, so group 2 retains both. 135° never reaches two.
        let flags = filter_flags(&project);
        assert_eq!(flags[&1], [false, false, true]);
        assert_eq!(flags[&2], [false, true, false]);
        let mut cache = crate::derived::Derivations::default();
        let first = cache.get(&project, &project.sources[0]);
        assert_eq!(
            first.track.as_ref().unwrap().segment.count,
            [vec![1], vec![1], vec![0]]
        );
        assert_eq!(
            first.base.bsp,
            [vec![Some(4.0)], vec![Some(5.0)], vec![None]]
        );
        // Hiding the other track invalidates the pooled choice in the cache.
        project.sources[1].visible = false;
        let hidden = cache.get(&project, &project.sources[0]);
        assert_eq!(hidden.track.as_ref().unwrap().filtered, [true, true, true]);
        project.sources[1].visible = true;
        project.sources[0]
            .overlay
            .excluded_samples
            .push(SampleId(1000));
        cache.bump(1, crate::derived::Touch::Segment);
        // Removing one calm sample makes 45° fall back to the two remaining.
        let second = cache.get(&project, &project.sources[1]);
        assert_eq!(
            second.track.as_ref().unwrap().filtered,
            [false, false, false]
        );
    }
}
