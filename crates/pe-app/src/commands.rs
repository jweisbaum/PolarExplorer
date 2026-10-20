//! The IPC command surface.
//!
//! Every function here is a Tauri command. Each returns
//! [`crate::error::Result`], so a failure always reaches the frontend as
//! `{ kind, message }`, and each type it returns is exported to TypeScript by
//! `examples/export_bindings.rs`.

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::paths::AppPaths;
use crate::session::Session;

/// Everything the running application holds, managed by Tauri.
#[derive(Debug, Clone)]
pub struct AppState {
    shared: std::sync::Arc<AppServices>,
    boat_context: Option<u64>,
}

impl std::ops::Deref for AppState {
    type Target = AppServices;
    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

/// Shared services; scoped command handles select a boat under the session lock.
#[derive(Debug)]
pub struct AppServices {
    /// Resolved application directories.
    pub paths: AppPaths,
    /// The open project and the settings.
    pub session: std::sync::Mutex<Session>,
    /// Set once the user has answered the unsaved-changes guard for quitting
    /// (spec.md 3.3); from then on nothing stops the exit.
    pub exit_allowed: std::sync::atomic::AtomicBool,
    /// The environment fetch queue (spec.md 7.7).
    pub env_jobs: crate::env::EnvJobs,
    /// The reanalysis provider for the current settings, built on the first
    /// fetch and kept so its opened archives serve every track.
    pub env_provider:
        std::sync::Mutex<Option<(crate::env::ProviderKey, std::sync::Arc<pe_env::Reanalysis>)>>,
    /// What Hindsight Sync listed at the last probe that found it, kept for
    /// the session so an archive can be chosen from it (spec.md 3.4).
    pub reanalysis_listing: std::sync::Mutex<Option<pe_env::hindsight::Listing>>,
    /// The reanalysis GRIB export, at most one at a time (spec.md 7.8).
    pub grib_jobs: crate::grib::GribJobs,
    /// Tracker events downloaded this session, and the running download
    /// (spec.md 7.2).
    pub trackers: crate::trackers::TrackerSession,
    /// Public ORR catalogue and user-started scraping job.
    pub orr: crate::orr::OrrCatalogue,
    /// The ORC catalogue with what was scraped, and the running scrape.
    pub orc: crate::orc::OrcCatalogue,
    /// The scheduled catalogue scrapes' part in quitting (spec.md 5.4).
    pub catalogue_shutdown: crate::catalogues::Shutdown,
    /// The read-only track library's metadata, as last read.
    pub library: crate::library::LibraryState,
    pub boat_import: crate::boats::tracker_project::BoatImportJob,
    /// Whether this session has looked for an earlier version's on-disk
    /// chunk cache to remove (`settings::remove_legacy_cache`).
    pub legacy_cache_checked: std::sync::atomic::AtomicBool,
    /// The earlier version's chunk cache announced and not yet removed.
    pub legacy_cache_pending: std::sync::Mutex<Option<std::path::PathBuf>>,
}

impl AppState {
    /// State over `paths`, with the settings read from disk.
    pub fn new(paths: AppPaths) -> Self {
        let session = Session::load(&paths.settings_file());
        Self {
            shared: std::sync::Arc::new(AppServices {
                paths,
                session: std::sync::Mutex::new(session),
                exit_allowed: std::sync::atomic::AtomicBool::new(false),
                env_jobs: crate::env::EnvJobs::default(),
                env_provider: std::sync::Mutex::new(None),
                reanalysis_listing: std::sync::Mutex::new(None),
                grib_jobs: crate::grib::GribJobs::default(),
                trackers: crate::trackers::TrackerSession::default(),
                orr: crate::orr::OrrCatalogue::default(),
                orc: crate::orc::OrcCatalogue::default(),
                catalogue_shutdown: crate::catalogues::Shutdown::default(),
                library: Default::default(),
                boat_import: Default::default(),
                legacy_cache_checked: std::sync::atomic::AtomicBool::new(false),
                legacy_cache_pending: std::sync::Mutex::new(None),
            }),
            boat_context: None,
        }
    }

    /// An explicit boat context survives delayed IPC and background work.
    pub fn scoped(&self, boat_context: Option<u64>) -> Self {
        Self {
            shared: self.shared.clone(),
            boat_context: boat_context.or(self.boat_context),
        }
    }

    /// Same access convention as Tauri's state handle.
    pub fn inner(&self) -> &Self {
        self
    }

    /// Runs `f` with the session locked.
    pub fn with_session<T>(&self, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| AppError::Internal("the session lock was poisoned".to_owned()))?;
        session.with_boat(self.boat_context, f)
    }
}

/// Build facts for the start screen and the About panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "AppInfo.ts")]
pub struct AppInfo {
    /// Product name.
    pub name: String,
    /// Semantic version of this build.
    pub version: String,
}

/// The application's name and version.
#[tauri::command]
pub fn app_info() -> Result<AppInfo> {
    Ok(AppInfo {
        name: "PolarExplorer".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The version the frontend shows must be the one the bundle is stamped
    /// with, which Tauri takes from its own config, not from Cargo.
    #[test]
    fn app_info_matches_the_bundle_config() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let info = app_info().unwrap();
        assert_eq!(info.name, conf["productName"]);
        assert_eq!(info.version, conf["version"]);
    }

    #[test]
    fn app_info_crosses_ipc_as_name_and_version() {
        let json = serde_json::to_value(app_info().unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "name": "PolarExplorer", "version": env!("CARGO_PKG_VERSION") })
        );
    }
}
