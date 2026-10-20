//! Error taxonomy for `pe-env`. Adapted from VectorEffects' `ve-zarr`.

use thiserror::Error;

/// Errors produced while reading a reanalysis archive.
#[derive(Debug, Error)]
pub enum EnvError {
    /// The store URL could not be parsed or the store could not be opened.
    #[error("could not open the archive: {0}")]
    Open(String),

    /// The archive refused the read for want of valid credentials: a 401 or
    /// 403, or a 400 whose body names an expired or unknown key or a bad
    /// signature. Its own variant so the application can say "Scan to
    /// refresh the credentials" without reading the message.
    #[error("the archive refused the read of {key} ({status})")]
    Auth {
        /// The HTTP status the archive answered.
        status: u16,
        /// The object key that was refused.
        key: String,
    },

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
