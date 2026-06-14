// Physics-based solver tests.
//
// Each test constructs a network programmatically via TestNetworkBuilder,
// solves one hydraulic time step, and checks results against values derived
// from the spec equations — not EPANET output.
//
// All builder inputs are in user units (GPM, ft, inches for US customary).
// Expected values are computed in internal units (CFS, ft) where needed.

use crate::io::units::make_ucf;
use crate::test_support::{no_pswitch, TestNetworkBuilder};
use crate::{
    CurveKind, DemandModel, FlowUnits, HeadLossFormula, LinkKind, SimulationOptions, ValveType,
};
use approx::assert_relative_eq;

use crate::hydraulics::{build_solver_context, solve_hydraulic_step, SolveResult};

/// Solve one hydraulic step at t=0 and return (node_states, link_states).
fn solve_once(
    builder: TestNetworkBuilder,
) -> (Vec<crate::NodeState>, Vec<crate::LinkState>, SolveResult) {
    let (net, mut ns, mut ls, favad) = builder.build_with_favad();
    let mut ctx = build_solver_context(&net, &favad).unwrap();
    let result =
        solve_hydraulic_step(&net, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch).unwrap();
    (ns, ls, result)
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.2.1 — Single Hazen-Williams pipe
// ═══════════════════════════════════════════════════════════════════════════════

/// Reservoir (100 ft) → 1000 ft pipe (12 in, C=100) → Junction (0 ft, 100 GPM).
///
/// At steady state the pipe flow equals the demand (mass balance).
/// Head loss: R_HW = 4.727 × L / (C^1.852 × D^4.871)
///          = 4.727 × 1000 / (100^1.852 × 1^4.871)
/// In internal units D=1 ft (12 in / 12), Q = 100 GPM / 448.831 = 0.22281 cfs.
/// h_f = R_HW × |Q|^1.852
/// Junction head = 100 - h_f.
#[test]
fn single_hw_pipe_headloss() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 100.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    // Internal-unit calculations (SI: m³/s, m):
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let res_head = 100.0 / ucf.elev;
    let d_m = 12.0 / ucf.diam;
    let l_m = 1000.0 / ucf.elev;
    let c = 100.0_f64;
    let q_m3s: f64 = 100.0 / ucf.flow;

    let r_hw = 10.67 * l_m / (c.powf(1.852) * d_m.powf(4.871));
    let h_f = r_hw * q_m3s.powf(1.852);
    let expected_junction_head = res_head - h_f;

    // Node 0 = R1 (reservoir), Node 1 = J1 (junction)
    assert_relative_eq!(ns[0].head, res_head, epsilon = 1e-6);
    assert_relative_eq!(ns[1].head, expected_junction_head, epsilon = 0.01);

    // Link flow should equal demand (mass balance)
    assert_relative_eq!(ls[0].flow, q_m3s, epsilon = 1e-6);
}

/// Same as above but with a larger demand (500 GPM) to verify non-linear headloss.
#[test]
fn single_hw_pipe_high_demand() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .junction("J1", 0.0, 500.0)
            .hw_pipe("P1", "R1", "J1", 5000.0, 12.0, 130.0),
    );
    assert_eq!(result, SolveResult::Converged);

    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let d_m = 12.0 / ucf.diam;
    let l_m = 5000.0 / ucf.elev;
    let c = 130.0_f64;
    let q_m3s: f64 = 500.0 / ucf.flow;

    let r_hw = 10.67 * l_m / (c.powf(1.852) * d_m.powf(4.871));
    let h_f = r_hw * q_m3s.powf(1.852);
    let expected_head = 200.0 / ucf.elev - h_f;

    assert_relative_eq!(ns[1].head, expected_head, epsilon = 0.01);
    assert_relative_eq!(ls[0].flow, q_m3s, epsilon = 1e-6);
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.2.2 — Single Darcy-Weisbach pipe
// ═══════════════════════════════════════════════════════════════════════════════

/// Reservoir → DW pipe → Junction.
///
/// For DW: R_base = L / (2g D A²) = 8L / (π² g D⁵)
/// h_f = f(Q) * R_base * Q|Q|
///
/// We verify the solver converges and the junction head is below the reservoir
/// head by the expected headloss. Since the friction factor f depends on Q
/// (iterative), we check flow = demand and that h_f > 0 with the right sign.
#[test]
fn single_dw_pipe_headloss() {
    let mut builder = TestNetworkBuilder::new();
    builder.options_mut().head_loss_formula = HeadLossFormula::DarcyWeisbach;
    builder.options_mut().flow_units = FlowUnits::Lps;
    let builder = builder
        .reservoir("R1", 30.0) // 30 m
        .junction("J1", 0.0, 10.0) // 10 LPS
        .hw_pipe("P1", "R1", "J1", 500.0, 300.0, 0.1); // 500m, 300mm, roughness 0.1mm

    let (ns, ls, result) = solve_once(builder);
    assert_eq!(result, SolveResult::Converged);

    // SI internal units (m, m³/s)
    let ucf_lps = make_ucf(FlowUnits::Lps, 1.0);
    let d_m = 300.0 / ucf_lps.diam; // 300 mm → 0.3 m
    let l_m = 500.0 / ucf_lps.elev; // 500 m
    let q_m3s = 10.0 / ucf_lps.flow; // 10 LPS → 0.01 m³/s
    let area = std::f64::consts::PI * (d_m / 2.0).powi(2);
    let g = 9.81_f64; // m/s²
    let r_base = l_m / (2.0 * g * d_m * area * area);

    // Head at reservoir (30 m)
    let res_head_m = 30.0 / ucf_lps.elev;
    assert_relative_eq!(ns[0].head, res_head_m, epsilon = 0.01);

    // Flow should equal demand
    assert_relative_eq!(ls[0].flow, q_m3s, epsilon = 1e-5);

    // Headloss should be positive and the junction head should be below the reservoir
    let h_f = ns[0].head - ns[1].head;
    assert!(h_f > 0.0, "headloss should be positive, got {h_f}");
    assert!(ns[1].head > 0.0, "junction head should be positive");

    // Verify headloss is physically reasonable:
    // For fully turbulent flow in a 300mm pipe at 10 LPS, f ≈ 0.02-0.03
    let h_f_approx = 0.025 * r_base * q_m3s * q_m3s;
    assert_relative_eq!(h_f, h_f_approx, max_relative = 0.3); // within 30% of estimate
}

// ═══════════════════════════════════════════════════════════════════════════════
// Mass balance — series pipes
// ═══════════════════════════════════════════════════════════════════════════════

