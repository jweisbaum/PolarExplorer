//! Reading and writing `.wpsproj` project files (spec.md 4.3).
//!
//! The container rules are VectorEffects' `.veproj` ones: a deflate ZIP of
//! canonical JSON with a plain-text version marker, fixed entry timestamps
//! and an atomic save. JSON over a binary format for diffability and
//! recoverability.
//!
//! The archive holds exactly:
//!
//! - `META-INF/version`: the schema version in plain text, so a newer file is
//!   refused before its document is parsed;
//! - `project.json`: the document, pretty-printed, everything except bulk
//!   track data;
//! - `tracks/<track id>.json`: one per track, its fixes and its samples by
//!   column (schema 2; schema 1 wrote one object per sample, read to
//!   migrate).
//!
//! **Nothing else, ever.** No rendered images, no blend results (invariant 2).
//! The writer cannot produce another entry, and the reader refuses an archive
//! that has one rather than silently dropping it on the next save.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::project::{Project, SCHEMA_VERSION};
use crate::track::{TrackBulk, TrackBulkV1};

/// The project document inside the archive.
pub const PROJECT_ENTRY: &str = "project.json";
/// A plain-text schema version, readable without parsing the document.
pub const VERSION_ENTRY: &str = "META-INF/version";
/// The directory of per-track bulk entries.
pub const TRACKS_PREFIX: &str = "tracks/";
/// The file extension, without a dot.
pub const EXTENSION: &str = "wpsproj";

/// A function that upgrades a document from one schema version to the next.
pub type Migration = fn(&mut Value) -> Result<()>;

/// The migration chain, keyed by the version each step upgrades *from*
/// (spec.md 4.3).
pub const MIGRATIONS: &[(u32, Migration)] = &[
    (1, samples_by_column),
    (2, analysis_overlays),
    (3, orr_certificate_details),
    (4, supplied_wind_and_changes),
    (5, boat_tabs),
    (6, weights_to_one),
    (7, filters_consolidated),
];

/// 7 → 8 (asked 2026-10-02): the sample filters a track, the global filters
/// and each priority group hold lose the tack/gybe window, the stop speed
/// and window, the timestamp interval and the given-or-derived origin
/// filters, which the interface no longer offers (the new bounds — heading,
/// course, VMG, wind direction, "remove tacks and gybes" — default to off);
/// a track's one given-or-derived preference becomes one for its heading
/// and one for its speed, both what it was; and Stokes drift, whose option
/// is gone, is off. Boat tabs are migrated with the document.
fn filters_consolidated(value: &mut Value) -> Result<()> {
    const DROPPED: [&str; 6] = [
        "heading_origin",
        "speed_origin",
        "tack_gybe_padding_s",
        "stop_speed_kn",
        "stop_padding_s",
        "utc_interval_s",
    ];
    fn strip(filters: &mut Value) {
        if let Some(map) = filters.as_object_mut() {
            for key in DROPPED {
                map.remove(key);
            }
        }
    }
    fn document(document: &mut Value) {
        if let Some(sources) = document.get_mut("sources").and_then(Value::as_array_mut) {
            for source in sources {
                if let Some(filters) = source.pointer_mut("/overlay/filters") {
                    strip(filters);
                }
                if let Some(derivation) = source.pointer_mut("/kind/track/derivation")
                    && let Some(map) = derivation.as_object_mut()
                {
                    let prefer = map.remove("prefer").unwrap_or_else(|| Value::from("given"));
                    map.insert("prefer_heading".to_owned(), prefer.clone());
                    map.insert("prefer_speed".to_owned(), prefer);
                }
            }
        }
        if let Some(blend) = document.get_mut("blend").and_then(Value::as_object_mut) {
            if let Some(filters) = blend.get_mut("global_filters") {
                strip(filters);
            }
            if let Some(groups) = blend
                .get_mut("priority_groups")
                .and_then(Value::as_array_mut)
            {
                for filters in groups {
                    strip(filters);
                }
            }
            if blend.contains_key("include_stokes_drift") {
                blend.insert("include_stokes_drift".to_owned(), Value::from(false));
            }
        }
    }
    document(value);
    if let Some(tabs) = value.get_mut("boat_tabs").and_then(Value::as_array_mut) {
        for tab in tabs {
            document(tab);
            tab["schema_version"] = Value::from(8);
        }
    }
    Ok(())
}

/// 6 → 7: a source's weight runs from 0 to 1 (spec.md 8), where it ran to 2.
/// A weight above 1 becomes 1, in the project and in each boat tab (settled
/// with the user 2026-10-01: clamped, not rescaled, so a blend that leaned
/// on a weight above 1 changes). Boat tabs carry their own version, which
/// moves with the document's.
fn weights_to_one(value: &mut Value) -> Result<()> {
    fn clamp(document: &mut Value) {
        let Some(sources) = document.get_mut("sources").and_then(Value::as_array_mut) else {
            return;
        };
        for source in sources {
            if source
                .get("weight")
                .and_then(Value::as_f64)
                .is_some_and(|weight| weight > crate::source::MAX_WEIGHT)
            {
                source["weight"] = Value::from(crate::source::MAX_WEIGHT);
            }
        }
    }
    clamp(value);
    if let Some(tabs) = value.get_mut("boat_tabs").and_then(Value::as_array_mut) {
        for tab in tabs {
            clamp(tab);
            tab["schema_version"] = Value::from(7);
        }
    }
    Ok(())
}

