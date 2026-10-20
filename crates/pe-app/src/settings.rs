//! Global settings, stored in `settings.json` beside the recent list
//! (spec.md 3.4). Ported from VectorEffects' `settings.rs`.
//!
//! Every field has a default and the file **grows**: a field missing from a
//! file written by an older build takes its default. The file is read field
//! by field, so one unreadable value (a hand edit, a build that wrote a unit
//! this one does not know) costs that one preference and not the recent list
//! with it. A file that is not JSON at all falls back to defaults entirely:
//! losing the preferences is far better than refusing to start.
//!
//! Settings are the person's, never the project's: no project, export or
//! history entry reads them.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Context, Result};

/// How many recent projects are remembered (spec.md 4.4).
pub const MAX_RECENT: usize = 10;

/// The interface languages the frontend carries a catalogue for (spec.md
/// 3.5). The first is the fallback.
pub const LANGUAGES: &[&str] = &["en", "fr", "de", "es", "it", "nl", "zh", "ja", "ar"];

/// The theme a new install, or an unknown theme id, gets (spec.md 3.1).
pub const DEFAULT_THEME: &str = "harbour";

/// How much downloaded weather is kept in memory for the session by
/// default, in megabytes (spec.md 3.4): a 5-day race sampled hourly is
/// about 150 MB of blocks, so the second boat of a race downloads nothing.
pub const DEFAULT_WEATHER_MEMORY_MB: u32 = 256;
/// The smallest and largest amounts offered, megabytes.
pub const WEATHER_MEMORY_RANGE_MB: (u32, u32) = (16, 4096);

/// Default number of archive requests in flight at once (spec.md 3.4).
pub const DEFAULT_CONCURRENCY: u32 = 8;
/// The most requests allowed in flight: the archives are shared public
/// buckets, and more than this is hammering them, not speeding up.
pub const MAX_CONCURRENCY: u32 = 32;
/// Default time allowed for one request, in seconds.
pub const DEFAULT_TIMEOUT_S: u32 = 60;
/// The shortest and longest request timeouts offered, in seconds.
pub const TIMEOUT_RANGE_S: (u32, u32) = (5, 600);

/// The narrowest and widest TWS band the 2D polar plot offers for its
/// sample dots, knots either side of the slice (spec.md 9.2).
pub const PLOT_BAND_RANGE_KN: (f64, f64) = (0.25, 5.0);

/// What the autosave thread does with a dirty project (spec.md 3.4, 4.5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "AutosaveMode.ts")]
#[serde(rename_all = "snake_case")]
pub enum AutosaveMode {
    /// A crash-recovery snapshot, offered back on the start screen.
    #[default]
    Recovery,
    /// The project file itself, written in place when it has a path; a
    /// recovery snapshot when it does not.
    Save,
    /// Nothing is written until the user saves.
    Off,
}

/// Display unit for boat and wind speed. Stored values stay knots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "SpeedUnit.ts")]
#[serde(rename_all = "snake_case")]
pub enum SpeedUnit {
    /// Knots.
    #[default]
    Kn,
    /// Metres per second.
    Ms,
    /// Kilometres per hour.
    Kmh,
}

/// Display unit for wave height. Stored values stay metres.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "WaveHeightUnit.ts")]
#[serde(rename_all = "snake_case")]
pub enum WaveHeightUnit {
    /// Metres.
    #[default]
    M,
    /// Feet.
    Ft,
}

/// Display unit for distances.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "DistanceUnit.ts")]
#[serde(rename_all = "snake_case")]
pub enum DistanceUnit {
    /// Nautical miles.
    #[default]
    Nm,
    /// Kilometres.
    Km,
}

/// The display units (spec.md 3.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "Units.ts")]
#[serde(default)]
pub struct Units {
    /// Boat and wind speed.
    pub speed: SpeedUnit,
    /// Wave height.
    pub wave_height: WaveHeightUnit,
    /// Distance.
    pub distance: DistanceUnit,
}

/// Archive used for newly requested historical weather.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "DataSource.ts")]
#[serde(rename_all = "snake_case")]
pub enum DataSource {
    /// Whirlwind Hindsight on S3.
    #[default]
    Whirlwind,
    /// Whirlwind Hindsight on Cloudflare R2 (Source 2).
    WhirlwindR2,
    /// Whirlwind Hindsight on Tigris (Source 3).
    WhirlwindTigris,
}

impl DataSource {
    pub fn whirlwind_source(self) -> pe_env::whirlwind::Source {
        use pe_env::whirlwind::Source;
        match self {
            Self::Whirlwind => Source::S3,
            Self::WhirlwindR2 => Source::R2,
            Self::WhirlwindTigris => Source::Tigris,
        }
    }
}

/// How the reanalysis fetcher uses the network (spec.md 3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "NetworkSettings.ts")]
#[serde(default)]
pub struct NetworkSettings {
    /// Requests in flight at once.
    pub concurrency: u32,
    /// Time allowed for one request, in seconds.
    pub timeout_s: u32,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            concurrency: DEFAULT_CONCURRENCY,
            timeout_s: DEFAULT_TIMEOUT_S,
        }
    }
}

