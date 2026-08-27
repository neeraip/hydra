// writable — implements crate::simulation::contract::WritableSimulation for Simulation.
//
// This bridges the simulation crate's internal state to the generic trait
// that the output writers in crate::io require. No solver logic lives
// here — only accessors.

use super::*;

use crate::simulation::contract::{
    FlowBalance, FlowBalanceSummary, HydSnapshot, MassBalance, PumpEnergy, SimWarning,
    WritableSimulation,
};
use crate::{LinkKind, Network};

impl WritableSimulation for Simulation {
    fn net(&self) -> &Network {
        self.network
            .as_ref()
            .expect("WritableSimulation::net called before network was loaded")
    }

    fn has_network(&self) -> bool {
        self.network.is_some()
    }

    fn current_instant(&self) -> Option<&HydSnapshot> {
        self.current_instant.as_ref()
    }

    fn instants_recorded(&self) -> u64 {
        self.instants_recorded
    }

    fn pump_energy_at(&self, link_index: usize) -> Option<&PumpEnergy> {
        self.accounting.as_ref().map(|a| &a.pump_energy[link_index])
    }

    fn peak_demand_kw(&self) -> f64 {
        self.accounting.as_ref().map_or(0.0, |a| a.peak_demand_kw)
    }

    fn mass_balance(&self) -> Option<&MassBalance> {
        self.quality_state.as_ref().map(|qs| &qs.mass_balance)
    }

    fn warnings(&self) -> &[SimWarning] {
        &self.warnings
    }

    fn pump_energy_by_id(&self, pump_id: &str) -> Option<&PumpEnergy> {
        let network = self.network.as_ref()?;
        let link_index = network
            .links
            .iter()
            .position(|l| l.base.id == pump_id && matches!(l.kind, LinkKind::Pump(_)))?;
        self.accounting.as_ref().map(|a| &a.pump_energy[link_index])
    }

    fn analysis_times(&self) -> (Option<std::time::SystemTime>, Option<std::time::SystemTime>) {
        (self.analysis_begun, self.analysis_ended)
    }

    fn flow_balance(&self) -> Option<&FlowBalance> {
        self.accounting.as_ref().map(|a| &a.flow_balance)
    }

    fn flow_balance_summary(&self) -> Option<FlowBalanceSummary> {
        self.flow_balance_summary().ok()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestNetworkBuilder;
    use crate::{QualityMode, SimulationOptions};

    /// A session that has been created but not loaded has no network, and the
    /// writers used to reach past that into `net()`'s `expect` — a panic
    /// across the published API, on a path the SDK re-exports the trait
    /// specifically to let integrators call.
    #[test]
    fn writing_before_a_model_is_loaded_is_an_error_not_a_panic() {
        use crate::dialect::{out_writer::OutStreamWriter, rpt_writer};
        use crate::FlowUnits;

        let sim = Simulation::create();
        assert!(!sim.has_network());

        let buf = std::io::Cursor::new(Vec::new());
        let err = OutStreamWriter::begin(buf, &sim, "in.inp", "out.rpt", FlowUnits::Gpm)
            .err()
            .expect("no network loaded");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("no network loaded"), "{err}");

        let err = rpt_writer::build_json_report(&sim).expect_err("no network loaded");
        assert!(err.to_string().contains("no network loaded"), "{err}");

        rpt_writer::build_text_report(&sim).expect_err("no network loaded");
    }

    /// Reservoir —pump→ J1 —pipe→ J2 network with a 2 h EPS horizon.
    fn pump_network(quality_mode: QualityMode) -> Network {
        TestNetworkBuilder::new()
            .with_options(SimulationOptions {
                duration: 2.0 * 3600.0,
                hyd_step: 3600.0,
                qual_step: 300.0,
                report_step: 3600.0,
                report_start: 0.0,
                quality_mode,
                ..SimulationOptions::default()
            })
            .reservoir("R1", 50.0)
            .junction("J1", 80.0, 10.0)
            .junction("J2", 80.0, 5.0)
            .const_hp_pump("PU1", "R1", "J1", 5.0)
            .hw_pipe("P1", "J1", "J2", 500.0, 8.0, 100.0)
            .build()
            .0
    }

