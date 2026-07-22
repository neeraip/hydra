// rpt_writer — simulation report serialization (crates/interface/cli/spec.md §4.3).
//
// Produces JSON or plain-text report strings from a completed simulation.
// No file I/O — callers write the returned strings to their destination.

use std::fmt::Write as FmtWrite;

use super::{SimWarning, WarningKind, WritableSimulation};
use crate::{DemandModel, HeadLossFormula, LinkKind, NodeKind, QualityMode};

// ── Public API ────────────────────────────────────────────────────────────────

/// Classify a simulation warning into a machine-readable (code, message, object_id) triple.
pub fn describe_warning(
    w: &SimWarning,
    session: &impl WritableSimulation,
) -> (String, String, Option<String>) {
    let network = session.net();
    match &w.kind {
        WarningKind::UnbalancedHydraulics => (
            "warning/unbalanced".to_string(),
            "hydraulic simulation did not converge".to_string(),
            None,
        ),
        WarningKind::NegativePressure { node_index } => {
            let node_id = &network.nodes[*node_index].base.id;
            (
                "warning/negative_pressure".to_string(),
                format!("negative pressure at node '{node_id}'"),
                Some(node_id.clone()),
            )
        }
        WarningKind::PumpXHead { link_index } => {
            let link_id = &network.links[*link_index].base.id;
            (
                "warning/pump_xhead".to_string(),
                format!("pump '{link_id}' exceeds maximum head"),
                Some(link_id.clone()),
            )
        }
    }
}

/// Build a JSON report string from a completed simulation (crates/cli/spec.md §4.3).
pub fn build_json_report(session: &impl WritableSimulation) -> Result<String, serde_json::Error> {
    let network = session.net();
    let options = &network.options;

    // ── Input summary ────────────────────────────────────────────────────────
    let n_junctions = network
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Junction(_)))
        .count();
    let n_reservoirs = network
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Reservoir(_)))
        .count();
    let n_tanks = network
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Tank(_)))
        .count();
    let n_pipes = network
        .links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Pipe(_)))
        .count();
    let n_pumps = network
        .links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Pump(_)))
        .count();
    let n_valves = network
        .links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Valve(_)))
        .count();

    let input_summary = serde_json::json!({
        "junctions": n_junctions,
        "reservoirs": n_reservoirs,
        "tanks": n_tanks,
        "pipes": n_pipes,
        "pumps": n_pumps,
        "valves": n_valves,
        "headloss_formula": match options.head_loss_formula {
            HeadLossFormula::HazenWilliams => "Hazen-Williams",
            HeadLossFormula::DarcyWeisbach => "Darcy-Weisbach",
            HeadLossFormula::ChezyManning => "Chezy-Manning",
        },
        "demand_model": match options.demand_model {
            DemandModel::DemandDriven => "DDA",
            DemandModel::PressureDriven => "PDA",
        },
        "hydraulic_timestep_s": options.hyd_step,
        "quality_timestep_s": options.qual_step,
        "duration_s": options.duration,
        "report_timestep_s": options.report_step,
    });

    // ── Warnings ─────────────────────────────────────────────────────────────
    let warnings: Vec<serde_json::Value> = session
        .warnings()
        .iter()
        .map(|w| {
            let (code, message, object_id) = describe_warning(w, session);
            serde_json::json!({
                "time": w.t,
                "code": code,
                "message": message,
                "object_id": object_id,
            })
        })
        .collect();

    // ── Pump energy ──────────────────────────────────────────────────────────
    let pump_energy: Vec<serde_json::Value> = network
        .links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Pump(_)))
        .filter_map(|l| {
            session.pump_energy_by_id(&l.base.id).map(|pe| {
                serde_json::json!({
                    "pump_id": &l.base.id,
                    "kwh": pe.kwh,
                    "total_cost": pe.total_cost,
                    "avg_efficiency": pe.avg_efficiency(),
                    "max_kw": pe.max_kw,
                    "time_online_s": pe.time_online,
                })
            })
        })
        .collect();
    let peak_demand_kw = session.peak_demand_kw();

    // ── Flow balance ─────────────────────────────────────────────────────────
    let flow_balance = session.flow_balance_summary().map(|fbs| {
        serde_json::json!({
            "total_inflow": fbs.total_inflow,
            "total_outflow": fbs.total_outflow,
            "tank_change": fbs.tank_change,
            "unaccounted": fbs.unaccounted,
            "ratio": fbs.ratio,
        })
    });

    // ── Mass balance ─────────────────────────────────────────────────────────
    let mass_balance = session.mass_balance().map(|mb| {
        serde_json::json!({
            "initial": mb.init,
            "added": mb.added,
            "demand": mb.demand,
            "reacted": mb.reacted,
            "reacted_bulk": mb.reacted_bulk,
            "reacted_wall": mb.reacted_wall,
            "reacted_tank": mb.reacted_tank,
            "source": mb.source,
            "final_mass": mb.final_mass,
            "ratio": mb.ratio(),
        })
    });

    // ── Analysis timestamps ──────────────────────────────────────────────────
    let (begun, ended) = session.analysis_times();
    let format_time = |t: std::time::SystemTime| -> String {
        let secs = t
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("{secs}")
    };

    let report = serde_json::json!({
        "input": input_summary,
        "warnings": warnings,
        "energy": {
            "pumps": pump_energy,
            "peak_demand_kw": peak_demand_kw,
        },
        "flow_balance": flow_balance,
        "mass_balance": mass_balance,
        "analysis": {
            "begun_epoch": begun.map(&format_time),
            "ended_epoch": ended.map(&format_time),
        },
    });

    serde_json::to_string_pretty(&report)
}