/// The world map's projection (spec.md 9.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "MapProjection.ts")]
#[serde(rename_all = "snake_case")]
pub enum MapProjection {
    /// Plate carrée: longitude and latitude as x and y.
    #[default]
    Equirectangular,
    /// A globe seen from far away.
    Orthographic,
}

/// The MCP service's switch, port and token (spec.md 3.7, D29).
///
/// The token is a plain string in the file the person already owns, written
/// owner-only: it grants a local process what sitting at the keyboard
/// grants, nothing more. Empty means no token, and `mcp::token::matches`
/// refuses everything then.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "McpSettings.ts")]
pub struct McpSettings {
    /// Whether the service listens. Off until the person turns it on.
    pub enabled: bool,
    /// The loopback port it listens on.
    pub port: u16,
    /// The bearer token a client must present; empty while off.
    pub token: String,
}

/// The port a fresh install listens on when the service is first enabled:
/// one above VectorEffects', so both applications can run at once.
pub const DEFAULT_MCP_PORT: u16 = 47392;

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_MCP_PORT,
            token: String::new(),
        }
    }
}

/// The settings file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "AppSettings.ts", rename = "AppSettings")]
#[serde(default)]
pub struct Settings {
    /// Where the SYRF track library's files are, and the database its
    /// metadata download reads. `load` reads an older file's `database`
    /// section for it too, connection fields included.
    pub library: crate::library::LibrarySettings,
    /// Most recently opened or saved projects, newest first.
    #[ts(type = "Array<string>")]
    pub recent_projects: Vec<PathBuf>,
    /// Autosave behaviour.
    pub autosave: AutosaveMode,
    /// The interface language, one of [`LANGUAGES`].
    pub language: String,
    /// The theme id, one of the bundled themes.
    pub theme: String,
    /// Display units.
    pub units: Units,
    /// Downloaded weather kept in memory for the session, megabytes
    /// (spec.md 3.4). Whirlwind can also keep compressed chunks on disk.
    pub weather_memory_mb: u32,
    /// Persistent Whirlwind compressed chunk cache.
    pub weather_cache: crate::weather_cache::WeatherCacheSettings,
    /// Network use by the reanalysis fetcher.
    pub network: NetworkSettings,
    /// Historical wind, waves and current archive.
    pub data_source: DataSource,
    /// The map projection.
    pub projection: MapProjection,
    /// How far from the 2D plot's wind speed a sample may be and still be
    /// drawn, knots either side (spec.md 9.2). A display preference: it
    /// changes what is drawn, never the blend.
    pub plot_tws_band_kn: f64,
    /// The MCP service (spec.md 3.7). Absent from older files: off.
    pub mcp: McpSettings,
    /// When the ORC and ORR catalogues are scraped by themselves
    /// (spec.md 5.4). Absent from older files: only when asked.
    pub catalogues: crate::catalogues::CatalogueSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            library: crate::library::LibrarySettings::default(),
            recent_projects: Vec::new(),
            autosave: AutosaveMode::default(),
            language: LANGUAGES[0].to_owned(),
            theme: DEFAULT_THEME.to_owned(),
            units: Units::default(),
            weather_memory_mb: DEFAULT_WEATHER_MEMORY_MB,
            weather_cache: crate::weather_cache::WeatherCacheSettings::default(),
            network: NetworkSettings::default(),
            data_source: DataSource::default(),
            projection: MapProjection::default(),
            plot_tws_band_kn: crate::polar_plot::DEFAULT_TWS_BAND_KN,
            mcp: McpSettings::default(),
            catalogues: crate::catalogues::CatalogueSettings::default(),
        }
    }
}

/// The ids of the bundled themes, read from the catalogue the frontend uses,
/// so the two can never disagree about which themes exist.
pub fn known_themes() -> &'static [String] {
    static THEMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    THEMES.get_or_init(|| {
        let catalogue: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../ui/src/settings/themes.json"))
                .unwrap_or_default();
        catalogue
            .iter()
            .filter_map(|theme| theme["id"].as_str().map(str::to_owned))
            .collect()
    })
}

/// Reads `key` from a settings object into `slot`, leaving the default when
/// the key is missing or its value unreadable.
fn read_field<T: DeserializeOwned>(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    slot: &mut T,
) {
    if let Some(value) = object.get(key)
        && let Ok(parsed) = serde_json::from_value(value.clone())
    {
        *slot = parsed;
    }
}

