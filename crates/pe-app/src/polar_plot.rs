//! The 2D polar plot over IPC (spec.md 9.2).
//!
//! Rust does every computation; the frontend only draws what it is given.
//! For a chosen true wind speed (or every one a source has, in "all" mode,
//! `tws: None`), each visible polar source gives one curve per pe-polar's
//! bilinear interpolation, swept across that source's own TWA axis — nothing
//! is invented beyond what the source's grid covers (invariant: no
//! extrapolation). A curve reads the source **through its overlay**, as the
//! blend will (spec.md 12.3): its edits written in and its excluded nodes
//! empty, so every edit shows here at once (spec.md 10.4). A track is not a
//! polar source, so it contributes no curve; its samples are dots.
//!
//! # Dots: wire layout, version 2
//!
//! Dots travel as one packed little-endian buffer, not JSON (plan.md M13):
//! in "all" mode every sample with wind is a dot, and 50 tracks of 10,000
//! fixes as JSON objects would be tens of megabytes of text. The frontend's
//! mirror is `ui/src/panels/dotPacket.ts`, and both are held to the same
//! bytes by `ui/src/panels/fixtures/dots-v2.bin`.
//!
//! ```text
//! header, 4 × u32 (16 bytes)
//!   0  magic    0x44324550 (the bytes "PE2D")
//!   1  version  2
//!   2  S        sources
//!   3  M        dots
//! sources, S × 2 u32   id lo, id hi
//! f32 [M × 3]  TWA °, TWS kn, BSP kn
//! u32 [M]      source index
//! u32 [M × 2]  sample id, lo then hi
//! u32 [M]      flags: bit 0 excluded, bit 1 filtered out, bit 3 placed
//!              through the water (corrected for current; otherwise the
//!              speed is the track's own, over the ground); bits 8–9 the
//!              band of the local solar day (0 night, 1 morning,
//!              2 afternoon, 3 evening; spec.md 9.2, `pe_tracks::daytime`)
//! ```
//!
//! A sample has a place only once it has wind (M9): until then it is left
//! out rather than drawn somewhere invented.
//!
//! The blend (spec.md 12.3) is drawn from `pe_polar::blend`'s grid when the
//! Blend entry is shown: at the slice, or one curve per output-grid wind
//! speed in "all", read the same way as a source's curves.

use std::collections::BTreeMap;
use std::sync::Arc;

use pe_core::source::Source;
use pe_polar::Polar;
use pe_tracks::daytime::DayBand;
use serde::Serialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::derived::{Derivations, Derived};
use crate::error::Result;

/// How close a sample's TWS may be to the slice and still count as at it,
/// knots, by default (spec.md 9.2). The person changes it in Settings
/// (`Settings::plot_tws_band_kn`).
pub const DEFAULT_TWS_BAND_KN: f64 = 1.0;

/// "PE2D" read as a little-endian u32.
pub const DOTS_MAGIC: u32 = u32::from_le_bytes(*b"PE2D");
/// The dots layout's version.
pub const DOTS_VERSION: u32 = 2;
/// Dot flag: excluded from the blend by hand.
pub const DOT_EXCLUDED: u32 = 1;
/// Dot flag: taken out by the track's filters.
pub const DOT_FILTERED: u32 = 2;
/// Dot flag: its speed is through the water (corrected for current), not
/// the track's own over the ground.
pub const DOT_THROUGH_WATER: u32 = 8;
/// Where a dot's flags hold its day band's two-bit code.
pub const DOT_BAND_SHIFT: u32 = 8;

/// One point of a curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export_to = "PolarCurvePoint.ts")]
pub struct PolarCurvePoint {
    /// True wind angle, degrees, folded to \[0, 180\].
    pub twa: f64,
    /// Boat speed, knots.
    pub bsp: f64,
}

/// One drawn curve: a source's polar at one true wind speed.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "PolarCurve.ts")]
pub struct PolarCurve {
    /// The source it came from; null for the blend.
    pub source_id: Option<u64>,
    /// Its label, for the legend and hover.
    pub label: String,
    /// Its colour, `#rrggbb`.
    pub colour: String,
    /// The true wind speed this curve is at, knots.
    pub tws: f64,
    /// Points with a value, in increasing TWA. Gaps in the source's coverage
    /// are gaps here too: nothing is interpolated across a missing corner.
    pub points: Vec<PolarCurvePoint>,
}

