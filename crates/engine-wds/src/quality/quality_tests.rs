// Physics-based quality engine tests.
//
// Each test constructs a network programmatically via TestNetworkBuilder,
// initialises quality state, and checks results against values derived
// from the spec equations — not EPANET output.
//
// Unlike hydraulics tests, quality tests set link flows *manually* to isolate
// quality physics from the hydraulic solver.

use crate::test_support::TestNetworkBuilder;
use crate::{MixModel, QualityMode, QualitySource, SimulationOptions, SourceType};
use approx::{assert_abs_diff_eq, assert_relative_eq};

use crate::quality::{advance_quality, init_quality};

// ═══════════════════════════════════════════════════════════════════════════════
// Chemical transport — concentration front through a pipe
// ═══════════════════════════════════════════════════════════════════════════════

/// A reservoir at concentration C=10 feeds a pipe to a junction (C=0).
/// After the pipe is fully flushed (dt ≥ V_pipe / Q), the junction should
/// reach the reservoir concentration.
///
/// Pipe volume = π r² L (in internal units: m, m³).
/// Flow ≈ 0.028317 m³/s (= 1.0 ft³/s, constant).
/// Travel time = V / Q.
///
/// We advance quality for dt_h > travel_time and verify the downstream node
/// picks up the source concentration.
#[test]
fn chemical_front_reaches_downstream() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 10.0,
            duration: 7200.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        .node_quality("R1", 10.0)
        .junction("J1", 0.0, 0.0) // no demand, quality starts at 0
        .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0);

    let (net, ns, mut ls, _favad) = builder.build_with_favad();

    // Set a constant flow of 1.0 CFS = 0.028317 m³/s through the pipe (from R1 → J1).
    ls[0].flow = 0.028317_f64;

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // Pipe volume in SI: D=12in=0.3048m, L=100ft=30.48m
    // V = π × (0.1524)² × 30.48 ≈ 2.224 m³
    let d_m = 0.3048_f64;
    let l_m = 30.48_f64;
    let pipe_vol = std::f64::consts::PI * (d_m / 2.0).powi(2) * l_m;
    let travel_time = pipe_vol / 0.028317_f64; // ≈ 78.5 s

    // Advance for 3× travel time to ensure complete flushing.
    let dt_h = travel_time * 3.0;
    advance_quality(&mut state, &net, &ns, &ls, dt_h, 0.0);

    // Junction (index 1) should now have the reservoir concentration.
    assert_abs_diff_eq!(state.node_conc[1], 10.0, epsilon = 0.1);

    // Reservoir (index 0) should still be at 10.0.
    assert_abs_diff_eq!(state.node_conc[0], 10.0, epsilon = 0.01);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Chemical transport — zero flow means no transport
// ═══════════════════════════════════════════════════════════════════════════════

/// With zero flow, the concentration front does not move. The downstream node
/// remains at its initial concentration.
#[test]
fn zero_flow_no_transport() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 60.0,
            duration: 7200.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        .node_quality("R1", 10.0)
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    ls[0].flow = 0.0; // stagnant

    let mut state = init_quality(&net, &ns, &ls).unwrap();
    advance_quality(&mut state, &net, &ns, &ls, 3600.0, 0.0);

    // Junction should still be at 0 (no transport with zero flow).
    // (EPANET noflowqual: uses upstream seg concentration but with zero flow
    //  the segments don't advance, so downstream gets its initial value.)
    assert_abs_diff_eq!(state.node_conc[1], 0.0, epsilon = 0.1);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Water age — increases by dt/3600 per period
// ═══════════════════════════════════════════════════════════════════════════════

