//! Test documents: a fixed one with a source of every kind, and a proptest
//! strategy for arbitrary ones.

use proptest::prelude::*;

use crate::id::SampleId;
use crate::orc::{OrcRecord, OrcSize, OrcVpp};
use crate::polar::{PolarFileFormat, PolarGrid};
use crate::project::{Boat, Project};
use crate::source::{
    CellOverride, CellRef, Colour, DirectionRange, Range, SampleFilters, Source, SourceKind,
    TimeWindow, WaveDirectionFilter, WaveSector,
};
use crate::track::{
    DatasetRecord, EnvStatus, Fix, Sample, Tack, Track, TrackOrigin, Tracker, ValueOrigin,
};

/// An ORC record with a small VPP.
pub fn orc_record() -> OrcRecord {
    OrcRecord {
        ref_no: Some("03160002QKB".to_owned()),
        sail_no: "GBR 1124".to_owned(),
        country: "GBR".to_owned(),
        name: "Test Boat".to_owned(),
        model: Some("Farr 40".to_owned()),
        builder: None,
        designer: Some("Farr".to_owned()),
        year: Some(1998),
        certificate_year: Some(2024),
        size: OrcSize {
            loa: Some(12.41),
            beam: Some(4.0),
            ..OrcSize::default()
        },
        gph: Some(574.3),
        osn: None,
        vpp: OrcVpp {
            angles: vec![52.0, 90.0, 150.0],
            speeds: vec![6.0, 12.0],
            bsp: vec![
                vec![Some(5.9), Some(7.3)],
                vec![Some(6.8), Some(8.4)],
                vec![Some(5.1), None],
            ],
            beat_angle: vec![44.1, 39.8],
            beat_vmg: vec![4.02, 5.61],
            run_angle: vec![141.0, 170.2],
            run_vmg: vec![4.3, 7.0],
        },
    }
}

/// A track with three fixes and a sample for each.
pub fn track(project: &mut Project) -> Track {
    let id = project.allocate_track_id();
    let mut track = Track::new(
        id,
        TrackOrigin::Tracker {
            tracker: Tracker::YellowBrick,
            event_url: "https://event.invalid/fastnet2025".to_owned(),
            event_title: "Fastnet".to_owned(),
            boat_id: "42".to_owned(),
            boat_name: "Boat".to_owned(),
            sail_no: Some("FRA 1".to_owned()),
            model: None,
            division: None,
            race_start: Some(1_753_000_000),
            race_finish: None,
        },
    );
    track.fixes = vec![
        Fix {
            tws: None,
            twd_from: None,
            t: 1_753_000_000,
            lat: 50.1,
            lon: -1.3,
            cog: None,
            sog: None,
        },
        Fix {
            tws: None,
            twd_from: None,
            t: 1_753_000_600,
            lat: 50.12,
            lon: -1.25,
            cog: Some(45.0),
            sog: Some(7.2),
        },
        Fix {
            tws: None,
            twd_from: None,
            t: 1_753_001_200,
            lat: 50.14,
            lon: -1.2,
            cog: None,
            sog: None,
        },
    ];
    track.samples = (0..3u32)
        .map(|i| {
            let id = project.allocate_sample_id();
            let mut sample = Sample::at(id, i, &track.fixes[i as usize]);
            sample.heading = Some(45.0);
            sample.heading_origin = Some(ValueOrigin::Derived);
            sample.tws = Some(14.2);
            sample.twd_from = Some(270.0);
            sample.twa = Some(135.0);
            sample.tack = Some(Tack::Port);
            sample.hs_m = Some(1.2);
            sample.wind_dataset = Some(0);
            sample
        })
        .collect();
    track.env_meta.datasets.push(DatasetRecord {
        name: "weatherbench2-era5".to_owned(),
        version: "2023-01-10".to_owned(),
        fetched_at: 1_760_000_000,
        has_tide: None,
    });
    track.env_meta.status = EnvStatus::Partial;
    track
}