/// One track sample near the slice (spec.md 9.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolarSampleDot {
    /// The track source it came from.
    pub source_id: u64,
    /// The sample, for selection shared with the map and the 3D view.
    pub sample_id: u64,
    /// True wind angle, degrees, folded to \[0, 180\].
    pub twa: f64,
    /// True wind speed at the sample, knots.
    pub tws: f64,
    /// Boat speed, knots.
    pub bsp: f64,
    /// Taken out by the track's filters (only sent when asked for).
    pub filtered: bool,
    /// Excluded from the blend by hand.
    pub excluded: bool,
    /// Placed through the water: `bsp` is corrected for current.
    pub through_water: bool,
    /// The band of the local solar day it was sailed in.
    pub band: DayBand,
}

/// What the 2D polar plot draws besides its dots (spec.md 9.2).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "PolarPlotResult.ts")]
pub struct PolarPlotResult {
    /// The lowest true wind speed any visible polar source covers; null when
    /// none does. The TWS slider's range.
    pub tws_min: Option<f64>,
    /// The highest true wind speed any visible polar source covers.
    pub tws_max: Option<f64>,
    /// One curve per visible polar source at the chosen slice; several per
    /// source, one per wind speed it has, when `tws` was null ("all").
    pub curves: Vec<PolarCurve>,
    /// The band used for the dots, knots.
    pub band_kn: f64,
    /// The blend at the slice, or one curve per output-grid wind speed in
    /// "all"; empty while the Blend entry is hidden. `source_id` is null
    /// and `label` is "Blend", which the interface translates.
    pub blend: Vec<PolarCurve>,
}

/// The label a blend curve carries: an English key the interface translates.
pub const BLEND_LABEL: &str = "Blend";

/// The points of `grid` at `tws`, read at every one of the grid's own TWA
/// angles (no extrapolation, gaps stay gaps).
fn points_at(
    grid: &Polar,
    tws: f64,
    mode: pe_core::project::Interpolation,
    blend: bool,
) -> Vec<PolarCurvePoint> {
    let reader = pe_polar::spline::Interpolator::new(grid, mode);
    let mut angles = grid.twa.clone();
    if mode == pe_core::project::Interpolation::MonotoneSpline
        && let (Some(lo), Some(hi)) = (grid.twa.first(), grid.twa.last())
    {
        angles.extend((lo.ceil() as u32..=hi.floor() as u32).map(f64::from));
        angles.sort_by(f64::total_cmp);
        angles.dedup();
    }
    let sampled = blend.then(|| pe_polar::blend::resample_blend_mode(grid, &angles, &[tws], mode));
    angles
        .iter()
        .enumerate()
        .filter_map(|(i, &twa)| {
            let value = match &sampled {
                Some(grid) => grid.get(i, 0),
                None => reader.at(twa, tws),
            };
            value.map(|bsp| PolarCurvePoint { twa, bsp })
        })
        .collect()
}

/// One curve of a source's `grid` at `tws`.
fn curve_at(
    source: &Source,
    grid: &Polar,
    tws: f64,
    mode: pe_core::project::Interpolation,
) -> PolarCurve {
    PolarCurve {
        source_id: Some(source.id.raw()),
        label: source.label.clone(),
        colour: source.colour.to_string(),
        tws,
        points: points_at(grid, tws, mode, false),
    }
}

/// The blend's wind speeds that hold a value off the 0° row (which is 0 kn
/// everywhere by definition, so says nothing about where the blend reaches).
fn blend_speeds(blend: &Polar) -> Vec<f64> {
    blend
        .tws
        .iter()
        .enumerate()
        .filter(|(j, _)| {
            blend
                .twa
                .iter()
                .enumerate()
                .any(|(i, twa)| *twa > 0.0 && *twa < 360.0 && blend.get(i, *j).is_some())
        })
        .map(|(_, tws)| *tws)
        .collect()
}

/// The blend's curves: at the slice, or at each of its wind speeds with a
/// value.
fn blend_curves(project: &pe_core::Project, blend: &Polar, tws: Option<f64>) -> Vec<PolarCurve> {
    let curve = |tws: f64| PolarCurve {
        source_id: None,
        label: BLEND_LABEL.to_owned(),
        colour: project.blend.colour.to_string(),
        tws,
        points: points_at(blend, tws, project.blend.interpolation, true),
    };
    match tws {
        Some(value) => vec![curve(value)],
        None => blend_speeds(blend).into_iter().map(curve).collect(),
    }
}

