//! Analysis (§13): the engine's report blocks under the hydra-common
//! reportable-output contract (hydra-common spec §3). The authoritative
//! specification is `spec.md` in this directory, included in the crate
//! documentation. This module holds the catalog, block production from a
//! persisted §14.9 results file, and block-option descriptions.
//!
//! Every block here derives from the results file and the loaded network
//! alone — production is read-only and deterministic (§3.4). Ids follow
//! the block-id stability rule: report templates persist them, so
//! removing or repurposing one is a compatibility break.
//!
//! Quantity-bearing numbers are **quantity-tagged** (hydra-common spec
//! §3.3, v1.7): produced in the referenced quantity's SI display unit and
//! re-expressed per display family by whichever consumer presents them.
//! The results file carries the model's declared units (§14.9), so
//! production converts — flow by the same m³/s-per-unit factor the file
//! was written under, linear quantities (depth, head, velocity, rainfall,
//! infiltration) through the engine's §5 descriptor inverse, whose US→SI
//! round trip is exact by construction. Dimensionless capacity, counts,
//! and clock values stay untagged.

use std::path::Path;

use hydra_common::{
    BlockDescriptor, BlockError, Chart, ChartData, Column, Fragment, FragmentItem, KeyValue,
    LineSeries, OptionDescriptor, OptionKind, Table, Value, ValueKind,
};

use crate::io::out_reader::{read_metadata, OutMetadata};
use crate::model::{Network, VertexKind};

const CATALOG: &[BlockDescriptor] = &[
    BlockDescriptor {
        id: "uds.run-summary",
        title: "Run Summary",
        summary: "Reporting horizon, reported element counts, and system-wide peak \
                  rates for the run.",
        category: "Summary",
    },
    BlockDescriptor {
        id: "uds.system-balance",
        title: "System Balance",
        summary: "Whole-network inflow, outflow, flooding, and storage volumes over \
                  the reporting horizon, with the inflow and outflow time series.",
        category: "Summary",
    },
    BlockDescriptor {
        id: "uds.subcatchment-peaks",
        title: "Subcatchment Peaks",
        summary: "Subcatchments ranked by peak runoff, with peak rainfall and \
                  infiltration rates.",
        category: "Hydrology",
    },
    BlockDescriptor {
        id: "uds.runoff-summary",
        title: "Runoff Summary",
        summary: "Per-subcatchment precipitation and infiltration depths, runoff \
                  volume, and runoff coefficient.",
        category: "Hydrology",
    },
    BlockDescriptor {
        id: "uds.node-extremes",
        title: "Node Extremes",
        summary: "Nodes ranked by maximum depth, with peak inflow and flooding.",
        category: "Network",
    },
    BlockDescriptor {
        id: "uds.link-extremes",
        title: "Link Extremes",
        summary: "Links ranked by peak flow, with peak velocity and capacity used.",
        category: "Network",
    },
    BlockDescriptor {
        id: "uds.flooding-summary",
        title: "Flooding Summary",
        summary: "Every node that floods: peak overflow rate, periods flooded, and \
                  first occurrence.",
        category: "Network",
    },
    BlockDescriptor {
        id: "uds.outfall-summary",
        title: "Outfall Summary",
        summary: "Per-outfall discharge frequency, mean and peak rates, and total \
                  volume discharged.",
        category: "Network",
    },
    BlockDescriptor {
        id: "uds.surcharge-summary",
        title: "Surcharge Summary",
        summary: "Nodes that come within the freeboard of their rim: maximum depth, \
                  minimum clearance, and time above the freeboard line.",
        category: "Network",
    },
    BlockDescriptor {
        id: "uds.capacity-summary",
        title: "Capacity Summary",
        summary: "Conduits that reach the capacity threshold: maximum capacity \
                  fraction and time at or above it.",
        category: "Network",
    },
    BlockDescriptor {
        id: "uds.velocity-thresholds",
        title: "Velocity Thresholds",
        summary: "Conduits counted into self-cleansing and erosive velocity bands \
                  by peak velocity.",
        category: "Network",
    },
    BlockDescriptor {
        id: "uds.storage-summary",
        title: "Storage Summary",
        summary: "Per storage node: depth utilisation, mean volume, peak inflow \
                  against peak outflow, and attenuation.",
        category: "Assets",
    },
];

/// The urban-drainage engine's report-block catalog (hydra-common spec §3.2).
pub fn report_catalog() -> &'static [BlockDescriptor] {
    CATALOG
}

/// Produce the fragment for one catalog block from a persisted results
/// file, the corresponding loaded network, and the optional per-block
/// options value (hydra-common spec §3.4).
pub fn produce_report_block(
    id: &str,
    out_path: &Path,
    network: &Network,
    options: Option<&serde_json::Value>,
) -> Result<Fragment, BlockError> {
    let meta = read_metadata(out_path).map_err(|message| BlockError::Failed { message })?;
    match id {
        "uds.run-summary" => run_summary(out_path, &meta),
        "uds.system-balance" => system_balance(out_path, &meta),
        "uds.subcatchment-peaks" => subcatchment_peaks(out_path, &meta, rows(options)?),
        "uds.runoff-summary" => runoff_summary(out_path, &meta, network, rows(options)?),
        "uds.node-extremes" => node_extremes(out_path, &meta, rows(options)?),
        "uds.link-extremes" => link_extremes(out_path, &meta, rows(options)?),
        "uds.flooding-summary" => flooding_summary(out_path, &meta),
        "uds.outfall-summary" => outfall_summary(out_path, &meta, network),
        "uds.surcharge-summary" => surcharge_summary(out_path, &meta, network, options),
        "uds.capacity-summary" => capacity_summary(out_path, &meta, network, options),
        "uds.velocity-thresholds" => velocity_thresholds(out_path, &meta, network, options),
        "uds.storage-summary" => storage_summary(out_path, &meta, network),
        _ => Err(BlockError::UnknownBlock { id: id.into() }),
    }
}

