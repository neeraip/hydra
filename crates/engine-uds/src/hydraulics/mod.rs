//! Hydraulics (§5–§7): cross-section geometry, network flow, and
//! structures. The authoritative specification is `spec.md` in this
//! directory, included in the crate documentation.
//!
//! Currently implemented: the §5 geometry core — the section-property
//! contract, analytic families (§5.2), tabulated families and standard-size
//! catalogues (§5.3–§5.4), custom shapes (§5.5), and the inversions of
//! §5.7. Transects (§5.6) follow.

pub mod inlets;
pub mod routing;
pub mod section;
pub mod tables;
#[cfg(feature = "threads")]
pub(crate) mod team;

/// Standard gravity (m/s²), exact per §2.11.
pub const GRAVITY: f64 = 9.80665;