/// Every sample of a visible track that has a place in the polar, within
/// `band` of `tws` (every one, for "all").
pub fn dots_of(
    project: &pe_core::Project,
    derived: &BTreeMap<u64, Arc<Derived>>,
    tws: Option<f64>,
    band: f64,
    show_filtered: bool,
) -> Vec<PolarSampleDot> {
    let mut dots = Vec::new();
    for source in project.sources.iter().filter(|s| s.visible) {
        let (Some(track), Some(placed)) = (
            source.track(),
            derived.get(&source.id.raw()).and_then(|d| d.track.as_ref()),
        ) else {
            continue;
        };
        let excluded = &source.overlay.excluded_samples;
        for ((sample, point), filtered) in track
            .samples
            .iter()
            .zip(&placed.points)
            .zip(&placed.filtered)
        {
            if *filtered && !show_filtered {
                continue;
            }
            let Some((twa, sample_tws, bsp)) = *point else {
                continue;
            };
            if tws.is_some_and(|slice| (sample_tws - slice).abs() > band) {
                continue;
            }
            dots.push(PolarSampleDot {
                source_id: source.id.raw(),
                sample_id: sample.id.raw(),
                twa,
                tws: sample_tws,
                bsp,
                filtered: *filtered,
                excluded: excluded.binary_search(&sample.id).is_ok(),
                through_water: pe_tracks::through_water(sample, project.blend.use_corrected),
                band: pe_tracks::daytime::day_band(sample.t, sample.lon),
            });
        }
    }
    dots
}

/// Packs dots into the layout in the module documentation.
pub fn pack_dots(dots: &[PolarSampleDot]) -> Vec<u8> {
    let mut sources: Vec<u64> = Vec::new();
    let mut index_of: BTreeMap<u64, u32> = BTreeMap::new();
    for dot in dots {
        index_of.entry(dot.source_id).or_insert_with(|| {
            sources.push(dot.source_id);
            (sources.len() - 1) as u32
        });
    }
    let m = dots.len();
    let mut out = Vec::with_capacity(16 + sources.len() * 8 + m * 28);
    let mut u = |value: u32| out.extend_from_slice(&value.to_le_bytes());
    u(DOTS_MAGIC);
    u(DOTS_VERSION);
    u(sources.len() as u32);
    u(m as u32);
    for id in &sources {
        u(*id as u32);
        u((id >> 32) as u32);
    }
    for dot in dots {
        for value in [dot.twa, dot.tws, dot.bsp] {
            u((value as f32).to_bits());
        }
    }
    for dot in dots {
        u(index_of.get(&dot.source_id).copied().unwrap_or(0));
    }
    for dot in dots {
        u(dot.sample_id as u32);
        u((dot.sample_id >> 32) as u32);
    }
    for dot in dots {
        u((if dot.excluded { DOT_EXCLUDED } else { 0 })
            | (if dot.filtered { DOT_FILTERED } else { 0 })
            | (if dot.through_water {
                DOT_THROUGH_WATER
            } else {
                0
            })
            | (dot.band.code() << DOT_BAND_SHIFT));
    }
    out
}

/// The plot's curves for the open project (spec.md 9.2).
///
/// `tws` chooses the slice; `None` is "all", which draws one curve per
/// visible polar source for every wind speed that source's own grid has,
/// rather than one slice shared by every source.
#[tauri::command]
pub fn polar_plot(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    tws: Option<f64>,
) -> Result<PolarPlotResult> {
    let state = state.scoped(boat_context);
    plot(&state, tws)
}

/// The plot's dots, packed (layout in the module documentation): every
/// sample within the band of `tws` (every one with wind, for null), and
/// with `show_filtered` the samples the filters take out, flagged.
#[tauri::command]
pub fn polar_plot_dots(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    tws: Option<f64>,
    show_filtered: Option<bool>,
) -> Result<tauri::ipc::Response> {
    let state = state.scoped(boat_context);
    dots_bytes(&state, tws, show_filtered.unwrap_or(false)).map(tauri::ipc::Response::new)
}