/// A project named "Fixture" with one source of each kind.
pub fn project() -> Project {
    let mut p = Project::new(
        "Fixture",
        Boat {
            name: "Boat".to_owned(),
            notes: "Notes".to_owned(),
            ..Boat::default()
        },
        1_760_000_000,
    );
    let id = p.allocate_source_id();
    let colour = p.next_palette_colour();
    p.sources.push(Source::new(
        id,
        "ORC",
        colour,
        SourceKind::Orc {
            record: Box::new(orc_record()),
        },
    ));

    let id = p.allocate_source_id();
    let colour = p.next_palette_colour();
    let mut polar = PolarGrid::empty(vec![0.0, 45.0, 90.0], vec![6.0, 12.0]);
    polar.bsp[1] = vec![Some(5.5), Some(7.0)];
    polar.bsp[2] = vec![Some(6.5), None];
    p.sources.push(Source::new(
        id,
        "a.txt",
        colour,
        SourceKind::PolarFile {
            format: PolarFileFormat::Expedition,
            file_name: "a.txt".to_owned(),
            polar,
            matrix: None,
        },
    ));

    let id = p.allocate_source_id();
    let colour = p.next_palette_colour();
    let track = track(&mut p);
    let mut source = Source::new(
        id,
        "Boat",
        colour,
        SourceKind::Track {
            track: Box::new(track),
        },
    );
    source
        .overlay
        .excluded_samples
        .push(SampleId(p.next_id - 1));
    p.sources.push(source);
    p
}

// ---------------------------------------------------------------- proptest

fn any_f(range: std::ops::Range<f64>) -> impl Strategy<Value = f64> {
    range
}

fn opt_f(range: std::ops::Range<f64>) -> impl Strategy<Value = Option<f64>> {
    proptest::option::of(range)
}

fn text() -> impl Strategy<Value = String> {
    // Unicode, quotes and control characters: the JSON writer must escape them
    // the same way every time.
    "[a-zA-Z0-9 éøß\"\\\\\n\t/ -]{1,12}"
        .prop_filter("needs a non-blank name", |s| !s.trim().is_empty())
}

fn colour() -> impl Strategy<Value = Colour> {
    "#[0-9a-fA-F]{6}".prop_map(|s| Colour::parse(&s).unwrap_or_else(|_| unreachable!()))
}

fn range() -> impl Strategy<Value = Range> {
    // Valid filters run upwards (`SampleFilters::validate`).
    (opt_f(0.0..50.0), opt_f(0.0..50.0)).prop_map(|(min, max)| match (min, max) {
        (Some(a), Some(b)) if a > b => Range {
            min: Some(b),
            max: Some(a),
        },
        _ => Range { min, max },
    })
}

fn filters() -> impl Strategy<Value = SampleFilters> {
    (
        proptest::option::of(range()),
        prop_oneof![
            Just(None),
            proptest::collection::vec(
                prop_oneof![
                    Just(WaveSector::Head),
                    Just(WaveSector::Beam),
                    Just(WaveSector::Following)
                ],
                0..3
            )
            .prop_map(|sectors| Some(WaveDirectionFilter::Sectors { sectors })),
            range().prop_map(|range| Some(WaveDirectionFilter::Relative { range })),
            (any_f(0.0..360.0), any_f(0.0..360.0)).prop_map(|(from, to)| Some(
                WaveDirectionFilter::Absolute {
                    range: DirectionRange { from, to }
                }
            )),
        ],
        proptest::option::of(range()),
        (
            proptest::option::of(any::<i64>()),
            proptest::option::of(any::<i64>()),
        ),
        // Above the 1 kn default minimum, so the band runs upwards.
        opt_f(1.0..40.0),
        opt_f(0.1..180.0),
        any::<bool>(),
    )
        .prop_map(
            |(wave_height_m, wave_direction, tws_kn, (start, end), max_bsp_kn, turn, no_tide)| {
                SampleFilters {
                    wave_height_m,
                    wave_direction,
                    tws_kn,
                    time_window: (start.is_some() || end.is_some()).then_some(match (start, end) {
                        (Some(a), Some(b)) if a > b => TimeWindow {
                            start: Some(b),
                            end: Some(a),
                        },
                        _ => TimeWindow { start, end },
                    }),
                    max_bsp_kn,
                    max_heading_change_deg: turn,
                    exclude_tacks: no_tide,
                    exclude_no_tide: no_tide,
                    ..SampleFilters::default()
                }
            },
        )
}

