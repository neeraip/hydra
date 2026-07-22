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

    fn snapshots(&self) -> &[HydSnapshot] {
        &self.hyd_snapshots
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
}
