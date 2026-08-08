//! Report blocks: the WDS implementation of the foundation layer's
//! reportable-output contract (analysis spec §4, hydra-common spec §3).
//!
//! Counts and result values come from the `.out` file
//! (result-authoritative); element identifiers and declared display units
//! come from the network. Production is read-only and deterministic.

use std::path::Path;

use hydra_common::{
    BlockDescriptor, BlockError, Chart, ChartData, Column, Fragment, FragmentItem, KeyValue,
    LineSeries, OptionDescriptor, OptionKind, Table, Value, ValueKind,
};

use super::binning::threshold_bands;
use super::demand_reliability::{
    compute_demand_reliability_from_out_with_options, DemandReliabilityOptions,
};
use super::service_compliance::{compute_service_compliance_from_out, ServiceComplianceThresholds};
use crate::io::out_reader::{self, OutMetadata};
use crate::{FlowUnits, LinkKind, Network, NodeKind};

/// Sample budget for the range scan (analysis spec §4.2). Matches the
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
    BlockDescriptor {
        id: "wds.service-compliance",
        title: "Pressure Adequacy",
        summary: "Junction-pressure service compliance against a minimum (and optional \
                  maximum) pressure criterion, with the worst-performing junctions.",
    },
    BlockDescriptor {
        id: "wds.demand-reliability",
        title: "Demand Reliability",
        summary: "Delivered-vs-required demand volumes, reliability ratio, and the \
                  worst-served junctions.",
    },
    BlockDescriptor {
        id: "wds.pressure-distribution",
        title: "Pressure Distribution",
        summary: "Distribution of each junction's minimum pressure over the run.",
    },
    BlockDescriptor {
        id: "wds.velocity-distribution",
        title: "Velocity Distribution",
        summary: "Distribution of each pipe's maximum velocity over the run \
                  (pumps and valves excluded).",
    },
    BlockDescriptor {
        id: "wds.tank-levels",
        title: "Tank Levels",
        summary: "Hydraulic head of each tank over the reporting horizon.",
    },
    BlockDescriptor {
        id: "wds.mass-balance",
        title: "Mass Balance",
        summary: "Cumulative network inflow and outflow, closure percentage, and \
                  per-period closure over the reporting horizon.",
    },
    BlockDescriptor {
        id: "wds.pipe-criticality",
        title: "Pipe Criticality",
        summary: "Pipes ranked by peak velocity over the reporting horizon.",
    },
    BlockDescriptor {
        id: "wds.pressure-thresholds",
        title: "Pressure Thresholds",
        summary: "Junction minimum pressure counted into caller-supplied threshold \
                  bands rather than observed-range bins.",
    },
    BlockDescriptor {
        id: "wds.velocity-thresholds",
        title: "Velocity Thresholds",
        summary: "Pipe maximum velocity counted into caller-supplied threshold bands.",
    },
];

/// The water-distribution engine's report-block catalog (analysis spec §4.1).
pub fn report_catalog() -> &'static [BlockDescriptor] {
    CATALOG
}

