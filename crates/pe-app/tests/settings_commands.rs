#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The settings commands (spec.md 3.4): each change is validated, saved, and
//! survives a restart; a refused change or a failed write changes nothing.

mod common;

use common::TempRoot;
use pe_app::error::AppError;
use pe_app::settings::{
    self, DistanceUnit, MapProjection, NetworkSettings, Settings, SpeedUnit, Units, WaveHeightUnit,
};

#[test]
fn every_setting_is_saved_and_read_back_after_a_restart() {
    let root = TempRoot::new("settings-roundtrip");
    let app = root.state();
    settings::language_set(&app, "fr".to_owned()).unwrap();
    settings::theme_set(&app, "paper".to_owned()).unwrap();
    let units = Units {
        speed: SpeedUnit::Ms,
        wave_height: WaveHeightUnit::Ft,
        distance: DistanceUnit::Km,
    };
    settings::units_set(&app, units).unwrap();
    settings::autosave_mode_set(&app, settings::AutosaveMode::Off).unwrap();
    settings::weather_memory_set(&app, 1024).unwrap();
    settings::network_set(
        &app,
        NetworkSettings {
            concurrency: 4,
            timeout_s: 120,
        },
    )
    .unwrap();
    let last = settings::projection_set(&app, MapProjection::Orthographic).unwrap();

    let restarted = root.state();
    let read = settings::current(&restarted).unwrap();
    assert_eq!(read, last);
    assert_eq!(read.language, "fr");
    assert_eq!(read.theme, "paper");
    assert_eq!(read.units, units);
    assert_eq!(read.weather_memory_mb, 1024);
    assert_eq!(read.network.concurrency, 4);
    assert_eq!(read.projection, MapProjection::Orthographic);
}

/// Hindsight Sync's port, saved like any other setting; a port where it
/// does not answer is still saved.
#[test]
fn every_setting_survives_a_restart() {
    let root = TempRoot::new("reanalysis-port");
    let app = root.state();
    // A port where nothing listens, so the probe that follows never reaches
    // a Hindsight Sync running on this computer.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let saved = pe_app::reanalysis::port_set(&app, port).unwrap();
    assert_eq!(saved.reanalysis.port, port);
    let read = settings::current(&root.state()).unwrap();
    assert_eq!(read, saved);
    assert_eq!(read.reanalysis.port, port);
    let zero = pe_app::reanalysis::port_set(&app, 0).unwrap_err();
    assert!(matches!(zero, AppError::BadOption { field: "Port", .. }));
    assert_eq!(
        settings::current(&root.state()).unwrap().reanalysis.port,
        port
    );
}

#[test]
fn a_refused_value_changes_nothing() {
    let root = TempRoot::new("settings-refused");
    let app = root.state();
    let before = settings::current(&app).unwrap();
    for result in [
        settings::language_set(&app, "tlh".to_owned()),
        settings::theme_set(&app, "neon".to_owned()),
        settings::weather_memory_set(&app, 0),
        settings::weather_memory_set(&app, 100_000),
        settings::network_set(
            &app,
            NetworkSettings {
                concurrency: 0,
                timeout_s: 60,
            },
        ),
        settings::network_set(
            &app,
            NetworkSettings {
                concurrency: 8,
                timeout_s: 1,
            },
        ),
    ] {
        assert!(
            matches!(result, Err(AppError::BadOption { .. })),
            "{result:?}"
        );
    }
    assert_eq!(settings::current(&app).unwrap(), before);
}

#[test]
fn a_failed_write_puts_the_previous_value_back() {
    let root = TempRoot::new("settings-write");
    let app = root.state();
    // A folder where the file should be: every write fails.
    std::fs::create_dir_all(app.paths.settings_file()).unwrap();
    assert!(settings::language_set(&app, "de".to_owned()).is_err());
    assert_eq!(settings::current(&app).unwrap().language, "en");
}

