# Polar Library Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One global polar library — the ORC and ORR catalogues plus library files the person imports or saves from a tab's blend — searched from the first left-nav section, with **Import…** feeding both library and boat and **Save to library…** on the Blend row.

**Architecture:** A new `pe-app` module `polar_library` owns a folder `polar-library/` in the config dir: `index.json` (a `BTreeMap<u64, LibraryEntry>`) and `files/<name>.<ext>`, loaded once into `AppState` and replaced atomically on every write. Search, Add, Save and Remove are `#[tauri::command]`s called by the interface and, through `app.state()`, by four MCP tools. The ORC and ORR catalogues are untouched; the interface interleaves three ranked lists. A polar file source gains an optional `library_id` (schema 9).

**Tech Stack:** Rust 2024 (`tauri` 2, `serde`, `sha2`, `ts-rs`, `rmcp`), React 18 + TypeScript + Vite, vitest, the WebDriver UX suite (`npm run ux`).

**Spec:** `docs/superpowers/specs/2026-10-18-polar-library-design.md` (read it first; section numbers below are its sections).

## Global Constraints

- Sources are immutable; a library file is one more immutable source, project sources stay copies (CLAUDE.md invariant 1). The blend is never stored as truth: Save to library is an export the person asked for (invariant 2).
- No network. No new hosts, no C dependencies; `sha2` is pure Rust.
- Determinism: `BTreeMap` for the index, every `f64` in `index.json` through `pe_core::canonical` helpers, atomic writes (`pe_core::io::write_atomic`), file first then index.
- `thiserror` in library crates; `AppError` over IPC; no `unwrap`/`expect` outside tests; `unsafe_code` forbidden.
- Every user-visible string through `t()`/`msg()` and translated into fr, de, es, it, nl, zh, ja, ar (`npm run ui:test` fails otherwise). Every interactive control carries `data-feature` and a registry entry (`features.test.ts`).
- Every new IPC command is registered in `lib.rs`'s `generate_handler!` **and** in `mcp/invoke.rs`'s `TABLE` or `EXCLUDED` (a test holds them equal) **and** in its policy effects table. A new MCP tool is named in `mcp/tools/guide.rs`.
- The §13 budget: a search keystroke under 30 ms with the catalogues plus 1,000 library entries.
- Format names are `PolarFileFormat`'s serde names: `expedition`, `adrena`, `csv`, `qtvlm`, `opencpn`, `luckgrib`, `whirlwind_matrix_polar`; extensions `txt`, `pol`, `csv`, `csv`, `pol`, `tsv`, `wmpolar`.
- Removing a library entry is permanent (not undoable). Saving under a held name asks to replace.
- Run from the repository root: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm run ui:typecheck`, `npm run ui:test`, `npm run ui:perf`, `npm run check:offline`. `CARGO_INCREMENTAL` stays 0. Editing Rust restarts the person's `tauri dev`: batch Rust edits.
- Commit after each task with the trailer `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Only `git add` the files the task touched; the working tree holds unrelated uncommitted work.

## Review Focus

1. A file name that is not a valid file name on Windows (`Fastnet: 2025 / blend?`) must still save; the entry's name keeps the characters, only the file name is sanitised. Test in Task 2.
2. Two saves with the same name but different formats: the second asks to replace, and replacing changes the file's extension and removes the old file, leaving no orphan. Test in Task 5.
3. An index entry whose file was deleted by hand: listed as unavailable, Add refused in words, Remove still works and cleans the entry. Tests in Tasks 3 and 4.
4. Import of a file whose bytes are already in the library gives the source the existing `library_id` and stores nothing twice, even across two imports in two projects. Test in Task 4.
5. The empty search (browse) must list the library newest first and paginate like the catalogues; an empty library shows a placeholder, not "No polar matches". Tests in Tasks 3 and 9.

---

### Task 1: `library_id` on polar file sources, schema 9

**Files:**
- Modify: `crates/pe-core/src/source.rs` (the `SourceKind::PolarFile` variant, about line 258)
- Modify: `crates/pe-core/src/project.rs:22` (`SCHEMA_VERSION`)
- Modify: `crates/pe-core/src/io.rs` (`MIGRATIONS`, a new migration, tests)
- Modify: `crates/pe-app/src/projects.rs` (`PolarFileSummary`, its construction near line 123)
- Modify: every constructor `SourceKind::PolarFile {` (the compiler lists them; 22 files, among them `crates/pe-app/src/polar_files.rs`, `crates/pe-app/tests/common/mod.rs`, `crates/pe-core/src/fixtures.rs`)
- Test: `crates/pe-core/src/io.rs` (unit tests module), `crates/pe-core/src/project.rs` tests

**Interfaces:**
- Produces: `SourceKind::PolarFile { format, file_name, polar, matrix, library_id: Option<u64> }`; `pe_core::SCHEMA_VERSION == 9`; `pe_app::projects::PolarFileSummary.library_id: Option<u64>`.

- [ ] **Step 1: Write the failing round-trip test**

In `crates/pe-core/src/io.rs`'s `#[cfg(test)] mod tests`, next to the existing migration tests (around line 750):

```rust
    #[test]
    fn a_library_id_survives_save_load_save_byte_for_byte() {
        let mut project = Project::new("Lib", Boat::default(), 1_700_000_000);
        let grid = crate::polar::PolarGrid {
            twa: vec![52.0, 90.0],
            tws: vec![6.0, 12.0],
            bsp: vec![vec![Some(5.9), Some(7.2)], vec![Some(7.1), Some(8.6)]],
        };
        let id = project.allocate_source_id();
        project.sources.push(crate::Source::new(
            id,
            "Farr 40",
            crate::Colour::parse("#4e79a7").unwrap(),
            crate::SourceKind::PolarFile {
                format: crate::polar::PolarFileFormat::Expedition,
                file_name: "Farr 40.txt".into(),
                polar: grid,
                matrix: None,
                library_id: Some(7),
            },
        ));
        let first = to_bytes(&project).unwrap();
        let loaded = from_bytes(&first).unwrap();
        assert!(matches!(
            &loaded.sources[0].kind,
            crate::SourceKind::PolarFile { library_id: Some(7), .. }
        ));
        assert_eq!(to_bytes(&loaded).unwrap(), first);
    }

    #[test]
    fn an_eight_document_reads_as_nine_with_no_library_ids() {
        let project = Project::new("Old", Boat::default(), 1_700_000_000);
        let mut value = serde_json::to_value(&project).unwrap();
        value["schema_version"] = Value::from(8);
        value["boat_tabs"] = Value::Array(vec![]);
        migrate(&mut value).unwrap();
        assert_eq!(value["schema_version"], 9);
        let loaded: Project = serde_json::from_value(value).unwrap();
        assert_eq!(loaded.schema_version, 9);
    }
```

Use the file's own serialisation helpers: read the names of the two functions the existing `save → load` tests call (`to_bytes`/`from_bytes` or `write`/`read` of a `.wpsproj`) and use those. `Colour::parse` — check `crates/pe-core/src/colour.rs` for the constructor's name and use it.

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p pe-core a_library_id_survives -- --nocapture`
Expected: compile error, `library_id` is not a field of `PolarFile`.

- [ ] **Step 3: Add the field, bump the schema, add the migration**

`crates/pe-core/src/source.rs`, inside `PolarFile`:

```rust
        /// Full WhirlwindMatrixPolar payload, when the source was imported
        /// from that matrix format. `polar` is its 2D editor/blend projection.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        matrix: Option<Box<crate::polar::WhirlwindMatrixPolar>>,
        /// The polar library entry this file was added from (spec.md 5.6),
        /// so the library marks it Added and race projects know it. A
        /// source outlives its entry: an id no entry holds is just a source.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        library_id: Option<u64>,
```

`crates/pe-core/src/project.rs:22`: `pub const SCHEMA_VERSION: u32 = 9;`

`crates/pe-core/src/io.rs`: add `(8, polar_library_ids),` to `MIGRATIONS` and the function:

```rust
/// 8 → 9 (spec.md 5.6): a polar file source may name the library entry it
/// came from. Nothing in an older file changes; boat tabs move with the
/// document's version as every migration moves them.
fn polar_library_ids(value: &mut Value) -> Result<()> {
    if let Some(tabs) = value.get_mut("boat_tabs").and_then(Value::as_array_mut) {
        for tab in tabs {
            tab["schema_version"] = Value::from(9);
        }
    }
    Ok(())
}
```

Update the existing test that asserts `loaded.schema_version, 8` (io.rs ~line 761) to 9.

- [ ] **Step 4: Fix every constructor**

Run: `cargo build --workspace --all-targets 2>&1 | grep -E "^error|-->" | head -80`
At each listed `SourceKind::PolarFile { … }` construction add `library_id: None,`. In pattern matches that destructure the variant with named fields and no `..`, add `..`. Repeat until the build is clean.

- [ ] **Step 5: Expose it in the summary**

`crates/pe-app/src/projects.rs`, `PolarFileSummary`:

```rust
    /// Its TWS axis, knots.
    pub tws: Vec<f64>,
    /// The polar library entry it came from, when it came from one.
    pub library_id: Option<u64>,
```

and where it is built (`SourceKind::PolarFile { format, file_name, polar, library_id, .. } =>` near line 118): `library_id: *library_id,`.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p pe-core && cargo test -p pe-app --lib`
Expected: PASS, including `every migration step exists` style tests.

- [ ] **Step 7: Commit**

```bash
git add crates/pe-core crates/pe-app/src crates/pe-app/tests crates/pe-app/examples crates/pe-polar/src
git commit -m "Give polar file sources an optional library id (schema 9)

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 2: The library store: index, files, add, replace, remove

**Files:**
- Create: `crates/pe-app/src/polar_library/mod.rs`
- Create: `crates/pe-app/src/polar_library/store.rs`
- Modify: `crates/pe-app/src/lib.rs` (`pub mod polar_library;` beside `pub mod polar_files;`)
- Modify: `crates/pe-app/src/commands.rs:31-64` (`AppServices.polar_library`), `:66-90` (`AppState::new`)
- Modify: `crates/pe-app/Cargo.toml` (move `sha2 = "0.10.9"` from `[dev-dependencies]` to `[dependencies]`)
- Modify: `crates/pe-app/src/projects.rs:157` (`fn format_name` → `pub(crate) fn format_name`)
- Test: unit tests inside `store.rs`

**Interfaces:**
- Produces (used by Tasks 3–7):
  - `polar_library::{DIR, FILES, INDEX, INDEX_SCHEMA, MAX_NAME_BYTES}`
  - `polar_library::LibraryOrigin`, `LibraryBoat`, `LibraryAxes`, `ThumbCurve`, `LibraryEntry`, `LibraryIndex`, `Loaded`, `PolarLibrary`
  - `polar_library::dir(&AppState) -> PathBuf`, `file_path(&AppState, &LibraryEntry) -> PathBuf`, `available(&AppState, &LibraryEntry) -> bool`, `extension(PolarFileFormat) -> &'static str`, `format_label(PolarFileFormat) -> &'static str`
  - `store::loaded(&AppState) -> Result<Arc<Loaded>>`
  - `store::NewEntry<'a> { name: &'a str, format: PolarFileFormat, bytes: &'a [u8], origin: LibraryOrigin, boat: LibraryBoat, polar: &'a PolarGrid }`
  - `store::add(&AppState, NewEntry) -> Result<(u64, bool)>` (id, `true` when an entry with the same bytes was reused)
  - `store::replace(&AppState, u64, NewEntry) -> Result<()>`
  - `store::remove(&AppState, u64) -> Result<()>`
  - `store::by_name(&Loaded, &str) -> Option<u64>`
  - `store::sha256_hex(&[u8]) -> String`, `store::file_name_for(&Path, &str, &str) -> String`, `store::summarise(&PolarGrid) -> (LibraryAxes, Vec<ThumbCurve>)`

- [ ] **Step 1: Write the types module**

`crates/pe-app/src/polar_library/mod.rs`:

```rust
//! The polar library (spec.md 5.6): one global set of polar files beside
//! the ORC and ORR catalogues, kept as files in their own formats in the
//! config folder with one index. The index is authoritative: a file no
//! entry names is ignored, an entry whose file is gone is "unavailable".
//! Not project data: a project holds copies (invariant 1).

pub mod commands;
pub mod search;
pub mod store;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pe_core::canonical;
use pe_core::polar::PolarFileFormat;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;

/// The index file's layout version.
pub const INDEX_SCHEMA: u32 = 1;
/// The folder under the config dir.
pub const DIR: &str = "polar-library";
/// The files' folder under it.
pub const FILES: &str = "files";
/// The index file under it.
pub const INDEX: &str = "index.json";
/// The most bytes of an entry's name that reach its file name.
pub const MAX_NAME_BYTES: usize = 120;

/// Where an entry came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "LibraryOrigin.ts")]
pub enum LibraryOrigin {
    /// Imported from a file of this name.
    Imported { file_name: String },
    /// Saved from a boat tab's blend.
    Saved { project: String, boat: String },
}

/// The boat an entry stands for, each field optional (spec.md 5.6).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export_to = "LibraryBoat.ts")]
pub struct LibraryBoat {
    /// Type or model, e.g. `"JPK 1180"`.
    pub model: Option<String>,
    /// Sail number as written, e.g. `"FRA 43800"`.
    pub sail_no: Option<String>,
    /// Three-letter country code.
    pub country: Option<String>,
    /// Builder.
    pub builder: Option<String>,
    /// Designer.
    pub designer: Option<String>,
    /// Year built.
    pub year: Option<i32>,
}

/// What the entry's grid covers, computed once when the entry is made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "LibraryAxes.ts")]
pub struct LibraryAxes {
    /// How many true wind angles.
    pub twa: u32,
    /// How many true wind speeds.
    pub tws: u32,
    /// The lowest wind speed, knots.
    #[serde(with = "canonical::knots_field")]
    pub tws_min: f64,
    /// The highest wind speed, knots.
    #[serde(with = "canonical::knots_field")]
    pub tws_max: f64,
    /// Whether any angle is beyond 180°: an independent port side.
    pub full_circle: bool,
}

/// One wind speed's curve of the thumbnail, as `orc::OrcThumbCurve` but
/// readable back from the index with canonical floats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThumbCurve {
    #[serde(with = "canonical::knots_field")]
    pub tws: f64,
    #[serde(with = "canonical::degrees_list")]
    pub twa: Vec<f64>,
    #[serde(with = "canonical::knots_list")]
    pub bsp: Vec<f64>,
}

impl From<&ThumbCurve> for crate::orc::OrcThumbCurve {
    fn from(curve: &ThumbCurve) -> Self {
        Self {
            tws: curve.tws,
            twa: curve.twa.clone(),
            bsp: curve.bsp.clone(),
        }
    }
}

/// One library file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryEntry {
    /// The name the person gave, or the imported file's name.
    pub name: String,
    pub format: PolarFileFormat,
    /// The file, relative to the library folder (`files/<name>.<ext>`).
    pub file: String,
    /// SHA-256 of the file's bytes, lower-case hex: what tells two imports
    /// of one file apart from two files.
    pub sha256: String,
    /// When it was made, UTC epoch seconds.
    pub added: i64,
    pub origin: LibraryOrigin,
    #[serde(default)]
    pub boat: LibraryBoat,
    pub axes: LibraryAxes,
    pub thumb: Vec<ThumbCurve>,
}

/// `index.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryIndex {
    pub schema_version: u32,
    /// The next id to give; ids are never reused.
    pub next_id: u64,
    pub entries: BTreeMap<u64, LibraryEntry>,
}

impl LibraryIndex {
    pub fn empty() -> Self {
        Self {
            schema_version: INDEX_SCHEMA,
            next_id: 1,
            entries: BTreeMap::new(),
        }
    }
}

/// The index as read, with the reason it could not be when it could not:
/// a damaged index reads as empty and refuses every write until repaired.
#[derive(Debug)]
pub struct Loaded {
    pub index: LibraryIndex,
    pub damaged: Option<String>,
}

/// The library's place in `AppServices`: the index loaded once, replaced
/// whole on every write.
#[derive(Debug, Default)]
pub struct PolarLibrary {
    pub(crate) loaded: Mutex<Option<Arc<Loaded>>>,
}

/// The library folder.
pub fn dir(state: &AppState) -> PathBuf {
    state.paths.config_dir.join(DIR)
}

/// An entry's file.
pub fn file_path(state: &AppState, entry: &LibraryEntry) -> PathBuf {
    dir(state).join(&entry.file)
}

/// Whether the entry's file is there to be read.
pub fn available(state: &AppState, entry: &LibraryEntry) -> bool {
    file_path(state, entry).is_file()
}

/// The extension each format is written with (spec.md 6).
pub fn extension(format: PolarFileFormat) -> &'static str {
    match format {
        PolarFileFormat::Expedition => "txt",
        PolarFileFormat::Adrena | PolarFileFormat::OpenCpn => "pol",
        PolarFileFormat::Csv | PolarFileFormat::QtVlm => "csv",
        PolarFileFormat::LuckGrib => "tsv",
        PolarFileFormat::WhirlwindMatrixPolar => "wmpolar",
    }
}

