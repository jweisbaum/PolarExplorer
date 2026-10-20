//! The reanalysis archive: chosen among those Hindsight Sync lists, and
//! remembered so fetches read it whether Hindsight Sync is running or not
//! (spec.md 3.4, 7.5).
//!
//! Hindsight Sync is a separate local application. A probe asks its
//! loopback API for the archives it can read; nothing is fetched beyond
//! that listing. The display details of the chosen archive live in
//! `settings.json`, which crosses IPC; its location, credentials included,
//! lives in [`LOCATION_FILE`] beside it, written owner-only, and never
//! crosses IPC or reaches a project.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pe_env::hindsight::{self, ListedArchive, Listing, Storage, Unavailable};
use pe_env::whirlwind::{ArchiveLocation, ArchiveRef};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Context, Result};
use crate::paths::AppPaths;
use crate::settings::{self, Settings};

/// The event carrying a [`ScanResult`] when a probe the person did not
/// press for (the startup one) finds Hindsight Sync.
pub const CHANGED_EVENT: &str = "reanalysis://changed";

/// The file holding the chosen archive's location, beside `settings.json`.
pub const LOCATION_FILE: &str = "reanalysis-archive.json";

/// How long one port is given to answer: the design's detection rule.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Why a fetch is refused with nothing chosen.
pub const NO_ARCHIVE: &str = "Choose a reanalysis archive in Settings before fetching weather.";
/// Why a fetch is refused when the location file is gone or names another
/// archive.
pub const LOCATION_MISSING: &str =
    "The remembered archive's location is missing. Press Scan in Settings.";

/// Where an archive is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "ArchiveStorage.ts")]
#[serde(rename_all = "lowercase")]
pub enum ArchiveStorage {
    /// An S3-compatible bucket.
    Cloud,
    /// A folder on this computer.
    Local,
}

impl From<Storage> for ArchiveStorage {
    fn from(storage: Storage) -> Self {
        match storage {
            Storage::Cloud => Self::Cloud,
            Storage::Local => Self::Local,
        }
    }
}

/// An archive as the Settings dialog shows it: everything but its
/// location, so no credential ever crosses IPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "RememberedArchive.ts")]
pub struct RememberedArchive {
    /// Hindsight Sync's id for it (opaque).
    pub id: String,
    /// Its name.
    pub name: String,
    /// Cloud or this computer.
    pub storage: ArchiveStorage,
    /// Its address as Hindsight Sync gives it.
    pub uri: String,
    /// For a local copy, the cloud archive it copies.
    pub source_uri: Option<String>,
    /// Hindsight Sync's mode (`global`, …).
    pub mode: String,
    /// The first time it holds, RFC 3339 UTC.
    pub start: String,
    /// The last time it holds, RFC 3339 UTC; none while it keeps growing.
    pub stop: Option<String>,
    /// Hindsight Sync's layout version (2 or 3).
    pub layout_version: u32,
    /// Degrees of latitude and longitude per inner chunk.
    pub chunk_degrees: u32,
    /// Hours per inner chunk.
    pub chunk_hours: u32,
    /// What it stores (`wind10m`, …).
    pub products: Vec<String>,
    /// The endpoint host, or the folder, a row shows.
    pub place: String,
}

impl From<&ListedArchive> for RememberedArchive {
    fn from(listed: &ListedArchive) -> Self {
        Self {
            id: listed.id.clone(),
            name: listed.name.clone(),
            storage: listed.storage.into(),
            uri: listed.uri.clone(),
            source_uri: listed.source_uri.clone(),
            mode: listed.mode.clone(),
            start: listed.start.clone(),
            stop: listed.stop.clone(),
            layout_version: listed.layout_version,
            chunk_degrees: listed.chunk_degrees,
            chunk_hours: listed.chunk_hours,
            products: listed.products.clone(),
            place: listed.place(),
        }
    }
}

impl RememberedArchive {
    /// What its values are recorded under: a local copy answers with its
    /// source's address, so a copy and its source count as one archive and
    /// no home folder lands in a shared project.
    pub fn identity(&self) -> &str {
        self.source_uri.as_deref().unwrap_or(&self.uri)
    }
}

/// The reanalysis settings: where Hindsight Sync answers, whether it ever
/// has, and the archive chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "ReanalysisSettings.ts")]
#[serde(default)]
pub struct ReanalysisSettings {
    /// Hindsight Sync's API port on `127.0.0.1`.
    pub port: u16,
    /// Whether a probe has ever found Hindsight Sync; never cleared.
    pub connected_once: bool,
    /// The archive fetches read, if one is chosen.
    pub archive: Option<RememberedArchive>,
}