/// 5 → 6: an existing project is the first boat tab; extra tabs default empty.
fn boat_tabs(_value: &mut Value) -> Result<()> {
    Ok(())
}

/// 1 → 2 (M14e, D27): track samples are stored by column, without what is
/// derived, and their environment at its stored precision. That lives in
/// the `tracks/` entries, which [`from_bytes`] reads by the file's version;
/// `project.json` itself is unchanged.
fn samples_by_column(_value: &mut Value) -> Result<()> {
    Ok(())
}

/// 2 → 3: optional analysis overlays default to disabled so older projects
/// retain their result. Bulk data gains only optional columns.
fn analysis_overlays(_value: &mut Value) -> Result<()> {
    Ok(())
}

/// 3 → 4: complete ORR metadata is optional. Existing imported records stay
/// immutable; absent details mean the older import did not capture them.
fn orr_certificate_details(_value: &mut Value) -> Result<()> {
    Ok(())
}

/// 4 → 5: optional supplied wind and change thresholds preserve old results.
fn supplied_wind_and_changes(_value: &mut Value) -> Result<()> {
    Ok(())
}

/// The archive entry holding one track's bulk data.
pub fn track_entry(id: crate::id::TrackId) -> String {
    format!("{TRACKS_PREFIX}{}.json", id.raw())
}

/// Serialises a project document to canonical JSON.
///
/// Canonical means byte-identical for equal documents: struct fields
/// serialise in declaration order, no map in the model is a `HashMap`, and
/// every float goes through [`crate::canonical`].
pub fn to_canonical_json(project: &Project) -> Result<String> {
    Ok(serde_json::to_string_pretty(project)?)
}

/// Parses, migrates and validates a project document, without bulk data.
pub fn from_json(json: &str) -> Result<Project> {
    let mut value: Value = serde_json::from_str(json)?;
    migrate(&mut value)?;
    let mut project: Project = serde_json::from_value(value)?;
    project.schema_version = SCHEMA_VERSION;
    Ok(project)
}

fn migrate(value: &mut Value) -> Result<()> {
    run_migrations(value, MIGRATIONS, SCHEMA_VERSION).map(|_| ())
}

/// Runs a migration chain up to `target`. Split out so tests can drive a
/// synthetic chain.
fn run_migrations(value: &mut Value, migrations: &[(u32, Migration)], target: u32) -> Result<u32> {
    let mut version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| CoreError::Migration {
            from: 0,
            reason: "the document has no schema_version".to_owned(),
        })?;

    if version > target {
        return Err(CoreError::SchemaTooNew {
            found: version,
            supported: target,
        });
    }
    while version < target {
        let Some((_, step)) = migrations.iter().find(|(from, _)| *from == version) else {
            return Err(CoreError::Migration {
                from: version,
                reason: format!("no migration from version {version}"),
            });
        };
        step(value)?;
        version += 1;
        value["schema_version"] = Value::from(version);
    }
    Ok(version)
}

/// Writes a project to `path`, atomically.
///
/// The archive is written to `<name>.wpsproj.tmp` beside it and renamed into
/// place, so an interrupted save cannot leave a truncated file where a valid
/// project used to be.
pub fn save(project: &Project, path: &Path) -> Result<()> {
    write_atomic(path, &to_bytes(project)?)
}

/// Writes `bytes` to `path` atomically: to `<name>.tmp` beside it, synced,
/// then renamed into place. A crash mid-write leaves the old file intact.
/// Used for project files, the settings file and autosave manifests.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = temp_path_for(path);
    let written = std::fs::File::create(&temp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    if let Err(err) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(err.into());
    }
    // Rename is atomic within a filesystem; if it fails, the original file is
    // still intact and only the temporary is left behind, then removed.
    if let Err(err) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(err.into());
    }
    Ok(())
}

/// [`write_atomic`] for a file that holds a secret: on Unix the temporary is
/// created owner-only (0600) before a byte is written, so the secret is never
/// readable by anyone else, not in the temporary and not after the rename
/// (which keeps the temporary's mode). A temporary left by an earlier crash is
/// removed first, since opening it would keep its old mode.
pub fn write_atomic_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = temp_path_for(path);
    match std::fs::remove_file(&temp) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err.into()),
        _ => {}
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options.open(&temp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    if let Err(err) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(err.into());
    }
    if let Err(err) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(err.into());
    }
    Ok(())
}

