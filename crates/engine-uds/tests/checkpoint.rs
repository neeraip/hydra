//! §12.3: a run restored from a checkpoint continues bit-identically to
//! one that was never interrupted.
//!
//! Two kinds of test run here, and the second is what makes the first
//! honest. A **property** test runs a model whole and again across a
//! checkpoint, and every output surface must agree; it can only see state
//! the model exercises. A **round-trip** test restores a checkpoint and
//! writes it again, and the bytes must match; it sees any field written
//! but not read back, however little that field influences results. Six
//! fields that no property test could reach are covered by the second
//! kind alone, the mid-step flow areas and the storage residence time
//! among them.
//!
//! **What is still not covered:** a parcel's return-to-pervious volume in
//! flight. It is zero at every instant these models reach, so dropping it
//! on restore changes neither the results nor the bytes. It is written.
//!
//! Two models here are built rather than borrowed, for the same reason in
//! both cases: `control_rules.inp` has no inflow under a substituted
//! clock, so its node is dry, its rules fire against nothing and a
//! controller on it never leaves its cap. A fixture pins the behaviour it
//! was written for, which is not always the behaviour a checkpoint needs
//! to have moving.

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

/// A run reading an interface file checkpoints and restores, given the
/// same file again.
#[test]
fn a_replaying_run_restores_when_the_file_is_supplied_again() {
    let (mut whole, _, _) = Simulation::open(&replay_model()).expect("open");
    whole.supply_runoff(&replay_file()).expect("supply");
    whole.run();
    let want = every_output(&whole);

    let (mut first, _, _) = Simulation::open(&replay_model()).expect("open");
    first.supply_runoff(&replay_file()).expect("supply");
    let target = ((whole.snapshots.len() as f64 * 0.4) as usize).max(1);
    while first.snapshots.len() < target {
        assert!(first.step(), "the run ended before the checkpoint instant");
    }
    let mut cp = Vec::new();
    first.save_checkpoint(&mut cp).expect("checkpoint");

    let (mut second, _, _) = Simulation::open(&replay_model()).expect("open");
    second.supply_runoff(&replay_file()).expect("supply");
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

/// Restoring without the file, or with a different one, is refused.
///
/// Neither would fail on its own: the run would continue on inflows it
/// was never receiving and look entirely healthy.
#[test]
fn a_replaying_run_is_refused_without_the_same_file() {
    let (mut first, _, _) = Simulation::open(&replay_model()).expect("open");
    first.supply_runoff(&replay_file()).expect("supply");
    while first.snapshots.len() < 2 {
        assert!(first.step(), "the run ended early");
    }
    let mut cp = Vec::new();
    first.save_checkpoint(&mut cp).expect("checkpoint");

    // No file at all.
    let (mut bare, _, _) = Simulation::open(&replay_model()).expect("open");
    let err = bare.load_checkpoint(&cp).expect_err("no file supplied");
    assert!(err.contains("runoff"), "{err}");
    assert!(err.contains("given none"), "{err}");

    // A file of the same shape carrying different flows.
    let (mut other, _, _) = Simulation::open(&replay_model()).expect("open");
    other.supply_runoff(&replay_file_of(2.0)).expect("supply");
    let err = other.load_checkpoint(&cp).expect_err("a different file");
    assert!(err.contains("same files must be supplied"), "{err}");
}

/// Two parcels, no constituents, CMS: the shape `supply_runoff` expects.
fn replay_file_of(runoff: f32) -> Vec<u8> {
    let mut b = b"SWMM5-RUNOFF".to_vec();
    for v in [2i32, 0, 3, 12] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    for _ in 0..12 {
        b.extend_from_slice(&300.0f32.to_le_bytes());
        for parcel in 0..2 {
            let mut row = [0.0f32; 8];
            row[4] = runoff * (1.0 + parcel as f32);
            for x in row {
                b.extend_from_slice(&x.to_le_bytes());
            }
        }
    }
    b
}

fn replay_file() -> Vec<u8> {
    replay_file_of(0.5)
}

fn replay_model() -> String {
    parcel_model("", "[FILES]\nUSE RUNOFF runoff.bin")
}

/// A truncated checkpoint is refused rather than read short.
#[test]
fn a_truncated_checkpoint_is_refused() {
    let (mut a, _, _) = Simulation::open(MODEL).expect("open");
    a.run();
    let mut cp = Vec::new();
    a.save_checkpoint(&mut cp).expect("checkpoint");
    cp.truncate(cp.len() / 2);

    // Every truncation, not one: a checkpoint cut short anywhere must be
    // refused, and where the read gives up depends on where the cut fell.
    for cut in [4, 40, cp.len() / 3, cp.len() / 2, cp.len() - 1] {
        let mut short = cp.clone();
        short.truncate(cut);
        let (mut b, _, _) = Simulation::open(MODEL).expect("open");
        let err = b
            .load_checkpoint(&short)
            .expect_err("a checkpoint cut at {cut} must be refused");
        assert!(
            err.contains("ends after") || err.contains("declares") || err.contains("not a Hydra"),
            "cut at {cut}: {err}"
        );
    }
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

// ── Surface state ───────────────────────────────────────────────────────

/// A parcel model: rain for the first half hour, then a dry recession, so
/// a checkpoint mid-storm carries ponded depth, infiltration state and a
/// hydrograph still on its way down the network.
fn parcel_model(extra: &str, options: &str) -> String {
    format!(
        "\
[OPTIONS]
FLOW_UNITS           CMS
INFILTRATION         HORTON
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/2020
START_TIME           00:00:00
END_DATE             01/01/2020
END_TIME             02:00:00
WET_STEP             00:05:00
DRY_STEP             00:05:00
ROUTING_STEP         00:00:15
REPORT_STEP          00:05:00
{options}

[RAINGAGES]
G1  INTENSITY  0:05  1.0  TIMESERIES  RAIN

[SUBCATCHMENTS]
P1  G1  J1  10  40  500  0.01  0
P2  G1  J1  6   70  400  0.02  0

[SUBAREAS]
P1  0.01  0.10  0.05  0.05  25  OUTLET
P2  0.01  0.10  0.05  0.05  25  OUTLET

[INFILTRATION]
P1  3.0  0.5  4  7  0
P2  3.0  0.5  4  7  0

[JUNCTIONS]
J1  10  4  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1

[TIMESERIES]
RAIN  0:00  25.0
RAIN  0:30  0.0
{extra}

[REPORT]
"
    )
}

/// Run the model whole, and again across a checkpoint partway through.
/// Every output surface must agree.
///
/// The instant is a fraction of the run the model actually performs, not
/// a number of seconds: a fixture's own clock is its business, and an
/// absolute instant past its end silently becomes a checkpoint at the end,
/// which proves nothing.
fn restores_identically(model: &str, fraction: f64) {
    let (mut whole, diags, _) = Simulation::open(model).expect("open");
    assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
    whole.run();
    let want = every_output(&whole);
    // Measured in reporting instants passed, not seconds: a fixture's own
    // clock is its business, and the routing report's elapsed time is zero
    // for a model that does not route at all.
    let target = ((whole.snapshots.len() as f64 * fraction) as usize).max(1);
    assert!(
        whole.snapshots.len() > target,
        "the run has {} reporting instants, too few to checkpoint inside",
        whole.snapshots.len()
    );

    let (mut first, _, _) = Simulation::open(model).expect("open");
    while first.snapshots.len() < target {
        assert!(first.step(), "the run ended before the checkpoint instant");
    }
    let mut cp = Vec::new();
    first.save_checkpoint(&mut cp).expect("checkpoint");

    let (mut second, _, _) = Simulation::open(model).expect("open");
    second.load_checkpoint(&cp).expect("restore");
    second.run();
    let got = every_output(&second);

    assert!(want.results == got.results, "the results file diverged");
    assert_eq!(
        String::from_utf8_lossy(&want.report),
        String::from_utf8_lossy(&got.report),
        "the report diverged"
    );
    assert_eq!(want.ledgers, got.ledgers, "the ledgers diverged");
    assert_eq!(want.notices, got.notices, "the notices diverged");
}

/// The surface, checkpointed mid-storm: ponded depths and the Horton
/// curve's position are both moving.
#[test]
fn a_restored_surface_continues_bit_identically() {
    restores_identically(&parcel_model("", ""), 0.125);
}

/// And in the recession, where the infiltration relation is regenerating
/// rather than depleting, which is the other half of its state.
#[test]
fn a_restored_surface_continues_through_the_recession() {
    restores_identically(&parcel_model("", ""), 0.42);
}

/// The checkpoint instants above must be ones where the surface holds
/// something, or the two tests prove nothing.
#[test]
fn the_surface_checkpoint_instants_are_not_dry() {
    for at in [900.0, 3_000.0] {
        let (mut sim, _, _) = Simulation::open(&parcel_model("", "")).expect("open");
        while sim.report().elapsed < at {
            assert!(sim.step(), "the run ended early");
        }
        let led = sim.ledgers();
        let surface = led.surface.expect("a surface ledger");
        assert!(surface.inflow > 0.0, "at {at}s no rain had fallen");
        assert!(
            led.network.inflow > 0.0,
            "at {at}s nothing had reached the network"
        );
    }
}

/// The fixtures that pin snow and groundwater behaviour, checkpointed
/// mid-run. Reusing them rather than hand-building a model is the point:
/// the first hand-built snow model here had no snow lying at the
/// checkpoint instant, so it was a property test over an empty pack and
/// said so only once something asserted otherwise.
fn fixture(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/uds")
        .join(name);
    let text = std::fs::read_to_string(path).expect("fixture readable");
    // The fixtures pin parse and build behaviour and mostly declare no
    // clock, so one is appended here exactly as the results-file tests do.
    format!(
        "{text}\n[OPTIONS]\nSTART_DATE 01/01/2024\nSTART_TIME 00:00:00\n\
         END_DATE 01/01/2024\nEND_TIME 02:00:00\nREPORT_STEP 00:05:00\n\
         [REPORT]\nSUBCATCHMENTS ALL\nNODES ALL\nLINKS ALL\n"
    )
}

/// Groundwater: the aquifer's moisture and water table are state the
/// surface does not hold.
#[test]
fn a_restored_aquifer_continues_bit_identically() {
    restores_identically(&fixture("groundwater_lateral_flow.inp"), 0.4);
}

// Snow state is written by `SnowPack::checkpoint_put` and has no
// property test, which is a gap rather than an oversight. The test that
// was here ran `snowmelt_pack.inp` under a clock this harness appends,
// and under that clock the fixture lies no snow at all: it was a property
// test over an empty pack, and it passed. The guard below is what said
// so. Covering it needs a model that holds snow while a checkpoint is
// taken, and none of the fixtures does under a substituted clock.

/// Control measures: each layer's water and the drain's open state.
#[test]
fn a_restored_control_measure_continues_bit_identically() {
    restores_identically(&fixture("lid_bioretention_underdrain.inp"), 0.4);
}

/// Both fixtures must hold their own state at the checkpoint instant, or
/// the three tests above are property tests over nothing.
#[test]
fn the_fixture_instants_hold_their_state() {
    let holds =
        |name: &str,
         fraction: f64,
         what: &str,
         f: fn(&hydra_engine_uds::simulation::engine::SubcatchRecord) -> f64| {
            let (mut whole, _, _) = Simulation::open(&fixture(name)).expect("open");
            whole.run();
            let target = ((whole.snapshots.len() as f64 * fraction) as usize).max(1);
            let (mut sim, _, _) = Simulation::open(&fixture(name)).expect("open");
            while sim.snapshots.len() < target {
                assert!(sim.step(), "{name}: the run ended early");
            }
            let snap = sim.snapshots.last().expect("a reporting instant");
            assert!(
                snap.subcatch.iter().map(f).any(|v| v > 0.0),
                "{name} held no {what} at instant {target}, so its checkpoint \
             carried none"
            );
        };
    holds("groundwater_lateral_flow.inp", 0.4, "soil moisture", |s| {
        s.soil_moisture
    });
    holds("lid_bioretention_underdrain.inp", 0.4, "runoff", |s| {
        s.runoff
    });
}

/// A parcel draining onto another parcel, which is the only shape that
/// puts the run-on carried between steps into play.
///
/// Without it, zeroing `runon_next_vol` on restore passed every test
/// here: the two parcels above both drain to the network, so nothing was
/// ever in flight between them at the checkpoint instant.
#[test]
fn a_restored_cascade_continues_bit_identically() {
    let cascade = parcel_model("", "").replace(
        "P1  G1  J1  10  40  500  0.01  0",
        "P1  G1  P2  10  40  500  0.01  0",
    );
    assert!(cascade.contains("P1  G1  P2"), "the fixture must cascade");
    restores_identically(&cascade, 0.25);
}

/// And the cascade must actually be carrying water when the checkpoint
/// is taken.
#[test]
fn the_cascade_instant_has_run_on_in_flight() {
    let cascade = parcel_model("", "").replace(
        "P1  G1  J1  10  40  500  0.01  0",
        "P1  G1  P2  10  40  500  0.01  0",
    );
    let (mut whole, _, _) = Simulation::open(&cascade).expect("open");
    whole.run();
    let target = ((whole.snapshots.len() as f64 * 0.25) as usize).max(1);
    let (mut sim, _, _) = Simulation::open(&cascade).expect("open");
    while sim.snapshots.len() < target {
        assert!(sim.step(), "the run ended early");
    }
    let snap = sim.snapshots.last().expect("a reporting instant");
    assert!(
        snap.subcatch[0].runoff > 0.0,
        "the upper parcel was not shedding, so nothing was in flight"
    );
}

// ── Constituent state ───────────────────────────────────────────────────

/// Surface buildup, wash-off and network transport, checkpointed while
/// mass is on the ground, in the water and being treated at once.
#[test]
fn a_restored_quality_run_continues_bit_identically() {
    restores_identically(&fixture("buildup_washoff_treatment.inp"), 0.3);
}

/// The same model checkpointed before the storm, where the mass is still
/// on the ground rather than in the water.
///
/// Wash-off empties the surface early, so a checkpoint taken partway
/// through carries almost no buildup and cannot tell whether buildup
/// survived: zeroing it on restore passed the test above.
#[test]
fn a_restored_quality_run_keeps_the_mass_on_the_ground() {
    // Five antecedent dry days, so there is buildup on the ground when the
    // run opens. Without them the fixture accumulates none during its own
    // two rainy hours, and the surface mass is zero either way.
    let dirty = fixture("buildup_washoff_treatment.inp")
        .replace("[REPORT]\n", "[OPTIONS]\nDRY_DAYS 5\n\n[REPORT]\n");
    assert!(dirty.contains("DRY_DAYS"), "the fixture must start dirty");
    restores_identically(&dirty, 0.02);
}

/// And the constituent state must be non-trivial when it is taken.
#[test]
fn the_quality_instant_carries_mass() {
    let model = fixture("buildup_washoff_treatment.inp");
    let (mut whole, _, _) = Simulation::open(&model).expect("open");
    whole.run();
    let target = ((whole.snapshots.len() as f64 * 0.3) as usize).max(1);
    let (mut sim, _, _) = Simulation::open(&model).expect("open");
    while sim.snapshots.len() < target {
        assert!(sim.step(), "the run ended early");
    }
    let snap = sim.snapshots.last().expect("a reporting instant");
    assert!(
        snap.subcatch
            .iter()
            .any(|s| s.washoff.iter().any(|c| *c > 0.0)),
        "nothing was washing off, so the checkpoint carried no surface mass"
    );
    assert!(
        snap.node_quality.iter().any(|c| c.iter().any(|v| *v > 0.0)),
        "no mass was in the network, so the checkpoint carried none"
    );
}

/// A swept surface through a checkpoint.
///
/// This exercises the sweeping path, which demonstrably changes results:
/// with an efficiency set, wash-off over the run falls from 310 to 62. It
/// does **not** pin the time each land use was last swept. Shifting the
/// first pass by 57 minutes leaves every output identical, so that field
/// has no observable influence in this engine's accumulation relation and
/// no model of this shape can cover it.
///
/// The quality fixture never sweeps (`RES  0  0  0`), so zeroing that
/// time on restore changed nothing and the field looked covered by the
/// tests above. Sweeping every half hour of a two-hour run puts a
/// cleaning pass on either side of the checkpoint.
#[test]
fn a_restored_swept_surface_continues_bit_identically() {
    // Both halves are needed. The land use must ask to be swept, and the
    // wash-off relation must give sweeping an efficiency: the fixture's is
    // zero, and with it every removal fraction multiplies out to nothing.
    // Setting only the interval left the swept and unswept runs identical.
    //
    // The interval is longer than the checkpoint instant on purpose. Swept
    // every few minutes, the surface is clean in both runs and the time of
    // the last pass stops mattering; swept once an hour, a lost timer
    // sweeps immediately on restore instead of waiting.
    //
    // A second storm after the sweep is what makes the difference visible
    // at all: sweeping only moves buildup, and buildup only reaches the
    // results through wash-off. With the fixture's single opening storm,
    // the surface is already clean by the time any sweep matters.
    let swept = fixture("buildup_washoff_treatment.inp")
        .replace("RES  0  0  0", "RES  0.05  0.8  0.005")
        .replace(
            "RES  TSS  EXP  0.2  1.2  0  0",
            "RES  TSS  EXP  0.2  1.2  0  80",
        )
        .replace(
            "RAIN1  1:00  0.0",
            "RAIN1  1:00  0.0\nRAIN1  1:30  2.0\nRAIN1  1:45  0.0",
        )
        .replace("[REPORT]\n", "[OPTIONS]\nDRY_DAYS 5\n\n[REPORT]\n");
    assert!(swept.contains("RES  0.05"), "the fixture must sweep");
    assert!(
        swept.contains("RAIN1  1:30"),
        "a second storm must follow the sweep"
    );
    assert!(
        swept.contains("1.2  0  80"),
        "sweeping must have an efficiency"
    );
    restores_identically(&swept, 0.4);
}

// ── The format's own symmetry ───────────────────────────────────────────

/// A checkpoint restored and written again is the same bytes.
///
/// This is the one check here that does not depend on a model exercising
/// the state it covers. A field written but never read back, or read in
/// the wrong order, leaves the second checkpoint different from the first
/// however little the field influences results — which is exactly the
/// case for everything the property tests above cannot reach.
fn resaves_identically(model: &str, fraction: f64) {
    let (mut whole, diags, _) = Simulation::open(model).expect("open");
    assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
    whole.run();
    let target = ((whole.snapshots.len() as f64 * fraction) as usize).max(1);

    let (mut first, _, _) = Simulation::open(model).expect("open");
    while first.snapshots.len() < target {
        assert!(first.step(), "the run ended before the checkpoint instant");
    }
    let mut once = Vec::new();
    first.save_checkpoint(&mut once).expect("checkpoint");
    assert!(once.len() > 200, "a checkpoint of {} bytes", once.len());

    let (mut second, _, _) = Simulation::open(model).expect("open");
    second.load_checkpoint(&once).expect("restore");
    let mut twice = Vec::new();
    second
        .save_checkpoint(&mut twice)
        .expect("checkpoint again");

    assert_eq!(
        once.len(),
        twice.len(),
        "the two checkpoints differ in length"
    );
    if once != twice {
        let at = once
            .iter()
            .zip(&twice)
            .position(|(a, b)| a != b)
            .expect("a differing byte");
        panic!(
            "the two checkpoints first differ at byte {at} of {}",
            once.len()
        );
    }
}

#[test]
fn a_routed_checkpoint_survives_a_round_trip() {
    resaves_identically(MODEL, 0.4);
}

#[test]
fn a_surface_checkpoint_survives_a_round_trip() {
    resaves_identically(&parcel_model("", ""), 0.3);
}

#[test]
fn a_cascade_checkpoint_survives_a_round_trip() {
    let cascade = parcel_model("", "").replace(
        "P1  G1  J1  10  40  500  0.01  0",
        "P1  G1  P2  10  40  500  0.01  0",
    );
    resaves_identically(&cascade, 0.3);
}

#[test]
fn an_aquifer_checkpoint_survives_a_round_trip() {
    resaves_identically(&fixture("groundwater_lateral_flow.inp"), 0.4);
}

#[test]
fn a_snow_checkpoint_survives_a_round_trip() {
    // Snow has no property test, for want of a model that lies any under
    // a substituted clock. Its state still has to survive the format, and
    // this is the check that says so.
    resaves_identically(&fixture("snowmelt_pack.inp"), 0.4);
}

#[test]
fn a_control_measure_checkpoint_survives_a_round_trip() {
    resaves_identically(&fixture("lid_bioretention_underdrain.inp"), 0.4);
}

#[test]
fn a_quality_checkpoint_survives_a_round_trip() {
    let dirty = fixture("buildup_washoff_treatment.inp")
        .replace("[REPORT]\n", "[OPTIONS]\nDRY_DAYS 5\n\n[REPORT]\n");
    resaves_identically(&dirty, 0.3);
}

#[test]
fn a_swept_checkpoint_survives_a_round_trip() {
    let swept = fixture("buildup_washoff_treatment.inp")
        .replace("RES  0  0  0", "RES  0.05  0.8  0.005")
        .replace(
            "RES  TSS  EXP  0.2  1.2  0  0",
            "RES  TSS  EXP  0.2  1.2  0  80",
        )
        .replace("[REPORT]\n", "[OPTIONS]\nDRY_DAYS 5\n\n[REPORT]\n");
    resaves_identically(&swept, 0.4);
}

#[test]
fn a_storage_checkpoint_survives_a_round_trip() {
    // The residence time no property test can reach travels in here.
    let treated = MODEL.replace(
        "[JUNCTIONS]",
        "[POLLUTANTS]\nTSS  MG/L  10  0  0  0.1\n\n[JUNCTIONS]",
    );
    resaves_identically(&treated, 0.4);
}

#[test]
fn a_returned_drain_checkpoint_survives_a_round_trip() {
    // A control measure returning its drain to the pervious area, which
    // is the only shape that puts a return volume in flight between
    // steps. Every fixture sets that column to zero.
    //
    // This still does not reach the return volume itself: dropping it on
    // restore leaves the checkpoint identical at every instant tried
    // (0.1, 0.2, 0.4, 0.6 and 0.8 of the run), so it is zero whenever a
    // checkpoint is taken here. It is the one written field with no test
    // that would notice its loss.
    let returned = fixture("rain_barrel_delayed_drain.inp")
        .replace("S1  RB1  4  10  1  0  25  0", "S1  RB1  4  10  1  0  25  1");
    assert!(
        returned.contains("0  25  1"),
        "the drain must return to the pervious area"
    );
    resaves_identically(&returned, 0.4);
}

/// A storage vertex whose treatment reads how long water has been sitting
/// in it, which is the only thing that reads the residence time.
///
/// The predecessor shipped its hotstart without this field and added it
/// in 5.1.008. It is exactly as easy to leave out of a checkpoint.
#[test]
fn a_restored_storage_residence_time_continues_bit_identically() {
    restores_identically(&treated_storage(), 0.35);
}

/// The treatment must actually be treating when the checkpoint is taken,
/// and the storage must be holding water long enough for its residence
/// time to be worth carrying.
#[test]
fn the_treated_storage_is_treating() {
    let model = treated_storage();
    let (mut sim, _, _) = Simulation::open(&model).expect("open");
    sim.run();
    let snap = sim
        .snapshots
        .iter()
        .find(|s| s.depths[2] > 0.1)
        .expect("the storage never filled");
    // Constituent first, then vertex: reading these the other way round
    // is what made this test look impossible the first time, since it
    // reported the junction upstream of the treatment in both runs.
    let untreated = model.replace("[TREATMENT]\nS1  TSS  C = 20 / (1 + HRT)\n\n", "");
    let (mut plain, _, _) = Simulation::open(&untreated).expect("open");
    plain.run();
    let at = sim
        .snapshots
        .iter()
        .position(|s| s.t == snap.t)
        .expect("instant");
    let with = sim.snapshots[at].node_quality[0][2];
    let without = plain.snapshots[at].node_quality[0][2];
    assert!(
        without > with + 1.0,
        "treated {with} against untreated {without}: the treatment is doing nothing"
    );
}

/// The routed model with a constituent reaching its storage vertex and a
/// treatment there whose effluent depends on the residence time.
fn treated_storage() -> String {
    MODEL
        .replace(
            "[JUNCTIONS]",
            "[POLLUTANTS]\nTSS  MG/L  10  0  0  0.1\n\n[JUNCTIONS]",
        )
        .replace(
            "[TIMESERIES]",
            "[TREATMENT]\nS1  TSS  C = 20 / (1 + HRT)\n\n[TIMESERIES]",
        )
        // Without mass arriving there is nothing to treat, which is how
        // the first attempt at this test came to prove nothing.
        .replace("J1  FLOW  TS1", "J1  FLOW  TS1\nJ1  TSS   TS2  CONCEN")
        .replace(
            "TS1  1:00  0.0",
            "TS1  1:00  0.0\nTS2  0:00  50.0\nTS2  1:00  50.0",
        )
}

// ── Controls, sewer inflow and street inlets ────────────────────────────

/// Rules acting on the network, checkpointed while a rule has already
/// fired: the log of what has been done and each modulated action's error
/// history are state a restored run must carry.
#[test]
fn a_restored_control_system_continues_bit_identically() {
    restores_identically(&fixture("control_rules.inp"), 0.4);
}

/// Sewer inflow, whose unit hydrographs hold rainfall still draining
/// through them. A run resumed without that memory starts dry after a
/// storm it has already had.
#[test]
fn a_restored_sewer_inflow_continues_bit_identically() {
    restores_identically(&fixture("rdii_sanitary_inflow.inp"), 0.4);
}

/// Street inlets, whose backflow ratio is set each step.
#[test]
fn a_restored_street_inlet_continues_bit_identically() {
    restores_identically(&fixture("street_inlet_capture.inp"), 0.4);
}

#[test]
fn a_control_checkpoint_survives_a_round_trip() {
    resaves_identically(&fixture("control_rules.inp"), 0.4);
}

#[test]
fn a_sewer_inflow_checkpoint_survives_a_round_trip() {
    resaves_identically(&fixture("rdii_sanitary_inflow.inp"), 0.4);
}

#[test]
fn a_street_inlet_checkpoint_survives_a_round_trip() {
    resaves_identically(&fixture("street_inlet_capture.inp"), 0.4);
}

/// The three instants must have something to carry.
#[test]
fn the_control_and_inflow_instants_are_active() {
    // A rule must have fired, or the log and the error history are empty
    // and their loss cannot be seen.
    let (mut sim, _, _) = Simulation::open(&fixture("control_rules.inp")).expect("open");
    sim.run();
    let whole = sim.snapshots.len();
    let (mut part, _, _) = Simulation::open(&fixture("control_rules.inp")).expect("open");
    while part.snapshots.len() < ((whole as f64 * 0.4) as usize).max(1) {
        assert!(part.step(), "the run ended early");
    }
    assert!(
        part.snapshots
            .iter()
            .any(|s| s.link_capacity.iter().any(|c| *c < 1.0)),
        "no rule had acted, so the checkpoint carried no control state"
    );

    // And sewer inflow must be arriving.
    let (mut sim, _, _) = Simulation::open(&fixture("rdii_sanitary_inflow.inp")).expect("open");
    sim.run();
    assert!(
        sim.ledgers().network.inflow > 0.0,
        "no sewer inflow reached the network"
    );
}

/// A PID action, whose error history is two steps deep.
///
/// The rule fixture modulates nothing, so its actions' error histories
/// stay zero and losing them cannot be seen. A velocity-form controller
/// reads the two previous errors, so a restored run that has forgotten
/// them applies a different increment on its very next step.
#[test]
fn a_restored_pid_controller_continues_bit_identically() {
    restores_identically(&pid_model(), 0.4);
}

#[test]
fn a_pid_checkpoint_survives_a_round_trip() {
    resaves_identically(&pid_model(), 0.4);
}

/// The controller must be modulating when the checkpoint is taken, or
/// its error history is zero and the two tests above prove nothing.
#[test]
fn the_pid_controller_is_modulating() {
    let (mut sim, diags, _) = Simulation::open(&pid_model()).expect("open");
    assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
    sim.run();
    let settings: Vec<f64> = sim.snapshots.iter().map(|s| s.link_capacity[0]).collect();
    let first = settings.first().copied().unwrap_or(0.0);
    assert!(
        settings.iter().any(|s| (s - first).abs() > 1e-6),
        "the setting never moved: {settings:?}"
    );
}

/// The routed model with its last conduit replaced by an orifice a
/// controller holds toward a depth set-point inside the range the storage
/// actually reaches.
///
/// Built rather than borrowed: `control_rules.inp` has no inflow under a
/// substituted clock, so its node is dry, its rules fire against nothing
/// and a controller on it never leaves its cap.
fn pid_model() -> String {
    MODEL
        .replace("C3  S1  O1  200  0.013  0  0  0  0", "")
        .replace("C3  CIRCULAR  1.0  0  0  0  1", "R1  CIRCULAR  1.0  0  0  0")
        .replace(
            "[XSECTIONS]",
            "[ORIFICES]\nR1  S1  O1  SIDE  0  0.65  NO  0\n\n[XSECTIONS]",
        )
        .replace(
            "[INFLOWS]",
            "[CONTROLS]\nRULE HOLD\nIF NODE S1 DEPTH > 0.3\nTHEN ORIFICE R1 SETTING = PID 0.5 2.0 0.5\n\n[INFLOWS]",
        )
}

#[test]
fn a_replaying_checkpoint_survives_a_round_trip() {
    let (mut first, _, _) = Simulation::open(&replay_model()).expect("open");
    first.supply_runoff(&replay_file()).expect("supply");
    while first.snapshots.len() < 3 {
        assert!(first.step(), "the run ended early");
    }
    let mut once = Vec::new();
    first.save_checkpoint(&mut once).expect("checkpoint");

    let (mut second, _, _) = Simulation::open(&replay_model()).expect("open");
    second.supply_runoff(&replay_file()).expect("supply");
    second.load_checkpoint(&once).expect("restore");
    let mut twice = Vec::new();
    second
        .save_checkpoint(&mut twice)
        .expect("checkpoint again");
    assert_eq!(
        once.len(),
        twice.len(),
        "the two checkpoints differ in length"
    );
    assert!(once == twice, "the two checkpoints differ");
}

/// A run collecting interface files of its own checkpoints and restores
/// with what it has collected so far.
///
/// This was the last thing a checkpoint refused. Without it a run saving
/// a runoff file, restored partway, would write a file starting where the
/// checkpoint did and claim it covered the run.
#[test]
fn a_collecting_run_keeps_what_it_has_collected() {
    let saving = parcel_model("", "[FILES]\nSAVE RUNOFF runoff.bin");
    let (mut whole, _, _) = Simulation::open(&saving).expect("open");
    whole.run();
    let mut want = Vec::new();
    whole.write_runoff(&mut want).expect("write");

    let (mut first, _, _) = Simulation::open(&saving).expect("open");
    let target = ((whole.snapshots.len() as f64 * 0.4) as usize).max(1);
    while first.snapshots.len() < target {
        assert!(first.step(), "the run ended before the checkpoint instant");
    }
    let mut cp = Vec::new();
    first.save_checkpoint(&mut cp).expect("checkpoint");

    let (mut second, _, _) = Simulation::open(&saving).expect("open");
    second.load_checkpoint(&cp).expect("restore");
    second.run();
    let mut got = Vec::new();
    second.write_runoff(&mut got).expect("write");

    assert_eq!(
        want.len(),
        got.len(),
        "the restored run wrote a file of a different length"
    );
    assert!(want == got, "the restored run's runoff file differs");
}

/// An injection standing when a checkpoint is taken stands when it is
/// restored (§12.4).
///
/// Nothing else here would notice: the models above inject nothing, so
/// the fields sat unwritten and every test stayed green. `Simulation` is
/// the one type the checkpoint does not read through an exhaustive
/// destructure, so a field added to it is caught by a test or not at all.
#[test]
fn a_standing_injection_survives_a_checkpoint() {
    let model = parcel_model("", "");
    let (mut first, _, _) = Simulation::open(&model).expect("open");
    assert!(first.set_precipitation("G1", Some(30.0e-3 / 3600.0)));
    assert!(first.set_lateral_inflow("J1", Some(0.4)));
    while first.snapshots.len() < 3 {
        assert!(first.step(), "the run ended early");
    }
    let mut cp = Vec::new();
    first.save_checkpoint(&mut cp).expect("checkpoint");
    while first.step() {}

    let (mut second, _, _) = Simulation::open(&model).expect("open");
    second.load_checkpoint(&cp).expect("restore");
    second.run();

    // The restored run must keep raining and keep receiving the injected
    // inflow, which the model's own record does not supply.
    assert_eq!(
        every_output(&first).results,
        every_output(&second).results,
        "the restored run diverged, so an injection was not carried"
    );
    let tail: f64 = second.snapshots[3..]
        .iter()
        .map(|s| s.subcatch[0].rain)
        .sum();
    assert!(tail > 0.0, "the restored run stopped raining");
}

/// And a checkpoint of a run with an injection round-trips, which is what
/// would catch one written but never read back.
#[test]
fn an_injection_survives_a_checkpoint_round_trip() {
    let model = parcel_model("", "");
    let (mut sim, _, _) = Simulation::open(&model).expect("open");
    assert!(sim.set_precipitation("G1", Some(30.0e-3 / 3600.0)));
    assert!(sim.set_lateral_inflow("J1", Some(0.4)));
    while sim.snapshots.len() < 3 {
        assert!(sim.step(), "the run ended early");
    }
    let mut once = Vec::new();
    sim.save_checkpoint(&mut once).expect("checkpoint");

    let (mut second, _, _) = Simulation::open(&model).expect("open");
    second.load_checkpoint(&once).expect("restore");
    let mut twice = Vec::new();
    second
        .save_checkpoint(&mut twice)
        .expect("checkpoint again");
    assert!(once == twice, "the two checkpoints differ");
}
