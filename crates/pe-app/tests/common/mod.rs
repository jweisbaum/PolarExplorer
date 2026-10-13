//! Shared by the integration tests: a throwaway application root.
#![allow(
    dead_code,
    clippy::expect_used,
    reason = "each test binary uses a different part"
)]

use std::path::PathBuf;

use pe_app::commands::AppState;
use pe_app::paths::AppPaths;
use pe_core::polar::{PolarFileFormat, PolarGrid};
use pe_core::track::{Fix, Sample, Track, TrackOrigin};
use pe_core::{Colour, Command, SampleId, Source, SourceId, SourceKind, TrackId};

/// A temporary directory that is removed on drop.
#[derive(Debug)]
pub struct TempRoot(pub PathBuf);

impl TempRoot {
    pub fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pe-app-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).expect("temp root");
        Self(dir)
    }

    pub fn file(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }

    pub fn state(&self) -> AppState {
        AppState::new(AppPaths::in_directory(&self.0.join("app")).expect("paths"))
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ------------------------------------------------- the blend fixture
//
// A polar file and a track that meet in one cell, 90° in 12 kn: the file
// reads 8.0 kn there, the track's segment 6.81 kn at a third of full
// confidence. Shared by the blend-cell tests and the MCP service's.

/// On output-grid points: 45°, 90°, 135° × 6, 10, 14 kn. At 90° it reads
/// 9 kn in 10 kn of wind and 7 kn in 14, so 8.0 kn half way, at 12.
pub fn blend_file_source() -> Source {
    Source::new(
        SourceId(1),
        "File",
        Colour::parse("#4e79a7").expect("a colour"),
        SourceKind::PolarFile {
            matrix: None,
            format: PolarFileFormat::Adrena,
            file_name: "file.pol".to_owned(),
            polar: PolarGrid {
                twa: vec![45.0, 90.0, 135.0],
                tws: vec![6.0, 10.0, 14.0],
                bsp: vec![
                    vec![Some(4.0), Some(5.0), Some(6.0)],
                    vec![Some(5.0), Some(9.0), Some(7.0)],
                    vec![Some(4.5), None, Some(6.5)],
                ],
            },
        },
    )
}

/// Ten samples at 90° in 12 kn, BSP 6.0 … 6.9: one segment cell, the 90th
/// percentile 6.81 (rank 8.1 between 6.8 and 6.9), at a third of full
/// confidence (10 of 30 samples).
pub fn blend_track_source() -> Source {
    let mut track = Track::new(
        TrackId(3),
        TrackOrigin::File {
            name: "race.csv".to_owned(),
            boat_name: None,
        },
    );
    for k in 0..10u64 {
        let fix = Fix {
            tws: None,
            twd_from: None,
            t: 1_753_531_200 + k as i64 * 60,
            lat: 50.0,
            lon: -5.0,
            cog: None,
            sog: None,
        };
        let mut sample = Sample::at(SampleId(200 + k), k as u32, &fix);
        sample.twa = Some(90.0);
        sample.tws = Some(12.0);
        sample.speed = Some(6.0 + k as f64 / 10.0);
        sample.heading = Some(0.0);
        track.fixes.push(fix);
        track.samples.push(sample);
    }
    let mut source = Source::new(
        SourceId(2),
        "Track",
        Colour::parse("#e15759").expect("a colour"),
        SourceKind::Track {
            track: Box::new(track),
        },
    );
    source.overlay.filters.min_bsp_kn = None;
    source.overlay.filters.max_heading_change_deg = None;
    source
}

/// Adds the two blend-fixture sources (ids 1 and 2) to the open project,
/// with a clean history.
pub fn add_blend_sources(app: &AppState) {
    app.with_session(|session| {
        let open = session.require_open()?;
        open.project.next_id = 10_000;
        for (index, source) in [blend_file_source(), blend_track_source()]
            .into_iter()
            .enumerate()
        {
            open.apply(Command::AddSource {
                index,
                source: Box::new(source),
            })?;
        }
        open.history.clear();
        Ok(())
    })
    .expect("the blend fixture's sources");
}
