#![doc = include_str!("spec.md")]
//! The SWMM dialect (specification §14): the predecessor file formats,
//! spoken on the urban drainage engine's behalf.
//!
//! This crate is the Tier 1 boundary: everything here binds syntax and
//! interpretation, and nothing here constrains how results are computed.
//! Model bytes are supplied in memory by callers; the one filesystem
//! carve-out is the path-based streaming of `.out` result files
//! (`out_reader`), which exists so large results never have to be
//! loaded whole. The engine itself is format-blind: models enter it as
//! typed data and results leave as typed streams, and this crate is
//! where the text becomes data and the data becomes text.

pub(crate) use hydra_engine_uds as engine_api;

#[path = "dialect/mod.rs"]
pub mod dialect;

pub use dialect::*;