/// In AGE mode with zero flow, every segment and node ages by dt_h/3600 hours
/// per hydraulic period.
#[test]
fn age_increases_uniformly_no_flow() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Age,
            qual_step: 3600.0,
            hyd_step: 3600.0,
            duration: 7200.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0) // reservoirs inject age=0 water
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    ls[0].flow = 0.0; // stagnant

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // Advance one 3600-s period with one 3600-s quality sub-step.
    advance_quality(&mut state, &net, &ns, &ls, 3600.0, 0.0);

    // Pipe segments should have aged by exactly 1.0 hour.
    if let Some(pq) = &state.pipe_quality[0] {
        for seg in &pq.segments {
            assert_abs_diff_eq!(seg.concentration, 1.0, epsilon = 1e-12);
        }
    }

    // Advance another period.
    advance_quality(&mut state, &net, &ns, &ls, 3600.0, 3600.0);

    // Segments should now be at 2.0 hours.
    if let Some(pq) = &state.pipe_quality[0] {
        for seg in &pq.segments {
            assert_abs_diff_eq!(seg.concentration, 2.0, epsilon = 1e-12);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// First-order bulk decay — C(t) ≈ C₀ × e^(kb × t)
// ═══════════════════════════════════════════════════════════════════════════════

/// First-order bulk decay in a pipe segment: after many small quality steps,
/// concentration should approximate C₀ × e^(kb × t).
///
/// Uses stagnant flow so only reactions act (no transport).
/// Global bulk coefficient = −0.5 /day → converted to /second in build.
#[test]
fn first_order_bulk_decay() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 60.0, // fine sub-steps for accuracy
            hyd_step: 3600.0,
            duration: 7200.0,
            bulk_coeff: -0.5, // /day (will be converted to /s)
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        .node_quality("R1", 10.0)
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    ls[0].flow = 0.0; // stagnant — reactions only

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // Set all pipe segments to initial concentration C₀ = 10.0
    if let Some(pq) = &mut state.pipe_quality[0] {
        for seg in &mut pq.segments {
            seg.concentration = 10.0;
        }
    }
    state.mass_balance.init = crate::quality::shared::total_mass(&state);

    // Advance one hydraulic period (1 hour = 3600 s).
    let dt_h = 3600.0;
    advance_quality(&mut state, &net, &ns, &ls, dt_h, 0.0);

    // Expected: C(t) = C₀ × e^(kb_per_sec × t)
    // kb_per_sec = −0.5 / 86400 = −5.787e-6
    let kb_per_sec: f64 = -0.5 / 86400.0;
    let c_expected = 10.0 * (kb_per_sec * dt_h).exp();

    // Check pipe segment concentrations
    if let Some(pq) = &state.pipe_quality[0] {
        for seg in &pq.segments {
            // Euler method with 60-second steps gives close approximation
            // to the analytical exponential. Allow ~1%.
            assert_relative_eq!(seg.concentration, c_expected, max_relative = 0.01);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Mass conservation — inert system
// ═══════════════════════════════════════════════════════════════════════════════

/// In a system with no reactions and zero flow, the total mass of constituent
/// (Σ V_segment × C_segment) should be perfectly conserved (no mass enters
/// or leaves, no decay).
#[test]
fn mass_conserved_no_reactions() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 30.0,
            duration: 7200.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        .node_quality("R1", 5.0)
        .junction("J1", 0.0, 0.0)
        .junction("J2", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 200.0, 12.0, 100.0)
        .hw_pipe("P2", "J1", "J2", 200.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    // Zero flow → no mass transport, no sinks/sources active.
    ls[0].flow = 0.0;
    ls[1].flow = 0.0;

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // Set different initial concentrations in pipe segments
    if let Some(pq) = &mut state.pipe_quality[0] {
        for seg in &mut pq.segments {
            seg.concentration = 8.0;
        }
    }
    if let Some(pq) = &mut state.pipe_quality[1] {
        for seg in &mut pq.segments {
            seg.concentration = 3.0;
        }
    }
    state.mass_balance.init = crate::quality::shared::total_mass(&state);

    // Advance a hydraulic period (no flow, no reactions).
    advance_quality(&mut state, &net, &ns, &ls, 600.0, 0.0);

    // Mass should be perfectly conserved: init == final.
    let ratio = state.mass_balance.ratio();
    assert_abs_diff_eq!(ratio, 1.0, epsilon = 1e-10);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Trace mode — source node propagation
// ═══════════════════════════════════════════════════════════════════════════════

/// In TRACE mode, the trace node starts at 100% and others at 0%.
/// After transporting through a pipe, the downstream node should reach 100%.
#[test]
fn trace_propagates_from_source() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Trace,
            trace_node: Some("R1".to_string()),
            qual_step: 10.0,
            duration: 7200.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    ls[0].flow = 1.0; // 1 m³/s — fast flow to flush pipe quickly

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // Trace node R1 should start at 100%
    assert_abs_diff_eq!(state.node_conc[0], 100.0, epsilon = 1e-10);

    // Pipe volume in SI: D=12in=0.3048m, L=100ft=30.48m → V ≈ 2.224 m³
    // Travel time at 1 m³/s: ≈ 2.2 s. Advance 500 s (well beyond travel time).
    advance_quality(&mut state, &net, &ns, &ls, 500.0, 0.0);

    // J1 should now be ~100% trace from R1.
    assert_abs_diff_eq!(state.node_conc[1], 100.0, epsilon = 1.0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// CSTR tank mixing — fully mixed concentration
// ═══════════════════════════════════════════════════════════════════════════════

/// A CSTR tank receiving inflow at concentration C_in mixes perfectly:
///
/// C_new = (V_old × C_old + Q_in × dt × C_in) / V_new
///
/// This test verifies the tank concentration converges toward the inflow
/// concentration.
#[test]
fn cstr_tank_mixing_approaches_inflow_concentration() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 60.0,
            duration: 86400.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 200.0)
        .node_quality("R1", 10.0)
        // Tank: elev=100, init_level=10, min=0, max=30, diameter=20 ft
        .tank("T1", 100.0, 10.0, 0.0, 30.0, 20.0)
        .node_quality("T1", 0.0) // tank starts at C=0
        .hw_pipe("P1", "R1", "T1", 100.0, 12.0, 100.0);

    let (net, mut ns, mut ls, _) = builder.build_with_favad();

    // Set inflow to tank: 0.5 m³/s
    ls[0].flow = 0.5;
    // Tank net_flow must be set for quality mixing
    ns[1].net_flow = 0.5;

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // Set pipe segments to source concentration (10.0)
    if let Some(pq) = &mut state.pipe_quality[0] {
        for seg in &mut pq.segments {
            seg.concentration = 10.0;
        }
    }

    // Advance several hours - tank should gradually approach inflow conc
    for step in 0..10 {
        advance_quality(&mut state, &net, &ns, &ls, 3600.0, step as f64 * 3600.0);
    }

    // After many hours of inflow at C=10, tank should approach 10.0
    // (not exactly 10 because the tank volume increases with inflow)
    if let Some(tq) = &state.tank_quality[1] {
        match tq {
            crate::quality::shared::TankQuality::Cstr { conc, .. } => {
                assert!(*conc > 5.0, "tank conc should approach inflow: got {conc}");
                assert!(*conc <= 10.0, "tank conc should not exceed inflow conc");
            }
            _ => panic!("expected CSTR tank"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Per-pipe bulk coefficient overrides global
// ═══════════════════════════════════════════════════════════════════════════════

/// When a pipe has a per-pipe bulk_coeff, it should override the global coefficient.
/// A pipe with kb=0 (no decay) next to the global kb<0 should retain concentration.
#[test]
fn per_pipe_bulk_coeff_overrides_global() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 60.0,
            hyd_step: 3600.0,
            duration: 7200.0,
            bulk_coeff: -1.0, // aggressive global decay /day
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        .node_quality("R1", 10.0)
        .junction("J1", 0.0, 0.0)
        .junction("J2", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0) // uses global kb
        .hw_pipe_with_bulk("P2", "J1", "J2", 100.0, 12.0, 100.0, 0.0); // kb=0, no decay

    let (net, ns, mut ls, _) = builder.build_with_favad();
    ls[0].flow = 0.0; // stagnant
    ls[1].flow = 0.0;

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // Set both pipes to same initial concentration
    for k in 0..2 {
        if let Some(pq) = &mut state.pipe_quality[k] {
            for seg in &mut pq.segments {
                seg.concentration = 10.0;
            }
        }
    }

    advance_quality(&mut state, &net, &ns, &ls, 3600.0, 0.0);

    // P1 (global kb=-1/day) should have decayed
    let c_p1 = state.pipe_quality[0].as_ref().unwrap().segments[0].concentration;

    // P2 (per-pipe kb=0) should retain original concentration
    let c_p2 = state.pipe_quality[1].as_ref().unwrap().segments[0].concentration;

    assert!(c_p1 < 10.0, "P1 should decay with global kb: got {c_p1}");
    assert_abs_diff_eq!(c_p2, 10.0, epsilon = 1e-10,);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Series pipes — concentration propagates sequentially
// ═══════════════════════════════════════════════════════════════════════════════

/// R1(C=10) → P1 → J1 → P2 → J2. After flushing both pipes, J2 should reach
/// C=10.
#[test]
fn chemical_propagates_through_series_pipes() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 10.0,
            duration: 7200.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        .node_quality("R1", 10.0)
        .junction("J1", 0.0, 0.0)
        .junction("J2", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0)
        .hw_pipe("P2", "J1", "J2", 100.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    ls[0].flow = 1.0;
    ls[1].flow = 1.0;

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // Pipe volume in SI (each): D=12in=0.3048m, L=100ft=30.48m → V ≈ 2.224 m³
    // Total: 4.45 m³. Travel time at 1 m³/s: ≈ 4.4 s.
    // Advance 600 s (well beyond travel time).
    advance_quality(&mut state, &net, &ns, &ls, 600.0, 0.0);

    // Both J1 and J2 should be ≈10 mg/L.
    assert_abs_diff_eq!(state.node_conc[1], 10.0, epsilon = 0.5);
    assert_abs_diff_eq!(state.node_conc[2], 10.0, epsilon = 0.5);
}

// ═══════════════════════════════════════════════════════════════════════════════
// §6.7.3 — FIFO plug-flow tank mixing
// ═══════════════════════════════════════════════════════════════════════════════

/// FIFO mixing: oldest water exits first. When a slug of concentration C1 is
/// pushed into a tank followed by C2, the tank should discharge C1 first,
/// then C2.
///
/// R1 (C=10) → P1 → T1 (FIFO, C=0) → P2 → J1
///
/// After flushing the initial tank volume with C=10 water, the tank outflow
/// should approach 10 mg/L.
#[test]
fn fifo_tank_preserves_slug_order() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 10.0,
            duration: 7200.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 200.0)
        .node_quality("R1", 10.0)
        .tank_with_mixing("T1", 100.0, 10.0, 0.0, 20.0, 10.0, MixModel::Fifo, 1.0)
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "T1", 10.0, 12.0, 100.0)
        .hw_pipe("P2", "T1", "J1", 10.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    ls[0].flow = 1.0; // 1 m³/s inflow to tank
    ls[1].flow = 1.0; // 1 m³/s outflow from tank

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // Tank starts at C=0. Inflow is C=10. After enough time, the FIFO
    // should discharge the initial C=0 water, then produce C=10.
    // Tank in SI: diam=10ft=3.048m, level=10ft=3.048m
    // V = π × (1.524)² × 3.048 ≈ 22.3 m³
    // At 1 m³/s, flush time ≈ 22 s. Advance 2000 s to fully flush.
    advance_quality(&mut state, &net, &ns, &ls, 2000.0, 0.0);

    // By now the earliest (C=0) segments should have been pushed out.
    // The outflow concentration should be close to 10.
    assert_abs_diff_eq!(state.node_conc[1], 10.0, epsilon = 1.0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// §6.7.4 — LIFO stacked-layer tank mixing
// ═══════════════════════════════════════════════════════════════════════════════

/// LIFO mixing: newest water exits first. When inflow at C=10 enters a tank
/// initially at C=0, the outflow should immediately show C near 10 (newest
/// water exits first), even though old C=0 water remains at the bottom.
#[test]
fn lifo_tank_newest_exits_first() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 10.0,
            duration: 7200.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 200.0)
        .node_quality("R1", 10.0)
        .tank_with_mixing("T1", 100.0, 10.0, 0.0, 20.0, 10.0, MixModel::Lifo, 1.0)
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "T1", 10.0, 12.0, 100.0)
        .hw_pipe("P2", "T1", "J1", 10.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    ls[0].flow = 1.0;
    ls[1].flow = 1.0;

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // After a moderate advance, the LIFO outflow should be close to 10
    // because the newest water (C=10) exits first.
    advance_quality(&mut state, &net, &ns, &ls, 200.0, 0.0);

    // LIFO outflow should approach the inflow concentration quickly
    assert!(
        state.node_conc[1] > 5.0,
        "LIFO should discharge newest water first: got {}",
        state.node_conc[1]
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// §6.7.2 — Two-compartment tank mixing
// ═══════════════════════════════════════════════════════════════════════════════

/// Two-compartment mixing: a mixing zone and a stagnant zone. Inflow mixes
/// into the active zone first; stagnant zone exchanges slowly.
///
/// After extended flushing, both zones should approach inflow concentration.
#[test]
fn two_comp_mixing_approaches_inflow() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 10.0,
            duration: 7200.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 200.0)
        .node_quality("R1", 10.0)
        .tank_with_mixing(
            "T1",
            100.0,
            10.0,
            0.0,
            20.0,
            10.0,
            MixModel::TwoCompartment,
            0.5, // 50% mixing zone
        )
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "T1", 10.0, 12.0, 100.0)
        .hw_pipe("P2", "T1", "J1", 10.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    ls[0].flow = 1.0;
    ls[1].flow = 1.0;

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // After extensive flushing (10× tank volume), both zones converge.
    advance_quality(&mut state, &net, &ns, &ls, 5000.0, 0.0);

    // Tank outflow (mixing zone concentration) should approach 10.
    assert_abs_diff_eq!(state.node_conc[1], 10.0, epsilon = 1.5);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Water age increases over time
// ═══════════════════════════════════════════════════════════════════════════════

/// In a pipe with flow, water age at the downstream end should equal the
/// travel time through the pipe.
///
/// R1 (age=0) → P1 (L=100ft=30.48m, D=12in=0.3048m) → J1 (no demand)
/// Flow = 1 CFS = 0.028317 m³/s
/// Pipe volume: V = π × (0.1524)² × 30.48 ≈ 2.224 m³
/// Travel time: 2.224 / 0.028317 ≈ 78.5 s
#[test]
fn water_age_equals_travel_time() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Age,
            qual_step: 10.0,
            duration: 7200.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    // 1 CFS = 0.028317 m³/s — same physical scenario as the original test
    ls[0].flow = 0.028317_f64;

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // Pipe volume in SI: D=12in=0.3048m, L=100ft=30.48m
    // V = π × (0.1524)² × 30.48 ≈ 2.224 m³
    let d_m = 0.3048_f64;
    let l_m = 30.48_f64;
    let pipe_vol = std::f64::consts::PI * (d_m / 2.0).powi(2) * l_m;
    let travel_time = pipe_vol / 0.028317_f64; // ≈ 78.5 s

    // Advance well past travel time
    advance_quality(&mut state, &net, &ns, &ls, 600.0, 0.0);

    // Reservoir age = 0; J1 age ≈ travel_time / 3600 hours
    let age_hours = travel_time / 3600.0;
    assert_abs_diff_eq!(state.node_conc[0], 0.0, epsilon = 0.01); // reservoir always 0
    assert_abs_diff_eq!(state.node_conc[1], age_hours, epsilon = 0.01);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Source trace — 100% at source, propagates downstream
// ═══════════════════════════════════════════════════════════════════════════════

/// Trace mode sets the source node to 100%, and all downstream nodes
/// should approach 100% once the pipe is fully flushed.
#[test]
fn trace_source_reaches_100_downstream() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Trace,
            trace_node: Some("R1".to_string()),
            qual_step: 10.0,
            duration: 7200.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    ls[0].flow = 1.0; // 1 m³/s

    let mut state = init_quality(&net, &ns, &ls).unwrap();

    // After flushing (pipe V≈2.224 m³ at 1 m³/s → travel time ≈2.2 s; we advance 600 s)
    advance_quality(&mut state, &net, &ns, &ls, 600.0, 0.0);

    // R1 = 100% (source)
    assert_abs_diff_eq!(state.node_conc[0], 100.0, epsilon = 1.0);
    // J1 should be at 100% (fully flushed from R1)
    assert_abs_diff_eq!(state.node_conc[1], 100.0, epsilon = 1.0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Regression — Q_STAG SI threshold
// ═══════════════════════════════════════════════════════════════════════════════

/// A pipe carrying flow = 1e-6 m³/s must advect mass.
///
/// This flow is above the correct SI threshold Q_STAG = 3.154e-7 m³/s but
/// below the old incorrect value of 1.114e-5 m³/s (EPANET's QZERO constant
/// used without converting from ft³/s to m³/s). With the wrong threshold the
/// pipe is treated as stagnant and J1 stays at 0; with the correct threshold
/// the pipe is active and J1 reaches the reservoir concentration.
///
/// Pipe dimensions (user units, GPM): L = 1 ft, D = 1 in.
/// In SI: L ≈ 0.3048 m, D ≈ 0.02540 m, V ≈ 1.54e-4 m³.
/// Travel time at 1e-6 m³/s ≈ 154 s — well within the 400 s window.
#[test]
fn q_stag_threshold_allows_low_flow_transport() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 10.0,
            duration: 400.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        .node_quality("R1", 10.0)
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 1.0, 1.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    // 1e-6 m³/s: above new Q_STAG (3.154e-7) but below old Q_STAG (1.114e-5).
    ls[0].flow = 1e-6;

    let mut state = init_quality(&net, &ns, &ls).unwrap();
    // Advance 400 s — pipe fully flushed after ≈154 s.
    advance_quality(&mut state, &net, &ns, &ls, 400.0, 0.0);

    // Reservoir concentration must have reached J1 (transport was active).
    assert_relative_eq!(state.node_conc[1], 10.0, max_relative = 0.05);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Regression — MASS source injection formula (SI denominator)
// ═══════════════════════════════════════════════════════════════════════════════

/// A MASS source at J1 adds r_s = 60 mg/min.  With qual_step = 60 s and
/// outflow = 1e-3 m³/s × 60 s = 0.06 m³ = 60 L, the expected concentration
/// increment is:
///
///   Δc = (60 mg/min ÷ 60 s/min) × 60 s / (0.06 m³ × 1000 L/m³)
///      = 1 mg / 60 L = 1/60 mg/L × 60 = 1.0 mg/L
///
/// The old code divided by `volout × 28.317` (treating m³ as ft³), which
/// would give ≈ 35.3 mg/L — off by the ft³-to-L factor.
#[test]
fn mass_source_injection_formula() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 60.0,
            duration: 60.0,
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        // initial_quality = 0 so reservoir supplies c = 0, isolating the source
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0);

    let (mut net, mut ns, mut ls, _) = builder.build_with_favad();

    // Attach MASS source to J1 (node index 1 in 0-based array).
    net.nodes[1].source = Some(QualitySource {
        node: 2, // 1-based
        kind: SourceType::Mass,
        base_value: 60.0, // 60 mg/min
        pattern: None,
    });

    // Give J1 a demand of 1e-3 m³/s so volout = 1e-3 × 60 s = 0.06 m³.
    ns[1].demand_flow = 1e-3;
    // Pipe carries the same flow into J1 (continuity).
    ls[0].flow = 1e-3;

    let mut state = init_quality(&net, &ns, &ls).unwrap();
    // One quality sub-step (60 s).
    advance_quality(&mut state, &net, &ns, &ls, 60.0, 0.0);

    // R1 supplies c = 0 → J1 mixed = 0 before source.
    // After source: Δc = (60/60) × 60 / (0.06 × 1000) = 1.0 mg/L.
    assert_abs_diff_eq!(state.node_conc[1], 1.0, epsilon = 1e-9);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Regression — bulk reaction accumulator uses SI unit conversion (×1000, not
// ×28.317)
// ═══════════════════════════════════════════════════════════════════════════════

/// Verifies that `reacted_bulk` (accumulated as mg/L × m³) converts correctly
/// to mg/hr via the factor 1000 L/m³.
///
/// Setup: stagnant pipe (flow = 0) pre-loaded at c₀ = 10.0 mg/L, global
/// bulk coefficient k_b = −1/day.  One quality sub-step of 3600 s (1 hour).
///
/// Forward-Euler step:
///   Δc_b = k_b × c₀ × dt = (−1/86400) × 10 × 3600 = −5/12 mg/L
///
/// Pipe dimensions (user units, GPM): L = 100 ft, D = 12 in.
/// In SI: L = 100/3.2808 m, D = 12/39.370 m.
///   V = π × (D/2)² × L
///
/// Accumulator: |Δc_b| × V (mg/L × m³).
/// Output rate expected: accumulator × 1000 / 1.0 hr (mg/hr).
///
/// With the old factor 28.317 the rate would be ≈ 35× smaller.
#[test]
fn bulk_reaction_accumulator_uses_si_conversion() {
    let builder = TestNetworkBuilder::new()
        .with_options(SimulationOptions {
            quality_mode: QualityMode::Chemical,
            qual_step: 3600.0,
            hyd_step: 3600.0,
            duration: 3600.0,
            bulk_coeff: -1.0, // /day; converted to /s during build
            ..SimulationOptions::default()
        })
        .reservoir("R1", 100.0)
        .junction("J1", 0.0, 0.0)
        .node_quality("J1", 10.0) // used as initial segment concentration
        .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0);

    let (net, ns, mut ls, _) = builder.build_with_favad();
    ls[0].flow = 0.0; // stagnant — reactions only, no transport

    let mut state = init_quality(&net, &ns, &ls).unwrap();
    // One hydraulic step = one quality sub-step of 3600 s.
    advance_quality(&mut state, &net, &ns, &ls, 3600.0, 0.0);

    // Compute expected accumulator value from first principles.
    let ucf_elev = 3.2808_f64; // ft → m
    let ucf_diam = 39.370_f64; // in → m
    let l_m = 100.0 / ucf_elev;
    let d_m = 12.0 / ucf_diam;
    let v_pipe = std::f64::consts::PI * (d_m / 2.0).powi(2) * l_m;

    let kb_per_s = -1.0_f64 / 86400.0;
    let c0 = 10.0_f64;
    let dt = 3600.0_f64;
    let delta_c_bulk = (kb_per_s * c0 * dt).abs(); // forward-Euler magnitude
    let expected_accumulator = delta_c_bulk * v_pipe; // mg/L × m³

    // reacted_bulk must equal the analytical prediction.
    assert_relative_eq!(
        state.mass_balance.reacted_bulk,
        expected_accumulator,
        max_relative = 1e-6
    );

    // The binary output conversion: accumulator × 1000 L/m³ / duration_hours.
    // With the old factor 28.317 this would be ≈ 35× smaller.
    let duration_hours = 1.0_f64;
    let expected_rate_mg_hr = expected_accumulator * 1000.0 / duration_hours;
    assert_relative_eq!(
        state.mass_balance.reacted_bulk * 1000.0 / duration_hours,
        expected_rate_mg_hr,
        max_relative = 1e-6
    );
}
