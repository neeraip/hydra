//! Report blocks: the WDS implementation of the foundation layer's
//! reportable-output contract (analysis spec §7, hydra-common spec §3).
//!
//! Counts and result values come from the `.out` file
//! (result-authoritative); element identifiers and declared display units
//! come from the network. Production is read-only and deterministic.

use std::path::Path;

use hydra_common::{
    BlockDescriptor, BlockError, Column, Fragment, FragmentItem, KeyValue, Table, Value, ValueKind,
};

use crate::io::out_reader::{self, OutMetadata};
use crate::{FlowUnits, Network};

/// Sample budget for the range scan (analysis spec §7.2). Matches the
/// `scan_ranges` guidance keeping scans under ~50 ms on long simulations.
const MAX_RANGE_SAMPLES: usize = 2048;

const CATALOG: &[BlockDescriptor] = &[
    BlockDescriptor {
        id: "wds.run-summary",
        title: "Run Summary",
        summary: "Network size, reporting window, units, and quality mode of the run.",
    },
    BlockDescriptor {
        id: "wds.result-extremes",
        title: "Result Extremes",
        summary: "Global minimum and maximum pressure, head, demand, flow, and velocity \
                  (plus quality when present) over the reporting horizon.",
    },
    BlockDescriptor {
        id: "wds.pump-energy",
        title: "Pump Energy",
        summary: "Per-pump utilization, efficiency, power, and cost, plus the network \
                  demand charge.",
    },
    BlockDescriptor {
        id: "wds.quality-summary",
        title: "Water Quality Summary",
        summary: "Quality mode and global quality extremes.",
    },
];

/// The water-distribution engine's report-block catalog (analysis spec §7.1).
pub fn report_catalog() -> &'static [BlockDescriptor] {
    CATALOG
}

/// Produce the fragment for one catalog block from a persisted `.out` file
/// and the corresponding loaded network (analysis spec §7.2).
pub fn produce_report_block(
    id: &str,
    out_path: &Path,
    network: &Network,
) -> Result<Fragment, BlockError> {
    match id {
        "wds.run-summary" => run_summary(out_path, network),
        "wds.result-extremes" => result_extremes(out_path, network),
        "wds.pump-energy" => pump_energy(out_path, network),
        "wds.quality-summary" => quality_summary(out_path),
        _ => Err(BlockError::UnknownBlock { id: id.into() }),
    }
}

// ── Value / metadata helpers ──────────────────────────────────────────────────

fn read_meta(out_path: &Path) -> Result<OutMetadata, BlockError> {
    out_reader::read_metadata_checked(out_path).map_err(|e| BlockError::Failed {
        message: e.to_string(),
    })
}

fn int(value: usize) -> Value {
    Value::Integer {
        value: value as i64,
    }
}

fn num(value: f64) -> Value {
    Value::Number { value, unit: None }
}

fn num_unit(value: f64, unit: &str) -> Value {
    Value::Number {
        value,
        unit: Some(unit.into()),
    }
}

fn text(value: impl Into<String>) -> Value {
    Value::Text {
        value: value.into(),
    }
}

fn entry(label: &str, value: Value) -> KeyValue {
    KeyValue {
        label: label.into(),
        value,
    }
}

/// `H:MM:SS` clock text for a non-negative duration in seconds.
fn fmt_hms(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!(
        "{}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

/// Whether the declared flow unit belongs to the SI/metric group (the same
/// grouping as the model spec §3.1 unit table).
fn is_si(units: FlowUnits) -> bool {
    matches!(
        units,
        FlowUnits::Lps
            | FlowUnits::Lpm
            | FlowUnits::Mld
            | FlowUnits::Cmh
            | FlowUnits::Cmd
            | FlowUnits::Cms
    )
}

/// INP keyword spelling of the declared flow unit (model spec §3.1).
fn flow_unit_label(units: FlowUnits) -> &'static str {
    match units {
        FlowUnits::Cfs => "CFS",
        FlowUnits::Gpm => "GPM",
        FlowUnits::Mgd => "MGD",
        FlowUnits::Imgd => "IMGD",
        FlowUnits::Afd => "AFD",
        FlowUnits::Lps => "LPS",
        FlowUnits::Lpm => "LPM",
        FlowUnits::Mld => "MLD",
        FlowUnits::Cmh => "CMH",
        FlowUnits::Cmd => "CMD",
        FlowUnits::Cms => "CMS",
    }
}

fn quality_mode_label(flag: i32) -> &'static str {
    match flag {
        0 => "None",
        1 => "Chemical",
        2 => "Age",
        3 => "Trace",
        _ => "Unknown",
    }
}