/// Produce the fragment for one catalog block from a persisted `.out`
/// file, the corresponding loaded network, and the optional per-block
/// options value (analysis spec §4.1.1/§4.2).
pub fn produce_report_block(
    id: &str,
    out_path: &Path,
    network: &Network,
    options: Option<&serde_json::Value>,
) -> Result<Fragment, BlockError> {
    match id {
        "wds.run-summary" => run_summary(out_path, network),
        "wds.result-extremes" => result_extremes(out_path, network),
        "wds.pump-energy" => pump_energy(out_path, network),
        "wds.quality-summary" => quality_summary(out_path),
        "wds.service-compliance" => service_compliance(out_path, network, options),
        "wds.demand-reliability" => demand_reliability(out_path, network, options),
        "wds.pressure-distribution" => pressure_distribution(out_path, network),
        "wds.velocity-distribution" => velocity_distribution(out_path, network),
        "wds.tank-levels" => tank_levels(out_path, network),
        "wds.mass-balance" => mass_balance(out_path, network),
        "wds.pipe-criticality" => pipe_criticality(out_path, network, options),
        "wds.pressure-thresholds" => pressure_thresholds(out_path, network, options),
        "wds.velocity-thresholds" => velocity_thresholds(out_path, network, options),
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
    Value::Number {
        value,
        unit: None,
        quantity: None,
    }
}

fn num_unit(value: f64, unit: &str) -> Value {
    Value::Number {
        value,
        unit: Some(unit.into()),
        quantity: None,
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

/// Display unit for quality values by mode flag (analysis spec §4.2):
/// chemical concentration reports the file-default mg/L.
fn quality_unit(flag: i32) -> &'static str {
    match flag {
        2 => "hours",
        3 => "%",
        _ => "mg/L",
    }
}

/// Sampling-disclosure note when the period count exceeds the scan budget
/// (analysis spec §4.2); `None` when the scan was exhaustive.
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

    // Counted from the network, not the results file: the `.out` prolog puts
    // reservoirs inside its tank group and carries no link-type breakdown, so
    // it can only report a combined figure. A reservoir is an infinite-source
    // boundary and a tank is finite storage — reporting them as one number
    // describes the file's layout rather than the network, and would make this
    // block disagree with the engine's own run-log summary (spec §4.2).
    let count_nodes =
        |f: fn(&NodeKind) -> bool| int(network.nodes.iter().filter(|n| f(&n.kind)).count());
    let count_links =
        |f: fn(&LinkKind) -> bool| int(network.links.iter().filter(|l| f(&l.kind)).count());

    let entries = vec![
        entry(
            "Junctions",
            count_nodes(|k| matches!(k, NodeKind::Junction(_))),
        ),
        entry(
            "Reservoirs",
            count_nodes(|k| matches!(k, NodeKind::Reservoir(_))),
        ),
        entry("Tanks", count_nodes(|k| matches!(k, NodeKind::Tank(_)))),
        entry("Pipes", count_links(|k| matches!(k, LinkKind::Pipe(_)))),
        entry("Pumps", count_links(|k| matches!(k, LinkKind::Pump(_)))),
        entry("Valves", count_links(|k| matches!(k, LinkKind::Valve(_)))),
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
/// unit system (analysis spec §4.2). Used only where output stays
/// file-flavored by spec: band labels, narrative notes, option echoes'
/// prose, and composite units.
fn unit_labels(network: &Network) -> (&'static str, &'static str, &'static str) {
    if is_si(network.options.flow_units) {
        ("m", "m", "m/s")
    } else {
        ("psi", "ft", "ft/s")
    }
}

// ── SI display conversion (analysis spec §4.2 Units) ──────────────────────────

/// This engine's §5 quantity descriptor for `key`, or `None` for a key the
/// catalog does not declare — which `every_used_quantity_key_is_cataloged`
/// below rules out for every key this module writes.
fn qty(key: &str) -> Option<&'static hydra_common::QuantityDescriptor> {
    crate::descriptors::QUANTITIES.iter().find(|q| q.key == key)
}

/// Converts values read from the results file — which carries the model's
/// declared display units — into quantity-catalog SI display units, so a
/// tagged value means what hydra-common §3.3 says it means.
///
/// Linear quantities convert through the §5 descriptor itself (its US→SI
/// inverse): the descriptor is the presentation authority, and inverting
/// it here makes US-family re-display reproduce the file's value exactly.
/// Flow converts by the declared spelling's §3.1 factor to L/s — eleven
/// spellings map onto one SI display unit, so an exact spelling round-trip
/// is not a property any conversion could have.
struct SiDisplay {
    si: bool,
    /// Declared flow units per m³/s (model spec §3.1).
    flow_factor: f64,
}

impl SiDisplay {
    fn new(network: &Network) -> Self {
        let ucf = crate::io::units::make_ucf(
            network.options.flow_units,
            network.options.specific_gravity,
        );
        Self {
            si: is_si(network.options.flow_units),
            flow_factor: ucf.flow,
        }
    }

    /// A descriptor-covered linear quantity: pressure, head, velocity,
    /// volume. Identity on SI files; descriptor-inverse on US files.
    fn linear(&self, key: &str, file_value: f64) -> f64 {
        if self.si {
            file_value
        } else {
            qty(key).map_or(file_value, |d| d.us_to_si(file_value))
        }
    }

    /// Flow or demand in the declared spelling → L/s.
    fn flow(&self, file_value: f64) -> f64 {
        file_value * 1_000.0 / self.flow_factor
    }
}

/// A tagged key-value number wearing its quantity's SI label.
fn q_num(value_si: f64, key: &str) -> Value {
    Value::Number {
        value: value_si,
        unit: qty(key).map(|d| d.si_label.to_string()),
        quantity: Some(key.into()),
    }
}

/// A tagged column: the header carries the SI label, and the tag converts
/// every number under it (hydra-common spec §3.3).
fn q_col(name: &str, key: &str) -> Column {
    Column {
        name: name.into(),
        unit: qty(key).map(|d| d.si_label.to_string()),
        kind: ValueKind::Number,
        quantity: Some(key.into()),
    }
}

fn result_extremes(out_path: &Path, network: &Network) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_periods == 0 {
        return Err(BlockError::Failed {
            message: "The results file holds no reporting periods.".into(),
        });
    }
    let ranges = out_reader::scan_ranges(out_path, &meta, MAX_RANGE_SAMPLES)
        .map_err(|message| BlockError::Failed { message })?;

    let u = SiDisplay::new(network);

    // Rows mix quantities, so the min/max cells are tagged per value and
    // wear their own unit text — a per-column unit or a text unit column
    // could not follow a chosen display family (analysis spec §4.2 Units).
    let tagged_row = |label: &str, min: f64, max: f64, key: &str| {
        vec![text(label), q_num(min, key), q_num(max, key)]
    };
    let mut rows = vec![
        tagged_row(
            "Pressure",
            u.linear("pressure", ranges.pressure_min),
            u.linear("pressure", ranges.pressure_max),
            "pressure",
        ),
        tagged_row(
            "Head",
            u.linear("head", ranges.head_min),
            u.linear("head", ranges.head_max),
            "head",
        ),
        tagged_row(
            "Demand",
            u.flow(ranges.demand_min),
            u.flow(ranges.demand_max),
            "demand",
        ),
        tagged_row(
            "Flow",
            u.flow(ranges.flow_min),
            u.flow(ranges.flow_max),
            "flow",
        ),
        tagged_row(
            "Velocity",
            u.linear("velocity", ranges.velocity_min),
            u.linear("velocity", ranges.velocity_max),
            "velocity",
        ),
    ];
    if let (Some(qmin), Some(qmax)) = (ranges.quality_min, ranges.quality_max) {
        // Quality's unit follows the mode and is identical in both display
        // families, so it stays untagged text (analysis spec §4.2 Units).
        let unit = quality_unit(meta.quality_flag);
        rows.push(vec![
            text("Quality"),
            num_unit(qmin, unit),
            num_unit(qmax, unit),
        ]);
    }

    let table = Table {
        columns: vec![
            Column {
                name: "Quantity".into(),
                unit: None,
                kind: ValueKind::Text,
                quantity: None,
            },
            Column {
                name: "Minimum".into(),
                unit: None,
                kind: ValueKind::Number,
                quantity: None,
            },
            Column {
                name: "Maximum".into(),
                unit: None,
                kind: ValueKind::Number,
                quantity: None,
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

fn pump_energy(out_path: &Path, network: &Network) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_pumps == 0 {
        return Err(BlockError::Unavailable {
            reason: "The network has no pumps.".into(),
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
                quantity: None,
            },
            Column {
                name: "Utilization".into(),
                unit: Some("%".into()),
                kind: ValueKind::Number,
                quantity: None,
            },
            Column {
                name: "Avg. efficiency".into(),
                unit: Some("%".into()),
                kind: ValueKind::Number,
                quantity: None,
            },
            Column {
                name: "Avg. power".into(),
                unit: Some("kW".into()),
                kind: ValueKind::Number,
                quantity: None,
            },
            Column {
                name: "Peak power".into(),
                unit: Some("kW".into()),
                kind: ValueKind::Number,
                quantity: None,
            },
            Column {
                name: "Avg. cost per day".into(),
                unit: None,
                kind: ValueKind::Number,
                quantity: None,
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
            reason: "The run has no water-quality results.".into(),
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

// ── Option descriptions (analysis spec §4.1.1, hydra-common spec §3.2.1) ──────

/// Describe the options `id` accepts, resolved for `network`.
///
/// Resolved against the model rather than fixed by the catalog because the
/// defaults and units of half these options follow the file's declared unit
/// system: `minPressure` is 14 m on an SI model and 20 psi on a US one. A
/// consumer displays what it is given and computes no units of its own.
///
/// Advisory only — production validates independently, so an unknown id or a
/// block with nothing to configure simply yields an empty list rather than an
/// error.
pub fn report_block_options(id: &str, network: &Network) -> Vec<OptionDescriptor> {
    let si = is_si(network.options.flow_units);
    let (pressure_unit, _, velocity_unit) = unit_labels(network);

    let worst_count = |what: &str| OptionDescriptor {
        key: "worstCount".into(),
        label: "Rows in the worst-performing table".into(),
        help: format!("How many {what} to list, worst first."),
        kind: OptionKind::Integer {
            default: Some(DEFAULT_WORST_COUNT as i64),
            min: Some(1),
            max: None,
        },
        unit: None,
    };

    match id {
        "wds.service-compliance" => vec![
            OptionDescriptor {
                key: "minPressure".into(),
                label: "Minimum acceptable pressure".into(),
                help: "Junction pressures below this count as a service violation.".into(),
                kind: OptionKind::Number {
                    default: Some(if si { 14.0 } else { 20.0 }),
                    min: Some(0.0),
                    max: None,
                },
                unit: Some(pressure_unit.into()),
            },
            OptionDescriptor {
                key: "maxPressure".into(),
                label: "Maximum acceptable pressure".into(),
                help: "Pressures above this count as a violation. Leave empty to \
                       disable the upper bound entirely."
                    .into(),
                kind: OptionKind::Number {
                    default: None,
                    min: Some(0.0),
                    max: None,
                },
                unit: Some(pressure_unit.into()),
            },
            worst_count("junctions"),
        ],
        // `deficitTolerance` is deliberately NOT described. It is a
        // floating-point noise floor rather than an engineering criterion, so
        // its default is imperceptible in any unit — 1e-9 m³/s is 0.000001 L/s
        // — and offering a field nobody can pick a value for is worse than
        // offering none. It remains fully accepted from a hand-authored
        // template and from the CLI; descriptions are advisory, never the
        // validation authority (hydra-common spec §3.2.1).
        "wds.demand-reliability" => vec![worst_count("junctions")],
        "wds.pipe-criticality" => vec![OptionDescriptor {
            key: "topCount".into(),
            label: "Rows in the ranked-pipes table".into(),
            help: "How many pipes to list, highest peak velocity first.".into(),
            kind: OptionKind::Integer {
                default: Some(DEFAULT_TOP_COUNT as i64),
                min: Some(1),
                max: None,
            },
            unit: None,
        }],
        "wds.pressure-thresholds" => vec![OptionDescriptor {
            key: "edges".into(),
            label: "Band boundaries".into(),
            help: "Ascending pressure boundaries to count junctions into. The \
                   outermost bands are unbounded, so no junction is dropped."
                .into(),
            kind: OptionKind::NumberList {
                default: Some(default_pressure_edges(si).to_vec()),
                min_len: Some(1),
                ascending: true,
            },
            unit: Some(pressure_unit.into()),
        }],
        "wds.velocity-thresholds" => vec![OptionDescriptor {
            key: "edges".into(),
            label: "Band boundaries".into(),
            help: "Ascending velocity boundaries to count pipes into. The \
                   outermost bands are unbounded, so no pipe is dropped."
                .into(),
            kind: OptionKind::NumberList {
                default: Some(default_velocity_edges(si).to_vec()),
                min_len: Some(1),
                ascending: true,
            },
            unit: Some(velocity_unit.into()),
        }],
        _ => Vec::new(),
    }
}

/// Spec §4.1.1 default pressure bands; US files take psi-scaled equivalents.
fn default_pressure_edges(si: bool) -> &'static [f64] {
    if si {
        &[0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0]
    } else {
        &[0.0, 15.0, 30.0, 45.0, 60.0, 75.0, 85.0]
    }
}

/// Spec §4.1.1 default velocity bands; US files take ft/s equivalents.
fn default_velocity_edges(si: bool) -> &'static [f64] {
    if si {
        &[0.1, 0.3, 0.6, 1.0]
    } else {
        &[0.3, 1.0, 2.0, 3.3]
    }
}

// ── Option parsing (analysis spec §4.1.1) ─────────────────────────────────────
//
// Options are opaque JSON per the foundation contract; unknown fields are
// ignored, malformed values fail production naming the field.

fn opt_f64(
    options: Option<&serde_json::Value>,
    field: &str,
    require_non_negative: bool,
) -> Result<Option<f64>, BlockError> {
    let Some(value) = options.and_then(|o| o.get(field)) else {
        return Ok(None);
    };
    let number = value.as_f64().ok_or_else(|| BlockError::Failed {
        message: format!("Option {field:?} must be a number."),
    })?;
    if !number.is_finite() || (require_non_negative && number < 0.0) {
        return Err(BlockError::Failed {
            message: format!("Option {field:?} must be a finite non-negative number."),
        });
    }
    Ok(Some(number))
}

fn opt_usize(
    options: Option<&serde_json::Value>,
    field: &str,
) -> Result<Option<usize>, BlockError> {
    let Some(value) = options.and_then(|o| o.get(field)) else {
        return Ok(None);
    };
    let number = value.as_u64().ok_or_else(|| BlockError::Failed {
        message: format!("Option {field:?} must be a non-negative integer."),
    })?;
    Ok(Some(number as usize))
}

/// Default worst-junctions table length (analysis spec §4.1.1).
const DEFAULT_WORST_COUNT: usize = 10;

/// `12.3%` style share text used in narrative values.
fn percent(ratio: f64) -> Value {
    num_unit(ratio * 100.0, "%")
}

fn service_compliance(
    out_path: &Path,
    network: &Network,
    options: Option<&serde_json::Value>,
) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_periods == 0 {
        return Err(BlockError::Failed {
            message: "The results file holds no reporting periods.".into(),
        });
    }
    let (pressure_unit, _, _) = unit_labels(network);
    // Spec §4.1.1 default criterion: 14 m for SI files, 20 psi for US files.
    let default_min = if is_si(network.options.flow_units) {
        14.0
    } else {
        20.0
    };
    let min_pressure = opt_f64(options, "minPressure", true)?.unwrap_or(default_min);
    let max_pressure = opt_f64(options, "maxPressure", true)?;
    let worst_count = opt_usize(options, "worstCount")?.unwrap_or(DEFAULT_WORST_COUNT);

    let thresholds = ServiceComplianceThresholds {
        min_pressure,
        max_pressure,
    };
    let report = compute_service_compliance_from_out(out_path, thresholds).map_err(|e| {
        BlockError::Failed {
            message: e.to_string(),
        }
    })?;
    let summary = &report.summary;

    // The option arrives in file display units (§4.1.1, unchanged); its
    // echo converts like any measured value so the block never mixes
    // display families. The narrative note below stays file-flavored.
    let u = SiDisplay::new(network);
    let entries = vec![
        entry("Junctions analysed", int(summary.node_count)),
        entry(
            "Minimum pressure criterion",
            q_num(u.linear("pressure", min_pressure), "pressure"),
        ),
        entry(
            "Maximum pressure criterion",
            match max_pressure {
                Some(max) => q_num(u.linear("pressure", max), "pressure"),
                None => Value::Absent,
            },
        ),
        entry("Compliance", percent(summary.compliance_ratio())),
        entry("Samples below minimum", int(summary.below_min_samples)),
        entry("Samples above maximum", int(summary.above_max_samples)),
        entry(
            "Worst pressure deficit",
            q_num(u.linear("pressure", summary.worst_below_min), "pressure"),
        ),
        // A composite unit no quantity covers: file-flavored by spec.
        entry(
            "Pressure deficit integral",
            num_unit(
                summary.pressure_deficit_integral / 3600.0,
                &format!("{pressure_unit}·h"),
            ),
        ),
    ];

    // Narrative: how many junctions ever dipped below the criterion.
    let below_nodes = report
        .nodes
        .iter()
        .filter(|n| n.below_min_count > 0)
        .count();
    let note = if below_nodes == 0 {
        format!(
            "All analysed junctions stayed at or above the minimum pressure \
             criterion of {} {pressure_unit} for the entire run.",
            fmt_compact(min_pressure)
        )
    } else {
        format!(
            "{below_nodes} junction{} below the minimum pressure criterion of {} \
             {pressure_unit} during at least one reporting period.",
            if below_nodes == 1 { " fell" } else { "s fell" },
            fmt_compact(min_pressure)
        )
    };

    // Worst offenders: by violation ratio, then deficit integral, then id
    // for a deterministic order.
    let mut worst: Vec<_> = report
        .nodes
        .iter()
        .filter(|n| n.violating_sample_count() > 0)
        .collect();
    worst.sort_by(|a, b| {
        b.violation_ratio()
            .partial_cmp(&a.violation_ratio())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                b.pressure_deficit_integral
                    .partial_cmp(&a.pressure_deficit_integral)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then_with(|| node_id(network, a.node_index).cmp(&node_id(network, b.node_index)))
    });
    worst.truncate(worst_count);

    let mut items = vec![
        FragmentItem::KeyValues { entries },
        FragmentItem::Note { text: note },
    ];
    if !worst.is_empty() {
        let table = Table {
            columns: vec![
                Column {
                    name: "Junction".into(),
                    unit: None,
                    kind: ValueKind::Text,
                    quantity: None,
                },
                Column {
                    name: "Out-of-limit share".into(),
                    unit: Some("%".into()),
                    kind: ValueKind::Number,
                    quantity: None,
                },
                Column {
                    name: "Samples below min".into(),
                    unit: None,
                    kind: ValueKind::Integer,
                    quantity: None,
                },
                q_col("Worst deficit", "pressure"),
                Column {
                    name: "Longest violation streak".into(),
                    unit: Some("periods".into()),
                    kind: ValueKind::Integer,
                    quantity: None,
                },
            ],
            rows: worst
                .iter()
                .map(|n| {
                    vec![
                        text(node_id(network, n.node_index)),
                        num(n.violation_ratio() * 100.0),
                        int(n.below_min_count),
                        num(u.linear("pressure", n.worst_below_min)),
                        int(n.longest_violation_streak),
                    ]
                })
                .collect(),
        };
        items.push(FragmentItem::Table { table });
    }

    Ok(Fragment {
        title: "Pressure Adequacy".into(),
        items,
    })
}

fn demand_reliability(
    out_path: &Path,
    network: &Network,
    options: Option<&serde_json::Value>,
) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_periods == 0 {
        return Err(BlockError::Failed {
            message: "The results file holds no reporting periods.".into(),
        });
    }
    let mut dr_options = DemandReliabilityOptions::default();
    if let Some(tolerance) = opt_f64(options, "deficitTolerance", true)? {
        dr_options.deficit_tolerance = tolerance;
    }
    let worst_count = opt_usize(options, "worstCount")?.unwrap_or(DEFAULT_WORST_COUNT);

    let report = compute_demand_reliability_from_out_with_options(out_path, network, dr_options)
        .map_err(|e| BlockError::Failed {
            message: e.to_string(),
        })?;
    let summary = &report.summary;

    let entries = vec![
        entry("Junctions analysed", int(summary.node_count)),
        entry("Demand model", text(format!("{:?}", report.demand_model))),
        // Already computed in m³ (SI display units); tagging costs no
        // conversion and buys family re-display.
        entry("Required volume", q_num(summary.required_volume, "volume")),
        entry(
            "Delivered volume",
            q_num(summary.delivered_volume, "volume"),
        ),
        entry("Unmet volume", q_num(summary.unmet_volume, "volume")),
        entry("Reliability", percent(summary.reliability_ratio())),
        entry(
            "Deficit (junction, period) pairs",
            int(summary.deficit_periods),
        ),
    ];

    // Worst-served junctions: by reliability ascending, then unmet volume
    // descending, then id.
    let mut worst: Vec<_> = report
        .nodes
        .iter()
        .filter(|n| n.unmet_volume > 0.0 || n.deficit_periods > 0)
        .collect();
    worst.sort_by(|a, b| {
        a.reliability_ratio()
            .partial_cmp(&b.reliability_ratio())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                b.unmet_volume
                    .partial_cmp(&a.unmet_volume)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    worst.truncate(worst_count);

    let mut items = vec![FragmentItem::KeyValues { entries }];
    if !worst.is_empty() {
        let table = Table {
            columns: vec![
                Column {
                    name: "Junction".into(),
                    unit: None,
                    kind: ValueKind::Text,
                    quantity: None,
                },
                Column {
                    name: "Reliability".into(),
                    unit: Some("%".into()),
                    kind: ValueKind::Number,
                    quantity: None,
                },
                q_col("Unmet volume", "volume"),
                Column {
                    name: "Deficit periods".into(),
                    unit: None,
                    kind: ValueKind::Integer,
                    quantity: None,
                },
                Column {
                    name: "Longest deficit streak".into(),
                    unit: Some("periods".into()),
                    kind: ValueKind::Integer,
                    quantity: None,
                },
            ],
            rows: worst
                .iter()
                .map(|n| {
                    vec![
                        text(n.node_id.clone()),
                        num(n.reliability_ratio() * 100.0),
                        num(n.unmet_volume),
                        int(n.deficit_periods),
                        int(n.longest_deficit_streak),
                    ]
                })
                .collect(),
        };
        items.push(FragmentItem::Table { table });
    }

    Ok(Fragment {
        title: "Demand Reliability".into(),
        items,
    })
}

fn pressure_distribution(out_path: &Path, network: &Network) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_periods == 0 {
        return Err(BlockError::Failed {
            message: "The results file holds no reporting periods.".into(),
        });
    }
    let scan = out_reader::scan_analytics(out_path, &meta)
        .map_err(|message| BlockError::Failed { message })?;
    let (pressure_unit, _, _) = unit_labels(network);
    // Junctions only (analysis spec §4.1.2): tank/reservoir indices sit at
    // the tail of the node list.
    let junction_count = meta.n_nodes.saturating_sub(meta.n_tanks);
    let values: Vec<f64> = scan
        .node_min_pressure
        .iter()
        .take(junction_count)
        .copied()
        .filter(|v| v.is_finite())
        .collect();

    Ok(distribution_fragment(
        "Pressure Distribution",
        "Junctions",
        "Minimum pressure",
        pressure_unit,
        &values,
        meta.n_periods,
    ))
}

fn velocity_distribution(out_path: &Path, network: &Network) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_periods == 0 {
        return Err(BlockError::Failed {
            message: "The results file holds no reporting periods.".into(),
        });
    }
    let scan = out_reader::scan_analytics(out_path, &meta)
        .map_err(|message| BlockError::Failed { message })?;
    let (_, _, velocity_unit) = unit_labels(network);
    // Pipes only (analysis spec §4.1.2): a pump or valve has no pipe
    // velocity, so including them banks one spurious zero per non-pipe link
    // into the lowest bin. Link order in the `.out` file matches
    // `network.links`, so the index maps directly.
    let values: Vec<f64> = scan
        .link_max_velocity
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            network
                .links
                .get(*i)
                .is_some_and(|l| matches!(l.kind, crate::LinkKind::Pipe(_)))
        })
        .map(|(_, v)| *v)
        .filter(|v| v.is_finite())
        .collect();

    Ok(distribution_fragment(
        "Velocity Distribution",
        "Pipes",
        "Maximum velocity",
        velocity_unit,
        &values,
        meta.n_periods,
    ))
}

/// Equal-width six-bin distribution as a bar chart (analysis spec
/// §4.1.2): edges rounded outward to whole display units; a degenerate
/// range yields a single bin. Table-derivable everywhere per the
/// foundation contract.
/// Ascending threshold edges from the `edges` option, or the supplied default
/// (analysis spec §4.1.1). Validates strict ascent and non-emptiness.
fn opt_edges(options: Option<&serde_json::Value>, default: &[f64]) -> Result<Vec<f64>, BlockError> {
    let Some(value) = options.and_then(|o| o.get("edges")) else {
        return Ok(default.to_vec());
    };
    let array = value.as_array().ok_or_else(|| BlockError::Failed {
        message: "Option \"edges\" must be an array of numbers.".into(),
    })?;
    let mut edges = Vec::with_capacity(array.len());
    for item in array {
        let n = item
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| BlockError::Failed {
                message: "Option \"edges\" must contain only finite numbers.".into(),
            })?;
        edges.push(n);
    }
    if edges.is_empty() {
        return Err(BlockError::Failed {
            message: "Option \"edges\" must contain at least one boundary.".into(),
        });
    }
    if edges.windows(2).any(|w| w[1] <= w[0]) {
        return Err(BlockError::Failed {
            message: "Option \"edges\" must be strictly ascending.".into(),
        });
    }
    Ok(edges)
}

/// Bar chart of threshold-band counts, shared by the two `*-thresholds` blocks.
fn threshold_fragment(
    title: &str,
    element_label: &str,
    quantity_label: &str,
    unit: &str,
    values: &[f64],
    edges: &[f64],
) -> Fragment {
    let counts = threshold_bands(values, edges);
    let mut categories = Vec::with_capacity(counts.len());
    categories.push(format!("< {} {unit}", fmt_edge(edges[0])));
    for w in edges.windows(2) {
        categories.push(format!("{} – {} {unit}", fmt_edge(w[0]), fmt_edge(w[1])));
    }
    categories.push(format!("≥ {} {unit}", fmt_edge(edges[edges.len() - 1])));

    let mut items = vec![FragmentItem::Chart {
        chart: Chart {
            x_label: format!("{quantity_label} band"),
            x_unit: Some(unit.into()),
            x_quantity: None,
            y_label: element_label.into(),
            y_unit: None,
            y_quantity: None,
            data: ChartData::Bar {
                categories,
                values: counts.iter().map(|&c| c as f64).collect(),
            },
        },
    }];
    if values.is_empty() {
        items.push(FragmentItem::Note {
            text: format!(
                "No {} carry a value for this quantity.",
                element_label.to_lowercase()
            ),
        });
    }
    Fragment {
        title: title.into(),
        items,
    }
}

/// Compact edge label: whole numbers where exact, one decimal otherwise.
fn fmt_edge(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

/// Junction minimum pressures, in the file's pressure display unit.
fn junction_min_pressures(scan: &out_reader::AnalyticsScan, meta: &OutMetadata) -> Vec<f64> {
    let junction_count = meta.n_nodes.saturating_sub(meta.n_tanks);
    scan.node_min_pressure
        .iter()
        .take(junction_count)
        .copied()
        .filter(|v| v.is_finite())
        .collect()
}

/// Per-pipe maximum velocities (pumps and valves excluded, §4.1.2).
fn pipe_max_velocities(scan: &out_reader::AnalyticsScan, network: &Network) -> Vec<(usize, f64)> {
    scan.link_max_velocity
        .iter()
        .enumerate()
        .filter(|(i, v)| {
            v.is_finite()
                && network
                    .links
                    .get(*i)
                    .is_some_and(|l| matches!(l.kind, crate::LinkKind::Pipe(_)))
        })
        .map(|(i, v)| (i, *v))
        .collect()
}

fn mass_balance(out_path: &Path, network: &Network) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_periods == 0 {
        return Err(BlockError::Failed {
            message: "The results file holds no reporting periods.".into(),
        });
    }
    let scan = out_reader::scan_analytics(out_path, &meta)
        .map_err(|message| BlockError::Failed { message })?;
    let (_, _, _) = unit_labels(network);

    // Closure is outflow over inflow; an empty or sourceless run leaves it
    // undefined rather than dividing by zero, and reports as 100 % closed.
    let closure = if scan.total_inflow > 0.0 {
        (scan.total_outflow / scan.total_inflow * 100.0).min(100.0)
    } else {
        100.0
    };

    let entries = vec![
        entry("Cumulative inflow", q_num(scan.total_inflow, "volume")),
        entry("Cumulative outflow", q_num(scan.total_outflow, "volume")),
        entry(
            "Imbalance",
            q_num(scan.total_inflow - scan.total_outflow, "volume"),
        ),
        entry("Closure", num_unit(closure, "%")),
    ];

    let points: Vec<[f64; 2]> = scan
        .mb_series
        .iter()
        .enumerate()
        .map(|(p, &pct)| {
            let hours = (meta.report_start + meta.report_step * p as f64) / 3600.0;
            [hours, pct]
        })
        .collect();

    Ok(Fragment {
        title: "Mass Balance".into(),
        items: vec![
            FragmentItem::KeyValues { entries },
            FragmentItem::Chart {
                chart: Chart {
                    x_label: "Time".into(),
                    x_unit: Some("h".into()),
                    x_quantity: None,
                    y_label: "Closure".into(),
                    y_unit: Some("%".into()),
                    y_quantity: None,
                    data: ChartData::Line {
                        series: vec![LineSeries {
                            name: "Closure".into(),
                            points,
                        }],
                    },
                },
            },
        ],
    })
}

const DEFAULT_TOP_COUNT: usize = 5;

fn pipe_criticality(
    out_path: &Path,
    network: &Network,
    options: Option<&serde_json::Value>,
) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_periods == 0 {
        return Err(BlockError::Failed {
            message: "The results file holds no reporting periods.".into(),
        });
    }
    let top_count = opt_usize(options, "topCount")?.unwrap_or(DEFAULT_TOP_COUNT);
    let scan = out_reader::scan_analytics(out_path, &meta)
        .map_err(|message| BlockError::Failed { message })?;
    let u = SiDisplay::new(network);

    let mut ranked = pipe_max_velocities(&scan, network);
    if ranked.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The network has no pipes carrying velocity results.".into(),
        });
    }
    // Descending by peak velocity; link index breaks ties so the ordering is
    // deterministic for equal velocities.
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });

    let rows: Vec<Vec<Value>> = ranked
        .iter()
        .take(top_count)
        .map(|(idx, v)| {
            let id = network
                .links
                .get(*idx)
                .map(|l| l.base.id.clone())
                .unwrap_or_default();
            vec![text(id), num(u.linear("velocity", *v))]
        })
        .collect();

    let shown = rows.len();
    let mut items = vec![FragmentItem::Table {
        table: Table {
            columns: vec![
                Column {
                    name: "Pipe".into(),
                    unit: None,
                    kind: ValueKind::Text,
                    quantity: None,
                },
                q_col("Peak velocity", "velocity"),
            ],
            rows,
        },
    }];
    if ranked.len() > shown {
        items.push(FragmentItem::Note {
            text: format!("Showing the {shown} fastest of {} pipes.", ranked.len()),
        });
    }
    Ok(Fragment {
        title: "Pipe Criticality".into(),
        items,
    })
}