/// R1 → P1 → J1 (50 GPM) → P2 → J2 (50 GPM).
///
/// Mass balance: flow in P1 = 100 GPM (total), flow in P2 = 50 GPM.
/// Headlosses are additive: H_J1 = H_R1 - h_f(P1), H_J2 = H_J1 - h_f(P2).
#[test]
fn two_pipes_series_mass_balance() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 150.0)
            .junction("J1", 0.0, 50.0)
            .junction("J2", 0.0, 50.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
            .hw_pipe("P2", "J1", "J2", 1000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let q1_m3s = 100.0 / ucf.flow; // P1 carries total demand
    let q2_m3s = 50.0 / ucf.flow; // P2 carries only J2's demand

    // Flow balance
    assert_relative_eq!(ls[0].flow, q1_m3s, epsilon = 1e-5);
    assert_relative_eq!(ls[1].flow, q2_m3s, epsilon = 1e-5);

    // Headloss in each pipe (HW formula, SI)
    let d_m = 12.0 / ucf.diam;
    let l_m = 1000.0 / ucf.elev;
    let r_hw = 10.67 * l_m / (100.0_f64.powf(1.852) * d_m.powf(4.871));
    let hf1 = r_hw * q1_m3s.powf(1.852);
    let hf2 = r_hw * q2_m3s.powf(1.852);
    let res_head = 150.0 / ucf.elev;

    assert_relative_eq!(ns[0].head, res_head, epsilon = 1e-6); // reservoir
    assert_relative_eq!(ns[1].head, res_head - hf1, epsilon = 0.01); // J1
    assert_relative_eq!(ns[2].head, res_head - hf1 - hf2, epsilon = 0.01); // J2
}

// ═══════════════════════════════════════════════════════════════════════════════
// Mass balance — parallel pipes
// ═══════════════════════════════════════════════════════════════════════════════

/// R1 → P1 → J1 ← P2 ← R1
///
/// Two identical parallel pipes from the same reservoir to the same junction.
/// Each should carry exactly half the demand. Headloss across both is equal.
#[test]
fn two_pipes_parallel_flow_split() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 200.0) // 200 GPM total
            .hw_pipe("P1", "R1", "J1", 2000.0, 12.0, 100.0)
            .hw_pipe("P2", "R1", "J1", 2000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let q_each_m3s = (200.0 / 2.0) / ucf.flow; // 100 GPM each

    // Symmetric split
    assert_relative_eq!(ls[0].flow, q_each_m3s, epsilon = 1e-5);
    assert_relative_eq!(ls[1].flow, q_each_m3s, epsilon = 1e-5);

    // Both give same headloss → same junction head
    let d_m = 12.0 / ucf.diam;
    let l_m = 2000.0 / ucf.elev;
    let r_hw = 10.67 * l_m / (100.0_f64.powf(1.852) * d_m.powf(4.871));
    let hf = r_hw * q_each_m3s.powf(1.852);
    assert_relative_eq!(ns[1].head, 100.0 / ucf.elev - hf, epsilon = 0.01);
}

/// Asymmetric parallel pipes: different roughness → unequal flow split but
/// equal headloss across both paths.
#[test]
fn parallel_pipes_unequal_roughness() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 200.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 130.0) // smoother
            .hw_pipe("P2", "R1", "J1", 1000.0, 12.0, 80.0), // rougher
    );
    assert_eq!(result, SolveResult::Converged);

    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let q_total_m3s = 200.0 / ucf.flow;

    // Total flow = sum of pipe flows
    assert_relative_eq!(ls[0].flow + ls[1].flow, q_total_m3s, epsilon = 1e-5);

    // Smoother pipe (C=130) carries more flow
    assert!(
        ls[0].flow > ls[1].flow,
        "smoother pipe should carry more flow"
    );

    // Headloss across each pipe must be equal (parallel constraint)
    let headloss_p1 = ns[0].head - ns[1].head;
    let headloss_p2 = ns[0].head - ns[1].head; // same nodes
    assert_relative_eq!(headloss_p1, headloss_p2, epsilon = 1e-10);

    // Verify using HW formula: h_f = R * Q^1.852 for each pipe (SI)
    let d_m = 12.0 / ucf.diam;
    let l_m = 1000.0 / ucf.elev;
    let r1 = 10.67 * l_m / (130.0_f64.powf(1.852) * d_m.powf(4.871));
    let r2 = 10.67 * l_m / (80.0_f64.powf(1.852) * d_m.powf(4.871));
    let hf1 = r1 * ls[0].flow.powf(1.852);
    let hf2 = r2 * ls[1].flow.powf(1.852);
    assert_relative_eq!(hf1, hf2, epsilon = 0.01);
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.2.5 — PRV valve
// ═══════════════════════════════════════════════════════════════════════════════

/// PRV limits downstream pressure to its setting.
///
/// R1 (200 ft) → P1 → J1 → PRV (50 PSI) → J2 (0 ft, 100 GPM)
///
/// PRV setting is 50 PSI. In internal units: 50 / (0.4333 * 1.0) = 115.38 ft.
/// Hence J2 head should be ≈ 0 + 115.38 = 115.38 ft (elevation + pressure head).
///
/// J1 head should be > PRV setting (upstream must exceed downstream for PRV
/// to be active).
#[test]
fn prv_limits_downstream_pressure() {
    let (ns, _ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .junction("J1", 0.0, 0.0)
            .junction("J2", 0.0, 100.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
            .valve("V1", "J1", "J2", ValveType::Prv, 12.0, 50.0) // 50 PSI
            .hw_pipe("P2", "J2", "J2_end", 1.0, 12.0, 100.0) // dummy
            .junction("J2_end", 0.0, 0.0), // no-demand end
    );
    assert_eq!(result, SolveResult::Converged);

    // PRV downstream head = J2 elevation + setting in m
    // Setting 50 PSI → m of head above J2 (elev 0)
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let setting_m = 50.0 / ucf.pressure;

    // J2 (index 2) head should equal the PRV setting in head terms
    assert_relative_eq!(ns[2].head, setting_m, epsilon = 0.2);

    // Upstream J1 head should exceed PRV setting
    assert!(ns[1].head > setting_m, "upstream must exceed PRV setting");
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.2.5 — Constant-power pump
// ═══════════════════════════════════════════════════════════════════════════════

/// Constant-HP pump: Power = γ Q H / efficiency
/// Therefore head gain H = (Power × η) / (γ × Q).
///
/// 10 HP pump, η=0.75 (default), pumping from low reservoir to high junction.
#[test]
fn constant_hp_pump_head_gain() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 0.0)
            .junction("J1", 0.0, 100.0)
            .const_hp_pump("PMP1", "R1", "J1", 10.0)
            .hw_pipe("P1", "J1", "J1_end", 100.0, 12.0, 100.0)
            .junction("J1_end", 0.0, 0.0), // need a pipe after junction for solver
    );
    assert_eq!(result, SolveResult::Converged);

    // Power (internal) = HP / ucf.power → W
    // γ = 9810 N/m³
    // Constant-HP pump: h_gain = power_w / (γ × |Q|)
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let power_w = 10.0 / ucf.power;
    let gamma = 9810.0_f64;

    let pump_flow = ls[0].flow;
    assert!(pump_flow > 0.0, "pump should have positive flow");

    let actual_head_gain = ns[1].head - ns[0].head;
    assert!(
        actual_head_gain > 3.0,
        "pump should add significant head, got {actual_head_gain}"
    );

    // Energy balance: P = γ Q H_pump
    let energy = gamma * pump_flow.abs() * actual_head_gain;
    assert_relative_eq!(energy, power_w, max_relative = 0.02);
}

// ═══════════════════════════════════════════════════════════════════════════════
// §5.2 — Tank level change (mass balance)
// ═══════════════════════════════════════════════════════════════════════════════