impl Settings {
    /// Reads the settings, falling back to defaults on any failure.
    pub fn load(file: &Path) -> Self {
        let value: Option<serde_json::Value> = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
        let mut settings = Self::default();
        if let Some(serde_json::Value::Object(object)) = value {
            if let Some(serde_json::Value::Object(library)) =
                object.get("library").or_else(|| object.get("database"))
            {
                read_field(
                    library,
                    "geojson_directory",
                    &mut settings.library.geojson_directory,
                );
                read_field(
                    library,
                    "metadata_directory",
                    &mut settings.library.metadata_directory,
                );
                read_field(
                    library,
                    "scrape_schedule",
                    &mut settings.library.scrape_schedule,
                );
                read_field(library, "scrape_urls", &mut settings.library.scrape_urls);
                read_field(
                    library,
                    "yellowbrick_user_key",
                    &mut settings.library.yellowbrick_user_key,
                );
                read_field(
                    library,
                    "yellowbrick_device_id",
                    &mut settings.library.yellowbrick_device_id,
                );
                // Before 2026-10-04 the connection was the `database`
                // section's own fields; since 2026-10-06 it is `library`'s
                // `database`.
                let connection = match library.get("database") {
                    Some(serde_json::Value::Object(c)) => c,
                    _ => library,
                };
                let slot = &mut settings.library.database;
                read_field(connection, "host", &mut slot.host);
                read_field(connection, "port", &mut slot.port);
                read_field(connection, "name", &mut slot.name);
                read_field(connection, "user", &mut slot.user);
                read_field(connection, "password", &mut slot.password);
                read_field(connection, "tls", &mut slot.tls);
            }
            read_field(&object, "recent_projects", &mut settings.recent_projects);
            read_field(&object, "autosave", &mut settings.autosave);
            read_field(&object, "language", &mut settings.language);
            read_field(&object, "theme", &mut settings.theme);
            // The nested groups are read member by member too: one unit this
            // build does not know must not reset the other two with it.
            if let Some(serde_json::Value::Object(units)) = object.get("units") {
                read_field(units, "speed", &mut settings.units.speed);
                read_field(units, "wave_height", &mut settings.units.wave_height);
                read_field(units, "distance", &mut settings.units.distance);
            }
            // An earlier build's `chunk_cache` (a folder and a size limit on
            // disk) is not read. The new Whirlwind cache has its own format
            // and directory, separate from `remove_legacy_cache`.
            read_field(
                &object,
                "weather_memory_mb",
                &mut settings.weather_memory_mb,
            );
            if let Some(serde_json::Value::Object(network)) = object.get("network") {
                read_field(network, "concurrency", &mut settings.network.concurrency);
                read_field(network, "timeout_s", &mut settings.network.timeout_s);
            }
            read_field(&object, "data_source", &mut settings.data_source);
            if let Some(serde_json::Value::Object(cache)) = object.get("weather_cache") {
                read_field(cache, "directory", &mut settings.weather_cache.directory);
                read_field(
                    cache,
                    "max_size_gb",
                    &mut settings.weather_cache.max_size_gb,
                );
            }
            read_field(&object, "projection", &mut settings.projection);
            read_field(&object, "plot_tws_band_kn", &mut settings.plot_tws_band_kn);
            if let Some(serde_json::Value::Object(mcp)) = object.get("mcp") {
                read_field(mcp, "enabled", &mut settings.mcp.enabled);
                read_field(mcp, "port", &mut settings.mcp.port);
                read_field(mcp, "token", &mut settings.mcp.token);
            }
            if let Some(serde_json::Value::Object(catalogues)) = object.get("catalogues") {
                read_field(
                    catalogues,
                    "orc_schedule",
                    &mut settings.catalogues.orc_schedule,
                );
                read_field(
                    catalogues,
                    "orr_schedule",
                    &mut settings.catalogues.orr_schedule,
                );
            }
        }
        settings.normalised()
    }

    /// Replaces anything out of range with its default. A file can be edited
    /// by hand or written by another build; a bad value costs one
    /// preference, never the launch.
    ///
    /// Not clamped: a limit of 0 or 500 is not a near miss of 1 or 32 but a
    /// value this build cannot vouch for, and the spec says an out-of-range
    /// value costs that preference (spec.md 3.4) — which is what the default
    /// is for.
    pub fn normalised(mut self) -> Self {
        let defaults = Self::default();
        self.weather_cache.normalise();
        self.recent_projects.truncate(MAX_RECENT);
        if !LANGUAGES.contains(&self.language.as_str()) {
            self.language = defaults.language;
        }
        if !known_themes().contains(&self.theme) {
            self.theme = defaults.theme;
        }
        if !(WEATHER_MEMORY_RANGE_MB.0..=WEATHER_MEMORY_RANGE_MB.1)
            .contains(&self.weather_memory_mb)
        {
            self.weather_memory_mb = defaults.weather_memory_mb;
        }
        if !(1..=MAX_CONCURRENCY).contains(&self.network.concurrency) {
            self.network.concurrency = defaults.network.concurrency;
        }
        if !(TIMEOUT_RANGE_S.0..=TIMEOUT_RANGE_S.1).contains(&self.network.timeout_s) {
            self.network.timeout_s = defaults.network.timeout_s;
        }
        if !(PLOT_BAND_RANGE_KN.0..=PLOT_BAND_RANGE_KN.1).contains(&self.plot_tws_band_kn) {
            self.plot_tws_band_kn = defaults.plot_tws_band_kn;
        }
        // Port 0 is "any port": no client configuration could name it.
        if self.mcp.port == 0 {
            self.mcp.port = defaults.mcp.port;
        }
        // On with no token would be a listener nothing can talk to; a
        // token while off is one nothing should hold. Either is a file
        // edited by hand: the service is off until switched on again.
        if self.mcp.enabled == self.mcp.token.is_empty() {
            self.mcp.enabled = false;
            self.mcp.token.clear();
        }
        self
    }

