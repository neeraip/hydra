//! Hydrology (§3–§4): the surface water balance and its subsurface
//! companions. The authoritative specification is `spec.md` in this
//! directory, included in the crate documentation.
//!
//! Currently implemented: gage precipitation, the three-sub-area
//! nonlinear-reservoir parcel with the §3.5 embedded-pair integrator, the
//! five §3.3 infiltration relations with their recovery models, internal
//! re-routing and parcel run-on. Control measures (§3.4), groundwater
//! (§4.1), snow (§4.2), and RDII (§4.3) join as they land.

pub mod groundwater;
pub mod infiltration;
pub mod lid;
pub mod rdii;
pub mod runoff;
pub mod snow;