fn pressure_thresholds(
    out_path: &Path,
    network: &Network,
    options: Option<&serde_json::Value>,
) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_periods == 0 {
        return Err(BlockError::Failed {
            message: "The results file holds no reporting periods.".into(),
        });
    }
    let edges = opt_edges(
        options,
        default_pressure_edges(is_si(network.options.flow_units)),
    )?;
    let scan = out_reader::scan_analytics(out_path, &meta)
        .map_err(|message| BlockError::Failed { message })?;
    let (pressure_unit, _, _) = unit_labels(network);
    let values = junction_min_pressures(&scan, &meta);
    Ok(threshold_fragment(
        "Pressure Thresholds",
        "Junctions",
        "Minimum pressure",
        pressure_unit,
        &values,
        &edges,
    ))
}

fn velocity_thresholds(
    out_path: &Path,
    network: &Network,
    options: Option<&serde_json::Value>,
) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_periods == 0 {
        return Err(BlockError::Failed {
            message: "The results file holds no reporting periods.".into(),
        });
    }
    let edges = opt_edges(
        options,
        default_velocity_edges(is_si(network.options.flow_units)),
    )?;
    let scan = out_reader::scan_analytics(out_path, &meta)
        .map_err(|message| BlockError::Failed { message })?;
    let (_, _, velocity_unit) = unit_labels(network);
    let values: Vec<f64> = pipe_max_velocities(&scan, network)
        .into_iter()
        .map(|(_, v)| v)
        .collect();
    Ok(threshold_fragment(
        "Velocity Thresholds",
        "Pipes",
        "Maximum velocity",
        velocity_unit,
        &values,
        &edges,
    ))
}