fn polar() -> impl Strategy<Value = PolarGrid> {
    (1usize..5, 1usize..4).prop_flat_map(|(rows, cols)| {
        (
            proptest::collection::vec(opt_f(0.0..30.0), rows * cols),
            any_f(0.0..10.0),
            any_f(0.0..3.0),
        )
            .prop_map(move |(cells, twa0, tws0)| PolarGrid {
                twa: (0..rows).map(|i| twa0 + i as f64 * 33.3).collect(),
                tws: (0..cols).map(|j| tws0 + j as f64 * 4.7).collect(),
                bsp: cells.chunks(cols).map(<[_]>::to_vec).collect(),
            })
    })
}

fn raw_fix() -> impl Strategy<Value = Fix> {
    (
        any::<i32>(),
        any_f(-90.0..90.0),
        any_f(-180.0..180.0),
        opt_f(0.0..360.0),
        opt_f(0.0..40.0),
    )
        .prop_map(|(t, lat, lon, cog, sog)| Fix {
            tws: None,
            twd_from: None,
            t: i64::from(t),
            lat,
            lon,
            cog,
            sog,
        })
}

type SampleEnv = (
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    bool,
);

fn sample_env() -> impl Strategy<Value = SampleEnv> {
    (
        opt_f(0.0..40.0),
        opt_f(0.0..360.0),
        opt_f(0.0..180.0),
        opt_f(0.0..12.0),
        opt_f(0.0..4.0),
        any::<bool>(),
    )
}

/// The kind of a generated source, before ids are allocated.
#[derive(Debug, Clone)]
enum Kind {
    Orc,
    Polar(PolarGrid),
    Track(Vec<(Fix, SampleEnv)>),
}

fn kind() -> impl Strategy<Value = Kind> {
    prop_oneof![
        Just(Kind::Orc),
        polar().prop_map(Kind::Polar),
        proptest::collection::vec((raw_fix(), sample_env()), 0..6).prop_map(Kind::Track),
    ]
}

/// An arbitrary valid project.
pub fn arb_project() -> impl Strategy<Value = Project> {
    (
        text(),
        text(),
        proptest::collection::vec(
            (
                text(),
                colour(),
                any::<bool>(),
                any_f(0.0..1.0),
                kind(),
                filters(),
            ),
            0..5,
        ),
        any::<bool>(),
    )
        .prop_map(|(name, boat, sources, smoothing)| {
            let mut p = Project::new(
                name,
                Boat {
                    name: boat,
                    notes: String::new(),
                    ..Boat::default()
                },
                1_700_000_000,
            );
            p.blend.smoothing = smoothing;
            for (label, colour, visible, weight, kind, filters) in sources {
                let id = p.allocate_source_id();
                let kind = match kind {
                    Kind::Orc => SourceKind::Orc {
                        record: Box::new(orc_record()),
                    },
                    Kind::Polar(polar) => SourceKind::PolarFile {
                        format: PolarFileFormat::Adrena,
                        file_name: format!("{label}.pol"),
                        polar,
                        matrix: None,
                    },
                    Kind::Track(points) => {
                        let track_id = p.allocate_track_id();
                        let mut track = Track::new(
                            track_id,
                            TrackOrigin::File {
                                name: format!("{label}.csv"),
                                boat_name: None,
                            },
                        );
                        for (i, (fix, env)) in points.into_iter().enumerate() {
                            let sample_id = p.allocate_sample_id();
                            let mut sample = Sample::at(sample_id, i as u32, &fix);
                            let (tws, twd, twa, hs, current, port) = env;
                            sample.tws = tws;
                            sample.twd_from = twd;
                            sample.twa = twa;
                            sample.hs_m = hs;
                            sample.current_speed = current;
                            sample.speed = fix.sog;
                            sample.speed_origin = fix.sog.map(|_| ValueOrigin::Given);
                            sample.tack = Some(if port { Tack::Port } else { Tack::Starboard });
                            track.fixes.push(fix);
                            track.samples.push(sample);
                        }
                        SourceKind::Track {
                            track: Box::new(track),
                        }
                    }
                };
                let mut source = Source::new(id, label, colour, kind);
                source.visible = visible;
                source.weight = weight;
                source.overlay.filters = filters;
                source.overlay.cell_overrides.push(CellOverride {
                    twa: 52.0,
                    tws: weight * 10.0,
                    bsp: weight * 3.0,
                });
                source.overlay.excluded_cells.push(CellRef {
                    twa: 90.0,
                    tws: 12.0,
                });
                p.sources.push(source);
            }
            p
        })
}
