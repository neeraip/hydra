use std::{sync::OnceLock, time::Duration};

pub(super) fn solve_timing_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("HYDRA_SOLVE_TIMING").is_some())
}

#[derive(Default)]
pub(super) struct SparsePhaseTimings {
    pub(super) reset: Duration,
    pub(super) factor: Duration,
    pub(super) forward: Duration,
    pub(super) backward: Duration,
}

#[derive(Default)]
pub(super) struct SolvePhaseTimings {
    pub(super) setup: Duration,
    pub(super) demand: Duration,
    pub(super) mapping: Duration,
    pub(super) py: Duration,
    pub(super) assembly: Duration,
    pub(super) linear_solve: Duration,
    pub(super) sparse_reset: Duration,
    pub(super) sparse_factor: Duration,
    pub(super) sparse_forward: Duration,
    pub(super) sparse_backward: Duration,
    pub(super) head_extract: Duration,
    pub(super) updates: Duration,
    pub(super) status_checks: Duration,
    pub(super) post: Duration,
    pub(super) iterations: usize,
}

impl SolvePhaseTimings {
    pub(super) fn enabled() -> bool {
        solve_timing_enabled()
    }

    pub(super) fn emit(&self, step_time: f64, junctions: usize, links: usize) {
        if self.iterations == 0 {
            return;
        }

        let total = self.setup
            + self.demand
            + self.mapping
            + self.py
            + self.assembly
            + self.linear_solve
            + self.head_extract
            + self.updates
            + self.status_checks
            + self.post;
        let total_ms = total.as_secs_f64() * 1000.0;
        let iter_scale = 1.0 / self.iterations as f64;

        eprintln!(
            "[hydra:solve] t={step_time:.0}s iter={} nodes={} links={} total={total_ms:.2}ms \
setup={:.2} demand={:.2} map={:.2} py={:.2} assembly={:.2} solve={:.2} sreset={:.2} chol={:.2} fwd={:.2} bwd={:.2} heads={:.2} update={:.2} status={:.2} post={:.2}",
            self.iterations,
            junctions,
            links,
            self.setup.as_secs_f64() * 1000.0,
            self.demand.as_secs_f64() * 1000.0,
            self.mapping.as_secs_f64() * 1000.0,
            self.py.as_secs_f64() * 1000.0 * iter_scale,
            self.assembly.as_secs_f64() * 1000.0 * iter_scale,
            self.linear_solve.as_secs_f64() * 1000.0 * iter_scale,
            self.sparse_reset.as_secs_f64() * 1000.0 * iter_scale,
            self.sparse_factor.as_secs_f64() * 1000.0 * iter_scale,
            self.sparse_forward.as_secs_f64() * 1000.0 * iter_scale,
            self.sparse_backward.as_secs_f64() * 1000.0 * iter_scale,
            self.head_extract.as_secs_f64() * 1000.0 * iter_scale,
            self.updates.as_secs_f64() * 1000.0 * iter_scale,
            self.status_checks.as_secs_f64() * 1000.0 * iter_scale,
            self.post.as_secs_f64() * 1000.0,
        );
    }
}
