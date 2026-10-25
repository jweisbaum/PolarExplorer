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
use pe_app::settings::{self, DistanceUnit, MapProjection, SpeedUnit, Units, WaveHeightUnit};

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
    let last = settings::projection_set(&app, MapProjection::Orthographic).unwrap();

    let restarted = root.state();
    let read = settings::current(&restarted).unwrap();
    assert_eq!(read, last);
    assert_eq!(read.language, "fr");
    assert_eq!(read.theme, "paper");
    assert_eq!(read.units, units);
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
    let before = settings::current(&app).unwrap();
    // A folder where the file should be: every write fails.
    std::fs::remove_file(app.paths.settings_file()).unwrap();
    std::fs::create_dir_all(app.paths.settings_file()).unwrap();
    assert!(settings::language_set(&app, "de".to_owned()).is_err());
    assert_eq!(settings::current(&app).unwrap(), before);
}

#[test]
fn retired_weather_preferences_do_not_limit_memory_or_open_a_disk_cache() {
    let root = TempRoot::new("retired-weather-preferences");
    let blocked = root.0.join("old-cache-file");
    std::fs::write(&blocked, b"keep this file").unwrap();
    let paths = root.state().paths.clone();
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(
        paths.settings_file(),
        serde_json::json!({
            "weather_memory_mb": 16,
            "weather_cache": { "directory": blocked, "max_size_gb": 1 },
            "network": { "timeout_s": 1, "concurrency": 1 }
        })
        .to_string(),
    )
    .unwrap();
    let app = root.state();
    common::remember_archive(&app, None);
    let provider = pe_app::env::provider(&app).unwrap();
    assert_eq!(provider.memory().limit(), u64::MAX);
    assert_eq!(std::fs::read(blocked).unwrap(), b"keep this file");
    assert!(!app.paths.cache_dir.join("whirlwind-hindsight-v1").exists());
    let saved = serde_json::to_value(settings::current(&app).unwrap()).unwrap();
    for retired in ["weather_memory_mb", "weather_cache", "network"] {
        assert!(saved.get(retired).is_none());
    }
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
}