    /// Writes the settings.
    pub fn save(&self, file: &Path) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).doing("write the settings to", file.display())?;
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .doing("make the settings folder at", parent.display())?;
        }
        pe_core::io::write_atomic(file, json.as_bytes())
            .doing("write the settings to", file.display())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))
                .doing("protect the settings at", file.display())?;
        }
        Ok(())
    }

    /// Moves `path` to the front of the recent list.
    pub fn remember(&mut self, path: &Path) {
        self.recent_projects.retain(|existing| existing != path);
        self.recent_projects.insert(0, path.to_path_buf());
        self.recent_projects.truncate(MAX_RECENT);
    }

    /// The weather memory limit, bytes.
    pub fn weather_memory_bytes(&self) -> u64 {
        u64::from(self.weather_memory_mb) << 20
    }
}

/// Applies `change` to the settings and saves them. A refused change or a
/// failed write leaves the settings exactly as they were, so what the
/// frontend shows is always what is on disk.
pub fn update(
    state: &AppState,
    change: impl FnOnce(&mut Settings) -> Result<()>,
) -> Result<Settings> {
    let file = state.paths.settings_file();
    state.with_session(|session| {
        let before = session.settings.clone();
        let saved = change(&mut session.settings).and_then(|()| session.settings.save(&file));
        match saved {
            Ok(()) => Ok(session.settings.clone()),
            Err(error) => {
                session.settings = before;
                Err(error)
            }
        }
    })
}

/// The current settings.
#[tauri::command]
pub fn app_settings(state: tauri::State<'_, AppState>) -> Result<Settings> {
    current(&state)
}

/// [`app_settings`] without a Tauri handle.
pub fn current(state: &AppState) -> Result<Settings> {
    state.with_session(|session| Ok(session.settings.clone()))
}

/// Changes the interface language and relabels the native menu.
#[tauri::command]
pub fn set_language(app: tauri::AppHandle, language: String) -> Result<Settings> {
    use tauri::Manager;
    let saved = language_set(&app.state::<AppState>(), language)?;
    crate::menu::install(&app, &saved.language);
    Ok(saved)
}

/// [`set_language`] without a Tauri handle.
pub fn language_set(state: &AppState, language: String) -> Result<Settings> {
    update(state, |settings| {
        if !LANGUAGES.contains(&language.as_str()) {
            return Err(AppError::BadOption {
                field: "language",
                value: language,
            });
        }
        settings.language = language;
        Ok(())
    })
}

/// Changes the theme.
#[tauri::command]
pub fn set_theme(state: tauri::State<'_, AppState>, theme: String) -> Result<Settings> {
    theme_set(&state, theme)
}

/// [`set_theme`] without a Tauri handle.
pub fn theme_set(state: &AppState, theme: String) -> Result<Settings> {
    update(state, |settings| {
        if !known_themes().contains(&theme) {
            return Err(AppError::BadOption {
                field: "theme",
                value: theme,
            });
        }
        settings.theme = theme;
        Ok(())
    })
}

/// Changes the display units.
#[tauri::command]
pub fn set_units(state: tauri::State<'_, AppState>, units: Units) -> Result<Settings> {
    units_set(&state, units)
}

/// [`set_units`] without a Tauri handle.
pub fn units_set(state: &AppState, units: Units) -> Result<Settings> {
    update(state, |settings| {
        settings.units = units;
        Ok(())
    })
}

/// Changes what autosave does.
#[tauri::command]
pub fn set_autosave_mode(
    state: tauri::State<'_, AppState>,
    mode: AutosaveMode,
) -> Result<Settings> {
    autosave_mode_set(&state, mode)
}

/// [`set_autosave_mode`] without a Tauri handle.
pub fn autosave_mode_set(state: &AppState, mode: AutosaveMode) -> Result<Settings> {
    update(state, |settings| {
        settings.autosave = mode;
        Ok(())
    })
}

/// Changes how much downloaded weather is kept in memory for the session.
#[tauri::command]
pub fn set_weather_memory(state: tauri::State<'_, AppState>, megabytes: u32) -> Result<Settings> {
    weather_memory_set(&state, megabytes)
}