/// The options a block accepts, resolved against the model (hydra-common
/// spec §3.2.1). Advisory; unknown ids yield an empty list.
pub fn report_block_options(id: &str, _network: &Network) -> Vec<OptionDescriptor> {
    match id {
        "uds.subcatchment-peaks"
        | "uds.runoff-summary"
        | "uds.node-extremes"
        | "uds.link-extremes" => {
            vec![OptionDescriptor {
                key: "rows".into(),
                label: "Rows in the table".into(),
                help: "How many elements to list, ranked worst first.".into(),
                kind: OptionKind::Integer {
                    default: Some(10),
                    min: Some(1),
                    max: None,
                },
                unit: None,
            }]
        }
        _ => Vec::new(),
    }
}

/// One named numeric option with a default (§13.5); refuses non-numbers.
fn num_option(
    options: Option<&serde_json::Value>,
    key: &str,
    default: f64,
) -> Result<f64, BlockError> {
    match options.and_then(|o| o.get(key)) {
        None => Ok(default),
        Some(v) => match v.as_f64() {
            Some(n) if n.is_finite() => Ok(n),
            _ => Err(BlockError::Failed {
                message: format!("options.{key} must be a finite number, got {v}"),
            }),
        },
    }
}

/// The `edges` option of `uds.velocity-thresholds` (§13.5): two ascending
/// velocities in m/s, defaulting to the catalog's band defaults.
fn edges_option(options: Option<&serde_json::Value>) -> Result<[f64; 2], BlockError> {
    let Some(v) = options.and_then(|o| o.get("edges")) else {
        return Ok([0.6, 3.0]);
    };
    let malformed = || BlockError::Failed {
        message: format!("options.edges must be two ascending velocities, got {v}"),
    };
    let arr = v.as_array().ok_or_else(malformed)?;
    if arr.len() != 2 {
        return Err(malformed());
    }
    let low = arr[0]
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(malformed)?;
    let high = arr[1]
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(malformed)?;
    if high <= low {
        return Err(malformed());
    }
    Ok([low, high])
}

/// The `rows` option: how many ranked elements a table lists.
fn rows(options: Option<&serde_json::Value>) -> Result<usize, BlockError> {
    let Some(v) = options.and_then(|o| o.get("rows")) else {
        return Ok(10);
    };
    match v.as_u64() {
        Some(n) if n >= 1 => Ok(n as usize),
        _ => Err(BlockError::Failed {
            message: format!("options.rows must be a positive integer, got {v}"),
        }),
    }
}

// ── SI display conversion (module doc; hydra-common spec §3.3) ────────────────

/// This engine's §5 quantity descriptor for `key`, or `None` for a key the
/// catalog does not declare — ruled out for every key this module writes
/// by `every_used_quantity_key_is_cataloged` in the crate's block tests.
fn qty(key: &str) -> Option<&'static hydra_common::QuantityDescriptor> {
    crate::descriptors::QUANTITIES.iter().find(|q| q.key == key)
}

/// Converts values read from the results file — the model's declared
/// display units — into quantity-catalog SI display units.
struct SiDisplay {
    us: bool,
    /// m³/s per declared flow unit — the factor the file was written under.
    flow_to_m3s: f64,
}

impl SiDisplay {
    fn new(meta: &OutMetadata) -> Self {
        Self {
            us: meta.flow_units.is_us(),
            flow_to_m3s: meta.flow_units.m3s_per_unit(),
        }
    }

    /// A descriptor-covered linear quantity: depth, elevation, velocity,
    /// rainfall, infiltration. Identity on SI files.
    fn linear(&self, key: &str, file_value: f64) -> f64 {
        if self.us {
            qty(key).map_or(file_value, |d| d.us_to_si(file_value))
        } else {
            file_value
        }
    }

