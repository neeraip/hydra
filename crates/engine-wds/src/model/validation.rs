use super::network::*;

// ── §2.9 Validation ───────────────────────────────────────────────────────────

/// A validation error produced by [`Network::validate`] (§2.9).
///
/// Each variant identifies the offending object by its string ID and the
/// constraint violated — as required by §8.1.2.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    // Check 1 — link node index out of range
    /// `from_node` index of a link names a node that does not exist.
    LinkUnknownFromNode {
        /// String ID of the offending link.
        link_id: String,
        /// The out-of-range node index stored in `from_node`.
        node_index: usize,
    },
    /// `to_node` index of a link names a node that does not exist.
    LinkUnknownToNode {
        /// String ID of the offending link.
        link_id: String,
        /// The out-of-range node index stored in `to_node`.
        node_index: usize,
    },

    // Check 2 — ID cross-references
    /// A pattern ID referenced by an object does not exist.
    UnknownPatternRef {
        /// String ID of the object that holds the reference.
        object_id: String,
        /// The unresolvable pattern string ID.
        pattern_id: String,
    },
    /// A curve ID referenced by an object does not exist.
    UnknownCurveRef {
        /// String ID of the object that holds the reference.
        object_id: String,
        /// The unresolvable curve string ID.
        curve_id: String,
    },
    /// A curve exists but has the wrong kind for the reference.
    WrongCurveKind {
        /// String ID of the object that holds the reference.
        object_id: String,
        /// String ID of the referenced curve.
        curve_id: String,
        /// The curve kind required by the reference.
        expected: CurveKind,
        /// The curve kind actually found.
        actual: CurveKind,
    },
    /// A required curve reference (e.g. pump head curve) is absent.
    MissingRequiredCurve {
        /// String ID of the object that requires the curve.
        object_id: String,
        /// The curve kind that is missing.
        expected_kind: CurveKind,
    },
    /// A node string ID referenced by an object does not exist.
    UnknownNodeIdRef {
        /// String ID of the object that holds the reference.
        object_id: String,
        /// The unresolvable node string ID.
        node_id: String,
    },
    /// A node index in a control or rule premise is out of range.
    UnknownNodeIndexRef {
        /// String ID of the control or rule that holds the reference.
        object_id: String,
        /// The out-of-range node index.
        node_index: usize,
    },
    /// A link index in a rule premise is out of range.
    UnknownLinkIndexRef {
        /// String ID of the rule that holds the reference.
        object_id: String,
        /// The out-of-range link index.
        link_index: usize,
    },

    // Check 3
    /// A link connects a node to itself.
    LinkSelfLoop {
        /// String ID of the self-looping link.
        link_id: String,
    },

    // Check 4
    /// The network has no reservoir (fixed-grade node).
    NoReservoir,
    /// A junction or tank not reachable from any reservoir.
    NodeNotReachable {
        /// String ID of the isolated node.
        node_id: String,
    },

    // Check 5
    /// A tank's `initial_level` is outside `[min_level, max_level]`.
    TankLevelOutOfRange {
        /// String ID of the tank node.
        node_id: String,
        /// Minimum level configured on the tank (m).
        min_level: f64,
        /// Initial level configured on the tank (m).
        initial_level: f64,
        /// Maximum level configured on the tank (m).
        max_level: f64,
    },

    // Check 6
    /// A `PUMP_HEAD` curve's y-values are not strictly decreasing.
    PumpCurveNotDecreasing {
        /// String ID of the offending curve.
        curve_id: String,
    },
    /// A `PUMP_EFFICIENCY` curve's y-values are not all in `(0, 100]`.
    EfficiencyCurveYOutOfRange {
        /// String ID of the offending curve.
        curve_id: String,
    },
    /// A `TANK_VOLUME` curve's y-values are not strictly increasing.
    TankVolumeCurveYNotIncreasing {
        /// String ID of the offending curve.
        curve_id: String,
    },
    /// A `GPV_HEADLOSS` curve's y-values are not non-decreasing.
    GpvHeadlossCurveYDecreasing {
        /// String ID of the offending curve.
        curve_id: String,
    },

    // Check 7
    /// A curve's x-values are not strictly increasing.
    CurveXNotIncreasing {
        /// String ID of the offending curve.
        curve_id: String,
    },

    // Check 8
    /// A pattern contains no factors.
    PatternEmpty {
        /// String ID of the empty pattern.
        pattern_id: String,
    },

    // Check 9
    /// A rule action references a link index that is out of range.
    RuleActionUnknownLink {
        /// Priority of the rule containing the offending action.
        rule_priority: f64,
        /// The out-of-range link index stored in the action.
        link_index: usize,
    },

    // Check 10
    /// A curve has fewer than 2 data points (§2.3 requires length ≥ 2).
    CurveTooFewPoints {
        /// String ID of the offending curve.
        curve_id: String,
        /// Number of points actually present.
        count: usize,
    },

    // Check 11
    /// A simple control references a link index that is out of range.
    ControlUnknownLink {
        /// Zero-based position of the offending control in `Network::controls`.
        control_index: usize,
        /// The out-of-range 1-based link index stored in the control.
        link_index: usize,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LinkUnknownFromNode {
                link_id,
                node_index,
            } => write!(
                f,
                "link '{link_id}' references unknown from-node index {node_index}"
            ),
            Self::LinkUnknownToNode {
                link_id,
                node_index,
            } => write!(
                f,
                "link '{link_id}' references unknown to-node index {node_index}"
            ),
            Self::UnknownPatternRef {
                object_id,
                pattern_id,
            } => write!(f, "'{object_id}' references unknown pattern '{pattern_id}'"),
            Self::UnknownCurveRef {
                object_id,
                curve_id,
            } => write!(f, "'{object_id}' references unknown curve '{curve_id}'"),
            Self::WrongCurveKind {
                object_id,
                curve_id,
                expected,
                actual,
            } => write!(
                f,
                "'{object_id}' expects {expected:?} curve but '{curve_id}' is {actual:?}"
            ),
            Self::MissingRequiredCurve {
                object_id,
                expected_kind,
            } => write!(
                f,
                "'{object_id}' requires a {expected_kind:?} curve but none is assigned"
            ),
            Self::UnknownNodeIdRef { object_id, node_id } => {
                write!(f, "'{object_id}' references unknown node '{node_id}'")
            }
            Self::UnknownNodeIndexRef {
                object_id,
                node_index,
            } => write!(
                f,
                "'{object_id}' references unknown node index {node_index}"
            ),
            Self::UnknownLinkIndexRef {
                object_id,
                link_index,
            } => write!(
                f,
                "'{object_id}' references unknown link index {link_index}"
            ),
            Self::LinkSelfLoop { link_id } => {
                write!(f, "link '{link_id}' connects a node to itself")
            }
            Self::NoReservoir => write!(f, "network has no reservoir"),
            Self::NodeNotReachable { node_id } => {
                write!(f, "node '{node_id}' is not reachable from any reservoir")
            }
            Self::TankLevelOutOfRange {
                node_id,
                min_level,
                initial_level: init_level,
                max_level,
            } => write!(
                f,
                "tank '{node_id}' init level {init_level} is outside [{min_level}, {max_level}]"
            ),
            Self::PumpCurveNotDecreasing { curve_id } => {
                write!(f, "pump head curve '{curve_id}' is not strictly decreasing")
            }
            Self::EfficiencyCurveYOutOfRange { curve_id } => write!(
                f,
                "efficiency curve '{curve_id}' has y-values outside (0, 100]"
            ),
            Self::TankVolumeCurveYNotIncreasing { curve_id } => write!(
                f,
                "tank volume curve '{curve_id}' is not strictly increasing"
            ),
            Self::GpvHeadlossCurveYDecreasing { curve_id } => {
                write!(f, "GPV headloss curve '{curve_id}' has decreasing y-values")
            }
            Self::CurveXNotIncreasing { curve_id } => {
                write!(f, "curve '{curve_id}' has non-increasing x-values")
            }
            Self::PatternEmpty { pattern_id } => write!(f, "pattern '{pattern_id}' has no factors"),
            Self::RuleActionUnknownLink {
                rule_priority,
                link_index,
            } => write!(
                f,
                "rule (priority {rule_priority}) references unknown link index {link_index}"
            ),
            Self::CurveTooFewPoints { curve_id, count } => {
                write!(f, "curve '{curve_id}' has {count} point(s), minimum is 2")
            }
            Self::ControlUnknownLink {
                control_index,
                link_index,
            } => write!(
                f,
                "control {control_index} references unknown link index {link_index}"
            ),
        }
    }
}

