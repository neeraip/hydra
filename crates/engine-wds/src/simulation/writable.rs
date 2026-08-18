// writable — implements crate::io::WritableSimulation for Simulation.
//
// This bridges the simulation crate's internal state to the generic trait
// that the output writers in crate::io require. No solver logic lives
// here — only accessors.

use super::*;

use crate::io::{
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

    fn snapshots(&self) -> &[HydSnapshot] {
        &self.hyd_snapshots
    }

    fn finalized_through(&self) -> f64 {
        match self.network.as_ref() {
            None => f64::NEG_INFINITY,
            // No quality analysis: snapshots are final as soon as the
            // hydraulic phase records them.
            Some(n) if n.options.quality_mode == crate::QualityMode::None => f64::INFINITY,
            // Quality enabled: snapshots hold provisional quality values
            // until the quality phase writes back through their time.
            // Before quality initialisation even the t=0 snapshot is
            // provisional (initial quality lands in it during init).
            Some(_) if self.quality_state.is_some() => self.quality_t,
            Some(_) => f64::NEG_INFINITY,
        }
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
        use crate::io::{out_writer::write_binary_output, rpt_writer};
        use crate::FlowUnits;

        let sim = Simulation::create();
        assert!(!sim.has_network());

        let mut buf = std::io::Cursor::new(Vec::new());
        let err = write_binary_output(&mut buf, &sim, "in.inp", "out.rpt", FlowUnits::Gpm)
            .expect_err("no network loaded");
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
    fn net_and_snapshots_mirror_session_state() {
        let sess = run_session(QualityMode::None);
        let net = WritableSimulation::net(&sess);
        let ids: Vec<&str> = net.nodes.iter().map(|n| n.base.id.as_str()).collect();
        assert_eq!(ids, sess.node_ids());

        let snap_times: Vec<f64> = WritableSimulation::snapshots(&sess)
            .iter()
            .map(|s| s.t)
            .collect();
        assert_eq!(snap_times, sess.snapshot_times());
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
    fn finalized_frontier_tracks_the_quality_phase() {
        // Quality disabled: every snapshot is final as hydraulics records it.
        let mut sess = Simulation::from_network(pump_network(QualityMode::None)).expect("load");
        assert_eq!(sess.finalized_through(), f64::INFINITY);
        sess.run().expect("run");
        assert_eq!(sess.finalized_through(), f64::INFINITY);

        // Quality enabled: nothing is final until the quality phase starts,
        // then the frontier follows it to the end of the run.
        let mut sess = Simulation::from_network(pump_network(QualityMode::Age)).expect("load");
        assert_eq!(sess.finalized_through(), f64::NEG_INFINITY);
        sess.run_hydraulics().expect("hydraulics");
        assert_eq!(
            sess.finalized_through(),
            f64::NEG_INFINITY,
            "snapshots hold provisional quality until the quality phase runs"
        );
        sess.run_quality().expect("quality");
        assert!(sess.finalized_through() >= 2.0 * 3600.0);
    }

    /// The CLI/GUI stream periods out while stepping. A streamed file must be
    /// byte-identical to one serialized after the full run — in particular,
    /// quality must not be frozen at initial values because the hydraulic
    /// phase's appends consumed the snapshots before quality was written back
    /// (simulation spec §8.3, streaming serialization).
    #[test]
    fn streamed_out_is_byte_identical_to_post_run_out() {
        use crate::io::out_writer::{write_binary_output, OutStreamWriter};
        use crate::FlowUnits;
        use std::io::Cursor;

        // Batch reference: full run, then serialize.
        let mut batch = Simulation::from_network(pump_network(QualityMode::Age)).expect("load");
        batch.run().expect("run");
        let mut buf = Cursor::new(Vec::new());
        write_binary_output(&mut buf, &batch, "t.inp", "", FlowUnits::Gpm).expect("write");
        let batch_bytes = buf.into_inner();

        // Streamed in lockstep, exactly as the CLI/GUI run loops do.
        let mut live = Simulation::from_network(pump_network(QualityMode::Age)).expect("load");
        let mut stream =
            OutStreamWriter::begin(Cursor::new(Vec::new()), &live, "t.inp", "", FlowUnits::Gpm)
                .expect("begin");
        stream.append_available(&live).expect("append");
        loop {
            let dt = live.step_hydraulics().expect("hydraulics step");
            stream.append_available(&live).expect("append");
            if dt == 0.0 {
                break;
            }
        }
        loop {
            let dt = live.step_quality().expect("quality step");
            stream.append_available(&live).expect("append");
            if dt == 0.0 {
                break;
            }
        }
        let streamed_bytes = stream.finish(&live).expect("finish").into_inner();

        assert_eq!(streamed_bytes, batch_bytes);

        // Guard against trivially passing on frozen values: the last period's
        // node quality (water age) must have advanced beyond its initial 0.
        let n_nodes = batch.node_ids().len();
        let n_links = batch.link_ids().len();
        let record = (4 * n_nodes + 8 * n_links) * 4;
        let last_period = batch_bytes.len() - 12 - 16 - record;
        let quality_column = last_period + 3 * n_nodes * 4;
        let ages: Vec<f32> = (0..n_nodes)
            .map(|i| {
                let at = quality_column + i * 4;
                f32::from_le_bytes(batch_bytes[at..at + 4].try_into().unwrap())
            })
            .collect();
        assert!(
            ages.iter().any(|a| *a > 0.5),
            "final-period ages should be nonzero, got {ages:?}"
        );
    }

    /// A stream finished before quality ran must not persist provisional
    /// quality: it closes with zero periods rather than wrong values.
    #[test]
    fn stream_finished_before_quality_holds_back_all_periods() {
        use crate::io::out_writer::OutStreamWriter;
        use crate::FlowUnits;
        use std::io::Cursor;

        let mut sess = Simulation::from_network(pump_network(QualityMode::Age)).expect("load");
        let mut stream =
            OutStreamWriter::begin(Cursor::new(Vec::new()), &sess, "t.inp", "", FlowUnits::Gpm)
                .expect("begin");
        sess.run_hydraulics().expect("hydraulics");
        stream.append_available(&sess).expect("append");
        let bytes = stream.finish(&sess).expect("finish").into_inner();

        let n_periods =
            i32::from_le_bytes(bytes[bytes.len() - 12..bytes.len() - 8].try_into().unwrap());
        assert_eq!(n_periods, 0);
    }
}
