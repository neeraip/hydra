//! Test support utilities — ergonomic [`Network`] construction for unit and
//! integration tests across all Hydra crates.
//!
//! Gated behind the `test-support` Cargo feature so it never compiles into
//! release builds.
//!
//! # Example
//!
//! ```ignore
//! use crate::test_support::TestNetworkBuilder;
//!
//! let (net, node_states, link_states) = TestNetworkBuilder::new()
//!     .reservoir("R1", 100.0)
//!     .junction("J1", 0.0, 10.0) // 10 GPM demand
//!     .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
//!     .build();
//! ```

use std::collections::HashMap;

use crate::io::units::apply_unit_conversion;
use crate::{
    Curve, CurveKind, CurvePoint, DemandCategory, FavadCoeffs, Junction, Link, LinkBase, LinkKind,
    LinkState, LinkStatus, MixModel, Network, Node, NodeBase, NodeKind, NodeState, Pattern, Pipe,
    Pump, PumpCurveType, ReportOptions, Reservoir, SimulationOptions, Tank, Valve, ValveType,
};

/// No-op pswitch function for tests without controls.
///
/// Pass this to `solve_hydraulic_step` when the network has no simple controls.
pub fn no_pswitch(
    _net: &Network,
    _ns: &[NodeState],
    _statuses: &mut [LinkStatus],
    _settings: &mut [f64],
) -> bool {
    false
}

/// Ergonomic builder for constructing [`Network`] instances in tests.
///
/// All dimensions are in the **user unit system** matching the default
/// [`SimulationOptions`] (GPM / ft unless overridden via [`options_mut`]).
/// The [`build`] method produces a `Network` whose internal fields (indices,
/// pattern_index, etc.) are fully consistent, along with matching
/// `Vec<NodeState>` and `Vec<LinkState>` initialised to sensible defaults.
///
/// [`options_mut`]: TestNetworkBuilder::options_mut
/// [`build`]: TestNetworkBuilder::build
pub struct TestNetworkBuilder {
    options: SimulationOptions,
    nodes: Vec<NodeDef>,
    links: Vec<LinkDef>,
    patterns: Vec<Pattern>,
    curves: Vec<Curve>,
}

// ── Internal staging types ────────────────────────────────────────────────────

struct NodeDef {
    id: String,
    kind: NodeDefKind,
    elevation: f64,
    initial_quality: f64,
}

enum NodeDefKind {
    Junction {
        demands: Vec<DemandCategory>,
        emitter_coeff: f64,
    },
    Reservoir {
        head_pattern: Option<String>,
    },
    Tank {
        min_level: f64,
        max_level: f64,
        initial_level: f64,
        diameter: f64,
        min_volume: f64,
        volume_curve: Option<String>,
        mix_model: MixModel,
        mix_fraction: f64,
        bulk_coeff: f64,
        overflow: bool,
    },
}

struct LinkDef {
    id: String,
    from: String,
    to: String,
    kind: LinkDefKind,
    initial_status: LinkStatus,
    initial_setting: Option<f64>,
}

enum LinkDefKind {
    Pipe(Pipe),
    Pump(Pump),
    Valve(Valve),
}

// ── Builder implementation ────────────────────────────────────────────────────

impl TestNetworkBuilder {
    /// Create a new builder with default [`SimulationOptions`].
    pub fn new() -> Self {
        Self {
            options: SimulationOptions::default(),
            nodes: Vec::new(),
            links: Vec::new(),
            patterns: Vec::new(),
            curves: Vec::new(),
        }
    }

    /// Access the options for direct mutation.
    pub fn options_mut(&mut self) -> &mut SimulationOptions {
        &mut self.options
    }

    /// Override the entire options struct.
    pub fn with_options(mut self, options: SimulationOptions) -> Self {
        self.options = options;
        self
    }

    // ── Nodes ─────────────────────────────────────────────────────────────────

    /// Add a reservoir with fixed head equal to `elevation`.
    pub fn reservoir(mut self, id: &str, elevation: f64) -> Self {
        self.nodes.push(NodeDef {
            id: id.to_string(),
            kind: NodeDefKind::Reservoir { head_pattern: None },
            elevation,
            initial_quality: 0.0,
        });
        self
    }

    /// Add a junction with a single demand category (GPM by default).
    pub fn junction(mut self, id: &str, elevation: f64, demand: f64) -> Self {
        self.nodes.push(NodeDef {
            id: id.to_string(),
            kind: NodeDefKind::Junction {
                demands: vec![DemandCategory {
                    base_demand: demand,
                    pattern: None,
                    name: None,
                }],
                emitter_coeff: 0.0,
            },
            elevation,
            initial_quality: 0.0,
        });
        self
    }

