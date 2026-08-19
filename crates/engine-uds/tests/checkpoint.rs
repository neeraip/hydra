//! §12.3: a run restored from a checkpoint continues bit-identically to
//! one that was never interrupted.
//!
//! That property is the contract, and it is the only thing that can
//! establish it: an omitted state does not fail, it continues from a
//! default and produces plausible results. Each test here therefore also
//! asserts that the state it covers is non-trivial at the checkpoint
//! instant, because a property test over a model at rest proves nothing.

use hydra_engine_uds::simulation::engine::Simulation;

/// Routed inflow through two channels with a storage vertex between, so
/// depths, flows and the storage's own losses are all in motion when the
/// checkpoint is taken.
const MODEL: &str = "\
[OPTIONS]
FLOW_UNITS           CMS
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/2020
START_TIME           00:00:00
END_DATE             01/01/2020
END_TIME             02:00:00
ROUTING_STEP         00:00:10
REPORT_STEP          00:05:00

[JUNCTIONS]
J1  10  4  0  0  0
J2  9   4  0  0  0

[STORAGE]
S1  8  5  0  FUNCTIONAL  100  0  0

[OUTFALLS]
O1  6  FREE  NO

[CONDUITS]
C1  J1  J2  400  0.013  0  0  0  0
C2  J2  S1  400  0.013  0  0  0  0
C3  S1  O1  200  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  1.5  0  0  0  1
C2  CIRCULAR  1.5  0  0  0  1
C3  CIRCULAR  1.0  0  0  0  1

[INFLOWS]
J1  FLOW  TS1

[TIMESERIES]
TS1  0:00  0.0
TS1  0:15  1.2
TS1  0:45  1.2
TS1  1:00  0.0

[REPORT]
";

/// Run the whole thing, and run it again in two halves across a
/// checkpoint. The results must not differ by a bit.
#[test]
fn a_restored_run_continues_bit_identically() {
    let (mut whole, _, _) = Simulation::open(MODEL).expect("open");
    whole.run();
    let want = every_output(&whole);

    // Stop mid-storm, where depths, flows and the head history are all
    // non-trivial: a checkpoint taken at rest would pass whatever it lost.
    let (mut first, _, _) = Simulation::open(MODEL).expect("open");
    while first.report().elapsed < 1_800.0 {
        if !first.step() {
            panic!("the run ended before the checkpoint instant");
        }
    }
    let mut cp = Vec::new();
    first.save_checkpoint(&mut cp).expect("checkpoint");
    assert!(!cp.is_empty(), "an empty checkpoint proves nothing");

    let (mut second, _, _) = Simulation::open(MODEL).expect("open");
    second.load_checkpoint(&cp).expect("restore");
    second.run();
    let got = every_output(&second);

    assert_eq!(
        want.results.len(),
        got.results.len(),
        "the restored run produced a different number of periods"
    );
    assert!(
        want.results == got.results,
        "the restored run's results file diverged"
    );
    // The statistics report is a second surface over the same state, and
    // one the results file says nothing about. Comparing only the results
    // left every §11.2 statistic free to be lost: zeroing `vertex_stats`
    // on restore passed the first version of this test.
    assert_eq!(
        String::from_utf8_lossy(&want.report),
        String::from_utf8_lossy(&got.report),
        "the restored run's report diverged"
    );
    assert_eq!(want.notices, got.notices, "the notices diverged");
    assert_eq!(want.ledgers, got.ledgers, "the ledgers diverged");
}

/// Everything a completed run can be asked for. A checkpoint is only as
/// good as the surface it is compared on, and there is more than one.
struct Outputs {
    results: Vec<u8>,
    report: Vec<u8>,
    notices: Vec<String>,
    ledgers: Vec<String>,
}

fn every_output(sim: &Simulation) -> Outputs {
    let mut results = Vec::new();
    sim.write_out(&mut results).expect("results");
    let mut report = Vec::new();
    sim.write_report(&mut report).expect("report");
    let led = sim.ledgers();
    Outputs {
        results,
        report,
        notices: sim
            .notices
            .iter()
            .map(|n| format!("{}: {}", n.t, n.message))
            .collect(),
        ledgers: vec![format!("{:?}", led.network), format!("{:?}", led.surface)],
    }
}

/// The checkpoint instant must be one where the state is worth carrying,
/// or the test above passes on an empty river.
#[test]
fn the_checkpoint_instant_is_not_a_model_at_rest() {
    let (mut sim, _, _) = Simulation::open(MODEL).expect("open");
    while sim.report().elapsed < 1_800.0 {
        assert!(sim.step(), "the run ended early");
    }
    let led = sim.ledgers();
    assert!(led.network.inflow > 0.0, "no inflow had arrived");
    let snap = sim.snapshots.last().expect("a reporting instant");
    assert!(
        snap.depths.iter().any(|d| *d > 0.01),
        "every vertex was dry, so the checkpoint carried nothing"
    );
    assert!(
        snap.flows.iter().any(|q| q.abs() > 0.01),
        "every channel was still, so the checkpoint carried nothing"
    );
}

/// A checkpoint from another model is refused rather than restored onto
/// whatever happens to line up.
#[test]
fn a_checkpoint_from_another_model_is_refused() {
    let (mut a, _, _) = Simulation::open(MODEL).expect("open");
    a.run();
    let mut cp = Vec::new();
    a.save_checkpoint(&mut cp).expect("checkpoint");

    // Renamed, not reshaped: the counts still match, which is the whole
    // of the check the predecessor makes.
    let renamed = MODEL.replace("J2", "JX");
    let (mut b, _, _) = Simulation::open(&renamed).expect("open");
    let err = b.load_checkpoint(&cp).expect_err("a different model");
    assert!(err.contains("different model"), "{err}");
}