impl Network {
    /// Validates the network against all topology and referential-integrity constraints.
    ///
    /// Returns `Ok(())` if every constraint is satisfied. Returns
    /// `Err(errors)` with every violation found — never stops at the first
    /// error, so the caller can report all problems at once. An invalid
    /// network must not be used for simulation.
    ///
    /// The following constraints must all hold:
    ///
    /// 1. Every node index referenced by a link exists in the node table.
    /// 2. Every curve, pattern, or node ID referenced by any object exists in
    ///    the corresponding table.
    /// 3. No link connects a node to itself (`from_node` ≠ `to_node`).
    /// 4. The network contains at least one fixed-grade node (reservoir or
    ///    tank). Every junction is reachable from at least one fixed-grade node
    ///    via the link graph.
    /// 5. For each tank: `min_level` ≤ `init_level` ≤ `max_level`.
    /// 6. All `PUMP_HEAD` curves are strictly decreasing in *y*.
    /// 7. All curves have strictly increasing *x*-values.
    /// 8. All patterns have at least one factor.
    /// 9. Every rule action that references a link references a valid link index.
    /// 10. `wall_order` is 0 or 1; no other value is valid.
    ///
    /// Violations of any of the above are fatal — the simulation must not proceed.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        use std::collections::{HashMap, HashSet, VecDeque};