    /// Flow in the declared spelling → m³/s, the flow quantity's SI
    /// display unit.
    fn flow(&self, file_value: f64) -> f64 {
        file_value * self.flow_to_m3s
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

fn num(value: f64, unit: Option<&str>) -> Value {
    Value::Number {
        value,
        unit: unit.map(str::to_string),
        quantity: None,
    }
}

fn col(name: &str, unit: Option<&str>, kind: ValueKind) -> Column {
    Column {
        name: name.into(),
        unit: unit.map(str::to_string),
        kind,
        quantity: None,
    }
}

/// Fold every period record through `f` — one sequential pass over the
/// file, which extremes and summaries share.
fn scan_periods(
    out_path: &Path,
    meta: &OutMetadata,
    f: impl FnMut(usize, &crate::io::out_reader::PeriodRecord),
) -> Result<(), BlockError> {
    crate::io::out_reader::scan_periods(out_path, meta, f)
        .map_err(|message| BlockError::Failed { message })
}

/// Rank rows by a metric, worst first, ties broken by id for determinism.
fn ranked(
    mut rows: Vec<(String, Vec<f64>)>,
    metric: usize,
    keep: usize,
) -> Vec<(String, Vec<f64>)> {
    rows.sort_by(|a, b| {
        b.1[metric]
            .partial_cmp(&a.1[metric])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    rows.truncate(keep);
    rows
}

// ── Blocks ────────────────────────────────────────────────────────────────────

fn run_summary(out_path: &Path, meta: &OutMetadata) -> Result<Fragment, BlockError> {
    let u = SiDisplay::new(meta);
    // System series indices per §14.9: 1 rainfall, 4 runoff, 9 total
    // lateral inflow, 10 flooding, 11 outflow.
    let mut peaks = [0f64; 5];
    scan_periods(out_path, meta, |_, rec| {
        for (slot, sys_index) in [(0, 1usize), (1, 4), (2, 9), (3, 10), (4, 11)] {
            peaks[slot] = peaks[slot].max(rec.system[sys_index] as f64);
        }
    })?;

    let span_hours = meta.n_periods as f64 * meta.report_step_s as f64 / 3600.0;
    let entries = vec![
        kv(
            "Reporting periods",
            Value::Integer {
                value: meta.n_periods as i64,
            },
        ),
        kv("Report step", num(meta.report_step_s as f64, Some("s"))),
        kv("Reported span", num(span_hours, Some("hr"))),
        kv(
            "Subcatchments reported",
            Value::Integer {
                value: meta.subcatchment_ids.len() as i64,
            },
        ),
        kv(
            "Nodes reported",
            Value::Integer {
                value: meta.node_ids.len() as i64,
            },
        ),
        kv(
            "Links reported",
            Value::Integer {
                value: meta.link_ids.len() as i64,
            },
        ),
        kv(
            "Pollutants",
            Value::Integer {
                value: meta.pollutant_ids.len() as i64,
            },
        ),
        kv(
            "Peak rainfall",
            q_num(u.linear("rainfall", peaks[0]), "rainfall"),
        ),
        kv("Peak runoff", q_num(u.flow(peaks[1]), "flow")),
        kv("Peak lateral inflow", q_num(u.flow(peaks[2]), "flow")),
        kv("Peak flooding", q_num(u.flow(peaks[3]), "flow")),
        kv("Peak outflow", q_num(u.flow(peaks[4]), "flow")),
    ];
    Ok(Fragment {
        title: "Run Summary".into(),
        items: vec![FragmentItem::KeyValues { entries }],
    })
}

fn kv(label: &str, value: Value) -> KeyValue {
    KeyValue {
        label: label.into(),
        value,
    }
}

fn subcatchment_peaks(
    out_path: &Path,
    meta: &OutMetadata,
    keep: usize,
) -> Result<Fragment, BlockError> {
    if meta.subcatchment_ids.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The run reports no subcatchments.".into(),
        });
    }
    let u = SiDisplay::new(meta);
    let n = meta.subcatchment_ids.len();
    let nv = meta.n_subcatch_vars;
    // Subcatchment variables per §14.9: 0 rainfall, 3 infiltration, 4 runoff.
    let mut maxima = vec![[0f64; 3]; n];
    scan_periods(out_path, meta, |_, rec| {
        for (i, m) in maxima.iter_mut().enumerate() {
            m[0] = m[0].max(rec.subcatchments[i * nv] as f64);
            m[1] = m[1].max(rec.subcatchments[i * nv + 3] as f64);
            m[2] = m[2].max(rec.subcatchments[i * nv + 4] as f64);
        }
    })?;

    let rows_data: Vec<(String, Vec<f64>)> = meta
        .subcatchment_ids
        .iter()
        .zip(&maxima)
        .map(|(id, m)| (id.clone(), m.to_vec()))
        .collect();
    let table = Table {
        columns: vec![
            col("Subcatchment", None, ValueKind::Text),
            q_col("Peak runoff", "flow"),
            q_col("Peak rainfall", "rainfall"),
            q_col("Peak infiltration", "infiltration"),
        ],
        rows: ranked(rows_data, 2, keep)
            .into_iter()
            .map(|(id, m)| {
                vec![
                    Value::Text { value: id },
                    num(u.flow(m[2]), None),
                    num(u.linear("rainfall", m[0]), None),
                    num(u.linear("infiltration", m[1]), None),
                ]
            })
            .collect(),
    };
    Ok(Fragment {
        title: "Subcatchment Peaks".into(),
        items: vec![FragmentItem::Table { table }],
    })
}

fn node_extremes(out_path: &Path, meta: &OutMetadata, keep: usize) -> Result<Fragment, BlockError> {
    if meta.node_ids.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The run reports no nodes.".into(),
        });
    }
    let u = SiDisplay::new(meta);
    let n = meta.node_ids.len();
    let nv = meta.n_node_vars;
    // Node variables per §14.9: 0 depth, 1 head, 4 total inflow, 5 flooding.
    let mut maxima = vec![[0f64; 4]; n];
    scan_periods(out_path, meta, |_, rec| {
        for (i, m) in maxima.iter_mut().enumerate() {
            m[0] = m[0].max(rec.nodes[i * nv] as f64);
            m[1] = m[1].max(rec.nodes[i * nv + 1] as f64);
            m[2] = m[2].max(rec.nodes[i * nv + 4] as f64);
            m[3] = m[3].max(rec.nodes[i * nv + 5] as f64);
        }
    })?;

    let rows_data: Vec<(String, Vec<f64>)> = meta
        .node_ids
        .iter()
        .zip(&maxima)
        .map(|(id, m)| (id.clone(), m.to_vec()))
        .collect();
    let table = Table {
        columns: vec![
            col("Node", None, ValueKind::Text),
            q_col("Max depth", "depth"),
            q_col("Max head", "elevation"),
            q_col("Peak inflow", "flow"),
            q_col("Peak flooding", "flow"),
        ],
        rows: ranked(rows_data, 0, keep)
            .into_iter()
            .map(|(id, m)| {
                vec![
                    Value::Text { value: id },
                    num(u.linear("depth", m[0]), None),
                    num(u.linear("elevation", m[1]), None),
                    num(u.flow(m[2]), None),
                    num(u.flow(m[3]), None),
                ]
            })
            .collect(),
    };
    Ok(Fragment {
        title: "Node Extremes".into(),
        items: vec![FragmentItem::Table { table }],
    })
}