impl Default for ReanalysisSettings {
    fn default() -> Self {
        Self {
            port: hindsight::DEFAULT_PORT,
            connected_once: false,
            archive: None,
        }
    }
}

/// An archive Hindsight Sync lists but that cannot be chosen, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "UnavailableArchive.ts")]
pub struct UnavailableArchive {
    /// Hindsight Sync's id for it.
    pub id: String,
    /// Its name, when Hindsight Sync gave one.
    pub name: Option<String>,
    /// Cloud or this computer.
    pub storage: ArchiveStorage,
    /// Hindsight Sync's words for why.
    pub reason: String,
}

impl From<&Unavailable> for UnavailableArchive {
    fn from(u: &Unavailable) -> Self {
        Self {
            id: u.id.clone(),
            name: u.name.clone(),
            storage: u.storage.into(),
            reason: u.reason.clone(),
        }
    }
}

/// What a probe or a scan found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "ScanResult.ts")]
pub struct ScanResult {
    /// Whether Hindsight Sync answered now. When not, the lists are the
    /// last ones this session heard, if any.
    pub detected: bool,
    /// The port it answered on, or the saved port when it did not.
    pub port: u16,
    /// The archives that can be chosen: cloud first, then this computer's.
    pub archives: Vec<RememberedArchive>,
    /// The archives listed that cannot be chosen.
    pub unavailable: Vec<UnavailableArchive>,
}

impl ScanResult {
    fn of(detected: bool, port: u16, listing: Option<&Listing>) -> Self {
        Self {
            detected,
            port,
            archives: listing
                .map(|l| l.archives.iter().map(Into::into).collect())
                .unwrap_or_default(),
            unavailable: listing
                .map(|l| l.unavailable.iter().map(Into::into).collect())
                .unwrap_or_default(),
        }
    }
}

/// [`LOCATION_FILE`]'s contents: the chosen archive's id and its location.
/// Written when an archive is chosen and by a probe that lists the chosen
/// id (credentials rotate in Hindsight Sync); removed when the choice is
/// cleared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationFile {
    /// The archive's id, matched against the settings' choice.
    pub id: String,
    /// Where it is, with its credentials.
    pub location: ArchiveLocation,
}

/// Where [`LOCATION_FILE`] is.
pub fn location_path(paths: &AppPaths) -> PathBuf {
    paths.config_dir.join(LOCATION_FILE)
}

/// Writes the location file atomically and owner-only from its first byte:
/// it holds a secret.
fn write_location(file: &Path, contents: &LocationFile) -> Result<()> {
    let json = serde_json::to_string_pretty(contents)
        .doing("write the reanalysis archive's location to", file.display())?;
    write_location_bytes(file, json.as_bytes())
}

fn write_location_bytes(file: &Path, bytes: &[u8]) -> Result<()> {
    pe_core::io::write_atomic_private(file, bytes)
        .doing("write the reanalysis archive's location to", file.display())
}

fn read_location(file: &Path) -> Option<LocationFile> {
    serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()
}

/// The archive a fetch reads: the settings' choice and its location.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedArchive {
    /// What the settings remember of it.
    pub archive: RememberedArchive,
    /// Where it is, with its credentials.
    pub location: ArchiveLocation,
}

impl CapturedArchive {
    /// The name its values are recorded under (provenance and cache keys).
    pub fn identity(&self) -> &str {
        self.archive.identity()
    }

    /// The reader's handle on it.
    pub fn archive_ref(&self) -> ArchiveRef {
        ArchiveRef {
            identity: self.identity().to_owned(),
            location: self.location.clone(),
        }
    }

    /// The version recorded with its values.
    pub fn version(&self) -> String {
        format!("hindsight-layout-{}", self.archive.layout_version)
    }
}

/// The settings' archive and its location, or the refusal a fetch shows.
pub fn captured(state: &AppState) -> Result<CapturedArchive> {
    let archive = state
        .with_session(|s| Ok(s.settings.reanalysis.archive.clone()))?
        .ok_or_else(|| AppError::NoReanalysisArchive {
            reason: NO_ARCHIVE.to_owned(),
        })?;
    let file = read_location(&location_path(&state.paths))
        .filter(|file| file.id == archive.id)
        .ok_or_else(|| AppError::NoReanalysisArchive {
            reason: LOCATION_MISSING.to_owned(),
        })?;
    Ok(CapturedArchive {
        archive,
        location: file.location,
    })
}