fn distribution_fragment(
    title: &str,
    element_label: &str,
    quantity_label: &str,
    unit: &str,
    values: &[f64],
    n_periods: usize,
) -> Fragment {
    const BIN_COUNT: usize = 6;

    let mut items = Vec::new();
    if values.is_empty() {
        items.push(FragmentItem::Note {
            text: format!(
                "No {} carry a value for this quantity.",
                element_label.to_lowercase()
            ),
        });
    } else {
        let lo = values.iter().copied().fold(f64::INFINITY, f64::min).floor();
        let hi = values
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil();
        let (lo, hi, bins) = if hi > lo {
            (lo, hi, BIN_COUNT)
        } else {
            (lo, lo + 1.0, 1)
        };
        let width = (hi - lo) / bins as f64;
        let mut counts = vec![0usize; bins];
        for &v in values {
            let index = (((v - lo) / width) as usize).min(bins - 1);
            counts[index] += 1;
        }

        let categories = (0..bins)
            .map(|i| {
                format!(
                    "{} – {}",
                    fmt_compact(lo + width * i as f64),
                    fmt_compact(lo + width * (i + 1) as f64)
                )
            })
            .collect();
        items.push(FragmentItem::Chart {
            chart: Chart {
                x_label: quantity_label.into(),
                x_unit: Some(unit.into()),
                x_quantity: None,
                y_label: element_label.into(),
                y_unit: None,
                y_quantity: None,
                data: ChartData::Bar {
                    categories,
                    values: counts.iter().map(|&c| c as f64).collect(),
                },
            },
        });
        items.push(FragmentItem::Note {
            text: format!(
                "Per-element extremes accumulated over all {n_periods} reporting periods."
            ),
        });
    }

    Fragment {
        title: title.into(),
        items,
    }
}