/// Display unit for quality values by mode flag (analysis spec §7.2):
/// chemical concentration reports the file-default mg/L.
fn quality_unit(flag: i32) -> &'static str {
    match flag {
        2 => "hours",
        3 => "%",
        _ => "mg/L",
    }
}

/// Sampling-disclosure note when the period count exceeds the scan budget
/// (analysis spec §7.2); `None` when the scan was exhaustive.
fn sampling_note(n_periods: usize) -> Option<FragmentItem> {
    (n_periods > MAX_RANGE_SAMPLES).then(|| FragmentItem::Note {
        text: format!(
            "Extremes were computed from {MAX_RANGE_SAMPLES} sampled reporting periods \
             (including the first and last) out of {n_periods}."
        ),
    })
}

// ── Blocks ────────────────────────────────────────────────────────────────────

fn run_summary(out_path: &Path, network: &Network) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    let (pressure, _, _) = unit_labels(network);
    let final_report_time =
        meta.report_start + meta.report_step * (meta.n_periods.max(1) - 1) as f64;

    let entries = vec![
        entry("Junctions", int(meta.n_nodes.saturating_sub(meta.n_tanks))),
        entry("Tanks & reservoirs", int(meta.n_tanks)),
        entry("Links", int(meta.n_links)),
        entry("Pumps", int(meta.n_pumps)),
        entry(
            "Flow units",
            text(flow_unit_label(network.options.flow_units)),
        ),
        entry("Pressure units", text(pressure)),
        entry("Quality mode", text(quality_mode_label(meta.quality_flag))),
        entry("Report start", text(fmt_hms(meta.report_start))),
        entry("Report step", text(fmt_hms(meta.report_step))),
        entry("Final report time", text(fmt_hms(final_report_time))),
        entry("Reporting periods", int(meta.n_periods)),
    ];

    Ok(Fragment {
        title: "Run Summary".into(),
        items: vec![FragmentItem::KeyValues { entries }],
    })
}

/// (pressure, length, velocity) display labels for the network's declared
/// unit system (analysis spec §7.2).
fn unit_labels(network: &Network) -> (&'static str, &'static str, &'static str) {
    if is_si(network.options.flow_units) {
        ("m", "m", "m/s")
    } else {
        ("psi", "ft", "ft/s")
    }
}

fn result_extremes(out_path: &Path, network: &Network) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_periods == 0 {
        return Err(BlockError::Failed {
            message: "results file holds no reporting periods".into(),
        });
    }
    let ranges = out_reader::scan_ranges(out_path, &meta, MAX_RANGE_SAMPLES)
        .map_err(|message| BlockError::Failed { message })?;

    let flow = flow_unit_label(network.options.flow_units);
    let (pressure, length, velocity) = unit_labels(network);

    let mut rows = vec![
        extremes_row(
            "Pressure",
            ranges.pressure_min,
            ranges.pressure_max,
            pressure,
        ),
        extremes_row("Head", ranges.head_min, ranges.head_max, length),
        extremes_row("Demand", ranges.demand_min, ranges.demand_max, flow),
        extremes_row("Flow", ranges.flow_min, ranges.flow_max, flow),
        extremes_row(
            "Velocity",
            ranges.velocity_min,
            ranges.velocity_max,
            velocity,
        ),
    ];
    if let (Some(qmin), Some(qmax)) = (ranges.quality_min, ranges.quality_max) {
        rows.push(extremes_row(
            "Quality",
            qmin,
            qmax,
            quality_unit(meta.quality_flag),
        ));
    }

    let table = Table {
        columns: vec![
            Column {
                name: "Quantity".into(),
                unit: None,
                kind: ValueKind::Text,
            },
            Column {
                name: "Minimum".into(),
                unit: None,
                kind: ValueKind::Number,
            },
            Column {
                name: "Maximum".into(),
                unit: None,
                kind: ValueKind::Number,
            },
            Column {
                name: "Unit".into(),
                unit: None,
                kind: ValueKind::Text,
            },
        ],
        rows,
    };

    let mut items = vec![FragmentItem::Table { table }];
    items.extend(sampling_note(meta.n_periods));

    Ok(Fragment {
        title: "Result Extremes".into(),
        items,
    })
}

fn extremes_row(quantity: &str, min: f64, max: f64, unit: &str) -> Vec<Value> {
    vec![text(quantity), num(min), num(max), text(unit)]
}