fn link_extremes(out_path: &Path, meta: &OutMetadata, keep: usize) -> Result<Fragment, BlockError> {
    if meta.link_ids.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The run reports no links.".into(),
        });
    }
    let u = SiDisplay::new(meta);
    let n = meta.link_ids.len();
    let nv = meta.n_link_vars;
    // Link variables per §14.9: 0 flow, 2 velocity, 4 capacity.
    let mut maxima = vec![[0f64; 3]; n];
    scan_periods(out_path, meta, |_, rec| {
        for (i, m) in maxima.iter_mut().enumerate() {
            m[0] = m[0].max((rec.links[i * nv] as f64).abs());
            m[1] = m[1].max(rec.links[i * nv + 2] as f64);
            m[2] = m[2].max(rec.links[i * nv + 4] as f64);
        }
    })?;

    let rows_data: Vec<(String, Vec<f64>)> = meta
        .link_ids
        .iter()
        .zip(&maxima)
        .map(|(id, m)| (id.clone(), m.to_vec()))
        .collect();
    let table = Table {
        columns: vec![
            col("Link", None, ValueKind::Text),
            q_col("Peak flow", "flow"),
            q_col("Peak velocity", "velocity"),
            col("Max capacity used", None, ValueKind::Number),
        ],
        rows: ranked(rows_data, 0, keep)
            .into_iter()
            .map(|(id, m)| {
                vec![
                    Value::Text { value: id },
                    num(u.flow(m[0]), None),
                    num(u.linear("velocity", m[1]), None),
                    num(m[2], None),
                ]
            })
            .collect(),
    };
    Ok(Fragment {
        title: "Link Extremes".into(),
        items: vec![FragmentItem::Table { table }],
    })
}

fn flooding_summary(out_path: &Path, meta: &OutMetadata) -> Result<Fragment, BlockError> {
    if meta.node_ids.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The run reports no nodes.".into(),
        });
    }
    let u = SiDisplay::new(meta);
    let n = meta.node_ids.len();
    let nv = meta.n_node_vars;
    // (peak flooding, periods flooded, first flooded period)
    let mut acc: Vec<(f64, usize, Option<usize>)> = vec![(0.0, 0, None); n];
    scan_periods(out_path, meta, |p, rec| {
        for (i, a) in acc.iter_mut().enumerate() {
            let flooding = rec.nodes[i * nv + 5] as f64;
            if flooding > 0.0 {
                a.0 = a.0.max(flooding);
                a.1 += 1;
                a.2.get_or_insert(p);
            }
        }
    })?;

    let step_hr = meta.report_step_s as f64 / 3600.0;
    let mut flooded: Vec<(String, Vec<f64>)> = meta
        .node_ids
        .iter()
        .zip(&acc)
        .filter(|(_, a)| a.1 > 0)
        .map(|(id, a)| {
            (
                id.clone(),
                // Peak flooding converts to m³/s here: its column is
                // quantity-tagged, and a raw file-unit value under a
                // tagged column would render as the wrong number.
                vec![
                    u.flow(a.0),
                    a.1 as f64,
                    (a.2.unwrap_or(0) + 1) as f64 * step_hr,
                ],
            )
        })
        .collect();
    if flooded.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "No node floods in this run.".into(),
        });
    }
    let keep = flooded.len();
    flooded = ranked(flooded, 0, keep);

    let table = Table {
        columns: vec![
            col("Node", None, ValueKind::Text),
            q_col("Peak flooding", "flow"),
            col("Periods flooded", None, ValueKind::Integer),
            col("First flooded", Some("hr"), ValueKind::Number),
        ],
        rows: flooded
            .into_iter()
            .map(|(id, m)| {
                vec![
                    Value::Text { value: id },
                    num(m[0], None),
                    Value::Integer { value: m[1] as i64 },
                    num(m[2], None),
                ]
            })
            .collect(),
    };
    Ok(Fragment {
        title: "Flooding Summary".into(),
        items: vec![FragmentItem::Table { table }],
    })
}

fn system_balance(out_path: &Path, meta: &OutMetadata) -> Result<Fragment, BlockError> {
    if meta.n_periods == 0 {
        return Err(BlockError::Unavailable {
            reason: "The results file stores no periods.".into(),
        });
    }
    let u = SiDisplay::new(meta);
    let dt_s = f64::from(meta.report_step_s);
    // System series per §14.9: 4 runoff, 5 dry-weather, 6 groundwater,
    // 7 RDII, 8 external, 9 total lateral inflow, 10 flooding, 11 outflow,
    // 12 storage volume.
    const COMPONENTS: [(usize, &str); 5] = [
        (4, "Runoff"),
        (5, "Dry-weather inflow"),
        (6, "Groundwater inflow"),
        (7, "RDII inflow"),
        (8, "External inflow"),
    ];
    let mut component_sums = [0f64; 5];
    let mut flood_sum = 0f64;
    let mut outflow_sum = 0f64;
    let mut storage = (0f64, 0f64);
    let mut inflow_series: Vec<[f64; 2]> = Vec::with_capacity(meta.n_periods);
    let mut outflow_series: Vec<[f64; 2]> = Vec::with_capacity(meta.n_periods);
    let mut flooding_series: Vec<[f64; 2]> = Vec::with_capacity(meta.n_periods);
    scan_periods(out_path, meta, |p, rec| {
        for (slot, (index, _)) in COMPONENTS.iter().enumerate() {
            component_sums[slot] += f64::from(rec.system[*index]);
        }
        flood_sum += f64::from(rec.system[10]);
        outflow_sum += f64::from(rec.system[11]);
        let stored = f64::from(rec.system[12]);
        if p == 0 {
            storage.0 = stored;
        }
        storage.1 = stored;
        // Elapsed hours at the end of the period, matching the flooding
        // summary's clock.
        let t_hr = (p + 1) as f64 * dt_s / 3600.0;
        inflow_series.push([t_hr, u.flow(f64::from(rec.system[9]))]);
        outflow_series.push([t_hr, u.flow(f64::from(rec.system[11]))]);
        flooding_series.push([t_hr, u.flow(f64::from(rec.system[10]))]);
    })?;

    // §13.3: rate sums integrate to volumes at the report step; conversion
    // to m³ happens on the integrated total.
    let vol = |rate_sum: f64| u.flow(rate_sum) * dt_s;
    let total_in = vol(component_sums.iter().sum());
    let total_out = vol(outflow_sum);
    let total_flood = vol(flood_sum);
    let storage_change = u.linear("volume", storage.1) - u.linear("volume", storage.0);
    let residual = total_in - total_out - total_flood - storage_change;

    let mut entries = Vec::new();
    for (slot, (_, label)) in COMPONENTS.iter().enumerate() {
        // §13.4.2: a component that never flows is omitted rather than
        // listed as a zero row.
        if component_sums[slot] > 0.0 {
            entries.push(kv(label, q_num(vol(component_sums[slot]), "volume")));
        }
    }
    entries.push(kv("Total inflow", q_num(total_in, "volume")));
    entries.push(kv("Outflow", q_num(total_out, "volume")));
    entries.push(kv("Flooding", q_num(total_flood, "volume")));
    entries.push(kv("Storage change", q_num(storage_change, "volume")));
    entries.push(kv("Residual", q_num(residual, "volume")));

    let mut series = vec![
        LineSeries {
            name: "Inflow".into(),
            points: inflow_series,
        },
        LineSeries {
            name: "Outflow".into(),
            points: outflow_series,
        },
    ];
    if flood_sum > 0.0 {
        series.push(LineSeries {
            name: "Flooding".into(),
            points: flooding_series,
        });
    }
    let chart = Chart {
        x_label: "Elapsed".into(),
        x_unit: Some("hr".into()),
        x_quantity: None,
        y_label: "Flow".into(),
        y_unit: qty("flow").map(|d| d.si_label.to_string()),
        y_quantity: Some("flow".into()),
        data: ChartData::Line { series },
    };
    Ok(Fragment {
        title: "System Balance".into(),
        items: vec![
            FragmentItem::KeyValues { entries },
            FragmentItem::Chart { chart },
        ],
    })
}

