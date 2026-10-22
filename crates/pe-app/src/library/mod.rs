//! The local SYRF track library (asked 2026-10-04): a boat metadata file
//! and a folder of individual-track GeoJSON, searched and imported from, and
//! filled by scraping YellowBrick, Geovoile and Blue Water races. A scraped
//! race is its track files and its records in the metadata file. The one
//! database connection is `database`'s read-only metadata download, made
//! only when the person asks for it (asked 2026-10-06).
pub mod catalogue;
pub mod database;
pub mod scrape;
#[cfg(test)]
mod tests;

pub use catalogue::{BoatTrackHit, BoatTrackSearch, import_database_track, search_database_boats};
pub use scrape::{
    ScrapeProgress, cancel_library_scrape, library_scrape_status, shutdown, start_library_scrape,
};

use crate::{
    commands::AppState,
    error::{AppError, Result},
    settings::Settings,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use ts_rs::TS;

/// Where the library's two files live, and how races are scraped into them.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct LibrarySettings {
    /// The folder of individual-track GeoJSON files the metadata names.
    pub geojson_directory: String,
    /// The folder holding `boat-metadata.json`; empty for the application's own.
    pub metadata_directory: String,
    /// Race addresses to scrape, one per line; empty discovers public races.
    #[serde(skip)]
    #[ts(skip)]
    pub scrape_urls: String,
    /// YellowBrick's app user key, to list its catalogue; local only, never logged.
    #[serde(skip)]
    #[ts(skip)]
    pub yellowbrick_user_key: String,
    /// YellowBrick's app device id (UDID), with the key; local only, never logged.
    #[serde(skip)]
    #[ts(skip)]
    pub yellowbrick_device_id: String,
    /// The SYRF PostgreSQL database the metadata download reads, read-only.
    pub database: database::DatabaseConnection,
}

impl std::fmt::Debug for LibrarySettings {
    // The YellowBrick key and device id never reach a log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibrarySettings")
            .field("geojson_directory", &self.geojson_directory)
            .field("metadata_directory", &self.metadata_directory)
            .field("database", &self.database)
            .finish_non_exhaustive()
    }
}

impl Default for LibrarySettings {
    fn default() -> Self {
        let local = PathBuf::from("/Volumes/Disk_Three/s3/syrf-tracks-individual-production");
        Self {
            geojson_directory: if local.is_dir() {
                local.to_string_lossy().into_owned()
            } else {
                String::new()
            },
            metadata_directory: String::new(),
            scrape_urls: String::new(),
            yellowbrick_user_key: String::new(),
            yellowbrick_device_id: String::new(),
            database: database::DatabaseConnection::default(),
        }
    }
}

impl LibrarySettings {
    /// Refuses a relative folder: the library is read from absolute paths only.
    ///
    /// # Errors
    /// [`AppError::Internal`] naming the problem.
    pub fn validate(&self) -> Result<()> {
        if self.yellowbrick_user_key.trim().is_empty()
            != self.yellowbrick_device_id.trim().is_empty()
        {
            return Err(AppError::Internal(
                "Enter both the YellowBrick user key and device ID, or leave both empty".into(),
            ));
        }
        for path in [&self.geojson_directory, &self.metadata_directory] {
            if !path.is_empty() && !std::path::Path::new(path).is_absolute() {
                return Err(AppError::Internal(
                    "Choose an absolute directory path".into(),
                ));
            }
        }
        Ok(())
    }

    /// Where `boat-metadata.json` is read from.
    pub fn metadata_path(&self, state: &AppState) -> PathBuf {
        if self.metadata_directory.is_empty() {
            state.paths.config_dir.join("boat-metadata")
        } else {
            PathBuf::from(&self.metadata_directory)
        }
        .join("boat-metadata.json")
    }
}

/// The catalogue read from the metadata at this path.
type Cached = Option<(PathBuf, Arc<catalogue::Catalogue>)>;

/// The metadata read last, by the path it was read from, and the scrape.
#[derive(Default)]
pub struct LibraryState {
    catalogue: Mutex<Cached>,
    /// The scrape: its progress, and how to cancel it.
    pub(crate) scrape: scrape::ScrapeState,
    /// The metadata download: its progress, and how to cancel it.
    pub(crate) download: database::DownloadState,
}

impl std::fmt::Debug for LibraryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryState").finish_non_exhaustive()
    }
}

impl LibraryState {
    pub(crate) fn lock(&self) -> Result<std::sync::MutexGuard<'_, Cached>> {
        self.catalogue
            .lock()
            .map_err(|_| AppError::Internal("The track library's lock was poisoned".into()))
    }
}

/// Saves the library's two folders.
#[tauri::command]
pub fn set_library_settings(
    state: tauri::State<'_, AppState>,
    settings: LibrarySettings,
) -> Result<Settings> {
    set(&state, settings)
}

/// [`set_library_settings`], for callers without Tauri state.
///
/// # Errors
/// A relative folder, or the settings file not being written.
pub fn set(state: &AppState, settings: LibrarySettings) -> Result<Settings> {
    settings.validate()?;
    if state.library.download.running() {
        return Err(AppError::Internal(
            "Wait for the metadata download to finish before changing the library settings".into(),
        ));
    }
    let changed = self::settings(state)?.metadata_directory != settings.metadata_directory;
    let saved = crate::settings::update(state, |s| {
        s.library = settings;
        Ok(())
    })?;
    if changed {
        catalogue::warm(state.clone());
    }
    Ok(saved)
}

pub(crate) fn settings(state: &AppState) -> Result<LibrarySettings> {
    state.with_session(|s| Ok(s.settings.library.clone()))
}