fn pump_energy(out_path: &Path, network: &Network) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_pumps == 0 {
        return Err(BlockError::Unavailable {
            reason: "the network has no pumps".into(),
        });
    }
    let energy = out_reader::read_energy(out_path, &meta)
        .map_err(|message| BlockError::Failed { message })?;

    let rows = energy
        .pumps
        .iter()
        .map(|rec| {
            // `link_index` is 1-based into the network's link order.
            let id = (rec.link_index as usize)
                .checked_sub(1)
                .and_then(|idx| network.links.get(idx))
                .map(|link| link.base.id.clone())
                .unwrap_or_else(|| format!("pump #{}", rec.link_index));
            vec![
                text(id),
                num(f64::from(rec.pct_online)),
                num(f64::from(rec.avg_efficiency)),
                num(f64::from(rec.avg_kw)),
                num(f64::from(rec.peak_kw)),
                num(f64::from(rec.avg_cost_per_day)),
            ]
        })
        .collect();

    let table = Table {
        columns: vec![
            Column {
                name: "Pump".into(),
                unit: None,
                kind: ValueKind::Text,
            },
            Column {
                name: "Utilization".into(),
                unit: Some("%".into()),
                kind: ValueKind::Number,
            },
            Column {
                name: "Avg. efficiency".into(),
                unit: Some("%".into()),
                kind: ValueKind::Number,
            },
            Column {
                name: "Avg. power".into(),
                unit: Some("kW".into()),
                kind: ValueKind::Number,
            },
            Column {
                name: "Peak power".into(),
                unit: Some("kW".into()),
                kind: ValueKind::Number,
            },
            Column {
                name: "Avg. cost per day".into(),
                unit: None,
                kind: ValueKind::Number,
            },
        ],
        rows,
    };

    Ok(Fragment {
        title: "Pump Energy".into(),
        items: vec![
            FragmentItem::Table { table },
            FragmentItem::KeyValues {
                entries: vec![entry("Demand charge", num(f64::from(energy.demand_charge)))],
            },
        ],
    })
}