fn runoff_summary(
    out_path: &Path,
    meta: &OutMetadata,
    network: &Network,
    keep: usize,
) -> Result<Fragment, BlockError> {
    if meta.subcatchment_ids.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The run reports no subcatchments.".into(),
        });
    }
    let u = SiDisplay::new(meta);
    let n = meta.subcatchment_ids.len();
    let nv = meta.n_subcatch_vars;
    // Intensity sums (rain, infiltration) and a runoff rate sum, per §13.3.
    let mut sums = vec![[0f64; 3]; n];
    scan_periods(out_path, meta, |_, rec| {
        for (i, s) in sums.iter_mut().enumerate() {
            s[0] += f64::from(rec.subcatchments[i * nv]);
            s[1] += f64::from(rec.subcatchments[i * nv + 3]);
            s[2] += f64::from(rec.subcatchments[i * nv + 4]);
        }
    })?;

    // Intensities are per hour (§14.9), so depth = Σ rate · Δt(h).
    let dt_h = f64::from(meta.report_step_s) / 3600.0;
    let dt_s = f64::from(meta.report_step_s);
    let area_by_id: std::collections::HashMap<&str, f64> = network
        .parcels
        .iter()
        .map(|p| (p.id.as_str(), p.area))
        .collect();

    let rows_data: Vec<(String, Vec<f64>)> = meta
        .subcatchment_ids
        .iter()
        .zip(&sums)
        .map(|(id, s)| {
            let precip_mm = u.linear("precipitation", s[0] * dt_h);
            let infil_mm = u.linear("precipitation", s[1] * dt_h);
            let runoff_m3 = u.flow(s[2]) * dt_s;
            // §13.4.3: C = V / (d · A), absent without precipitation or a
            // model subcatchment to take the area from (encoded NaN here,
            // rendered absent below).
            let coefficient = match area_by_id.get(id.as_str()) {
                Some(&area_m2) if precip_mm > 0.0 && area_m2 > 0.0 => {
                    runoff_m3 / (precip_mm / 1000.0 * area_m2)
                }
                _ => f64::NAN,
            };
            (
                id.clone(),
                vec![precip_mm, infil_mm, runoff_m3, coefficient],
            )
        })
        .collect();

    let table = Table {
        columns: vec![
            col("Subcatchment", None, ValueKind::Text),
            q_col("Precipitation", "precipitation"),
            q_col("Infiltration", "precipitation"),
            q_col("Runoff volume", "volume"),
            col("Runoff coefficient", None, ValueKind::Number),
        ],
        rows: ranked(rows_data, 2, keep)
            .into_iter()
            .map(|(id, s)| {
                vec![
                    Value::Text { value: id },
                    num(s[0], None),
                    num(s[1], None),
                    num(s[2], None),
                    if s[3].is_nan() {
                        Value::Absent
                    } else {
                        num(s[3], None)
                    },
                ]
            })
            .collect(),
    };
    Ok(Fragment {
        title: "Runoff Summary".into(),
        items: vec![FragmentItem::Table { table }],
    })
}