        let mut errors: Vec<ValidationError> = Vec::new();

        let node_count = self.nodes.len();
        let link_count = self.links.len();

        // Lookup tables reused across checks.
        let pattern_ids: HashSet<&str> = self.patterns.iter().map(|p| p.id.as_str()).collect();
        let node_ids: HashSet<&str> = self.nodes.iter().map(|n| n.base.id.as_str()).collect();
        let curve_by_id: HashMap<&str, &Curve> =
            self.curves.iter().map(|c| (c.id.as_str(), c)).collect();

        // Helper: check a pattern ID reference produced by `object_id`.
        macro_rules! chk_pattern {
            ($obj:expr, $pat_id:expr) => {
                if !pattern_ids.contains($pat_id.as_str()) {
                    errors.push(ValidationError::UnknownPatternRef {
                        object_id: $obj.to_string(),
                        pattern_id: $pat_id.clone(),
                    });
                }
            };
        }

        // Helper: check a curve ID reference with an expected kind.
        macro_rules! chk_curve {
            ($obj:expr, $curve_id:expr, $expected:expr) => {
                match curve_by_id.get($curve_id.as_str()) {
                    None => errors.push(ValidationError::UnknownCurveRef {
                        object_id: $obj.to_string(),
                        curve_id: $curve_id.clone(),
                    }),
                    Some(c) if c.kind != $expected => {
                        errors.push(ValidationError::WrongCurveKind {
                            object_id: $obj.to_string(),
                            curve_id: $curve_id.clone(),
                            expected: $expected,
                            actual: c.kind,
                        });
                    }
                    _ => {}
                }
            };
        }

        // ── Check 1: link node index bounds ───────────────────────────────────
        for link in &self.links {
            if link.base.from_node < 1 || link.base.from_node > node_count {
                errors.push(ValidationError::LinkUnknownFromNode {
                    link_id: link.base.id.clone(),
                    node_index: link.base.from_node,
                });
            }
            if link.base.to_node < 1 || link.base.to_node > node_count {
                errors.push(ValidationError::LinkUnknownToNode {
                    link_id: link.base.id.clone(),
                    node_index: link.base.to_node,
                });
            }
        }

        // ── Check 2: ID cross-references ──────────────────────────────────────
        // Global options.
        if let Some(ref pat_id) = self.options.default_pattern {
            chk_pattern!("options", pat_id);
        }
        if let Some(ref pat_id) = self.options.energy_price_pattern {
            chk_pattern!("options", pat_id);
        }
        if let Some(ref nid) = self.options.trace_node {
            if !node_ids.contains(nid.as_str()) {
                errors.push(ValidationError::UnknownNodeIdRef {
                    object_id: "options".to_string(),
                    node_id: nid.clone(),
                });
            }
        }

