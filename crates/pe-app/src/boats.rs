//! Independent boat sessions stored in one project container.
pub mod matching;
pub mod tracker_project;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pe_core::{Boat, Command, Project, ProjectId, io};
use serde::Serialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Context, Result};
use crate::projects::ProjectSummary;
use crate::session::OpenProject;

#[derive(Debug, Clone, Serialize, TS)]
pub struct BoatTab {
    pub id: u64,
    pub name: String,
    pub details: BTreeMap<String, String>,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct BoatTabs {
    pub project_id: u64,
    pub name: String,
    pub dirty: bool,
    pub tabs: Vec<BoatTab>,
    pub can_restore: bool,
}

pub fn list(state: &AppState) -> Result<BoatTabs> {
    state.with_session(|session| {
        let root = session.open.as_ref().ok_or(AppError::NoProjectOpen)?;
        Ok(BoatTabs {
            project_id: root.project.id.raw(),
            name: root.project.name.clone(),
            dirty: root.dirty || session.boats.iter().any(|boat| boat.dirty),
            can_restore: !session.removed_boats.is_empty(),
            tabs: std::iter::once(root)
                .chain(session.boats.iter())
                .map(|open| BoatTab {
                    id: open.project.id.raw(),
                    name: if open.project.boat.name.trim().is_empty() {
                        open.project.name.clone()
                    } else {
                        open.project.boat.name.clone()
                    },
                    details: open.project.boat.details.clone(),
                    read_only: open.project.read_only,
                })
                .collect(),
        })
    })
}

#[tauri::command]
pub fn boat_tabs(state: tauri::State<'_, AppState>) -> Result<BoatTabs> {
    list(&state)
}

pub fn add(state: &AppState, name: String) -> Result<ProjectSummary> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::BadOption {
            field: "Boat name",
            value: name,
        });
    }
    state.with_session(|session| {
        let root = session.require_open()?;
        root.ensure_editable()?;
        let mut open = OpenProject::created(Project::new(
            name.clone(),
            Boat {
                name,
                ..Boat::default()
            },
            root.project.created,
        ));
        open.path = root.path.clone();
        root.touch();
        let summary = ProjectSummary::of(&mut open);
        session.boats.push(open);
        Ok(summary)
    })
}

#[tauri::command]
pub fn add_boat(state: tauri::State<'_, AppState>, name: String) -> Result<ProjectSummary> {
    add(&state, name)
}

/// Imports one portable tab archive into the open project's tab list. The
/// input archive is required to be standalone; its imported copy always gets
/// a fresh identity and is locked, regardless of its embedded flag.
pub fn import_tab(state: &AppState, path: &Path) -> Result<ProjectSummary> {
    if !path
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("petab"))
    {
        return Err(AppError::BadOption {
            field: "Tab file",
            value: path.display().to_string(),
        });
    }
    let mut project = io::load(path).doing("read the tab from", path.display())?;
    if !project.boat_tabs.is_empty() {
        return Err(AppError::Core(pe_core::CoreError::Invalid(
            "a .petab file must hold one standalone tab".to_owned(),
        )));
    }
    project.id = ProjectId::fresh();
    project.read_only = true;
    state.with_session(|session| {
        let root = session.require_open()?;
        root.ensure_editable()?;
        let root_path = root.path.clone();
        root.touch();
        let mut open = OpenProject::created(project);
        open.path = root_path;
        let summary = ProjectSummary::of(&mut open);
        session.boats.push(open);
        Ok(summary)
    })
}

#[tauri::command(async)]
pub fn import_boat_tab(state: tauri::State<'_, AppState>, path: String) -> Result<ProjectSummary> {
    import_tab(&state, Path::new(&path))
}

/// Writes the selected tab and all its raw sources and track data as a
/// platform-independent `.petab` archive. The file copy is locked; the tab
/// in the current project remains editable.
pub fn export_tab(state: &AppState, path: &Path) -> Result<String> {
    let project = state.with_session(|session| {
        let open = session.require_open()?;
        let mut project = open.project.clone();
        project.boat_tabs.clear();
        project.read_only = true;
        Ok(project)
    })?;
    let path = with_tab_extension(path.to_path_buf());
    let bytes = io::to_bytes(&project).doing("prepare the tab export for", path.display())?;
    io::write_atomic(&path, &bytes).doing("write the tab export to", path.display())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command(async)]