/// [`set_weather_memory`] without a Tauri handle. An out-of-range amount
/// is refused rather than clamped: the user typed it and should see why it
/// did not take. The next fetch starts with an empty memory of the new
/// size.
pub fn weather_memory_set(state: &AppState, megabytes: u32) -> Result<Settings> {
    update(state, |settings| {
        if !(WEATHER_MEMORY_RANGE_MB.0..=WEATHER_MEMORY_RANGE_MB.1).contains(&megabytes) {
            return Err(AppError::BadOption {
                field: "Weather kept in memory",
                value: megabytes.to_string(),
            });
        }
        settings.weather_memory_mb = megabytes;
        Ok(())
    })
}

/// Changes the network settings.
#[tauri::command]
pub fn set_network(
    state: tauri::State<'_, AppState>,
    network: NetworkSettings,
) -> Result<Settings> {
    network_set(&state, network)
}

/// [`set_network`] without a Tauri handle.
pub fn network_set(state: &AppState, network: NetworkSettings) -> Result<Settings> {
    update(state, |settings| {
        if !(1..=MAX_CONCURRENCY).contains(&network.concurrency) {
            return Err(AppError::BadOption {
                field: "Concurrent requests",
                value: network.concurrency.to_string(),
            });
        }
        if !(TIMEOUT_RANGE_S.0..=TIMEOUT_RANGE_S.1).contains(&network.timeout_s) {
            return Err(AppError::BadOption {
                field: "Request timeout",
                value: network.timeout_s.to_string(),
            });
        }
        settings.network = network;
        Ok(())
    })
}

/// Changes the archive for subsequent weather requests.
#[tauri::command]
pub fn set_data_source(state: tauri::State<'_, AppState>, source: DataSource) -> Result<Settings> {
    data_source_set(&state, source)
}

/// Saves the preference without altering already fetched project values.
pub fn data_source_set(state: &AppState, source: DataSource) -> Result<Settings> {
    update(state, |settings| {
        settings.data_source = source;
        Ok(())
    })
}

/// Changes the map projection.
#[tauri::command]
pub fn set_projection(
    state: tauri::State<'_, AppState>,
    projection: MapProjection,
) -> Result<Settings> {
    projection_set(&state, projection)
}

/// [`set_projection`] without a Tauri handle.
pub fn projection_set(state: &AppState, projection: MapProjection) -> Result<Settings> {
    update(state, |settings| {
        settings.projection = projection;
        Ok(())
    })
}

/// The on-disk chunk cache an earlier version kept, being removed (spec.md
/// 3.4, D27): what the status line says once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "LegacyCacheNotice.ts")]
pub struct LegacyCacheNotice {
    /// The folder.
    pub path: String,
    /// Bytes it held.
    pub bytes: u64,
}

/// Total size of the regular files under `dir`; zero if it does not exist.
fn folder_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(std::result::Result::ok)
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => folder_size(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

/// Where an earlier version kept downloaded chunks: the `chunks` folder in
/// the location its settings named (read from the raw settings file, since
/// these settings no longer have the field), or in the platform cache
/// directory.
pub fn legacy_chunk_dir(settings_file: &Path, default_cache_dir: &Path) -> PathBuf {
    let location = std::fs::read_to_string(settings_file)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v["chunk_cache"]["location"].as_str().map(str::to_owned))
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty() && Path::new(l).is_absolute());
    location
        .map_or_else(|| default_cache_dir.to_path_buf(), PathBuf::from)
        .join("chunks")
}

/// The dataset folders an earlier version's chunk cache wrote, one per
/// archive it read.
const LEGACY_CACHE_FOLDERS: [&str; 10] = [
    "whirlwind-hindsight",
    "whirlwind-hindsight-r2",
    "whirlwind-hindsight-tigris",
    "wb2-era5-1h",
    "arco-era5",
    "cmems-nws-my-uv-geo",
    "cmems-ibi-my-cur-geo",
    "cmems-glo-merged-uv-geo",
    "globcurrent-my-geo",
    "globcurrent-nrt-geo",
];

/// Whether `dir` looks like nothing but an earlier version's chunk cache:
/// every entry a folder named for a dataset it read (`wb2-era5-1h`, …).
/// A folder holding anything else is not ours to delete.
fn only_dataset_folders(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(std::result::Result::ok).all(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        (entry.file_type().is_ok_and(|kind| kind.is_dir())
            && LEGACY_CACHE_FOLDERS.contains(&name.as_ref()))
            || name == ".DS_Store"
    })
}

/// Says, once a session, that an earlier version's chunk cache is there to
/// be removed, and remembers it for [`remove_legacy_cache`]; `None` when
/// there is none, or its folder holds anything but the dataset folders the
/// cache wrote (then it is left alone).
pub fn legacy_cache_found(state: &AppState) -> Option<LegacyCacheNotice> {
    use std::sync::atomic::Ordering;
    if state.legacy_cache_checked.swap(true, Ordering::SeqCst) {
        return None;
    }
    let dir = legacy_chunk_dir(&state.paths.settings_file(), &state.paths.cache_dir);
    if !dir.is_dir() || !only_dataset_folders(&dir) {
        return None;
    }
    let notice = LegacyCacheNotice {
        path: dir.to_string_lossy().into_owned(),
        bytes: folder_size(&dir),
    };
    if let Ok(mut pending) = state.legacy_cache_pending.lock() {
        *pending = Some(dir);
    }
    Some(notice)
}

