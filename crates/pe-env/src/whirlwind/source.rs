//! Fixed archive endpoints. R2/Tigris credentials are embedded at the user's
//! explicit request; they are never sent through IPC, URLs, projects or logs.
use super::Credentials;
use crate::Dataset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    S3,
    R2,
    Tigris,
}

impl Source {
    pub const fn url(self) -> &'static str {
        match self {
            Self::S3 => "https://whirlwind-hindsight.s3.us-east-1.amazonaws.com/hindsight-routing",
            Self::R2 => {
                "https://3d5456ad10ebc32c8a3c259aa84e6c64.r2.cloudflarestorage.com/whirlwind-hindsight/hindsight-routing"
            }
            Self::Tigris => "https://fly.storage.tigris.dev/whirlwind-hindsight/hindsight-routing",
        }
    }

    pub const fn region(self) -> &'static str {
        match self {
            Self::S3 => "us-east-1",
            Self::R2 | Self::Tigris => "auto",
        }
    }

    pub const fn dataset(self) -> Dataset {
        match self {
            Self::S3 => Dataset::WhirlwindHindsight,
            Self::R2 => Dataset::WhirlwindR2,
            Self::Tigris => Dataset::WhirlwindTigris,
        }
    }

    /// S3 is public. Ignore ambient AWS credentials and old credential files.
    pub fn credentials(self) -> Option<Credentials> {
        match self {
            Self::S3 => None,
            Self::R2 => Some(Credentials::fixed(
                "18bf3daf0129f869e08697abf38a6d0e",
                "a4159f3410e7047c23930847dcd6fe678cc5ff5dbeb9b44e0d5e881c03384bac",
            )),
            Self::Tigris => Some(Credentials::fixed(
                "tid_dDVavduKOeciqUWKMFVJuSeNrNtSyxBlSweeILUAKJxFHQdOqH",
                "tsec_RM-E+RA20+u74rIXEKqUYzbmpO5eHb4lwBWyOcYo+QN-cIQQgHkk0AqGbXqJB0H+NL7iP+",
            )),
        }
    }
}
