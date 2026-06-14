// timestep — §5 of crates/engine-wds/src/simulation/spec.md
//
// Selects the adaptive hydraulic time step Δtₕ (§5.2) and advances tank
// levels between hydraulic solves (§5.3).
//
// Parallelism (∥): per-tank time-to-limit calculations and tank level updates
// are independent and may be computed concurrently.

mod schedule;
mod tank;

pub(crate) use schedule::{adaptive_timestep, control_timestep};
pub(crate) use tank::update_tank_levels;