/// The format as the interface names it, searchable as a word.
pub fn format_label(format: PolarFileFormat) -> &'static str {
    match format {
        PolarFileFormat::Expedition => "Expedition",
        PolarFileFormat::Adrena => "Adrena",
        PolarFileFormat::Csv => "CSV",
        PolarFileFormat::QtVlm => "qtVlm",
        PolarFileFormat::OpenCpn => "OpenCPN",
        PolarFileFormat::LuckGrib => "LuckGrib",
        PolarFileFormat::WhirlwindMatrixPolar => "WhirlwindMatrixPolar",
    }
}
```

Create empty `search.rs` and `commands.rs` files for now (one doc comment line each) so the module compiles; Tasks 3–5 fill them.

- [ ] **Step 2: Write the failing store tests**

`crates/pe-app/src/polar_library/store.rs` — write the tests first, at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;

    const EXPEDITION: &[u8] = b"!twa/bsp\n6 52 5.9 90 7.1 150 6.0\n12 52 7.2 90 8.6 150 8.1\n";
    const ADRENA: &[u8] = b"TWA\\TWS\t6\t12\n52\t5.9\t7.2\n90\t7.1\t8.6\n";

    fn state(label: &str) -> AppState {
        let dir = std::env::temp_dir().join(format!(
            "pe-polar-library-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        AppState::new(AppPaths::in_directory(&dir).unwrap())
    }

    fn entry<'a>(name: &'a str, bytes: &'a [u8], polar: &'a PolarGrid) -> NewEntry<'a> {
        NewEntry {
            name,
            format: PolarFileFormat::Expedition,
            bytes,
            origin: LibraryOrigin::Imported { file_name: format!("{name}.txt") },
            boat: LibraryBoat::default(),
            polar,
        }
    }

    #[test]
    fn the_index_round_trips_byte_for_byte_and_ids_are_never_reused() {
        let state = state("roundtrip");
        let polar = pe_polar::read(EXPEDITION).unwrap().polar;
        let (first, reused) = add(&state, entry("Farr 40", EXPEDITION, &polar)).unwrap();
        assert_eq!((first, reused), (1, false));
        let adrena = pe_polar::read(ADRENA).unwrap().polar;
        let mut second = entry("Sun Fast", ADRENA, &adrena);
        second.format = PolarFileFormat::Adrena;
        assert_eq!(add(&state, second).unwrap(), (2, false));
        let before = std::fs::read(dir(&state).join(INDEX)).unwrap();
        // Dropping the loaded copy and reading again writes the same bytes.
        *state.polar_library.loaded.lock().unwrap() = None;
        let again = loaded(&state).unwrap();
        assert_eq!(again.index.entries.len(), 2);
        assert_eq!(serde_json::to_vec_pretty(&again.index).unwrap(), before);
        remove(&state, 2).unwrap();
        assert_eq!(add(&state, entry("Third", ADRENA, &adrena)).unwrap().0, 3, "id 2 is not reused");
    }

    #[test]
    fn the_same_bytes_are_stored_once() {
        let state = state("dedup");
        let polar = pe_polar::read(EXPEDITION).unwrap().polar;
        let (id, _) = add(&state, entry("Farr 40", EXPEDITION, &polar)).unwrap();
        let (same, reused) = add(&state, entry("Farr 40 copy", EXPEDITION, &polar)).unwrap();
        assert_eq!((same, reused), (id, true));
        assert_eq!(std::fs::read_dir(dir(&state).join(FILES)).unwrap().count(), 1);
    }

    #[test]
    fn a_name_a_file_system_refuses_is_kept_and_its_file_name_is_not() {
        let state = state("sanitise");
        let polar = pe_polar::read(EXPEDITION).unwrap().polar;
        let (id, _) = add(&state, entry("Fastnet: 2025 / blend?", EXPEDITION, &polar)).unwrap();
        let loaded = loaded(&state).unwrap();
        let entry = &loaded.index.entries[&id];
        assert_eq!(entry.name, "Fastnet: 2025 / blend?");
        assert_eq!(entry.file, "files/Fastnet_ 2025 _ blend_.txt");
        assert!(available(&state, entry));
    }

    #[test]
    fn a_clashing_file_name_gets_a_suffix() {
        let files = std::env::temp_dir().join(format!("pe-lib-names-{}", std::process::id()));
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("Farr 40.txt"), b"x").unwrap();
        std::fs::write(files.join("Farr 40 (2).txt"), b"x").unwrap();
        assert_eq!(file_name_for(&files, "Farr 40", "txt"), "Farr 40 (3).txt");
        assert_eq!(file_name_for(&files, "   ", "pol"), "polar.pol");
        let long = "x".repeat(300);
        assert_eq!(file_name_for(&files, &long, "txt").len(), MAX_NAME_BYTES + 4);
        let _ = std::fs::remove_dir_all(&files);
    }

    #[test]
    fn a_missing_file_is_unavailable_and_an_orphan_is_ignored() {
        let state = state("missing");
        let polar = pe_polar::read(EXPEDITION).unwrap().polar;
        let (id, _) = add(&state, entry("Farr 40", EXPEDITION, &polar)).unwrap();
        std::fs::write(dir(&state).join(FILES).join("orphan.txt"), EXPEDITION).unwrap();
        std::fs::remove_file(file_path(&state, &loaded(&state).unwrap().index.entries[&id])).unwrap();
        *state.polar_library.loaded.lock().unwrap() = None;
        let loaded = loaded(&state).unwrap();
        assert_eq!(loaded.index.entries.len(), 1, "the orphan is not an entry");
        assert!(!available(&state, &loaded.index.entries[&id]));
        remove(&state, id).unwrap();
        assert!(dir(&state).join(FILES).join("orphan.txt").is_file(), "only owned files are deleted");
    }

    #[test]
    fn a_damaged_index_reads_as_empty_and_refuses_writes() {
        let state = state("damaged");
        std::fs::create_dir_all(dir(&state)).unwrap();
        std::fs::write(dir(&state).join(INDEX), b"{ not json").unwrap();
        let loaded = loaded(&state).unwrap();
        assert!(loaded.index.entries.is_empty());
        assert!(loaded.damaged.is_some());
        let polar = pe_polar::read(EXPEDITION).unwrap().polar;
        let err = add(&state, entry("Farr 40", EXPEDITION, &polar)).unwrap_err().to_string();
        assert!(err.contains("index.json"), "{err}");
        assert_eq!(std::fs::read(dir(&state).join(INDEX)).unwrap(), b"{ not json");
    }

    #[test]
    fn replace_keeps_the_id_and_removes_the_old_file() {
        let state = state("replace");
        let polar = pe_polar::read(EXPEDITION).unwrap().polar;
        let (id, _) = add(&state, entry("Farr 40", EXPEDITION, &polar)).unwrap();
        let adrena = pe_polar::read(ADRENA).unwrap().polar;
        let mut next = entry("Farr 40", ADRENA, &adrena);
        next.format = PolarFileFormat::Adrena;
        replace(&state, id, next).unwrap();
        let loaded = loaded(&state).unwrap();
        assert_eq!(loaded.index.entries.len(), 1);
        assert_eq!(loaded.index.entries[&id].file, "files/Farr 40.pol");
        assert_eq!(std::fs::read_dir(dir(&state).join(FILES)).unwrap().count(), 1);
        assert_eq!(by_name(&loaded, "farr 40 "), Some(id), "names compare trimmed, case-insensitively");
    }

    #[test]
    fn the_summary_has_the_axes_and_the_thumbnail() {
        let polar = pe_polar::read(EXPEDITION).unwrap().polar;
        let (axes, thumb) = summarise(&polar);
        assert_eq!((axes.twa, axes.tws, axes.tws_min, axes.tws_max, axes.full_circle), (3, 2, 6.0, 12.0, false));
        let expected = crate::orc::thumbnail_grid(&polar);
        assert_eq!(thumb.len(), expected.len());
        assert_eq!(thumb[0].tws, expected[0].tws);
        assert_eq!(thumb[0].bsp, expected[0].bsp);
    }

    #[test]
    fn format_names_are_the_serde_names() {
        assert_eq!(serde_json::to_string(&PolarFileFormat::WhirlwindMatrixPolar).unwrap(), "\"whirlwind_matrix_polar\"");
        assert_eq!(serde_json::to_string(&PolarFileFormat::QtVlm).unwrap(), "\"qtvlm\"");
    }
}
```

If `PolarFileFormat` does not serialise to those names, the last test tells you; then add `#[serde(rename_all = "snake_case")]` is **not** the fix (it would change project files): instead use `crate::projects::format_name` as the string everywhere in this module and store the format in the index as that string via a `#[serde(with)]` adapter. Expect the names to match (`projects::format_name` mirrors them).

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p pe-app --lib polar_library`
Expected: compile errors (nothing defined yet).

- [ ] **Step 4: Write the store**

Top of `crates/pe-app/src/polar_library/store.rs`:

```rust
//! Reading and writing the library (spec.md 5.6): the index loaded once,
//! each write the file then the index, both atomic, the loaded copy
//! replaced whole after the index is on disk.

use std::path::{Path, PathBuf};
use std::sync::{Arc, MutexGuard};

use pe_core::polar::{PolarFileFormat, PolarGrid};
use sha2::{Digest, Sha256};

use super::{
    DIR, FILES, INDEX, INDEX_SCHEMA, LibraryAxes, LibraryBoat, LibraryEntry, LibraryIndex,
    LibraryOrigin, Loaded, MAX_NAME_BYTES, ThumbCurve, available, dir, extension, file_path,
};
use crate::commands::AppState;
use crate::error::{AppError, Context, Result};

/// What a new or replacing entry is made from.
pub struct NewEntry<'a> {
    pub name: &'a str,
    pub format: PolarFileFormat,
    pub bytes: &'a [u8],
    pub origin: LibraryOrigin,
    pub boat: LibraryBoat,
    pub polar: &'a PolarGrid,
}

fn lock(state: &AppState) -> Result<MutexGuard<'_, Option<Arc<Loaded>>>> {
    state
        .polar_library
        .loaded
        .lock()
        .map_err(|_| AppError::Internal("the polar library lock was poisoned".into()))
}

fn index_path(state: &AppState) -> PathBuf {
    dir(state).join(INDEX)
}

/// Reads the index; absent is empty, unreadable is empty and damaged.
fn load(path: &Path) -> Loaded {
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Loaded { index: LibraryIndex::empty(), damaged: None },
        Err(e) => Loaded { index: LibraryIndex::empty(), damaged: Some(format!("{}: {e}", path.display())) },
        Ok(bytes) => match serde_json::from_slice::<LibraryIndex>(&bytes) {
            Ok(index) if index.schema_version <= INDEX_SCHEMA => Loaded { index, damaged: None },
            Ok(index) => Loaded {
                index: LibraryIndex::empty(),
                damaged: Some(format!(
                    "{} was written by a newer PolarExplorer (index version {})",
                    path.display(),
                    index.schema_version
                )),
            },
            Err(e) => Loaded { index: LibraryIndex::empty(), damaged: Some(format!("{}: {e}", path.display())) },
        },
    }
}

/// The library as loaded, read from disk on first use.
pub fn loaded(state: &AppState) -> Result<Arc<Loaded>> {
    let mut guard = lock(state)?;
    if let Some(loaded) = guard.as_ref() {
        return Ok(Arc::clone(loaded));
    }
    let loaded = Arc::new(load(&index_path(state)));
    *guard = Some(Arc::clone(&loaded));
    Ok(loaded)
}

/// Lower-case hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// `name` as a file name every file system takes, with `extension`, and
/// ` (2)`, ` (3)`… when `files` already has one.
pub fn file_name_for(files: &Path, name: &str, extension: &str) -> String {
    let mut base: String = name
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    base = base.trim().trim_matches('.').trim().to_owned();
    while base.len() > MAX_NAME_BYTES {
        base.pop();
    }
    if base.is_empty() {
        base = "polar".to_owned();
    }
    let mut candidate = format!("{base}.{extension}");
    let mut n = 2;
    while files.join(&candidate).exists() {
        candidate = format!("{base} ({n}).{extension}");
        n += 1;
    }
    candidate
}

/// The axes summary and thumbnail of a grid, once, when the entry is made.
pub fn summarise(polar: &PolarGrid) -> (LibraryAxes, Vec<ThumbCurve>) {
    let axes = LibraryAxes {
        twa: u32::try_from(polar.twa.len()).unwrap_or(u32::MAX),
        tws: u32::try_from(polar.tws.len()).unwrap_or(u32::MAX),
        tws_min: polar.tws.first().copied().unwrap_or(0.0),
        tws_max: polar.tws.last().copied().unwrap_or(0.0),
        full_circle: polar.twa.iter().any(|a| *a > 180.0),
    };
    let thumb = crate::orc::thumbnail_grid(polar)
        .into_iter()
        .map(|c| ThumbCurve { tws: c.tws, twa: c.twa, bsp: c.bsp })
        .collect();
    (axes, thumb)
}

/// The id of the entry named `name`, compared trimmed and case-insensitively.
pub fn by_name(loaded: &Loaded, name: &str) -> Option<u64> {
    let wanted = name.trim().to_lowercase();
    loaded
        .index
        .entries
        .iter()
        .find(|(_, e)| e.name.trim().to_lowercase() == wanted)
        .map(|(id, _)| *id)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Refuses to write over an index that could not be read.
fn writable(loaded: &Loaded) -> Result<()> {
    match &loaded.damaged {
        Some(why) => Err(AppError::Internal(format!(
            "The polar library's index could not be read, so nothing is written to it until it is repaired or removed: {why}"
        ))),
        None => Ok(()),
    }
}

/// Writes `index` atomically and publishes it as the loaded copy.
fn publish(state: &AppState, guard: &mut MutexGuard<'_, Option<Arc<Loaded>>>, index: LibraryIndex) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(&index).map_err(|e| AppError::Internal(e.to_string()))?;
    let path = index_path(state);
    pe_core::io::write_atomic(&path, &bytes).doing("write the polar library index", path.display())?;
    **guard = Some(Arc::new(Loaded { index, damaged: None }));
    Ok(())
}

/// Writes the file of a new or replaced entry and returns its relative path.
fn write_file(state: &AppState, new: &NewEntry<'_>) -> Result<String> {
    let files = dir(state).join(FILES);
    std::fs::create_dir_all(&files).doing("make the polar library folder at", files.display())?;
    let file_name = file_name_for(&files, new.name, extension(new.format));
    let path = files.join(&file_name);
    pe_core::io::write_atomic(&path, new.bytes).doing("write the library polar to", path.display())?;
    Ok(format!("{FILES}/{file_name}"))
}

fn entry_of(new: &NewEntry<'_>, file: String, added: i64) -> LibraryEntry {
    let (axes, thumb) = summarise(new.polar);
    LibraryEntry {
        name: new.name.trim().to_owned(),
        format: new.format,
        file,
        sha256: sha256_hex(new.bytes),
        added,
        origin: new.origin.clone(),
        boat: new.boat.clone(),
        axes,
        thumb,
    }
}

/// Adds an entry, or answers the one that already holds these bytes
/// (`true`): nothing is stored twice (spec.md 5.6).
pub fn add(state: &AppState, new: NewEntry<'_>) -> Result<(u64, bool)> {
    let mut guard = lock(state)?;
    let current = match guard.as_ref() {
        Some(loaded) => Arc::clone(loaded),
        None => Arc::new(load(&index_path(state))),
    };
    writable(&current)?;
    let sha = sha256_hex(new.bytes);
    if let Some((id, entry)) = current.index.entries.iter().find(|(_, e)| e.sha256 == sha) {
        if available(state, entry) {
            *guard = Some(current);
            return Ok((*id, true));
        }
    }
    let file = write_file(state, &new)?;
    let mut index = current.index.clone();
    let id = index.next_id;
    index.next_id += 1;
    index.entries.insert(id, entry_of(&new, file, now()));
    publish(state, &mut guard, index)?;
    Ok((id, false))
}

/// Rewrites entry `id` from `new`, keeping its id and place; the old file
/// goes when the new one and the index are on disk.
pub fn replace(state: &AppState, id: u64, new: NewEntry<'_>) -> Result<()> {
    let mut guard = lock(state)?;
    let current = match guard.as_ref() {
        Some(loaded) => Arc::clone(loaded),
        None => Arc::new(load(&index_path(state))),
    };
    writable(&current)?;
    let Some(old) = current.index.entries.get(&id) else {
        return Err(AppError::Internal(format!("There is no library polar {id}.")));
    };
    let old_path = file_path(state, old);
    let file = write_file(state, &new)?;
    let mut index = current.index.clone();
    let added = old.added;
    index.entries.insert(id, entry_of(&new, file.clone(), added));
    publish(state, &mut guard, index)?;
    if old.file != file {
        let _ = std::fs::remove_file(old_path);
    }
    Ok(())
}

/// Removes entry `id` and its file (only a file under `files/`).
pub fn remove(state: &AppState, id: u64) -> Result<()> {
    let mut guard = lock(state)?;
    let current = match guard.as_ref() {
        Some(loaded) => Arc::clone(loaded),
        None => Arc::new(load(&index_path(state))),
    };
    writable(&current)?;
    let Some(entry) = current.index.entries.get(&id) else {
        return Err(AppError::Internal(format!("There is no library polar {id}.")));
    };
    let path = file_path(state, entry);
    if entry.file.starts_with(&format!("{FILES}/")) {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(AppError::Io(e)),
        }
    }
    let mut index = current.index.clone();
    index.entries.remove(&id);
    publish(state, &mut guard, index)
}
```

Keep `DIR` imported only if used; drop unused imports when clippy complains. `Context::doing` is the extension trait used in `paths.rs` (`use crate::error::{AppError, Context, Result};`).

`replace` keeps `added` of the old entry (the spec says the entry is rewritten in place; keeping the date is the simpler reading — the test does not check it). If `added` should move to now, change `let added = old.added;` to `now()`; either way the test passes.

- [ ] **Step 5: Wire the state**

`crates/pe-app/src/commands.rs`, in `AppServices` after `orc`:

```rust
    /// The polar library's index, loaded once (spec.md 5.6).
    pub polar_library: crate::polar_library::PolarLibrary,
```

and in `AppState::new`: `polar_library: crate::polar_library::PolarLibrary::default(),`. In `lib.rs` add `pub mod polar_library;`. In `crates/pe-app/Cargo.toml` move the `sha2 = "0.10.9"` line into `[dependencies]` with the comment `# The polar library tells two imports of one file from two files by their bytes (spec.md 5.6).` In `projects.rs` make `format_name` `pub(crate)`.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p pe-app --lib polar_library`
Expected: all nine PASS. Then `cargo clippy -p pe-app --all-targets -- -D warnings`.

- [ ] **Step 7: Commit**