fn outfall_summary(
    out_path: &Path,
    meta: &OutMetadata,
    network: &Network,
) -> Result<Fragment, BlockError> {
    // §13.4.6: outfalls are identified from the model, membership in the
    // results file from the report selection.
    let outfall_ids: std::collections::HashSet<&str> = network
        .vertices
        .iter()
        .filter(|v| matches!(v.kind, VertexKind::Outfall { .. }))
        .map(|v| v.id.as_str())
        .collect();
    let reported: Vec<usize> = meta
        .node_ids
        .iter()
        .enumerate()
        .filter(|(_, id)| outfall_ids.contains(id.as_str()))
        .map(|(i, _)| i)
        .collect();
    if reported.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The run reports no outfalls.".into(),
        });
    }

    let u = SiDisplay::new(meta);
    let nv = meta.n_node_vars;
    // (rate sum, peak rate, discharging periods) per reported outfall,
    // judged on the node's total inflow series (§14.9 node variable 4).
    let mut acc = vec![(0f64, 0f64, 0usize); reported.len()];
    scan_periods(out_path, meta, |_, rec| {
        for (slot, &i) in reported.iter().enumerate() {
            let q = f64::from(rec.nodes[i * nv + 4]);
            if q > 0.0 {
                let a = &mut acc[slot];
                a.0 += q;
                a.1 = a.1.max(q);
                a.2 += 1;
            }
        }
    })?;

    let dt_s = f64::from(meta.report_step_s);
    let rows_data: Vec<(String, Vec<f64>)> = reported
        .iter()
        .zip(&acc)
        .map(|(&i, a)| {
            let frequency = 100.0 * a.2 as f64 / meta.n_periods as f64;
            let mean = if a.2 > 0 {
                u.flow(a.0 / a.2 as f64)
            } else {
                0.0
            };
            (
                meta.node_ids[i].clone(),
                vec![frequency, mean, u.flow(a.1), u.flow(a.0) * dt_s],
            )
        })
        .collect();

    let table = Table {
        columns: vec![
            col("Outfall", None, ValueKind::Text),
            q_col("Flow frequency", "percent"),
            q_col("Mean discharge", "flow"),
            q_col("Peak discharge", "flow"),
            q_col("Total volume", "volume"),
        ],
        rows: {
            let keep = rows_data.len();
            ranked(rows_data, 3, keep)
                .into_iter()
                .map(|(id, m)| {
                    vec![
                        Value::Text { value: id },
                        num(m[0], None),
                        num(m[1], None),
                        num(m[2], None),
                        num(m[3], None),
                    ]
                })
                .collect()
        },
    };
    Ok(Fragment {
        title: "Outfall Summary".into(),
        items: vec![FragmentItem::Table { table }],
    })
}

/// A vertex's rim depth (m): a junction's or storage's maximum depth,
/// `None` for kinds without one or a rim the model left at zero.
fn rim_depth(kind: &VertexKind) -> Option<f64> {
    match kind {
        VertexKind::Junction { max_depth, .. } | VertexKind::Storage { max_depth, .. } => {
            (*max_depth > 0.0).then_some(*max_depth)
        }
        _ => None,
    }
}

fn surcharge_summary(
    out_path: &Path,
    meta: &OutMetadata,
    network: &Network,
    options: Option<&serde_json::Value>,
) -> Result<Fragment, BlockError> {
    let freeboard = num_option(options, "freeboard", 0.3)?;
    // §13.4.7: candidates are reported vertices with a positive model rim.
    let rims: std::collections::HashMap<&str, f64> = network
        .vertices
        .iter()
        .filter_map(|v| rim_depth(&v.kind).map(|d| (v.id.as_str(), d)))
        .collect();
    let candidates: Vec<(usize, f64)> = meta
        .node_ids
        .iter()
        .enumerate()
        .filter_map(|(i, id)| rims.get(id.as_str()).map(|&rim| (i, rim)))
        .collect();
    if candidates.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The run reports no depth-limited nodes.".into(),
        });
    }

    let u = SiDisplay::new(meta);
    let nv = meta.n_node_vars;
    // (max depth m, periods above the freeboard line) per candidate.
    let mut acc = vec![(0f64, 0usize); candidates.len()];
    scan_periods(out_path, meta, |_, rec| {
        for (slot, (i, rim)) in candidates.iter().enumerate() {
            let depth = u.linear("depth", f64::from(rec.nodes[i * nv]));
            let a = &mut acc[slot];
            a.0 = a.0.max(depth);
            if depth > rim - freeboard {
                a.1 += 1;
            }
        }
    })?;

    let step_hr = f64::from(meta.report_step_s) / 3600.0;
    let rows_data: Vec<(String, Vec<f64>)> = candidates
        .iter()
        .zip(&acc)
        .filter(|(_, a)| a.1 > 0)
        .map(|((i, rim), a)| {
            (
                meta.node_ids[*i].clone(),
                vec![*rim, a.0, rim - a.0, a.1 as f64 * step_hr],
            )
        })
        .collect();
    if rows_data.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "No node comes within the freeboard of its rim.".into(),
        });
    }

    let table = Table {
        columns: vec![
            col("Node", None, ValueKind::Text),
            q_col("Rim depth", "depth"),
            q_col("Max depth", "depth"),
            q_col("Min clearance", "depth"),
            col("Hours above freeboard", Some("hr"), ValueKind::Number),
        ],
        rows: {
            let keep = rows_data.len();
            ranked(rows_data, 3, keep)
                .into_iter()
                .map(|(id, m)| {
                    vec![
                        Value::Text { value: id },
                        num(m[0], None),
                        num(m[1], None),
                        num(m[2], None),
                        num(m[3], None),
                    ]
                })
                .collect()
        },
    };
    Ok(Fragment {
        title: "Surcharge Summary".into(),
        items: vec![FragmentItem::Table { table }],
    })
}

/// Reported link indices that are conduits in the model (§13.4.8: other
/// link kinds have no meaningful capacity fraction).
fn reported_conduits(meta: &OutMetadata, network: &Network) -> Vec<usize> {
    let conduits: std::collections::HashSet<&str> = network
        .links
        .iter()
        .filter(|l| matches!(l.kind, crate::model::LinkKind::Channel { .. }))
        .map(|l| l.id.as_str())
        .collect();
    meta.link_ids
        .iter()
        .enumerate()
        .filter(|(_, id)| conduits.contains(id.as_str()))
        .map(|(i, _)| i)
        .collect()
}