/// Removes, in the background, the chunk cache [`legacy_cache_found`] said
/// was there: lossless by invariant 3, since every value a project uses is
/// saved in the project. Only the `chunks` folder goes, never the folder
/// the user chose to hold it. The handle is for tests.
pub fn remove_legacy_cache(state: &AppState) -> Option<std::thread::JoinHandle<()>> {
    let dir = state.legacy_cache_pending.lock().ok()?.take()?;
    std::thread::Builder::new()
        .name("old-chunk-cache".to_owned())
        .spawn(move || {
            let _ = std::fs::remove_dir_all(&dir);
        })
        .ok()
}

/// An earlier version's on-disk chunk cache, the first time the frontend
/// asks in a session; `None` when there is nothing to remove. The frontend
/// shows it on the status line, then asks for the removal.
#[tauri::command(async)]
pub fn legacy_cache_notice(state: tauri::State<'_, AppState>) -> Result<Option<LegacyCacheNotice>> {
    Ok(legacy_cache_found(&state))
}

/// Removes the chunk cache [`legacy_cache_notice`] announced.
#[tauri::command(async)]
pub fn remove_old_chunk_cache(state: tauri::State<'_, AppState>) -> Result<()> {
    remove_legacy_cache(&state);
    Ok(())
}

/// Changes the 2D polar plot's TWS band for sample dots, knots.
#[tauri::command]
pub fn set_plot_band(state: tauri::State<'_, AppState>, band_kn: f64) -> Result<Settings> {
    plot_band_set(&state, band_kn)
}

/// [`set_plot_band`] without a Tauri handle.
pub fn plot_band_set(state: &AppState, band_kn: f64) -> Result<Settings> {
    update(state, |settings| {
        if !(PLOT_BAND_RANGE_KN.0..=PLOT_BAND_RANGE_KN.1).contains(&band_kn) {
            return Err(AppError::BadOption {
                field: "Dot band",
                value: band_kn.to_string(),
            });
        }
        settings.plot_tws_band_kn = band_kn;
        Ok(())
    })
}

/// What the Settings dialog shows about the MCP service (spec.md 3.7).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "McpStatus.ts")]
pub struct McpStatus {
    /// Whether the setting is on.
    pub enabled: bool,
    /// The port in the settings.
    pub port: u16,
    /// The token, so the dialog can build a client configuration.
    pub token: String,
    /// The port actually bound, or null while off or if binding failed.
    pub bound_port: Option<u16>,
    /// Why the listener is not up although the setting is on.
    pub bind_error: Option<String>,
    /// Open client sessions.
    pub sessions: u32,
    /// The last tool a client called, if any.
    pub last_tool: Option<String>,
    /// The clients the dialog can add the service to on this platform.
    pub clients: Vec<crate::mcp::clients::McpClient>,
}

/// Reads the service's settings and live state.
#[tauri::command]
pub fn mcp_status(
    state: tauri::State<'_, AppState>,
    service: tauri::State<'_, crate::mcp::McpService>,
) -> Result<McpStatus> {
    let mcp = state.with_session(|session| Ok(session.settings.mcp.clone()))?;
    Ok(service.status(&mcp))
}

/// Turns the service on or off and sets its port. Enabling issues a fresh
/// token; disabling clears it and drops the listener.
///
/// Generic over the Tauri runtime so the integration tests can drive it
/// through a mock application. `async`: `McpService::apply` calls
/// `Running::stop`, which can block briefly waiting for the listener's
/// accept loop to confirm its socket closed, so this must run on Tauri's
/// thread pool and never the main (webview) thread.
#[tauri::command(async)]
pub fn mcp_set<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    service: tauri::State<'_, crate::mcp::McpService>,
    enabled: bool,
    port: u16,
) -> Result<McpStatus> {
    // Port 0 is "any free port": a service whose port moves every launch is
    // no use to a client configuration.
    if port == 0 {
        return Err(AppError::BadOption {
            field: "Port",
            value: port.to_string(),
        });
    }
    let settings = update(&state, |settings| {
        let turning_on = enabled && !settings.mcp.enabled;
        settings.mcp.enabled = enabled;
        settings.mcp.port = port;
        if turning_on || (enabled && settings.mcp.token.is_empty()) {
            settings.mcp.token = crate::mcp::token::fresh();
        }
        if !enabled {
            settings.mcp.token.clear();
        }
        Ok(())
    })?;
    service.apply(&app, &settings.mcp);
    Ok(service.status(&settings.mcp))
}