    fn run_session(quality_mode: QualityMode) -> Simulation {
        let mut sess = Simulation::from_network(pump_network(quality_mode)).expect("load");
        sess.run().expect("run");
        sess
    }

    #[test]
    fn net_and_held_instant_mirror_session_state() {
        let sess = run_session(QualityMode::None);
        let net = WritableSimulation::net(&sess);
        let ids: Vec<&str> = net.nodes.iter().map(|n| n.base.id.as_str()).collect();
        assert_eq!(ids, sess.node_ids());

        // The trait and the inherent accessors describe the same held instant.
        let held = WritableSimulation::current_instant(&sess).map(|inst| inst.t);
        assert_eq!(held, sess.current_time());
        assert!(
            WritableSimulation::instants_recorded(&sess) > 1,
            "a completed run recorded more than one instant, keeping only the last"
        );
    }

    #[test]
    fn pump_energy_lookup_by_id_and_index() {
        let sess = run_session(QualityMode::None);
        let by_id = sess.pump_energy_by_id("PU1").expect("pump energy by id");
        assert!(by_id.kwh >= 0.0);
        // Index 0 is the pump link; the id- and index-based accessors agree.
        let by_index = sess.pump_energy_at(0).expect("pump energy by index");
        assert_eq!(by_id.kwh.to_bits(), by_index.kwh.to_bits());

        // Non-pump and unknown IDs yield None.
        assert!(sess.pump_energy_by_id("P1").is_none());
        assert!(sess.pump_energy_by_id("ZZZZ").is_none());
    }

    #[test]
    fn mass_balance_none_before_quality_some_after() {
        let mut sess = Simulation::from_network(pump_network(QualityMode::Age)).expect("load");
        assert!(WritableSimulation::mass_balance(&sess).is_none());
        sess.run().expect("run");
        assert!(WritableSimulation::mass_balance(&sess).is_some());
    }

    #[test]
    fn analysis_times_recorded_across_run() {
        let mut sess = Simulation::from_network(pump_network(QualityMode::None)).expect("load");
        assert_eq!(sess.analysis_times(), (None, None));
        sess.run().expect("run");
        let (begun, ended) = sess.analysis_times();
        let begun = begun.expect("begun set");
        let ended = ended.expect("ended set");
        assert!(begun <= ended);
    }

    #[test]
    fn flow_balance_and_summary_via_trait() {
        let sess = run_session(QualityMode::None);
        assert!(WritableSimulation::flow_balance(&sess).is_some());
        let summary = WritableSimulation::flow_balance_summary(&sess).expect("summary after run");
        assert!(summary.total_inflow > 0.0, "no inflow recorded");

        // Before load there is nothing to summarise.
        let empty = Simulation::create();
        assert!(WritableSimulation::flow_balance(&empty).is_none());
        assert!(WritableSimulation::flow_balance_summary(&empty).is_none());
        assert_eq!(empty.peak_demand_kw(), 0.0);
    }

    #[test]
    fn warnings_trait_matches_inherent_accessor() {
        let sess = run_session(QualityMode::None);
        let trait_warnings = WritableSimulation::warnings(&sess);
        assert_eq!(trait_warnings.len(), Simulation::warnings(&sess).len());
    }

    #[test]
    fn an_instant_reaches_the_stream_in_the_step_that_records_it() {
        use crate::dialect::out_writer::OutStreamWriter;
        use crate::FlowUnits;
        use std::io::Cursor;

        // A frontier used to hold periods back until a second pass wrote
        // quality through their time; with quality advancing inside the step
        // that records the instant, nothing is provisional and the frontier is
        // gone (§8.3). Observed where it matters: the period is written by the
        // very next append, with quality on exactly as with it off.
        for mode in [QualityMode::None, QualityMode::Age] {
            let mut sess = Simulation::from_network(pump_network(mode)).expect("load");
            let mut stream =
                OutStreamWriter::begin(Cursor::new(Vec::new()), &sess, "t.inp", "", FlowUnits::Gpm)
                    .expect("begin");
            sess.step_hydraulics().expect("one step");
            stream.append_available(&sess).expect("append");
            let bytes = stream.finish(&sess).expect("finish").into_inner();

            let n =
                i32::from_le_bytes(bytes[bytes.len() - 12..bytes.len() - 8].try_into().unwrap());
            assert_eq!(n, 1, "{mode:?}: the first instant was held back");
        }
    }