        // Nodes.
        for node in &self.nodes {
            let oid = &node.base.id;
            match &node.kind {
                NodeKind::Junction(j) => {
                    for demand in &j.demands {
                        if let Some(ref pat_id) = demand.pattern {
                            chk_pattern!(oid, pat_id);
                        }
                    }
                }
                NodeKind::Reservoir(r) => {
                    if let Some(ref pat_id) = r.head_pattern {
                        chk_pattern!(oid, pat_id);
                    }
                }
                NodeKind::Tank(t) => {
                    if let Some(ref curve_id) = t.volume_curve {
                        chk_curve!(oid, curve_id, CurveKind::TankVolume);
                    }
                }
            }
            if let Some(ref src) = node.source {
                if let Some(ref pat_id) = src.pattern {
                    chk_pattern!(oid, pat_id);
                }
            }
        }

        // Links.
        for link in &self.links {
            let oid = &link.base.id;
            match &link.kind {
                LinkKind::Pump(p) => {
                    match p.curve_type {
                        PumpCurveType::ConstHp => {}
                        _ => match &p.head_curve {
                            None => errors.push(ValidationError::MissingRequiredCurve {
                                object_id: oid.clone(),
                                expected_kind: CurveKind::PumpHead,
                            }),
                            Some(curve_id) => {
                                chk_curve!(oid, curve_id, CurveKind::PumpHead);
                            }
                        },
                    }
                    if let Some(ref curve_id) = p.efficiency_curve {
                        chk_curve!(oid, curve_id, CurveKind::PumpEfficiency);
                    }
                    if let Some(ref pat_id) = p.speed_pattern {
                        chk_pattern!(oid, pat_id);
                    }
                    if let Some(ref pat_id) = p.price_pattern {
                        chk_pattern!(oid, pat_id);
                    }
                }
                LinkKind::Valve(v) if v.valve_type == ValveType::Gpv => match &v.curve {
                    None => errors.push(ValidationError::MissingRequiredCurve {
                        object_id: oid.clone(),
                        expected_kind: CurveKind::GpvHeadloss,
                    }),
                    Some(curve_id) => {
                        chk_curve!(oid, curve_id, CurveKind::GpvHeadloss);
                    }
                },
                LinkKind::Valve(v) if v.valve_type == ValveType::Pcv => match &v.curve {
                    None => errors.push(ValidationError::MissingRequiredCurve {
                        object_id: oid.clone(),
                        expected_kind: CurveKind::PcvLossRatio,
                    }),
                    Some(curve_id) => {
                        chk_curve!(oid, curve_id, CurveKind::PcvLossRatio);
                    }
                },
                _ => {}
            }
        }

        // Simple control trigger node indices and link indices.
        for (i, ctrl) in self.controls.iter().enumerate() {
            if ctrl.link < 1 || ctrl.link > link_count {
                errors.push(ValidationError::ControlUnknownLink {
                    control_index: i,
                    link_index: ctrl.link,
                });
            }
            if let Some(idx) = ctrl.trigger_node {
                if idx < 1 || idx > node_count {
                    errors.push(ValidationError::UnknownNodeIndexRef {
                        object_id: format!("control[{i}]"),
                        node_index: idx,
                    });
                }
            }
        }

        // Rule premise node/link indices.
        for rule in &self.rules {
            let oid = format!("rule[priority={}]", rule.priority);
            for premise in &rule.premises {
                match premise.object {
                    PremiseObject::Node(idx) => {
                        if idx < 1 || idx > node_count {
                            errors.push(ValidationError::UnknownNodeIndexRef {
                                object_id: oid.clone(),
                                node_index: idx,
                            });
                        }
                    }
                    PremiseObject::Link(idx) => {
                        if idx < 1 || idx > link_count {
                            errors.push(ValidationError::UnknownLinkIndexRef {
                                object_id: oid.clone(),
                                link_index: idx,
                            });
                        }
                    }
                    PremiseObject::Clock => {}
                }
            }
        }