/// Makes `listed` the archive fetches read: its details into the settings,
/// its location into its own file, both under the session lock. A failed
/// settings write puts the earlier location file back, so the two never
/// disagree.
pub fn remember(state: &AppState, listed: &ListedArchive) -> Result<Settings> {
    let file = location_path(&state.paths);
    let before = std::fs::read(&file).ok();
    let saved = settings::update(state, |s| {
        write_location(
            &file,
            &LocationFile {
                id: listed.id.clone(),
                location: listed.location.clone(),
            },
        )?;
        s.reanalysis.archive = Some(listed.into());
        Ok(())
    });
    if saved.is_err() {
        let _ = match before {
            Some(bytes) => write_location_bytes(&file, &bytes).map_err(|_| ()),
            None => std::fs::remove_file(&file).map_err(|_| ()),
        };
    }
    saved
}

/// The listing this session last heard, if any.
fn last_listing(state: &AppState) -> Option<Listing> {
    state
        .reanalysis_listing
        .lock()
        .ok()
        .and_then(|listing| listing.clone())
}

/// Asks `port` once. Detected: the listing is kept for the session, the
/// port saved, `connected_once` set, and the chosen archive's details and
/// location refreshed when the listing names it. Not detected: nothing
/// changes, and the result carries the last listing heard.
fn probe_port(state: &AppState, port: u16) -> Result<Option<ScanResult>> {
    let listing =
        hindsight::probe(port, PROBE_TIMEOUT).doing("ask Hindsight Sync on port", port)?;
    let Some(listing) = listing else {
        return Ok(None);
    };
    if let Ok(mut slot) = state.reanalysis_listing.lock() {
        *slot = Some(listing.clone());
    }
    let file = location_path(&state.paths);
    settings::update(state, |s| {
        s.reanalysis.port = port;
        s.reanalysis.connected_once = true;
        let chosen = s.reanalysis.archive.as_ref().map(|a| a.id.clone());
        if let Some(listed) = chosen.and_then(|id| listing.archives.iter().find(|a| a.id == id)) {
            write_location(
                &file,
                &LocationFile {
                    id: listed.id.clone(),
                    location: listed.location.clone(),
                },
            )?;
            s.reanalysis.archive = Some(listed.into());
        }
        Ok(())
    })?;
    Ok(Some(ScanResult::of(true, port, Some(&listing))))
}

/// What a probe that found nothing answers.
fn not_detected(state: &AppState) -> Result<ScanResult> {
    let port = state.with_session(|s| Ok(s.settings.reanalysis.port))?;
    Ok(ScanResult::of(false, port, last_listing(state).as_ref()))
}

/// Probes the saved port only: at startup, when Settings opens, and after
/// the port changes.
pub fn probe_now(state: &AppState) -> Result<ScanResult> {
    let port = state.with_session(|s| Ok(s.settings.reanalysis.port))?;
    first_answer(state, driven_ports().unwrap_or_else(|| vec![port]))
}

/// The first of `ports` where Hindsight Sync answers, or "not detected".
fn first_answer(state: &AppState, ports: Vec<u16>) -> Result<ScanResult> {
    for port in ports {
        if let Some(found) = probe_port(state, port)? {
            return Ok(found);
        }
    }
    not_detected(state)
}

/// A driven run (D25) must never reach the person's own Hindsight Sync: it
/// asks only the port in `PE_DRIVER_HINDSIGHT_PORT` (a test's fake), or no
/// port at all. Compiled only with the WebDriver feature, like
/// `PE_AUTOMATION_ROOT` itself, so a shipped build has no such seam.
#[cfg(feature = "webdriver")]
fn driven_ports() -> Option<Vec<u16>> {
    std::env::var_os("PE_AUTOMATION_ROOT")?;
    let port = std::env::var("PE_DRIVER_HINDSIGHT_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|&p| p != 0);
    Some(port.into_iter().collect())
}

#[cfg(not(feature = "webdriver"))]
fn driven_ports() -> Option<Vec<u16>> {
    None
}

/// The Scan button: the saved port, then Hindsight Sync's default, then
/// the port in its own settings file. The first that answers is saved.
pub fn scan_now(state: &AppState) -> Result<ScanResult> {
    scan_with(state, sibling_port())
}

/// [`scan_now`] with the sibling application's port given, so a test never
/// reads the real home folder.
pub fn scan_with(state: &AppState, sibling: Option<u16>) -> Result<ScanResult> {
    if let Some(ports) = driven_ports() {
        return first_answer(state, ports);
    }
    let saved = state.with_session(|s| Ok(s.settings.reanalysis.port))?;
    let mut ports = vec![saved];
    for port in [Some(hindsight::DEFAULT_PORT), sibling]
        .into_iter()
        .flatten()
    {
        if !ports.contains(&port) {
            ports.push(port);
        }
    }
    first_answer(state, ports)
}

/// Hindsight Sync's own settings file.
fn sibling_settings_file() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| {
        dirs.config_dir()
            .join("com.historysyncer.desktop")
            .join("settings.json")
    })
}