```bash
git add crates/pe-app/src/polar_library crates/pe-app/src/lib.rs crates/pe-app/src/commands.rs crates/pe-app/src/projects.rs crates/pe-app/Cargo.toml Cargo.lock
git commit -m "Add the polar library store: index, files, add, replace, remove

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 3: Library search and the combined search

**Files:**
- Create (fill): `crates/pe-app/src/polar_library/search.rs`
- Test: unit tests inside `search.rs`

**Interfaces:**
- Consumes: `store::loaded`, `polar_library::{available, format_label, LibraryEntry, LibraryBoat, LibraryAxes, LibraryOrigin}`, `crate::orc::{OrcFilters, validated_filters, OrcThumbCurve, OrcHit, search_page}`, `crate::orr::{OrrHit, search}`, `pe_orc::fold::{fold, words, compact}`.
- Produces:
  - `search::LibraryHit { id: u64, name: String, format: String, origin: LibraryOrigin, boat: LibraryBoat, axes: LibraryAxes, added: i64, available: bool, in_project: bool, thumb: Vec<OrcThumbCurve> }` (Serialize, TS → `LibraryHit.ts`)
  - `search::LibrarySearchResult { total: u32, hits: Vec<LibraryHit> }` (TS → `LibrarySearchResult.ts`)
  - `search::search(&AppState, query: &str, filters: OrcFilters, limit: u32, offset: u32) -> Result<LibrarySearchResult>`
  - `search::browsing(query: &str, filters: &pe_orc::Filters) -> bool` (empty box, empty fields, no bounds)
  - `search::CombinedHit` (`#[serde(tag = "catalogue", rename_all = "snake_case")]` enum `Library(LibraryHit) | Orc(OrcHit) | Orr(OrrHit)`), `search::CombinedSearch { total: u32, hits: Vec<CombinedHit> }`, `search::combined(&AppState, query, OrcFilters, limit, offset) -> Result<CombinedSearch>` — the MCP tool's answer; the interface pages the three lists itself.
  - `search::holds(&pe_core::Project, u64) -> bool` (a source with that `library_id`).

- [ ] **Step 1: Write the failing tests**

At the bottom of `search.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use crate::polar_library::store::{NewEntry, add};
    use crate::polar_library::{LibraryBoat, LibraryOrigin};
    use pe_core::polar::PolarFileFormat;

    const EXPEDITION: &[u8] = b"!twa/bsp\n6 52 5.9 90 7.1 150 6.0\n12 52 7.2 90 8.6 150 8.1\n";

    fn state(label: &str) -> AppState {
        let dir = std::env::temp_dir().join(format!("pe-lib-search-{label}-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default()));
        AppState::new(AppPaths::in_directory(&dir).unwrap())
    }

    /// Adds `name` with `boat`; each call's bytes differ by a comment so none is deduplicated.
    fn seed(state: &AppState, name: &str, boat: LibraryBoat, k: usize) -> u64 {
        let bytes = format!("!{k}\n{}", std::str::from_utf8(&EXPEDITION[9..]).unwrap());
        let polar = pe_polar::read(bytes.as_bytes()).unwrap().polar;
        add(state, NewEntry { name, format: PolarFileFormat::Expedition, bytes: bytes.as_bytes(),
            origin: LibraryOrigin::Saved { project: "Fastnet polar".into(), boat: "Astarte".into() }, boat, polar: &polar }).unwrap().0
    }

    fn boat(model: &str, sail: &str, year: Option<i32>) -> LibraryBoat {
        LibraryBoat { model: Some(model.into()), sail_no: Some(sail.into()), country: Some(sail[..3].into()),
            builder: Some("JPK".into()), designer: None, year }
    }

    fn names(result: &LibrarySearchResult) -> Vec<&str> {
        result.hits.iter().map(|h| h.name.as_str()).collect()
    }

    #[test]
    fn words_start_words_across_fields_and_the_format_is_a_word() {
        let state = state("words");
        seed(&state, "Fastnet 2025 blend", boat("JPK 1180", "FRA 43800", Some(2019)), 1);
        seed(&state, "Sun Fast 3300 import", boat("Sun Fast 3300", "GBR 9", None), 2);
        let found = search(&state, "jpk 1180", OrcFilters::default(), 50, 0).unwrap();
        assert_eq!(names(&found), ["Fastnet 2025 blend"]);
        let found = search(&state, "fra43800", OrcFilters::default(), 50, 0).unwrap();
        assert_eq!(names(&found), ["Fastnet 2025 blend"], "a compact sail number");
        let found = search(&state, "expedition", OrcFilters::default(), 50, 0).unwrap();
        assert_eq!(found.total, 2, "the format name is a word of the main box");
        let found = search(&state, "astarte", OrcFilters::default(), 50, 0).unwrap();
        assert_eq!(found.total, 2, "the origin boat is a word of the main box");
        let found = search(&state, "astnet", OrcFilters::default(), 50, 0).unwrap();
        assert_eq!(found.total, 0, "a word's middle does not match");
    }

    #[test]
    fn field_boxes_match_their_field_only_and_bounds_exclude() {
        let state = state("fields");
        seed(&state, "JPK", boat("Sun Fast 3300", "FRA 1", Some(2021)), 1);
        seed(&state, "Other", boat("JPK 1180", "FRA 2", None), 2);
        let found = search(&state, "", OrcFilters { model: "jpk".into(), ..OrcFilters::default() }, 50, 0).unwrap();
        assert_eq!(names(&found), ["Other"]);
        let found = search(&state, "", OrcFilters { name: "jpk".into(), ..OrcFilters::default() }, 50, 0).unwrap();
        assert_eq!(names(&found), ["JPK"]);
        let found = search(&state, "", OrcFilters { year_min: Some(2020), ..OrcFilters::default() }, 50, 0).unwrap();
        assert_eq!(names(&found), ["JPK"], "no year is left out while a bound is set");
        let found = search(&state, "", OrcFilters { certificate_year: "2024".into(), ..OrcFilters::default() }, 50, 0).unwrap();
        assert_eq!(found.total, 0, "no library entry has a certificate year");
        let found = search(&state, "", OrcFilters { size_min: vec![Some(10.0)], ..OrcFilters::default() }, 50, 0).unwrap();
        assert_eq!(found.total, 0, "no library entry has measurements");
        let found = search(&state, "", OrcFilters { country: Some("fra".into()), ..OrcFilters::default() }, 50, 0).unwrap();
        assert_eq!(found.total, 2);
    }

    #[test]
    fn ranking_then_newest_then_name_and_the_empty_search_browses() {
        let state = state("rank");
        seed(&state, "Zulu", boat("Farr 40", "USA 40", None), 1);
        seed(&state, "Farr 40 one design", boat("Farr 40", "USA 41", None), 2);
        seed(&state, "Alpha", boat("Farr 40", "USA 42", None), 3);
        // The entries' `added` is the same second; force an order.
        let found = search(&state, "farr", OrcFilters::default(), 50, 0).unwrap();
        assert_eq!(names(&found)[0], "Farr 40 one design", "a name prefix beats a model match");
        let found = search(&state, "usa 42", OrcFilters::default(), 50, 0).unwrap();
        assert_eq!(names(&found), ["Alpha"], "an exact sail number");
        let all = search(&state, "", OrcFilters::default(), 2, 0).unwrap();
        assert_eq!(all.total, 3);
        assert_eq!(all.hits.len(), 2, "paged");
        let rest = search(&state, "", OrcFilters::default(), 2, 2).unwrap();
        assert_eq!(rest.hits.len(), 1);
        let mut seen: Vec<_> = all.hits.iter().chain(&rest.hits).map(|h| h.id).collect();
        seen.sort_unstable();
        assert_eq!(seen, [1, 2, 3]);
        assert!(browsing("  ", &crate::orc::validated_filters(OrcFilters::default()).unwrap()));
        assert!(!browsing("", &crate::orc::validated_filters(OrcFilters { name: "x".into(), ..OrcFilters::default() }).unwrap()));
    }

    #[test]
    fn an_unavailable_entry_is_still_listed_and_says_so() {
        let state = state("unavailable");
        let id = seed(&state, "Gone", boat("Farr 40", "USA 40", None), 1);
        let loaded = crate::polar_library::store::loaded(&state).unwrap();
        std::fs::remove_file(crate::polar_library::file_path(&state, &loaded.index.entries[&id])).unwrap();
        let found = search(&state, "gone", OrcFilters::default(), 50, 0).unwrap();
        assert_eq!(found.hits.len(), 1);
        assert!(!found.hits[0].available);
    }

    #[test]
    fn a_thousand_entries_answer_a_keystroke_within_the_budget() {
        let state = state("budget");
        for k in 0..1000 {
            seed(&state, &format!("Boat {k} blend"), boat(&format!("Model {}", k % 37), &format!("FRA {k}"), Some(1990 + (k % 30) as i32)), k);
        }
        for query in ["b", "bo", "boa", "boat 9", "model 1", "fra 99", "fra 999", "blend", "x", "model 12 boat"] {
            let start = std::time::Instant::now();
            let found = search(&state, query, OrcFilters::default(), 50, 0).unwrap();
            let took = start.elapsed();
            assert!(took.as_millis() < 30, "{query:?} took {took:?} for {} hits", found.total);
        }
    }
}
```

Seeding 1,000 entries writes 1,000 files and 1,000 index writes; if that takes over a minute in debug, seed through a single `LibraryIndex` built in memory and written once with `serde_json::to_vec_pretty` to `index.json` (files need not exist for search; `available` will be false, which is fine here).

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p pe-app --lib polar_library::search`
Expected: compile errors.

- [ ] **Step 3: Write the search**

```rust
//! Searching the library files (spec.md 5.6, the rules of §5.2): every
//! word starts a word of some field, field boxes match their field only,
//! ranked, newest first within a rank. An empty search browses the library.

use pe_core::SourceKind;
use pe_orc::fold::{compact, fold, words};
use serde::Serialize;
use ts_rs::TS;

use super::{LibraryAxes, LibraryBoat, LibraryEntry, LibraryOrigin, available, format_label, store};
use crate::commands::AppState;
use crate::error::Result;
use crate::orc::{OrcFilters, OrcHit, OrcThumbCurve};
use crate::orr::OrrHit;

/// A library file in the search results.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "LibraryHit.ts")]
pub struct LibraryHit {
    /// The entry's id, for polar_library_add and polar_library_remove.
    pub id: u64,
    pub name: String,
    /// The format's serde name (`expedition`, …).
    pub format: String,
    pub origin: LibraryOrigin,
    pub boat: LibraryBoat,
    pub axes: LibraryAxes,
    /// UTC epoch seconds.
    pub added: i64,
    /// Whether its file is there to be read.
    pub available: bool,
    /// Whether the boat already holds a source from this entry.
    pub in_project: bool,
    pub thumb: Vec<OrcThumbCurve>,
}

/// One page of library results.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "LibrarySearchResult.ts")]
pub struct LibrarySearchResult {
    pub total: u32,
    pub hits: Vec<LibraryHit>,
}

/// Whether a source came from entry `id`.
pub fn holds(project: &pe_core::Project, id: u64) -> bool {
    project.sources.iter().any(|s| matches!(&s.kind, SourceKind::PolarFile { library_id: Some(l), .. } if *l == id))
}

/// Nothing typed anywhere: the library is listed whole.
pub fn browsing(query: &str, filters: &pe_orc::Filters) -> bool {
    let f = &filters.fields;
    query.trim().is_empty()
        && [&f.name, &f.sail_no, &f.model, &f.builder, &f.designer, &f.certificate_year].iter().all(|s| s.trim().is_empty())
        && filters.country.is_none()
        && filters.year_min.is_none()
        && filters.year_max.is_none()
        && filters.size_min.iter().chain(&filters.size_max).all(Option::is_none)
}

fn matches(text: &str, query: &str) -> bool {
    let ws = words(text);
    let c = compact(text);
    words(query).iter().all(|q| ws.iter().any(|w| w.starts_with(q)) || c.starts_with(q))
}

fn field(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

/// Whether `entry` passes the filters and the main box.
fn admits(entry: &LibraryEntry, query: &str, filters: &pe_orc::Filters) -> bool {
    let b = &entry.boat;
    if !filters.fields.certificate_year.trim().is_empty()
        || filters.size_min.iter().chain(&filters.size_max).any(Option::is_some)
    {
        return false;
    }
    if filters.country.as_ref().is_some_and(|c| !c.eq_ignore_ascii_case(field(&b.country)))
        || filters.year_min.is_some_and(|min| b.year.is_none_or(|y| y < min))
        || filters.year_max.is_some_and(|max| b.year.is_none_or(|y| y > max))
    {
        return false;
    }
    let f = &filters.fields;
    let own = [
        (entry.name.as_str(), f.name.as_str()),
        (field(&b.sail_no), f.sail_no.as_str()),
        (field(&b.model), f.model.as_str()),
        (field(&b.builder), f.builder.as_str()),
        (field(&b.designer), f.designer.as_str()),
    ];
    if !own.iter().all(|(text, q)| q.trim().is_empty() || matches(text, q)) {
        return false;
    }
    let (project, boat) = match &entry.origin {
        LibraryOrigin::Saved { project, boat } => (project.as_str(), boat.as_str()),
        LibraryOrigin::Imported { file_name } => (file_name.as_str(), ""),
    };
    let haystack = format!(
        "{} {} {} {} {} {} {} {} {} {}",
        entry.name,
        field(&b.sail_no),
        field(&b.model),
        field(&b.country),
        field(&b.builder),
        field(&b.designer),
        b.year.map(|y| y.to_string()).unwrap_or_default(),
        format_label(entry.format),
        project,
        boat
    );
    matches(&haystack, query)
}

/// 0 a field box equal to its whole field, 1 the main box an exact sail
/// number, 2 a name prefix, 3 a model prefix, 4 anything else (spec.md 5.2).
fn rank(entry: &LibraryEntry, query: &str, filters: &pe_orc::Filters) -> u8 {
    let f = &filters.fields;
    let b = &entry.boat;
    let equal = |text: &str, q: &str| !q.trim().is_empty() && compact(text) == compact(q);
    if equal(&entry.name, &f.name) || equal(field(&b.sail_no), &f.sail_no) || equal(field(&b.model), &f.model)
        || equal(field(&b.builder), &f.builder) || equal(field(&b.designer), &f.designer)
    {
        return 0;
    }
    let q = fold(query.trim());
    if q.is_empty() {
        return 4;
    }
    let sail = compact(field(&b.sail_no));
    let number: String = sail.trim_start_matches(|c: char| c.is_alphabetic()).to_owned();
    if !sail.is_empty() && (compact(query) == sail || (!number.is_empty() && compact(query) == number)) {
        return 1;
    }
    if fold(&entry.name).starts_with(&q) {
        return 2;
    }
    if fold(field(&b.model)).starts_with(&q) {
        return 3;
    }
    4
}

fn hit(state: &AppState, id: u64, entry: &LibraryEntry, in_project: bool) -> LibraryHit {
    LibraryHit {
        id,
        name: entry.name.clone(),
        format: crate::projects::format_name(entry.format).to_owned(),
        origin: entry.origin.clone(),
        boat: entry.boat.clone(),
        axes: entry.axes.clone(),
        added: entry.added,
        available: available(state, entry),
        in_project,
        thumb: entry.thumb.iter().map(OrcThumbCurve::from).collect(),
    }
}

/// The library's answer to one search, one page of it.
pub fn search(state: &AppState, query: &str, filters: OrcFilters, limit: u32, offset: u32) -> Result<LibrarySearchResult> {
    let loaded = store::loaded(state)?;
    let filters = crate::orc::validated_filters(filters)?;
    let browse = browsing(query, &filters);
    let mut found: Vec<(u8, &u64, &LibraryEntry)> = loaded
        .index
        .entries
        .iter()
        .filter(|(_, e)| browse || admits(e, query, &filters))
        .map(|(id, e)| (if browse { 4 } else { rank(e, query, &filters) }, id, e))
        .collect();
    found.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| b.2.added.cmp(&a.2.added))
            .then_with(|| a.2.name.to_lowercase().cmp(&b.2.name.to_lowercase()))
            .then_with(|| b.1.cmp(a.1))
    });
    let total = u32::try_from(found.len()).unwrap_or(u32::MAX);
    let page: Vec<(&u64, &LibraryEntry)> = found
        .into_iter()
        .skip(offset as usize)
        .take(limit.clamp(1, crate::orc::MAX_LIMIT) as usize)
        .map(|(_, id, e)| (id, e))
        .collect();
    let held: Vec<bool> = state.with_session(|session| {
        Ok(match session.require_open() {
            Ok(open) => page.iter().map(|(id, _)| holds(&open.project, **id)).collect(),
            Err(_) => vec![false; page.len()],
        })
    })?;
    Ok(LibrarySearchResult {
        total,
        hits: page.into_iter().zip(held).map(|((id, e), in_project)| hit(state, *id, e, in_project)).collect(),
    })
}

/// A hit of any of the three lists, badged by `catalogue`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "catalogue", rename_all = "snake_case")]
#[ts(export_to = "CombinedHit.ts")]
pub enum CombinedHit {
    Library(LibraryHit),
    Orc(OrcHit),
    Orr(OrrHit),
}

/// The three lists interleaved — library, ORC, ORR — each in its own rank.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "CombinedSearch.ts")]
pub struct CombinedSearch {
    pub total: u32,
    pub hits: Vec<CombinedHit>,
}

/// The MCP tool's one search over everything (spec.md 5.6 §7): a page of
/// each list, interleaved.
pub fn combined(state: &AppState, query: &str, filters: OrcFilters, limit: u32, offset: u32) -> Result<CombinedSearch> {
    let library = search(state, query, filters.clone(), limit, offset)?;
    let (orc, orr) = if browsing(query, &crate::orc::validated_filters(filters.clone())?) {
        (None, None)
    } else {
        (
            Some(crate::orc::search_page(state, query, filters.clone(), limit, offset)?),
            Some(crate::orr::search(state, query, filters, limit, offset)?),
        )
    };
    let mut hits = Vec::new();
    let mut l = library.hits.into_iter().map(CombinedHit::Library);
    let mut c = orc.as_ref().map(|o| o.hits.clone()).unwrap_or_default().into_iter().map(CombinedHit::Orc);
    let mut r = orr.as_ref().map(|o| o.hits.clone()).unwrap_or_default().into_iter().map(CombinedHit::Orr);
    loop {
        let (a, b, d) = (l.next(), c.next(), r.next());
        if a.is_none() && b.is_none() && d.is_none() {
            break;
        }
        hits.extend([a, b, d].into_iter().flatten());
    }
    Ok(CombinedSearch {
        total: library.total + orc.map_or(0, |o| o.total) + orr.map_or(0, |o| o.total),
        hits,
    })
}
```

`OrcFilters` needs `Clone` (it derives `Clone` already, line 134 of `orc.rs`). `OrcHit` and `OrrHit` derive `Clone` and `Serialize`; `OrrHit` derives `TS` without `PartialEq`, fine. `pe_orc::fold::fold` exists (`fold.rs:45`).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p pe-app --lib polar_library::search`
Expected: five PASS. If the ranking test's first assertion fails because all three were added in the same second and order among rank-2 hits is by name, that is still correct: "Farr 40 one design" is the only name-prefix hit.

- [ ] **Step 5: Commit**

