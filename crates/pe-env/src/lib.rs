//! Reanalysis environment at track positions.
//!
//! The Whirlwind Hindsight sharded Zarr v3 reader (`whirlwind/`) and
//! space-time sampling (spec.md 7.5). Only the inner chunks that hold a
//! track's stencil are downloaded, and they are kept in memory for the
//! session (`memory.rs`) and, validated and compressed, in a bounded disk
//! cache. Projects save only interpolated sample values, with their dataset
//! name and version (invariant 3).
//!
//! One of the two crates allowed to use the network (invariant 4), and only
//! for the archive hosts that `tools/check-offline.sh` allows. Nothing here
//! runs unless the user asked for an import or a refetch.

pub mod dataset;
pub mod error;
pub mod grid;
pub mod memory;
pub mod net;
pub mod sampler;
pub mod store;
pub mod time;
pub mod whirlwind;

pub use dataset::Dataset;
pub use error::{EnvError, Result};
pub use memory::BlockCache;
pub use sampler::{
    EnvPoint, Estimate, Interval, Options, Parts, Point, Provider, Reanalysis, Vector, Waves,
};
