//! Shared by the integration tests: a throwaway application root.
#![allow(
    dead_code,
    clippy::expect_used,
    clippy::unwrap_used,
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

// ------------------------------------------- the remembered archive

/// The archive the fetch tests read: the identity the fixture archive has
/// always been recorded under.
pub const ARCHIVE_URI: &str = "s3://whirlwind-hindsight/hindsight-routing";

/// An archive as Hindsight Sync would list it, at `location`.
pub fn listed_archive(
    id: &str,
    uri: &str,
    location: pe_env::whirlwind::ArchiveLocation,
) -> pe_env::hindsight::ListedArchive {
    let storage = match location {
        pe_env::whirlwind::ArchiveLocation::File { .. } => pe_env::hindsight::Storage::Local,
        pe_env::whirlwind::ArchiveLocation::S3 { .. } => pe_env::hindsight::Storage::Cloud,
    };
    pe_env::hindsight::ListedArchive {
        id: id.to_owned(),
        name: id.to_owned(),
        storage,
        uri: uri.to_owned(),
        source_uri: None,
        mode: "global".to_owned(),
        start: "2000-01-01T00:00:00Z".to_owned(),
        stop: None,
        layout_version: 3,
        chunk_degrees: 5,
        chunk_hours: 72,
        products: vec!["wind10m".to_owned()],
        location,
    }
}

/// Chooses the fixture archive, as Settings would, without Hindsight Sync:
/// at `endpoint` (a `pe_env::whirlwind::testing::Server`'s), or at an
/// address nothing answers when the test's provider reads nothing.
pub fn remember_archive(state: &AppState, endpoint: Option<&str>) {
    let location = pe_env::whirlwind::testing::location(endpoint.unwrap_or("http://127.0.0.1:9"));
    pe_app::reanalysis::remember(state, &listed_archive("fixture", ARCHIVE_URI, location))
        .expect("remember the archive");
}

/// For the live tests: finds Hindsight Sync on this computer and chooses
/// the first archive it lists (cloud first), as a person would in Settings.
pub fn choose_live_archive(state: &AppState) {
    let found = pe_app::reanalysis::scan_now(state).expect("scan");
    assert!(found.detected, "the live tests read through Hindsight Sync");
    let first = found.archives.first().expect("an archive to read");
    pe_app::reanalysis::archive_set(state, Some(first.id.clone())).expect("choose it");
}

// ------------------------------------------- a stub on loopback

pub type Routes = std::sync::Arc<std::sync::Mutex<Vec<(&'static str, u16, String)>>>;

/// A loopback server answering each request with the first route whose
/// path ends the request's, or 404; the routes can change while it runs.
/// Records every request line, and when it came in the process-wide
/// order of requests to any stub, so a test can tell which stub was asked
/// first.
pub struct Stub {
    pub port: u16,
    pub routes: Routes,
    pub seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    pub order: std::sync::Arc<std::sync::Mutex<Vec<u64>>>,
}

/// Counts requests to every stub in this test process.
static ARRIVALS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn stub(routes: Vec<(&'static str, u16, String)>) -> Stub {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let routes: Routes = std::sync::Arc::new(std::sync::Mutex::new(routes));
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (served, log) = (std::sync::Arc::clone(&routes), std::sync::Arc::clone(&seen));
    let ordered = std::sync::Arc::clone(&order);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut socket) = stream else { break };
            let (routes, log) = (std::sync::Arc::clone(&served), std::sync::Arc::clone(&log));
            let ordered = std::sync::Arc::clone(&ordered);
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let mut request = Vec::new();
                let mut buf = [0u8; 4096];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&request).to_string();
                let path = head.split_whitespace().nth(1).unwrap_or("").to_owned();
                log.lock().unwrap().push(path.clone());
                ordered
                    .lock()
                    .unwrap()
                    .push(ARRIVALS.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
                let (status, body) = routes
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|(p, ..)| path.ends_with(p))
                    .map(|(_, c, b)| (*c, b.clone()))
                    .unwrap_or((404, String::new()));
                let _ = write!(
                    socket,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    Stub {
        port,
        routes,
        seen,
        order,
    }
}