    /// Add a junction with a specific demand pattern.
    pub fn junction_with_pattern(
        mut self,
        id: &str,
        elevation: f64,
        demand: f64,
        pattern: &str,
    ) -> Self {
        self.nodes.push(NodeDef {
            id: id.to_string(),
            kind: NodeDefKind::Junction {
                demands: vec![DemandCategory {
                    base_demand: demand,
                    pattern: Some(pattern.to_string()),
                    name: None,
                }],
                emitter_coeff: 0.0,
            },
            elevation,
            initial_quality: 0.0,
        });
        self
    }

    /// Add a junction with an emitter (emitter exponent defaults to 0.5).
    pub fn junction_with_emitter(
        mut self,
        id: &str,
        elevation: f64,
        demand: f64,
        emitter_coeff: f64,
    ) -> Self {
        self.nodes.push(NodeDef {
            id: id.to_string(),
            kind: NodeDefKind::Junction {
                demands: vec![DemandCategory {
                    base_demand: demand,
                    pattern: None,
                    name: None,
                }],
                emitter_coeff,
            },
            elevation,
            initial_quality: 0.0,
        });
        self
    }

    /// Add a cylindrical tank with defaults for mixing and overflow.
    pub fn tank(
        mut self,
        id: &str,
        elevation: f64,
        initial_level: f64,
        min_level: f64,
        max_level: f64,
        diameter: f64,
    ) -> Self {
        self.nodes.push(NodeDef {
            id: id.to_string(),
            kind: NodeDefKind::Tank {
                min_level,
                max_level,
                initial_level,
                diameter,
                min_volume: 0.0,
                volume_curve: None,
                mix_model: MixModel::Cstr,
                mix_fraction: 1.0,
                bulk_coeff: 0.0,
                overflow: false,
            },
            elevation,
            initial_quality: 0.0,
        });
        self
    }

    /// Add a cylindrical tank with a specific mixing model.
    #[allow(clippy::too_many_arguments)]
    pub fn tank_with_mixing(
        mut self,
        id: &str,
        elevation: f64,
        initial_level: f64,
        min_level: f64,
        max_level: f64,
        diameter: f64,
        mix_model: MixModel,
        mix_fraction: f64,
    ) -> Self {
        self.nodes.push(NodeDef {
            id: id.to_string(),
            kind: NodeDefKind::Tank {
                min_level,
                max_level,
                initial_level,
                diameter,
                min_volume: 0.0,
                volume_curve: None,
                mix_model,
                mix_fraction,
                bulk_coeff: 0.0,
                overflow: false,
            },
            elevation,
            initial_quality: 0.0,
        });
        self
    }

