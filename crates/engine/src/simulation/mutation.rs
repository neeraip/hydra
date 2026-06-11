use super::*;

impl Simulation {
    /// Modify a node property before the simulation is run (§8.3 `set_node_property()`).
    ///
    /// The network must be in the `Loaded` phase. Returns
    /// [`SessionError::InvalidPhase`] if no network is loaded, or
    /// [`SessionError::UnknownId`] if `node_id` does not exist.
    pub fn set_node_property(
        &mut self,
        node_id: &str,
        property: NodeProperty,
        value: f64,
    ) -> Result<(), SessionError> {
        let network = self
            .network
            .as_mut()
            .ok_or_else(|| SessionError::InvalidPhase {
                expected: "Loaded".into(),
                actual: self.phase.name().to_string(),
            })?;
        let idx = network
            .nodes
            .iter()
            .position(|n| n.base.id == node_id)
            .ok_or_else(|| SessionError::UnknownId(node_id.to_string()))?;
        match property {
            NodeProperty::Elevation => network.nodes[idx].base.elevation = value,
            NodeProperty::InitialQuality => network.nodes[idx].base.initial_quality = value,
        }
        Ok(())
    }

    /// Modify a link property (§8.3 `set_link_property()`).
    pub fn set_link_property(
        &mut self,
        link_id: &str,
        property: LinkProperty,
        value: f64,
    ) -> Result<(), SessionError> {
        let network = self
            .network
            .as_mut()
            .ok_or_else(|| SessionError::InvalidPhase {
                expected: "Loaded".into(),
                actual: self.phase.name().to_string(),
            })?;
        let idx = network
            .links
            .iter()
            .position(|l| l.base.id == link_id)
            .ok_or_else(|| SessionError::UnknownId(link_id.to_string()))?;
        match property {
            LinkProperty::Roughness => {
                if let LinkKind::Pipe(p) = &mut network.links[idx].kind {
                    p.roughness = value;
                }
            }
            LinkProperty::InitialStatus => {
                network.links[idx].base.initial_status = if value < 0.5 {
                    LinkStatus::Closed
                } else {
                    LinkStatus::Open
                };
            }
            LinkProperty::InitialSetting => {
                network.links[idx].base.initial_setting = Some(value);
            }
        }
        Ok(())
    }

    // ── Peak demand cost convenience ──────────────────────────────────────────

    /// Return the total peak demand cost (§7.1).
    pub fn peak_demand_cost(&self) -> f64 {
        match (&self.accounting, &self.network) {
            (Some(acc), Some(network)) => accounting::peak_demand_cost(acc, network),
            _ => 0.0,
        }
    }
}
