//! Integration tests for the §5.3 tank predictor–corrector.
//!
//! One filling-tank network, exercised through the public session API under
//! different `level_err_tol` settings. The reference trajectory is the same
//! model integrated at a 5 s hydraulic step with the corrector disabled —
//! fine enough that first-order error is negligible against the differences
//! the assertions measure.

use hydra_engine_wds::{io, NodeQuantity, Simulation, WarningKind};

/// Reservoir at 150 ft filling a 40 ft-diameter tank through a long 8-inch
/// pipe: a smooth exponential-approach trajectory with visibly curving net
/// flow over a 1 h step, so a single Euler step carries measurable error.
/// The tank band (5–100 ft) is wide enough that no boundary clamp fires.
const TANK_INP: &str = "
[TITLE]
tank corrector fixture

[RESERVOIRS]
;ID    Head
 R1    150

[TANKS]
;ID  Elev  InitLvl  MinLvl  MaxLvl  Diam  MinVol
 T1  0     20       5       100     40    0

[PIPES]
;ID  Node1  Node2  Length  Diam  Rough
 P1  R1     T1     2000    8     100

[TIMES]
 DURATION            4:00
 HYDRAULIC TIMESTEP  1:00
 REPORT TIMESTEP     1:00

[OPTIONS]
 UNITS     GPM
 HEADLOSS  H-W

[END]
";

/// Tank level (head, m internal) at the end of the run.
fn final_tank_head(sim: &Simulation) -> f64 {
    let t_end = *sim.snapshot_times().last().expect("snapshots");
    sim.get_node_result("T1", NodeQuantity::Head, t_end)
        .expect("tank head")
}

fn run_with(level_err_tol: f64, hyd_step: Option<f64>) -> Simulation {
    let mut network = io::parse(TANK_INP.as_bytes()).expect("fixture parses");
    network.options.level_err_tol = level_err_tol;
    if let Some(step) = hyd_step {
        network.options.hyd_step = step;
    }
    let mut sim = Simulation::from_network(network).expect("loads");
    sim.run().expect("runs");
    sim
}

#[test]
fn the_corrector_beats_euler_against_a_fine_step_reference() {
    // Reference: 5 s Euler — first-order error at this step is ~µm scale.
    let reference = final_tank_head(&run_with(0.0, Some(5.0)));
    // Coarse Euler (corrector disabled) vs coarse corrected, both at 1 h.
    let euler = final_tank_head(&run_with(0.0, None));
    let corrected = final_tank_head(&run_with(1.0e-3, None));

    let euler_err = (euler - reference).abs();
    let corrected_err = (corrected - reference).abs();
    assert!(
        euler_err > 1.0e-4,
        "fixture too easy: coarse Euler error {euler_err:.3e} m is below \
         measurable range, so the comparison below would prove nothing"
    );
    assert!(
        corrected_err < euler_err / 5.0,
        "corrector error {corrected_err:.3e} m is not clearly better than \
         Euler error {euler_err:.3e} m against the fine-step reference"
    );
}

#[test]
fn disabling_error_control_reproduces_first_order_euler_exactly() {
    // level_err_tol = 0 must be the predecessor path: a single Euler step
    // V1 = V0 + Q0·Δt, verifiable by hand from the t=0 solve.
    let mut network = io::parse(TANK_INP.as_bytes()).expect("fixture parses");
    network.options.level_err_tol = 0.0;
    let mut sim = Simulation::from_network(network).expect("loads");

    let q0 = {
        // One step; a tank's Demand result is its net inflow — the exact
        // quantity the Euler update integrates, not the pipe flow, which
        // matches it only to solver tolerance.
        sim.step_hydraulics().expect("step");
        sim.get_node_result("T1", NodeQuantity::Demand, 0.0)
            .expect("tank net inflow at t=0")
    };
    let h0 = sim
        .get_node_result("T1", NodeQuantity::Head, 0.0)
        .expect("tank head at t=0");
    let h1 = sim
        .get_node_result("T1", NodeQuantity::Head, 3600.0)
        .or_else(|_| {
            // Head at 3600 s is not yet snapshotted until the next step
            // records it; step once more and re-read.
            sim.step_hydraulics().expect("step 2");
            sim.get_node_result("T1", NodeQuantity::Head, 3600.0)
        })
        .expect("tank head at 1 h");

    // Cylindrical tank: Δh = Q·Δt / A, A = π(D/2)². The 40 ft diameter is
    // converted with the engine's own elevation factor (tank diameters share
    // it — they are not pipe diameters), so the hand model uses exactly the
    // geometry the engine integrates.
    let ucf = io::units::make_ucf(hydra_engine_wds::FlowUnits::Gpm, 1.0);
    let d_m = 40.0 / ucf.elev;
    let area = std::f64::consts::PI * (d_m / 2.0) * (d_m / 2.0);
    let expected = h0 + q0 * 3600.0 / area;
    assert!(
        (h1 - expected).abs() < 1.0e-9,
        "disabled path is not plain Euler: got {h1}, hand-computed {expected}"
    );
}

#[test]
fn a_network_without_tanks_is_untouched_by_the_error_control() {
    const TANKLESS: &str = "
[TITLE]
no tanks

[RESERVOIRS]
 R1    150
 R2    100

[JUNCTIONS]
;ID  Elev  Demand
 J1  50    100

[PIPES]
 P1  R1  J1  2000  8  100
 P2  J1  R2  2000  8  100

[TIMES]
 DURATION            2:00
 HYDRAULIC TIMESTEP  1:00

[OPTIONS]
 UNITS     GPM
 HEADLOSS  H-W

[END]
";
    let run = |tol: f64| {
        let mut network = io::parse(TANKLESS.as_bytes()).expect("parses");
        network.options.level_err_tol = tol;
        let mut sim = Simulation::from_network(network).expect("loads");
        sim.run().expect("runs");
        let t_end = *sim.snapshot_times().last().expect("snapshots");
        sim.get_node_result("J1", NodeQuantity::Head, t_end)
            .expect("head")
    };
    // Bit-identical, not approximately equal: with no differential state the
    // corrector must not run at all.
    assert_eq!(run(1.0e-3), run(0.0));
}

#[test]
fn an_unachievable_tolerance_bottoms_out_at_the_floor_and_says_so() {
    // 1 fm of level accuracy is unachievable at any step this network can
    // take: every period collapses to the 1 s floor, is accepted anyway, and
    // carries the degraded-accuracy warning naming the tank.
    let mut network = io::parse(TANK_INP.as_bytes()).expect("fixture parses");
    network.options.level_err_tol = 1.0e-15;
    // Short duration: at a 1 s floor a full run is 14 400 steps; 60 s keeps
    // the test fast while still crossing several rejected periods.
    network.options.duration = 60.0;
    let mut sim = Simulation::from_network(network).expect("loads");

    // First period: rejection halves 3600 s down to the floor.
    let dt1 = sim.step_hydraulics().expect("step");
    assert!(
        (dt1 - 1.0).abs() < 1.0e-12,
        "expected the 1 s floor, got {dt1}"
    );
    // §5.2: the period after a rejection-laden one is capped at twice the
    // accepted interval, not re-attempted at the full nominal step.
    let dt2 = sim.step_hydraulics().expect("step");
    assert!(
        dt2 <= 2.0 + 1.0e-12,
        "post-rejection cap not applied: {dt2}"
    );

    sim.run_hydraulics().expect("rest of run");
    assert!(
        sim.warnings()
            .iter()
            .any(|w| matches!(w.kind, WarningKind::TankLevelAccuracy { .. })),
        "no degraded-accuracy warning despite an unachievable tolerance"
    );
}