```bash
git add crates/pe-app/src/polar_library
git commit -m "Search the polar library by the catalogue rules, and combine the three lists

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 4: Import feeds the library; Add from and Remove from the library

**Files:**
- Modify: `crates/pe-app/src/polar_files.rs` (`parse` → `pub(crate) fn parse_file` returning the bytes too; `import` adds to the library)
- Create (fill): `crates/pe-app/src/polar_library/commands.rs`
- Modify: `crates/pe-app/src/error.rs` (two variants)
- Modify: `crates/pe-app/src/mcp/tools/mod.rs:88-100` (the refusal hint for the new error kind)
- Test: `crates/pe-app/tests/polar_library.rs` (new integration test file; it uses `common::TempRoot`)

**Interfaces:**
- Consumes: Task 2's `store::{add, remove, loaded}`, `NewEntry`; Task 3's `search::holds`.
- Produces:
  - `polar_files::parse_file(path: &Path, file: &str) -> Result<(pe_polar::format::Parsed, Vec<u8>), PolarImportFailure>`
  - `polar_files::ADD_LIBRARY: &str = "Add library polar"` (history label)
  - `polar_files::library_source(project: &mut Project, entry_id: u64, label: String, parsed: Parsed) -> Source` — builds the `PolarFile` source with `library_id: Some(entry_id)` (shared by Add and Task 6)
  - `polar_library::commands::LibraryInfo { entries: u32, folder: String, damaged: Option<String> }` (TS → `LibraryInfo.ts`)
  - `#[tauri::command] polar_library_info(state) -> Result<LibraryInfo>`; `commands::info(&AppState) -> Result<LibraryInfo>`
  - `#[tauri::command(async)] polar_library_search(state, boat_context, query: String, filters: OrcFilters, limit: u32, offset: Option<u32>) -> Result<LibrarySearchResult>`
  - `#[tauri::command] polar_library_add(state, boat_context, id: u64) -> Result<ProjectSummary>`; `commands::add(&AppState, u64) -> Result<ProjectSummary>`
  - `#[tauri::command] polar_library_remove(state, id: u64) -> Result<LibraryInfo>`; `commands::remove(&AppState, u64) -> Result<LibraryInfo>`
  - `AppError::LibraryUnavailable { name: String, file: String }` (kind `"library-unavailable"`), `AppError::LibraryNameTaken { name: String }` (kind `"library-name-taken"`, used by Task 5)

- [ ] **Step 1: Write the failing integration tests**

`crates/pe-app/tests/polar_library.rs`:

```rust
#![allow(clippy::expect_used, clippy::unwrap_used, reason = "test code")]
//! The polar library through the functions the commands call (spec.md 5.6):
//! import feeds it, Add reads from it, Remove empties it, projects keep copies.

mod common;

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::orc::OrcFilters;
use pe_app::polar_library::{commands, search, store};
use pe_app::{polar_files, projects};
use pe_core::SourceKind;

const EXPEDITION: &[u8] = b"!twa/bsp\n6 52 5.9 90 7.1 150 6.0\n12 52 7.2 90 8.6 150 8.1\n";

fn open(state: &AppState, name: &str) {
    projects::new(state, name.to_owned(), None, true).expect("new project");
}

#[test]
fn an_import_adds_to_the_library_once_and_marks_the_source() {
    let root = TempRoot::new("lib-import");
    let state = root.state();
    open(&state, "One");
    let file = root.file("farr.txt");
    std::fs::write(&file, EXPEDITION).unwrap();
    let first = polar_files::import(&state, &[file.clone()]).unwrap();
    assert_eq!(first.failures, vec![]);
    let id = first.project.sources[0].polar_file.as_ref().unwrap().library_id.expect("library id");
    let loaded = store::loaded(&state).unwrap();
    assert_eq!(loaded.index.entries.len(), 1);
    assert!(matches!(&loaded.index.entries[&id].origin, pe_app::polar_library::LibraryOrigin::Imported { file_name } if file_name == "farr.txt"));

    // The same bytes under another name, in another project: the same entry.
    open(&state, "Two");
    let copy = root.file("copy.txt");
    std::fs::write(&copy, EXPEDITION).unwrap();
    let second = polar_files::import(&state, &[copy]).unwrap();
    assert_eq!(second.project.sources[0].polar_file.as_ref().unwrap().library_id, Some(id));
    assert_eq!(store::loaded(&state).unwrap().index.entries.len(), 1);

    // A file that does not parse is not in the library.
    let bad = root.file("bad.csv");
    std::fs::write(&bad, b"TWA\\TWS;6\n52;x\n").unwrap();
    let third = polar_files::import(&state, &[bad]).unwrap();
    assert_eq!(third.failures.len(), 1);
    assert_eq!(store::loaded(&state).unwrap().index.entries.len(), 1);
}

#[test]
fn add_from_the_library_is_one_undo_step_and_marks_the_hit_added() {
    let root = TempRoot::new("lib-add");
    let state = root.state();
    open(&state, "Source");
    let file = root.file("farr.txt");
    std::fs::write(&file, EXPEDITION).unwrap();
    let id = polar_files::import(&state, &[file]).unwrap().project.sources[0].polar_file.as_ref().unwrap().library_id.unwrap();

    open(&state, "Target");
    let before = search::search(&state, "farr", OrcFilters::default(), 50, 0).unwrap();
    assert!(!before.hits[0].in_project);
    let added = commands::add(&state, id).unwrap();
    assert_eq!(added.sources.len(), 1);
    assert_eq!(added.sources[0].label, "farr.txt");
    assert_eq!(added.sources[0].polar_file.as_ref().unwrap().library_id, Some(id));
    assert_eq!(added.undo_label.as_deref(), Some(polar_files::ADD_LIBRARY));
    let after = search::search(&state, "farr", OrcFilters::default(), 50, 0).unwrap();
    assert!(after.hits[0].in_project);
    let undone = pe_app::edit::undo(&state).unwrap();
    assert_eq!(undone.sources.len(), 0);
}

#[test]
fn an_unavailable_entry_is_refused_in_words_and_can_still_be_removed() {
    let root = TempRoot::new("lib-unavailable");
    let state = root.state();
    open(&state, "P");
    let file = root.file("farr.txt");
    std::fs::write(&file, EXPEDITION).unwrap();
    let id = polar_files::import(&state, &[file]).unwrap().project.sources[0].polar_file.as_ref().unwrap().library_id.unwrap();
    let entry = store::loaded(&state).unwrap().index.entries[&id].clone();
    std::fs::remove_file(pe_app::polar_library::file_path(&state, &entry)).unwrap();
    open(&state, "Q");
    let err = commands::add(&state, id).unwrap_err();
    assert_eq!(err.kind(), "library-unavailable");
    assert!(err.to_string().contains("farr.txt"), "{err}");
    let info = commands::remove(&state, id).unwrap();
    assert_eq!(info.entries, 0);
}

#[test]
fn removing_an_entry_leaves_project_sources_alone() {
    let root = TempRoot::new("lib-remove");
    let state = root.state();
    open(&state, "P");
    let file = root.file("farr.txt");
    std::fs::write(&file, EXPEDITION).unwrap();
    let id = polar_files::import(&state, &[file]).unwrap().project.sources[0].polar_file.as_ref().unwrap().library_id.unwrap();
    let path = pe_app::polar_library::file_path(&state, &store::loaded(&state).unwrap().index.entries[&id]);
    commands::remove(&state, id).unwrap();
    assert!(!path.exists());
    let summary = projects::summary(&state).unwrap().unwrap();
    assert_eq!(summary.sources.len(), 1, "the project's copy stays");
    assert!(matches!(summary.sources[0].polar_file.as_ref().unwrap().library_id, Some(_)), "the id is kept, it dangles harmlessly");
    assert!(!search::holds(&pe_core::Project::new("x", pe_core::Boat::default(), 0), id));
}
```

Check the real names of `projects::new` (the function `new_project` calls; it may be `projects::create` or take other arguments) and of the undo function `edit::undo` by reading `crates/pe-app/src/projects.rs` and `edit.rs` headers, and adjust the two calls. `AppError::kind()` is public (`error.rs:132`).

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p pe-app --test polar_library`
Expected: compile errors.

- [ ] **Step 3: The errors**

`crates/pe-app/src/error.rs`, after `DuplicateCertificate`:

```rust
    /// A library polar whose file is gone (spec.md 5.6).
    #[error("The library polar {name:?} is unavailable: its file {file} is missing.")]
    LibraryUnavailable { name: String, file: String },

    /// Save to library under a name the library holds, without asking to replace.
    #[error("The library already holds a polar named {name:?}.")]
    LibraryNameTaken { name: String },
```

and in `kind()`: `Self::LibraryUnavailable { .. } => "library-unavailable", Self::LibraryNameTaken { .. } => "library-name-taken",`. In `mcp/tools/mod.rs`'s `From<AppError> for ToolError` add `"library-name-taken" => " Pass overwrite: true to replace it, only if the user means that.",`.

- [ ] **Step 4: Import feeds the library**

In `crates/pe-app/src/polar_files.rs` rename `parse` to `pub(crate) fn parse_file` and return `(Parsed, Vec<u8>)`: after `let bytes = std::fs::read(path)…`, keep `bytes` and end with `Ok((parsed, bytes))`. Add:

```rust
/// The history label of adding a library polar (spec.md 5.6).
pub const ADD_LIBRARY: &str = "Add library polar";

/// A polar file source from library entry `entry_id`, with the next
/// palette colour and the next id.
pub fn library_source(project: &mut Project, entry_id: u64, label: String, parsed: pe_polar::format::Parsed) -> Source {
    let colour = project.next_palette_colour();
    let id = project.allocate_source_id();
    Source::new(
        id,
        label.clone(),
        colour,
        SourceKind::PolarFile {
            format: parsed.format,
            file_name: label,
            polar: parsed.polar,
            matrix: parsed.matrix.map(Box::new),
            library_id: Some(entry_id),
        },
    )
}
```

(`use pe_core::Project;`.) In `import`, replace the parse loop and the source construction:

```rust
    let mut parsed = Vec::new();
    let mut failures = Vec::new();
    for raw in paths {
        let path = Path::new(raw);
        let file = file_name(path);
        match parse_file(path, &file) {
            Ok((polar, bytes)) => {
                // Into the library first (spec.md 5.6): the same bytes are one entry.
                let added = crate::polar_library::store::add(
                    state,
                    crate::polar_library::store::NewEntry {
                        name: &file,
                        format: polar.format,
                        bytes: &bytes,
                        origin: crate::polar_library::LibraryOrigin::Imported { file_name: file.clone() },
                        boat: crate::polar_library::LibraryBoat::default(),
                        polar: &polar.polar,
                    },
                );
                match added {
                    Ok((id, _)) => parsed.push((file, polar, id)),
                    Err(e) => failures.push(PolarImportFailure {
                        file: file.clone(),
                        line: None,
                        column: None,
                        reason: "library".to_owned(),
                        message: format!("Could not add {file} to the polar library: {e}"),
                    }),
                }
            }
            Err(failure) => failures.push(failure),
        }
    }
```

and in the apply closure build each source with `library_id: Some(id)` (destructure `(file, polar, id)`). Keep the labels `IMPORT_ONE`/`IMPORT_MANY`. The interface's `describeFailure` in `ui/src/panels/polarImport.ts` maps reason codes to text: add `library` there in Task 9 (`msg("could not be added to the polar library")`).

- [ ] **Step 5: The commands**

`crates/pe-app/src/polar_library/commands.rs`:

```rust
//! The library's IPC commands (spec.md 5.6): what the interface and the MCP
//! tools call. Search is in `search`, saving a blend in `save` (Task 5).

use pe_core::Command;
use serde::Serialize;
use ts_rs::TS;

use super::{available, dir, file_path, search, store};
use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::orc::OrcFilters;
use crate::projects::ProjectSummary;

/// The section footer's facts.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "LibraryInfo.ts")]
pub struct LibraryInfo {
    /// How many library polars there are.
    pub entries: u32,
    /// The library folder, for the person to find the files.
    pub folder: String,
    /// Why the index could not be read, when it could not.
    pub damaged: Option<String>,
}

pub fn info(state: &AppState) -> Result<LibraryInfo> {
    let loaded = store::loaded(state)?;
    Ok(LibraryInfo {
        entries: u32::try_from(loaded.index.entries.len()).unwrap_or(u32::MAX),
        folder: dir(state).to_string_lossy().into_owned(),
        damaged: loaded.damaged.clone(),
    })
}

#[tauri::command]
pub fn polar_library_info(state: tauri::State<'_, AppState>) -> Result<LibraryInfo> {
    info(&state)
}

/// Searches the library files (spec.md 5.6).
#[tauri::command(async)]
pub fn polar_library_search(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    query: String,
    filters: OrcFilters,
    limit: u32,
    offset: Option<u32>,
) -> Result<search::LibrarySearchResult> {
    let state = state.scoped(boat_context);
    search::search(&state, &query, filters, limit, offset.unwrap_or(0))
}

/// Adds library entry `id` to the boat as a polar file source, one undo step.
pub fn add(state: &AppState, id: u64) -> Result<ProjectSummary> {
    state.with_session(|session| session.require_open().map(|_| ()))?;
    let loaded = store::loaded(state)?;
    let entry = loaded
        .index
        .entries
        .get(&id)
        .ok_or_else(|| AppError::BadOption { field: "Library polar", value: id.to_string() })?;
    if !available(state, entry) {
        return Err(AppError::LibraryUnavailable { name: entry.name.clone(), file: entry.file.clone() });
    }
    let path = file_path(state, entry);
    let (parsed, _) = crate::polar_files::parse_file(&path, &entry.name)
        .map_err(|f| AppError::Internal(f.message))?;
    state.with_session(|session| {
        let open = session.require_open()?;
        open.ensure_editable()?;
        if search::holds(&open.project, id) {
            return Ok(ProjectSummary::of(open));
        }
        let source = crate::polar_files::library_source(&mut open.project, id, entry.name.clone(), parsed);
        let index = open.project.sources.len();
        open.apply(Command::Batch {
            label: crate::polar_files::ADD_LIBRARY.to_owned(),
            commands: vec![Command::AddSource { index, source: Box::new(source) }],
        })?;
        Ok(ProjectSummary::of(open))
    })
}

#[tauri::command]
pub fn polar_library_add(state: tauri::State<'_, AppState>, boat_context: Option<u64>, id: u64) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
    add(&state, id)
}

/// Removes entry `id` and its file. Not undoable; project sources are copies.
pub fn remove(state: &AppState, id: u64) -> Result<LibraryInfo> {
    store::remove(state, id)?;
    info(state)
}

#[tauri::command]
pub fn polar_library_remove(state: tauri::State<'_, AppState>, id: u64) -> Result<LibraryInfo> {
    remove(&state, id)
}
```

`allocate_source_id` and `next_palette_colour` are `Project` methods taking `&mut self` / `&self` (as `polar_source` in `tracker_project.rs:371` uses them); `library_source` takes `&mut Project`, so pass `&mut open.project`.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p pe-app --test polar_library && cargo test -p pe-app --lib polar_`
Expected: PASS. Then `cargo clippy -p pe-app --all-targets -- -D warnings`.

- [ ] **Step 7: Commit**

```bash
git add crates/pe-app/src/polar_files.rs crates/pe-app/src/polar_library crates/pe-app/src/error.rs crates/pe-app/src/mcp/tools/mod.rs crates/pe-app/tests/polar_library.rs
git commit -m "Import polar files into the library, and add or remove library polars

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 5: Save to library

**Files:**
- Create: `crates/pe-app/src/polar_library/save.rs` (add `pub mod save;` to `mod.rs`)
- Modify: `crates/pe-app/src/blend.rs:873` (`fn parse_format` → `pub(crate) fn parse_format`)
- Test: `crates/pe-app/tests/polar_library.rs` (two more tests)

**Interfaces:**
- Consumes: `crate::blend::{export_bytes_with_matrix, ExportAxes}`, `crate::matrix_export::MatrixExportOptions`, `store::{add, replace, by_name, loaded}`, `crate::boats::list`.
- Produces:
  - `save::LibrarySaveInput { name: String, format: String, axes: Option<ExportAxes>, matrix: Option<MatrixExportOptions>, boat: LibraryBoat, overwrite: bool }` (Deserialize, `#[serde(default)]` on `axes`, `matrix`, `boat`, `overwrite`; TS → `LibrarySaveInput.ts`)
  - `save::LibrarySaveDefaults { name: String, boat: LibraryBoat }` (TS → `LibrarySaveDefaults.ts`)
  - `#[tauri::command] polar_library_save_defaults(state, boat_context) -> Result<LibrarySaveDefaults>`; `save::defaults(&AppState) -> Result<LibrarySaveDefaults>`
  - `#[tauri::command(async)] polar_library_save(state, boat_context, input: LibrarySaveInput) -> Result<LibraryHit>`; `save::save(&AppState, LibrarySaveInput) -> Result<LibraryHit>`

- [ ] **Step 1: Write the failing tests**

Append to `crates/pe-app/tests/polar_library.rs`:

```rust
use pe_app::polar_library::save::{self, LibrarySaveInput};
use pe_app::polar_library::LibraryBoat;

fn input(name: &str, format: &str, overwrite: bool) -> LibrarySaveInput {
    LibrarySaveInput { name: name.into(), format: format.into(), axes: None, matrix: None, boat: LibraryBoat::default(), overwrite }
}

#[test]
fn a_saved_blend_is_the_export_bytes_in_every_format() {
    let root = TempRoot::new("lib-save");
    let state = root.state();
    open(&state, "Blend");
    common::add_blend_sources(&state);
    for format in ["expedition", "adrena", "csv", "qtvlm", "opencpn", "luckgrib", "whirlwind_matrix_polar"] {
        let hit = save::save(&state, input(&format!("Blend as {format}"), format, false)).unwrap();
        assert_eq!(hit.format, format);
        assert!(hit.available);
        assert!(matches!(&hit.origin, pe_app::polar_library::LibraryOrigin::Saved { project, .. } if project == "Blend"));
        let entry = store::loaded(&state).unwrap().index.entries[&hit.id].clone();
        let written = std::fs::read(pe_app::polar_library::file_path(&state, &entry)).unwrap();
        let project = projects::current_project(&state).unwrap();
        let expected = pe_app::blend::export_bytes_with_matrix(&project, format, None, None).unwrap();
        assert_eq!(written, expected, "{format}");
    }
    assert_eq!(store::loaded(&state).unwrap().index.entries.len(), 7);
}

#[test]
fn a_held_name_asks_to_replace_and_replacing_keeps_the_id_and_changes_the_extension() {
    let root = TempRoot::new("lib-replace");
    let state = root.state();
    open(&state, "Blend");
    common::add_blend_sources(&state);
    let first = save::save(&state, input("Mine", "expedition", false)).unwrap();
    let err = save::save(&state, input("mine", "adrena", false)).unwrap_err();
    assert_eq!(err.kind(), "library-name-taken");
    let second = save::save(&state, input("Mine", "adrena", true)).unwrap();
    assert_eq!(second.id, first.id);
    let loaded = store::loaded(&state).unwrap();
    assert_eq!(loaded.index.entries.len(), 1);
    assert_eq!(loaded.index.entries[&first.id].file, "files/Mine.pol");
    assert_eq!(std::fs::read_dir(pe_app::polar_library::dir(&state).join("files")).unwrap().count(), 1);
    // An empty name and an empty blend are refused before anything is written.
    assert!(save::save(&state, input("   ", "csv", false)).is_err());
    open(&state, "Empty");
    assert!(save::save(&state, input("Nothing", "csv", false)).is_err());
    assert_eq!(store::loaded(&state).unwrap().index.entries.len(), 1);
    let defaults = save::defaults(&state).unwrap();
    assert_eq!(defaults.name, "Empty");
}
```

`common::add_blend_sources(&AppState)` exists (`tests/common/mod.rs:123`) and adds a polar file and a track. Check for a helper that returns the open `Project` clone (`projects::current_project` may not exist): if there is none, add to `crates/pe-app/src/projects.rs` a `pub fn current_project(state: &AppState) -> Result<pe_core::Project>` that clones `session.require_open()?.project`, documented as a test and export seam.

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p pe-app --test polar_library`
Expected: compile errors.

- [ ] **Step 3: Write `save.rs`**

```rust
//! Save to library (spec.md 5.6 §4): the blend written as Export… would
//! write it, into the library under a name, with the boat's identity.

use pe_core::SourceKind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::search::{self, LibraryHit};
use super::store::{self, NewEntry};
use super::{LibraryBoat, LibraryOrigin};
use crate::blend::ExportAxes;
use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::matrix_export::MatrixExportOptions;
use crate::orc::OrcFilters;

/// What the Save to library dialog sends.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export_to = "LibrarySaveInput.ts")]
pub struct LibrarySaveInput {
    /// The entry's name, required.
    pub name: String,
    /// `expedition`, `adrena`, `csv`, `qtvlm`, `opencpn`, `luckgrib` or `whirlwind_matrix_polar`.
    pub format: String,
    /// Custom axes; the project's output grid when absent (spec.md 12.4).
    #[serde(default)]
    pub axes: Option<ExportAxes>,
    /// WhirlwindMatrixPolar options.
    #[serde(default)]
    pub matrix: Option<MatrixExportOptions>,
    /// The boat the polar stands for.
    #[serde(default)]
    pub boat: LibraryBoat,
    /// Replace an entry of the same name instead of refusing.
    #[serde(default)]
    pub overwrite: bool,
}

/// What the dialog opens with.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "LibrarySaveDefaults.ts")]
pub struct LibrarySaveDefaults {
    /// The boat tab's name.
    pub name: String,
    /// From the boat's first visible ORC or ORR source, else its first of either, else empty.
    pub boat: LibraryBoat,
}

fn identity(kind: &SourceKind) -> Option<LibraryBoat> {
    match kind {
        SourceKind::Orc { record } => Some(LibraryBoat {
            model: record.model.clone(),
            sail_no: Some(record.sail_no.clone()).filter(|s| !s.is_empty()),
            country: Some(record.country.clone()).filter(|s| !s.is_empty()),
            builder: record.builder.clone(),
            designer: record.designer.clone(),
            year: record.year,
        }),
        SourceKind::Orr { record } => Some(LibraryBoat {
            model: record.model.clone(),
            sail_no: Some(record.sail_no.clone()).filter(|s| !s.is_empty()),
            country: {
                let prefix: String = record.sail_no.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
                (prefix.len() == 3).then(|| prefix.to_ascii_uppercase())
            },
            builder: record.field("builder-bt").filter(|s| !s.is_empty()).map(str::to_owned),
            designer: record.field("designer-bt").filter(|s| !s.is_empty()).map(str::to_owned),
            year: record.build_year(),
        }),
        _ => None,
    }
}

pub fn defaults(state: &AppState) -> Result<LibrarySaveDefaults> {
    state.with_session(|session| {
        let open = session.require_open()?;
        let summary = crate::projects::ProjectSummary::of(open);
        let name = if summary.boat_name.trim().is_empty() { summary.name } else { summary.boat_name };
        let certificates = open.project.sources.iter().filter(|s| matches!(s.kind, SourceKind::Orc { .. } | SourceKind::Orr { .. }));
        let boat = certificates
            .clone()
            .find(|s| s.visible)
            .or_else(|| certificates.clone().next())
            .and_then(|s| identity(&s.kind))
            .unwrap_or_default();
        Ok(LibrarySaveDefaults { name, boat })
    })
}

#[tauri::command]
pub fn polar_library_save_defaults(state: tauri::State<'_, AppState>, boat_context: Option<u64>) -> Result<LibrarySaveDefaults> {
    let state = state.scoped(boat_context);
    defaults(&state)
}

/// Writes the blend into the library (spec.md 5.6 §4): the export bytes,
/// recomputed now, then the entry; a held name is refused unless `overwrite`.
pub fn save(state: &AppState, input: LibrarySaveInput) -> Result<LibraryHit> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::BadOption { field: "Name", value: input.name.clone() });
    }
    let format = crate::blend::parse_format(&input.format)?;
    let (project, boat_label) = state.with_session(|session| {
        let open = session.require_open()?;
        let summary = crate::projects::ProjectSummary::of(open);
        let label = if summary.boat_name.trim().is_empty() { summary.name } else { summary.boat_name };
        Ok((open.project.clone(), label))
    })?;
    let project_name = crate::boats::list(state)?.name;
    // Every export refusal happens here, before anything is written.
    let bytes = crate::blend::export_bytes_with_matrix(&project, &input.format, input.axes.as_ref(), input.matrix.as_ref())?;
    let parsed = pe_polar::read(&bytes).map_err(|e| AppError::Internal(format!("The saved polar does not read back: {e}")))?;
    let loaded = store::loaded(state)?;
    let existing = store::by_name(&loaded, name);
    let new = NewEntry {
        name,
        format,
        bytes: &bytes,
        origin: LibraryOrigin::Saved { project: project_name, boat: boat_label },
        boat: input.boat.clone(),
        polar: &parsed.polar,
    };
    let id = match existing {
        Some(id) if input.overwrite => {
            store::replace(state, id, new)?;
            id
        }
        Some(_) => return Err(AppError::LibraryNameTaken { name: name.to_owned() }),
        None => store::add(state, new)?.0,
    };
    let found = search::search(state, "", OrcFilters::default(), crate::orc::MAX_LIMIT, 0)?;
    found
        .hits
        .into_iter()
        .find(|h| h.id == id)
        .ok_or_else(|| AppError::Internal("The saved polar is not in the library it was just written to.".to_owned()))
}

#[tauri::command(async)]
pub fn polar_library_save(state: tauri::State<'_, AppState>, boat_context: Option<u64>, input: LibrarySaveInput) -> Result<LibraryHit> {
    let state = state.scoped(boat_context);
    save(&state, input)
}
```

Note `store::add` deduplicates by bytes: saving the same blend twice under two names answers the first entry's id with the first name. That matches "nothing is stored twice"; the dialog shows the hit it got back. If the second save gives `reused == true` and a different name, the first entry keeps its name — accept this; the plan's tests do not save identical bytes under two names.

If the library has more than `MAX_LIMIT` (200) entries the final lookup may miss the new one; instead of searching, build the hit directly: make `search::hit` `pub(crate)` and call `search::hit(state, id, &store::loaded(state)?.index.entries[&id], holds)` — do this rather than the search when implementing.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p pe-app --test polar_library`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/pe-app/src/polar_library crates/pe-app/src/blend.rs crates/pe-app/src/projects.rs crates/pe-app/tests/polar_library.rs
git commit -m "Save a tab's blend to the polar library under a name and format

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 6: Race projects match library polars

**Files:**
- Modify: `crates/pe-app/src/boats/tracker_project.rs:393-470` (profiles) and `:535-560` (the per-boat match loop)
- Test: `crates/pe-app/tests/polar_library.rs` (one test) — or, if a tracker-project unit test module already builds a `TrackerEvent` fixture (search `tracker_project.rs` for `#[cfg(test)]` and `TrackerEvent {`), put the test there and reuse that fixture.

**Interfaces:**
- Consumes: `store::loaded`, `available`, `file_path`, `polar_files::{parse_file, library_source}`, `search::holds`, `Profile::from_details`, `match_mode.matches`.
- Produces: nothing new; `BoatImportReport.polars` counts library matches.

- [ ] **Step 1: Write the failing test**

Find how the MCP race test builds a recorded event (`crates/pe-app/tests/mcp.rs`, the test that calls `race_project`, and its fixture under `tests/fixtures/` or the tracker cache helper it uses). Write, in `tests/polar_library.rs`:

```rust
#[test]
fn a_race_project_adds_a_library_polar_of_the_same_model() {
    let root = TempRoot::new("lib-race");
    let state = root.state();
    open(&state, "Seed");
    common::add_blend_sources(&state);
    let hit = save::save(&state, LibrarySaveInput {
        name: "Sun Fast 3300 library".into(), format: "expedition".into(), axes: None, matrix: None,
        boat: LibraryBoat { model: Some("Sun Fast 3300".into()), builder: Some("Jeanneau".into()), ..LibraryBoat::default() },
        overwrite: false,
    }).unwrap();
    let event = pe_trackers::event::TrackerEvent {
        // Copy the fixture construction the race_project MCP test uses, with one boat whose
        // `model` is Some("Sun Fast 3300") and one whose model is Some("Farr 40").
        ..
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let (project, reports, _warnings) = pe_app::boats::tracker_project::build(&state, &event, &cancel).unwrap();
    let tabs: Vec<&pe_core::Project> = std::iter::once(&project).chain(project.boat_tabs.iter()).collect();
    let matched = tabs.iter().find(|p| p.boat.name == event.boats[0].name).unwrap();
    assert!(search::holds(matched, hit.id), "the Sun Fast gets the library polar");
    let other = tabs.iter().find(|p| p.boat.name == event.boats[1].name).unwrap();
    assert!(!search::holds(other, hit.id), "the Farr 40 does not");
    assert_eq!(reports.iter().find(|r| r.boat == event.boats[0].name).unwrap().polars, 1);
}
```

Fill the `TrackerEvent` literal from the real type (`crates/pe-trackers/src/event.rs`); every field must be written out. The `..` above is a prompt to copy the fixture, not code to leave in.

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p pe-app --test polar_library a_race_project`
Expected: FAIL at the `holds` assertion (no library matching yet).

- [ ] **Step 3: Add the library profiles**

In `build_with_mode`, after `orr_profiles` is built:

```rust
    // Library polars that name a model join the catalogues (spec.md 5.6 §6).
    let library = crate::polar_library::store::loaded(state)?;
    let library_profiles: Vec<(u64, Profile)> = library
        .index
        .entries
        .iter()
        .filter(|(_, e)| e.boat.model.as_deref().is_some_and(|m| !m.trim().is_empty()))
        .filter(|(_, e)| crate::polar_library::available(state, e))
        .map(|(id, e)| {
            let mut fields = BTreeMap::new();
            fields.insert("model".into(), e.boat.model.clone().unwrap_or_default());
            fields.insert("builder".into(), e.boat.builder.clone().unwrap_or_default());
            fields.insert("sailNumber".into(), e.boat.sail_no.clone().unwrap_or_default());
            (*id, Profile::from_details(&fields))
        })
        .collect();
```

Add `.chain(library_profiles.iter().map(|(_, p)| p.clone()))` to `identity_candidates`. In the per-boat loop, after the ORR loop:

```rust
        for (id, candidate) in &library_profiles {
            check(cancel)?;
            if !match_mode.matches(&profile, candidate) || crate::polar_library::search::holds(&project, *id) {
                continue;
            }
            let Some(entry) = library.index.entries.get(id) else { continue };
            match crate::polar_files::parse_file(&crate::polar_library::file_path(state, entry), &entry.name) {
                Ok((parsed, _)) => {
                    let source = crate::polar_files::library_source(&mut project, *id, entry.name.clone(), parsed);
                    project.sources.push(source);
                    report.polars += 1;
                }
                Err(failure) => report.warnings.push(failure.message),
            }
        }
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p pe-app --test polar_library && cargo test -p pe-app --test mcp race`
Expected: PASS; the existing race tests still pass (an empty library adds nothing).

- [ ] **Step 5: Commit**

```bash
git add crates/pe-app/src/boats/tracker_project.rs crates/pe-app/tests/polar_library.rs
git commit -m "Match library polars by model when building a race project

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 7: Register the commands; the four MCP tools; the guide