/// Tank hydraulic head over the reporting horizon as a line chart
/// (analysis spec §4.1.2): one series per tank in node order, capped at
/// the first [`MAX_TANK_SERIES`] with a disclosure note when more exist.
const MAX_TANK_SERIES: usize = 8;

fn tank_levels(out_path: &Path, network: &Network) -> Result<Fragment, BlockError> {
    let meta = read_meta(out_path)?;
    if meta.n_periods == 0 {
        return Err(BlockError::Failed {
            message: "The results file holds no reporting periods.".into(),
        });
    }
    let scan = out_reader::scan_analytics(out_path, &meta)
        .map_err(|message| BlockError::Failed { message })?;
    let u = SiDisplay::new(network);
    let tank_start = meta.n_nodes.saturating_sub(meta.n_tanks);

    // The scan's tank series cover tanks AND reservoirs (the prolog count
    // groups them); keep genuine tanks only.
    let tanks: Vec<(String, &Vec<f64>)> = scan
        .tank_head
        .iter()
        .enumerate()
        .filter_map(|(ti, series)| {
            let node = network.nodes.get(tank_start + ti)?;
            matches!(node.kind, crate::NodeKind::Tank(_)).then(|| (node.base.id.clone(), series))
        })
        .collect();
    if tanks.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The network has no tanks.".into(),
        });
    }

    let total = tanks.len();
    let series: Vec<LineSeries> = tanks
        .into_iter()
        .take(MAX_TANK_SERIES)
        .map(|(id, heads)| LineSeries {
            name: id,
            points: heads
                .iter()
                .enumerate()
                .map(|(p, &head)| {
                    let hours = (meta.report_start + meta.report_step * p as f64) / 3600.0;
                    [hours, u.linear("head", head)]
                })
                .collect(),
        })
        .collect();

    let mut items = vec![FragmentItem::Chart {
        chart: Chart {
            x_label: "Time".into(),
            x_unit: Some("h".into()),
            x_quantity: None,
            y_label: "Hydraulic head".into(),
            y_unit: qty("head").map(|d| d.si_label.to_string()),
            y_quantity: Some("head".into()),
            data: ChartData::Line { series },
        },
    }];
    if total > MAX_TANK_SERIES {
        items.push(FragmentItem::Note {
            text: format!("Showing the first {MAX_TANK_SERIES} of {total} tanks in node order."),
        });
    }

    Ok(Fragment {
        title: "Tank Levels".into(),
        items,
    })
}

