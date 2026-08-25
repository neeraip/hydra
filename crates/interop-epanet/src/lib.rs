//! The EPANET dialect: the predecessor file formats, spoken on the
//! water distribution engine's behalf.
//!
//! This crate is the Tier 1 boundary: everything here binds syntax and
//! interpretation, and nothing here constrains how results are
//! computed. Model bytes are supplied in memory by callers; the one
//! filesystem carve-out is the path-based `.out` reading
//! (`out_reader`), which exists so large results never have to be
//! loaded whole. The engine itself is format-blind.

pub(crate) use hydra_engine_wds as engine_api;

#[path = "dialect/mod.rs"]
pub mod dialect;

pub use dialect::*;
