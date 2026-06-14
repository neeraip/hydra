use crate::{Network, RuntimeEstimate};

const LOW_EFFORT_THRESHOLD_MS: f64 = 600.0;
const MEDIUM_EFFORT_THRESHOLD_MS: f64 = 3_000.0;

/// Estimate simulation runtime from a fully loaded network.
///
/// Advisory only — does not influence time-step selection, convergence
/// behaviour, or any simulation result. For identical network inputs the
/// output is deterministic.
///
/// Cost model increases monotonically with: hydraulic step count
/// (duration / hyd_step), network size (nodes + links), mesh density, and
/// quality step count when quality is enabled. Larger/longer simulations
/// will not systematically receive lower estimates than smaller/shorter ones.
pub fn estimate_simulation_runtime(network: &Network) -> RuntimeEstimate {
    estimate_simulation_runtime_from_summary(
        network.nodes.len(),
        network.links.len(),
        network.options.duration,
        network.options.hyd_step,
        network.options.qual_step,
        network.options.quality_mode != crate::QualityMode::None,
    )
}

/// Classify a predicted runtime (milliseconds) into effort buckets.
pub fn classify_simulation_runtime_millis(predicted_millis: f64) -> RuntimeEstimate {
    if predicted_millis < LOW_EFFORT_THRESHOLD_MS {
        RuntimeEstimate::Low
    } else if predicted_millis < MEDIUM_EFFORT_THRESHOLD_MS {
        RuntimeEstimate::Medium
    } else {
        RuntimeEstimate::High
    }
}

/// Estimate simulation runtime in milliseconds from summary metadata.
///
/// Inputs: node count, link count, simulation duration, hydraulic time step,
/// quality time step, and whether quality simulation is enabled. Does not
/// depend on mutable post-run state — the estimate is stable before and
/// after executing a simulation on the same network definition.
pub fn estimate_simulation_runtime_millis_from_summary(
    node_count: usize,
    link_count: usize,
    duration_seconds: f64,
    hydraulic_timestep_seconds: f64,
    quality_timestep_seconds: f64,
    has_quality: bool,
) -> f64 {
    let hydraulic_steps =
        ((duration_seconds.max(0.0) / hydraulic_timestep_seconds.max(1.0)).floor() + 1.0).max(1.0);
    let quality_steps =
        ((duration_seconds.max(0.0) / quality_timestep_seconds.max(1.0)).floor() + 1.0).max(1.0);
    let nodes = node_count.max(1) as f64;
    let links = link_count.max(1) as f64;

    // Static proxy terms:
    // 1) setup scales with graph size and mesh density,
    // 2) hydraulic cost scales with hydraulic step count,
    // 3) quality cost scales with quality step count when enabled.
    let mesh_factor = (links / nodes).clamp(0.75, 2.5);
    let setup_work_ms = 1.2e-6 * (nodes * nodes) * mesh_factor;

    // Warm-step timings were fit from Criterion solve benchmarks and then
    // translated into a per-step linear proxy on (nodes + links).
    let hydraulic_step_us = (-2.1 + 0.0266 * (nodes + links)).max(25.0);
    let hydraulic_work_ms = hydraulic_steps * (hydraulic_step_us / 1_000.0);

    let quality_work_ms = if has_quality {
        // Keep quality as an additive static term; this preserves monotonicity
        // with respect to quality step count and topology size.
        quality_steps * (0.012 * (nodes + links) / 1_000.0)
    } else {
        0.0
    };

    12.0 + setup_work_ms + hydraulic_work_ms + quality_work_ms
}

/// Estimate simulation runtime from summary metadata.
pub fn estimate_simulation_runtime_from_summary(
    node_count: usize,
    link_count: usize,
    duration_seconds: f64,
    hydraulic_timestep_seconds: f64,
    quality_timestep_seconds: f64,
    has_quality: bool,
) -> RuntimeEstimate {
    classify_simulation_runtime_millis(estimate_simulation_runtime_millis_from_summary(
        node_count,
        link_count,
        duration_seconds,
        hydraulic_timestep_seconds,
        quality_timestep_seconds,
        has_quality,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        classify_simulation_runtime_millis, estimate_simulation_runtime_from_summary,
        estimate_simulation_runtime_millis_from_summary,
    };

    #[test]
    fn estimate_increases_with_network_size() {
        let small =
            estimate_simulation_runtime_from_summary(200, 250, 86_400.0, 3_600.0, 300.0, false);
        let large =
            estimate_simulation_runtime_from_summary(2_000, 2_500, 86_400.0, 3_600.0, 300.0, false);
        assert!(large >= small);
    }

    #[test]
    fn estimate_increases_with_duration() {
        let short =
            estimate_simulation_runtime_from_summary(500, 600, 3_600.0, 3_600.0, 300.0, false);
        let long =
            estimate_simulation_runtime_from_summary(500, 600, 86_400.0, 3_600.0, 300.0, false);
        assert!(long >= short);
    }

    #[test]
    fn quality_mode_increases_estimate() {
        let hyd_only =
            estimate_simulation_runtime_from_summary(800, 900, 86_400.0, 3_600.0, 300.0, false);
        let with_quality =
            estimate_simulation_runtime_from_summary(800, 900, 86_400.0, 3_600.0, 300.0, true);
        assert!(with_quality >= hyd_only);
    }

    #[test]
    fn smaller_quality_timestep_increases_estimate_when_quality_enabled() {
        let coarse_quality =
            estimate_simulation_runtime_from_summary(1_200, 1_500, 86_400.0, 3_600.0, 900.0, true);
        let fine_quality =
            estimate_simulation_runtime_from_summary(1_200, 1_500, 86_400.0, 3_600.0, 60.0, true);
        assert!(fine_quality >= coarse_quality);
    }

    #[test]
    fn very_large_case_maps_to_high_effort() {
        let estimate =
            estimate_simulation_runtime_from_summary(80_000, 90_000, 604_800.0, 300.0, 30.0, true);
        assert_eq!(estimate, crate::RuntimeEstimate::High);
    }

    #[test]
    fn predicted_millis_increase_with_network_size() {
        let small = estimate_simulation_runtime_millis_from_summary(
            200, 250, 86_400.0, 3_600.0, 300.0, false,
        );
        let large = estimate_simulation_runtime_millis_from_summary(
            2_000, 2_500, 86_400.0, 3_600.0, 300.0, false,
        );
        assert!(large >= small);
    }

    #[test]
    fn classifier_thresholds_are_stable() {
        assert_eq!(
            classify_simulation_runtime_millis(100.0),
            crate::RuntimeEstimate::Low
        );
        assert_eq!(
            classify_simulation_runtime_millis(1_000.0),
            crate::RuntimeEstimate::Medium
        );
        assert_eq!(
            classify_simulation_runtime_millis(10_000.0),
            crate::RuntimeEstimate::High
        );
    }
}