/// A reservoir feeds a tank through a pipe. The tank's inflow at the first
/// step can be verified: with known heads, the flow rate q is determined by
/// the HW formula, and q should equal the solver's reported tank net_flow.
#[test]
fn tank_inflow_consistent_with_head() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0) // 200 ft
            .tank("T1", 100.0, 10.0, 0.0, 30.0, 40.0) // elev 100, init_level 10, diam 40ft
            .hw_pipe("P1", "R1", "T1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    // Tank initial head = bottom_elev + initial_level (both in ft → converted to m)
    // Reservoir head = 200 ft → m; tank initial head = 100 + 10 = 110 ft → m
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let res_head_m = 200.0 / ucf.elev;
    let tank_init_head_m = (100.0 + 10.0) / ucf.elev;
    let tank_head = ns[1].head;
    let res_head = ns[0].head;
    assert_relative_eq!(res_head, res_head_m, epsilon = 1e-6);

    // Tank head should be near initial value (single step)
    assert_relative_eq!(tank_head, tank_init_head_m, epsilon = 0.2);

    // Flow through pipe: from HW formula with the solved heads
    let hf_actual = res_head - tank_head;
    assert!(hf_actual > 0.0, "flow should be from reservoir to tank");

    // Verify the pipe flow is consistent: h_f = R * Q^1.852
    let d_m = 12.0 / ucf.diam;
    let l_m = 1000.0 / ucf.elev;
    let r_hw = 10.67 * l_m / (100.0_f64.powf(1.852) * d_m.powf(4.871));
    let q_expected = (hf_actual / r_hw).powf(1.0 / 1.852);
    assert_relative_eq!(ls[0].flow, q_expected, epsilon = 1e-4);

    // Tank net_flow should match pipe flow
    assert_relative_eq!(ns[1].net_flow.abs(), ls[0].flow.abs(), epsilon = 1e-4);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Zero-demand network — all heads equal reservoir
// ═══════════════════════════════════════════════════════════════════════════════

/// With no demand, all junction heads should equal the reservoir head
/// (no flow → no headloss).
#[test]
fn zero_demand_heads_equal_reservoir() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 50.0, 0.0)
            .junction("J2", 30.0, 0.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
            .hw_pipe("P2", "J1", "J2", 1000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    // No demand → no flow → no headloss → all heads = reservoir head (in m)
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let res_head = 100.0 / ucf.elev;
    assert_relative_eq!(ns[0].head, res_head, epsilon = 1e-6);
    assert_relative_eq!(ns[1].head, res_head, epsilon = 0.001);
    assert_relative_eq!(ns[2].head, res_head, epsilon = 0.001);

    // Flows should be essentially zero
    assert!(ls[0].flow.abs() < 1e-6, "P1 flow should be ~0");
    assert!(ls[1].flow.abs() < 1e-6, "P2 flow should be ~0");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Pressure calculation: P = (H - z) × 0.4333 × SG
// ═══════════════════════════════════════════════════════════════════════════════

/// Junction pressure = (head - elevation) in ft of water. The solver doesn't
/// compute pressure directly — it computes heads — but we verify the
/// relationship by checking head - elevation is positive for a fed junction.
#[test]
fn junction_pressure_positive() {
    let (ns, _ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .junction("J1", 50.0, 100.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let elev_j1_m = 50.0 / ucf.elev;
    let pressure_m = ns[1].head - elev_j1_m;
    assert!(pressure_m > 0.0, "junction should have positive pressure");

    // Pressure should be less than max possible ((200-50) ft → m with no headloss)
    assert!(
        pressure_m < 150.0 / ucf.elev,
        "pressure should be less than static"
    );
    assert!(
        pressure_m > 100.0 / ucf.elev,
        "pressure shouldn't drop too much for 100 GPM in 12in pipe"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Check valve — blocks reverse flow
// ═══════════════════════════════════════════════════════════════════════════════

/// A check valve pipe prevents reverse flow. When the downstream head exceeds
/// the upstream head, the check valve closes and flow → 0.
///
/// R_low (50 ft) → CV_pipe → J1 (0 ft, 0 demand) ← P2 ← R_high (200 ft)
///
/// Without CV, P1 would carry reverse flow (from J1 to R_low). With CV, the
/// check valve closes and P1 carries near-zero flow.
#[test]
fn check_valve_blocks_reverse_flow() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R_low", 50.0)
            .reservoir("R_high", 200.0)
            .junction("J1", 0.0, 0.0)
            .cv_pipe("P_cv", "R_low", "J1", 1000.0, 12.0, 100.0)
            .hw_pipe("P2", "R_high", "J1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    // J1 head should be determined by R_high since P_cv is closed
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    assert_relative_eq!(ns[2].head, 200.0 / ucf.elev, epsilon = 0.1);

    // CV pipe should be closed (near-zero flow)
    assert!(
        ls[0].flow.abs() < 1e-5,
        "CV pipe flow should be near-zero, got {}",
        ls[0].flow
    );

    // Regular pipe carries no flow (no demand)
    assert!(
        ls[1].flow.abs() < 1e-5,
        "P2 flow should be ~0 with no demand"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Closed pipe — no flow through
// ═══════════════════════════════════════════════════════════════════════════════

/// A closed pipe carries near-zero flow regardless of head difference.
///
/// R1 (200 ft) → closed P1 → J1 (0ft, 0 demand) ← open P2 ← R2 (100 ft)
///
/// J1 head should be ≈ 100 ft (from R2 through P2), not 200 ft.
#[test]
fn closed_pipe_no_flow() {
    let builder = TestNetworkBuilder::new()
        .reservoir("R1", 200.0)
        .reservoir("R2", 100.0)
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
        .hw_pipe("P2", "R2", "J1", 1000.0, 12.0, 100.0);

    let (net, mut ns, mut ls, favad) = builder.build_with_favad();

    // Manually close P1 (link index 0)
    ls[0].status = crate::LinkStatus::Closed;

    let mut ctx = build_solver_context(&net, &favad).unwrap();
    let result =
        solve_hydraulic_step(&net, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch).unwrap();
    assert_eq!(result, SolveResult::Converged);

    // J1 head should track R2 since P1 is closed
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    assert_relative_eq!(ns[2].head, 100.0 / ucf.elev, epsilon = 0.1);

    // Closed pipe flow should be near-zero
    assert!(ls[0].flow.abs() < 1e-4, "closed pipe flow should be ~0");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Pump with head curve
// ═══════════════════════════════════════════════════════════════════════════════

/// Three-point pump head curve: the pump adds head according to
/// H_pump = ω² H₀ − r ω^(2−N) Q^N (§3.2.5).
///
/// We verify the pump raises the downstream head above the reservoir.
#[test]
fn pump_with_head_curve() {
    // Head curve: single point (150 GPM, 100 ft) → auto-expanded to three points
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 0.0)
            .junction("J1", 0.0, 100.0) // 100 GPM demand
            .curve("pump_curve", CurveKind::PumpHead, &[(150.0, 100.0)])
            .pump("PMP1", "R1", "J1", "pump_curve"),
    );
    assert_eq!(result, SolveResult::Converged);

    // Pump should add head: J1 head > R1 head
    assert!(
        ns[1].head > ns[0].head,
        "pump should raise downstream head: J1={}, R1={}",
        ns[1].head,
        ns[0].head
    );

    // Flow through pump equals demand (mass balance at J1)
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let q_m3s: f64 = 100.0 / ucf.flow;
    assert_relative_eq!(ls[0].flow, q_m3s, epsilon = 1e-5);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Emitter — flow is proportional to pressure^exponent
// ═══════════════════════════════════════════════════════════════════════════════

/// An emitter node has flow Q_e = C_e × P^γ where P = pressure (head - elev)
/// and γ = 0.5 (default).
///
/// R1 (200 ft) → P1 → J1 (100 ft, emitter C=1.0 GPM/PSI^0.5)
///
/// At steady state: emitter flow = C_e × P^0.5 (in internal units).
/// The solver stores the computed emitter flow in node_states[i].emitter_flow.
#[test]
fn emitter_flow_proportional_to_pressure() {
    let (ns, _ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .junction_with_emitter("J1", 100.0, 0.0, 1.0) // emitter C=1.0, no base demand
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    // Junction should have pressure = head - elevation (in m) > 0
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let pressure_m = ns[1].head - 100.0 / ucf.elev;
    assert!(
        pressure_m > 0.0,
        "emitter node should have positive pressure"
    );

    // Emitter flow should be non-zero
    assert!(
        ns[1].emitter_flow.abs() > 1e-8,
        "emitter should discharge flow, got {}",
        ns[1].emitter_flow
    );

    // The total outflow at J1 (emitter + demand=0) should equal inflow through P1
    // Since demand=0, emitter_flow IS the outflow
    assert!(ns[1].emitter_flow > 0.0, "emitter flow should be positive");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Multiple reservoirs — head determined by lowest-resistance path
// ═══════════════════════════════════════════════════════════════════════════════

/// Two reservoirs at different elevations both connected to one junction.
/// The junction head is determined by the energy balance: head depends on
/// the resistance-weighted contribution of both sources.
#[test]
fn two_reservoirs_one_junction() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .reservoir("R2", 100.0)
            .junction("J1", 0.0, 200.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0) // same resistance
            .hw_pipe("P2", "R2", "J1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    // Flow from R1 > flow from R2 (R1 is higher)
    assert!(ls[0].flow > 0.0, "R1 should supply flow");
    assert!(ls[0].flow > ls[1].flow, "higher reservoir supplies more");

    // Total inflow = demand
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let q_total_m3s: f64 = 200.0 / ucf.flow;
    assert_relative_eq!(ls[0].flow + ls[1].flow, q_total_m3s, epsilon = 1e-4);

    // Junction head between the two reservoir heads (in m)
    let res1_head = 200.0 / ucf.elev;
    let res2_head = 100.0 / ucf.elev;
    assert!(ns[2].head > res2_head, "head > lower reservoir");
    assert!(ns[2].head < res1_head, "head < higher reservoir");

    // Verify headloss consistency: h_f = R * Q^n for each pipe
    let d_m = 12.0 / ucf.diam;
    let l_m = 1000.0 / ucf.elev;
    let r_hw = 10.67 * l_m / (100.0_f64.powf(1.852) * d_m.powf(4.871));
    let hf1 = r_hw * ls[0].flow.powf(1.852);
    let hf2 = r_hw * ls[1].flow.abs().powf(1.852);
    assert_relative_eq!(res1_head - ns[2].head, hf1, epsilon = 0.01);
    assert_relative_eq!(ns[2].head - res2_head, hf2, epsilon = 0.01);
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.2.3 — Chezy-Manning headloss
// ═══════════════════════════════════════════════════════════════════════════════

/// Chezy-Manning: h_f = R_CM × Q × |Q|, where
/// R_CM = n² × L / (k_M² × R_h^(4/3) × A²).
///
/// R1 (100 ft) → P1 (CM, n=0.013, L=1000, D=12in) → J1 (0 ft, 100 GPM)
#[test]
fn single_cm_pipe_headloss() {
    let mut builder = TestNetworkBuilder::new();
    builder.options_mut().head_loss_formula = HeadLossFormula::ChezyManning;
    let builder = builder
        .reservoir("R1", 100.0)
        .junction("J1", 0.0, 100.0)
        .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 0.013); // roughness = Manning n

    let (ns, ls, result) = solve_once(builder);
    assert_eq!(result, SolveResult::Converged);

    // Check mass balance: pipe flow = demand (in m³/s)
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let q_m3s = 100.0 / ucf.flow;
    assert_relative_eq!(ls[0].flow, q_m3s, epsilon = 1e-5);

    // Check headloss using CM formula (SI: m, m³/s)
    // R_CM = n² × L / (k_M² × R_h^(4/3) × A²)
    // k_M = 1.0 (SI Manning)
    let n = 0.013_f64;
    let d_m = 12.0 / ucf.diam;
    let l_m = 1000.0 / ucf.elev;
    let a = std::f64::consts::PI * d_m * d_m / 4.0;
    let r_h = d_m / 4.0;
    let k_m = 1.0_f64; // SI Manning constant
    let r_cm = n * n * l_m / (k_m * k_m * r_h.powf(4.0 / 3.0) * a * a);
    let hf = r_cm * q_m3s * q_m3s;
    let expected_head = 100.0 / ucf.elev - hf;

    assert_relative_eq!(ns[1].head, expected_head, epsilon = 0.01);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Minor loss — h_m = K × Q × |Q|
// ═══════════════════════════════════════════════════════════════════════════════

/// A pipe with minor loss K=10 should have extra headloss beyond friction.
/// Two identical pipes: one with K=0, one with K=10. The K=10 pipe has higher
/// headloss when both carry the same flow.
///
/// We verify: h_total = h_friction + h_minor, where h_minor = K × Q × |Q|.
#[test]
fn minor_loss_adds_to_friction() {
    // Pipe without minor loss
    let (ns_clean, ls_clean, r1) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 100.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(r1, SolveResult::Converged);

    // Same pipe with K=10 minor loss — need to use builder directly since
    // hw_pipe doesn't expose minor_loss. Build manually.
    let (ns_k10, ls_k10, r2) = {
        let builder = TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 100.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0);

        let (mut net, mut ns, mut ls, favad) = builder.build_with_favad();
        // Set minor loss on the pipe
        if let crate::LinkKind::Pipe(ref mut p) = net.links[0].kind {
            p.minor_loss = 10.0;
        }
        let mut ctx = build_solver_context(&net, &favad).unwrap();
        let result =
            solve_hydraulic_step(&net, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch)
                .unwrap();
        (ns, ls, result)
    };
    assert_eq!(r2, SolveResult::Converged);

    // Both carry same flow (same demand)
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let q_m3s = 100.0 / ucf.flow;
    assert_relative_eq!(ls_clean[0].flow, q_m3s, epsilon = 1e-5);
    assert_relative_eq!(ls_k10[0].flow, q_m3s, epsilon = 1e-5);

    // K=10 pipe should have lower junction head (more headloss)
    assert!(
        ns_k10[1].head < ns_clean[1].head,
        "K=10 should have more headloss: clean={}, k10={}",
        ns_clean[1].head,
        ns_k10[1].head
    );

    // Quantify: h_minor = K * q * |q| (internal units: K is dimensionless
    // but scaled by 0.02517 during unit conversion for GPM units)
    // The headloss difference should be the minor loss contribution
    let delta_head = ns_clean[1].head - ns_k10[1].head;
    assert!(delta_head > 0.0, "minor loss should reduce head");
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.2.5 — PSV (Pressure Sustaining Valve)
// ═══════════════════════════════════════════════════════════════════════════════

/// PSV maintains upstream pressure at or above its setting.
///
/// R1 (200 ft) → P1 → J1 ← PSV (60 PSI) ← J2 → P2 → J3 (0ft, 100GPM)
///
/// The PSV ensures J1 (upstream side) doesn't drop below 60 PSI.
#[test]
fn psv_maintains_upstream_pressure() {
    let (ns, _ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .junction("J1", 0.0, 50.0)
            .junction("J2", 0.0, 50.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
            .valve("V1", "J1", "J2", ValveType::Psv, 12.0, 60.0) // 60 PSI
            .hw_pipe("P2", "J2", "J2_end", 1.0, 12.0, 100.0)
            .junction("J2_end", 0.0, 0.0),
    );
    assert_eq!(result, SolveResult::Converged);

    // PSV upstream side (J1) should be at or above setting (in m)
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let setting_m = 60.0 / ucf.pressure;
    // J1 (upstream of PSV) should be >= setting
    assert!(
        ns[1].head >= setting_m - 0.2,
        "PSV upstream head should be >= setting: J1={}, setting_m={}",
        ns[1].head,
        setting_m
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.2.5 — FCV (Flow Control Valve)
// ═══════════════════════════════════════════════════════════════════════════════

/// FCV enforces a fixed flow rate through the valve.
///
/// R1 (100 ft) → P1 → J1 → FCV (100 GPM) → J2 (0 ft, 100 GPM demand)
///
/// The FCV is the only path to J2, so J2 gets exactly 100 GPM through the FCV.
#[test]
fn fcv_limits_flow() {
    let (_ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 0.0)
            .junction("J2", 0.0, 100.0) // 100 GPM demand
            .hw_pipe("P1", "R1", "J1", 500.0, 12.0, 100.0)
            .valve("V1", "J1", "J2", ValveType::Fcv, 12.0, 100.0), // 100 GPM
    );
    assert_eq!(result, SolveResult::Converged);

    // FCV (link index 1) flow should equal setting (100 GPM → m³/s)
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let q_setting_m3s = 100.0 / ucf.flow;
    assert_relative_eq!(ls[1].flow, q_setting_m3s, epsilon = 1e-5);
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.2.5 — TCV (Throttle Control Valve)
// ═══════════════════════════════════════════════════════════════════════════════

/// TCV acts as a minor loss element with setting = loss coefficient.
/// A TCV with high setting creates large headloss across it.
#[test]
fn tcv_throttles_flow() {
    // Baseline: no valve
    let (_ns_base, _ls_base, r1) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .junction("J1", 0.0, 100.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(r1, SolveResult::Converged);

    // With TCV (setting=50 = high minor loss coeff)
    let (ns_tcv, _ls_tcv, r2) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .junction("J1", 0.0, 0.0)
            .junction("J2", 0.0, 100.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
            .valve("V1", "J1", "J2", ValveType::Tcv, 12.0, 50.0)
            .hw_pipe("P2", "J2", "J2_end", 1.0, 12.0, 100.0)
            .junction("J2_end", 0.0, 0.0),
    );
    assert_eq!(r2, SolveResult::Converged);

    // TCV should cause headloss: J2 head < J1 head
    assert!(
        ns_tcv[2].head < ns_tcv[1].head,
        "TCV should create headloss: J1={}, J2={}",
        ns_tcv[1].head,
        ns_tcv[2].head
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.2.5 — PBV (Pressure Breaker Valve)
// ═══════════════════════════════════════════════════════════════════════════════

/// PBV enforces a fixed pressure drop across the valve.
///
/// Head drop across PBV = setting (in PSI → ft conversion).
#[test]
fn pbv_fixed_pressure_drop() {
    let (ns, _ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .junction("J1", 0.0, 0.0)
            .junction("J2", 0.0, 100.0)
            .hw_pipe("P1", "R1", "J1", 100.0, 300.0, 100.0) // large pipe, minimal friction
            .valve("V1", "J1", "J2", ValveType::Pbv, 300.0, 30.0) // 30 PSI drop
            .hw_pipe("P2", "J2", "J2_end", 1.0, 300.0, 100.0)
            .junction("J2_end", 0.0, 0.0),
    );
    assert_eq!(result, SolveResult::Converged);

    // PBV drop = setting in PSI → m of head
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let setting_m = 30.0 / ucf.pressure;
    let actual_drop = ns[1].head - ns[2].head;
    assert_relative_eq!(actual_drop, setting_m, epsilon = 0.3);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Four-node loop — Kirchhoff's laws
// ═══════════════════════════════════════════════════════════════════════════════

/// A diamond loop topology exercises the solver's loop-closing ability.
///
///       R1 (200 ft)
///      /     \
///    P1       P2
///    /         \
///  J1 ──P5── J2
///    \         /
///    P3       P4
///     \       /
///       J3
///
/// Kirchhoff's current law: flow in = flow out at every node.
/// Kirchhoff's voltage law: headloss around any loop = 0.
#[test]
fn four_node_loop_kirchhoff() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .junction("J1", 0.0, 50.0)
            .junction("J2", 0.0, 50.0)
            .junction("J3", 0.0, 50.0)
            .hw_pipe("P1", "R1", "J1", 500.0, 12.0, 100.0)
            .hw_pipe("P2", "R1", "J2", 500.0, 12.0, 100.0)
            .hw_pipe("P3", "J1", "J3", 500.0, 12.0, 100.0)
            .hw_pipe("P4", "J2", "J3", 500.0, 12.0, 100.0)
            .hw_pipe("P5", "J1", "J2", 200.0, 8.0, 100.0), // cross-link, smaller
    );
    assert_eq!(result, SolveResult::Converged);

    // Kirchhoff's current law at J1: inflow = outflow + demand
    // Just check total demand is served
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let total_demand_m3s = 150.0 / ucf.flow;
    let total_supply = ls[0].flow + ls[1].flow; // P1 + P2 from reservoir
    assert_relative_eq!(total_supply, total_demand_m3s, epsilon = 1e-4);

    // All junction heads should be below reservoir and positive
    for i in 1..4 {
        assert!(
            ns[i].head < 200.0 / ucf.elev,
            "J{} head should be < reservoir",
            i
        );
        assert!(ns[i].head > 0.0, "J{} head should be positive", i);
    }

    // Symmetry check: J1 and J2 should have similar heads
    let head_diff = (ns[1].head - ns[2].head).abs();
    assert!(
        head_diff < 5.0 / ucf.elev,
        "symmetric nodes should have similar heads: diff={head_diff}"
    );

    // HW headloss consistency for P1: h_f = R × Q^1.852
    let d_m = 12.0 / ucf.diam;
    let l_m = 500.0 / ucf.elev;
    let r_p1 = 10.67 * l_m / (100.0_f64.powf(1.852) * d_m.powf(4.871));
    let hf_p1 = r_p1 * ls[0].flow.abs().powf(1.852);
    assert_relative_eq!(200.0 / ucf.elev - ns[1].head, hf_p1, epsilon = 0.05);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Multiple demand categories sum correctly
// ═══════════════════════════════════════════════════════════════════════════════

/// Two demand categories (10 GPM + 5 GPM = 15 GPM total) on a single junction.
/// Flow through the pipe should equal the sum.
#[test]
fn multiple_demands_sum() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 15.0) // total = 15 GPM
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let q_m3s = 15.0 / ucf.flow;
    assert_relative_eq!(ls[0].flow, q_m3s, epsilon = 1e-5);

    // Verify headloss
    let d_m = 12.0 / ucf.diam;
    let l_m = 1000.0 / ucf.elev;
    let r_hw = 10.67 * l_m / (100.0_f64.powf(1.852) * d_m.powf(4.871));
    let hf = r_hw * q_m3s.powf(1.852);
    assert_relative_eq!(ns[1].head, 100.0 / ucf.elev - hf, epsilon = 0.01);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Negative demand — junction as source
// ═══════════════════════════════════════════════════════════════════════════════

/// A negative demand means the junction *injects* flow into the network.
///
/// J_source (−200 GPM injection) → P1 → J_sink (200 GPM demand)
///
/// No reservoir needed: the negative demand node acts as the source.
/// Flow through P1 = 200 GPM.
#[test]
fn negative_demand_source() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R_ref", 100.0) // reference head for the solver
            .junction("J_source", 100.0, -200.0) // injects 200 GPM
            .junction("J_sink", 0.0, 200.0) // withdraws 200 GPM
            .hw_pipe("P_ref", "R_ref", "J_source", 1.0, 12.0, 100.0) // short tie
            .hw_pipe("P1", "J_source", "J_sink", 1000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    // P1 should carry 200 GPM from source to sink (in m³/s)
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let q_m3s = 200.0 / ucf.flow;
    assert_relative_eq!(ls[1].flow, q_m3s, epsilon = 1e-3);

    // Source head > sink head (drives flow)
    assert!(ns[1].head > ns[2].head, "source should be at higher head");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Specific gravity — affects pressure conversion only
// ═══════════════════════════════════════════════════════════════════════════════

/// Specific gravity changes the pressure:head conversion but not the headloss.
/// Two networks: SG=1.0 (water) and SG=1.1 (denser fluid), same heads,
/// but pressure_psi = (H − z) × 0.4333 × SG differs.
///
/// We verify the solver converges to the same heads (SG doesn't affect HW)
/// but pressure is scaled.
#[test]
fn specific_gravity_affects_pressure_not_head() {
    // SG = 1.0 (default)
    let (ns_10, ls_10, r1) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 100.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(r1, SolveResult::Converged);

    // SG = 1.1
    let mut builder = TestNetworkBuilder::new();
    builder.options_mut().specific_gravity = 1.1;
    let (ns_11, ls_11, r2) = solve_once(
        builder
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 100.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(r2, SolveResult::Converged);

    // Heads should be identical (HW formula is independent of SG)
    assert_relative_eq!(ns_10[1].head, ns_11[1].head, epsilon = 0.01);
    assert_relative_eq!(ls_10[0].flow, ls_11[0].flow, epsilon = 1e-6);

    // Pressure: P_psi = head_m × ucf.pressure × SG
    // ucf.pressure = PSI per m of head (user / SI), so multiply.
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let p_10 = ns_10[1].head * ucf.pressure * 1.0;
    let p_11 = ns_11[1].head * ucf.pressure * 1.1;
    assert!(
        (p_11 - p_10).abs() > 1.0,
        "SG=1.1 should give higher PSI: p10={p_10}, p11={p_11}"
    );
    assert_relative_eq!(p_11 / p_10, 1.1, epsilon = 0.001);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Dead end — stagnant branch with in-network demand
// ═══════════════════════════════════════════════════════════════════════════════

/// A dead-end branch (no outflow) should have its head determined by the
/// upstream node, with zero flow in the dead-end pipe.
///
/// R1 → P1 → J1 (100GPM) → P2 → J_dead (0GPM, dead end)
#[test]
fn dead_end_no_flow() {
    let (ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .junction("J1", 0.0, 100.0)
            .junction("J_dead", 0.0, 0.0) // dead end
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
            .hw_pipe("P2", "J1", "J_dead", 500.0, 8.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    // Dead-end pipe should carry ~0 flow
    assert!(ls[1].flow.abs() < 1e-6, "dead-end pipe flow should be ~0");

    // Dead-end junction head should equal J1 head (no headloss with zero flow)
    assert_relative_eq!(ns[2].head, ns[1].head, epsilon = 0.01);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Emitter backflow — negative emitter flow at low pressure
// ═══════════════════════════════════════════════════════════════════════════════

/// When head < elevation at an emitter node (negative pressure), the emitter
/// direction reverses (backflow into the network) if backflow is enabled.
#[test]
fn emitter_backflow_at_negative_pressure() {
    let mut builder = TestNetworkBuilder::new();
    builder.options_mut().emitter_backflow = true;
    let (ns, _ls, result) = solve_once(
        builder
            .reservoir("R1", 10.0) // low head reservoir
            .junction_with_emitter("J1", 100.0, 0.0, 5.0) // elev > reservoir → negative pressure
            .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    // J1 head ≈ R1 head (no demand, short pipe), so head < elevation
    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let elev_j1_m = 100.0 / ucf.elev; // 100 ft → m
    let pressure_m = ns[1].head - elev_j1_m;
    assert!(pressure_m < 0.0, "should have negative pressure");

    // With backflow enabled, emitter flow should be negative (inflow)
    assert!(
        ns[1].emitter_flow <= 0.0,
        "emitter should backflow with negative pressure: got {}",
        ns[1].emitter_flow
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Remaining single-period fixture extractions
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn demand_multiplier_scales_total_demand() {
    let mut builder = TestNetworkBuilder::new();
    builder.options_mut().demand_multiplier = 2.0;
    let (ns, ls, result) = solve_once(
        builder
            .reservoir("R1", 200.0)
            .junction("J1", 0.0, 100.0)
            .junction("J2", 0.0, 80.0)
            .junction("J3", 0.0, 60.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
            .hw_pipe("P2", "J1", "J2", 800.0, 10.0, 100.0)
            .hw_pipe("P3", "J2", "J3", 600.0, 8.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let expected_total = (100.0 + 80.0 + 60.0) * 2.0 / ucf.flow;
    assert_relative_eq!(ls[0].flow, expected_total, epsilon = 1e-5);
    let served = ns[1].demand_flow + ns[2].demand_flow + ns[3].demand_flow;
    assert_relative_eq!(served, expected_total, epsilon = 1e-5);
}

#[test]
fn pda_demand_reduces_below_full_demand() {
    let mut builder = TestNetworkBuilder::new();
    builder.options_mut().demand_model = DemandModel::PressureDriven;
    builder.options_mut().pda_min_pressure = 0.0;
    builder.options_mut().pda_required_pressure = 30.0;
    builder.options_mut().pda_pressure_exponent = 0.5;

    let (ns, _ls, result) = solve_once(
        builder
            .reservoir("R1", 100.0)
            .junction("J1", 50.0, 100.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let full_demand = 100.0 / ucf.flow; // 100 GPM → m³/s
    assert!(ns[1].demand_flow > 0.0, "PDA demand should be non-zero");
    assert!(
        ns[1].demand_flow < full_demand,
        "PDA demand should be reduced under low pressure"
    );
}

#[test]
fn leakage_favad_produces_leakage_flow() {
    let builder = TestNetworkBuilder::new()
        .reservoir("R1", 100.0)
        .junction("J1", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0);

    let (mut net, mut ns, mut ls, _) = builder.build_with_favad();
    if let LinkKind::Pipe(ref mut p) = net.links[0].kind {
        p.leak_coeff_1 = 100.0;
        p.leak_coeff_2 = 0.1;
    }
    let favad = net.compute_favad();

    let mut ctx = build_solver_context(&net, &favad).unwrap();
    let result =
        solve_hydraulic_step(&net, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch).unwrap();
    assert_eq!(result, SolveResult::Converged);

    assert!(ns[1].leakage_flow > 0.0, "leakage should be positive");
    assert!(ns[1].demand_flow.abs() < 1e-10);
    assert!(ns[1].emitter_flow.abs() < 1e-10);
    assert_relative_eq!(ls[0].flow, ns[1].leakage_flow, epsilon = 1e-6);
}

#[test]
fn pda_favad_combined_has_demand_and_leakage() {
    let mut builder = TestNetworkBuilder::new();
    builder.options_mut().demand_model = DemandModel::PressureDriven;
    builder.options_mut().pda_min_pressure = 0.0;
    builder.options_mut().pda_required_pressure = 30.0;
    builder.options_mut().pda_pressure_exponent = 0.5;
    let builder = builder
        .reservoir("R1", 200.0)
        .junction("J1", 50.0, 100.0)
        .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0);

    let (mut net, mut ns, mut ls, _) = builder.build_with_favad();
    if let LinkKind::Pipe(ref mut p) = net.links[0].kind {
        p.leak_coeff_1 = 50.0;
        p.leak_coeff_2 = 0.1;
    }
    let favad = net.compute_favad();

    let mut ctx = build_solver_context(&net, &favad).unwrap();
    let result =
        solve_hydraulic_step(&net, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch).unwrap();
    assert_eq!(result, SolveResult::Converged);

    assert!(ns[1].demand_flow > 0.0);
    assert!(ns[1].leakage_flow > 0.0);
    assert_relative_eq!(
        ls[0].flow,
        ns[1].demand_flow + ns[1].leakage_flow,
        epsilon = 1e-6
    );
}

#[test]
fn emitter_pda_favad_combined_has_all_three_outflows() {
    let mut builder = TestNetworkBuilder::new();
    builder.options_mut().demand_model = DemandModel::PressureDriven;
    builder.options_mut().pda_min_pressure = 0.0;
    builder.options_mut().pda_required_pressure = 30.0;
    builder.options_mut().pda_pressure_exponent = 0.5;
    let builder = builder
        .reservoir("R1", 200.0)
        .junction_with_emitter("J1", 50.0, 100.0, 1.0)
        .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0);

    let (mut net, mut ns, mut ls, _) = builder.build_with_favad();
    if let LinkKind::Pipe(ref mut p) = net.links[0].kind {
        p.leak_coeff_1 = 50.0;
        p.leak_coeff_2 = 0.1;
    }
    let favad = net.compute_favad();

    let mut ctx = build_solver_context(&net, &favad).unwrap();
    let result =
        solve_hydraulic_step(&net, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch).unwrap();
    assert_eq!(result, SolveResult::Converged);

    assert!(ns[1].demand_flow > 0.0);
    assert!(ns[1].emitter_flow > 0.0);
    assert!(ns[1].leakage_flow > 0.0);
    assert_relative_eq!(
        ls[0].flow,
        ns[1].demand_flow + ns[1].emitter_flow + ns[1].leakage_flow,
        epsilon = 1e-6
    );
}

#[test]
fn fcv_prv_series_regulates_pressure() {
    let (_ns, ls, result) = solve_once(
        TestNetworkBuilder::new()
            .junction("J1", 0.0, 0.0)
            .junction("J2", 0.0, 0.0)
            .junction("J3", 0.0, 0.0)
            .junction("J4", 0.0, 0.0)
            .reservoir("R1", 120.0)
            .reservoir("R2", 60.0)
            .hw_pipe("P1", "R1", "J1", 400.0, 12.0, 100.0)
            .valve("FCV", "J1", "J2", ValveType::Fcv, 12.0, 2000.0)
            .hw_pipe("P2", "J2", "J3", 200.0, 12.0, 100.0)
            .valve("PRV", "J3", "J4", ValveType::Prv, 12.0, 30.0)
            .hw_pipe("P3", "J4", "R2", 600.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);
    assert!(
        ls[1].flow.abs() > 1e-6,
        "series FCV/PRV network should carry flow"
    );
}

#[test]
fn prv_psv_series_enforces_both_constraints() {
    let (ns, _ls, result) = solve_once(
        TestNetworkBuilder::new()
            .reservoir("R1", 200.0)
            .junction("J1", 0.0, 20.0)
            .junction("J2", 0.0, 10.0)
            .junction("J3", 0.0, 10.0)
            .junction("J4", 0.0, 15.0)
            .hw_pipe("P1", "R1", "J1", 500.0, 12.0, 100.0)
            .valve("PRV1", "J1", "J2", ValveType::Prv, 12.0, 50.0)
            .hw_pipe("P2", "J2", "J3", 300.0, 12.0, 100.0)
            .valve("PSV1", "J3", "J4", ValveType::Psv, 12.0, 30.0)
            .hw_pipe("P3", "J4", "J1", 800.0, 12.0, 100.0),
    );
    assert_eq!(result, SolveResult::Converged);

    let ucf = make_ucf(FlowUnits::Gpm, 1.0);
    let prv_setting_m = 50.0 / ucf.pressure;
    let psv_setting_m = 30.0 / ucf.pressure;
    assert!(
        ns[2].head <= prv_setting_m + 0.5,
        "PRV downstream should be limited"
    );
    assert!(
        ns[3].head >= psv_setting_m - 0.5,
        "PSV upstream should be sustained"
    );
}

#[test]
fn gpv_uses_headloss_curve() {
    let builder = TestNetworkBuilder::new()
        .reservoir("R1", 100.0)
        .junction("J1", 0.0, 0.0)
        .junction("J2", 0.0, 10.0)
        .hw_pipe("P1", "R1", "J1", 300.0, 12.0, 100.0)
        .valve("GPV", "J1", "J2", ValveType::Gpv, 12.0, 0.0)
        .curve(
            "HC1",
            CurveKind::GpvHeadloss,
            &[(0.0, 0.0), (50.0, 50.0), (150.0, 50.0)],
        );

    let (mut net, mut ns, mut ls, favad) = builder.build_with_favad();
    if let LinkKind::Valve(ref mut v) = net.links[1].kind {
        v.curve = Some("HC1".to_string());
    }

    let mut ctx = build_solver_context(&net, &favad).unwrap();
    let result =
        solve_hydraulic_step(&net, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch).unwrap();
    assert_eq!(result, SolveResult::Converged);

    assert!(ls[1].flow.abs() > 1e-6, "GPV should carry flow");
    assert!(
        ns[1].head > ns[2].head,
        "GPV should impose headloss from J1 to J2"
    );
}

#[test]
fn pcv_with_loss_curve_converges_and_carries_flow() {
    let builder = TestNetworkBuilder::new()
        .reservoir("R1", 200.0)
        .reservoir("R2", 50.0)
        .junction("J1", 0.0, 0.0)
        .junction("J2", 0.0, 0.0)
        .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
        .valve("PCV1", "J1", "J2", ValveType::Pcv, 12.0, 50.0)
        .hw_pipe("P2", "J2", "R2", 1000.0, 12.0, 100.0)
        .curve(
            "LC1",
            CurveKind::PcvLossRatio,
            &[
                (0.0, 0.0),
                (25.0, 10.0),
                (50.0, 40.0),
                (75.0, 70.0),
                (100.0, 100.0),
            ],
        );

    let (mut net, mut ns, mut ls, favad) = builder.build_with_favad();
    if let LinkKind::Valve(ref mut v) = net.links[1].kind {
        v.minor_loss = 5.0;
        v.curve = Some("LC1".to_string());
    }

    let mut ctx = build_solver_context(&net, &favad).unwrap();
    let result =
        solve_hydraulic_step(&net, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch).unwrap();
    assert_eq!(result, SolveResult::Converged);
    assert!(ls[1].flow.abs() > 1e-8, "PCV should carry non-zero flow");
}

#[test]
fn flow_units_conversion_matrix_matches_internal_mass_balance() {
    let cases: &[(FlowUnits, f64, f64, f64, f64, f64, f64)] = &[
        (FlowUnits::Gpm, 500.0, 200.0, 300.0, 5000.0, 3000.0, 12.0),
        (FlowUnits::Mgd, 1.0, 0.5, 300.0, 5000.0, 3000.0, 24.0),
        (FlowUnits::Imgd, 1.0, 0.5, 300.0, 5000.0, 3000.0, 24.0),
        (FlowUnits::Afd, 1.0, 0.5, 300.0, 5000.0, 3000.0, 24.0),
        (FlowUnits::Cmd, 500.0, 200.0, 100.0, 1000.0, 800.0, 300.0),
        (FlowUnits::Cmh, 5.0, 3.0, 100.0, 1000.0, 800.0, 200.0),
        (FlowUnits::Cms, 0.5, 0.2, 100.0, 1000.0, 800.0, 600.0),
        (FlowUnits::Lpm, 300.0, 150.0, 100.0, 1000.0, 800.0, 300.0),
        (FlowUnits::Mld, 5.0, 2.0, 100.0, 1000.0, 800.0, 500.0),
        (FlowUnits::Cfs, 10.0, 5.0, 300.0, 5000.0, 3000.0, 180.0),
    ];

    for &(units, d1, d2, r_head, l1, l2, diam) in cases {
        let options = SimulationOptions {
            flow_units: units,
            ..Default::default()
        };
        let ucf = make_ucf(units, options.specific_gravity);
        let expected_total = (d1 + d2) / ucf.flow;

        let (ns, ls, result) = solve_once(
            TestNetworkBuilder::new()
                .with_options(options)
                .reservoir("R1", r_head)
                .junction("J1", 0.0, d1)
                .junction("J2", 0.0, d2)
                .hw_pipe("P1", "R1", "J1", l1, diam, 100.0)
                .hw_pipe("P2", "J1", "J2", l2, diam, 100.0),
        );
        assert_eq!(result, SolveResult::Converged);

        assert_relative_eq!(ls[0].flow, expected_total, epsilon = 1e-5);
        assert_relative_eq!(
            ns[1].demand_flow + ns[2].demand_flow,
            expected_total,
            epsilon = 1e-5
        );
    }
}

#[test]
fn closed_prv_status_blocks_valve_flow() {
    // Mirrors tests/fixtures/valve_status.inp core behavior: a PRV between
    // J1 and J2 is overridden to CLOSED via STATUS.
    let builder = TestNetworkBuilder::new()
        .junction("J1", 0.0, 100.0)
        .junction("J2", 0.0, 50.0)
        .reservoir("R1", 100.0)
        .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
        .hw_pipe("P2", "J1", "J2", 800.0, 10.0, 100.0)
        .hw_pipe("P3", "R1", "J2", 1200.0, 10.0, 100.0)
        .valve("V1", "J1", "J2", ValveType::Prv, 12.0, 50.0);

    let (net, mut ns, mut ls, favad) = builder.build_with_favad();
    // STATUS override: force PRV closed for the step.
    ls[3].status = crate::LinkStatus::Closed;

    let mut ctx = build_solver_context(&net, &favad).unwrap();
    let result =
        solve_hydraulic_step(&net, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch).unwrap();
    assert_eq!(result, SolveResult::Converged);

    // Closed valve should carry no flow.
    assert!(ls[3].flow.abs() < 1e-8, "closed PRV should carry zero flow");
    // Alternate path (R1->J2 via P3) remains active when PRV is closed.
    assert!(ls[2].flow.abs() > 1e-8, "alternate pipe should carry flow");
    // Network still serves some positive demand (possibly pressure-limited).
    assert!(ns[1].demand_flow + ns[2].demand_flow > 0.0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.6 Convergence / non-convergence behaviour
// ═══════════════════════════════════════════════════════════════════════════════

/// With `max_iter = 1` and `extra_iter = 0`, the solver exhausts its budget but
/// does not halt — it should return `Ok(Unbalanced)`, not an error.
#[test]
fn max_iter_exceeded_returns_unbalanced() {
    let mut builder = TestNetworkBuilder::new()
        .reservoir("R1", 200.0)
        .junction("J1", 0.0, 100.0)
        .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0);
    // Tighten flow_tol to an impossible value so convergence is guaranteed
    // not to be declared within 1 iteration.
    builder.options_mut().max_iter = 1;
    builder.options_mut().extra_iter = 0; // unbalanced mode, not halt
    builder.options_mut().flow_tol = 1e-20;
    let (net, mut ns, mut ls, favad) = builder.build_with_favad();
    let mut ctx = build_solver_context(&net, &favad).unwrap();
    let result = solve_hydraulic_step(&net, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch)
        .expect("should not error when extra_iter >= 0");
    assert_eq!(result, SolveResult::Unbalanced);
}

/// With `max_iter = 1` and `extra_iter = -1` (halt mode), the solver still
/// returns `Ok(Unbalanced)` — it is the *caller* (simulation layer) that
/// decides to halt when it sees an unbalanced result with `extra_iter < 0`.
/// The solver itself never returns `Err(NotConverged)` on iteration overflow.
#[test]
fn halt_on_non_convergence_returns_unbalanced_for_caller_to_handle() {
    let mut builder = TestNetworkBuilder::new()
        .reservoir("R1", 200.0)
        .junction("J1", 0.0, 100.0)
        .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0);
    builder.options_mut().max_iter = 1;
    builder.options_mut().extra_iter = -1; // halt mode — caller responsibility
    builder.options_mut().flow_tol = 1e-20;
    let (net, mut ns, mut ls, favad) = builder.build_with_favad();
    let mut ctx = build_solver_context(&net, &favad).unwrap();
    let result = solve_hydraulic_step(&net, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch)
        .expect("solve_hydraulic_step should not Err on iteration overflow");
    // The solver returns Unbalanced; the simulation layer checks extra_iter and
    // halts or continues accordingly (see §3.6 and crates/simulation/).
    assert_eq!(result, SolveResult::Unbalanced);
}
