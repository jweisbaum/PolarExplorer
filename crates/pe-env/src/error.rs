//! Error taxonomy for `pe-env`. Adapted from VectorEffects' `ve-zarr`.

use thiserror::Error;

/// Errors produced while reading a reanalysis archive.
#[derive(Debug, Error)]
pub enum EnvError {
    /// The store URL could not be parsed or the store could not be opened.
    #[error("could not open the archive: {0}")]
    Open(String),

    /// The store is not shaped the way this crate requires.
    #[error("unexpected archive layout: {0}")]
    Layout(String),

    /// The requested time or place does not lie inside the dataset.
    #[error("{0}")]
    OutOfRange(String),

    /// The on-disk chunk cache could not be read or written.
    #[error("chunk cache: {0}")]
    Cache(String),

    /// The work was cancelled by the user.
    #[error("cancelled")]
    Cancelled,
}

/// Convenience alias for results in this crate.
pub type Result<T> = std::result::Result<T, EnvError>;
