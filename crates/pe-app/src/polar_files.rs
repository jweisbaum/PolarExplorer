//! Importing polar files (spec.md 6).
//!
//! Every file is parsed in Rust (`pe_polar`); the webview only hands over the
//! paths the native picker returned. A file that fails names its line and
//! column and imports nothing, while the other files of the batch still
//! import. Everything that did import is **one** history entry, so one undo
//! takes the whole batch back out (spec.md 4.6).

use std::path::Path;

use pe_core::source::Source;
use pe_core::{Command, SourceKind};
use pe_polar::format::MAX_FILE_BYTES;
use pe_polar::{PolarError, Reason};
use serde::Serialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::Result;
use crate::projects::ProjectSummary;

/// The history label of a one-file import: an English key to translate.
pub const IMPORT_ONE: &str = "Import polar file";
/// The history label of a multi-file import.
pub const IMPORT_MANY: &str = "Import polar files";

/// A file that did not import, and why.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "PolarImportFailure.ts")]
pub struct PolarImportFailure {
    /// The file name, without its folder.
    pub file: String,
    /// 1-based line of the problem; null when the file could not be read.
    pub line: Option<u32>,
    /// 1-based column, in characters; null when the file could not be read.
    pub column: Option<u32>,
    /// A stable code the interface translates: one of `pe_polar`'s
    /// `Reason::code`s, or `"unreadable"`.
    pub reason: String,
    /// The English explanation, for the tooltip and logs.
    pub message: String,
}

/// What an import did.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "PolarImportResult.ts")]
pub struct PolarImportResult {
    /// The project after the import.
    pub project: ProjectSummary,
    /// The file names that became sources, in the order given.
    pub imported: Vec<String>,
    /// The files that did not.
    pub failures: Vec<PolarImportFailure>,
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn clamp(value: usize) -> Option<u32> {
    Some(u32::try_from(value).unwrap_or(u32::MAX))
}

fn format_failure(file: &str, err: &PolarError) -> PolarImportFailure {
    PolarImportFailure {
        file: file.to_owned(),
        line: clamp(err.line),
        column: clamp(err.column),
        reason: err.reason.code().to_owned(),
        message: format!("{file}, {err}"),
    }
}

fn unreadable(file: &str, why: impl std::fmt::Display) -> PolarImportFailure {
    PolarImportFailure {
        file: file.to_owned(),
        line: None,
        column: None,
        reason: "unreadable".to_owned(),
        message: format!("Could not read {file}: {why}"),
    }
}

/// Reads and parses one file. Refuses anything larger than a polar can be
/// before reading it.
fn parse(
    path: &Path,
    file: &str,
) -> std::result::Result<pe_polar::format::Parsed, PolarImportFailure> {
    let size = std::fs::metadata(path)
        .map_err(|e| unreadable(file, e))?
        .len();
    if size > MAX_FILE_BYTES as u64 {
        return Err(format_failure(
            file,
            &PolarError {
                line: 1,
                column: 1,
                reason: Reason::TooLarge,
            },
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| unreadable(file, e))?;
    let mut parsed = pe_polar::read(&bytes).map_err(|e| format_failure(file, &e))?;
    if parsed.format == pe_core::polar::PolarFileFormat::WhirlwindMatrixPolar {
        let Some(mut matrix) = parsed.matrix.take() else {
            return Err(unreadable(file, "the matrix payload is missing"));
        };
        if matrix.sog.is_none() {
            return Err(format_failure(
                file,
                &PolarError {
                    line: 1,
                    column: 1,
                    reason: Reason::MatrixInvalid(
                        "PDF-only files cannot be imported as speed points".to_owned(),
                    ),
                },
            ));
        }
        // The editor imports the SOG mode as one point per wind/wave
        // combination. PDF mode is deliberately not imported into the 2D
        // speed model.
        matrix.pdf = None;
        parsed.matrix = Some(matrix);
    }
    Ok(parsed)
}

/// Imports polar files as sources, one per file, labelled with its name.
#[tauri::command]
pub fn import_polar_files(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    paths: Vec<String>,
) -> Result<PolarImportResult> {
    let state = state.scoped(boat_context);
    import(&state, &paths)
}

/// [`import_polar_files`] without a Tauri handle.
pub fn import(state: &AppState, paths: &[String]) -> Result<PolarImportResult> {
    // Refuse before touching the disk when there is nowhere to put them.
    state.with_session(|session| session.require_open().map(|_| ()))?;

    let mut parsed = Vec::new();
    let mut failures = Vec::new();
    for raw in paths {
        let path = Path::new(raw);
        let file = file_name(path);
        match parse(path, &file) {
            Ok(polar) => parsed.push((file, polar)),
            Err(failure) => failures.push(failure),
        }
    }

    state.with_session(|session| {
        let open = session.require_open()?;
        open.ensure_editable()?;
        let imported: Vec<String> = parsed.iter().map(|(file, _)| file.clone()).collect();
        if !parsed.is_empty() {
            let colours = open.project.next_palette_colours(parsed.len());
            let start = open.project.sources.len();
            let mut commands = Vec::with_capacity(parsed.len());
            for (k, ((file, polar), colour)) in parsed.into_iter().zip(colours).enumerate() {
                let id = open.project.allocate_source_id();
                let source = Source::new(
                    id,
                    file.clone(),
                    colour,
                    SourceKind::PolarFile {
                        format: polar.format,
                        file_name: file,
                        polar: polar.polar,
                        matrix: polar.matrix.map(Box::new),
                    },
                );
                commands.push(Command::AddSource {
                    index: start + k,
                    source: Box::new(source),
                });
            }
            let label = if commands.len() == 1 {
                IMPORT_ONE
            } else {
                IMPORT_MANY
            };
            open.apply(Command::Batch {
                label: label.to_owned(),
                commands,
            })?;
        }
        Ok(PolarImportResult {
            project: ProjectSummary::of(open),
            imported,
            failures,
        })
    })
}