        // ── Check 3: no self-loops ─────────────────────────────────────────────
        for link in &self.links {
            if link.base.from_node == link.base.to_node {
                errors.push(ValidationError::LinkSelfLoop {
                    link_id: link.base.id.clone(),
                });
            }
        }

        // ── Check 4: every junction/tank reachable from a reservoir ───────────
        // Build undirected adjacency list (0-based). Guard on index validity to
        // avoid out-of-bounds access when check 1 already found bad indices.
        let mut adj: Vec<Vec<usize>> = vec![vec![]; node_count];
        for link in &self.links {
            let f = link.base.from_node;
            let t = link.base.to_node;
            if f >= 1 && f <= node_count && t >= 1 && t <= node_count {
                adj[f - 1].push(t - 1);
                adj[t - 1].push(f - 1);
            }
        }

        let reservoir_indices: Vec<usize> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.kind, NodeKind::Reservoir(_)))
            .map(|(i, _)| i)
            .collect();

        let tank_indices: Vec<usize> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.kind, NodeKind::Tank(_)))
            .map(|(i, _)| i)
            .collect();

        // Need at least one fixed-grade node (reservoir or tank).
        let fixed_grade_indices: Vec<usize> = reservoir_indices
            .iter()
            .chain(tank_indices.iter())
            .copied()
            .collect();

        if fixed_grade_indices.is_empty() {
            errors.push(ValidationError::NoReservoir);
        } else {
            let mut visited = vec![false; node_count];
            let mut queue: VecDeque<usize> = VecDeque::new();
            for &r in &fixed_grade_indices {
                visited[r] = true;
                queue.push_back(r);
            }
            while let Some(u) = queue.pop_front() {
                for &v in &adj[u] {
                    if !visited[v] {
                        visited[v] = true;
                        queue.push_back(v);
                    }
                }
            }
            for (i, node) in self.nodes.iter().enumerate() {
                if !visited[i] && matches!(node.kind, NodeKind::Junction(_) | NodeKind::Tank(_)) {
                    errors.push(ValidationError::NodeNotReachable {
                        node_id: node.base.id.clone(),
                    });
                }
            }
        }

        // ── Check 5: tank level bounds ─────────────────────────────────────────
        for node in &self.nodes {
            if let NodeKind::Tank(t) = &node.kind {
                if t.initial_level < t.min_level || t.initial_level > t.max_level {
                    errors.push(ValidationError::TankLevelOutOfRange {
                        node_id: node.base.id.clone(),
                        min_level: t.min_level,
                        initial_level: t.initial_level,
                        max_level: t.max_level,
                    });
                }
            }
        }

        // ── Check 6: curve y-value invariants by kind ───────────────────────
        for curve in &self.curves {
            match curve.kind {
                CurveKind::PumpHead => {
                    let ok = curve.points.windows(2).all(|w| w[1].y < w[0].y);
                    if !ok {
                        errors.push(ValidationError::PumpCurveNotDecreasing {
                            curve_id: curve.id.clone(),
                        });
                    }
                }
                CurveKind::PumpEfficiency => {
                    let ok = curve.points.iter().all(|p| p.y >= 0.0 && p.y <= 100.0);
                    if !ok {
                        errors.push(ValidationError::EfficiencyCurveYOutOfRange {
                            curve_id: curve.id.clone(),
                        });
                    }
                }
                CurveKind::TankVolume => {
                    let ok = curve.points.windows(2).all(|w| w[1].y > w[0].y);
                    if !ok {
                        errors.push(ValidationError::TankVolumeCurveYNotIncreasing {
                            curve_id: curve.id.clone(),
                        });
                    }
                }
                CurveKind::GpvHeadloss => {
                    let ok = curve.points.windows(2).all(|w| w[1].y >= w[0].y);
                    if !ok {
                        errors.push(ValidationError::GpvHeadlossCurveYDecreasing {
                            curve_id: curve.id.clone(),
                        });
                    }
                }
                _ => {}
            }
        }

        // ── Check 10: all curves have ≥ 2 data points (§2.3) ─────────────────
        // Generic (untagged) and PumpEfficiency curves are exempt: EPANET allows
        // single-point efficiency curves (interpreted as constant efficiency).
        for curve in &self.curves {
            if curve.points.len() < 2
                && curve.kind != CurveKind::Generic
                && curve.kind != CurveKind::PumpEfficiency
            {
                errors.push(ValidationError::CurveTooFewPoints {
                    curve_id: curve.id.clone(),
                    count: curve.points.len(),
                });
            }
        }

        // ── Check 7: all curves have strictly increasing x ────────────────────
        for curve in &self.curves {
            let ok = curve.points.windows(2).all(|w| w[1].x > w[0].x);
            if !ok {
                errors.push(ValidationError::CurveXNotIncreasing {
                    curve_id: curve.id.clone(),
                });
            }
        }

        // ── Check 8: all patterns non-empty ───────────────────────────────────
        for pattern in &self.patterns {
            if pattern.factors.is_empty() {
                errors.push(ValidationError::PatternEmpty {
                    pattern_id: pattern.id.clone(),
                });
            }
        }

        // ── Check 9: rule action link indices ─────────────────────────────────
        for rule in &self.rules {
            for action in rule.then_actions.iter().chain(rule.else_actions.iter()) {
                if action.link < 1 || action.link > link_count {
                    errors.push(ValidationError::RuleActionUnknownLink {
                        rule_priority: rule.priority,
                        link_index: action.link,
                    });
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Minimal two-node (reservoir + junction) + one-pipe network.
    fn make_simple() -> Network {
        Network {
            title: vec![],
            options: SimulationOptions::default(),
            patterns: vec![],
            curves: vec![],
            nodes: vec![
                Node {
                    base: NodeBase {
                        id: "R1".into(),
                        index: 1,
                        elevation: 100.0,
                        initial_quality: 0.0,
                    },
                    kind: NodeKind::Reservoir(Reservoir { head_pattern: None }),
                    source: None,
                },
                Node {
                    base: NodeBase {
                        id: "J1".into(),
                        index: 2,
                        elevation: 0.0,
                        initial_quality: 0.0,
                    },
                    kind: NodeKind::Junction(Junction {
                        demands: vec![DemandCategory {
                            base_demand: 0.01,
                            pattern: None,
                            name: None,
                        }],
                        emitter_coeff: 0.0,
                        emitter_exp: 0.5,
                    }),
                    source: None,
                },
            ],
            links: vec![Link {
                base: LinkBase {
                    id: "P1".into(),
                    index: 1,
                    from_node: 1,
                    to_node: 2,
                    initial_status: LinkStatus::Open,
                    initial_setting: Some(1.0),
                },
                kind: LinkKind::Pipe(Pipe {
                    length: 1000.0,
                    diameter: 0.3,
                    roughness: 100.0,
                    minor_loss: 0.0,
                    check_valve: false,
                    bulk_coeff: None,
                    wall_coeff: None,
                    leak_coeff_1: 0.0,
                    leak_coeff_2: 0.0,
                }),
            }],
            controls: vec![],
            rules: vec![],
            pattern_index: HashMap::new(),
            report: ReportOptions::default(),
            coordinates: HashMap::new(),
            vertices: HashMap::new(),
            node_tags: HashMap::new(),
            link_tags: HashMap::new(),
        }
    }

    #[test]
    fn valid_network_passes_validation() {
        assert!(make_simple().validate().is_ok());
    }

    // ── Check 1: link node index bounds ──────────────────────────────────────

    #[test]
    fn link_unknown_from_node_detected() {
        let mut net = make_simple();
        net.links[0].base.from_node = 99;
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::LinkUnknownFromNode { .. })),
            "expected LinkUnknownFromNode"
        );
    }

    #[test]
    fn link_unknown_to_node_detected() {
        let mut net = make_simple();
        net.links[0].base.to_node = 99;
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::LinkUnknownToNode { .. })),
            "expected LinkUnknownToNode"
        );
    }

    // ── Check 2: ID cross-references ──────────────────────────────────────────

    #[test]
    fn unknown_pattern_ref_detected() {
        let mut net = make_simple();
        if let NodeKind::Junction(j) = &mut net.nodes[1].kind {
            j.demands[0].pattern = Some("NO_SUCH_PAT".into());
        }
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::UnknownPatternRef { .. })),
            "expected UnknownPatternRef"
        );
    }

    #[test]
    fn missing_required_pump_curve_detected() {
        let mut net = make_simple();
        // Extra junction so the pump has a valid from/to.
        net.nodes.push(Node {
            base: NodeBase {
                id: "J2".into(),
                index: 3,
                elevation: 0.0,
                initial_quality: 0.0,
            },
            kind: NodeKind::Junction(Junction {
                demands: vec![DemandCategory {
                    base_demand: 0.01,
                    pattern: None,
                    name: None,
                }],
                emitter_coeff: 0.0,
                emitter_exp: 0.5,
            }),
            source: None,
        });
        net.links.push(Link {
            base: LinkBase {
                id: "PU1".into(),
                index: 2,
                from_node: 2,
                to_node: 3,
                initial_status: LinkStatus::Open,
                initial_setting: Some(1.0),
            },
            kind: LinkKind::Pump(Pump {
                curve_type: PumpCurveType::PowerFunction,
                head_curve: None, // required but absent
                power: None,
                efficiency_curve: None,
                default_efficiency: 0.75,
                speed_pattern: None,
                energy_price: None,
                price_pattern: None,
            }),
        });
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::MissingRequiredCurve { .. })),
            "expected MissingRequiredCurve"
        );
    }

    // ── Check 3: no self-loops ────────────────────────────────────────────────

    #[test]
    fn link_self_loop_detected() {
        let mut net = make_simple();
        net.links[0].base.to_node = 1; // same as from_node
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::LinkSelfLoop { .. })),
            "expected LinkSelfLoop"
        );
    }

    // ── Check 4: connectivity ─────────────────────────────────────────────────

    #[test]
    fn no_reservoir_detected() {
        let mut net = make_simple();
        net.nodes[0].kind = NodeKind::Junction(Junction {
            demands: vec![DemandCategory {
                base_demand: 0.0,
                pattern: None,
                name: None,
            }],
            emitter_coeff: 0.0,
            emitter_exp: 0.5,
        });
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::NoReservoir)),
            "expected NoReservoir"
        );
    }

    #[test]
    fn node_not_reachable_detected() {
        let mut net = make_simple();
        net.nodes.push(Node {
            base: NodeBase {
                id: "J2".into(),
                index: 3,
                elevation: 0.0,
                initial_quality: 0.0,
            },
            kind: NodeKind::Junction(Junction {
                demands: vec![DemandCategory {
                    base_demand: 0.01,
                    pattern: None,
                    name: None,
                }],
                emitter_coeff: 0.0,
                emitter_exp: 0.5,
            }),
            source: None,
        });
        // No link connecting J2 — it is unreachable.
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter().any(|e| matches!(
                e,
                ValidationError::NodeNotReachable { node_id } if node_id == "J2"
            )),
            "expected NodeNotReachable for J2"
        );
    }

    // ── Check 5: tank level bounds ────────────────────────────────────────────

    #[test]
    fn tank_level_out_of_range_detected() {
        let mut net = make_simple();
        net.nodes[1].kind = NodeKind::Tank(Tank {
            min_level: 1.0,
            max_level: 5.0,
            initial_level: 0.5, // below min_level
            diameter: 10.0,
            min_volume: 0.0,
            volume_curve: None,
            mix_model: MixModel::Cstr,
            mix_fraction: 1.0,
            bulk_coeff: 0.0,
            overflow: false,
        });
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::TankLevelOutOfRange { .. })),
            "expected TankLevelOutOfRange"
        );
    }

    // ── Check 6: curve y-value invariants ────────────────────────────────────

    #[test]
    fn pump_curve_not_decreasing_detected() {
        let mut net = make_simple();
        net.curves.push(Curve {
            id: "HC1".into(),
            kind: CurveKind::PumpHead,
            points: vec![
                CurvePoint { x: 0.0, y: 50.0 },
                CurvePoint { x: 1.0, y: 60.0 }, // increasing — invalid
                CurvePoint { x: 2.0, y: 40.0 },
            ],
        });
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::PumpCurveNotDecreasing { .. })),
            "expected PumpCurveNotDecreasing"
        );
    }

    #[test]
    fn efficiency_curve_out_of_range_detected() {
        let mut net = make_simple();
        net.curves.push(Curve {
            id: "EFF1".into(),
            kind: CurveKind::PumpEfficiency,
            points: vec![CurvePoint { x: 0.5, y: 150.0 }], // >100 — invalid
        });
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::EfficiencyCurveYOutOfRange { .. })),
            "expected EfficiencyCurveYOutOfRange"
        );
    }

    #[test]
    fn tank_volume_curve_not_increasing_detected() {
        let mut net = make_simple();
        net.curves.push(Curve {
            id: "VOL1".into(),
            kind: CurveKind::TankVolume,
            points: vec![
                CurvePoint { x: 0.0, y: 100.0 },
                CurvePoint { x: 1.0, y: 80.0 }, // decreasing — invalid
            ],
        });
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::TankVolumeCurveYNotIncreasing { .. })),
            "expected TankVolumeCurveYNotIncreasing"
        );
    }

    #[test]
    fn gpv_headloss_curve_decreasing_detected() {
        let mut net = make_simple();
        net.curves.push(Curve {
            id: "GPV1".into(),
            kind: CurveKind::GpvHeadloss,
            points: vec![
                CurvePoint { x: 0.0, y: 10.0 },
                CurvePoint { x: 1.0, y: 5.0 }, // decreasing — invalid
            ],
        });
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::GpvHeadlossCurveYDecreasing { .. })),
            "expected GpvHeadlossCurveYDecreasing"
        );
    }

    // ── Check 7: curve x strictly increasing ─────────────────────────────────

    #[test]
    fn curve_x_not_increasing_detected() {
        let mut net = make_simple();
        net.curves.push(Curve {
            id: "HC2".into(),
            kind: CurveKind::PumpHead,
            points: vec![
                CurvePoint { x: 2.0, y: 80.0 },
                CurvePoint { x: 1.0, y: 60.0 }, // x decreasing — invalid
                CurvePoint { x: 0.5, y: 40.0 },
            ],
        });
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::CurveXNotIncreasing { .. })),
            "expected CurveXNotIncreasing"
        );
    }

    // ── Check 8: patterns non-empty ───────────────────────────────────────────

    #[test]
    fn pattern_empty_detected() {
        let mut net = make_simple();
        net.patterns.push(Pattern {
            id: "PAT1".into(),
            factors: vec![],
        });
        net.build_pattern_index();
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::PatternEmpty { .. })),
            "expected PatternEmpty"
        );
    }

    // ── Check 10: curve must have ≥ 2 points ─────────────────────────────────

    #[test]
    fn curve_too_few_points_detected() {
        let mut net = make_simple();
        net.curves.push(Curve {
            id: "HC_TINY".into(),
            kind: CurveKind::PumpHead,
            points: vec![CurvePoint { x: 1.0, y: 50.0 }], // only 1 point
        });
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::CurveTooFewPoints { .. })),
            "expected CurveTooFewPoints"
        );
    }

    // ── Check 11: simple control link index ──────────────────────────────────

    #[test]
    fn control_unknown_link_detected() {
        let mut net = make_simple();
        net.controls.push(SimpleControl {
            link: 99,
            trigger_type: TriggerType::Timer,
            trigger_time: Some(3600.0),
            trigger_node: None,
            trigger_grade: None,
            action_status: Some(LinkStatus::Closed),
            action_setting: None,
            enabled: true,
        });
        let errs = net.validate().unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::ControlUnknownLink { .. })),
            "expected ControlUnknownLink"
        );
    }

    // ── Multi-error collection ────────────────────────────────────────────────

    #[test]
    fn multiple_errors_all_collected() {
        let mut net = make_simple();
        net.links[0].base.from_node = 99;
        net.links[0].base.to_node = 99;
        let errs = net.validate().unwrap_err();
        // Expect at least LinkUnknownFromNode + LinkUnknownToNode.
        assert!(
            errs.len() >= 2,
            "expected ≥2 errors collected, got {}",
            errs.len()
        );
    }
}
