//! Rebuild the small offline project exercised by the M19 UX test.
use pe_core::{
    Boat, Project, Source, SourceKind,
    polar::{PolarFileFormat, PolarGrid},
    track::{Fix, Sample, Track, TrackOrigin, ValueOrigin},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut p = Project::new("Polar analysis fixture", Boat::default(), 1_790_769_600);
    p.grid.twa = vec![0.0, 45.0, 90.0, 135.0, 180.0];
    p.grid.tws = vec![6.0, 10.0, 14.0];
    let source = Source::new(
        p.allocate_source_id(),
        "Independent sides",
        p.next_palette_colour(),
        SourceKind::PolarFile {
            matrix: None,
            format: PolarFileFormat::Csv,
            file_name: "independent.csv".into(),
            polar: PolarGrid {
                twa: vec![45.0, 90.0, 135.0, 225.0, 270.0, 315.0],
                tws: vec![6.0, 10.0, 14.0],
                bsp: vec![
                    vec![Some(4.0), Some(5.0), Some(6.0)],
                    vec![Some(5.0), Some(6.0), Some(7.0)],
                    vec![Some(4.0), Some(5.0), Some(6.0)],
                    vec![Some(6.0), Some(7.0), Some(8.0)],
                    vec![Some(7.0), Some(8.0), Some(9.0)],
                    vec![Some(6.0), Some(7.0), Some(8.0)],
                ],
            },
        },
    );
    p.sources.push(source);
    let mut track = Track::new(
        p.allocate_track_id(),
        TrackOrigin::File {
            name: "minute-samples.csv".into(),
            boat_name: None,
        },
    );
    for k in 0..120u32 {
        let heading = if k < 60 { 90.0 } else { 270.0 };
        let speed = if k == 20 {
            0.0
        } else {
            6.0 + f64::from(k % 5) / 10.0
        };
        let fix = Fix {
            tws: None,
            twd_from: None,
            t: 1_790_769_600 + i64::from(k) * 60,
            lat: 50.0 + f64::from(k) / 1000.0,
            lon: -1.0,
            cog: Some(heading),
            sog: Some(speed),
        };
        let mut sample = Sample::at(p.allocate_sample_id(), k, &fix);
        sample.heading = Some(heading);
        sample.speed = Some(speed);
        sample.heading_origin = Some(ValueOrigin::Given);
        sample.speed_origin = Some(ValueOrigin::Given);
        sample.tws = Some(10.0);
        sample.twd_from = Some(0.0);
        if k % 4 != 0 {
            sample.hs_m = Some(0.5 + f64::from(k % 8) / 10.0);
            sample.wave_from = Some(f64::from(k * 3));
            sample.wave_period_s = Some(6.0 + f64::from(k % 5));
        }
        if k % 5 != 0 {
            sample.current_speed = Some(0.2);
            sample.current_toward = Some(0.0);
        }
        track.fixes.push(fix);
        track.samples.push(sample);
    }
    let mut source = Source::new(
        p.allocate_source_id(),
        "Minute samples",
        p.next_palette_colour(),
        SourceKind::Track {
            track: Box::new(track),
        },
    );
    source.overlay.filters.min_bsp_kn = None;
    source.overlay.filters.max_heading_change_deg = None;
    p.sources.push(source);
    pe_core::io::save(
        &p,
        std::path::Path::new("tools/webdriver/fixtures/analysis.wpsproj"),
    )?;
    Ok(())
}