/// The API port in Hindsight Sync's own settings file: `None` when the
/// file is absent or unreadable, its default when the file names none.
/// A read of a file on this computer, not of the network.
pub fn sibling_port() -> Option<u16> {
    sibling_port_in(&sibling_settings_file()?)
}

fn sibling_port_in(file: &Path) -> Option<u16> {
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    match value.get("archive_api").and_then(|api| api.get("port")) {
        None => Some(hindsight::DEFAULT_PORT),
        Some(port) => port
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .filter(|&n| n != 0),
    }
}

/// Saves the port, then probes it; a probe that fails is not the port's
/// failure, so it is ignored and the saved settings answered.
pub fn port_set(state: &AppState, port: u16) -> Result<Settings> {
    if port == 0 {
        return Err(AppError::BadOption {
            field: "Port",
            value: port.to_string(),
        });
    }
    settings::update(state, |s| {
        s.reanalysis.port = port;
        Ok(())
    })?;
    let _ = probe_now(state);
    settings::current(state)
}

/// Chooses the archive fetches read, from the last listing heard; `None`
/// clears the choice and removes the location file.
pub fn archive_set(state: &AppState, id: Option<String>) -> Result<Settings> {
    let Some(id) = id else {
        let file = location_path(&state.paths);
        return settings::update(state, |s| {
            s.reanalysis.archive = None;
            match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).doing(
                    "remove the reanalysis archive's location at",
                    file.display(),
                ),
                _ => Ok(()),
            }
        });
    };
    let listing = last_listing(state).unwrap_or_default();
    if let Some(listed) = listing.archives.iter().find(|a| a.id == id) {
        return remember(state, listed);
    }
    if let Some(unavailable) = listing.unavailable.iter().find(|u| u.id == id) {
        return Err(AppError::Doing {
            doing: "choose",
            what: unavailable.name.clone().unwrap_or(id),
            why: unavailable.reason.clone(),
        });
    }
    Err(AppError::BadOption {
        field: "Reanalysis archive",
        value: id,
    })
}

/// Probes once at startup, off the main thread; tells the frontend what it
/// found when Hindsight Sync answered.
pub fn on_startup<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    let _ = std::thread::Builder::new()
        .name("hindsight-probe".to_owned())
        .spawn(move || {
            use tauri::{Emitter, Manager};
            let state = app.state::<AppState>();
            if let Ok(found) = probe_now(&state)
                && found.detected
            {
                let _ = app.emit(CHANGED_EVENT, &found);
            }
        });
}

/// Runs `work` on a blocking thread: Hindsight Sync's client is
/// `reqwest::blocking`, which panics when made or dropped on a tokio worker,
/// and a panicking command never answers.
async fn off_the_runtime<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| AppError::Internal(format!("Hindsight Sync worker failed: {e}")))?
}

/// [`scan_now`] off the runtime: the Scan button's body.
pub async fn scan(state: AppState) -> Result<ScanResult> {
    off_the_runtime(move || scan_now(&state)).await
}

/// [`probe_now`] off the runtime: the body of the probe when Settings opens.
pub async fn probe(state: AppState) -> Result<ScanResult> {
    off_the_runtime(move || probe_now(&state)).await
}

/// [`port_set`] off the runtime, since it probes.
pub async fn set_port(state: AppState, port: u16) -> Result<Settings> {
    off_the_runtime(move || port_set(&state, port)).await
}

/// The Scan button.
#[tauri::command]
pub async fn reanalysis_scan(state: tauri::State<'_, AppState>) -> Result<ScanResult> {
    scan(state.inner().clone()).await
}

