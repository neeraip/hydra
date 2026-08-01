//! Hydraulics (§5–§7): cross-section geometry, network flow, and
//! structures. The authoritative specification is `spec.md` in this
//! directory, included in the crate documentation.
//!
//! Currently implemented: the §5 geometry core — the section-property
//! contract, analytic families (§5.2), custom shapes (§5.5), and the
//! inversions of §5.7. The tabulated families (§5.3), standard-size
//! catalogues (§5.4), and transects (§5.6) follow.

pub mod section;

/// Standard gravity (m/s²), exact per §2.11.
pub const GRAVITY: f64 = 9.80665;
