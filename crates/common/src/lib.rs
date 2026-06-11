//! `hydra-common` — engine-agnostic geographic primitives.
//!
//! This crate is intentionally minimal. It contains only the two types that are
//! genuinely engine-agnostic: [`Coordinate`] and [`Crs`]. All domain types
//! (network data model, solver state, session API, etc.) belong in
//! `hydra-engine`.
//!
//! # Non-goals
//!
//! - No solver logic, session logic, data model types, parsers, or writers.
//! - No network or filesystem I/O.
//! - No new types should be added here. Hydra is WD-only; `hydra-common` is
//!   intentionally frozen.

/// A geographic coordinate.
///
/// `x` is longitude in decimal degrees, or easting in metres, depending on the
/// CRS. `y` is latitude in decimal degrees, or northing in metres.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Coordinate {
    /// Longitude (degrees east) or easting (metres), depending on the CRS.
    pub x: f64,
    /// Latitude (degrees north) or northing (metres), depending on the CRS.
    pub y: f64,
}

/// Coordinate Reference System identifier.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Crs {
    /// Authority-prefixed CRS identifier, e.g. `"EPSG:4326"` or `"EPSG:32617"`.
    pub id: String,
}