fn temp_path_for(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// The archive bytes for a project. Everything is validated and serialised
/// before a single byte reaches the disk.
pub fn to_bytes(project: &Project) -> Result<Vec<u8>> {
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    project.validate()?;
    let json = to_canonical_json(project)?;
    // Validation above saw the in-memory values; the file holds them rounded
    // (`canonical`). Two axis values closer than the canonical precision are
    // distinct in memory and equal on disk, and that file would never open
    // again. So the document is also validated as it will be read back, and
    // an unsaveable one is refused here, before anything is written.
    from_json(&json)?.validate_document()?;

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    // A fixed timestamp keeps the archive byte-reproducible: with the clock
    // baked in, two saves of an unchanged project would differ.
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);

    zip.start_file(VERSION_ENTRY, options).map_err(zip_err)?;
    zip.write_all(SCHEMA_VERSION.to_string().as_bytes())?;
    zip.start_file(PROJECT_ENTRY, options).map_err(zip_err)?;
    zip.write_all(json.as_bytes())?;

    // In source order: the document's order, not a hash map's.
    for (boat, prefix) in std::iter::once((project, String::new())).chain(
        project
            .boat_tabs
            .iter()
            .map(|boat| (boat, format!("boats/{}/", boat.id.raw()))),
    ) {
        for source in &boat.sources {
            if let Some(track) = source.track() {
                zip.start_file(format!("{prefix}{}", track_entry(track.id)), options)
                    .map_err(zip_err)?;
                let bulk = serde_json::to_vec(&track.bulk()?)?;
                zip.write_all(&bulk)?;
            }
        }
    }
    Ok(zip.finish().map_err(zip_err)?.into_inner())
}

/// Reads a project from `path`.
pub fn load(path: &Path) -> Result<Project> {
    from_bytes(&std::fs::read(path)?)
}

/// Reads a project from archive bytes.
pub fn from_bytes(bytes: &[u8]) -> Result<Project> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(zip_err)?;

    // The version first: a newer file is refused with both versions named,
    // before its document is parsed at all.
    let version = read_version(&mut archive)?;
    if version > SCHEMA_VERSION {
        return Err(CoreError::SchemaTooNew {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }

    let json = read_entry(&mut archive, PROJECT_ENTRY)?;
    let mut project = from_json(&json)?;

    let mut expected: BTreeSet<String> = [VERSION_ENTRY, PROJECT_ENTRY]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let mut read_tracks = |boat: &mut Project, prefix: &str| -> Result<()> {
        for source in &mut boat.sources {
            let Some(track) = source.track_mut() else {
                continue;
            };
            let name = format!("{prefix}{}", track_entry(track.id));
            let text = read_entry(&mut archive, &name)?;
            let in_entry = |err: CoreError| CoreError::Archive(format!("{name}: {err}"));
            let (fixes, samples) = if version < 2 {
                serde_json::from_str::<TrackBulkV1>(&text)?
                    .into_parts()
                    .map_err(in_entry)?
            } else {
                let bulk: TrackBulk = serde_json::from_str(&text)?;
                let samples = bulk.samples.into_samples(&bulk.fixes).map_err(in_entry)?;
                (bulk.fixes, samples)
            };
            track.fixes = fixes;
            track.samples = samples;
            for sample in &mut track.samples {
                sample.supplied_wind = track
                    .fixes
                    .get(sample.fix as usize)
                    .and_then(|f| f.supplied_wind());
                sample.downloaded_wind_only = track.derivation.downloaded_wind_only;
                sample.relate();
            }
            expected.insert(name);
        }
        Ok(())
    };
    read_tracks(&mut project, "")?;
    for boat in &mut project.boat_tabs {
        read_tracks(boat, &format!("boats/{}/", boat.id.raw()))?;
    }
    if let Some(unexpected) = archive
        .file_names()
        .find(|name| !expected.contains(*name) && !is_os_artefact(name))
    {
        return Err(CoreError::Archive(format!(
            "it holds {unexpected:?}, which is not part of a project"
        )));
    }

    project.validate()?;
    Ok(project)
}

/// Whether an archive entry is litter an operating system adds when a user
/// re-zips or browses the file: macOS `__MACOSX/` resource forks and
/// `.DS_Store`, Windows `Thumbs.db`. Ignored on load (and never written back)
/// rather than refused, because they carry nothing of the project.
fn is_os_artefact(name: &str) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name);
    name.starts_with("__MACOSX/") || file == ".DS_Store" || file == "Thumbs.db"
}

/// Reads only the schema version, without parsing the document.
pub fn peek_version(path: &Path) -> Result<u32> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(zip_err)?;
    read_version(&mut archive)
}

fn read_version<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Result<u32> {
    let text = read_entry(archive, VERSION_ENTRY)?;
    text.trim()
        .parse()
        .map_err(|_| CoreError::Archive(format!("its version marker {text:?} is not a number")))
}

fn read_entry<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<String> {
    let mut text = String::new();
    archive
        .by_name(name)
        .map_err(|_| CoreError::Archive(format!("it has no {name}")))?
        .read_to_string(&mut text)?;
    Ok(text)
}

