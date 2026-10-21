//! Application-level error type and its IPC wire form.
//!
//! Library crates return typed errors (`thiserror`); this is where they are
//! collapsed for the frontend. The wire form keeps a machine-readable `kind`
//! alongside the message so the UI can branch without parsing English.
//!
//! **A message names three things** (spec.md 1.4): what the application was
//! trying to do, what it was doing it to, and what went wrong. A library error
//! is only ever the third — `No such file or directory (os error 2)` is true
//! and useless — so the first two are attached at the call site with
//! [`Context::doing`], which is the only place that knows them.
//!
//! Library crates add their own variants here, each with its own `kind`, as
//! they gain error types.

use serde::Serialize;
use thiserror::Error;
use ts_rs::TS;

/// Every error that can reach the IPC boundary.
#[derive(Debug, Error)]
pub enum AppError {
    /// Filesystem access failed.
    #[error("A file could not be read or written: {0}")]
    Io(#[from] std::io::Error),

    /// The document model or the project file refused.
    #[error("{0}")]
    Core(#[from] pe_core::CoreError),

    /// An operation needed an open project and there is none.
    #[error("No project is open.")]
    NoProjectOpen,

    /// An operation would change a read-only tab or project.
    #[error("This tab or project is read only and cannot be changed.")]
    ReadOnlyTab,

    /// Save was asked of a project that has never been saved; the frontend
    /// answers with Save As.
    #[error("This project has not been saved yet, so it has no file to save to.")]
    ProjectNeverSaved,

    /// Replacing or closing the open project would have discarded unsaved
    /// changes (spec.md 3.3).
    ///
    /// The prompt that offers to save lives in the frontend, but the refusal
    /// lives here, so no caller can lose work by forgetting to ask. The
    /// caller asks, then repeats the call with `discard_unsaved = true`.
    #[error("\"{name}\" has unsaved changes.")]
    UnsavedChanges {
        /// The open project's name, for the message.
        name: String,
    },

    /// An argument was not one the command accepts.
    #[error("{field} cannot be {value:?}.")]
    BadOption {
        /// Which argument.
        field: &'static str,
        /// What was received.
        value: String,
    },

    /// Add was asked for an ORC certificate the project already holds
    /// (spec.md 5.3). The frontend asks, then repeats the call allowing it.
    #[error("The project already holds the certificate of {name}.")]
    DuplicateCertificate {
        /// The boat, for the message.
        name: String,
    },

    /// A named step failed, on a named thing.
    ///
    /// The general-purpose contextual error: `doing` is the action in the
    /// infinitive without its "to" ("save the project", "read the polar"),
    /// `what` is the file or object it acted on, and `why` is whatever the
    /// underlying failure said. Kept as three fields rather than one formatted
    /// string so the wording stays in one place.
    #[error("Could not {doing} {what}: {why}")]
    Doing {
        /// The action, e.g. `"write"`.
        doing: &'static str,
        /// What it acted on, e.g. a file name.
        what: String,
        /// What the underlying failure said.
        why: String,
    },

    /// An export was refused (spec.md 6, 12): a grid that would not read
    /// back, or nothing to write. `code` is `pe_polar::ExportProblem::code`.
    #[error("The polar cannot be exported: {message}.")]
    Export {
        /// The problem's stable code.
        code: &'static str,
        /// What it was, in English.
        message: String,
    },

    /// A fetch or a GRIB export was asked for with no reanalysis archive to
    /// read (spec.md 7.5): none chosen, or its remembered location gone.
    /// `reason` names the setting to go to.
    #[error("{reason}")]
    NoReanalysisArchive {
        /// The whole message.
        reason: String,
    },

    /// A tracker download or decode failed (spec.md 7.2).
    #[error("{0}")]
    Tracker(#[from] pe_trackers::TrackerError),

    /// A failure with no more specific classification.
    #[error("{0}")]
    Internal(String),
}

/// Attaches what was being done, and to what, to a failure that knows neither.
///
/// `std::io::Error` says a file was not found and never which file; a decoder
/// says a byte was unexpected and never which import it came from. The caller
/// is the only place both halves are known, so this is where they meet.
pub trait Context<T> {
    /// Names the action and its subject. `doing` reads as an infinitive
    /// without "to", so the message comes out as "Could not `doing` `what`".
    fn doing(self, doing: &'static str, what: impl std::fmt::Display) -> Result<T>;
}

impl<T, E: std::fmt::Display> Context<T> for std::result::Result<T, E> {
    fn doing(self, doing: &'static str, what: impl std::fmt::Display) -> Result<T> {
        self.map_err(|why| AppError::Doing {
            doing,
            what: what.to_string(),
            why: why.to_string(),
        })
    }
}

impl AppError {
    /// A stable, machine-readable discriminant for the frontend.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Core(pe_core::CoreError::SchemaTooNew { .. }) => "schema-too-new",
            Self::Core(_) => "core",
            Self::NoProjectOpen => "no-project",
            Self::ReadOnlyTab => "read-only-tab",
            Self::ProjectNeverSaved => "never-saved",
            Self::UnsavedChanges { .. } => "unsaved-changes",
            Self::BadOption { .. } => "bad-option",
            Self::DuplicateCertificate { .. } => "orc-duplicate",
            Self::Doing { .. } => "doing",
            Self::Export { .. } => "export-refused",
            Self::NoReanalysisArchive { .. } => "no-reanalysis-archive",
            Self::Tracker(e) => match e {
                pe_trackers::TrackerError::NotAnEvent { .. } => "tracker-address",
                pe_trackers::TrackerError::Unavailable { .. } => "tracker-unavailable",
                pe_trackers::TrackerError::NoSuchEvent { .. } => "tracker-no-event",
                pe_trackers::TrackerError::Cancelled => "cancelled",
                pe_trackers::TrackerError::Decode { .. } => "tracker-decode",
                pe_trackers::TrackerError::Unsupported { .. } => "tracker-unsupported",
                pe_trackers::TrackerError::Legacy { .. } => "tracker-legacy",
                pe_trackers::TrackerError::Network(_) => "tracker-network",
                pe_trackers::TrackerError::Http { .. } => "tracker-network",
            },
            Self::Internal(_) => "internal",
        }
    }
}

/// The shape an [`AppError`] takes when it crosses IPC.
#[derive(Debug, Serialize, TS)]
#[ts(export_to = "AppErrorPayload.ts")]
pub struct AppErrorPayload {
    /// Stable discriminant, e.g. `"io"`.
    pub kind: String,
    /// Human-readable message. Not intended for programmatic branching.
    pub message: String,
}

impl Serialize for AppError {
    // Fully qualified: the `Result` alias below shadows `std::result::Result`.
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        AppErrorPayload {
            kind: self.kind().to_owned(),
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}

/// Convenience alias for command results.
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_with_kind_and_message() {
        let err = AppError::Internal("boom".to_owned());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "kind": "internal", "message": "boom" })
        );
    }

    /// The three things a message names: the action, the subject, and what
    /// went wrong. A bare `io::Error` says only the third.
    #[test]
    fn context_names_what_was_being_done_and_to_what() {
        let bare: std::result::Result<(), std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "permission denied",
        ));
        let message = bare
            .doing("save the project to", "/tmp/a.wpsproj")
            .expect_err("the failure carries through")
            .to_string();

        assert_eq!(
            message,
            "Could not save the project to /tmp/a.wpsproj: permission denied"
        );
    }

    /// A context error is still a serialisable error with a stable kind, so
    /// the frontend branches on it the way it branches on every other one.
    #[test]
    fn context_crosses_ipc_like_any_other() {
        let err: std::result::Result<(), &str> = Err("no such file");
        let err = err.doing("read", "x").expect_err("an error");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "doing");
        assert_eq!(json["message"], "Could not read x: no such file");
    }

    #[test]
    fn io_errors_keep_their_own_kind() {
        let err: AppError = std::io::Error::other("disk full").into();
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "io");
        assert_eq!(
            json["message"],
            "A file could not be read or written: disk full"
        );
    }
}