    /// Set initial quality for a node that was already added.
    ///
    /// For Chemical mode: mg/L; for Age mode: 0.0 (hours); for Trace: 0–100 (%).
    ///
    /// # Panics
    ///
    /// Panics if no node with the given `id` exists.
    pub fn node_quality(mut self, id: &str, quality: f64) -> Self {
        let node = self
            .nodes
            .iter_mut()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("unknown node '{id}' in node_quality"));
        node.initial_quality = quality;
        self
    }

    // ── Links ─────────────────────────────────────────────────────────────────

    /// Add a Hazen-Williams pipe.
    pub fn hw_pipe(
        mut self,
        id: &str,
        from: &str,
        to: &str,
        length: f64,
        diameter: f64,
        roughness: f64,
    ) -> Self {
        self.links.push(LinkDef {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            kind: LinkDefKind::Pipe(Pipe {
                length,
                diameter,
                roughness,
                minor_loss: 0.0,
                check_valve: false,
                bulk_coeff: None,
                wall_coeff: None,
                leak_coeff_1: 0.0,
                leak_coeff_2: 0.0,
            }),
            initial_status: LinkStatus::Open,
            initial_setting: Some(1.0),
        });
        self
    }

    /// Add a Hazen-Williams pipe with a per-pipe bulk reaction coefficient.
    ///
    /// `bulk_coeff` is in per-day units (converted to per-second during build).
    #[allow(clippy::too_many_arguments)]
    pub fn hw_pipe_with_bulk(
        mut self,
        id: &str,
        from: &str,
        to: &str,
        length: f64,
        diameter: f64,
        roughness: f64,
        bulk_coeff: f64,
    ) -> Self {
        self.links.push(LinkDef {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            kind: LinkDefKind::Pipe(Pipe {
                length,
                diameter,
                roughness,
                minor_loss: 0.0,
                check_valve: false,
                bulk_coeff: Some(bulk_coeff),
                wall_coeff: None,
                leak_coeff_1: 0.0,
                leak_coeff_2: 0.0,
            }),
            initial_status: LinkStatus::Open,
            initial_setting: Some(1.0),
        });
        self
    }

    /// Add a pipe with a check valve.
    pub fn cv_pipe(
        mut self,
        id: &str,
        from: &str,
        to: &str,
        length: f64,
        diameter: f64,
        roughness: f64,
    ) -> Self {
        self.links.push(LinkDef {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            kind: LinkDefKind::Pipe(Pipe {
                length,
                diameter,
                roughness,
                minor_loss: 0.0,
                check_valve: true,
                bulk_coeff: None,
                wall_coeff: None,
                leak_coeff_1: 0.0,
                leak_coeff_2: 0.0,
            }),
            initial_status: LinkStatus::Open,
            initial_setting: Some(1.0),
        });
        self
    }

    /// Add a constant-horsepower pump.
    pub fn const_hp_pump(mut self, id: &str, from: &str, to: &str, power_hp: f64) -> Self {
        self.links.push(LinkDef {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            kind: LinkDefKind::Pump(Pump {
                curve_type: PumpCurveType::ConstHp,
                head_curve: None,
                power: Some(power_hp),
                efficiency_curve: None,
                default_efficiency: 0.75,
                speed_pattern: None,
                energy_price: None,
                price_pattern: None,
            }),
            initial_status: LinkStatus::Open,
            initial_setting: Some(1.0),
        });
        self
    }

    /// Add a pump with a head curve. The curve must be added separately via
    /// [`curve`](TestNetworkBuilder::curve).
    pub fn pump(mut self, id: &str, from: &str, to: &str, head_curve: &str) -> Self {
        self.links.push(LinkDef {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            kind: LinkDefKind::Pump(Pump {
                curve_type: PumpCurveType::Custom,
                head_curve: Some(head_curve.to_string()),
                power: None,
                efficiency_curve: None,
                default_efficiency: 0.75,
                speed_pattern: None,
                energy_price: None,
                price_pattern: None,
            }),
            initial_status: LinkStatus::Open,
            initial_setting: Some(1.0),
        });
        self
    }

    /// Add a valve.
    pub fn valve(
        mut self,
        id: &str,
        from: &str,
        to: &str,
        valve_type: ValveType,
        diameter: f64,
        setting: f64,
    ) -> Self {
        self.links.push(LinkDef {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            kind: LinkDefKind::Valve(Valve {
                valve_type,
                diameter,
                minor_loss: 0.0,
                curve: None,
            }),
            initial_status: LinkStatus::Active,
            initial_setting: Some(setting),
        });
        self
    }

    // ── Patterns & Curves ─────────────────────────────────────────────────────

    /// Add a demand/price pattern.
    pub fn pattern(mut self, id: &str, factors: &[f64]) -> Self {
        self.patterns.push(Pattern {
            id: id.to_string(),
            factors: factors.to_vec(),
        });
        self
    }

    /// Add a curve (pump head, efficiency, volume, etc.).
    pub fn curve(mut self, id: &str, kind: CurveKind, points: &[(f64, f64)]) -> Self {
        self.curves.push(Curve {
            id: id.to_string(),
            kind,
            points: points.iter().map(|&(x, y)| CurvePoint { x, y }).collect(),
        });
        self
    }

    // ── Build ─────────────────────────────────────────────────────────────────

    /// Consume the builder and produce a consistent `(Network, Vec<NodeState>, Vec<LinkState>)`.
    ///
    /// All values passed to the builder are in **user units** (matching the
    /// configured `FlowUnits` — GPM + ft/inches by default). The build step
    /// applies the same unit conversion as the INP parser so the resulting
    /// `Network` is in internal units (CFS, ft).
    ///
    /// Node and link indices are assigned in insertion order (1-based).
    /// `pattern_index` is populated from the patterns list.
    /// `NodeState.head` is initialised to the node's elevation (junctions)
    /// or elevation (reservoirs), or elevation + initial_level (tanks),
    /// all in internal units after conversion.
    ///
    /// # Panics
    ///
    /// Panics if a link references a node ID that was not added.
    pub fn build(self) -> (Network, Vec<NodeState>, Vec<LinkState>) {
        // Build node ID → 1-based index map.
        let id_to_index: HashMap<String, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i + 1))
            .collect();

        // Build pattern_index.
        let pattern_index: HashMap<String, usize> = self
            .patterns
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.clone(), i))
            .collect();

        // Convert NodeDefs to Nodes (still in user units).
        let mut nodes: Vec<Node> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, def)| {
                let base = NodeBase {
                    id: def.id.clone(),
                    index: i + 1,
                    elevation: def.elevation,
                    initial_quality: def.initial_quality,
                };
                let kind = match &def.kind {
                    NodeDefKind::Junction {
                        demands,
                        emitter_coeff,
                    } => NodeKind::Junction(Junction {
                        demands: demands.clone(),
                        emitter_coeff: *emitter_coeff,
                        emitter_exp: 0.5,
                    }),
                    NodeDefKind::Reservoir { head_pattern } => NodeKind::Reservoir(Reservoir {
                        head_pattern: head_pattern.clone(),
                    }),
                    NodeDefKind::Tank {
                        min_level,
                        max_level,
                        initial_level,
                        diameter,
                        min_volume,
                        volume_curve,
                        mix_model,
                        mix_fraction,
                        bulk_coeff,
                        overflow,
                    } => NodeKind::Tank(Tank {
                        min_level: *min_level,
                        max_level: *max_level,
                        initial_level: *initial_level,
                        diameter: *diameter,
                        min_volume: *min_volume,
                        volume_curve: volume_curve.clone(),
                        mix_model: *mix_model,
                        mix_fraction: *mix_fraction,
                        bulk_coeff: *bulk_coeff,
                        overflow: *overflow,
                        head_pattern: None,
                    }),
                };
                Node {
                    base,
                    kind,
                    source: None,
                }
            })
            .collect();

        // Convert LinkDefs to Links (still in user units).
        let mut links: Vec<Link> = self
            .links
            .iter()
            .enumerate()
            .map(|(i, def)| {
                let from_idx = *id_to_index.get(&def.from).unwrap_or_else(|| {
                    panic!("unknown from-node '{}' on link '{}'", def.from, def.id)
                });
                let to_idx = *id_to_index
                    .get(&def.to)
                    .unwrap_or_else(|| panic!("unknown to-node '{}' on link '{}'", def.to, def.id));
                let base = LinkBase {
                    id: def.id.clone(),
                    index: i + 1,
                    from_node: from_idx,
                    to_node: to_idx,
                    initial_status: def.initial_status,
                    initial_setting: def.initial_setting,
                };
                let kind = match &def.kind {
                    LinkDefKind::Pipe(p) => LinkKind::Pipe(p.clone()),
                    LinkDefKind::Pump(p) => LinkKind::Pump(p.clone()),
                    LinkDefKind::Valve(v) => LinkKind::Valve(v.clone()),
                };
                Link { base, kind }
            })
            .collect();

        // Apply unit conversion (user units → internal units), matching the
        // INP parser's apply_unit_conversion. This mutates options, nodes,
        // links, and curves in-place.
        let mut options = self.options;
        let mut curves = self.curves;
        let mut controls = vec![];
        let mut rules = vec![];
        apply_unit_conversion(
            &mut options,
            &mut nodes,
            &mut links,
            &mut curves,
            &mut controls,
            &mut rules,
        );

        // Initialise node states (now in internal units).
        let node_states: Vec<NodeState> = nodes
            .iter()
            .map(|node| {
                let head = match &node.kind {
                    NodeKind::Junction(_) | NodeKind::Reservoir(_) => node.base.elevation,
                    NodeKind::Tank(t) => t.head_from_level(node.base.elevation, t.initial_level),
                };
                let (level, volume) = match &node.kind {
                    NodeKind::Tank(t) => {
                        let lvl = t.initial_level;
                        let vol = t.volume_from_level(lvl, &curves);
                        (lvl, vol)
                    }
                    _ => (0.0, 0.0),
                };
                NodeState {
                    head,
                    quality: node.base.initial_quality,
                    level,
                    volume,
                    ..NodeState::default()
                }
            })
            .collect();

        // Initialise link states (settings already converted).
        let link_states: Vec<LinkState> = links
            .iter()
            .map(|link| LinkState {
                flow: 0.0,
                status: link.base.initial_status,
                setting: link.base.initial_setting.unwrap_or(1.0),
                quality: 0.0,
                reaction_rate: 0.0,
            })
            .collect();

        let network = Network {
            title: vec![],
            options,
            patterns: self.patterns,
            curves,
            nodes,
            links,
            controls,
            rules,
            pattern_index,
            report: ReportOptions::default(),
            coordinates: HashMap::new(),
            vertices: HashMap::new(),
            node_tags: HashMap::new(),
            link_tags: HashMap::new(),
        };

        (network, node_states, link_states)
    }

    /// Build the network and also compute FAVAD coefficients.
    ///
    /// Convenience for tests that call `build_solver_context`, which requires
    /// `&FavadCoeffs`.
    pub fn build_with_favad(self) -> (Network, Vec<NodeState>, Vec<LinkState>, FavadCoeffs) {
        let (network, node_states, link_states) = self.build();
        let favad = network.compute_favad();
        (network, node_states, link_states, favad)
    }
}

impl Default for TestNetworkBuilder {
    fn default() -> Self {
        Self::new()
    }
}
