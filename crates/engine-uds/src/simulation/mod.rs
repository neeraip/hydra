//! Simulation orchestration (§9–§12): the cascade and its clocks, external
//! forcing, and the session interface. The authoritative specification is
//! `spec.md` in this directory, included in the crate documentation.
//!
//! Currently implemented: the §10.1 routing-period loop over the §6 router
//! with external and sanitary forcing, clock-indexed tidal and series
//! outfall stages, §10.3 event windows, operational control (§9.1–§9.2:
//! prioritised rules, PID, named variables and expressions), the §9.3
//! expression language, and the §12 session skeleton — load, run, results
//! by identity. Hydrology (§3–§4) drives the cascade; transport (§8)
//! joins as it lands.

pub mod controls;
pub mod expression;
pub mod time;

pub mod engine;

pub use engine::{OpenError, Simulation};
pub use expression::{ExprError, Expression};