// ── Plain-text report ─────────────────────────────────────────────────────────

/// Build a plain-text report string from a completed simulation (crates/interface/cli/spec.md §4.3).
pub fn build_text_report(session: &impl WritableSimulation) -> Result<String, std::fmt::Error> {
    let mut report = String::new();
    let network = session.net();
    let options = &network.options;

    // ── Date stamp (right-aligned to banner width) ───────────────────────────
    let now: chrono::DateTime<chrono::Local> = chrono::Local::now();
    let date_str = now.format("%a %b %e %T %Y").to_string();
    writeln!(report, "{date_str:>68}")?;
    writeln!(report)?;

    // ── Banner ───────────────────────────────────────────────────────────────
    let version_label = format!("Version {}", env!("CARGO_PKG_VERSION"));
    let pad_total = 64 - version_label.len();
    let pad_left = pad_total / 2;
    let pad_right = pad_total - pad_left;
    let version_line = format!(
        "  *{}{}{}*",
        " ".repeat(pad_left),
        version_label,
        " ".repeat(pad_right)
    );
    writeln!(
        report,
        "  ******************************************************************"
    )?;
    writeln!(
        report,
        "  *                           H Y D R A                            *"
    )?;
    writeln!(
        report,
        "  *                   Hydraulic and Water Quality                  *"
    )?;
    writeln!(
        report,
        "  *                   Analysis for Pipe Networks                   *"
    )?;
    writeln!(report, "{version_line}")?;
    writeln!(
        report,
        "  ******************************************************************"
    )?;
    writeln!(report)?;

    // ── Title ────────────────────────────────────────────────────────────────
    for line in &network.title {
        if !line.is_empty() {
            writeln!(report, "  {line}")?;
        }
    }
    if !network.title.is_empty() {
        writeln!(report)?;
    }

    // ── Input summary ────────────────────────────────────────────────────────
    let n_junctions = network
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Junction(_)))
        .count();
    let n_reservoirs = network
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Reservoir(_)))
        .count();
    let n_tanks = network
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Tank(_)))
        .count();
    let n_pipes = network
        .links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Pipe(_)))
        .count();
    let n_pumps = network
        .links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Pump(_)))
        .count();
    let n_valves = network
        .links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Valve(_)))
        .count();

    let head_loss_formula_label = match options.head_loss_formula {
        HeadLossFormula::HazenWilliams => "Hazen-Williams",
        HeadLossFormula::DarcyWeisbach => "Darcy-Weisbach",
        HeadLossFormula::ChezyManning => "Chezy-Manning",
    };
    let demand_model_label = match options.demand_model {
        DemandModel::DemandDriven => "DDA",
        DemandModel::PressureDriven => "PDA",
    };

    writeln!(
        report,
        "      Number of Junctions................ {n_junctions}"
    )?;
    writeln!(
        report,
        "      Number of Reservoirs............... {n_reservoirs}"
    )?;
    writeln!(
        report,
        "      Number of Tanks ................... {n_tanks}"
    )?;
    writeln!(
        report,
        "      Number of Pipes ................... {n_pipes}"
    )?;
    writeln!(
        report,
        "      Number of Pumps ................... {n_pumps}"
    )?;
    writeln!(
        report,
        "      Number of Valves .................. {n_valves}"
    )?;
    writeln!(
        report,
        "      Headloss Formula .................. {head_loss_formula_label}"
    )?;
    writeln!(
        report,
        "      Nodal Demand Model ................ {demand_model_label}"
    )?;
    writeln!(
        report,
        "      Hydraulic Timestep ................ {}",
        fmt_duration(options.hyd_step)
    )?;
    writeln!(
        report,
        "      Hydraulic Accuracy ................ {:.6}",
        options.flow_tol
    )?;
    writeln!(
        report,
        "      Maximum Trials .................... {}",
        options.max_iter
    )?;

    match options.quality_mode {
        QualityMode::None => {
            writeln!(report, "      Quality Analysis .................. None")?;
        }
        QualityMode::Chemical => {
            let name = if options.chem_name.is_empty() {
                "Chemical"
            } else {
                &options.chem_name
            };
            writeln!(report, "      Quality Analysis .................. {name}")?;
            writeln!(
                report,
                "      Water Quality Time Step ........... {}",
                fmt_duration(options.qual_step)
            )?;
            writeln!(
                report,
                "      Water Quality Tolerance ........... {} {}",
                options.quality_tolerance, &options.chem_units
            )?;
        }
        QualityMode::Age => {
            writeln!(report, "      Quality Analysis .................. Age")?;
            writeln!(
                report,
                "      Water Quality Time Step ........... {}",
                fmt_duration(options.qual_step)
            )?;
        }
        QualityMode::Trace => {
            let node = options.trace_node.as_deref().unwrap_or("?");
            writeln!(
                report,
                "      Quality Analysis .................. Trace {node}"
            )?;
            writeln!(
                report,
                "      Water Quality Time Step ........... {}",
                fmt_duration(options.qual_step)
            )?;
        }
    }

    writeln!(
        report,
        "      Specific Gravity .................. {:.2}",
        options.specific_gravity
    )?;
    writeln!(
        report,
        "      Demand Multiplier ................. {:.2}",
        options.demand_multiplier
    )?;
    writeln!(
        report,
        "      Total Duration .................... {}",
        fmt_duration(options.duration)
    )?;
    writeln!(
        report,
        "      Report Timestep ................... {}",
        fmt_duration(options.report_step)
    )?;
    writeln!(report)?;

    // ── Analysis begun ───────────────────────────────────────────────────────
    let (begun, ended) = session.analysis_times();
    if let Some(t) = begun {
        writeln!(report, "  Analysis begun {}", fmt_system_time(t))?;
    }

    // ── Warnings (between begun / ended, like EPANET) ────────────────────────
    let warnings = session.warnings();
    if !warnings.is_empty() {
        // Group warnings by simulation timestep; insert a blank line between groups.
        let mut prev_t: Option<f64> = None;
        for w in warnings {
            if prev_t.is_none_or(|pt| (w.t - pt).abs() > 0.5) {
                // New timestep group — blank line separator.
                writeln!(report)?;
            }
            let msg = match &w.kind {
                WarningKind::UnbalancedHydraulics => {
                    format!(
                        "  WARNING: Hydraulics not converged at {} hrs.",
                        fmt_clocktime(w.t)
                    )
                }
                WarningKind::NegativePressure { node_index: _ } => {
                    format!(
                        "  WARNING: Negative pressures at {} hrs.",
                        fmt_clocktime(w.t)
                    )
                }
                WarningKind::PumpXHead { link_index } => {
                    let link_id = &network.links[*link_index].base.id;
                    format!(
                        "  WARNING: Pump {} exceeds maximum head at {} hrs.",
                        link_id,
                        fmt_clocktime(w.t)
                    )
                }
            };
            writeln!(report, "{msg}")?;
            prev_t = Some(w.t);
        }
    }

    // ── Analysis ended ───────────────────────────────────────────────────────
    writeln!(report)?;
    if let Some(t) = ended {
        writeln!(report, "  Analysis ended {}", fmt_system_time(t))?;
    }

    Ok(report)
}