pub fn export_boat_tab(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    path: String,
) -> Result<String> {
    export_tab(&state.scoped(boat_context), Path::new(&path))
}

fn with_tab_extension(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("petab"))
    {
        return path;
    }
    let mut name = path.into_os_string();
    name.push(".petab");
    PathBuf::from(name)
}

/// Exports every tab as one standalone, locked project. The lock is written
/// into the exported copy only; the open project remains editable.
pub fn export_locked_project(state: &AppState, path: &Path) -> Result<String> {
    let mut project = state.with_session(|session| session.document())?;
    fn lock(project: &mut Project) {
        project.read_only = true;
        for tab in &mut project.boat_tabs {
            lock(tab);
        }
    }
    lock(&mut project);

    let path = with_locked_extension(path.to_path_buf());
    let bytes =
        io::to_bytes(&project).doing("prepare the read-only project export for", path.display())?;
    io::write_atomic(&path, &bytes)
        .doing("write the read-only project export to", path.display())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command(async)]
pub async fn export_locked_boat_project(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || export_locked_project(&state, Path::new(&path)))
        .await
        .map_err(|error| {
            AppError::Internal(format!("The read-only project export failed: {error}"))
        })?
}

fn with_locked_extension(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("pelocked"))
    {
        return path;
    }
    let mut name = path.into_os_string();
    name.push(".pelocked");
    PathBuf::from(name)
}

pub fn rename(state: &AppState, name: String) -> Result<ProjectSummary> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::BadOption {
            field: "Boat name",
            value: name,
        });
    }
    crate::edit::apply(state, |project| {
        Ok((project.boat.name != name).then(|| Command::RenameBoat {
            before: project.boat.name.clone(),
            after: name,
        }))
    })
}

#[tauri::command]
pub fn rename_boat(
    state: tauri::State<'_, AppState>,
    boat_context: u64,
    name: String,
) -> Result<ProjectSummary> {
    rename(&state.scoped(Some(boat_context)), name)
}

/// A deleted tab keeps its data and local undo history until the project closes.
#[derive(Debug)]
pub struct RemovedBoat {
    open: OpenProject,
    index: usize,
    /// The promoted child's document name, before it carried the fleet title.
    promoted_name: Option<String>,
}

pub fn remove(state: &AppState, project_id: u64, boat_id: u64) -> Result<ProjectSummary> {
    state.with_session(|session| {
        let root = session.require_open()?;
        root.ensure_editable()?;
        if root.project.id.raw() != project_id {
            return Err(AppError::NoProjectOpen);
        }
        if session.boats.is_empty() {
            return Err(AppError::Internal(
                "A project must contain at least one boat".into(),
            ));
        }
        let removed = if boat_id == project_id {
            let mut promoted = session.boats.remove(0);
            let promoted_name = if promoted.project.read_only {
                None
            } else {
                Some(std::mem::replace(
                    &mut promoted.project.name,
                    session.require_open()?.project.name.clone(),
                ))
            };
            promoted.path = session.require_open()?.path.clone();
            promoted.saves = session.require_open()?.saves;
            promoted.touch();
            let open = session
                .open
                .replace(promoted)
                .ok_or(AppError::NoProjectOpen)?;
            crate::autosave::forget(state, project_id);
            RemovedBoat {
                open,
                index: 0,
                promoted_name,
            }
        } else {
            let index = session
                .boats
                .iter()
                .position(|boat| boat.project.id.raw() == boat_id)
                .ok_or(AppError::NoProjectOpen)?;
            let open = session.boats.remove(index);
            session.require_open()?.touch();
            RemovedBoat {
                open,
                index: index + 1,
                promoted_name: None,
            }
        };
        session.removed_boats.push(removed);
        // Late results still carry the removed identity and cannot hit a sibling.
        state.env_jobs.cancel_boat(Some(boat_id), None);
        Ok(ProjectSummary::of(session.require_open()?))
    })
}