/// Probes the saved port: when Settings opens.
#[tauri::command]
pub async fn reanalysis_probe(state: tauri::State<'_, AppState>) -> Result<ScanResult> {
    probe(state.inner().clone()).await
}

/// Changes Hindsight Sync's port, then probes it.
#[tauri::command]
pub async fn set_reanalysis_port(state: tauri::State<'_, AppState>, port: u16) -> Result<Settings> {
    set_port(state.inner().clone(), port).await
}

/// Chooses the archive fetches read, or clears the choice with null.
#[tauri::command]
pub fn set_reanalysis_archive(
    state: tauri::State<'_, AppState>,
    id: Option<String>,
) -> Result<Settings> {
    archive_set(&state, id)
}

/// Hindsight Sync's product page. Opened in the person's browser when they
/// ask; never fetched by this application (invariant 4).
pub const SITE: &str = "https://hindsight-sync.whirlwindai.com";

/// The platform's opener for a URL. Separate from the spawn so a test can read
/// the program and arguments without running anything.
fn launch_command(url: &str) -> std::process::Command {
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    }
    // `start` takes the first quoted argument for a window title, hence "".
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    }
}

/// Opens Hindsight Sync's site in the browser. Takes no argument, so nothing
/// else can be opened through it.
#[tauri::command]
pub fn open_hindsight_sync_site() -> Result<()> {
    let mut child = launch_command(SITE)
        .stdin(std::process::Stdio::null())
        .spawn()
        .doing("open", SITE)?;
    // Reap the opener off-thread; its exit status does not matter.
    let _ = std::thread::Builder::new()
        .name("open-site".to_owned())
        .spawn(move || {
            let _ = child.wait();
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_site_link_opens_exactly_the_product_page() {
        assert_eq!(SITE, "https://hindsight-sync.whirlwindai.com");
        let command = launch_command(SITE);
        let program = command.get_program().to_string_lossy().into_owned();
        let args: Vec<String> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        if cfg!(target_os = "macos") {
            assert_eq!((program.as_str(), args), ("open", vec![SITE.to_owned()]));
        } else if cfg!(windows) {
            assert_eq!(program, "cmd");
            assert_eq!(args, ["/C", "start", "", SITE]);
        } else {
            assert_eq!(
                (program.as_str(), args),
                ("xdg-open", vec![SITE.to_owned()])
            );
        }
    }

    fn temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pe-reanalysis-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Hindsight Sync's file: `archive_api.port`, its default when absent,
    /// nothing when the file is missing or not JSON.
    #[test]
    fn the_sibling_port_is_read_from_its_settings_file() {
        let dir = temp("sibling");
        let file = dir.join("settings.json");
        assert_eq!(sibling_port_in(&file), None);
        std::fs::write(&file, "{ not json").unwrap();
        assert_eq!(sibling_port_in(&file), None);
        std::fs::write(&file, r#"{"archive_path": "/x"}"#).unwrap();
        assert_eq!(sibling_port_in(&file), Some(8765));
        std::fs::write(&file, r#"{"archive_api": {"port": 9123}}"#).unwrap();
        assert_eq!(sibling_port_in(&file), Some(9123));
        std::fs::write(&file, r#"{"archive_api": {"port": 70000}}"#).unwrap();
        assert_eq!(sibling_port_in(&file), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A local copy is recorded as the cloud archive it copies; the
    /// version names the layout.
    #[test]
    fn a_local_copy_answers_with_its_source() {
        let mut archive = RememberedArchive {
            id: "copy".into(),
            name: "Copy".into(),
            storage: ArchiveStorage::Local,
            uri: "file:///data/copy".into(),
            source_uri: Some("s3://weather/source".into()),
            mode: "global".into(),
            start: "2000-01-01T00:00:00Z".into(),
            stop: None,
            layout_version: 3,
            chunk_degrees: 5,
            chunk_hours: 24,
            products: vec!["wind10m".into()],
            place: "/data/copy".into(),
        };
        let captured = CapturedArchive {
            archive: archive.clone(),
            location: ArchiveLocation::File {
                path: "/data/copy".into(),
            },
        };
        assert_eq!(captured.archive_ref().identity, "s3://weather/source");
        assert_eq!(captured.version(), "hindsight-layout-3");
        archive.source_uri = None;
        assert_eq!(archive.identity(), "file:///data/copy");
    }
}