/// Compact numeric text for narrative strings and bin labels: up to two
/// decimals, trailing zeros trimmed.
fn fmt_compact(value: f64) -> String {
    let mut s = format!("{value:.2}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

fn node_id(network: &Network, node_index: usize) -> String {
    network
        .nodes
        .get(node_index)
        .map(|n| n.base.id.clone())
        .unwrap_or_else(|| format!("node #{node_index}"))
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
        let err = produce_report_block("wds.nope", Path::new("/nonexistent"), &network, None)
            .expect_err("unknown id must fail");
        assert!(matches!(err, BlockError::UnknownBlock { .. }));
    }

    // ── option descriptions (hydra-common spec §3.2.1) ───────────────────────

    /// Same INP in US-customary units, so the descriptions can be compared
    /// across unit systems.
    const FIXTURE_INP_US: &str = "[JUNCTIONS]\nJ1  0  10\nJ2  0  10\n\n\
        [RESERVOIRS]\nR1  100\n\n\
        [PIPES]\nP1  R1  J1  1000  300  100  0  Open\nP2  J1  J2  800  250  100  0  Open\n\n\
        [OPTIONS]\nUnits  GPM\nHeadloss  H-W\n\n[END]\n";

    fn described(id: &str, inp: &str) -> Vec<OptionDescriptor> {
        let network = crate::io::parse(inp.as_bytes()).expect("parse network");
        report_block_options(id, &network)
    }

    fn by_key(descriptors: &[OptionDescriptor], key: &str) -> OptionDescriptor {
        descriptors
            .iter()
            .find(|d| d.key == key)
            .unwrap_or_else(|| panic!("option {key:?} must be described"))
            .clone()
    }

    #[test]
    fn described_defaults_follow_the_files_unit_system() {
        // The reason descriptions are resolved against a model at all: a
        // consumer must never have to know that 14 m and 20 psi are the same
        // criterion.
        let si = by_key(
            &described("wds.service-compliance", FIXTURE_INP),
            "minPressure",
        );
        assert_eq!(si.unit.as_deref(), Some("m"));
        assert!(matches!(si.kind, OptionKind::Number { default: Some(d), .. } if d == 14.0));

        let us = by_key(
            &described("wds.service-compliance", FIXTURE_INP_US),
            "minPressure",
        );
        assert_eq!(us.unit.as_deref(), Some("psi"));
        assert!(matches!(us.kind, OptionKind::Number { default: Some(d), .. } if d == 20.0));
    }

    #[test]
    fn described_edges_match_what_production_defaults_to() {
        // Guards the drift this pairing is prone to: a description that
        // advertises bands the block does not actually use.
        for (inp, si) in [(FIXTURE_INP, true), (FIXTURE_INP_US, false)] {
            let pressure = by_key(&described("wds.pressure-thresholds", inp), "edges");
            assert!(
                matches!(pressure.kind, OptionKind::NumberList { default: Some(d), .. }
                    if d == default_pressure_edges(si))
            );
            let velocity = by_key(&described("wds.velocity-thresholds", inp), "edges");
            assert!(
                matches!(velocity.kind, OptionKind::NumberList { default: Some(d), .. }
                    if d == default_velocity_edges(si))
            );
        }
    }

    #[test]
    fn described_keys_are_the_keys_production_reads() {
        // Every described key must be one the block actually consumes, or the
        // editor offers a control that does nothing.
        let expected: &[(&str, &[&str])] = &[
            (
                "wds.service-compliance",
                &["minPressure", "maxPressure", "worstCount"],
            ),
            // deficitTolerance is accepted but deliberately not described:
            // a noise floor is not a value anyone can be asked to pick.
            ("wds.demand-reliability", &["worstCount"]),
            ("wds.pipe-criticality", &["topCount"]),
            ("wds.pressure-thresholds", &["edges"]),
            ("wds.velocity-thresholds", &["edges"]),
        ];
        for (id, keys) in expected {
            let described = described(id, FIXTURE_INP);
            let actual: Vec<&str> = described.iter().map(|d| d.key.as_str()).collect();
            assert_eq!(&actual, keys, "described options for {id}");
        }
    }

    #[test]
    fn an_undescribed_option_is_still_accepted() {
        // Descriptions are advisory (hydra-common spec §3.2.1): dropping
        // `deficitTolerance` from the editor must not stop a hand-authored
        // template or the CLI from setting it.
        with_fixture_out(|path, network| {
            let options = serde_json::json!({ "deficitTolerance": 0.001 });
            produce_report_block("wds.demand-reliability", path, network, Some(&options))
                .expect("an undescribed option must still be honoured");
        });
    }

    #[test]
    fn blocks_without_options_describe_none() {
        let network = crate::io::parse(FIXTURE_INP.as_bytes()).expect("parse network");
        for id in ["wds.run-summary", "wds.tank-levels", "wds.mass-balance"] {
            assert!(
                report_block_options(id, &network).is_empty(),
                "{id} takes no options"
            );
        }
        // An unknown id is not an error — descriptions are advisory.
        assert!(report_block_options("wds.nope", &network).is_empty());
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

    /// Fixture with a tank appended (T1 behind J2), for tank-facing
    /// blocks. Junction heads stay at 50/45 m; the tank sits at 25 m.
    const TANK_FIXTURE_INP: &str = "[JUNCTIONS]\nJ1  0  10\nJ2  0  10\n\n\
        [RESERVOIRS]\nR1  100\n\n\
        [TANKS]\nT1  20  5  2  8  12  0\n\n\
        [PIPES]\nP1  R1  J1  1000  300  100  0  Open\nP2  J1  J2  800  250  100  0  Open\n\
        P3  J2  T1  600  200  100  0  Open\n\n\
        [OPTIONS]\nUnits  LPS\nHeadloss  H-W\n\n[END]\n";

    /// Persist a one-snapshot `.out` for the fixture network and run `f`
    /// with its path; the file is removed afterwards.
    fn with_fixture_out(f: impl FnOnce(&Path, &crate::Network)) {
        with_inp_out(FIXTURE_INP, f)
    }

    // ── SI display tagging (analysis spec §4.2 Units) ────────────────────

    /// The keys this module tags with must exist in the engine's §5
    /// catalog: a tag naming an uncataloged key is a producer defect the
    /// consumer silently renders as-written (hydra-common spec §3.3), so
    /// only this test would ever catch the typo.
    #[test]
    fn every_used_quantity_key_is_cataloged() {
        for key in ["pressure", "head", "demand", "flow", "velocity", "volume"] {
            assert!(qty(key).is_some(), "quantity key {key:?} is not cataloged");
        }
    }

    /// The core §3.3 obligation: a US-unit results file produces *SI*
    /// display values, tagged, wearing SI labels — and re-expressing them
    /// through the descriptor reproduces the file's value exactly, which
    /// is what makes a US reader's report read like their file.
    #[test]
    fn us_files_produce_tagged_si_values_that_round_trip() {
        with_inp_out(FIXTURE_INP_US, |path, network| {
            let fragment =
                produce_report_block("wds.result-extremes", path, network, None).expect("produce");
            let FragmentItem::Table { table } = &fragment.items[0] else {
                panic!("extremes table");
            };
            // Row 0 is pressure; columns are Quantity / Minimum / Maximum.
            let Value::Number {
                value,
                unit,
                quantity,
            } = &table.rows[0][2]
            else {
                panic!("pressure maximum is a number");
            };
            assert_eq!(quantity.as_deref(), Some("pressure"));
            assert_eq!(unit.as_deref(), Some("m"), "tagged values wear SI labels");

            // The file's psi value, recovered exactly by the descriptor —
            // the writer converted internal heads with the model-spec
            // factors, so recover the file value the same way and compare
            // round-trips rather than absolutes.
            let d = qty("pressure").expect("cataloged");
            let head_m: f64 = 50.0; // J1 head set by the harness
            let elevation = 0.0;
            let ucf = crate::io::units::make_ucf(
                network.options.flow_units,
                network.options.specific_gravity,
            );
            // The .out format stores f32, so the file's value is the
            // f32 quantization of the writer's conversion — that, exactly,
            // is what US re-display must reproduce.
            let file_psi = f64::from(((head_m - elevation) * ucf.pressure) as f32);
            assert!(
                (d.si_to_us(*value) - file_psi).abs() < 1e-9 * file_psi.abs(),
                "US re-display {} should reproduce the file's {}",
                d.si_to_us(*value),
                file_psi
            );
        });
    }

    /// On an SI file conversion is the identity: the same numbers as
    /// before v1.7, now tagged. Guards against a conversion sneaking into
    /// the SI path.
    #[test]
    fn si_files_tag_without_changing_values() {
        with_inp_out(FIXTURE_INP, |path, network| {
            let fragment =
                produce_report_block("wds.result-extremes", path, network, None).expect("produce");
            let FragmentItem::Table { table } = &fragment.items[0] else {
                panic!("extremes table");
            };
            let Value::Number {
                value, quantity, ..
            } = &table.rows[0][2]
            else {
                panic!("pressure maximum is a number");
            };
            assert_eq!(quantity.as_deref(), Some("pressure"));
            // J1: head 50, elevation 0 → 50 m of pressure, byte-identical
            // to the pre-tagging output.
            assert!((value - 50.0).abs() < 1e-9, "{value}");
        });
    }

    /// The compliance criterion is authored in file units (§4.1.1) and its
    /// echo is tagged: a 20 psi criterion must re-display as exactly
    /// 20 psi for a US reader, or the report appears to misquote its own
    /// input.
    #[test]
    fn a_us_criterion_echo_round_trips_exactly() {
        with_inp_out(FIXTURE_INP_US, |path, network| {
            let options = serde_json::json!({ "minPressure": 20.0 });
            let fragment =
                produce_report_block("wds.service-compliance", path, network, Some(&options))
                    .expect("produce");
            let FragmentItem::KeyValues { entries } = &fragment.items[0] else {
                panic!("key values");
            };
            let echo = entries
                .iter()
                .find(|e| e.label == "Minimum pressure criterion")
                .expect("criterion echo");
            let Value::Number {
                value, quantity, ..
            } = &echo.value
            else {
                panic!("criterion is a number");
            };
            assert_eq!(quantity.as_deref(), Some("pressure"));
            let d = qty("pressure").expect("cataloged");
            assert!(
                (d.si_to_us(*value) - 20.0).abs() < 1e-12,
                "echo re-displays as {}, not 20 psi",
                d.si_to_us(*value)
            );
        });
    }

    fn with_inp_out(inp: &str, f: impl FnOnce(&Path, &crate::Network)) {
        let network = crate::io::parse(inp.as_bytes()).expect("parse network");
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
        // The file carries the model's own declared units, as a real run's
        // would — the US fixtures below depend on this.
        let declared = session.network.options.flow_units;
        crate::io::out_writer::write_binary_output(&mut buf, &session, "test.inp", "", declared)
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
            let fragment = produce_report_block("wds.run-summary", path, network, None)
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
            // The fixture is 2 junctions + 1 RESERVOIR and 2 pipes: the
            // reservoir must not surface as a tank, which is exactly what the
            // `.out` prolog's combined tank group would have reported.
            assert_eq!(get("Junctions"), &int(2));
            assert_eq!(get("Reservoirs"), &int(1));
            assert_eq!(get("Tanks"), &int(0));
            assert_eq!(get("Pipes"), &int(2));
            assert_eq!(get("Pumps"), &int(0));
            assert_eq!(get("Valves"), &int(0));
            assert_eq!(get("Flow units"), &text("LPS"));
            assert_eq!(get("Pressure units"), &text("m"));
            assert_eq!(get("Quality mode"), &text("None"));
            assert_eq!(get("Reporting periods"), &int(1));
        });
    }

    #[test]
    fn result_extremes_covers_core_quantities_without_quality() {
        with_fixture_out(|path, network| {
            let fragment = produce_report_block("wds.result-extremes", path, network, None)
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
            let err = produce_report_block("wds.pump-energy", path, network, None)
                .expect_err("no pumps in fixture");
            assert_eq!(
                err,
                BlockError::Unavailable {
                    reason: "The network has no pumps.".into()
                }
            );
        });
    }

    #[test]
    fn service_compliance_defaults_are_compliant_for_healthy_fixture() {
        // Fixture junction gauge pressures are 50 m and 45 m — both above
        // the 14 m SI default criterion.
        with_fixture_out(|path, network| {
            let fragment = produce_report_block("wds.service-compliance", path, network, None)
                .expect("produce compliance");
            let FragmentItem::KeyValues { entries } = &fragment.items[0] else {
                panic!("expected key-values item");
            };
            let compliance = entries
                .iter()
                .find(|e| e.label == "Compliance")
                .expect("compliance entry");
            assert_eq!(
                compliance.value,
                Value::Number {
                    value: 100.0,
                    unit: Some("%".into()),
                    quantity: None,
                }
            );
            let FragmentItem::Note { text } = &fragment.items[1] else {
                panic!("expected narrative note");
            };
            assert!(text.contains("All analysed junctions"), "{text}");
            // Fully compliant → no worst-offenders table.
            assert_eq!(fragment.items.len(), 2);
        });
    }

    #[test]
    fn service_compliance_honors_min_pressure_option() {
        // A 48 m criterion puts J2 (45 m) in violation but not J1 (50 m).
        with_fixture_out(|path, network| {
            let options = serde_json::json!({ "minPressure": 48 });
            let fragment =
                produce_report_block("wds.service-compliance", path, network, Some(&options))
                    .expect("produce compliance");
            let FragmentItem::Note { text } = &fragment.items[1] else {
                panic!("expected narrative note");
            };
            assert!(
                text.contains("1 junction fell below the minimum pressure criterion of 48"),
                "{text}"
            );
            let FragmentItem::Table { table } = &fragment.items[2] else {
                panic!("expected worst-junctions table");
            };
            assert_eq!(table.rows.len(), 1);
            assert_eq!(table.rows[0][0], Value::Text { value: "J2".into() });
        });
    }

    #[test]
    fn malformed_options_fail_naming_the_field() {
        with_fixture_out(|path, network| {
            let options = serde_json::json!({ "minPressure": "high" });
            let err = produce_report_block("wds.service-compliance", path, network, Some(&options))
                .expect_err("string threshold must fail");
            assert!(matches!(
                &err,
                BlockError::Failed { message } if message.contains("minPressure")
            ));
        });
    }

    #[test]
    fn demand_reliability_reports_summary_and_worst_table() {
        // The fixture snapshot delivers zero demand against required base
        // demands of 5 and 8 LPS — full deficit everywhere.
        with_fixture_out(|path, network| {
            let fragment = produce_report_block("wds.demand-reliability", path, network, None)
                .expect("produce reliability");
            let FragmentItem::KeyValues { entries } = &fragment.items[0] else {
                panic!("expected key-values item");
            };
            let reliability = entries
                .iter()
                .find(|e| e.label == "Reliability")
                .expect("reliability entry");
            let Value::Number { value, .. } = reliability.value else {
                panic!("expected numeric reliability");
            };
            assert!(value < 100.0, "zero delivery cannot be fully reliable");
            assert!(
                fragment
                    .items
                    .iter()
                    .any(|i| matches!(i, FragmentItem::Table { .. })),
                "expected worst-served table"
            );
        });
    }

    #[test]
    fn pressure_distribution_bins_junction_minima_as_bar_chart() {
        with_fixture_out(|path, network| {
            let fragment = produce_report_block("wds.pressure-distribution", path, network, None)
                .expect("produce distribution");
            let FragmentItem::Chart { chart } = &fragment.items[0] else {
                panic!("expected distribution chart");
            };
            let ChartData::Bar { categories, values } = &chart.data else {
                panic!("expected bar data");
            };
            assert_eq!(categories.len(), values.len());
            // Two junctions total, spread across the bins.
            let total: f64 = values.iter().sum();
            assert!((total - 2.0).abs() < 1e-12, "bin counts must sum to 2");
            assert_eq!(chart.y_label, "Junctions");
            assert!(matches!(&fragment.items[1], FragmentItem::Note { .. }));
        });
    }

    #[test]
    fn threshold_bands_are_unbounded_at_both_ends() {
        // Spec §4.1.2: n edges give n+1 bands, outer two unbounded, so every
        // finite value is counted and the counts sum to the population.
        let edges = [0.0, 10.0, 20.0];
        let values = [-5.0, -0.1, 0.0, 9.9, 10.0, 19.9, 20.0, 1000.0];
        let counts = threshold_bands(&values, &edges);
        assert_eq!(counts, vec![2, 2, 2, 2]);
        assert_eq!(counts.iter().sum::<u64>(), values.len() as u64);
    }

    #[test]
    fn threshold_bands_place_edge_values_in_the_upper_band() {
        // Bands are half-open [e_i, e_i+1): a value exactly on an edge belongs
        // above it, so a junction at exactly 0 m is not counted as in deficit.
        let counts = threshold_bands(&[0.0], &[0.0]);
        assert_eq!(counts, vec![0, 1]);
    }

    #[test]
    fn edges_option_rejects_non_ascending_and_empty() {
        let bad = serde_json::json!({ "edges": [10.0, 5.0] });
        assert!(opt_edges(Some(&bad), &[1.0]).is_err());
        let empty = serde_json::json!({ "edges": [] });
        assert!(opt_edges(Some(&empty), &[1.0]).is_err());
        let not_numbers = serde_json::json!({ "edges": ["a"] });
        assert!(opt_edges(Some(&not_numbers), &[1.0]).is_err());
        // Absent option falls back to the supplied default.
        assert_eq!(opt_edges(None, &[1.0, 2.0]).unwrap(), vec![1.0, 2.0]);
    }

    #[test]
    fn velocity_blocks_exclude_pumps_and_valves() {
        // A pump link must not contribute a zero-velocity sample: spec §4.1.2.
        const PUMP_INP: &str = "[JUNCTIONS]\nJ1  0  10\nJ2  0  10\n\n\
            [RESERVOIRS]\nR1  100\n\n\
            [PIPES]\nP1  J1  J2  800  250  100  0  Open\n\n\
            [PUMPS]\nPU1  R1  J1  HEAD C1\n\n\
            [CURVES]\nC1  0  100\nC1  50  80\nC1  100  0\n\n\
            [OPTIONS]\nUnits  LPS\nHeadloss  H-W\n\n[END]\n";
        with_inp_out(PUMP_INP, |path, network| {
            let meta = read_meta(path).expect("meta");
            let scan = out_reader::scan_analytics(path, &meta).expect("scan");
            let pipes = pipe_max_velocities(&scan, network);
            assert_eq!(pipes.len(), 1, "only the pipe should be counted");
            let pipe_idx = pipes[0].0;
            assert!(matches!(
                network.links[pipe_idx].kind,
                crate::LinkKind::Pipe(_)
            ));
        });
    }

    #[test]
    fn mass_balance_block_reports_closure_and_a_series() {
        with_inp_out(FIXTURE_INP, |path, network| {
            let fragment =
                produce_report_block("wds.mass-balance", path, network, None).expect("block");
            assert_eq!(fragment.title, "Mass Balance");
            let has_chart = fragment
                .items
                .iter()
                .any(|i| matches!(i, FragmentItem::Chart { .. }));
            let has_kvs = fragment
                .items
                .iter()
                .any(|i| matches!(i, FragmentItem::KeyValues { .. }));
            assert!(has_chart && has_kvs);
        });
    }

    #[test]
    fn pipe_criticality_ranks_descending_and_honours_top_count() {
        with_inp_out(FIXTURE_INP, |path, network| {
            let options = serde_json::json!({ "topCount": 1 });
            let fragment =
                produce_report_block("wds.pipe-criticality", path, network, Some(&options))
                    .expect("block");
            let FragmentItem::Table { table } = &fragment.items[0] else {
                panic!("expected a table first");
            };
            assert_eq!(table.rows.len(), 1, "topCount must bound the rows");
        });
    }

    #[test]
    fn tank_levels_charts_tanks_but_not_reservoirs() {
        with_inp_out(TANK_FIXTURE_INP, |path, network| {
            let fragment = produce_report_block("wds.tank-levels", path, network, None)
                .expect("produce tank levels");
            let FragmentItem::Chart { chart } = &fragment.items[0] else {
                panic!("expected line chart");
            };
            let ChartData::Line { series } = &chart.data else {
                panic!("expected line data");
            };
            // Fixture has one tank (T1) and one reservoir (R1); only the
            // tank charts.
            assert_eq!(series.len(), 1);
            assert_eq!(series[0].name, "T1");
            assert_eq!(series[0].points.len(), 1); // one snapshot period
            assert_eq!(chart.y_unit.as_deref(), Some("m"));
        });
    }

    #[test]
    fn quality_summary_requires_a_quality_run() {
        with_fixture_out(|path, network| {
            let err = produce_report_block("wds.quality-summary", path, network, None)
                .expect_err("fixture has no quality results");
            assert_eq!(
                err,
                BlockError::Unavailable {
                    reason: "The run has no water-quality results.".into()
                }
            );
        });
    }
}
