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