fn zip_err(err: zip::result::ZipError) -> CoreError {
    CoreError::Archive(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use proptest::prelude::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "pe-io-{}-{label}-{}",
                std::process::id(),
                crate::id::ProjectId::fresh().raw()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A secret is owner-only from its first byte: a stale world-readable
    /// temporary and a world-readable earlier file both end up 0600.
    #[cfg(unix)]
    #[test]
    fn a_private_write_is_owner_only_whatever_was_there() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("private");
        let file = dir.path("secret.json");
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        std::fs::write(&file, b"old").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let stale = temp_path_for(&file);
        std::fs::write(&stale, b"left by a crash").unwrap();
        std::fs::set_permissions(&stale, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_atomic_private(&file, b"new").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"new");
        assert_eq!(mode(&file), 0o600);
        assert!(!stale.exists());
    }

    fn entry_names(bytes: &[u8]) -> Vec<String> {
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        archive.file_names().map(str::to_owned).collect()
    }

    /// Rebuilds an archive with its entries changed by `edit`.
    fn rewrite(bytes: &[u8], edit: impl Fn(&mut Vec<(String, Vec<u8>)>)) -> Vec<u8> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut entries = Vec::new();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            entries.push((file.name().to_owned(), data));
        }
        edit(&mut entries);
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in entries {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn a_project_round_trips_through_a_file() {
        let dir = TempDir::new("round-trip");
        let path = dir.path("a.wpsproj");
        let project = fixtures::project();
        save(&project, &path).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded, project);
        assert_eq!(loaded.sources[2].track().unwrap().samples.len(), 3);
    }

    /// The M1 acceptance criterion, for any generated project: save, load,
    /// save again, and the bytes are identical.
    #[test]
    fn save_load_save_is_byte_identical_for_the_fixture() {
        let first = to_bytes(&fixtures::project()).unwrap();
        let second = to_bytes(&from_bytes(&first).unwrap()).unwrap();
        assert_eq!(first, second);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn any_project_survives_save_load_save_byte_identically(project in fixtures::arb_project()) {
            let first = to_bytes(&project).unwrap();
            let loaded = from_bytes(&first).unwrap();
            let second = to_bytes(&loaded).unwrap();
            prop_assert_eq!(&first, &second);
            // And the loaded document is a fixed point.
            prop_assert_eq!(&from_bytes(&second).unwrap(), &loaded);
        }
    }

    #[test]
    fn saving_twice_gives_identical_files() {
        let dir = TempDir::new("twice");
        let (a, b) = (dir.path("a.wpsproj"), dir.path("b.wpsproj"));
        let project = fixtures::project();
        save(&project, &a).unwrap();
        save(&project, &b).unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    }

    /// Spec 4.3: exactly the version marker, the document and one entry per
    /// track. A rendered image or a blend result would show up here.
    #[test]
    fn the_archive_holds_no_unexpected_entries() {
        let project = fixtures::project();
        let track = project.sources[2].track().unwrap().id;
        let names = entry_names(&to_bytes(&project).unwrap());
        assert_eq!(
            names,
            vec![
                VERSION_ENTRY.to_owned(),
                PROJECT_ENTRY.to_owned(),
                track_entry(track)
            ]
        );
    }

    #[test]
    fn an_archive_with_an_unexpected_entry_is_refused() {
        let bytes = to_bytes(&fixtures::project()).unwrap();
        let tampered = rewrite(&bytes, |entries| {
            entries.push(("blend.png".to_owned(), vec![0x89, b'P', b'N', b'G']));
        });
        let err = from_bytes(&tampered).unwrap_err();
        assert!(
            matches!(&err, CoreError::Archive(m) if m.contains("blend.png")),
            "{err}"
        );

        // A bulk entry for a track the document does not have is also foreign.
        let stray = rewrite(&bytes, |entries| {
            entries.push(("tracks/999.json".to_owned(), b"{}".to_vec()));
        });
        assert!(matches!(from_bytes(&stray), Err(CoreError::Archive(_))));
    }

    #[test]
    fn operating_system_artefacts_are_ignored_on_load() {
        let project = fixtures::project();
        let bytes = to_bytes(&project).unwrap();
        let littered = rewrite(&bytes, |entries| {
            for name in [
                "__MACOSX/._project.json",
                "__MACOSX/tracks/._3.json",
                ".DS_Store",
                "tracks/.DS_Store",
                "Thumbs.db",
            ] {
                entries.push((name.to_owned(), b"junk".to_vec()));
            }
        });
        let loaded = from_bytes(&littered).unwrap();
        assert_eq!(loaded, project);
        // And they are not carried into the next save.
        assert_eq!(to_bytes(&loaded).unwrap(), bytes);

        // Anything merely resembling one is still foreign.
        let lookalike = rewrite(&bytes, |entries| {
            entries.push(("MACOSX/x".to_owned(), vec![]));
        });
        assert!(matches!(from_bytes(&lookalike), Err(CoreError::Archive(_))));
    }

    /// Controller ruling on M1: values distinct in memory but equal once
    /// rounded would write a file that can never open. Refused at save time.
    #[test]
    fn a_document_that_rounds_into_an_invalid_one_is_not_written() {
        let dir = TempDir::new("rounds-invalid");
        let path = dir.path("p.wpsproj");

        let mut polar_axis = fixtures::project();
        if let crate::source::SourceKind::PolarFile { polar, .. } = &mut polar_axis.sources[1].kind
        {
            polar.twa[1] = polar.twa[2] - 1e-12;
        }
        polar_axis.validate().unwrap();
        let err = save(&polar_axis, &path).unwrap_err();
        assert!(err.to_string().contains("strictly increasing"), "{err}");

        let mut grid = fixtures::project();
        grid.grid.tws[1] = grid.grid.tws[0] + 1e-9;
        grid.validate().unwrap();
        assert!(save(&grid, &path).is_err());

        assert!(!path.exists(), "nothing was written");
        assert!(!dir.path("p.wpsproj.tmp").exists());
    }

    #[test]
    fn write_atomic_replaces_whole_files_and_leaves_no_temporary() {
        let dir = TempDir::new("write-atomic");
        let path = dir.path("settings.json");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert!(!dir.path("settings.json.tmp").exists());
        assert!(write_atomic(&dir.path("missing/dir/x"), b"x").is_err());
    }

    #[test]
    fn a_missing_track_entry_is_refused() {
        let bytes = to_bytes(&fixtures::project()).unwrap();
        let cut = rewrite(&bytes, |entries| {
            entries.retain(|(name, _)| !name.starts_with(TRACKS_PREFIX));
        });
        let err = from_bytes(&cut).unwrap_err();
        assert!(err.to_string().contains("tracks/"), "{err}");
    }

    #[test]
    fn project_json_holds_no_bulk_track_data() {
        let json = to_canonical_json(&fixtures::project()).unwrap();
        assert!(!json.contains("\"fixes\""), "{json}");
        assert!(!json.contains("\"samples\""));
        assert!(json.contains("\"env_meta\""));
    }

    /// Acceptance: a newer file is refused with a message naming both
    /// versions, and before the document is parsed (here, it is not even
    /// JSON).
    #[test]
    fn a_newer_schema_is_refused_naming_both_versions() {
        let bytes = to_bytes(&fixtures::project()).unwrap();
        let newer = SCHEMA_VERSION + 1;
        let future = rewrite(&bytes, |entries| {
            for (name, data) in entries.iter_mut() {
                if name == VERSION_ENTRY {
                    *data = newer.to_string().into_bytes();
                }
                if name == PROJECT_ENTRY {
                    *data = b"{ something from the future".to_vec();
                }
            }
        });
        let err = from_bytes(&future).unwrap_err();
        assert!(
            matches!(err, CoreError::SchemaTooNew { found, supported } if found == newer && supported == SCHEMA_VERSION)
        );
        let message = err.to_string();
        assert!(message.contains(&format!("version {newer}")), "{message}");
        assert!(
            message.contains(&format!("up to version {SCHEMA_VERSION}")),
            "{message}"
        );
    }

    /// The document's own version is checked too, for a marker that lies.
    #[test]
    fn a_newer_document_behind_an_old_marker_is_refused() {
        let mut value = serde_json::to_value(fixtures::project()).unwrap();
        value["schema_version"] = Value::from(SCHEMA_VERSION + 5);
        let err = from_json(&value.to_string()).unwrap_err();
        assert!(matches!(err, CoreError::SchemaTooNew { .. }));
    }

    #[test]
    fn the_version_marker_can_be_read_without_parsing() {
        let dir = TempDir::new("peek");
        let path = dir.path("p.wpsproj");
        save(&fixtures::project(), &path).unwrap();
        assert_eq!(peek_version(&path).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn a_document_without_a_version_is_refused() {
        let mut value = serde_json::to_value(fixtures::project()).unwrap();
        value.as_object_mut().unwrap().remove("schema_version");
        assert!(matches!(
            from_json(&value.to_string()),
            Err(CoreError::Migration { from: 0, .. })
        ));
    }

    #[test]
    fn the_migration_runner_walks_the_chain() {
        fn one_to_two(value: &mut Value) -> Result<()> {
            value["added_in_2"] = Value::from(true);
            Ok(())
        }
        fn two_to_three(value: &mut Value) -> Result<()> {
            let had = value["added_in_2"].as_bool().unwrap_or(false);
            value["seen_2"] = Value::from(had);
            Ok(())
        }
        let chain: &[(u32, Migration)] = &[(2, two_to_three), (1, one_to_two)];
        let mut value = serde_json::json!({ "schema_version": 1 });
        assert_eq!(run_migrations(&mut value, chain, 3).unwrap(), 3);
        assert_eq!(value["schema_version"], 3);
        assert_eq!(value["seen_2"], true);

        let mut gap = serde_json::json!({ "schema_version": 1 });
        assert!(matches!(
            run_migrations(&mut gap, &[(2, two_to_three)], 3),
            Err(CoreError::Migration { from: 1, .. })
        ));
    }

    #[test]
    fn the_shipped_chain_reaches_the_current_version_from_every_older_one() {
        for from in 1..SCHEMA_VERSION {
            assert!(
                MIGRATIONS.iter().any(|(v, _)| *v == from),
                "no migration from {from}"
            );
        }
    }

    /// 6 → 7: weights run from 0 to 1. A weight an older version allowed
    /// above 1 loads as 1, in the project and in every boat tab; weights
    /// already in range are untouched.
    #[test]
    fn weights_above_one_load_clamped_to_one() {
        let mut tab = fixtures::project();
        tab.id = crate::id::ProjectId(tab.id.0 + 1);
        let mut project = fixtures::project();
        project.boat_tabs = vec![tab];
        let mut value = serde_json::to_value(&project).unwrap();
        value["schema_version"] = Value::from(6);
        value["sources"][0]["weight"] = Value::from(1.5);
        value["sources"][1]["weight"] = Value::from(0.4);
        value["boat_tabs"][0]["schema_version"] = Value::from(6);
        value["boat_tabs"][0]["sources"][0]["weight"] = Value::from(2.0);
        value["boat_tabs"][0]["sources"][1]["weight"] = Value::from(0.25);

        let loaded = from_json(&value.to_string()).unwrap();
        assert_eq!(loaded.schema_version, 8);
        assert_eq!(loaded.sources[0].weight, 1.0);
        assert_eq!(loaded.sources[1].weight, 0.4);
        assert_eq!(loaded.boat_tabs[0].schema_version, 8);
        assert_eq!(loaded.boat_tabs[0].sources[0].weight, 1.0);
        assert_eq!(loaded.boat_tabs[0].sources[1].weight, 0.25);
        loaded.validate_document().unwrap();
    }

    /// 7 → 8: the filters the interface no longer offers are dropped on
    /// load, a track's one preference becomes two, and Stokes drift is off.
    #[test]
    fn dropped_filters_leave_the_preference_splits_and_stokes_drift_is_off() {
        use crate::track::PreferValues;
        let mut project = fixtures::project();
        let tab = {
            let mut tab = fixtures::project();
            tab.id = crate::id::ProjectId(tab.id.0 + 1);
            tab
        };
        project.boat_tabs = vec![tab];
        let mut value = serde_json::to_value(&project).unwrap();
        value["schema_version"] = Value::from(7);
        value["boat_tabs"][0]["schema_version"] = Value::from(7);
        let track = value["sources"]
            .as_array()
            .unwrap()
            .iter()
            .position(|s| s["kind"]["type"] == "track")
            .unwrap();
        fn age(document: &mut Value, track: usize) {
            let filters = &mut document["sources"][track]["overlay"]["filters"];
            filters["tack_gybe_padding_s"] = Value::from(60);
            filters["stop_speed_kn"] = Value::from(0.5);
            filters["stop_padding_s"] = Value::from(120);
            filters["utc_interval_s"] = Value::from(60);
            filters["heading_origin"] = Value::from("given_only");
            filters["speed_origin"] = Value::from("derived_only");
            let derivation = &mut document["sources"][track]["kind"]["track"]["derivation"];
            derivation.as_object_mut().unwrap().remove("prefer_heading");
            derivation.as_object_mut().unwrap().remove("prefer_speed");
            derivation["prefer"] = Value::from("derived");
            document["blend"]["include_stokes_drift"] = Value::from(true);
            document["blend"]["global_filters"] = serde_json::json!({ "utc_interval_s": 30 });
        }
        age(&mut value["boat_tabs"][0], track);
        age(&mut value, track);

        let loaded = from_json(&value.to_string()).unwrap();
        for document in [&loaded, &loaded.boat_tabs[0]] {
            assert_eq!(document.schema_version, 8);
            let source = &document.sources[track];
            // The dropped keys are gone; what the fixture set is kept.
            assert_eq!(
                source.overlay.filters,
                project.sources[track].overlay.filters
            );
            let derivation = &source.track().unwrap().derivation;
            assert_eq!(derivation.prefer_heading, PreferValues::Derived);
            assert_eq!(derivation.prefer_speed, PreferValues::Derived);
            assert!(!document.blend.include_stokes_drift);
            assert_eq!(
                document.blend.global_filters,
                Some(crate::source::SampleFilters::default())
            );
        }
        loaded.validate_document().unwrap();
    }

    #[test]
    fn a_file_that_is_not_an_archive_is_refused() {
        assert!(matches!(
            from_bytes(b"not a project"),
            Err(CoreError::Archive(_))
        ));
    }

    #[test]
    fn saving_leaves_no_temporary_file_and_replaces_the_old_one() {
        let dir = TempDir::new("atomic");
        let path = dir.path("p.wpsproj");
        std::fs::write(&path, b"old").unwrap();
        save(&fixtures::project(), &path).unwrap();
        assert!(!dir.path("p.wpsproj.tmp").exists());
        assert!(load(&path).is_ok());
    }

    /// An invalid document is refused before anything touches the disk, so
    /// the file that was there survives.
    #[test]
    fn an_invalid_project_is_not_written() {
        let dir = TempDir::new("invalid");
        let path = dir.path("p.wpsproj");
        save(&fixtures::project(), &path).unwrap();
        let before = std::fs::read(&path).unwrap();

        let mut bad = fixtures::project();
        bad.sources[0].weight = f64::NAN;
        assert!(save(&bad, &path).is_err());
        let mut bad = fixtures::project();
        if let Some(track) = bad.sources[2].track_mut() {
            track.samples[0].tws = Some(f64::INFINITY);
        }
        assert!(save(&bad, &path).is_err());

        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!dir.path("p.wpsproj.tmp").exists());
    }

    /// A project whose one track has `n` fixes a boat 10 minutes apart and
    /// a sample for each with its motion and the environment as a fetch
    /// leaves it: slowly varying wind, waves and current, as interpolated
    /// reanalysis is. A random walk, so nothing repeats.
    fn fetched_track_project(n: usize) -> Project {
        use crate::track::{
            DatasetRecord, EnvStatus, Fix, Sample, Track, TrackOrigin, ValueOrigin,
        };
        let mut p = Project::new(
            "Sizes".to_owned(),
            crate::project::Boat {
                name: "B".to_owned(),
                notes: String::new(),
                ..crate::project::Boat::default()
            },
            1_700_000_000,
        );
        let id = p.allocate_source_id();
        let track_id = p.allocate_track_id();
        let mut track = Track::new(
            track_id,
            TrackOrigin::File {
                name: "race.csv".to_owned(),
                boat_name: None,
            },
        );
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state as f64 / u64::MAX as f64) * 2.0 - 1.0
        };
        let (mut lat, mut lon, mut tws, mut twd, mut hs, mut wd, mut cs, mut ct) =
            (50.1, -5.2, 12.0, 240.0, 1.5, 250.0, 0.8, 90.0);
        let (mut dtws, mut dtwd) = (0.0, 0.0);
        for i in 0..n {
            lat += 0.01 * next();
            lon += 0.01 * next();
            let fix = Fix {
                tws: None,
                twd_from: None,
                t: 1_753_000_000 + 600 * i as i64,
                lat: crate::canonical::degrees(lat),
                lon: crate::canonical::degrees(lon),
                cog: None,
                sog: None,
            };
            let mut s = Sample::at(p.allocate_sample_id(), i as u32, &fix);
            s.heading = Some(crate::canonical::degrees(
                (200.0 + 40.0 * next()).rem_euclid(360.0),
            ));
            s.heading_origin = Some(ValueOrigin::Derived);
            s.speed = Some(crate::canonical::knots(7.0 + next()));
            s.speed_origin = Some(ValueOrigin::Derived);
            dtws = 0.9 * dtws + 0.08 * next();
            dtwd = 0.9 * dtwd + 0.3 * next();
            tws = f64::max(tws + dtws, 0.0);
            twd = (twd + dtwd).rem_euclid(360.0);
            hs = f64::max(hs + 0.005 * next(), 0.0);
            wd = (wd + 0.2 * next()).rem_euclid(360.0);
            cs = f64::abs(cs + 0.01 * next());
            ct = (ct + 1.5 * next()).rem_euclid(360.0);
            (s.tws, s.twd_from, s.hs_m, s.wave_from) = (Some(tws), Some(twd), Some(hs), Some(wd));
            (s.current_speed, s.current_toward) = (Some(cs), Some(ct));
            (s.wind_dataset, s.wave_dataset, s.current_dataset) = (Some(0), Some(1), Some(2));
            s.env_fetched = true;
            s.quantise_env();
            s.relate();
            track.fixes.push(fix);
            track.samples.push(s);
        }
        for name in ["arco-era5", "arco-era5-waves", "cmems-nws-my-uv-geo"] {
            track.env_meta.datasets.push(DatasetRecord {
                name: name.to_owned(),
                version: "v".to_owned(),
                fetched_at: 1_760_000_000,
                has_tide: None,
            });
        }
        track.env_meta.status = EnvStatus::Ready;
        p.sources.push(crate::Source::new(
            id,
            "Boat".to_owned(),
            crate::Colour::parse("#123456").unwrap(),
            crate::SourceKind::Track {
                track: Box::new(track),
            },
        ));
        p
    }

    /// The compressed size of `name` in an archive.
    fn stored_size(bytes: &[u8], name: &str) -> u64 {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        archive.by_name(name).unwrap().compressed_size()
    }

    /// Schema 1's bulk entry for `track`: one object per sample, every
    /// field written.
    fn v1_entry(track: &crate::track::Track) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "fixes": track.fixes,
            "samples": track.samples,
        }))
        .unwrap()
    }

    /// The same project as schema 1 wrote it.
    fn as_v1(project: &Project) -> Vec<u8> {
        let bytes = to_bytes(project).unwrap();
        rewrite(&bytes, |entries| {
            for (name, data) in entries.iter_mut() {
                if name == VERSION_ENTRY {
                    *data = b"1".to_vec();
                } else if name == PROJECT_ENTRY {
                    let mut doc: Value = serde_json::from_slice(data).unwrap();
                    doc["schema_version"] = Value::from(1);
                    *data = serde_json::to_vec_pretty(&doc).unwrap();
                } else if let Some(track) = project
                    .sources
                    .iter()
                    .filter_map(|s| s.track())
                    .find(|t| *name == track_entry(t.id))
                {
                    *data = v1_entry(track);
                }
            }
        })
    }

    /// D27: a track's samples cost a few kilobytes per thousand in the
    /// file. Measured here on 1,000 fetched samples, before (schema 1, one
    /// object per sample) and after (schema 2, by column); printed with
    /// `--nocapture` for plan.md.
    #[test]
    fn a_thousand_fetched_samples_are_a_few_kilobytes() {
        let project = fetched_track_project(1000);
        let track = project.sources[0].track().unwrap();
        let entry = track_entry(track.id);
        let after = to_bytes(&project).unwrap();
        let before = as_v1(&project);
        let (after_z, before_z) = (stored_size(&after, &entry), stored_size(&before, &entry));
        // The fixes alone, as both schemas write them.
        let fixes_only = {
            let mut bare = project.clone();
            if let Some(t) = bare.sources[0].track_mut() {
                t.samples.clear();
            }
            stored_size(&to_bytes(&bare).unwrap(), &entry)
        };
        // Without the environment (never fetched): what the weather adds.
        let no_env = {
            let mut bare = project.clone();
            if let Some(t) = bare.sources[0].track_mut() {
                for s in &mut t.samples {
                    s.clear_env();
                }
            }
            stored_size(&to_bytes(&bare).unwrap(), &entry)
        };
        println!(
            "D27 | 1,000 samples, tracks/<id>.json deflated: schema 1 {before_z} B, schema 2 {after_z} B; \
             fixes alone {fixes_only} B; schema 2 without the environment {no_env} B, so the \
             environment adds {} B",
            after_z - no_env
        );
        assert!(after_z * 3 < before_z, "{after_z} vs {before_z}");
        assert!(
            after_z - no_env < 12_000,
            "the weather is {} B",
            after_z - no_env
        );
        // And it reads back to the same samples.
        assert_eq!(from_bytes(&after).unwrap(), project);
    }

    /// Schema 1 files open: their samples are placed on their fixes, the
    /// environment rounded to its stored precision and every derived value
    /// recomputed; the next save is schema 2 and a fixed point.
    #[test]
    fn a_schema_1_project_migrates_its_samples() {
        let project = fetched_track_project(50);
        let mut unrounded = project.clone();
        if let Some(t) = unrounded.sources[0].track_mut() {
            for s in &mut t.samples {
                s.tws = s.tws.map(|v| v + 0.001_234);
                s.twa = Some(1.0); // stale derived values, as a v1 file may hold
            }
        }
        let v1 = as_v1(&unrounded);
        let loaded = from_bytes(&v1).unwrap();
        assert_eq!(loaded.schema_version, SCHEMA_VERSION);
        assert_eq!(loaded, project, "rounded back and related again");
        let saved = to_bytes(&loaded).unwrap();
        assert_eq!(saved, to_bytes(&project).unwrap());
        assert_eq!(from_bytes(&saved).unwrap(), loaded);
    }

    /// Damaged columns are refused naming the entry and what is wrong,
    /// never a panic or a silently shorter track.
    #[test]
    fn damaged_sample_columns_are_refused_by_name() {
        let project = fetched_track_project(5);
        let track = project.sources[0].track().unwrap();
        let entry = track_entry(track.id);
        let bytes = to_bytes(&project).unwrap();
        let spoil = |edit: &dyn Fn(&mut Value)| {
            rewrite(&bytes, |entries| {
                for (name, data) in entries.iter_mut() {
                    if *name == entry {
                        let mut doc: Value = serde_json::from_slice(data).unwrap();
                        edit(&mut doc["samples"]);
                        *data = serde_json::to_vec(&doc).unwrap();
                    }
                }
            })
        };
        for (edit, says) in [
            (
                (|v: &mut Value| v["tws"] = serde_json::json!([1.0])) as fn(&mut Value),
                "tws column has 1 values for 5 samples",
            ),
            (
                |v| v["heading_origin"] = Value::from("gdxdg"),
                "not a value origin",
            ),
            (
                |v| v["fetched"] = Value::from("11112"),
                "not a fetched flag",
            ),
            (
                |v| v["fixes"] = serde_json::json!([[3, 5]]),
                "refers to fix 5 of 5",
            ),
            (
                |v| v["ids"] = serde_json::json!([[1, 999]]),
                "more than the 5 allowed",
            ),
        ] {
            let err = from_bytes(&spoil(&edit)).unwrap_err();
            let text = err.to_string();
            assert!(text.contains(&entry) && text.contains(says), "{text}");
        }
    }

    /// Ids and fixes with gaps (a sample removed, a fix without a sample)
    /// round-trip through their runs.
    #[test]
    fn gaps_in_ids_and_fixes_round_trip() {
        let mut project = fetched_track_project(12);
        if let Some(t) = project.sources[0].track_mut() {
            t.samples.remove(7);
            t.samples.remove(3);
        }
        let bytes = to_bytes(&project).unwrap();
        assert_eq!(from_bytes(&bytes).unwrap(), project);
        let track = project.sources[0].track().unwrap();
        let columns = crate::track::SampleColumns::of(&track.samples).unwrap();
        assert_eq!(columns.fixes, vec![[0, 3], [4, 3], [8, 4]]);
        assert_eq!(columns.ids.len(), 3);
    }
}