fn capacity_summary(
    out_path: &Path,
    meta: &OutMetadata,
    network: &Network,
    options: Option<&serde_json::Value>,
) -> Result<Fragment, BlockError> {
    let threshold = num_option(options, "threshold", 0.8)?;
    let conduits = reported_conduits(meta, network);
    if conduits.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The run reports no conduits.".into(),
        });
    }

    let nv = meta.n_link_vars;
    // (max capacity fraction, periods at or above threshold) per conduit.
    let mut acc = vec![(0f64, 0usize); conduits.len()];
    scan_periods(out_path, meta, |_, rec| {
        for (slot, &i) in conduits.iter().enumerate() {
            let capacity = f64::from(rec.links[i * nv + 4]);
            let a = &mut acc[slot];
            a.0 = a.0.max(capacity);
            if capacity >= threshold {
                a.1 += 1;
            }
        }
    })?;

    let step_hr = f64::from(meta.report_step_s) / 3600.0;
    let rows_data: Vec<(String, Vec<f64>)> = conduits
        .iter()
        .zip(&acc)
        .filter(|(_, a)| a.1 > 0)
        .map(|(&i, a)| (meta.link_ids[i].clone(), vec![a.0, a.1 as f64 * step_hr]))
        .collect();
    if rows_data.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "No conduit reaches the capacity threshold.".into(),
        });
    }

    let table = Table {
        columns: vec![
            col("Conduit", None, ValueKind::Text),
            col("Max capacity used", None, ValueKind::Number),
            col("Hours at capacity", Some("hr"), ValueKind::Number),
        ],
        rows: {
            let keep = rows_data.len();
            ranked(rows_data, 1, keep)
                .into_iter()
                .map(|(id, m)| vec![Value::Text { value: id }, num(m[0], None), num(m[1], None)])
                .collect()
        },
    };
    Ok(Fragment {
        title: "Capacity Summary".into(),
        items: vec![FragmentItem::Table { table }],
    })
}

fn velocity_thresholds(
    out_path: &Path,
    meta: &OutMetadata,
    network: &Network,
    options: Option<&serde_json::Value>,
) -> Result<Fragment, BlockError> {
    let [low, high] = edges_option(options)?;
    let conduits = reported_conduits(meta, network);
    if conduits.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The run reports no conduits.".into(),
        });
    }

    let u = SiDisplay::new(meta);
    let nv = meta.n_link_vars;
    let mut peaks = vec![0f64; conduits.len()];
    scan_periods(out_path, meta, |_, rec| {
        for (slot, &i) in conduits.iter().enumerate() {
            let v = u.linear("velocity", f64::from(rec.links[i * nv + 2]).abs());
            peaks[slot] = peaks[slot].max(v);
        }
    })?;

    let mut counts = [0f64; 3];
    for &peak in &peaks {
        let band = if peak < low {
            0
        } else if peak <= high {
            1
        } else {
            2
        };
        counts[band] += 1.0;
    }

    Ok(Fragment {
        title: "Velocity Thresholds".into(),
        items: vec![
            // The edges as tagged values, so they re-express with the
            // reader's display family; the band labels stay numberless
            // for the same reason.
            FragmentItem::KeyValues {
                entries: vec![
                    kv("Self-cleansing velocity", q_num(low, "velocity")),
                    kv("Erosive velocity", q_num(high, "velocity")),
                ],
            },
            FragmentItem::Chart {
                chart: Chart {
                    x_label: "Band".into(),
                    x_unit: None,
                    x_quantity: None,
                    y_label: "Conduits".into(),
                    y_unit: None,
                    y_quantity: None,
                    data: ChartData::Bar {
                        categories: vec![
                            "Below self-cleansing".into(),
                            "Self-cleansing range".into(),
                            "Above erosive".into(),
                        ],
                        values: counts.to_vec(),
                    },
                },
            },
        ],
    })
}

fn storage_summary(
    out_path: &Path,
    meta: &OutMetadata,
    network: &Network,
) -> Result<Fragment, BlockError> {
    // §13.4.10: reported storage vertices, with the model's full depth and
    // the incident links that carry water away.
    struct Candidate {
        node: usize,
        full_depth: f64,
        /// (reported link index, +1 when oriented out of the vertex).
        links: Vec<(usize, f64)>,
    }
    let vertex_index_by_id: std::collections::HashMap<&str, usize> = network
        .vertices
        .iter()
        .enumerate()
        .map(|(vi, v)| (v.id.as_str(), vi))
        .collect();
    let link_slot_by_id: std::collections::HashMap<&str, usize> = meta
        .link_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let candidates: Vec<Candidate> = meta
        .node_ids
        .iter()
        .enumerate()
        .filter_map(|(node, id)| {
            let &vi = vertex_index_by_id.get(id.as_str())?;
            let VertexKind::Storage { max_depth, .. } = network.vertices[vi].kind else {
                return None;
            };
            let links = network
                .links
                .iter()
                .filter_map(|l| {
                    let sign = if l.from == vi {
                        1.0
                    } else if l.to == vi {
                        -1.0
                    } else {
                        return None;
                    };
                    link_slot_by_id.get(l.id.as_str()).map(|&slot| (slot, sign))
                })
                .collect();
            Some(Candidate {
                node,
                full_depth: max_depth,
                links,
            })
        })
        .collect();
    if candidates.is_empty() {
        return Err(BlockError::Unavailable {
            reason: "The run reports no storage nodes.".into(),
        });
    }

    let u = SiDisplay::new(meta);
    let nnv = meta.n_node_vars;
    let nlv = meta.n_link_vars;
    // (max depth, volume sum, peak inflow, peak outflow) per candidate.
    let mut acc = vec![(0f64, 0f64, 0f64, 0f64); candidates.len()];
    scan_periods(out_path, meta, |_, rec| {
        for (slot, c) in candidates.iter().enumerate() {
            let a = &mut acc[slot];
            a.0 = a.0.max(f64::from(rec.nodes[c.node * nnv]));
            a.1 += f64::from(rec.nodes[c.node * nnv + 2]);
            a.2 = a.2.max(f64::from(rec.nodes[c.node * nnv + 4]));
            // §13.4.10: a period's outflow is water leaving through
            // incident links — flow away on links oriented out of the
            // vertex plus reverse flow on links oriented into it.
            let out: f64 = c
                .links
                .iter()
                .map(|&(li, sign)| (sign * f64::from(rec.links[li * nlv])).max(0.0))
                .sum();
            a.3 = a.3.max(out);
        }
    })?;

    let rows_data: Vec<(String, Vec<f64>)> = candidates
        .iter()
        .zip(&acc)
        .map(|(c, a)| {
            let max_depth = u.linear("depth", a.0);
            let used = if c.full_depth > 0.0 {
                100.0 * max_depth / c.full_depth
            } else {
                f64::NAN
            };
            let peak_in = u.flow(a.2);
            let peak_out = u.flow(a.3);
            let attenuation = if peak_in > 0.0 {
                100.0 * (1.0 - peak_out / peak_in)
            } else {
                f64::NAN
            };
            (
                meta.node_ids[c.node].clone(),
                vec![
                    max_depth,
                    used,
                    u.linear("volume", a.1) / meta.n_periods.max(1) as f64,
                    peak_in,
                    peak_out,
                    attenuation,
                ],
            )
        })
        .collect();

    let cell = |v: f64| {
        if v.is_nan() {
            Value::Absent
        } else {
            num(v, None)
        }
    };
    let table = Table {
        columns: vec![
            col("Storage", None, ValueKind::Text),
            q_col("Max depth", "depth"),
            q_col("Depth used", "percent"),
            q_col("Mean volume", "volume"),
            q_col("Peak inflow", "flow"),
            q_col("Peak outflow", "flow"),
            q_col("Attenuation", "percent"),
        ],
        rows: {
            let keep = rows_data.len();
            ranked(rows_data, 3, keep)
                .into_iter()
                .map(|(id, m)| {
                    let mut row = vec![Value::Text { value: id }];
                    row.extend(m.into_iter().map(cell));
                    row
                })
                .collect()
        },
    };
    Ok(Fragment {
        title: "Storage Summary".into(),
        items: vec![FragmentItem::Table { table }],
    })
}