fn quality_summary(out_path: &Path) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.quality_flag == 0 {
        return Err(BlockError::Unavailable {
            reason: "the run has no water-quality results".into(),
        });
    }
    let ranges = out_reader::scan_ranges(out_path, &meta, MAX_RANGE_SAMPLES)
        .map_err(|message| BlockError::Failed { message })?;

    let unit = quality_unit(meta.quality_flag);
    let bound = |b: Option<f64>| b.map(|v| num_unit(v, unit)).unwrap_or(Value::Absent);
    let entries = vec![
        entry("Quality mode", text(quality_mode_label(meta.quality_flag))),
        entry("Minimum", bound(ranges.quality_min)),
        entry("Maximum", bound(ranges.quality_max)),
    ];

    let mut items = vec![FragmentItem::KeyValues { entries }];
    items.extend(sampling_note(meta.n_periods));

    Ok(Fragment {
        title: "Water Quality Summary".into(),
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_namespaced_and_unique() {
        let mut ids: Vec<_> = CATALOG.iter().map(|b| b.id).collect();
        for id in &ids {
            assert!(
                id.starts_with("wds."),
                "block id {id:?} must be wds.-namespaced"
            );
        }
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len(), "block ids must be unique");
    }

    #[test]
    fn unknown_block_id_is_rejected() {
        let network = crate::io::parse(FIXTURE_INP.as_bytes()).expect("parse network");
        let err = produce_report_block("wds.nope", Path::new("/nonexistent"), &network)
            .expect_err("unknown id must fail");
        assert!(matches!(err, BlockError::UnknownBlock { .. }));
    }

    #[test]
    fn formats_hms_clock_text() {
        assert_eq!(fmt_hms(0.0), "0:00:00");
        assert_eq!(fmt_hms(3600.0), "1:00:00");
        assert_eq!(fmt_hms(90061.0), "25:01:01");
        assert_eq!(fmt_hms(-5.0), "0:00:00");
    }

    // ── Fixture: a tiny network persisted to a real .out file ────────────────

    const FIXTURE_INP: &str = "[JUNCTIONS]\nJ1  0  10\nJ2  0  10\n\n\
        [RESERVOIRS]\nR1  100\n\n\
        [PIPES]\nP1  R1  J1  1000  300  100  0  Open\nP2  J1  J2  800  250  100  0  Open\n\n\
        [OPTIONS]\nUnits  LPS\nHeadloss  H-W\n\n[END]\n";

    struct MockSession {
        network: crate::Network,
        snapshots: Vec<crate::io::HydSnapshot>,
    }

    impl crate::io::WritableSimulation for MockSession {
        fn net(&self) -> &crate::Network {
            &self.network
        }
        fn snapshots(&self) -> &[crate::io::HydSnapshot] {
            &self.snapshots
        }
        fn pump_energy_at(&self, _: usize) -> Option<&crate::io::PumpEnergy> {
            None
        }
        fn peak_demand_kw(&self) -> f64 {
            0.0
        }
        fn mass_balance(&self) -> Option<&crate::io::MassBalance> {
            None
        }
        fn warnings(&self) -> &[crate::io::SimWarning] {
            &[]
        }
        fn pump_energy_by_id(&self, _: &str) -> Option<&crate::io::PumpEnergy> {
            None
        }
        fn analysis_times(&self) -> (Option<std::time::SystemTime>, Option<std::time::SystemTime>) {
            (None, None)
        }
        fn flow_balance(&self) -> Option<&crate::io::FlowBalance> {
            None
        }
        fn flow_balance_summary(&self) -> Option<crate::io::FlowBalanceSummary> {
            None
        }
    }

    /// Persist a one-snapshot `.out` for the fixture network and run `f`
    /// with its path; the file is removed afterwards.
    fn with_fixture_out(f: impl FnOnce(&Path, &crate::Network)) {
        let network = crate::io::parse(FIXTURE_INP.as_bytes()).expect("parse network");
        let mut node_states: Vec<crate::NodeState> = network
            .nodes
            .iter()
            .map(|n| crate::NodeState {
                head: n.base.elevation,
                ..crate::NodeState::default()
            })
            .collect();
        node_states[0].head = 50.0;
        node_states[1].head = 45.0;
        let link_states = network
            .links
            .iter()
            .map(|_| crate::LinkState::default())
            .collect();
        let session = MockSession {
            network,
            snapshots: vec![crate::io::HydSnapshot {
                t: 0.0,
                node_states,
                link_states,
            }],
        };

        let mut buf = std::io::Cursor::new(Vec::new());
        crate::io::out_writer::write_binary_output(
            &mut buf,
            &session,
            "test.inp",
            "",
            crate::FlowUnits::Lps,
        )
        .expect("write .out");

        let mut path = std::env::temp_dir();
        path.push(format!(
            "hydra-report-blocks-{}-{:?}.out",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, buf.into_inner()).expect("persist .out");
        f(&path, &session.network);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn run_summary_reports_counts_units_and_window() {
        with_fixture_out(|path, network| {
            let fragment = produce_report_block("wds.run-summary", path, network)
                .expect("produce run summary");
            assert_eq!(fragment.title, "Run Summary");
            let FragmentItem::KeyValues { entries } = &fragment.items[0] else {
                panic!("expected key-values item");
            };
            let get = |label: &str| {
                &entries
                    .iter()
                    .find(|e| e.label == label)
                    .unwrap_or_else(|| panic!("missing entry {label:?}"))
                    .value
            };
            assert_eq!(get("Junctions"), &int(2));
            assert_eq!(get("Tanks & reservoirs"), &int(1));
            assert_eq!(get("Links"), &int(2));
            assert_eq!(get("Pumps"), &int(0));
            assert_eq!(get("Flow units"), &text("LPS"));
            assert_eq!(get("Pressure units"), &text("m"));
            assert_eq!(get("Quality mode"), &text("None"));
            assert_eq!(get("Reporting periods"), &int(1));
        });
    }

    #[test]
    fn result_extremes_covers_core_quantities_without_quality() {
        with_fixture_out(|path, network| {
            let fragment = produce_report_block("wds.result-extremes", path, network)
                .expect("produce extremes");
            let FragmentItem::Table { table } = &fragment.items[0] else {
                panic!("expected table item");
            };
            let quantities: Vec<_> = table
                .rows
                .iter()
                .map(|r| match &r[0] {
                    Value::Text { value } => value.clone(),
                    other => panic!("expected text quantity, got {other:?}"),
                })
                .collect();
            // No quality run → no Quality row.
            assert_eq!(
                quantities,
                ["Pressure", "Head", "Demand", "Flow", "Velocity"]
            );
            // Junction gauge pressures are 50 and 45 m; the reservoir sits at 0.
            let Value::Number { value: pmax, .. } = table.rows[0][2].clone() else {
                panic!("expected numeric pressure max");
            };
            assert!((pmax - 50.0).abs() < 0.5, "pressure max ≈ 50 m, got {pmax}");
            // One period → exhaustive scan, no sampling note.
            assert_eq!(fragment.items.len(), 1);
        });
    }

    #[test]
    fn pump_energy_requires_a_pump() {
        with_fixture_out(|path, network| {
            let err = produce_report_block("wds.pump-energy", path, network)
                .expect_err("no pumps in fixture");
            assert_eq!(
                err,
                BlockError::Unavailable {
                    reason: "the network has no pumps".into()
                }
            );
        });
    }

    #[test]
    fn quality_summary_requires_a_quality_run() {
        with_fixture_out(|path, network| {
            let err = produce_report_block("wds.quality-summary", path, network)
                .expect_err("fixture has no quality results");
            assert_eq!(
                err,
                BlockError::Unavailable {
                    reason: "the run has no water-quality results".into()
                }
            );
        });
    }
}
