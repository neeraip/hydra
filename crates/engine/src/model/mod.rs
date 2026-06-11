#![doc = include_str!("spec.md")]

mod network;
mod state;
mod validation;

pub use network::*;
pub use state::*;
pub use validation::*;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── Minimal valid network builder ─────────────────────────────────────────
    //
    // One reservoir (index 1) + one junction (index 2) + one pipe (index 1).
    // All §2.9 constraints are satisfied. Tests mutate a clone of this.

    fn minimal_options() -> SimulationOptions {
        SimulationOptions {
            duration: 3600.0,
            hyd_step: 3600.0,
            qual_step: 300.0,
            report_step: 3600.0,
            report_start: 0.0,
            pattern_step: 3600.0,
            pattern_start: 0.0,
            start_clocktime: 0.0,
            head_loss_formula: HeadLossFormula::HazenWilliams,
            demand_model: DemandModel::DemandDriven,
            flow_units: FlowUnits::Gpm,
            viscosity: 1.022e-6,
            diffusivity: 1.208e-9,
            specific_gravity: 1.0,
            demand_multiplier: 1.0,
            default_pattern: None,
            pda_min_pressure: 0.0,
            pda_required_pressure: 30.0,
            pda_pressure_exponent: 0.5,
            emitter_backflow: true,
            quality_mode: QualityMode::None,
            trace_node: None,
            chem_name: String::new(),
            chem_units: String::new(),
            max_iter: 200,
            extra_iter: -1,
            head_tol: 1.524e-4,
            flow_change_tol: 2.832e-6,
            flow_tol: 0.001,
            head_error_limit: 0.0,
            flow_change_limit: 0.0,
            rq_tol: 1.0e-7,
            damp_limit: 0.0,
            check_freq: 2,
            max_check: 10,
            bulk_order: 1.0,
            tank_order: 1.0,
            wall_order: WallOrder::One,
            bulk_coeff: 0.0,
            wall_coeff: 0.0,
            conc_limit: 0.0,
            energy_price: 0.0,
            energy_price_pattern: None,
            energy_efficiency: 0.75,
            peak_demand_charge: 0.0,
            roughness_reaction_factor: 0.0,
            rule_timestep: 360.0,
            quality_tolerance: 0.01,
            statistic: StatisticType::Series,
        }
    }

    fn reservoir(id: &str, index: usize) -> Node {
        Node {
            base: NodeBase {
                id: id.to_string(),
                index,
                elevation: 100.0,
                initial_quality: 0.0,
            },
            kind: NodeKind::Reservoir(Reservoir { head_pattern: None }),
            source: None,
        }
    }

    fn junction(id: &str, index: usize) -> Node {
        Node {
            base: NodeBase {
                id: id.to_string(),
                index,
                elevation: 0.0,
                initial_quality: 0.0,
            },
            kind: NodeKind::Junction(Junction {
                demands: vec![],
                emitter_coeff: 0.0,
                emitter_exp: 0.5,
            }),
            source: None,
        }
    }

    fn pipe(id: &str, index: usize, from: usize, to: usize) -> Link {
        Link {
            base: LinkBase {
                id: id.to_string(),
                index,
                from_node: from,
                to_node: to,
                initial_status: LinkStatus::Open,
                initial_setting: Some(1.0),
            },
            kind: LinkKind::Pipe(Pipe {
                length: 1000.0,
                diameter: 1.0,
                roughness: 100.0,
                minor_loss: 0.0,
                check_valve: false,
                bulk_coeff: None,
                wall_coeff: None,
                leak_coeff_1: 0.0,
                leak_coeff_2: 0.0,
            }),
        }
    }

    fn minimal_network() -> Network {
        Network {
            options: minimal_options(),
            patterns: vec![],
            curves: vec![],
            nodes: vec![reservoir("R1", 1), junction("J1", 2)],
            links: vec![pipe("P1", 1, 1, 2)],
            controls: vec![],
            rules: vec![],
            title: vec![],
            pattern_index: HashMap::new(),
            report: ReportOptions::default(),
            coordinates: HashMap::new(),
            vertices: HashMap::new(),
            node_tags: HashMap::new(),
            link_tags: HashMap::new(),
        }
    }

    // Helper: check if any error matches a predicate.
    fn contains<F: Fn(&ValidationError) -> bool>(errors: &[ValidationError], f: F) -> bool {
        errors.iter().any(f)
    }

    // ── Check 0: minimal valid network passes ─────────────────────────────────

    #[test]
    fn valid_minimal_network_passes() {
        assert!(minimal_network().validate().is_ok());
    }

    // ── Check 1: link node index bounds ──────────────────────────────────────

    #[test]
    fn check1_link_from_node_out_of_range() {
        let mut net = minimal_network();
        net.links[0].base.from_node = 99;
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::LinkUnknownFromNode { link_id, .. } if link_id == "P1"
        )));
    }

    #[test]
    fn check1_link_to_node_out_of_range() {
        let mut net = minimal_network();
        net.links[0].base.to_node = 0; // 0 is never a valid 1-based index
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::LinkUnknownToNode { link_id, .. } if link_id == "P1"
        )));
    }

    // ── Check 2: unknown pattern reference ────────────────────────────────────

    #[test]
    fn check2_unknown_pattern_on_options() {
        let mut net = minimal_network();
        net.options.default_pattern = Some("no_such_pattern".to_string());
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::UnknownPatternRef { pattern_id, .. } if pattern_id == "no_such_pattern"
        )));
    }

    #[test]
    fn check2_unknown_curve_on_pump() {
        let mut net = minimal_network();
        net.links.push(Link {
            base: LinkBase {
                id: "PMP1".to_string(),
                index: 2,
                from_node: 1,
                to_node: 2,
                initial_status: LinkStatus::Open,
                initial_setting: Some(1.0),
            },
            kind: LinkKind::Pump(Pump {
                curve_type: PumpCurveType::Custom,
                head_curve: Some("missing_curve".to_string()),
                power: None,
                efficiency_curve: None,
                default_efficiency: 0.0,
                speed_pattern: None,
                energy_price: None,
                price_pattern: None,
            }),
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::UnknownCurveRef { object_id, curve_id }
                if object_id == "PMP1" && curve_id == "missing_curve"
        )));
    }

    #[test]
    fn check2_missing_head_curve_on_pump() {
        let mut net = minimal_network();
        net.links.push(Link {
            base: LinkBase {
                id: "PMP2".to_string(),
                index: 2,
                from_node: 1,
                to_node: 2,
                initial_status: LinkStatus::Open,
                initial_setting: Some(1.0),
            },
            kind: LinkKind::Pump(Pump {
                curve_type: PumpCurveType::Custom,
                head_curve: None,
                power: None,
                efficiency_curve: None,
                default_efficiency: 0.0,
                speed_pattern: None,
                energy_price: None,
                price_pattern: None,
            }),
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::MissingRequiredCurve { object_id, .. } if object_id == "PMP2"
        )));
    }

    #[test]
    fn check2_unknown_trace_node() {
        let mut net = minimal_network();
        net.options.quality_mode = QualityMode::Trace;
        net.options.trace_node = Some("ghost".to_string());
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::UnknownNodeIdRef { node_id, .. } if node_id == "ghost"
        )));
    }

    // ── Check 3: self-loop ────────────────────────────────────────────────────

    #[test]
    fn check3_self_loop() {
        let mut net = minimal_network();
        net.links[0].base.to_node = net.links[0].base.from_node;
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::LinkSelfLoop { link_id } if link_id == "P1"
        )));
    }

    // ── Check 4: reachability ─────────────────────────────────────────────────

    #[test]
    fn check4_no_reservoir() {
        let mut net = minimal_network();
        // Replace the reservoir with a second junction.
        net.nodes[0] = junction("J0", 1);
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::NoReservoir
        )));
    }

    #[test]
    fn check4_isolated_junction() {
        let mut net = minimal_network();
        // Add a junction with no connecting links.
        net.nodes.push(junction("J2", 3));
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::NodeNotReachable { node_id } if node_id == "J2"
        )));
    }

    // ── Check 5: tank level bounds ────────────────────────────────────────────

    #[test]
    fn check5_tank_init_level_below_min() {
        let mut net = minimal_network();
        net.nodes.push(Node {
            base: NodeBase {
                id: "T1".to_string(),
                index: 3,
                elevation: 0.0,
                initial_quality: 0.0,
            },
            kind: NodeKind::Tank(Tank {
                min_level: 5.0,
                max_level: 10.0,
                initial_level: 2.0, // below min
                diameter: 10.0,
                min_volume: 0.0,
                volume_curve: None,
                mix_model: MixModel::Cstr,
                mix_fraction: 0.5,
                bulk_coeff: 0.0,
                overflow: false,
                head_pattern: None,
            }),
            source: None,
        });
        // Connect it so it doesn't also fail check 4.
        net.links.push(pipe("P2", 2, 1, 3));
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::TankLevelOutOfRange { node_id, .. } if node_id == "T1"
        )));
    }

    #[test]
    fn check5_tank_init_level_above_max() {
        let mut net = minimal_network();
        net.nodes.push(Node {
            base: NodeBase {
                id: "T2".to_string(),
                index: 3,
                elevation: 0.0,
                initial_quality: 0.0,
            },
            kind: NodeKind::Tank(Tank {
                min_level: 0.0,
                max_level: 10.0,
                initial_level: 15.0, // above max
                diameter: 10.0,
                min_volume: 0.0,
                volume_curve: None,
                mix_model: MixModel::Cstr,
                mix_fraction: 0.5,
                bulk_coeff: 0.0,
                overflow: false,
                head_pattern: None,
            }),
            source: None,
        });
        net.links.push(pipe("P2", 2, 1, 3));
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::TankLevelOutOfRange { node_id, .. } if node_id == "T2"
        )));
    }

    // ── Check 6: PUMP_HEAD curve strictly decreasing y ────────────────────────

    #[test]
    fn check6_pump_curve_not_decreasing() {
        let mut net = minimal_network();
        net.curves.push(Curve {
            id: "C1".to_string(),
            kind: CurveKind::PumpHead,
            points: vec![
                CurvePoint { x: 0.0, y: 10.0 },
                CurvePoint { x: 1.0, y: 15.0 }, // y goes up — invalid
                CurvePoint { x: 2.0, y: 5.0 },
            ],
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::PumpCurveNotDecreasing { curve_id } if curve_id == "C1"
        )));
    }

    #[test]
    fn check6_efficiency_curve_y_out_of_range() {
        let mut net = minimal_network();
        net.curves.push(Curve {
            id: "EFF1".to_string(),
            kind: CurveKind::PumpEfficiency,
            points: vec![
                CurvePoint { x: 0.0, y: -1.0 }, // y < 0 is not in [0, 100]
                CurvePoint { x: 1.0, y: 80.0 },
            ],
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::EfficiencyCurveYOutOfRange { curve_id } if curve_id == "EFF1"
        )));
    }

    #[test]
    fn check6_efficiency_curve_y_above_100() {
        let mut net = minimal_network();
        net.curves.push(Curve {
            id: "EFF2".to_string(),
            kind: CurveKind::PumpEfficiency,
            points: vec![
                CurvePoint { x: 0.0, y: 50.0 },
                CurvePoint { x: 1.0, y: 110.0 }, // > 100 — invalid
            ],
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::EfficiencyCurveYOutOfRange { curve_id } if curve_id == "EFF2"
        )));
    }

    #[test]
    fn check6_tank_volume_curve_y_not_increasing() {
        let mut net = minimal_network();
        net.curves.push(Curve {
            id: "TV1".to_string(),
            kind: CurveKind::TankVolume,
            points: vec![
                CurvePoint { x: 0.0, y: 100.0 },
                CurvePoint { x: 1.0, y: 50.0 }, // y decreases — invalid
            ],
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::TankVolumeCurveYNotIncreasing { curve_id } if curve_id == "TV1"
        )));
    }

    #[test]
    fn check6_gpv_headloss_curve_y_decreasing() {
        let mut net = minimal_network();
        net.curves.push(Curve {
            id: "GPV1".to_string(),
            kind: CurveKind::GpvHeadloss,
            points: vec![
                CurvePoint { x: 0.0, y: 10.0 },
                CurvePoint { x: 1.0, y: 5.0 }, // y decreases — invalid
            ],
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::GpvHeadlossCurveYDecreasing { curve_id } if curve_id == "GPV1"
        )));
    }

    #[test]
    fn check6_gpv_headloss_curve_y_flat_is_ok() {
        // GPV requires non-decreasing (≥), so a flat curve is valid.
        let mut net = minimal_network();
        net.curves.push(Curve {
            id: "GPV2".to_string(),
            kind: CurveKind::GpvHeadloss,
            points: vec![
                CurvePoint { x: 0.0, y: 5.0 },
                CurvePoint { x: 1.0, y: 5.0 }, // flat — valid
            ],
        });
        assert!(net.validate().is_ok());
    }

    // ── Check 7: curve x strictly increasing ─────────────────────────────────

    #[test]
    fn check7_curve_x_not_increasing() {
        let mut net = minimal_network();
        net.curves.push(Curve {
            id: "C2".to_string(),
            kind: CurveKind::TankVolume,
            points: vec![
                CurvePoint { x: 0.0, y: 0.0 },
                CurvePoint { x: 0.0, y: 100.0 }, // duplicate x — invalid
            ],
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::CurveXNotIncreasing { curve_id } if curve_id == "C2"
        )));
    }

    // ── Check 8: patterns non-empty ───────────────────────────────────────────

    #[test]
    fn check8_empty_pattern() {
        let mut net = minimal_network();
        net.patterns.push(Pattern {
            id: "PAT1".to_string(),
            factors: vec![],
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::PatternEmpty { pattern_id } if pattern_id == "PAT1"
        )));
    }

    // ── Check 9: rule action link indices ─────────────────────────────────────

    #[test]
    fn check9_rule_action_unknown_link() {
        let mut net = minimal_network();
        net.rules.push(Rule {
            priority: 1.0,
            premises: vec![],
            then_actions: vec![RuleAction {
                link: 99,
                value: ActionValue::Status(LinkStatus::Open),
            }],
            else_actions: vec![],
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::RuleActionUnknownLink { link_index: 99, .. }
        )));
    }

    #[test]
    fn check10_curve_too_few_points() {
        let mut net = minimal_network();
        net.curves.push(Curve {
            id: "C1".to_string(),
            kind: CurveKind::GpvHeadloss,
            points: vec![CurvePoint { x: 1.0, y: 2.0 }],
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::CurveTooFewPoints { count: 1, .. }
        )));
    }

    #[test]
    fn check11_control_unknown_link() {
        let mut net = minimal_network();
        net.controls.push(SimpleControl {
            link: 99,
            trigger_type: TriggerType::Timer,
            trigger_time: Some(0.0),
            trigger_node: None,
            trigger_grade: None,
            action_status: Some(LinkStatus::Open),
            action_setting: None,
            enabled: true,
        });
        let errs = net.validate().unwrap_err();
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::ControlUnknownLink { link_index: 99, .. }
        )));
    }

    // ── All errors collected (not fail-fast) ──────────────────────────────────

    #[test]
    fn collects_multiple_errors() {
        let mut net = minimal_network();
        // Trigger check 7 (bad curve x) and check 8 (empty pattern) simultaneously.
        net.curves.push(Curve {
            id: "CX".to_string(),
            kind: CurveKind::TankVolume,
            points: vec![
                CurvePoint { x: 5.0, y: 0.0 },
                CurvePoint { x: 1.0, y: 100.0 }, // x decreasing
            ],
        });
        net.patterns.push(Pattern {
            id: "PX".to_string(),
            factors: vec![],
        });
        let errs = net.validate().unwrap_err();
        assert!(errs.len() >= 2);
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::CurveXNotIncreasing { .. }
        )));
        assert!(contains(&errs, |e| matches!(
            e,
            ValidationError::PatternEmpty { .. }
        )));
    }

    // ── SimulationOptions::default ────────────────────────────────────────────

    #[test]
    fn default_options_spec_values() {
        let o = SimulationOptions::default();
        assert_eq!(o.max_iter, 200);
        assert_eq!(o.extra_iter, -1);
        assert!((o.head_tol - 1.524e-4).abs() < 1e-10);
        assert!((o.flow_change_tol - 2.832e-6).abs() < 1e-12);
        assert!((o.flow_tol - 0.001).abs() < 1e-10);
        assert_eq!(o.check_freq, 2);
        assert_eq!(o.max_check, 10);
        assert!((o.quality_tolerance - 0.01).abs() < 1e-10);
        assert!((o.energy_efficiency - 0.75).abs() < 1e-10);
        // qual_step must satisfy [1, hyd_step]
        assert!(o.qual_step >= 1.0 && o.qual_step <= o.hyd_step);
    }

    // ── Pattern::eval ─────────────────────────────────────────────────────────

    #[test]
    fn pattern_eval_wraps_modulo() {
        // Pattern [1.0, 2.0, 3.0] with step 3600 s; at t = 7200 s the period
        // index p = floor(7200/3600) = 2, so multiplier = factors[2] = 3.0.
        let pat = Pattern {
            id: "P".to_string(),
            factors: vec![1.0, 2.0, 3.0],
        };
        assert!((pat.eval(0.0, 3600.0, 0.0) - 1.0).abs() < 1e-12);
        assert!((pat.eval(3600.0, 3600.0, 0.0) - 2.0).abs() < 1e-12);
        assert!((pat.eval(7200.0, 3600.0, 0.0) - 3.0).abs() < 1e-12);
        // Wraps: period 3 → index 0
        assert!((pat.eval(10800.0, 3600.0, 0.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn pattern_eval_with_pattern_start() {
        // With pattern_start = 1800, at t = 0 effective offset = 1800 s,
        // p = floor(1800/3600) = 0 → factors[0].
        // At t = 1800, offset = 3600, p = 1 → factors[1].
        let pat = Pattern {
            id: "P".to_string(),
            factors: vec![10.0, 20.0],
        };
        assert!((pat.eval(0.0, 3600.0, 1800.0) - 10.0).abs() < 1e-12);
        assert!((pat.eval(1800.0, 3600.0, 1800.0) - 20.0).abs() < 1e-12);
    }

    // ── Curve::eval ───────────────────────────────────────────────────────────

    fn two_point_curve() -> Curve {
        Curve {
            id: "C".to_string(),
            kind: CurveKind::TankVolume,
            points: vec![
                CurvePoint { x: 0.0, y: 0.0 },
                CurvePoint { x: 10.0, y: 100.0 },
            ],
        }
    }

    #[test]
    fn curve_eval_midpoint() {
        let c = two_point_curve();
        // At x = 5 the interpolated y = 50.
        assert!((c.eval(5.0) - 50.0).abs() < 1e-10);
    }

    #[test]
    fn curve_eval_extrapolate_low() {
        let c = two_point_curve();
        // Extrapolating below x = 0: slope is 10 per unit, so at x = -2 → y = -20.
        assert!((c.eval(-2.0) - (-20.0)).abs() < 1e-10);
    }

    #[test]
    fn curve_eval_extrapolate_high() {
        let c = two_point_curve();
        // Extrapolating above x = 10: slope is 10 per unit, so at x = 15 → y = 150.
        assert!((c.eval(15.0) - 150.0).abs() < 1e-10);
    }

    #[test]
    fn curve_eval_three_point_interior() {
        let c = Curve {
            id: "C3".to_string(),
            kind: CurveKind::PumpHead,
            points: vec![
                CurvePoint { x: 0.0, y: 300.0 },
                CurvePoint { x: 5.0, y: 200.0 },
                CurvePoint { x: 10.0, y: 50.0 },
            ],
        };
        // At x = 7.5, bracket [5, 10], y = 200 + (50-200)*(7.5-5)/5 = 200 - 75 = 125.
        assert!((c.eval(7.5) - 125.0).abs() < 1e-10);
    }

    // ── Junction::total_demand ────────────────────────────────────────────────

    #[test]
    fn total_demand_no_pattern() {
        // base_demand 1.0, multiplier 2.0, no pattern → demand = 2.0.
        let opts = SimulationOptions {
            demand_multiplier: 2.0,
            ..SimulationOptions::default()
        };
        let j = Junction {
            demands: vec![DemandCategory {
                base_demand: 1.0,
                pattern: None,
                name: None,
            }],
            emitter_coeff: 0.0,
            emitter_exp: 0.5,
        };
        assert!((j.total_demand(0.0, &opts, &[], &HashMap::new()) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn total_demand_with_pattern() {
        let opts = SimulationOptions {
            demand_multiplier: 1.0,
            pattern_step: 3600.0,
            pattern_start: 0.0,
            ..SimulationOptions::default()
        };
        let pat = Pattern {
            id: "P1".to_string(),
            factors: vec![1.5, 2.5],
        };
        let j = Junction {
            demands: vec![DemandCategory {
                base_demand: 4.0,
                pattern: Some("P1".to_string()),
                name: None,
            }],
            emitter_coeff: 0.0,
            emitter_exp: 0.5,
        };
        let idx: HashMap<String, usize> = [("P1".to_string(), 0)].into_iter().collect();
        // t = 0: factor[0] = 1.5 → demand = 4 * 1.5 = 6.0
        assert!((j.total_demand(0.0, &opts, std::slice::from_ref(&pat), &idx) - 6.0).abs() < 1e-12);
        // t = 3600: factor[1] = 2.5 → demand = 4 * 2.5 = 10.0
        assert!((j.total_demand(3600.0, &opts, &[pat], &idx) - 10.0).abs() < 1e-12);
    }

    #[test]
    fn total_demand_falls_back_to_default_pattern() {
        let opts = SimulationOptions {
            demand_multiplier: 1.0,
            pattern_step: 3600.0,
            pattern_start: 0.0,
            default_pattern: Some("DEF".to_string()),
            ..SimulationOptions::default()
        };
        let pat = Pattern {
            id: "DEF".to_string(),
            factors: vec![3.0],
        };
        let j = Junction {
            demands: vec![DemandCategory {
                base_demand: 2.0,
                pattern: None,
                name: None,
            }],
            emitter_coeff: 0.0,
            emitter_exp: 0.5,
        };
        let idx: HashMap<String, usize> = [("DEF".to_string(), 0)].into_iter().collect();
        // No per-category pattern; falls back to DEF with multiplier 3.0.
        assert!((j.total_demand(0.0, &opts, &[pat], &idx) - 6.0).abs() < 1e-12);
    }

    #[test]
    fn total_demand_sums_categories() {
        let opts = SimulationOptions::default();
        let j = Junction {
            demands: vec![
                DemandCategory {
                    base_demand: 1.0,
                    pattern: None,
                    name: None,
                },
                DemandCategory {
                    base_demand: 3.0,
                    pattern: None,
                    name: None,
                },
            ],
            emitter_coeff: 0.0,
            emitter_exp: 0.5,
        };
        // Both use multiplier 1.0 (no pattern, demand_multiplier = 1.0).
        assert!((j.total_demand(0.0, &opts, &[], &HashMap::new()) - 4.0).abs() < 1e-12);
    }

    // ── Reservoir::head ───────────────────────────────────────────────────────

    #[test]
    fn reservoir_head_no_pattern() {
        let opts = SimulationOptions::default();
        let r = Reservoir { head_pattern: None };
        assert!((r.head(100.0, 0.0, &opts, &[], &HashMap::new()) - 100.0).abs() < 1e-12);
    }

    #[test]
    fn reservoir_head_with_pattern() {
        let opts = SimulationOptions {
            pattern_step: 3600.0,
            pattern_start: 0.0,
            ..SimulationOptions::default()
        };
        let pat = Pattern {
            id: "HP".to_string(),
            factors: vec![1.0, 1.1],
        };
        let r = Reservoir {
            head_pattern: Some("HP".to_string()),
        };
        let idx: HashMap<String, usize> = [("HP".to_string(), 0)].into_iter().collect();
        // t = 0: factor 1.0 → head = 100 * 1.0 = 100.
        assert!(
            (r.head(100.0, 0.0, &opts, std::slice::from_ref(&pat), &idx) - 100.0).abs() < 1e-12
        );
        // t = 3600: factor 1.1 → head = 100 * 1.1 = 110.
        assert!((r.head(100.0, 3600.0, &opts, &[pat], &idx) - 110.0).abs() < 1e-12);
    }

    // ── Tank geometry methods ─────────────────────────────────────────────────

    fn cylindrical_tank(diameter: f64, min_level: f64) -> Tank {
        Tank {
            min_level,
            max_level: min_level + 10.0,
            initial_level: min_level + 5.0,
            diameter,
            min_volume: 0.0,
            volume_curve: None,
            mix_model: MixModel::Cstr,
            mix_fraction: 0.5,
            bulk_coeff: 0.0,
            overflow: false,
            head_pattern: None,
        }
    }

    #[test]
    fn tank_bottom_elevation() {
        let t = cylindrical_tank(2.0, 2.0);
        // bottom_elevation = elevation(10) - min_level(2) = 8.
        assert!((t.bottom_elevation(10.0) - 8.0).abs() < 1e-12);
    }

    #[test]
    fn tank_head_from_level() {
        let t = cylindrical_tank(2.0, 2.0);
        // bottom = 10 - 2 = 8; head at level 5 = 8 + 5 = 13.
        assert!((t.head_from_level(10.0, 5.0) - 13.0).abs() < 1e-12);
    }

    #[test]
    fn cylindrical_tank_area() {
        let d = 2.0_f64;
        let t = cylindrical_tank(d, 0.0);
        let expected = std::f64::consts::PI * d * d / 4.0;
        assert!((t.area(3.0, &[]) - expected).abs() < 1e-10);
    }

    #[test]
    fn cylindrical_tank_volume_from_level() {
        let d = 2.0_f64;
        let t = cylindrical_tank(d, 0.0);
        let a = std::f64::consts::PI * d * d / 4.0;
        // volume = area * level
        assert!((t.volume_from_level(4.0, &[]) - a * 4.0).abs() < 1e-10);
    }

    #[test]
    fn cylindrical_tank_level_from_volume_roundtrip() {
        let d = 3.0_f64;
        let t = cylindrical_tank(d, 0.0);
        let level = 6.5;
        let vol = t.volume_from_level(level, &[]);
        let recovered = t.level_from_volume(vol, &[]);
        assert!((recovered - level).abs() < 1e-10);
    }

    #[test]
    fn volume_curve_tank_area_and_roundtrip() {
        // Curve: level → volume, linear: V = 5 * h (A = 5 everywhere).
        let curve = Curve {
            id: "V1".to_string(),
            kind: CurveKind::TankVolume,
            points: vec![
                CurvePoint { x: 0.0, y: 0.0 },
                CurvePoint { x: 10.0, y: 50.0 },
            ],
        };
        let t = Tank {
            min_level: 0.0,
            max_level: 10.0,
            initial_level: 5.0,
            diameter: 1.0, // ignored when vol_curve is set
            min_volume: 0.0,
            volume_curve: Some("V1".to_string()),
            mix_model: MixModel::Cstr,
            mix_fraction: 0.5,
            bulk_coeff: 0.0,
            overflow: false,
            head_pattern: None,
        };
        let curves = vec![curve];
        // Area at any interior level = dV/dh = 50/10 = 5.
        assert!((t.area(3.0, &curves) - 5.0).abs() < 1e-10);
        // Volume at level 4 = 20.
        assert!((t.volume_from_level(4.0, &curves) - 20.0).abs() < 1e-10);
        // Invert: level_from_volume(20) = 4.
        assert!((t.level_from_volume(20.0, &curves) - 4.0).abs() < 1e-10);
    }

    // ── QualitySource::effective_value ────────────────────────────────────────

    #[test]
    fn quality_source_no_pattern() {
        let opts = SimulationOptions::default();
        let src = QualitySource {
            node: 1,
            kind: SourceType::Concentration,
            base_value: 5.0,
            pattern: None,
        };
        assert!((src.effective_value(0.0, &opts, &[], &HashMap::new()) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn quality_source_with_pattern() {
        let opts = SimulationOptions {
            pattern_step: 3600.0,
            pattern_start: 0.0,
            ..SimulationOptions::default()
        };
        let pat = Pattern {
            id: "QP".to_string(),
            factors: vec![0.0, 2.0],
        };
        let src = QualitySource {
            node: 1,
            kind: SourceType::Mass,
            base_value: 10.0,
            pattern: Some("QP".to_string()),
        };
        let idx: HashMap<String, usize> = [("QP".to_string(), 0)].into_iter().collect();
        assert!(
            (src.effective_value(0.0, &opts, std::slice::from_ref(&pat), &idx) - 0.0).abs() < 1e-12
        );
        assert!((src.effective_value(3600.0, &opts, &[pat], &idx) - 20.0).abs() < 1e-12);
    }

    // ── Network::compute_favad ────────────────────────────────────────────────

    fn pipe_with_leak(id: &str, index: usize, from: usize, to: usize, k1: f64, k2: f64) -> Link {
        Link {
            base: LinkBase {
                id: id.to_string(),
                index,
                from_node: from,
                to_node: to,
                initial_status: LinkStatus::Open,
                initial_setting: Some(1.0),
            },
            kind: LinkKind::Pipe(Pipe {
                length: 1000.0,
                diameter: 1.0,
                roughness: 100.0,
                minor_loss: 0.0,
                check_valve: false,
                bulk_coeff: None,
                wall_coeff: None,
                leak_coeff_1: k1,
                leak_coeff_2: k2,
            }),
        }
    }

    #[test]
    fn favad_no_leakage_all_zero() {
        // Default minimal network has no FAVAD coefficients.
        let net = minimal_network();
        let fc = net.compute_favad();
        assert!(fc.c_fa.iter().all(|&v| v == 0.0));
        assert!(fc.c_va.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn favad_both_junctions_split_half() {
        // Network: R(1) — J1(2) — J2(3), pipe J1-J2 has K1 = 2.0.
        // Both ends are junctions so each gets 0.5 * 2.0 = 1.0.
        // c_fa = 1 / K_fa^2 = 1 / 1^2 = 1.0 for both.
        let mut net = minimal_network();
        net.nodes.push(junction("J2", 3));
        // Replace existing pipe (R→J1) with no leakage, add J1→J2 with leakage.
        net.links[0] = pipe_with_leak("P_RJ1", 1, 1, 2, 0.0, 0.0);
        net.links.push(pipe_with_leak("P_J1J2", 2, 2, 3, 2.0, 0.0));
        let fc = net.compute_favad();
        // node 0 = R (reservoir, not junction) → c_fa[0] = 0
        assert!((fc.c_fa[0]).abs() < 1e-12);
        // node 1 = J1, K_fa = 1.0 → c_fa = 1/1^2 = 1.0
        assert!((fc.c_fa[1] - 1.0).abs() < 1e-10);
        // node 2 = J2, K_fa = 1.0 → c_fa = 1.0
        assert!((fc.c_fa[2] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn favad_reservoir_end_full_coefficient() {
        // Network: R(1) — J1(2), pipe has K1 = 2.0.
        // One end is reservoir (fixed-grade) so J1 gets full K1 = 2.0.
        // c_fa = 1 / 2^2 = 0.25.
        let mut net = minimal_network();
        net.links[0] = pipe_with_leak("P1", 1, 1, 2, 2.0, 0.0);
        let fc = net.compute_favad();
        // node 0 = R → 0
        assert!((fc.c_fa[0]).abs() < 1e-12);
        // node 1 = J1, K_fa = 2.0 → c_fa = 1/4 = 0.25
        assert!((fc.c_fa[1] - 0.25).abs() < 1e-10);
    }
}