// ── Criteria (§13.6; hydra-common spec §7) ────────────────────────────────────

/// The assessment standard (§13.6). Defaults are SI display units of each
/// criterion's quantity.
const CRITERIA: &[hydra_common::CriterionDescriptor] = &[
    hydra_common::CriterionDescriptor {
        key: "freeboard",
        label: "Freeboard",
        help: "The clearance kept below a node's rim; the surcharge figures \
               judge against it.",
        quantity: Some("depth"),
        kind: hydra_common::CriterionKind::Value { default: 0.3 },
    },
    hydra_common::CriterionDescriptor {
        key: "capacity",
        label: "Capacity threshold",
        help: "The fraction of conduit capacity treated as full for the \
               capacity figures.",
        quantity: Some("percent"),
        kind: hydra_common::CriterionKind::Value { default: 80.0 },
    },
    hydra_common::CriterionDescriptor {
        key: "velocity",
        label: "Velocity",
        help: "The self-cleansing and erosive velocities conduits are \
               judged between.",
        quantity: Some("velocity"),
        kind: hydra_common::CriterionKind::Band {
            cuts: &[
                hydra_common::BandCut {
                    key: "selfCleansing",
                    label: "Self-cleansing",
                    default: 0.6,
                },
                hydra_common::BandCut {
                    key: "erosive",
                    label: "Erosive",
                    default: 3.0,
                },
            ],
        },
    },
];

/// The engine's criteria catalog (hydra-common spec §7.2).
pub fn criteria_catalog() -> &'static [hydra_common::CriterionDescriptor] {
    CRITERIA
}

/// Per-block options from a valuation (§13.6; hydra-common spec §7.4).
/// Options are already SI (§13.5), so nothing converts; a degenerate
/// velocity band omits its block.
pub fn criteria_block_options(
    valuation: &serde_json::Value,
    _network: &Network,
) -> Result<std::collections::HashMap<&'static str, serde_json::Value>, String> {
    let value_of = |key: &str, default: f64| -> Result<f64, String> {
        match valuation.get(key) {
            None => Ok(default),
            Some(v) => match v.as_f64() {
                Some(n) if n.is_finite() => Ok(n),
                _ => Err(format!(
                    "criterion {key:?} must be a finite number, got {v}"
                )),
            },
        }
    };

    let mut options = std::collections::HashMap::new();
    options.insert(
        "uds.surcharge-summary",
        serde_json::json!({ "freeboard": value_of("freeboard", 0.3)? }),
    );
    options.insert(
        "uds.capacity-summary",
        serde_json::json!({ "threshold": value_of("capacity", 80.0)? / 100.0 }),
    );

    let band =
        match valuation.get("velocity") {
            None => Some([0.6, 3.0]),
            Some(v) => {
                let arr = v
                    .as_array()
                    .ok_or_else(|| format!("criterion \"velocity\" must be a list, got {v}"))?;
                if arr.len() != 2 {
                    return Err(format!(
                        "criterion \"velocity\" must supply 2 values, got {}",
                        arr.len()
                    ));
                }
                let low = arr[0].as_f64().filter(|n| n.is_finite()).ok_or_else(|| {
                    format!("criterion \"velocity\" holds a non-number: {}", arr[0])
                })?;
                let high = arr[1].as_f64().filter(|n| n.is_finite()).ok_or_else(|| {
                    format!("criterion \"velocity\" holds a non-number: {}", arr[1])
                })?;
                // Degenerate, not malformed (hydra-common §7.3): the block
                // runs on its documented defaults.
                (high > low).then_some([low, high])
            }
        };
    if let Some([low, high]) = band {
        options.insert(
            "uds.velocity-thresholds",
            serde_json::json!({ "edges": [low, high] }),
        );
    }
    Ok(options)
}
