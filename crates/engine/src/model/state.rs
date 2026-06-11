use super::network::LinkStatus;

// ── Per-step state ─────────────────────────────────────────────────────────────

/// Per-step hydraulic and quality state for a single node (§2.4, per-step fields).
///
/// Not all fields are meaningful for every node type:
/// - `head`, `quality`: all node types.
/// - `demand_flow`, `emitter_flow`, `leakage_flow`: junctions only.
/// - `net_flow`: reservoirs and tanks.
/// - `level`, `volume`: tanks only.
#[derive(Debug, Clone, Default)]
pub struct NodeState {
    /// Hydraulic head (m).
    pub head: f64,
    /// Actual delivered demand flow (m³/s).
    pub demand_flow: f64,
    /// Emitter outflow (m³/s).
    pub emitter_flow: f64,
    /// Leakage outflow from FAVAD model (m³/s).
    pub leakage_flow: f64,
    /// Net inflow to reservoir or tank (m³/s; positive = filling).
    pub net_flow: f64,
    /// Current water level above tank bottom (m); tanks only.
    pub level: f64,
    /// Current tank volume (m³); tanks only.
    pub volume: f64,
    /// Water quality concentration (mg/L, h, or % depending on mode).
    pub quality: f64,
}

/// Per-step hydraulic and quality state for a single link (§2.6, per-step fields).
#[derive(Debug, Clone)]
pub struct LinkState {
    /// Volumetric flow rate (m³/s; positive = from_node → to_node).
    pub flow: f64,
    /// Current operational status.
    pub status: LinkStatus,
    /// Current setting value (pump speed ratio or valve pressure setpoint).
    pub setting: f64,
    /// Water quality in this link.
    pub quality: f64,
    /// Volume-weighted average reaction rate (mass/L/day); only meaningful for pipes.
    pub reaction_rate: f64,
}

impl Default for LinkState {
    fn default() -> Self {
        Self {
            flow: 0.0,
            status: LinkStatus::Open,
            setting: 1.0,
            quality: 0.0,
            reaction_rate: 0.0,
        }
    }
}
