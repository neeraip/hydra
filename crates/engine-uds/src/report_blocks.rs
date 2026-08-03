//! The engine's report blocks under the hydra-common reportable-output
//! contract (hydra-common spec §3): the catalog, block production from a
//! persisted §14.9 results file, and block-option descriptions.
//!
//! Every block here derives from the results file and the loaded network
//! alone — production is read-only and deterministic (§3.4). Values are
//! presented in the file's own unit system, labelled from its declared
//! flow units; nothing here converts. Ids follow the block-id stability
//! rule: report templates persist them, so removing or repurposing one is
//! a compatibility break.

use std::path::Path;

use hydra_common::{
    BlockDescriptor, BlockError, Column, Fragment, FragmentItem, KeyValue, OptionDescriptor,
    OptionKind, Table, Value, ValueKind,
};

use crate::io::out_reader::{read_metadata, read_period, OutMetadata};
use crate::model::Network;

const CATALOG: &[BlockDescriptor] = &[
    BlockDescriptor {
        id: "uds.run-summary",
        title: "Run Summary",
        summary: "Reporting horizon, reported element counts, and system-wide peak \
                  rates for the run.",
    },
    BlockDescriptor {
        id: "uds.subcatchment-peaks",
        title: "Subcatchment Peaks",
        summary: "Subcatchments ranked by peak runoff, with peak rainfall and \
                  infiltration rates.",
    },
    BlockDescriptor {
        id: "uds.node-extremes",
        title: "Node Extremes",
        summary: "Nodes ranked by maximum depth, with peak inflow and flooding.",
    },
    BlockDescriptor {
        id: "uds.link-extremes",
        title: "Link Extremes",
        summary: "Links ranked by peak flow, with peak velocity and capacity used.",
    },
    BlockDescriptor {
        id: "uds.flooding-summary",
        title: "Flooding Summary",
        summary: "Every node that floods: peak overflow rate, periods flooded, and \
                  first occurrence.",
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
    _network: &Network,
    options: Option<&serde_json::Value>,
) -> Result<Fragment, BlockError> {
    let meta = read_metadata(out_path).map_err(|message| BlockError::Failed { message })?;
    match id {
        "uds.run-summary" => run_summary(out_path, &meta),
        "uds.subcatchment-peaks" => subcatchment_peaks(out_path, &meta, rows(options)?),
        "uds.node-extremes" => node_extremes(out_path, &meta, rows(options)?),
        "uds.link-extremes" => link_extremes(out_path, &meta, rows(options)?),
        "uds.flooding-summary" => flooding_summary(out_path, &meta),
        _ => Err(BlockError::UnknownBlock { id: id.into() }),
    }
}

/// The options a block accepts, resolved against the model (hydra-common
/// spec §3.2.1). Advisory; unknown ids yield an empty list.
pub fn report_block_options(id: &str, _network: &Network) -> Vec<OptionDescriptor> {
    match id {
        "uds.subcatchment-peaks" | "uds.node-extremes" | "uds.link-extremes" => {
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

// ── Unit labels (the file's own system; §14.9 values are served as stored) ────

struct Units {
    flow: &'static str,
    depth: &'static str,
    rain: &'static str,
}

fn units(meta: &OutMetadata) -> Units {
    use crate::io::options::FlowUnits::*;
    let flow = match meta.flow_units {
        Cfs => "cfs",
        Gpm => "gpm",
        Mgd => "mgd",
        Cms => "m³/s",
        Lps => "L/s",
        Mld => "ML/d",
    };
    if meta.flow_units.is_us() {
        Units {
            flow,
            depth: "ft",
            rain: "in/hr",
        }
    } else {
        Units {
            flow,
            depth: "m",
            rain: "mm/hr",
        }
    }
}

fn num(value: f64, unit: Option<&str>) -> Value {
    Value::Number {
        value,
        unit: unit.map(str::to_string),
    }
}

fn col(name: &str, unit: Option<&str>, kind: ValueKind) -> Column {
    Column {
        name: name.into(),
        unit: unit.map(str::to_string),
        kind,
    }
}

/// Fold every period record through `f` — one sequential pass over the
/// file, which extremes and summaries share.
fn scan_periods(
    out_path: &Path,
    meta: &OutMetadata,
    mut f: impl FnMut(usize, &crate::io::out_reader::PeriodRecord),
) -> Result<(), BlockError> {
    for p in 0..meta.n_periods {
        let rec =
            read_period(out_path, meta, p).map_err(|message| BlockError::Failed { message })?;
        f(p, &rec);
    }
    Ok(())
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
    let u = units(meta);
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
        kv("Peak rainfall", num(peaks[0], Some(u.rain))),
        kv("Peak runoff", num(peaks[1], Some(u.flow))),
        kv("Peak lateral inflow", num(peaks[2], Some(u.flow))),
        kv("Peak flooding", num(peaks[3], Some(u.flow))),
        kv("Peak outflow", num(peaks[4], Some(u.flow))),
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
    let u = units(meta);
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
            col("Peak runoff", Some(u.flow), ValueKind::Number),
            col("Peak rainfall", Some(u.rain), ValueKind::Number),
            col("Peak infiltration", Some(u.rain), ValueKind::Number),
        ],
        rows: ranked(rows_data, 2, keep)
            .into_iter()
            .map(|(id, m)| {
                vec![
                    Value::Text { value: id },
                    num(m[2], None),
                    num(m[0], None),
                    num(m[1], None),
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
    let u = units(meta);
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
            col("Max depth", Some(u.depth), ValueKind::Number),
            col("Max head", Some(u.depth), ValueKind::Number),
            col("Peak inflow", Some(u.flow), ValueKind::Number),
            col("Peak flooding", Some(u.flow), ValueKind::Number),
        ],
        rows: ranked(rows_data, 0, keep)
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
    let u = units(meta);
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
            col("Peak flow", Some(u.flow), ValueKind::Number),
            col(
                "Peak velocity",
                Some(if meta.flow_units.is_us() {
                    "ft/s"
                } else {
                    "m/s"
                }),
                ValueKind::Number,
            ),
            col("Max capacity used", None, ValueKind::Number),
        ],
        rows: ranked(rows_data, 0, keep)
            .into_iter()
            .map(|(id, m)| {
                vec![
                    Value::Text { value: id },
                    num(m[0], None),
                    num(m[1], None),
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
    let u = units(meta);
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
                vec![a.0, a.1 as f64, (a.2.unwrap_or(0) + 1) as f64 * step_hr],
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
            col("Peak flooding", Some(u.flow), ValueKind::Number),
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