/// [`polar_plot_dots`] without a Tauri handle, unpacked.
pub fn dots(
    state: &AppState,
    tws: Option<f64>,
    show_filtered: bool,
) -> Result<Vec<PolarSampleDot>> {
    state.with_session(|session| {
        let band = session.settings.plot_tws_band_kn;
        let open = session.require_open()?;
        let derived = open.derived.visible(&open.project);
        Ok(dots_of(&open.project, &derived, tws, band, show_filtered))
    })
}

/// [`polar_plot_dots`] without a Tauri handle.
pub fn dots_bytes(state: &AppState, tws: Option<f64>, show_filtered: bool) -> Result<Vec<u8>> {
    dots(state, tws, show_filtered).map(|dots| pack_dots(&dots))
}

/// [`polar_plot`] without a Tauri handle.
pub fn plot(state: &AppState, tws: Option<f64>) -> Result<PolarPlotResult> {
    state.with_session(|session| {
        let band = session.settings.plot_tws_band_kn;
        let open = session.require_open()?;
        let derived = open.derived.visible(&open.project);
        let blend = open.derived.blend(&open.project);
        let shown = open.project.blend.visible.then_some(&blend.polar);
        Ok(plot_of(&open.project, &derived, shown, tws, band))
    })
}

/// The curves from each visible source's derived data, and the blend's
/// when it is shown (`blend`).
pub fn plot_of(
    project: &pe_core::Project,
    derived: &BTreeMap<u64, Arc<Derived>>,
    blend: Option<&Polar>,
    tws: Option<f64>,
    band: f64,
) -> PolarPlotResult {
    // Tracks are not polar sources (spec.md 9.2): their samples are dots.
    let grids: Vec<(&Source, &Polar)> = project
        .sources
        .iter()
        .filter(|source| source.visible && source.track().is_none())
        .filter_map(|source| {
            derived
                .get(&source.id.raw())
                .map(|data| (source, &data.blend))
        })
        .collect();

    // The blend's wind speeds with a value count toward the slider's range.
    let blend = blend.filter(|grid| !blend_speeds(grid).is_empty());
    let spans: Vec<(f64, f64)> = grids
        .iter()
        .filter_map(|(_, grid)| Some((*grid.tws.first()?, *grid.tws.last()?)))
        .chain(blend.and_then(|grid| {
            let speeds = blend_speeds(grid);
            Some((*speeds.first()?, *speeds.last()?))
        }))
        .collect();
    let tws_min = spans
        .iter()
        .map(|(low, _)| *low)
        .fold(None, |acc: Option<f64>, value| {
            Some(acc.map_or(value, |current| current.min(value)))
        });
    let tws_max = spans
        .iter()
        .map(|(_, high)| *high)
        .fold(None, |acc: Option<f64>, value| {
            Some(acc.map_or(value, |current| current.max(value)))
        });

    let curves = match tws {
        Some(value) => grids
            .iter()
            .map(|(source, grid)| curve_at(source, grid, value, project.blend.interpolation))
            .collect(),
        None => grids
            .iter()
            .flat_map(|(source, grid)| {
                grid.tws
                    .iter()
                    .map(move |&value| curve_at(source, grid, value, project.blend.interpolation))
            })
            .collect(),
    };

    PolarPlotResult {
        tws_min,
        tws_max,
        curves,
        band_kn: band,
        blend: blend
            .map(|grid| blend_curves(project, grid, tws))
            .unwrap_or_default(),
    }
}

/// A fresh derivation of every visible source, for tests and measurements
/// that change a project behind the session's back.
pub fn derive_all(project: &pe_core::Project) -> BTreeMap<u64, Arc<Derived>> {
    Derivations::default().visible(project)
}

#[cfg(test)]
mod tests {
    use pe_core::polar::{PolarFileFormat, PolarGrid};
    use pe_core::source::SourceKind;
    use pe_core::{Colour, Command, SourceId};

    use super::*;
    use crate::error::AppError;
    use crate::projects;