**Files:**
- Modify: `crates/pe-app/src/lib.rs:111-200` (`generate_handler!`)
- Modify: `crates/pe-app/src/mcp/invoke.rs` (`TABLE` near line 219, `EXCLUDED` near line 93, the effects table near line 356)
- Create: `crates/pe-app/src/mcp/tools/polar_library.rs`
- Modify: `crates/pe-app/src/mcp/tools/mod.rs:24-36` (`mod polar_library;`) and wherever the tool routers are combined (search `tool_router` / `+` in `mod.rs` for how `sources::` routes are added and add `polar_library`'s the same way)
- Modify: `crates/pe-app/src/mcp/tools/guide.rs` (`INSTRUCTIONS` step 2, `GUIDE` §3 "Sources", "A whole race" step 3, §7 "Export")
- Modify: `crates/pe-app/examples/export_bindings.rs` (export the new TS types)
- Test: `crates/pe-app/tests/mcp.rs`

**Interfaces:**
- Consumes: Tasks 3–5's commands.
- Produces: tools `polar_library_search`, `polar_library_add`, `polar_library_save`, `polar_library_remove`; generated TS `LibraryHit.ts`, `LibrarySearchResult.ts`, `LibraryBoat.ts`, `LibraryAxes.ts`, `LibraryOrigin.ts`, `LibraryInfo.ts`, `LibrarySaveInput.ts`, `LibrarySaveDefaults.ts`, `CombinedHit.ts`, `CombinedSearch.ts`.

- [ ] **Step 1: Write the failing MCP tests**

Append to `crates/pe-app/tests/mcp.rs` (after the ORC test):

```rust
#[tokio::test]
async fn the_library_is_searched_saved_added_and_removed_through_the_tools() {
    let root = TempRoot::new("mcp-library");
    let app = mock_app(&root);
    let (port, token) = serve(&app);
    let (client, _) = with_polar(&root, port, &token).await;

    // The import put the file in the library; an empty search browses it.
    let browsed = call(&client, "polar_library_search", json!({})).await;
    assert_eq!(browsed["total"], 1);
    assert_eq!(browsed["hits"][0]["catalogue"], "library");
    assert_eq!(browsed["hits"][0]["name"], "farr.txt");
    assert_eq!(browsed["hits"][0]["in_project"], true);

    // Save the blend under a name with the boat's identity.
    let saved = call(&client, "polar_library_save", json!({
        "name": "Farr blend", "format": "adrena", "identity": { "model": "Farr 40", "sail_no": "USA 40" }
    })).await;
    assert_eq!(saved["format"], "adrena");
    assert_eq!(saved["boat"]["model"], "Farr 40");
    let id = saved["id"].as_u64().unwrap();
    let message = call_err(&client, "polar_library_save", json!({ "name": "farr blend", "format": "csv" })).await;
    assert!(message.contains("overwrite"), "{message}");
    let replaced = call(&client, "polar_library_save", json!({ "name": "Farr blend", "format": "csv", "overwrite": true })).await;
    assert_eq!(replaced["id"], id);

    // The combined search names its three lists; a model word finds the saved one.
    let found = call(&client, "polar_library_search", json!({ "query": "farr 40", "limit": 5 })).await;
    let catalogues: Vec<&str> = found["hits"].as_array().unwrap().iter().map(|h| h["catalogue"].as_str().unwrap()).collect();
    assert_eq!(catalogues[0], "library");
    assert!(catalogues.contains(&"orc"), "{catalogues:?}");

    // Add it to a second boat; the interface's read agrees; one undo.
    let boat = call(&client, "boat_add", json!({ "name": "Second" })).await["id"].as_u64().unwrap();
    let added = call(&client, "polar_library_add", json!({ "boat": boat, "id": id })).await;
    assert_eq!(added["sources"][0]["label"], "Farr blend");
    assert_eq!(added["sources"][0]["polar_file"]["library_id"], id);
    call(&client, "undo", json!({ "boat": boat })).await;
    assert_eq!(call(&client, "sources_list", json!({ "boat": boat })).await["sources"], json!([]));

    // Remove: gone from the library, the first boat's copy stays.
    let info = call(&client, "polar_library_remove", json!({ "id": id })).await;
    assert_eq!(info["entries"], 1);
    let message = call_err(&client, "polar_library_add", json!({ "boat": boat, "id": id })).await;
    assert!(message.contains("Library polar"), "{message}");
    assert_eq!(call(&client, "sources_list", json!({})).await["sources"].as_array().unwrap().len(), 1);
    client.cancel().await.expect("close");
}
```

Check `boat_add`'s answer shape in the boats test (`boats_are_added_renamed_removed_and_restored`) and read the new boat's id the way it does.

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p pe-app --test mcp the_library_is`
Expected: FAIL, unknown tool.

- [ ] **Step 3: Register the commands**

`lib.rs` `generate_handler!`, after `polar_files::import_polar_files,`:

```rust
            polar_library::commands::polar_library_info,
            polar_library::commands::polar_library_search,
            polar_library::commands::polar_library_add,
            polar_library::commands::polar_library_remove,
            polar_library::save::polar_library_save_defaults,
            polar_library::save::polar_library_save,
```

`invoke.rs` `TABLE`, after the `import_polar_files` line:

```rust
    command!(stateless polar_library_info, crate::polar_library::commands::polar_library_info, {}),
    command!(async polar_library_search, crate::polar_library::commands::polar_library_search, { boat_context: Option<u64>, query: String, filters: crate::orc::OrcFilters, limit: u32, offset: Option<u32> }),
    command!(polar_library_add, crate::polar_library::commands::polar_library_add, { boat_context: Option<u64>, id: u64 }),
    command!(polar_library_remove, crate::polar_library::commands::polar_library_remove, { id: u64 }),
    command!(polar_library_save_defaults, crate::polar_library::save::polar_library_save_defaults, { boat_context: Option<u64> }),
```

Look at how `orc_catalogue_info` (no `boat_context`, takes `state`) is written in `TABLE` and copy its macro form for `polar_library_info` and `polar_library_remove` (the `stateless` form is for commands that take no state at all; these take state). `EXCLUDED`: add `"polar_library_save",` with the comment `// Holds the overwrite rule, like export_polar: its tool asks for overwrite.` Effects table: `polar_library_info` Read, `polar_library_search` Read, `polar_library_add` Edit, `polar_library_remove` Anything, `polar_library_save_defaults` Read.

- [ ] **Step 4: The tool group**

`crates/pe-app/src/mcp/tools/polar_library.rs`:

```rust
//! The polar library group (spec.md 5.6 §7): one search over the library
//! files and both catalogues, adding a library polar to a boat, saving a
//! boat's blend into the library, and removing an entry.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use tauri::Manager;

use super::{PolarExplorer, ToolResult, json, typed};
use crate::commands::AppState;
use crate::orc::OrcFilters;

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibrarySearchParams {
    /// The boat's id, from boats_list. Without it, the first boat (for the Added marks).
    #[serde(default)]
    pub boat: Option<u64>,
    /// Words that must each start a word of some field: name, sail number, country, model, builder, designer, year, format. Empty lists the library's own polars, newest first, and no certificates.
    #[serde(default)]
    pub query: String,
    /// The same filters as orc_search (country, year_min, year_max, name, sail_no, model, builder, designer, certificate_year, size_min, size_max), as an object or a string of JSON.
    #[serde(default)]
    pub filters: Option<Value>,
    /// Results per list (library, ORC, ORR), 1–100; 20 without it.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Results of each list to skip, for the next page.
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryAddParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The library polar's `id` from a polar_library_search hit whose catalogue is "library".
    pub id: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibrarySaveParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The name the polar is listed under in the library.
    pub name: String,
    /// expedition, adrena, csv, qtvlm, opencpn, luckgrib or whirlwind_matrix_polar.
    pub format: String,
    /// Custom axes `{ "twa": [...], "tws": [...] }`; the boat's output grid without them.
    #[serde(default)]
    pub axes: Option<Value>,
    /// WhirlwindMatrixPolar options, as export_polar takes them.
    #[serde(default)]
    pub matrix: Option<Value>,
    /// The boat the polar stands for: `{ "model", "sail_no", "country", "builder", "designer", "year" }`, each optional. Lets the library find it by model and race projects match it.
    #[serde(default)]
    pub identity: Option<Value>,
    /// Replace a library polar of the same name. Pass it only when the user means that.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryRemoveParams {
    /// The library polar's `id`. Removing it is permanent; boats that hold it keep their copies.
    pub id: u64,
}

#[tool_router(router = polar_library_router, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "Searches the polar library: the user's saved and imported polar files, the ORC catalogue and the ORR catalogue, the three lists interleaved, each hit with `catalogue` library, orc or orr. A library hit has `id`, `name`, `format`, `boat` (model, sail number…), `available` and `in_project`. An empty query lists the library's own polars newest first. Pages with limit and offset per list."
    )]
    async fn polar_library_search(&self, Parameters(p): Parameters<LibrarySearchParams>) -> ToolResult {
        let filters: OrcFilters = match p.filters {
            Some(raw) => typed("filters", raw)?,
            None => OrcFilters::default(),
        };
        let limit = p.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = p.offset.unwrap_or(0);
        let found = self
            .run("polar_library_search", move |app| {
                let state = app.state::<AppState>().scoped(p.boat);
                crate::polar_library::search::combined(&state, &p.query, filters, limit, offset)
            })
            .await?;
        json(&found)
    }

    #[tool(
        description = "Adds a library polar (a polar_library_search hit with catalogue \"library\") to the boat as a polar file source, one undo step; the same entry is not added twice. Refused when its file is missing. For ORC and ORR hits use orc_add and orr_add."
    )]
    async fn polar_library_add(&self, Parameters(p): Parameters<LibraryAddParams>) -> ToolResult {
        let project = self
            .write("polar_library_add", false, move |app| {
                crate::polar_library::commands::polar_library_add(app.state(), p.boat, p.id)
            })
            .await?;
        json(&project)
    }

    #[tool(
        description = "Saves the boat's blend into the polar library under `name`, in `format`, recomputed from the sources as export_polar writes it, with the boat's `identity` so the library and race projects can find it by model. Answers the library hit. A name the library already holds is refused unless `overwrite` is true."
    )]
    async fn polar_library_save(&self, Parameters(p): Parameters<LibrarySaveParams>) -> ToolResult {
        let axes = match p.axes {
            Some(raw) => Some(typed::<crate::blend::ExportAxes>("axes", raw)?),
            None => None,
        };
        let matrix = match p.matrix {
            Some(raw) => Some(typed::<crate::matrix_export::MatrixExportOptions>("matrix", raw)?),
            None => None,
        };
        let boat = match p.identity {
            Some(raw) => typed::<crate::polar_library::LibraryBoat>("identity", raw)?,
            None => crate::polar_library::LibraryBoat::default(),
        };
        let input = crate::polar_library::save::LibrarySaveInput {
            name: p.name,
            format: p.format,
            axes,
            matrix,
            boat,
            overwrite: p.overwrite,
        };
        let hit = self
            .write("polar_library_save", false, move |app| {
                crate::polar_library::save::save(&app.state::<AppState>().scoped(p.boat), input)
            })
            .await?;
        json(&hit)
    }

    #[tool(
        description = "Removes a polar from the library and deletes its file. Permanent, not undoable: ask the user first. Boats that hold it keep their copies. Answers the library's count."
    )]
    async fn polar_library_remove(&self, Parameters(p): Parameters<LibraryRemoveParams>) -> ToolResult {
        let info = self
            .write("polar_library_remove", false, move |app| {
                crate::polar_library::commands::polar_library_remove(app.state(), p.id)
            })
            .await?;
        json(&info)
    }
}
```

`polar_library_save` calls `save::save` rather than the `#[tauri::command(async)]` wrapper because an async Tauri command cannot be called from `spawn_blocking` with a `State`; the recipe's rule is "the command function itself" — `save` is the function the command calls and nothing else, as `export_to` is for `export_polar` in `tools/export.rs`. Follow whatever `tools/export.rs` does for `export_polar` exactly. `polar_library_search` likewise calls `search::combined` because the combined answer is a tool-only shape; its library part is the command's `search::search`.

`LibraryBoat` needs `Deserialize` (it has) — `typed` needs `DeserializeOwned`; `ExportAxes` and `MatrixExportOptions` derive `Deserialize` (the export tool reads them the same way; copy its `typed` calls). Register the router in `mod.rs` beside the others (find where `sources_router`-style routers are summed into `tool_router`), and add `mod polar_library;`.

- [ ] **Step 5: The guide**

`guide.rs` `INSTRUCTIONS` step 2 becomes:

```
2. Add sources to a boat: polar_library_search (the user's saved and imported polars, ORC and ORR certificates, one search) then polar_library_add, orc_add or orr_add by the hit's catalogue; polar_files_import (polar files, also kept in the library); track_files_import, tracker_event + tracker_import or library_search + library_import (race tracks).
```

Keep the text under the limit the test `the_instructions_fit_what_a_client_shows` holds (run it). In `GUIDE` §3 add a first bullet:

```
   - The polar library: polar_library_search searches the user's own saved and imported polar files and both certificate catalogues at once; each hit says its `catalogue` (library, orc, orr). Add a library hit with polar_library_add, a certificate with orc_add or orr_add. An empty query lists the library's own polars newest first. polar_library_save writes a boat's blend into the library under a name (with the boat's model and sail number so it is found again); polar_library_remove deletes an entry, permanently, so ask first.
```

In "A whole race" step 3, before the `orc_search and orr_search` bullet: `- polar_library_search by the boat's design or class name: a polar the user saved or imported for that model is the best source there is.` In §7 "Export": `polar_library_save keeps the blend in the library for other projects; export_polar writes it to a path.` Run `cargo test -p pe-app --test mcp guide` and `--lib guide` so the "every tool is named" test passes.

- [ ] **Step 6: Bindings**

In `examples/export_bindings.rs` add imports and `export_all(&cfg)?` lines for `LibraryHit, LibrarySearchResult, LibraryBoat, LibraryAxes, LibraryOrigin, LibraryInfo, LibrarySaveInput, LibrarySaveDefaults, CombinedHit, CombinedSearch` (from `pe_app::polar_library::{…}`, `::search::{…}`, `::commands::LibraryInfo`, `::save::{…}`). Run `npm run bindings`; commit the generated files under `ui/src/generated/` — including the regenerated `PolarFileSummary.ts` with `library_id`.

- [ ] **Step 7: Run everything Rust**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p pe-app`
Expected: PASS, including `invoke.rs`'s table test and the guide tests.

- [ ] **Step 8: Commit**

```bash
git add crates/pe-app/src/lib.rs crates/pe-app/src/mcp crates/pe-app/examples/export_bindings.rs crates/pe-app/tests/mcp.rs ui/src/generated
git commit -m "Register the polar library commands and add its four MCP tools

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 8: The Polar library section of the left navigation

**Files:**
- Modify: `ui/src/ipc.ts` (the `api` object near line 414; `LONG_RUNNING` near line 93)
- Rename: `ui/src/panels/OrcPolars.tsx` → `ui/src/panels/PolarLibrary.tsx` (`git mv`), then edit
- Delete: `ui/src/panels/PolarFiles.tsx` (its Import and file list move into `PolarLibrary.tsx`)
- Modify: `ui/src/panels/catalogueSearch.ts` (three lists), `ui/src/panels/catalogueSearch.test.ts`
- Modify: `ui/src/panels/polarImport.ts:38` (`reasonText` gains `library`)
- Modify: `ui/src/panels/LeftNav.tsx`, `ui/src/panels/layout.ts` (`PanelState`)
- Modify: `ui/src/styles.css:1204` (`.catalogue-badge.library`)
- Rename: `ui/src/help/features/orc.ts` → `ui/src/help/features/polar-library.ts` (`git mv`), then edit; delete `ui/src/help/features/polar-files.ts`; `ui/src/help/features/panels.ts` (the `nav:*` ids)
- Modify: `ui/src/i18n/locales/{fr,de,es,it,nl,zh,ja,ar}/panels.ts` and `measurements.ts` (moved and new strings)
- Modify: `ui/src/App.test.tsx` (ids, headings, mocks), `tools/webdriver/ux/07-orc-fields.test.mjs` (selectors), `tools/webdriver/ux/02-polar-import.test.mjs` (`polar-library:import`)
- Test: `ui/src/panels/catalogueSearch.test.ts`, `ui/src/App.test.tsx`

**Interfaces:**
- Consumes: generated `LibraryHit`, `LibrarySearchResult`, `LibraryInfo` (Task 7), commands of Tasks 4–5.
- Produces: `api.polarLibraryInfo(): Promise<LibraryInfo>`, `api.polarLibrarySearch(query, filters, limit, offset): Promise<LibrarySearchResult>`, `api.polarLibraryAdd(id): Promise<ProjectSummary>`, `api.polarLibraryRemove(id): Promise<LibraryInfo>`, `api.polarLibrarySaveDefaults(): Promise<LibrarySaveDefaults>`, `api.polarLibrarySave(input): Promise<LibraryHit>`; `interleave3<T>(a, b, c)`; `PanelState.library`; feature ids `nav:polar-library`, `polar-library:*`; reveal ids `polar-library:results`, `polar-library:fields`, `polar-library:measurements`.

- [ ] **Step 1: Write the failing interleave test**

`ui/src/panels/catalogueSearch.test.ts`, add:

```ts
it("interleaves three lists, library first, and goes on when one runs out", () => {
  expect(interleave3(["l1", "l2"], ["c1"], ["r1", "r2", "r3"])).toEqual(["l1", "c1", "r1", "l2", "r2", "r3"]);
  expect(interleave3([], [], [])).toEqual([]);
});
```

Run: `npm run ui:test -- catalogueSearch` → FAIL (`interleave3` is not exported). Implement in `catalogueSearch.ts`:

```ts
/** Three lists — library, ORC, ORR — each in its own rank, alternated (spec.md 5.6). */
export function interleave3<T>(library: readonly T[], orc: readonly T[], orr: readonly T[]): T[] {
  const out: T[] = [];
  for (let k = 0; k < Math.max(library.length, orc.length, orr.length); k++) {
    if (k < library.length) out.push(library[k]!);
    if (k < orc.length) out.push(orc[k]!);
    if (k < orr.length) out.push(orr[k]!);
  }
  return out;
}
```

Keep `interleave` for now; delete it once nothing imports it (the panel will not).

- [ ] **Step 2: Update the App tests first**

In `ui/src/App.test.tsx`:
- The ids list near line 314: replace `"nav:orc", "nav:polar-files"` with `"nav:polar-library"`; the headings assertion near line 321 becomes `["Polar library", "Tracks"]`; line 396 `feature("nav:orc")` → `feature("nav:polar-library")`.
- The mock `switch` (near line 203) gains:
  ```ts
      case "polar_library_info": return { entries: 0, folder: "/lib/polar-library", damaged: null };
      case "polar_library_search": return { total: 0, hits: [] };
  ```
- Every `polar-files:import` → `polar-library:import`, `polar-files:remove` → `polar-library:remove-source`, every `orc:` feature id used in tests → `polar-library:` (e.g. `orc:search`, `orc:add`, `orc:fields`, `orc:field-designer`, `orc:fields-clear`), and `orc:remove` → `polar-library:remove-source`. Keep `orc-measure:*`.
- The polar-files test that reads `.polar-file-list` still applies: the imported files list lives in the merged section with the same class.
- Add one test, in the ORC search `describe`:

```ts
  it("browses the library when nothing is typed, and badges a library hit", async () => {
    await open([]);
    handlers.polar_library_search = (args) => (args?.query === ""
      ? { total: 1, hits: [LIBRARY_HIT] }
      : { total: 0, hits: [] });
    await mount();
    expect(q(".orc-results .catalogue-badge.library")?.textContent).toBe("Library");
    expect(q(".orc-results")!.textContent).toContain("Fastnet blend");
    expect(q(".orc-results")!.textContent).toContain("Adrena");
    await click(feature("polar-library:add"));
    expect(commands("polar_library_add")).toEqual([{ boatContext: 1, id: 3 }]);
  });
```

with a fixture beside `HIT`:

```ts
/** A library polar, as the Polar library section lists it. */
const LIBRARY_HIT: LibraryHit = {
  id: 3, name: "Fastnet blend", format: "adrena", origin: { saved: { project: "Fastnet polar", boat: "Astarte" } },
  boat: { model: "JPK 1180", sail_no: "FRA 43800", country: "FRA", builder: "JPK", designer: null, year: 2019 },
  axes: { twa: 37, tws: 11, tws_min: 4, tws_max: 24, full_circle: false }, added: 1_760_800_000, available: true, in_project: false,
  thumb: [{ tws: 6, twa: [52, 90, 150], bsp: [5.9, 7.1, 6.0] }],
};
```

Adapt to how the file's mock dispatch is actually written (a `switch` on the command name with `args`; add the case in the switch and make the browse case return the hit when `args?.query === ""`). Run `npm run ui:test -- App` → the new and the renamed tests FAIL.

- [ ] **Step 3: ipc.ts**

After `orcAdd`:

```ts
  // The polar library (spec.md 5.6).
  /** The library's size and folder, and why its index could not be read when it could not. */
  polarLibraryInfo: () => call<LibraryInfo>("polar_library_info"),
  /** Searches the library files by the catalogue rules; an empty query lists them all, newest first. */
  polarLibrarySearch: (query: string, filters: OrcFilters, limit: number, offset = 0) =>
    call<LibrarySearchResult>("polar_library_search", { query, filters, limit, offset }),
  /** Adds a library polar to the boat as a polar file source (undoable). */
  polarLibraryAdd: (id: number) => call<ProjectSummary>("polar_library_add", { id }),
  /** Removes a library polar and its file. Not undoable. */
  polarLibraryRemove: (id: number) => call<LibraryInfo>("polar_library_remove", { id }),
  /** The Save to library dialog's prefilled name and boat identity. */
  polarLibrarySaveDefaults: () => call<LibrarySaveDefaults>("polar_library_save_defaults"),
  /** Writes the blend into the library; kind "library-name-taken" when the name is held and `overwrite` is false. */
  polarLibrarySave: (input: LibrarySaveInput) => call<LibraryHit>("polar_library_save", { input }),
```

Import the generated types at the top. `LONG_RUNNING`: `polar_library_save: msg("Saving to the library"),`.

- [ ] **Step 4: The merged section**

`git mv ui/src/panels/OrcPolars.tsx ui/src/panels/PolarLibrary.tsx`. In it:

1. Rename the component `PolarLibrary`; update the doc comment: "The Polar library section of the left navigation (spec.md 5, 5.6, 6): one search box over the library files and both catalogues, …; Import… adds polar files to the library and to the boat; the boat's polar sources, each with Remove."
2. Types: `type SearchHit = OrcHit | OrrHit | LibraryHit;` and `interface Results { library: SearchResult; orc: SearchResult; orr: SearchResult; pages: number }`. Discriminate: `function isLibrary(hit: SearchHit): hit is LibraryHit { return "axes" in hit; }`, `isOrr` stays (`typeof hit.id === "string"`).
3. Badge:
   ```tsx
   function Badge({ kind }: { kind: "orc" | "orr" | "library" }) {
     const t = useT();
     return <span className={`catalogue-badge ${kind}`}>{kind === "library" ? t("Library") : kind === "orr" ? "ORR" : "ORC"}</span>;
   }
   ```
   (ORC/ORR are verbatim words; "Library" is translated.)
4. `load`: browsing is `query.trim() === "" && active === 0`. When browsing, ask only the library: `Promise.all([ask(wanted(before?.library), api.polarLibrarySearch), browsing ? none : ask(wanted(before?.orc), api.orcSearch), browsing ? none : ask(wanted(before?.orr), api.orrSearch)])` where `const none = Promise.resolve<SearchResult>({ total: 0, hits: [] })`. The effect that clears results when nothing is typed must instead search when browsing: replace its condition with `if (!measuresValid) { …clear…; return; }` and always `load(0, ticket, null)`.
5. Also re-run the search on `libraryRevision`, a counter bumped after Import, Add from library, Remove entry, and on `document://changed` (`listen("document://changed", …)` — if `ipc.ts` has no exported constant for it, add `export const DOCUMENT_CHANGED = "document://changed";` and use it; `App.tsx` listens to the same literal, switch it to the constant).
6. Count line: `t("No polar matches.")`, `t("Polars 1–{last} of {total}")`, `t("1 polar")`, `t("{total} polars")`; when browsing and `total === 0`: `t("The library has no saved or imported polars yet.")` (not the "no match" text).
7. Result rows: `interleave3(result.library.hits, result.orc.hits, result.orr.hits)`. A library row:
   ```tsx
   <li key={`library-${hit.id}`}>
     <Thumb hit={hit} />
     <span className="orc-text">
       <span className="orc-name"><Badge kind="library" />{hit.name}</span>
       <span className="muted orc-meta">{meta([FORMAT_NAMES[hit.format] ?? hit.format, describeLibraryAxes(hit.axes, t)])}</span>
       {(hit.boat.sail_no || hit.boat.model || hit.boat.year || hit.boat.builder) &&
         <span className="muted orc-meta">{meta([hit.boat.sail_no, hit.boat.model, hit.boat.year, hit.boat.builder])}</span>}
       <span className="muted orc-meta">{describeOrigin(hit, t)}</span>
     </span>
     <button className="small" data-feature="polar-library:add" disabled={hit.in_project || !hit.available}
       title={!hit.available ? t("This polar's file is missing from the library folder") : hit.in_project ? t("Already in this boat") : t("Add this library polar to the boat")}
       onClick={() => void addLibrary(hit)}>
       {!hit.available ? t("Unavailable") : hit.in_project ? t("Added") : t("Add")}
     </button>
     <button className="icon-button" data-feature="polar-library:remove-entry" onClick={() => setRemoving(hit)}
       title={t("Remove this polar from the library. This cannot be undone; boats that hold it keep their copies.")} aria-label={t("Remove from library")}>✕</button>
   </li>
   ```
   Keys of ORC rows become `` `orc-${hit.id}` `` and ORR `` `orr-${hit.id}` `` so a library id and a catalogue id never collide. Helpers in `polarImport.ts`:
   ```ts
   /** A library polar's axes, as its row shows them. */
   export function describeLibraryAxes(axes: LibraryAxes, t: (key: string, params?: Record<string, string | number>) => string): string {
     return t("TWA ({twaCount}) · TWS {min}–{max} kn ({twsCount})", {
       twaCount: axes.twa, min: axisValue(axes.tws_min), max: axisValue(axes.tws_max), twsCount: axes.tws,
     }) + (axes.full_circle ? ` · ${t("full circle")}` : "");
   }
   /** Where a library polar came from and when. */
   export function describeOrigin(hit: LibraryHit, t: (key: string, params?: Record<string, string | number>) => string): string {
     const date = new Date(hit.added * 1000).toISOString().slice(0, 10);
     return "saved" in hit.origin
       ? t("Saved from {project} · {boat}, {date}", { project: hit.origin.saved.project, boat: hit.origin.saved.boat, date })
       : t("Imported {file}, {date}", { file: hit.origin.imported.file_name, date });
   }
   ```
   (The generated `LibraryOrigin` is `{ "imported": { file_name } } | { "saved": { project, boat } }`; read the generated file and match its shape.) `reasonText` gains `case "library": return t("could not be added to the polar library");`.
8. `addLibrary`: `onProject(await api.polarLibraryAdd(hit.id)); setHint(later(msg("Added {name}."), { name: hit.name })); setLibraryRevision(r => r + 1)`. `removeEntry` after the ConfirmDialog (`title={t("Remove from the library?")}`, `body={t("“{name}” and its file will be removed from the library. Boats that hold it keep their copies. This cannot be undone.", { name })}`, `confirmLabel={t("Remove")}`): `await api.polarLibraryRemove(id); setLibraryRevision(…); setHint(later(msg("Removed {name} from the library."), { name }))`.
9. Import: move `importFiles` and the `failures` state and list from `PolarFiles.tsx` here, button `data-feature="polar-library:import"`, placed under the search block and above the results, inside `<div className="section-actions">`; after a successful import bump `libraryRevision`.
10. The added list: `const added = project.sources.filter((s) => s.orc !== null || s.orr !== null || s.polar_file !== null);` rendered with `<Badge kind={source.orr ? "orr" : source.orc ? "orc" : "library"} />` (a polar file shows the Library badge whether or not it has a `library_id`; its meta line is `FORMAT_NAMES[format] · describeAxes(file)` as `PolarFiles.tsx` did; certificate rows as today). Remove button `data-feature="polar-library:remove-source"`, placeholder `t("No polars in this boat yet.")`, `aria-label={t("Polars in the boat")}`.
11. Footer: before the ORC provenance line, `{libraryInfo !== null && <p className="muted orc-provenance" title={libraryInfo.folder}>{libraryInfo.entries === 1 ? t("Library: 1 saved or imported polar") : t("Library: {count} saved or imported polars", { count: libraryInfo.entries })}</p>}` and `{libraryInfo?.damaged && <p className="modal-error" role="alert">{t("The library index could not be read: {why}", { why: libraryInfo.damaged })}</p>}`; `libraryInfo` loaded with the catalogue infos and on `libraryRevision`.
12. Feature ids: every `data-feature="orc:…"` → `polar-library:…`; `onReveal("orc:results")` → `"polar-library:results"`, `"orc:fields"` → `"polar-library:fields"`, `"orc:measurements"` → `"polar-library:measurements"`; the `aria-label` of the search box becomes `t("Search the polar library")`, the title `t("Every word must match a field: name, sail number, country, model, builder, designer, year or format; the library's polars and both certificate catalogues are searched")`.

`LeftNav.tsx`: one section `<Section feature="nav:polar-library" title={t("Polar library")} tooltip={t("Search saved and imported polars and the ORC and ORR catalogues")} open={panels.library} onToggle={() => onToggle("library")}><PolarLibrary …/></Section>` then Tracks. `layout.ts`: `PanelState` gets `library: boolean` in place of `orc` and `polarFiles` (`OPEN.library = true`, `SECTION_DOCK.library = "left"`); `loadPanels` must tolerate a stored object without `library` (spread `OPEN` first — check it already does). Delete `PolarFiles.tsx`.

`styles.css`: after `.catalogue-badge.orr`: `.catalogue-badge.library { background: var(--accent-2, var(--accent)); }` — pick a token the theme defines (grep `--accent` in `styles.css`); the badge must read in every theme.

- [ ] **Step 5: The feature registry**

`git mv ui/src/help/features/orc.ts ui/src/help/features/polar-library.ts`; in it `section = ["section:polar-library"]`, `results = ["section:polar-library", "polar-library:results"]`, `byField = ["section:polar-library", "polar-library:fields"]`; every id `orc:x` → `polar-library:x`; `orc:remove` → `polar-library:remove-source` (label `msg("Remove polar from boat")`, description `msg("Remove this polar from the boat. Undo puts it back.")`); topics stay `"orc"` for the field and catalogue entries, `"polar-library"` for search, add, import, remove-entry (the topic is written in Task 10; until then `topics.test.ts` complains — do Tasks 8 and 10 before running the whole suite, or point these at `"orc"` and move them in Task 10). Add:

```ts
  { id: "polar-library:import", label: msg("Import polar files"),
    description: msg("Import Expedition, Adrena, qtVlm, OpenCPN, LuckGrib and WhirlwindMatrixPolar files into the library and the boat, several at once."),
    keywords: [msg("import"), "Expedition", "Adrena", msg("open"), ".txt", ".pol", ".csv", msg("library")], topic: "polar-library", reveal: section },
  { id: "polar-library:add", label: msg("Add polar"),
    description: msg("Add this library polar or certificate to the boat as a source."),
    keywords: [msg("add"), msg("library"), msg("certificate")], topic: "polar-library", reveal: results },
  { id: "polar-library:remove-entry", label: msg("Remove from library"),
    description: msg("Remove a saved or imported polar from the library and delete its file. Boats keep their copies; this cannot be undone."),
    keywords: [msg("delete"), msg("remove"), msg("library")], topic: "polar-library", reveal: results },
```

and change `polar-library:search`'s label to `msg("Search the polar library")`. Delete `features/polar-files.ts`. In `features/panels.ts` replace the `nav:orc` and `nav:polar-files` entries with one `nav:polar-library` (`label: msg("Polar library")`, description `msg("Fold or unfold the Polar library section: saved and imported polars and the ORC and ORR catalogues.")`). The reveal handler for `section:polar-library` is wherever `section:orc` was handled (grep `"section:"` in `ui/src`); rename there.

- [ ] **Step 6: Translations**

Run `npm run ui:test -- coverage`; it lists every missing key per language and every unused key. For each of the eight languages, in `locales/<lang>/panels.ts` (section strings) and `locales/<lang>/measurements.ts` (the search box strings live there), add the new keys with real translations (glossaries in `ui/src/i18n/GLOSSARY*.md`; "polar library" → fr "bibliothèque de polaires", de "Polarbibliothek", es "biblioteca de polares", it "libreria di polari", nl "polarbibliotheek", zh "极曲线库", ja "ポーラーライブラリ", ar "مكتبة المنحنيات القطبية") and delete the keys now unused ("No ORC or ORR polars in this project yet.", "ORC / ORR polars", "Search polar catalogues and add certificates", "Import Expedition and Adrena polars", "No polar files in this project yet.", "No certificate matches.", "Certificates 1–{last} of {total}", "1 certificate", "{total} certificates", …). Repeat until the coverage test is green.

- [ ] **Step 7: UX selectors**

In `tools/webdriver/ux/07-orc-fields.test.mjs` replace `orc:fields`, `orc:field-designer`, `orc:field-model`, `orc:fields-clear` with their `polar-library:` names; `.orc-count` text now matches `/polar/` not `/certificate/`. In `02-polar-import.test.mjs` `polar-files:import` → `polar-library:import`. (They run in Task 11.)

- [ ] **Step 8: Run the UI suite**

Run: `npm run ui:typecheck && npm run ui:test`
Expected: PASS (if `topics.test.ts` fails only on the missing `polar-library` topic, go on to Task 10 and run again before committing both).

- [ ] **Step 9: Commit**

```bash
git add ui/src tools/webdriver/ux/07-orc-fields.test.mjs tools/webdriver/ux/02-polar-import.test.mjs
git commit -m "Merge the catalogue and polar file sections into one Polar library section

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 9: Save to library… on the Blend row

**Files:**
- Create: `ui/src/panels/exportChoices.tsx` (the format, matrix, grid and preview controls, extracted from `ExportDialog.tsx`)
- Modify: `ui/src/panels/ExportDialog.tsx` (uses the extracted pieces; behaviour unchanged)
- Create: `ui/src/panels/SaveToLibraryDialog.tsx`
- Modify: `ui/src/panels/SourceList.tsx:50` (dialog state) and `:182-186` (the button)
- Modify: `ui/src/help/features/sources.ts` (`sources:save-library`), `ui/src/help/features/blend.ts` (the `save-library:*` controls)
- Modify: `ui/src/i18n/locales/*/blend.ts` (new strings)
- Test: `ui/src/App.test.tsx`

**Interfaces:**
- Consumes: `api.polarLibrarySaveDefaults`, `api.polarLibrarySave`, generated `LibrarySaveInput`, `LibrarySaveDefaults`, `LibraryBoat`, `LibraryHit`; `IpcError.kind === "library-name-taken"`.
- Produces:
  - `exportChoices.tsx`: `useExportChoices(project: ProjectSummary)` returning `{ format, setFormat, matrix, setMatrix, matrixFormat, custom, setCustom, twaText, setTwaText, twsText, setTwsText, twa, tws, axes, customInvalid, matrixInvalid, windCells, preview, error, canSave }` and `<ExportChoices choices={…} busy prefix="export" | "save-library" />` rendering the format select, `MatrixExportFields`, the grid fieldset, the error lines and `ExportTable`, every `data-feature` as `` `${prefix}:format` `` etc.
  - Feature ids `sources:save-library`, `save-library:name`, `save-library:model`, `save-library:sail`, `save-library:country`, `save-library:builder`, `save-library:designer`, `save-library:year`, `save-library:format`, `save-library:project-grid`, `save-library:custom-grid`, `save-library:custom-twa`, `save-library:custom-tws`, `save-library:preview`, plus the format options `save-library:expedition` … `save-library:whirlwind-matrix-polar`.

- [ ] **Step 1: Write the failing App tests**

Beside the export dialog test (`sources:export`, App.test.tsx ~line 966):

```ts
  it("saves the blend to the library under a name, asking before replacing", async () => {
    await open([POLAR, ORC]);
    handlers.polar_library_save_defaults = () => ({ name: "Astarte", boat: { model: "Swan 112", sail_no: "GBR 1124", country: "GBR", builder: "Nautor", designer: null, year: 1999 } });
    let saves = 0;
    handlers.polar_library_save = (args) => {
      saves += 1;
      const input = args?.input as { name: string; overwrite: boolean };
      if (saves === 1 && !input.overwrite) throw { kind: "library-name-taken", message: `The library already holds a polar named "${input.name}".` };
      return { ...LIBRARY_HIT, name: input.name };
    };
    await click(feature("sources:save-library"));
    const name = feature("save-library:name") as HTMLInputElement;
    expect(name.value).toBe("Astarte");
    expect((feature("save-library:model") as HTMLInputElement).value).toBe("Swan 112");
    await type(name, "Astarte 2026");
    await click(q(".save-library-dialog .primary")!);
    // Refused: the dialog asks to replace.
    expect(q(".modal-narrow")?.textContent).toContain("Replace");
    await click(q(".modal-narrow .danger")!);
    const sent = commands("polar_library_save") as { input: { name: string; overwrite: boolean; format: string; boat: { model: string } } }[];
    expect(sent.map((s) => [s.input.name, s.input.overwrite])).toEqual([["Astarte 2026", false], ["Astarte 2026", true]]);
    expect(sent[1]!.input.boat.model).toBe("Swan 112");
    expect(q(".save-library-dialog")).toBeNull();
  });
```

Use the file's existing helpers for typing into an input (search for how the rename test sets a value: `fireEvent.input`/`change` under `act`) and for the mock dispatch. Run `npm run ui:test -- App` → FAIL (no button).

- [ ] **Step 2: Extract the export choices**

Create `exportChoices.tsx` by moving, from `ExportDialog.tsx`, the state (`format`, `matrix`, `custom`, `twaText`, `twsText`, `preview`, `error`), the `axes`/`customInvalid`/`windCells`/`matrixInvalid` derivations and the preview effect into `useExportChoices(project)`, and the JSX from `<label className="export-format">` through `{preview && <ExportTable …/>}` into `ExportChoices({ choices, busy, prefix })`, substituting `` data-feature={`${prefix}:format`} `` and so on (the option ids keep the `whirlwind-matrix-polar` spelling). `ExportDialog` then reads `const choices = useExportChoices(project);` and renders `<ExportChoices choices={choices} busy={busy} prefix="export" />`; `canSave` becomes `!busy && choices.canSave`. `EXPORT_FORMATS`, `problemText` and `ExportTable` move to `exportChoices.tsx` and are re-exported from `ExportDialog.tsx` if anything imports them from there (grep). Run the export tests (`npm run ui:test -- App`) — the existing ones must still pass before going on.

- [ ] **Step 3: The dialog**

`ui/src/panels/SaveToLibraryDialog.tsx`:

```tsx
import { useBoatApi } from "../boats/context";
import { useEffect, useState } from "react";

import { describeError, reportFailure } from "../errors";
import type { LibraryBoat } from "../generated/LibraryBoat";
import type { ProjectSummary } from "../generated/ProjectSummary";
import { later, setHint } from "../hint";
import { msg, useT } from "../i18n";
import { IpcError } from "../ipc";
import ConfirmDialog from "../project/ConfirmDialog";
import { ExportChoices, useExportChoices } from "./exportChoices";

/**
 * Save to library (spec.md 5.6 §4): the export dialog's choices with a name
 * and the boat's identity, written into the library instead of a file the
 * person picks. A name the library holds asks to replace it.
 */
export default function SaveToLibraryDialog({ project, onClose, onSaved }: {
  project: ProjectSummary;
  onClose: () => void;
  /** After a save, so the Polar library section can refresh. */
  onSaved?: () => void;
}) {
  const api = useBoatApi();
  const t = useT();
  const choices = useExportChoices(project);
  const [name, setName] = useState("");
  const [boat, setBoat] = useState<LibraryBoat>({ model: null, sail_no: null, country: null, builder: null, designer: null, year: null });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [replacing, setReplacing] = useState(false);

  useEffect(() => {
    let live = true;
    void api.polarLibrarySaveDefaults().then((defaults) => {
      if (!live) return;
      setName(defaults.name);
      setBoat(defaults.boat);
    }).catch(reportFailure);
    return () => { live = false; };
  }, []);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") { event.preventDefault(); if (!busy) onClose(); } };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, busy]);

  const field = (key: keyof LibraryBoat) => ({
    value: boat[key] === null ? "" : String(boat[key]),
    onChange: (event: { target: { value: string } }) => {
      const text = event.target.value;
      setBoat((before) => ({ ...before, [key]: key === "year" ? (text.trim() === "" || Number.isNaN(Number(text)) ? null : Number(text)) : (text.trim() === "" ? null : text) }));
    },
  });

  const save = async (overwrite: boolean) => {
    setBusy(true);
    setError(null);
    try {
      const hit = await api.polarLibrarySave({
        name: name.trim(), format: choices.format, axes: choices.axes, matrix: choices.matrixFormat ? choices.matrix : null, boat, overwrite,
      });
      setHint(later(msg("Saved {name} to the library."), { name: hit.name }));
      onSaved?.();
      onClose();
    } catch (failure) {
      if (failure instanceof IpcError && failure.kind === "library-name-taken") setReplacing(true);
      else { reportFailure(failure); setError(failure); }
    } finally {
      setBusy(false);
    }
  };

  const title = t("Save to library");
  const canSave = !busy && name.trim() !== "" && choices.canSave;
  return (
    <div className="modal-backdrop" onClick={() => { if (!busy) onClose(); }}>
      <div className="modal settings export-dialog save-library-dialog" role="dialog" aria-label={title} aria-busy={busy} onClick={(event) => event.stopPropagation()}>
        <h2>{title}</h2>
        <p className="modal-summary muted">{t("The blend of the visible sources, recomputed now, kept in the library for every project.")}</p>
        <label className="settings-field">
          {t("Name")}
          <input type="text" data-feature="save-library:name" value={name} disabled={busy} required
            title={t("The name the polar is listed under in the library")} onChange={(event) => setName(event.target.value)} />
        </label>
        <fieldset className="save-library-boat" disabled={busy}>
          <legend>{t("Boat")}</legend>
          <label className="orc-field"><span>{t("Model / type")}</span><input type="text" {...field("model")} data-feature="save-library:model" title={t("So the library and race projects find this polar by model")} /></label>
          <label className="orc-field"><span>{t("Sail number")}</span><input type="text" {...field("sail_no")} data-feature="save-library:sail" title={t("As written on the sail, with its country")} /></label>
          <label className="orc-field"><span>{t("Country")}</span><input type="text" maxLength={3} {...field("country")} data-feature="save-library:country" title={t("Three-letter country code")} /></label>
          <label className="orc-field"><span>{t("Builder")}</span><input type="text" {...field("builder")} data-feature="save-library:builder" title={t("The builder")} /></label>
          <label className="orc-field"><span>{t("Designer")}</span><input type="text" {...field("designer")} data-feature="save-library:designer" title={t("The designer")} /></label>
          <label className="orc-field"><span>{t("Year built")}</span><input type="number" inputMode="numeric" {...field("year")} data-feature="save-library:year" title={t("The year the boat was built")} /></label>
        </fieldset>
        <ExportChoices choices={choices} busy={busy} prefix="save-library" />
        {error !== null && <p className="modal-error" role="alert" title={describeError(error).detail}>{describeError(error).text}</p>}
        <div className="modal-actions">
          <button disabled={busy} onClick={onClose} title={t("Close without saving")}>{t("Cancel")}</button>
          <span className="spacer" />
          <button className="primary" disabled={!canSave} title={t("Write the polar into the library")} onClick={() => void save(false)}>
            {busy ? t("Saving…") : t("Save")}
          </button>
        </div>
      </div>
      {replacing && (
        <ConfirmDialog title={t("Replace the library polar?")}
          body={t("The library already holds “{name}”. Replace it with this blend? Boats that hold the old one keep their copies.", { name: name.trim() })}
          confirmLabel={t("Replace")}
          onCancel={() => setReplacing(false)}
          onConfirm={() => { setReplacing(false); void save(true); }} />
      )}
    </div>
  );
}
```

`SourceList.tsx`: `useState<"settings" | "export" | "library" | "correction" | null>`; after the Export button:

```tsx
          <button className="small" data-feature="sources:save-library" disabled={readOnly}
            title={t("Keep the blend in the polar library under a name, for every project")}
            onClick={() => setDialog("library")}>
            {t("Save to library…")}
          </button>
```

and `{dialog === "library" && <SaveToLibraryDialog project={project} onClose={() => setDialog(null)} />}`. The Polar library section refreshes on `document://changed`? A save does not change the document. Have the dialog's `onSaved` dispatch `window.dispatchEvent(new CustomEvent("polar-library:changed"))` and the section bump `libraryRevision` on that window event (add the listener in Task 8's component now; export the event name `POLAR_LIBRARY_CHANGED = "polar-library:changed"` from `ui/src/panels/polarImport.ts`). Pass `onSaved` from `SourceList` as `() => window.dispatchEvent(new CustomEvent(POLAR_LIBRARY_CHANGED))`.

- [ ] **Step 4: Registry and strings**

`features/sources.ts`, after `sources:export`:

```ts
  { id: "sources:save-library", label: msg("Save to library"),
    description: msg("Keep the blend in the polar library under a name, with the boat's model and sail number, for every project."),
    keywords: [msg("save"), msg("library"), msg("blend"), msg("polar")], topic: "polar-library", reveal: list },
```

`features/blend.ts`: `const saving = { topic: "polar-library", reveal: list, landing: "sources:save-library" };` and entries for every `save-library:*` id listed in **Interfaces** (name: `msg("Library polar name")`, `msg("The name the polar is listed under in the library.")`; model/sail/country/builder/designer/year: `msg("Boat model")` … with one-sentence descriptions; format/grid/preview: copy the `export:*` entries' labels prefixed "Save to library: …", e.g. `msg("Save to library format")`). The `matrixFeatures` list in `blend.ts` is per-dialog too: read how its ids are built and give the save dialog's `MatrixExportFields` the same treatment (if it tags `export-matrix:*` ids without a prefix, give `MatrixExportFields` a `prefix` prop and register the `save-library-matrix:*` family the same way).

Translate every new string in the eight `blend.ts` locale files (and `panels.ts` where the section-side strings landed); `npm run ui:test -- coverage` lists them.

- [ ] **Step 5: Run the UI suite**

Run: `npm run ui:typecheck && npm run ui:test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src
git commit -m "Add Save to library… beside Export… on the Blend row

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 10: Help topic, user guide, badge style

**Files:**
- Modify: `ui/src/help/topics.ts` (new topic after `orc`; `polar-files` and `orc` `related` lists), `ui/src/help/topics.test.ts:16` (the required page list gains `"polar-library"`)
- Modify: `ui/src/help/locales/{ar,de,es,fr,it,ja,nl,zh}.ts` (the topic, translated whole)
- Modify: `ui/src/help/features/polar-library.ts` (topics of search/add/import/remove-entry → `"polar-library"` if parked on `"orc"` in Task 8)
- Modify: `docs/USER-GUIDE.md` (**Add sources** opens with the library; **Blend, compare and export** gains Save to library)

- [ ] **Step 1: The topic**

In `topics.ts` after the `orc` topic:

```ts
  { id: "polar-library", group: "Sources", title: "Polar library",
    paragraphs: [
      "The polar library is everything PolarExplorer can add to a boat as a polar: the ORC and ORR certificate catalogues, and the library's own files — every polar file you import and every blend you save from a tab. There is one library per computer, in PolarExplorer's own data folder; the section's footer names it and counts its polars. A project never depends on it: adding a polar copies it into the project, and removing it from the library leaves every project as it was.",
      "The search box at the top of the section searches all three at once. Every word must start a word of some field — name, sail number, country, model, builder, designer, year or format — and the results interleave the library's hits, badged Library, with the ORC and ORR certificates, each list in its own rank. With nothing typed, the library's own polars are listed newest first, so the box empty is the library itself. Search by field and Measurements work as for the catalogues; a certificate year or a measurement bound matches no library polar, since files carry neither.",
      "Import… adds polar files to the library and to the boat in one step. The same file imported twice is one library polar. Add puts a library polar into the boat as a polar file source, labelled with its name; Added marks one the boat holds, and Unavailable one whose file has gone from the library folder. Remove (✕ on a result) deletes the polar and its file from the library after asking; this cannot be undone, and boats that hold it keep their copies.",
      "Save to library…, beside Export… on the Blend row, writes the blend of the visible sources into the library in the format you choose, under a name, with the boat's model, sail number, country, builder, designer and year built — prefilled from the boat's certificate when it has one, and editable. The grid and format choices are the export dialog's, and what is written is exactly what Export… would write. A name the library already holds asks whether to replace that polar; replacing keeps its place and changes nothing in any project. A saved polar with a model is matched like a certificate when a race project is built.",
    ],
    parameters: [
      ["Polar library (section)", "Click the heading to fold or unfold the section."],
      ["Search", "Words to find in the library's polars and both catalogues. Empty, it lists the library."],
      ["Import…", "Adds one or more polar files to the library and to the boat. One undo takes them out of the boat; they stay in the library."],
      ["Add", "Adds that library polar or certificate to the boat as a source."],
      ["Remove (✕ on a result)", "Removes that polar from the library, after asking. Not undoable."],
      ["Polars in the boat", "Every polar source of the boat, with its colour, badge and details; ✕ removes it from the boat (undoable)."],
      ["Save to library…", "Names the blend, chooses its format and grid, and writes it into the library."],
      ["Name", "The name the polar is listed under. A held name asks to replace."],
      ["Boat", "Model, sail number, country, builder, designer and year built, so the polar is found by field and matched by model."],
    ],
    related: ["orc", "polar-files", "blend", "sources"] },
```

Add `"polar-library"` to the `orc` and `polar-files` topics' `related`, and to the required list in `topics.test.ts`. Translate the whole topic into the eight help locales, in the same position (the test compares shapes page for page). Point the `polar-library:*` features at the topic.

- [ ] **Step 2: The user guide**

`docs/USER-GUIDE.md` **Add sources**: open with a paragraph **Polar library.** describing the section (one search over the library's polars and both catalogues, Import…, Add, Remove, Unavailable, the folder), then keep the ORC and ORR paragraphs as detail. In **Blend, compare and export** add **Save to library.** (name, format, boat identity, replace rule). Keep the guide's voice.

- [ ] **Step 3: Run the UI suite and commit**

Run: `npm run ui:test`
Expected: PASS (`topics.test.ts` finds the page and its translations).

```bash
git add ui/src/help docs/USER-GUIDE.md
git commit -m "Document the polar library in the help reference and the user guide

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 11: The UX test and the budget

**Files:**
- Create: `tools/webdriver/ux/24-polar-library.test.mjs`
- Modify: `tools/webdriver/ux/07-orc-fields.test.mjs`, `02-polar-import.test.mjs` (already renamed in Task 8; run them)
- Test: the search budget test of Task 3 is the Rust side; add the interface's keystroke timing to `ui/src/panels/PolarLibrary.perf.test.tsx` only if the existing perf suite has a precedent for a panel (`ls ui/src/**/*.perf.test.ts*`; `TrackerImportDialog.perf.test.tsx` is one) — otherwise rely on the Rust test.

- [ ] **Step 1: The UX test**

```js
/**
 * The polar library (spec.md 5.6): Import… puts a file in the library and
 * the boat → an empty search lists it → Save to library… keeps the blend
 * under a name → the name finds the Library-badged hit → Add to a second
 * boat → Remove from the library asks and empties it.
 */
import { assert, newProject } from "./harness.mjs";

export default {
  name: "polar library",
  async run(t) {
    const d = t.driver;
    await newProject(d, "Library");
    await d.queueDialog([t.path("polar_examples/polars/Farr 40.txt")]);
    await d.click('[data-feature="polar-library:import"]');
    await d.waitFor(".polar-file-list li", { text: "Farr 40" });
    // Nothing typed: the library is listed, with the import in it.
    await d.waitFor(".orc-results li .catalogue-badge.library");
    assert.match(await d.text(".orc-results"), /Farr 40\.txt/);
    assert.match(await d.text(".orc-results"), /Added/);
    await t.shot("browse");

    await d.click('[data-feature="sources:save-library"]');
    await d.waitFor('[data-feature="save-library:name"]', { visible: true });
    await d.type('[data-feature="save-library:name"]', "Farr 40 blend");
    await d.type('[data-feature="save-library:model"]', "Farr 40");
    await d.waitFor('[data-feature="save-library:preview"] td');
    await t.shot("save-dialog");
    await d.click(".save-library-dialog .modal-actions .primary");
    await d.waitGone(".save-library-dialog");
    await d.waitFor(".orc-results li .orc-name", { text: "Farr 40 blend" });

    await d.type('[data-feature="polar-library:search"]', "farr 40 blend");
    await d.waitFor(".orc-results li .orc-name", { text: "Farr 40 blend" });
    const badges = await d.texts(".orc-results li .catalogue-badge");
    assert.equal(badges[0], "Library", "the library hit comes first");
    await t.shot("search");

    // A second boat, and the saved polar added to it.
    await d.click('[data-feature="boats:add"]');
    await d.waitFor(".polar-file-list, .placeholder");
    await d.type('[data-feature="polar-library:search"]', "farr 40 blend");
    await d.waitFor(".orc-results li .orc-name", { text: "Farr 40 blend" });
    await d.click('[data-feature="polar-library:add"]');
    await d.waitFor(".polar-file-list li", { text: "Farr 40 blend" });
    await t.shot("added-to-second-boat");

    await d.click('[data-feature="polar-library:remove-entry"]');
    await d.waitFor(".modal-narrow", { text: "Remove" });
    await d.click(".modal-narrow .danger");
    await d.waitGone(".orc-results li .orc-name");
    assert.equal(await d.count(".polar-file-list li"), 1, "the boat keeps its copy");
    await t.shot("removed");
  },
};
```

Check the real feature id of the add-boat control (`grep -rn 'data-feature="boats:' ui/src/boats`) and the save dialog's primary button selector against the markup; adjust. `d.texts` exists (`client.mjs:641`).

- [ ] **Step 2: Run the three UX tests**

Run: `npm run ux -- polar-library && npm run ux -- orc-fields && npm run ux -- polar-import`
Expected: PASS, with pictures in `target/ux-shots/polar library/`. Open `browse.png`, `save-dialog.png` and `search.png` and look at them: the Library badge is legible in the default theme, the dialog fits, the three badges differ.

- [ ] **Step 3: Commit**

```bash
git add tools/webdriver/ux
git commit -m "Drive the polar library end to end: import, browse, save, add, remove

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 12: spec.md and plan.md

**Files:**
- Modify: `spec.md` (§3.2 layout and left navigation; §5 title and a new §5.6; §6 Import…; §8 the Blend row; §12.4; §3.7's tool table and the `invoke` exclusions; §13)
- Modify: `plan.md` (a milestone after M27, before "## 3. Testing strategy")

- [ ] **Step 1: spec.md**

- §3.2: in the drawing, the first two left sections become `◀ Polar library / search… / Import… / polars in boat`; the bullet **Left navigation** reads "Two sections, in this order: Polar library (§5, §5.6, §6), Tracks (§7)."
- §5 heading → `## 5. Polar library`; add an opening sentence: "The polar library is the ORC catalogue (§5.1–5.4), the ORR catalogue (§5.5) and the library files (§5.6); one search (§5.2, §5.6) covers all three."
- §5.2: the paragraph on **Both catalogues at once** gains "and the library files (§5.6), three lists interleaved — library, ORC, ORR — each hit badged; with nothing typed, the library is listed newest first and the catalogues not at all."
- New `### 5.6 Library files`: §§2–6 of the design spec, in the spec's voice (storage, import, save, search rules, race projects), dated "asked 2026-10-18".
- §6: "**Import…** … Each file becomes one source … **and one library polar (§5.6)**; the same bytes are one polar."
- §8 (around line 1908): "**Export…** (§12.4) and **Save to library…** (§5.6)".
- §12.4: a paragraph "**Save to library…** writes the same bytes into the library (§5.6) under a name, with the boat's identity; a held name asks to replace."
- §3.7 table: Sources row gains `polar_library_search`, `polar_library_add`, `polar_library_save`, `polar_library_remove`; the `invoke` exclusions name `polar_library_save` with the overwrite rule; the Export paragraph mentions `polar_library_save`.
- §13: the ORC row becomes "Library and catalogue search result update per keystroke (1,000 library polars)".

- [ ] **Step 2: plan.md**

After M27:

```markdown
### M28 — Polar library · **complete** (2026-10-18)

Asked 2026-10-18: one global polar library — ORC, ORR and the person's own
imported and saved polar files — searched from the first left-nav section,
with Save to library on every tab's blend (design:
`docs/superpowers/specs/2026-10-18-polar-library-design.md`).

- [x] `polar_library` module: `polar-library/index.json` + `files/` in the
  config dir, atomic writes, ids never reused, bytes stored once (SHA-256),
  unavailable entries listed, a damaged index refuses writes.
- [x] Import… feeds the library and the boat; `library_id` on polar file
  sources (schema 9); Add from the library, Remove from it (permanent).
- [x] Save to library… beside Export…: name, boat identity, the export
  dialog's format and grid; bytes equal the export's; a held name asks.
- [x] One search over three lists, library first; empty search browses.
- [x] Race projects match library polars by model.
- [x] MCP: `polar_library_search`, `polar_library_add`, `polar_library_save`,
  `polar_library_remove`; the guide sends an agent to the library first.

**Validation.** `tests/polar_library.rs` and the module's unit tests (index
round trip, dedup, sanitising, unavailable, damaged, replace, save = export
bytes for all seven formats, race match); `tests/mcp.rs` drives the four
tools over HTTP; the UX suite's `24-polar-library` imports, browses, saves,
adds to a second boat and removes; 1,000 library entries answer a keystroke
under 30 ms (debug).
```

Fill the status honestly from what was actually done and measured (the number from the Task 3 test's output).

- [ ] **Step 3: The full check, then commit**

Run, from the root:

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && npm run ui:typecheck && npm run ui:test && npm run ui:perf && npm run check:offline
```

Expected: all PASS. Then:

```bash
git add spec.md plan.md
git commit -m "Specify the polar library and record milestone M28

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

## Self-review notes (done while writing)

- **Spec coverage.** §2 storage → Task 2; §3 import → Task 4; §4 save → Tasks 5, 9; §5 search → Tasks 3, 8; §6 race projects → Task 6; §7 MCP → Task 7; §8 interface/help/text → Tasks 8–10, 12; §9 tests → each task; §10 out of scope untouched.
- **Type consistency.** `LibraryBoat`, `LibraryOrigin`, `LibraryAxes`, `LibraryHit`, `LibrarySearchResult`, `LibraryInfo`, `LibrarySaveInput`, `LibrarySaveDefaults`, `CombinedHit`, `CombinedSearch` are named the same in Tasks 2–9; `store::add` returns `(u64, bool)`; `search::holds(&Project, u64)`; `polar_files::parse_file` returns `(Parsed, Vec<u8>)`; `polar_files::library_source(&mut Project, u64, String, Parsed) -> Source`.
- **Known seams the implementer must confirm against the code** (named in their steps): the project-creating and undo function names in `projects.rs`/`edit.rs` (Task 4), the `TrackerEvent` fixture (Task 6), the `command!` macro form for state-only commands and the router registration (Task 7), the App test mock dispatch and typing helpers (Tasks 8, 9), the add-boat feature id (Task 11).
