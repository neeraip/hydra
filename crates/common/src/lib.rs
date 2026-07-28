#![doc = include_str!("spec.md")]

/// The crate version, taken from `Cargo.toml` at compile time.
pub const HYDRA_COMMON_VERSION: &str = env!("CARGO_PKG_VERSION");

mod identity;
mod report;

pub use identity::{engine_by_key, EngineDescriptor, UnknownEngineError, ENGINES};
pub use report::{
    BlockDescriptor, BlockError, Column, Fragment, FragmentItem, KeyValue, Table, Value, ValueKind,
};