    fn polar_file_source(
        id: u64,
        label: &str,
        twa: Vec<f64>,
        tws: Vec<f64>,
        bsp: Vec<Vec<Option<f64>>>,
    ) -> Source {
        Source::new(
            SourceId(id),
            label,
            Colour::parse("#4e79a7").unwrap(),
            SourceKind::PolarFile {
                format: PolarFileFormat::Expedition,
                file_name: format!("{label}.txt"),
                polar: PolarGrid { twa, tws, bsp },
                matrix: None,
            },
        )
    }

    /// A throwaway state rooted under a name unique to this test run.
    fn state(label: &str) -> AppState {
        let dir = std::env::temp_dir().join(format!(
            "pe-polar-plot-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        AppState::new(crate::paths::AppPaths::in_directory(&dir).expect("paths"))
    }

    /// A project with two polar-file sources of different axes, ids 1 and 2.
    fn two_sources() -> AppState {
        let app = state("two-sources");
        projects::create(&app, "Plot".to_owned(), None, false).expect("create");
        app.with_session(|session| {
            let open = session.require_open()?;
            let a = polar_file_source(
                1,
                "A",
                vec![40.0, 90.0, 150.0],
                vec![6.0, 12.0],
                vec![
                    vec![Some(4.0), Some(6.0)],
                    vec![Some(6.0), Some(8.0)],
                    vec![Some(5.0), Some(9.0)],
                ],
            );
            let b = polar_file_source(
                2,
                "B",
                vec![45.0, 135.0],
                vec![10.0],
                vec![vec![Some(5.0)], vec![Some(7.0)]],
            );
            open.apply(Command::AddSource {
                index: 0,
                source: Box::new(a),
            })?;
            open.apply(Command::AddSource {
                index: 1,
                source: Box::new(b),
            })?;
            Ok(())
        })
        .expect("add sources");
        app
    }

    #[test]
    fn a_slice_gives_one_curve_per_visible_source_at_that_tws() {
        let app = two_sources();
        let result = plot(&app, Some(9.0)).unwrap();
        assert_eq!(result.curves.len(), 2);
        let a = result.curves.iter().find(|c| c.label == "A").unwrap();
        assert_eq!(a.tws, 9.0);
        // 40°: 5.0, 90°: 7.0, 150°: 7.0 at 9 kn (halfway between the 6 and 12
        // kn columns).
        assert_eq!(
            a.points,
            vec![
                PolarCurvePoint {
                    twa: 40.0,
                    bsp: 5.0
                },
                PolarCurvePoint {
                    twa: 90.0,
                    bsp: 7.0
                },
                PolarCurvePoint {
                    twa: 150.0,
                    bsp: 7.0
                },
            ]
        );
        let b = result.curves.iter().find(|c| c.label == "B").unwrap();
        // B has no 9 kn coverage (its only column is 10 kn): nothing is
        // extrapolated, so its curve is empty at this slice.
        assert!(b.points.is_empty());
    }

    #[test]
    fn all_mode_gives_one_curve_per_source_per_native_wind_speed() {
        let app = two_sources();
        let result = plot(&app, None).unwrap();
        // A has two TWS columns, B has one: three curves in all.
        assert_eq!(result.curves.len(), 3);
        let a_speeds: Vec<f64> = result
            .curves
            .iter()
            .filter(|c| c.label == "A")
            .map(|c| c.tws)
            .collect();
        assert_eq!(a_speeds, vec![6.0, 12.0]);
    }

    #[test]
    fn the_domain_spans_every_visible_sources_tws_axis() {
        let app = two_sources();
        let result = plot(&app, Some(9.0)).unwrap();
        assert_eq!(result.tws_min, Some(6.0));
        assert_eq!(result.tws_max, Some(12.0));
    }

    #[test]
    fn a_hidden_source_is_left_out_of_every_plot() {
        let app = two_sources();
        edit_visible(&app, 1, false);
        let result = plot(&app, None).unwrap();
        assert!(result.curves.iter().all(|c| c.label != "A"));
        assert_eq!(result.tws_min, Some(10.0));
    }

    fn edit_visible(app: &AppState, id: u64, visible: bool) {
        crate::edit::source_visible_set(app, id, visible).unwrap();
    }

    #[test]
    fn a_track_source_gives_no_curve() {
        let app = two_sources();
        app.with_session(|session| {
            let open = session.require_open()?;
            let origin = pe_core::track::TrackOrigin::File {
                name: "race.gpx".to_owned(),
                boat_name: None,
            };
            let track = Source::new(
                SourceId(3),
                "Track",
                Colour::parse("#e15759").unwrap(),
                SourceKind::Track {
                    track: Box::new(pe_core::track::Track::new(pe_core::TrackId(3), origin)),
                },
            );
            open.apply(Command::AddSource {
                index: 2,
                source: Box::new(track),
            })
        })
        .unwrap();
        let result = plot(&app, None).unwrap();
        assert!(result.curves.iter().all(|c| c.label != "Track"));
        assert!(dots(&app, None, false).unwrap().is_empty());
    }

    /// A track without wind has no dots; once environment values are on
    /// its samples (put there by hand here, as M9's fetch will), the ones
    /// within the band of the slice appear, with their flags.
    #[test]
    fn samples_become_dots_once_they_have_wind() {
        let app = two_sources();
        let (track_id, ids) = add_track(&app);
        assert!(dots(&app, Some(10.0), false).unwrap().is_empty());

        app.with_session(|session| {
            let open = session.require_open()?;
            let source = open.project.source_mut(SourceId(track_id)).unwrap();
            let track = source.track_mut().unwrap();
            for (k, sample) in track.samples.iter_mut().enumerate() {
                sample.tws = Some(9.5 + k as f64);
                sample.twa = Some(60.0 + k as f64);
                sample.speed = Some(6.0);
            }
            // As the environment fetch does after writing samples.
            open.touch_samples(track_id);
            Ok(())
        })
        .unwrap();
        let result = dots(&app, Some(10.0), false).unwrap();
        // TWS 9.5 and 10.5 are within ±1 kn of 10; 11.5 is not.
        assert_eq!(plot(&app, None).unwrap().band_kn, DEFAULT_TWS_BAND_KN);
        assert_eq!(
            result.iter().map(|d| d.sample_id).collect::<Vec<_>>(),
            [ids[0], ids[1]]
        );
        assert_eq!(result[0].twa, 60.0);
        assert_eq!(result[0].bsp, 6.0);
        assert!(!result[0].filtered && !result[0].excluded);
        assert_eq!(dots(&app, None, false).unwrap().len(), 3);

        // A wider band takes the third; a hidden track gives none.
        crate::settings::plot_band_set(&app, 2.0).unwrap();
        assert_eq!(dots(&app, Some(10.0), false).unwrap().len(), 3);
        edit_visible(&app, track_id, false);
        assert!(dots(&app, None, false).unwrap().is_empty());
    }

    /// A track of three fixes 10 minutes apart heading east at 3.6 kn,
    /// imported through the real command; returns its source id and sample
    /// ids.
    fn add_track(app: &AppState) -> (u64, Vec<u64>) {
        // The fixture's sources took ids 1 and 2 by hand.
        app.with_session(|session| {
            session.require_open()?.project.next_id = 10;
            Ok(())
        })
        .unwrap();
        let dir = std::env::temp_dir().join(format!("pe-plot-track-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("boat.csv");
        std::fs::write(
            &path,
            "time,lat,lon\n1753531200,0,0\n1753531800,0,0.01\n1753532400,0,0.02\n",
        )
        .unwrap();
        let result = crate::tracks::import(
            app,
            &[crate::tracks::TrackFileRequest {
                path: path.to_string_lossy().into_owned(),
                mapping: None,
                boats: None,
            }],
        )
        .unwrap();
        assert!(result.failures.is_empty(), "{:?}", result.failures);
        app.with_session(|session| {
            let open = session.require_open()?;
            let source = open.project.sources.last().unwrap();
            let ids = source
                .track()
                .unwrap()
                .samples
                .iter()
                .map(|s| s.id.raw())
                .collect();
            Ok((source.id.raw(), ids))
        })
        .unwrap()
    }

    /// The blend is drawn at the slice in its own colour, one curve per
    /// output-grid wind speed in "all", and not at all once hidden.
    #[test]
    fn the_blend_is_drawn_while_shown() {
        let app = two_sources();
        let slice = plot(&app, Some(10.0)).unwrap();
        assert_eq!(slice.blend.len(), 1);
        let curve = &slice.blend[0];
        assert_eq!((curve.source_id, curve.label.as_str()), (None, BLEND_LABEL));
        assert_eq!(curve.colour, pe_core::project::DEFAULT_BLEND_COLOUR);
        assert!(!curve.points.is_empty());
        // A (6–12 kn) and B (10 kn) reach the grid's 6, 8, 10 and 12 kn.
        let all = plot(&app, None).unwrap();
        let speeds: Vec<f64> = all.blend.iter().map(|c| c.tws).collect();
        assert_eq!(speeds, [6.0, 8.0, 10.0, 12.0]);
        crate::blend::blend_visible_set(&app, false).unwrap();
        assert!(plot(&app, Some(10.0)).unwrap().blend.is_empty());
    }

    #[test]
    fn no_open_project_is_refused() {
        let app = state("none");
        assert!(matches!(plot(&app, None), Err(AppError::NoProjectOpen)));
    }

    #[test]
    fn nothing_visible_gives_an_empty_domain_and_no_curves() {
        let app = two_sources();
        edit_visible(&app, 1, false);
        edit_visible(&app, 2, false);
        let result = plot(&app, Some(10.0)).unwrap();
        assert!(result.curves.is_empty());
        assert_eq!(result.tws_min, None);
        assert_eq!(result.tws_max, None);
    }

    /// Two dots of two tracks, one filtered (and placed through the water)
    /// and one excluded.
    fn fixture_dots() -> Vec<PolarSampleDot> {
        vec![
            PolarSampleDot {
                source_id: (2 << 32) | 7,
                sample_id: 11,
                twa: 45.0,
                tws: 10.5,
                bsp: 6.25,
                filtered: false,
                excluded: true,
                through_water: false,
                band: DayBand::Morning,
            },
            PolarSampleDot {
                source_id: 3,
                sample_id: (1 << 32) | 12,
                twa: 135.0,
                tws: 9.5,
                bsp: 8.0,
                filtered: true,
                excluded: false,
                through_water: true,
                band: DayBand::Evening,
            },
        ]
    }

    /// Offsets by hand from the layout; the same bytes the frontend reads.
    #[test]
    fn the_dots_layout_is_the_documented_one() {
        let bytes = pack_dots(&fixture_dots());
        let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        // 4 header + 2×2 sources + 2×3 points + 2 sources + 2×2 ids + 2 flags.
        assert_eq!(bytes.len(), 22 * 4);
        assert_eq!(&bytes[0..4], b"PE2D");
        assert_eq!((1..8).map(word).collect::<Vec<_>>(), [2, 2, 2, 7, 2, 3, 0]);
        assert_eq!(
            (8..14).map(|i| f32::from_bits(word(i))).collect::<Vec<_>>(),
            [45.0, 10.5, 6.25, 135.0, 9.5, 8.0]
        );
        assert_eq!(
            (14..22).map(word).collect::<Vec<_>>(),
            // Flags: the band in bits 8–9, morning = 1 and evening = 3;
            // through the water is bit 3.
            [
                0,
                1,
                11,
                0,
                12,
                1,
                DOT_EXCLUDED | (1 << 8),
                DOT_FILTERED | 8 | (3 << 8)
            ]
        );
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../ui/src/panels/fixtures/dots-v2.bin");
        if std::env::var_os("PE_BLESS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &bytes).unwrap();
        }
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    /// A curve reads the source through its overlay: an edit moves it, an
    /// excluded node leaves a gap (M6 carry).
    #[test]
    fn a_curve_shows_edits_and_leaves_out_excluded_nodes() {
        let app = two_sources();
        crate::polar_edit::polar_edit(
            &app,
            1,
            &crate::polar_edit::EditOp::Type { bsp: Some(7.5) },
            &[crate::polar_edit::PolarCell {
                twa_index: 1,
                tws_index: 1,
            }],
            None,
        )
        .unwrap();
        crate::polar3d::excluded_set(
            &app,
            &[crate::polar3d::PolarNodeRef {
                source_id: 1,
                twa_index: 0,
                tws_index: 1,
            }],
            &[],
            true,
        )
        .unwrap();
        let result = plot(&app, Some(12.0)).unwrap();
        let a = result.curves.iter().find(|c| c.label == "A").unwrap();
        assert_eq!(
            a.points,
            vec![
                PolarCurvePoint {
                    twa: 90.0,
                    bsp: 7.5
                },
                PolarCurvePoint {
                    twa: 150.0,
                    bsp: 9.0
                },
            ]
        );
    }
}