pub fn restore(state: &AppState, project_id: u64) -> Result<ProjectSummary> {
    state.with_session(|session| {
        let root = session.require_open()?;
        root.ensure_editable()?;
        if root.project.id.raw() != project_id {
            return Err(AppError::NoProjectOpen);
        }
        let mut removed = session
            .removed_boats
            .pop()
            .ok_or_else(|| AppError::Internal("There is no deleted boat to restore".into()))?;
        removed.open.path = session.require_open()?.path.clone();
        removed.open.touch();
        if removed.index == 0 {
            removed.open.project.name = session.require_open()?.project.name.clone();
            removed.open.saves = session.require_open()?.saves;
            let mut promoted = session
                .open
                .replace(removed.open)
                .ok_or(AppError::NoProjectOpen)?;
            if let Some(name) = removed.promoted_name {
                promoted.project.name = name;
            }
            session.boats.insert(0, promoted);
            crate::autosave::forget(state, project_id);
        } else {
            session
                .boats
                .insert((removed.index - 1).min(session.boats.len()), removed.open);
            session.require_open()?.touch();
        }
        Ok(ProjectSummary::of(session.require_open()?))
    })
}

#[tauri::command]
pub fn delete_boat(
    state: tauri::State<'_, AppState>,
    project_id: u64,
    boat_id: u64,
) -> Result<ProjectSummary> {
    remove(&state, project_id, boat_id)
}

#[tauri::command]
pub fn restore_boat(state: tauri::State<'_, AppState>, project_id: u64) -> Result<ProjectSummary> {
    restore(&state, project_id)
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct BoatExportResult {
    pub paths: Vec<String>,
    /// Boat name and reason. Empty boats are reported, never silently exported.
    pub failures: Vec<(String, String)>,
}

fn file_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(100)
        .collect();
    if safe.is_empty() { "boat".into() } else { safe }
}

pub fn export_all(state: &AppState, directory: &Path, format: &str) -> Result<BoatExportResult> {
    export_all_with_matrix(state, directory, format, None)
}

fn export_all_with_matrix(
    state: &AppState,
    directory: &Path,
    format: &str,
    matrix: Option<&crate::matrix_export::MatrixExportOptions>,
) -> Result<BoatExportResult> {
    let extension = match format {
        "expedition" => "txt",
        "adrena" => "pol",
        "csv" => "csv",
        "qtvlm" => "csv",
        "opencpn" => "pol",
        "luckgrib" => "tsv",
        "whirlwind_matrix_polar" => "wmpolar",
        other => {
            return Err(AppError::BadOption {
                field: "Export format",
                value: other.into(),
            });
        }
    };
    let document = state.with_session(|session| session.document())?;
    let mut result = BoatExportResult {
        paths: Vec::new(),
        failures: Vec::new(),
    };
    std::fs::create_dir_all(directory).doing("create export directory", directory.display())?;
    for boat in std::iter::once(&document).chain(document.boat_tabs.iter()) {
        let name = if boat.boat.name.trim().is_empty() {
            &boat.name
        } else {
            &boat.boat.name
        };
        let bytes = match crate::blend::export_bytes_with_matrix(boat, format, None, matrix) {
            Ok(bytes) => bytes,
            Err(error) => {
                result.failures.push((name.clone(), error.to_string()));
                continue;
            }
        };
        let base = format!("{}-{}", file_name(name), boat.id.raw());
        let mut number = 0u64;
        loop {
            let suffix = if number == 0 {
                String::new()
            } else {
                format!("-{number}")
            };
            let path = directory.join(format!("{base}{suffix}.{extension}"));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
                        let _ = std::fs::remove_file(&path);
                        result.failures.push((name.clone(), error.to_string()));
                    } else {
                        result.paths.push(path.to_string_lossy().into_owned());
                    }
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => number += 1,
                Err(error) => {
                    result.failures.push((name.clone(), error.to_string()));
                    break;
                }
            }
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn export_all_polars(
    state: tauri::State<'_, AppState>,
    directory: String,
    format: String,
    matrix: Option<crate::matrix_export::MatrixExportOptions>,
) -> Result<BoatExportResult> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        export_all_with_matrix(&state, Path::new(&directory), &format, matrix.as_ref())
    })
    .await
    .map_err(|error| AppError::Internal(format!("The fleet export task failed: {error}")))?
}