/// Reordering alone is refused, which counts cannot catch.
#[test]
fn a_reordered_model_is_refused() {
    let (mut a, _, _) = Simulation::open(MODEL).expect("open");
    a.run();
    let mut cp = Vec::new();
    a.save_checkpoint(&mut cp).expect("checkpoint");

    let swapped = MODEL.replace(
        "J1  10  4  0  0  0\nJ2  9   4  0  0  0",
        "J2  9   4  0  0  0\nJ1  10  4  0  0  0",
    );
    assert_ne!(swapped, MODEL, "the fixture must actually reorder");
    let (mut b, _, _) = Simulation::open(&swapped).expect("open");
    let err = b.load_checkpoint(&cp).expect_err("a reordered model");
    assert!(err.contains("reordered"), "{err}");
}

/// A model whose state is not yet carried is refused by name.
#[test]
fn a_model_whose_state_is_not_carried_is_refused() {
    let with_parcel = MODEL.replace(
        "[JUNCTIONS]",
        "[RAINGAGES]
G1  INTENSITY  0:15  1.0  TIMESERIES  TS1

[SUBCATCHMENTS]
S9  G1  J1  10  50  500  0.01  0

[SUBAREAS]
S9  0.01  0.10  0.05  0.05  25  OUTLET

[INFILTRATION]
S9  3.0  0.5  4  7  0

[JUNCTIONS]",
    );
    let (sim, _, _) = Simulation::open(&with_parcel).expect("open");
    let mut cp = Vec::new();
    let err = sim.save_checkpoint(&mut cp).expect_err("surface state");
    assert!(err.contains("surface state"), "{err}");
    assert!(cp.is_empty(), "nothing may be written when it is refused");
}

/// A truncated checkpoint is refused rather than read short.
#[test]
fn a_truncated_checkpoint_is_refused() {
    let (mut a, _, _) = Simulation::open(MODEL).expect("open");
    a.run();
    let mut cp = Vec::new();
    a.save_checkpoint(&mut cp).expect("checkpoint");
    cp.truncate(cp.len() / 2);

    let (mut b, _, _) = Simulation::open(MODEL).expect("open");
    let err = b.load_checkpoint(&cp).expect_err("a short file");
    assert!(err.contains("ends after"), "{err}");
}

/// The same property with reporting starting late, which is a different
/// code path: the statistics window opens after the run does, so the
/// instant it opens is itself state.
///
/// Two pieces of router state are written but *not* covered by either
/// model here, and saying so is better than implying they are: the
/// mid-step flow areas, which every step recomputes before reading, and
/// the quiet-period streak, which needs a model that goes quiet. Both are
/// carried; neither has a test that would notice if they stopped being.
#[test]
fn a_restored_run_continues_bit_identically_when_reporting_starts_late() {
    let late = MODEL.replace(
        "REPORT_STEP          00:05:00",
        "REPORT_STEP          00:05:00\nREPORT_START_TIME    00:40:00",
    );
    assert_ne!(
        late, MODEL,
        "the fixture must actually move the report start"
    );

    let (mut whole, _, _) = Simulation::open(&late).expect("open");
    whole.run();
    let want = every_output(&whole);

    let (mut first, _, _) = Simulation::open(&late).expect("open");
    while first.report().elapsed < 3_000.0 {
        assert!(first.step(), "the run ended before the checkpoint instant");
    }
    // Past the report start, so the statistics window is already open and
    // a checkpoint that forgot when it opened would reopen it here.
    assert!(
        !first.snapshots.is_empty(),
        "no reporting instant had passed, so this proves nothing"
    );
    let mut cp = Vec::new();
    first.save_checkpoint(&mut cp).expect("checkpoint");

    let (mut second, _, _) = Simulation::open(&late).expect("open");
    second.load_checkpoint(&cp).expect("restore");
    second.run();
    let got = every_output(&second);

    assert!(want.results == got.results, "the results file diverged");
    assert_eq!(
        String::from_utf8_lossy(&want.report),
        String::from_utf8_lossy(&got.report),
        "the report diverged"
    );
}

/// Checkpointing *before* the reporting window opens, which is the only
/// way the instant it opens can be seen to matter.
///
/// Restored after the window has opened, a lost opening instant changes
/// nothing: the window is open either way. Restored before, it opens
/// immediately and every statistic then covers a longer run than it
/// should. The test above cannot tell the difference; this one can.
#[test]
fn a_run_restored_before_reporting_opens_still_opens_it_on_time() {
    let late = MODEL.replace(
        "REPORT_STEP          00:05:00",
        "REPORT_STEP          00:05:00\nREPORT_START_TIME    00:40:00",
    );
    let (mut whole, _, _) = Simulation::open(&late).expect("open");
    whole.run();
    let want = every_output(&whole);

    let (mut first, _, _) = Simulation::open(&late).expect("open");
    while first.report().elapsed < 600.0 {
        assert!(first.step(), "the run ended before the checkpoint instant");
    }
    assert!(
        first.snapshots.is_empty(),
        "reporting had already opened, so this is the other test"
    );
    let mut cp = Vec::new();
    first.save_checkpoint(&mut cp).expect("checkpoint");

    let (mut second, _, _) = Simulation::open(&late).expect("open");
    second.load_checkpoint(&cp).expect("restore");
    second.run();
    let got = every_output(&second);

    assert!(want.results == got.results, "the results file diverged");
    assert_eq!(
        String::from_utf8_lossy(&want.report),
        String::from_utf8_lossy(&got.report),
        "the report diverged"
    );
}