    /// A streamed run carries quality that has actually advanced.
    ///
    /// This was a comparison against a batch serialization, which existed to
    /// catch quality being frozen at its initial values when the hydraulic
    /// appends consumed snapshots before the second pass wrote back. There is
    /// no second pass and no batch path: a session holds one instant (§8.2),
    /// so streaming is the only way a file gets written. What remains worth
    /// asserting is that the file is not full of initial values. Whole-file
    /// agreement is pinned by the byte gate over the benchmark corpus
    /// (scripts/wds_byte_gate.sh).
    #[test]
    fn a_streamed_run_carries_advanced_quality() {
        use crate::dialect::out_writer::OutStreamWriter;
        use crate::FlowUnits;
        use std::io::Cursor;

        let mut live = Simulation::from_network(pump_network(QualityMode::Age)).expect("load");
        let mut stream =
            OutStreamWriter::begin(Cursor::new(Vec::new()), &live, "t.inp", "", FlowUnits::Gpm)
                .expect("begin");
        loop {
            let dt = live.step_hydraulics().expect("hydraulics step");
            stream.append_available(&live).expect("append");
            if dt == 0.0 {
                break;
            }
        }
        let bytes = stream.finish(&live).expect("finish").into_inner();

        let n_nodes = live.node_ids().len();
        let n_links = live.link_ids().len();
        let record = (4 * n_nodes + 8 * n_links) * 4;
        let last_period = bytes.len() - 12 - 16 - record;
        let quality_column = last_period + 3 * n_nodes * 4;
        let ages: Vec<f32> = (0..n_nodes)
            .map(|i| {
                let at = quality_column + i * 4;
                f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
            })
            .collect();
        assert!(
            ages.iter().any(|a| *a > 0.5),
            "final-period ages should have advanced past their initial 0, got {ages:?}"
        );
    }

    /// A stream driven per step holds every period; one left behind says so.
    ///
    /// A session holds one instant (§8.2), so running to completion and then
    /// appending once cannot work: the instants in between are gone. That
    /// used to silently produce a short file. It is now an error naming what
    /// happened, because a results file with holes in it that calls itself a
    /// run is worse than no file.
    #[test]
    fn a_stream_must_be_appended_every_step_and_says_so_if_not() {
        use crate::dialect::out_writer::OutStreamWriter;
        use crate::FlowUnits;
        use std::io::Cursor;

        // Driven properly: every period lands.
        let mut sess = Simulation::from_network(pump_network(QualityMode::Age)).expect("load");
        let mut stream =
            OutStreamWriter::begin(Cursor::new(Vec::new()), &sess, "t.inp", "", FlowUnits::Gpm)
                .expect("begin");
        loop {
            let dt = sess.step_hydraulics().expect("step");
            stream.append_available(&sess).expect("append");
            if dt == 0.0 {
                break;
            }
        }
        let bytes = stream.finish(&sess).expect("finish").into_inner();
        let n_periods =
            i32::from_le_bytes(bytes[bytes.len() - 12..bytes.len() - 8].try_into().unwrap());
        // Two hours reported hourly from zero: t = 0, 3600, 7200.
        assert_eq!(n_periods, 3, "every period is written");

        // Left behind: refused, rather than written short.
        let mut late = Simulation::from_network(pump_network(QualityMode::Age)).expect("load");
        let mut stream =
            OutStreamWriter::begin(Cursor::new(Vec::new()), &late, "t.inp", "", FlowUnits::Gpm)
                .expect("begin");
        late.run_hydraulics().expect("run");
        let err = stream
            .append_available(&late)
            .expect_err("a stream that missed instants must refuse");
        assert!(err.to_string().contains("fell behind"), "{err}");
    }
}
