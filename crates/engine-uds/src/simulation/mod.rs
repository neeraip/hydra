//! Simulation orchestration (§9–§12): the cascade and its clocks, external
//! forcing, and the session interface. The authoritative specification is
//! `spec.md` in this directory, included in the crate documentation.
//!
//! Currently implemented: the §10.1 routing-period loop over the §6 router
//! with external and sanitary forcing, clock-indexed tidal and series
//! outfall stages, §10.3 event windows, and the §12 session skeleton —
//! load, run, results by identity. Hydrology (§3–§4), transport (§8), and
//! controls (§9) join the cascade as they land.

mod time;

pub mod engine;

pub use engine::{OpenError, Simulation};
