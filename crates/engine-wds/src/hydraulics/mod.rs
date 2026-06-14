#![doc = include_str!("spec.md")]

/// Semver version of the hydraulics engine, taken from `Cargo.toml` at compile time.
pub const HYDRA_HYDRAULICS_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod solve_tests;

mod assembly;
mod demand;
mod diagnostics;
mod headloss;
mod pump;
mod shared;
mod solve;
mod sparse;
mod sparse_order;
mod valve;

use demand::{
    apply_emitter_coeffs, apply_favad_leakage_coeffs, apply_pda_demand_coeffs, leakage_converged,
    update_emitter_flows, update_leakage_flows, update_pda_demand_flows,
};
pub use headloss::G_DW;
use headloss::{pipe_resistance, pipe_total_hg, HW_EXP};
use valve::{bad_valve, check_link_status, check_valve_status};

pub use shared::{HydraulicError, SolveResult};
pub use solve::{build_solver_context, solve_hydraulic_step, SolverContext};
pub(crate) use sparse::SparseSolver;