/// Format simulation seconds as `H:MM:SS` (matches EPANET's `clocktime()` format).
fn fmt_clocktime(seconds: f64) -> String {
    let total = seconds.round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{h}:{m:02}:{s:02}")
}

/// Format a duration in seconds to a human-readable string (e.g. "1.00 hrs", "15.00 min").
fn fmt_duration(seconds: f64) -> String {
    if seconds == 0.0 {
        "0.00 hrs".to_string()
    } else if seconds >= 3600.0 {
        format!("{:.2} hrs", seconds / 3600.0)
    } else if seconds >= 60.0 {
        format!("{:.2} min", seconds / 60.0)
    } else {
        format!("{:.2} sec", seconds)
    }
}

/// Format a `SystemTime` as "Mon Jan  2 15:04:05 2006" (ctime-like, UTC —
/// no timezone adjustment, matching the historical output of this writer).
fn fmt_system_time(t: std::time::SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Utc> = t.into();
    dt.format("%a %b %e %H:%M:%S %Y").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::parse;
    use std::path::Path;

    struct MockSession {
        network: crate::Network,
        snapshots: Vec<crate::io::HydSnapshot>,
        warnings: Vec<crate::io::SimWarning>,
        begun: Option<std::time::SystemTime>,
        ended: Option<std::time::SystemTime>,
        flow_balance: Option<crate::io::FlowBalance>,
        mass_balance: Option<crate::io::MassBalance>,
    }

    impl crate::io::WritableSimulation for MockSession {
        fn net(&self) -> &crate::Network {
            &self.network
        }
        fn snapshots(&self) -> &[crate::io::HydSnapshot] {
            &self.snapshots
        }
        fn pump_energy_at(&self, _link_index: usize) -> Option<&crate::io::PumpEnergy> {
            None
        }
        fn peak_demand_kw(&self) -> f64 {
            0.0
        }
        fn mass_balance(&self) -> Option<&crate::io::MassBalance> {
            self.mass_balance.as_ref()
        }
        fn warnings(&self) -> &[crate::io::SimWarning] {
            &self.warnings
        }
        fn pump_energy_by_id(&self, _pump_id: &str) -> Option<&crate::io::PumpEnergy> {
            None
        }
        fn analysis_times(&self) -> (Option<std::time::SystemTime>, Option<std::time::SystemTime>) {
            (self.begun, self.ended)
        }
        fn flow_balance(&self) -> Option<&crate::io::FlowBalance> {
            self.flow_balance.as_ref()
        }
        fn flow_balance_summary(&self) -> Option<crate::io::FlowBalanceSummary> {
            self.flow_balance
                .as_ref()
                .map(|fb| fb.summarize(fb.initial_tank_volume))
        }
    }

    fn load_fixture_network(name: &str) -> crate::Network {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures")
            .join(name);
        let bytes = std::fs::read(path).expect("read fixture");
        parse(&bytes).expect("parse fixture")
    }

    fn mock_session(name: &str) -> MockSession {
        let network = load_fixture_network(name);
        let node_states = network
            .nodes
            .iter()
            .map(|node| crate::NodeState {
                head: node.base.elevation,
                ..crate::NodeState::default()
            })
            .collect();
        let link_states = network
            .links
            .iter()
            .map(|_| crate::LinkState::default())
            .collect();

        MockSession {
            network,
            snapshots: vec![crate::io::HydSnapshot {
                t: 0.0,
                node_states,
                link_states,
            }],
            warnings: Vec::new(),
            begun: Some(std::time::UNIX_EPOCH),
            ended: Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(3600)),
            flow_balance: Some(crate::io::FlowBalance {
                total_inflow: 10.0,
                total_outflow: 10.0,
                demand_deficit: 0.0,
                initial_tank_volume: 0.0,
            }),
            mass_balance: Some(crate::io::MassBalance::default()),
        }
    }

    #[test]
    fn describe_warning_uses_network_object_ids() {
        let session = mock_session("single_pipe_hw.inp");
        let warning = SimWarning {
            t: 0.0,
            kind: WarningKind::NegativePressure { node_index: 0 },
        };
        let expected_id = session.net().nodes[0].base.id.clone();
        let (code, message, object_id) = describe_warning(&warning, &session);
        assert_eq!(code, "warning/negative_pressure");
        assert!(message.contains("negative pressure at node"));
        assert_eq!(object_id.as_deref(), Some(expected_id.as_str()));
    }

    #[test]
    fn json_report_contains_expected_top_level_keys() {
        let session = mock_session("single_pipe_hw.inp");
        let report = build_json_report(&session).expect("build json report");
        let value: serde_json::Value = serde_json::from_str(&report).expect("parse json report");
        assert!(value.get("input").is_some());
        assert!(value.get("warnings").is_some());
        assert!(value.get("energy").is_some());
        assert!(value.get("energy").unwrap().get("pumps").is_some());
        assert!(value.get("flow_balance").is_some());
        assert!(value.get("mass_balance").is_some());
        assert!(value.get("analysis").is_some());
    }

    #[test]
    fn text_report_contains_banner_and_duration() {
        let session = mock_session("single_pipe_hw.inp");
        let report = build_text_report(&session).expect("build text report");
        assert!(report.contains("H Y D R A"));
        assert!(report.contains("Total Duration"));
        assert!(report.contains("Analysis ended"));
    }

    #[test]
    fn fmt_clocktime_formats_hours_minutes_seconds() {
        assert_eq!(fmt_clocktime(0.0), "0:00:00");
        assert_eq!(fmt_clocktime(3661.0), "1:01:01");
    }

    #[test]
    fn fmt_system_time_matches_ctime_layout_in_utc() {
        assert_eq!(
            fmt_system_time(std::time::UNIX_EPOCH),
            "Thu Jan  1 00:00:00 1970"
        );
        // 2021-03-14 01:59:26 UTC (day-of-month needs space padding).
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_615_687_166);
        assert_eq!(fmt_system_time(t), "Sun Mar 14 01:59:26 2021");
    }

    #[test]
    fn fmt_duration_switches_units_by_scale() {
        assert_eq!(fmt_duration(0.0), "0.00 hrs");
        assert_eq!(fmt_duration(120.0), "2.00 min");
        assert_eq!(fmt_duration(7200.0), "2.00 hrs");
        assert_eq!(fmt_duration(15.0), "15.00 sec");
    }
}