/// Issues a new token and restarts the listener with it. Refused while the
/// service is off: there is nothing to rotate.
///
/// Generic and `async` for the same reasons as [`mcp_set`].
#[tauri::command(async)]
pub fn mcp_rotate_token<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    service: tauri::State<'_, crate::mcp::McpService>,
) -> Result<McpStatus> {
    let settings = update(&state, |settings| {
        if !settings.mcp.enabled {
            return Err(AppError::BadOption {
                field: "MCP service",
                value: "off".to_owned(),
            });
        }
        settings.mcp.token = crate::mcp::token::fresh();
        Ok(())
    })?;
    service.apply(&app, &settings.mcp);
    Ok(service.status(&settings.mcp))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pe-settings-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A file written before the MCP service existed loads with it off,
    /// on the default port, with no token (spec.md 3.7).
    #[test]
    fn a_settings_file_without_mcp_loads_with_it_off() {
        let file = temp("no-mcp").join("settings.json");
        std::fs::write(&file, r#"{"language":"fr","theme":"harbour"}"#).unwrap();
        let settings = Settings::load(&file);
        assert_eq!(settings.language, "fr");
        assert_eq!(
            settings.mcp,
            McpSettings {
                enabled: false,
                port: 47392,
                token: String::new()
            }
        );
    }

    /// The service's settings survive a save and a load, and a file that
    /// says on without a token, or holds a token while off, is off with
    /// none: no listener nothing can talk to, no token nothing should hold.
    #[test]
    fn mcp_settings_round_trip_and_a_contradictory_file_is_off() {
        let file = temp("mcp").join("settings.json");
        let on = Settings {
            mcp: McpSettings {
                enabled: true,
                port: 50123,
                token: "t".repeat(43),
            },
            ..Settings::default()
        };
        on.save(&file).unwrap();
        assert_eq!(Settings::load(&file).mcp, on.mcp);

        for (enabled, token) in [(true, ""), (false, "left-behind")] {
            std::fs::write(
                &file,
                serde_json::json!({ "mcp": { "enabled": enabled, "port": 50123, "token": token } })
                    .to_string(),
            )
            .unwrap();
            let loaded = Settings::load(&file).mcp;
            assert!(!loaded.enabled && loaded.token.is_empty(), "{loaded:?}");
            assert_eq!(loaded.port, 50123, "the port is kept");
        }
        // Port 0 names no port a client could be given.
        std::fs::write(&file, r#"{"mcp":{"enabled":false,"port":0,"token":""}}"#).unwrap();
        assert_eq!(Settings::load(&file).mcp.port, DEFAULT_MCP_PORT);
    }

    #[test]
    fn recent_entries_move_to_the_front_without_duplicating() {
        let mut s = Settings::default();
        s.remember(Path::new("/a"));
        s.remember(Path::new("/b"));
        s.remember(Path::new("/a"));
        assert_eq!(
            s.recent_projects,
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
    }

    #[test]
    fn the_recent_list_holds_ten() {
        let mut s = Settings::default();
        for i in 0..15 {
            s.remember(Path::new(&format!("/p{i}")));
        }
        assert_eq!(s.recent_projects.len(), MAX_RECENT);
        assert_eq!(s.recent_projects[0], PathBuf::from("/p14"));
    }

    /// The defaults spec.md 3.4 names.
    #[test]
    fn the_defaults_are_the_ones_the_spec_names() {
        let s = Settings::default();
        assert_eq!(s.language, "en");
        assert_eq!(s.data_source, DataSource::Whirlwind);
        assert_eq!(s.theme, "harbour");
        assert_eq!(s.units.speed, SpeedUnit::Kn);
        assert_eq!(s.units.wave_height, WaveHeightUnit::M);
        assert_eq!(s.units.distance, DistanceUnit::Nm);
        assert_eq!(s.autosave, AutosaveMode::Recovery);
        assert_eq!(s.weather_memory_mb, 256);
        assert_eq!(s.network.concurrency, 8);
        assert_eq!(s.projection, MapProjection::Equirectangular);
        assert!(known_themes().contains(&s.theme));
        for id in ["harbour", "midnight", "ocean", "plum", "ember", "paper"] {
            assert!(known_themes().iter().any(|k| k == id), "{id}");
        }
    }

    #[test]
    fn a_broken_or_partial_file_falls_back_to_defaults() {
        let dir = temp("partial");
        let file = dir.join("settings.json");
        std::fs::write(&file, "{ not json").unwrap();
        assert_eq!(Settings::load(&file), Settings::default());

        std::fs::write(&file, r#"{ "recent_projects": ["/x.wpsproj"] }"#).unwrap();
        let partial = Settings::load(&file);
        assert_eq!(partial.recent_projects, vec![PathBuf::from("/x.wpsproj")]);
        assert_eq!(partial.autosave, AutosaveMode::Recovery);

        let mut s = Settings {
            autosave: AutosaveMode::Off,
            language: "de".to_owned(),
            theme: "paper".to_owned(),
            ..Settings::default()
        };
        s.remember(Path::new("/y.wpsproj"));
        s.save(&file).unwrap();
        assert_eq!(Settings::load(&file), s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Open Data choice is gone: a file that saved it reads as the
    /// default, Whirlwind on S3.
    #[test]
    fn a_saved_open_data_choice_reads_as_whirlwind() {
        let dir = temp("open-data");
        let file = dir.join("settings.json");
        std::fs::write(&file, r#"{ "data_source": "open_data", "theme": "paper" }"#).unwrap();
        let s = Settings::load(&file);
        assert_eq!(s.data_source, DataSource::Whirlwind);
        assert_eq!(s.theme, "paper");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One bad value costs that preference, not the recent list beside it.
    #[test]
    fn an_unreadable_field_loses_only_itself() {
        let dir = temp("field");
        let file = dir.join("settings.json");
        std::fs::write(
            &file,
            r#"{ "recent_projects": ["/keep.wpsproj"], "units": { "speed": "furlongs" },
                 "language": "tlh", "theme": "neon", "autosave": "save",
                 "network": { "concurrency": 500, "timeout_s": 1 },
                 "weather_memory_mb": 1, "data_source": "unknown" }"#,
        )
        .unwrap();
        let s = Settings::load(&file);
        assert_eq!(s.recent_projects, vec![PathBuf::from("/keep.wpsproj")]);
        assert_eq!(s.autosave, AutosaveMode::Save);
        assert_eq!(s.units, Units::default());
        assert_eq!(s.language, "en");
        assert_eq!(s.theme, DEFAULT_THEME);
        // Out of range is the default, not the nearest allowed value.
        assert_eq!(s.network.concurrency, DEFAULT_CONCURRENCY);
        assert_eq!(s.network.timeout_s, DEFAULT_TIMEOUT_S);
        assert_eq!(s.weather_memory_mb, DEFAULT_WEATHER_MEMORY_MB);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One unit this build does not know resets that unit only; the other
    /// two, and the other members of the nested groups, are kept.
    #[test]
    fn each_nested_member_is_read_on_its_own() {
        let dir = temp("nested");
        let file = dir.join("settings.json");
        std::fs::write(
            &file,
            r#"{ "units": { "speed": "furlongs", "wave_height": "ft", "distance": "km" },
                 "network": { "concurrency": 12, "timeout_s": "soon" },
                 "chunk_cache": { "location": "/old", "size_limit_gb": 55 },
                 "weather_memory_mb": 512 }"#,
        )
        .unwrap();
        let s = Settings::load(&file);
        assert_eq!(
            s.units,
            Units {
                speed: SpeedUnit::Kn,
                wave_height: WaveHeightUnit::Ft,
                distance: DistanceUnit::Km,
            }
        );
        assert_eq!(s.network.concurrency, 12);
        assert_eq!(s.network.timeout_s, DEFAULT_TIMEOUT_S);
        assert_eq!(s.weather_memory_mb, 512);
        // An earlier build's chunk cache settings are not carried: the
        // next save writes none.
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("chunk_cache"), "{json}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The edges of each range are in range; one past them is the default.
    #[test]
    fn range_edges_are_kept_and_one_past_is_the_default() {
        let at_edges = Settings {
            weather_memory_mb: WEATHER_MEMORY_RANGE_MB.1,
            network: NetworkSettings {
                concurrency: 1,
                timeout_s: TIMEOUT_RANGE_S.1,
            },
            ..Settings::default()
        };
        assert_eq!(at_edges.clone().normalised(), at_edges);
        let past = Settings {
            weather_memory_mb: WEATHER_MEMORY_RANGE_MB.0 - 1,
            network: NetworkSettings {
                concurrency: MAX_CONCURRENCY + 1,
                timeout_s: TIMEOUT_RANGE_S.0 - 1,
            },
            ..Settings::default()
        }
        .normalised();
        assert_eq!(past.weather_memory_mb, DEFAULT_WEATHER_MEMORY_MB);
        assert_eq!(past.network.concurrency, DEFAULT_CONCURRENCY);
        assert_eq!(past.network.timeout_s, DEFAULT_TIMEOUT_S);
    }

    /// An earlier version's chunks were in `chunks` inside the location
    /// its settings named, or inside the platform cache directory.
    #[test]
    fn the_legacy_chunk_folder_is_found_where_it_was_kept() {
        let dir = temp("legacy");
        let file = dir.join("settings.json");
        let default = dir.join("default cache");
        let chosen = dir.join("chosen cache");
        // A Unix /Volumes path is rooted but not absolute on Windows. Use
        // this platform's absolute temp path and let serde escape backslashes.
        assert!(chosen.is_absolute());
        assert_eq!(legacy_chunk_dir(&file, &default), default.join("chunks"));
        std::fs::write(
            &file,
            serde_json::to_vec(&serde_json::json!({
                "chunk_cache": { "location": format!(" {} ", chosen.display()), "size_limit_gb": 20 }
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(legacy_chunk_dir(&file, &default), chosen.join("chunks"));
        std::fs::write(&file, r#"{ "chunk_cache": { "location": "relative" } }"#).unwrap();
        assert_eq!(legacy_chunk_dir(&file, &default), default.join("chunks"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