/// An earlier version's on-disk chunk cache is announced once, then
/// removed in the background when asked; only its `chunks` folder goes,
/// never the folder the user had chosen for it, and a `chunks` folder that
/// holds anything but dataset folders is left alone (spec.md 3.4, D27).
#[test]
fn an_earlier_versions_chunk_cache_is_announced_then_removed_once() {
    let root = TempRoot::new("settings-legacy");
    let app = root.state();
    assert!(
        settings::legacy_cache_found(&app).is_none(),
        "nothing there"
    );

    // As an earlier version left it: a chosen folder holding chunks/ and
    // something of the user's.
    let chosen = root.0.join("chosen");
    let chunk = chosen.join("chunks/wb2-era5-1h/10m_u_component_of_wind/539724.0.0");
    std::fs::create_dir_all(chunk.parent().unwrap()).unwrap();
    std::fs::write(&chunk, vec![0u8; 1234]).unwrap();
    std::fs::write(chosen.join("keep-me.txt"), b"user data").unwrap();
    std::fs::create_dir_all(app.paths.settings_file().parent().unwrap()).unwrap();
    std::fs::write(
        app.paths.settings_file(),
        format!(
            r#"{{ "chunk_cache": {{ "location": {:?}, "size_limit_gb": 20 }} }}"#,
            chosen.to_string_lossy()
        ),
    )
    .unwrap();

    // Something that is not a dataset folder: announced not, touched not.
    let stranger = chosen.join("chunks/holiday-photos");
    std::fs::create_dir_all(&stranger).unwrap();
    let wary = root.state();
    assert!(settings::legacy_cache_found(&wary).is_none());
    assert!(settings::remove_legacy_cache(&wary).is_none());
    assert!(chunk.is_file());
    std::fs::remove_dir(&stranger).unwrap();

    let app = root.state();
    let notice = settings::legacy_cache_found(&app).expect("found");
    assert_eq!(notice.bytes, 1234);
    assert!(notice.path.ends_with("chunks"), "{}", notice.path);
    assert!(
        chunk.is_file(),
        "nothing removed before the notice is shown"
    );
    settings::remove_legacy_cache(&app)
        .expect("removing")
        .join()
        .unwrap();
    assert!(!chosen.join("chunks").exists(), "no file left under it");
    assert!(
        chosen.join("keep-me.txt").is_file(),
        "only chunks/ is removed"
    );
    // Said once a session, removed once.
    assert!(settings::legacy_cache_found(&app).is_none());
    assert!(settings::remove_legacy_cache(&app).is_none());
    assert_eq!(Settings::default().weather_memory_mb, 256);
}

#[test]
fn whirlwind_disk_cache_settings_persist_and_clear_preserves_other_files() {
    let root = TempRoot::new("weather-disk-cache");
    let app = root.state();
    let directory = root.file("chosen-cache");
    let saved = pe_app::weather_cache::configure(&app, directory.clone(), 2).unwrap();
    assert_eq!(
        root.state()
            .with_session(|s| Ok(s.settings.weather_cache.clone()))
            .unwrap(),
        saved.weather_cache
    );
    let cache = pe_app::weather_cache::open(&app, &saved.weather_cache).unwrap();
    let key = pe_env::whirlwind::DiskCache::key(
        "s3://whirlwind-hindsight/hindsight-routing",
        "data/c/0/0/0/0",
        "version",
        0,
        100,
    );
    cache.put(&key, &vec![1; 100].into(), 0).unwrap();
    std::fs::write(std::path::Path::new(&directory).join("keep"), b"project").unwrap();
    assert!(pe_app::weather_cache::status(&app, false).unwrap().bytes > 0);
    assert_eq!(pe_app::weather_cache::status(&app, true).unwrap().bytes, 0);
    assert_eq!(
        std::fs::read(std::path::Path::new(&directory).join("keep")).unwrap(),
        b"project"
    );
    for (path, size) in [
        ("relative".to_owned(), 2),
        (directory.clone(), 0),
        (directory, 4097),
    ] {
        assert!(pe_app::weather_cache::configure(&app, path, size).is_err());
    }
    assert_eq!(settings::current(&app).unwrap(), saved);
}
