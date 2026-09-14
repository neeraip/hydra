//! Urban-drainage results provider: the engine-described variable catalog
//! with per-run ranges served through `load_result_meta`, and the generic
//! per-period payload the canvas colors elements with.
//!
//! "Engines describe, applications render": every variable id, label,
//! quantity, and ramp hint here comes from the engine's §6 catalog
//! (`hydra::uds::descriptors::result_variables_for`, which is the fixed
//! catalog for the class plus one concentration series per pollutant the
//! model declares) — nothing is invented in the GUI. Values are served in **SI** regardless of the file's declared
//! unit system, each variable carrying its §5 quantity descriptor, so the
//! frontend converts to the user's display-unit preference at the render
//! boundary — the same discipline as wds results and the uds attribute
//! rows. Link capacity is the one shape change: the file stores a 0–1
//! fraction and the catalog declares a percent, so it is scaled ×100.
//!
//! # Generic period payload layout
//!
//! Little-endian, mirroring the snapshot-v4 discipline (`uds_view`): the
//! element order of every array is the **snapshot order** — the same
//! points/polylines/regions order `build_view` produced for the canvas —
//! so the frontend indexes values positionally without an id join.
//! Elements the `[REPORT]` selection excluded from the results file carry
//! `NaN` (the codec's existing "no value" sentinel).
//!
//! ```text
//! u32 n_points   u32 n_polylines   u32 n_regions
//! u32 n_point_vars   u32 n_polyline_vars   u32 n_region_vars
//! f32 × (n_point_vars × n_points)        variable-major, catalog order
//! f32 × (n_polyline_vars × n_polylines)
//! f32 × (n_region_vars × n_regions)
//! ```

use std::collections::HashMap;
use std::path::Path;

use hydra::common::{ElementClass, ModelVariable};
use hydra::swmm::out_reader::{scan_periods, OutMetadata, PeriodRecord};
use hydra::uds::model::{ConcentrationUnits, Constituent, Network};

use super::generic_results::{GenericResultMetaDto, GenericVariableDto};
use super::uds_view::UdsView;

/// File column and serving scale for a *fixed* catalog variable (§14.9
/// record order). The catalog's presentation order deliberately differs
/// from file order, and capacity is stored as a fraction — this map is the
/// single place both facts live. Pollutant series are not here: they are
/// the model's, and `pollutant_column` places them.
fn column(class: ElementClass, id: &str) -> Option<(usize, f64)> {
    let col = match (class, id) {
        (ElementClass::Point, "depth") => (0, 1.0),
        (ElementClass::Point, "head") => (1, 1.0),
        (ElementClass::Point, "volume") => (2, 1.0),
        (ElementClass::Point, "lateralInflow") => (3, 1.0),
        (ElementClass::Point, "totalInflow") => (4, 1.0),
        (ElementClass::Point, "flooding") => (5, 1.0),
        (ElementClass::Polyline, "flow") => (0, 1.0),
        (ElementClass::Polyline, "depth") => (1, 1.0),
        (ElementClass::Polyline, "velocity") => (2, 1.0),
        (ElementClass::Polyline, "capacity") => (4, 100.0),
        (ElementClass::Region, "rainfall") => (0, 1.0),
        (ElementClass::Region, "runoff") => (4, 1.0),
        (ElementClass::Region, "infiltration") => (3, 1.0),
        _ => return None,
    };
    Some(col)
}

/// The §5 quantity descriptor for a catalog quantity key.
pub(super) fn quantity_descriptor(key: &str) -> Option<hydra::common::QuantityDescriptor> {
    hydra::uds::descriptors::QUANTITIES
        .iter()
        .find(|q| q.key == key)
        .copied()
}

/// File-units → SI factor for one quantity of this results file (§14.9
/// stores values in the file's declared system). Flow is per-declared-unit
/// (a gpm file is not a cfs file — the writer's own factors, inverted);
/// every other catalog quantity is either already SI or converts through
/// its §5 descriptor (all affine offsets in the catalog are zero).
fn si_factor(meta: &OutMetadata, quantity: Option<&str>) -> f64 {
    use hydra::swmm::options::FlowUnits::*;
    let Some(key) = quantity else { return 1.0 };
    if key == "flow" {
        return match meta.flow_units {
            Cfs => 0.028_316_846_592,
            Gpm => 6.309_019_64e-5,
            Mgd => 0.043_812_636_4,
            Cms => 1.0,
            Lps => 1.0e-3,
            Mld => 1.0 / 86.4,
        };
    }
    if !meta.flow_units.is_us() {
        return 1.0;
    }
    quantity_descriptor(key)
        .map(|q| 1.0 / q.si_to_us_scale)
        .unwrap_or(1.0)
}

/// How many fixed result columns a class's §14.9 record carries before its
/// pollutant concentrations begin.
///
/// The writer's own counts (8 + pollutants per subcatchment, 6 per node,
/// 5 per link). A collection has no record at all.
fn fixed_columns(class: ElementClass) -> usize {
    match class {
        ElementClass::Point => 6,
        ElementClass::Polyline => 5,
        ElementClass::Region => 8,
        ElementClass::Collection => 0,
    }
}

/// The §14.9 file's concentration code for a declared unit (0 mg/L,
/// 1 µg/L, 2 counts/L).
fn units_code(units: ConcentrationUnits) -> i32 {
    match units {
        ConcentrationUnits::MgPerL => 0,
        ConcentrationUnits::UgPerL => 1,
        ConcentrationUnits::CountPerL => 2,
    }
}

/// File column serving one model pollutant's concentration: the class's
/// fixed columns, then the pollutants in the *file's* order.
///
/// Resolved by name and by unit, never by position. A model edited since
/// the run can declare a different set, and laying the model's pollutants
/// over the file's columns in order would then label one substance with
/// another's name, or read µg/L as mg/L. A pollutant the file does not
/// carry under the same name and unit has no column here, which hydra-common
/// §6.2 already makes an absent variable rather than an error.
fn pollutant_column(meta: &OutMetadata, class: ElementClass, c: &Constituent) -> Option<usize> {
    let k = meta.pollutant_ids.iter().position(|p| *p == c.id)?;
    (meta.pollutant_units.get(k).copied() == Some(units_code(c.units)))
        .then(|| fixed_columns(class) + k)
}

/// The declared catalog variables that resolve to a file column, with the
/// full serving scale (shape scale × file-to-SI factor), per class.
///
/// The catalog is the model's (hydra-common §6.3): the fixed variables,
/// then one concentration series per pollutant the model declares. Without
/// a model only the fixed ones can be named, since nothing else knows what
/// the pollutants are called.
fn resolved_variables(
    meta: &OutMetadata,
    class: ElementClass,
    net: Option<&Network>,
) -> Vec<(ModelVariable, usize, f64)> {
    let fixed = hydra::uds::descriptors::result_variables(class);
    let vars = match net {
        Some(net) => hydra::uds::descriptors::result_variables_for(class, net),
        None => fixed.iter().map(ModelVariable::from).collect(),
    };
    // The catalog composes the pollutants after the fixed variables, in
    // the model's own order, so the tail lines up with `net.constituents`
    // positionally without parsing an id to recover its pollutant.
    let constituents: &[Constituent] = net.map(|n| &n.constituents[..]).unwrap_or(&[]);
    vars.into_iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let (col, shape) = match i.checked_sub(fixed.len()) {
                None => column(class, &v.id)?,
                Some(k) => (pollutant_column(meta, class, constituents.get(k)?)?, 1.0),
            };
            let scale = shape * si_factor(meta, v.quantity.as_deref());
            Some((v, col, scale))
        })
        .collect()
}

/// Build the variable catalog with min/max ranges from one sequential pass
/// over every period record.
pub fn generic_meta(
    out_path: &Path,
    meta: &OutMetadata,
    net: Option<&Network>,
) -> Result<GenericResultMetaDto, String> {
    let classes = [
        ElementClass::Point,
        ElementClass::Polyline,
        ElementClass::Region,
    ];
    let vars: Vec<Vec<(ModelVariable, usize, f64)>> = classes
        .iter()
        .map(|&c| resolved_variables(meta, c, net))
        .collect();
    // ranges[class][var] = (min, max)
    let mut ranges: Vec<Vec<(f64, f64)>> = vars
        .iter()
        .map(|v| vec![(f64::INFINITY, f64::NEG_INFINITY); v.len()])
        .collect();

    scan_periods(out_path, meta, |_, rec| {
        for (ci, class_vars) in vars.iter().enumerate() {
            let (values, n_vars) = class_values(rec, meta, classes[ci]);
            for (vi, (_, col, scale)) in class_vars.iter().enumerate() {
                for element in values.chunks_exact(n_vars) {
                    let v = element[*col] as f64 * scale;
                    if v.is_finite() {
                        let (min, max) = &mut ranges[ci][vi];
                        *min = min.min(v);
                        *max = max.max(v);
                    }
                }
            }
        }
    })?;

    let mut out: Vec<Vec<GenericVariableDto>> = Vec::with_capacity(3);
    for (ci, class_vars) in vars.iter().enumerate() {
        out.push(
            class_vars
                .iter()
                .enumerate()
                .map(|(vi, (v, _, _))| {
                    let (min, max) = ranges[ci][vi];
                    GenericVariableDto::from_model_variable(v, min, max, quantity_descriptor)
                })
                .collect(),
        );
    }
    let mut it = out.into_iter();
    Ok(GenericResultMetaDto {
        point_vars: it.next().unwrap_or_default(),
        polyline_vars: it.next().unwrap_or_default(),
        region_vars: it.next().unwrap_or_default(),
    })
}

/// A period record's value slab and per-element stride for one class.
fn class_values<'a>(
    rec: &'a PeriodRecord,
    meta: &OutMetadata,
    class: ElementClass,
) -> (&'a [f32], usize) {
    match class {
        ElementClass::Point => (&rec.nodes, meta.n_node_vars),
        ElementClass::Polyline => (&rec.links, meta.n_link_vars),
        ElementClass::Region => (&rec.subcatchments, meta.n_subcatch_vars),
        ElementClass::Collection => (&[], 1),
    }
}

/// Record index by element id for one class of the results file.
fn out_index(ids: &[String]) -> HashMap<&str, usize> {
    ids.iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect()
}

/// Encode one period's values for every declared variable, in snapshot
/// order (see the module docs for the layout).
pub fn encode_generic_period(
    view: &UdsView,
    meta: &OutMetadata,
    rec: &PeriodRecord,
    net: &Network,
) -> Vec<u8> {
    let point_vars = resolved_variables(meta, ElementClass::Point, Some(net));
    let polyline_vars = resolved_variables(meta, ElementClass::Polyline, Some(net));
    let region_vars = resolved_variables(meta, ElementClass::Region, Some(net));

    let n_values = point_vars.len() * view.points.len()
        + polyline_vars.len() * view.polylines.len()
        + region_vars.len() * view.regions.len();
    let mut buf = Vec::with_capacity(24 + 4 * n_values);
    for n in [
        view.points.len(),
        view.polylines.len(),
        view.regions.len(),
        point_vars.len(),
        polyline_vars.len(),
        region_vars.len(),
    ] {
        buf.extend_from_slice(&(n as u32).to_le_bytes());
    }

    let node_index = out_index(&meta.node_ids);
    let link_index = out_index(&meta.link_ids);
    let region_index = out_index(&meta.subcatchment_ids);

    let mut write_class = |ids: Vec<&str>, index: &HashMap<&str, usize>, class: ElementClass| {
        let (values, n_vars) = class_values(rec, meta, class);
        for (_, col, scale) in resolved_variables(meta, class, Some(net)) {
            for id in &ids {
                let v = index
                    .get(id)
                    .map(|&i| values[i * n_vars + col] * scale as f32)
                    .unwrap_or(f32::NAN);
                buf.extend_from_slice(&v.to_le_bytes());
            }
        }
    };
    write_class(
        view.points.iter().map(|p| p.id.as_str()).collect(),
        &node_index,
        ElementClass::Point,
    );
    write_class(
        view.polylines.iter().map(|p| p.id.as_str()).collect(),
        &link_index,
        ElementClass::Polyline,
    );
    write_class(
        view.regions.iter().map(|r| r.id.as_str()).collect(),
        &region_index,
        ElementClass::Region,
    );
    buf
}

/// Stream every reporting period into per-class CSV files, one row per
/// (element, period), period-major — the drainage counterpart of the wds
/// `stream_results_csv`. Columns are the §6 catalog variables in catalog
/// order (headers use the catalog ids); values are as served to the canvas:
/// the file's own unit system, capacity scaled to percent. The
/// subcatchments file is written only when the results report any.
pub fn stream_uds_results_csv(
    out_path: &Path,
    meta: &OutMetadata,
    net: Option<&Network>,
    nodes_csv: &Path,
    links_csv: &Path,
    subcatchments_csv: &Path,
) -> Result<(), String> {
    use std::io::Write;

    let open = |p: &Path| {
        std::fs::File::create(p)
            .map(std::io::BufWriter::new)
            .map_err(|e| format!("Cannot create {}: {e}", p.display()))
    };
    let werr = |e: std::io::Error| format!("Cannot write CSV: {e}");

    let classes: [(ElementClass, &[String], &Path); 3] = [
        (ElementClass::Point, &meta.node_ids, nodes_csv),
        (ElementClass::Polyline, &meta.link_ids, links_csv),
        (
            ElementClass::Region,
            &meta.subcatchment_ids,
            subcatchments_csv,
        ),
    ];
    let mut writers = Vec::new();
    for (class, ids, path) in classes {
        if ids.is_empty() {
            continue;
        }
        let vars = resolved_variables(meta, class, net);
        let mut w = open(path)?;
        let header = vars
            .iter()
            .map(|(v, _, _)| v.id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        writeln!(w, "id,time_s,{header}").map_err(werr)?;
        writers.push((class, ids, vars, w));
    }

    scan_result(out_path, meta, |p, rec| {
        let t = (p as i64 + 1) * meta.report_step_s as i64;
        for (class, ids, vars, w) in writers.iter_mut() {
            let (values, n_vars) = class_values(rec, meta, *class);
            for (i, id) in ids.iter().enumerate() {
                write!(w, "{id},{t}").map_err(werr)?;
                for (_, col, scale) in vars.iter() {
                    let v = values[i * n_vars + col] as f64 * scale;
                    write!(w, ",{v}").map_err(werr)?;
                }
                writeln!(w).map_err(werr)?;
            }
        }
        Ok(())
    })?;
    for (_, _, _, mut w) in writers {
        w.flush().map_err(werr)?;
    }
    Ok(())
}

/// Full-simulation series for one element, addressed the way the frontend
/// addresses everything: element class ("node"/"link"/"region") + snapshot
/// index.
/// The snapshot and results file can disagree about membership (the
/// `[REPORT]` selection), so the id is resolved from the snapshot view and
/// joined into the file's record order; an unreported element answers
/// `None` like an unsimulated one. Field names are the §6 catalog's
/// variable ids, values as served everywhere else (file units, capacity
/// scaled to percent); times are the sim-relative instants
/// `load_result_meta` serves.
pub fn element_series(
    out_path: &Path,
    network: &hydra::uds::model::Network,
    kind: &str,
    index: usize,
) -> Result<Option<super::results::SeriesDto>, String> {
    use hydra::swmm::out_reader::{read_element_series, ElementKind};

    let meta = hydra::swmm::out_reader::read_metadata(out_path)?;
    let view = super::uds_view::build_view(network);
    let (element_id, out_ids, out_kind, class) = match kind {
        "node" => (
            view.points.get(index).map(|p| p.id.as_str()),
            &meta.node_ids,
            ElementKind::Node,
            ElementClass::Point,
        ),
        "link" => (
            view.polylines.get(index).map(|p| p.id.as_str()),
            &meta.link_ids,
            ElementKind::Link,
            ElementClass::Polyline,
        ),
        // Areal elements read exactly like the other two classes — the
        // results file stores subcatchment records first, and the reader
        // has always known how to address them. The inspector showed a
        // Time series section for a node and a conduit and nothing for the
        // subcatchment draining into them, which read as "this engine does
        // not record runoff over time" rather than "this arm was missing".
        "region" => (
            view.regions.get(index).map(|r| r.id.as_str()),
            &meta.subcatchment_ids,
            ElementKind::Subcatchment,
            ElementClass::Region,
        ),
        other => return Err(format!("unknown element kind {other:?}")),
    };
    let Some(element_id) = element_id else {
        return Ok(None);
    };
    let Some(out_index) = out_ids.iter().position(|id| id == element_id) else {
        return Ok(None);
    };

    let series = read_element_series(out_path, &meta, out_kind, out_index)?;
    let times: Vec<u32> = (0..meta.n_periods)
        .map(|i| ((i as i64 + 1) * meta.report_step_s as i64) as u32)
        .collect();
    let fields = resolved_variables(&meta, class, Some(network))
        .into_iter()
        .filter_map(|(v, col, scale)| {
            let values = series.vars.get(col)?;
            Some(super::results::SeriesFieldDto {
                name: v.id.clone(),
                values: values.iter().map(|&x| x as f64 * scale).collect(),
            })
        })
        .collect();
    Ok(Some(super::results::SeriesDto { times, fields }))
}

/// `scan_periods` with a fallible callback: the reader's scan takes an
/// infallible closure, so IO errors are carried out through a slot.
fn scan_result(
    out_path: &Path,
    meta: &OutMetadata,
    mut f: impl FnMut(usize, &PeriodRecord) -> Result<(), String>,
) -> Result<(), String> {
    let mut failed: Option<String> = None;
    scan_periods(out_path, meta, |p, rec| {
        if failed.is_none() {
            if let Err(e) = f(p, rec) {
                failed = Some(e);
            }
        }
    })?;
    match failed {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A polluted model end to end: hydra-common §6.3 says the catalog is
    /// the model's, so every reported class gains one concentration series
    /// per declared pollutant, and every surface serving results (catalog,
    /// period payload, CSV, element series) must agree about that.
    #[test]
    fn a_declared_pollutant_becomes_a_series_on_every_reported_class() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\nFLOW_ROUTING DYNWAVE\n\
                     START_DATE 01/01/2024\nSTART_TIME 00:00:00\n\
                     END_DATE 01/01/2024\nEND_TIME 01:00:00\nREPORT_STEP 00:05:00\n\
                     [POLLUTANTS]\nTSS MG/L 0 0 0 0 NO * 0 0 0\n\
                     Lead UG/L 0 0 0 0 NO * 0 0 0\n\
                     [JUNCTIONS]\nJ1 100 4\n[OUTFALLS]\nO1 98 FREE\n\
                     [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
                     [TIMESERIES]\nQ1 0:00 2.0\nQ1 2:00 2.0\n\
                     CT 0:00 100\nCT 2:00 100\nCP 0:00 25\nCP 2:00 25\n\
                     [INFLOWS]\nJ1 FLOW Q1\nJ1 TSS CT CONCENTRATION\n\
                     J1 Lead CP CONCENTRATION\n\
                     [REPORT]\nNODES ALL\nLINKS ALL\n\
                     [COORDINATES]\nJ1 0 0\nO1 100 0\n";
        let (sim, _diags, _findings) = hydra::swmm::session::open(model).expect("open uds model");
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let (_es, err, _wall, _steps) = crate::commands::simulation::run_sim_loops(
            hydra::engines::EngineSession::from_uds(sim),
            Some(out.clone()),
            crate::commands::simulation::RunContext {
                duration_seconds: 3600.0,
                network_digest: None,
                pre_run_warnings: Vec::new(),
            },
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "uds run must succeed: {err:?}");

        let meta = hydra::swmm::out_reader::read_metadata(&out).expect("readable");
        let (network, _diags) = hydra::swmm::objects::parse_network(model);
        let gm = generic_meta(&out, &meta, Some(&network)).expect("generic meta");

        // Two series added to each class's fixed catalog, in the order the
        // model declares them, after the fixed variables.
        for (vars, fixed) in [
            (&gm.point_vars, 6),
            (&gm.polyline_vars, 4),
            (&gm.region_vars, 3),
        ] {
            let ids: Vec<&str> = vars.iter().map(|v| v.id.as_str()).collect();
            assert_eq!(
                &ids[fixed..],
                &["pollutant:TSS", "pollutant:Lead"],
                "catalog was {ids:?}"
            );
        }
        // Each carries its own declared unit, not one standing for both.
        let var = |id: &str| gm.point_vars.iter().find(|v| v.id == id).unwrap();
        assert_eq!(var("pollutant:TSS").quantity.unwrap().key, "concentration");
        assert_eq!(
            var("pollutant:Lead").quantity.unwrap().key,
            "concentrationMicro"
        );
        // A concentration is stated the same way in either unit system, so
        // it is served as stored. This is a CFS file, and the inflow runs
        // at 100 mg/L: putting a concentration through the flow factor
        // would report it as 2.83 or as 3531.
        let tss = var("pollutant:TSS");
        assert!(
            tss.max > 50.0 && tss.max <= 100.0 + 1e-6,
            "TSS range must be the file's own mg/L, got {}",
            tss.max
        );

        // The period payload declares the same counts the catalog does —
        // the frontend addresses columns by catalog position, so a
        // disagreement here recolours the map with another variable.
        let view = super::super::uds_view::build_view(&network);
        let rec = hydra::swmm::out_reader::read_period(&out, &meta, 0).unwrap();
        let payload = encode_generic_period(&view, &meta, &rec, &network);
        let header: Vec<u32> = payload[..24]
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert_eq!(header, vec![2, 1, 0, 8, 6, 5]);

        // CSV export and the inspector's series name them the same way.
        let nodes_csv = dir.path().join("p-nodes.csv");
        let links_csv = dir.path().join("p-links.csv");
        let subs_csv = dir.path().join("p-subcatchments.csv");
        stream_uds_results_csv(
            &out,
            &meta,
            Some(&network),
            &nodes_csv,
            &links_csv,
            &subs_csv,
        )
        .unwrap();
        let nodes = std::fs::read_to_string(&nodes_csv).unwrap();
        assert_eq!(
            nodes.lines().next().unwrap(),
            "id,time_s,depth,head,volume,lateralInflow,totalInflow,flooding,\
             pollutant:TSS,pollutant:Lead"
        );
        let series = element_series(&out, &network, "link", 0)
            .unwrap()
            .expect("C1 series");
        let names: Vec<&str> = series.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(&names[4..], &["pollutant:TSS", "pollutant:Lead"]);
        // Named right and read from the right place: a link's pollutants
        // begin after its five stored variables, and the one before them
        // is a 0-1 capacity fraction that would pass any range check a
        // concentration got.
        let tss = &series.fields[4].values;
        assert!(
            tss.iter().cloned().fold(0.0, f64::max) > 50.0,
            "link TSS series reads the wrong column: {tss:?}"
        );

        // The columns themselves, since a concentration read one place
        // early is still a plausible-looking number: the fixed variables
        // of the class, then the pollutants in the file's own order.
        for (class, fixed) in [
            (ElementClass::Point, 6),
            (ElementClass::Polyline, 5),
            (ElementClass::Region, 8),
        ] {
            let cols: Vec<usize> = resolved_variables(&meta, class, Some(&network))
                .into_iter()
                .filter(|(v, _, _)| v.id.starts_with("pollutant:"))
                .map(|(_, col, _)| col)
                .collect();
            assert_eq!(cols, vec![fixed, fixed + 1], "{class:?}");
        }

        // Reconciliation: the file names its pollutants, so a model edited
        // since the run resolves by name and unit rather than by position.
        // Renaming one retires its series; the other is still the same
        // substance in the same column and stays.
        let mut renamed = network.clone();
        renamed.constituents[1].id = "Pb".to_string();
        let ids: Vec<String> = resolved_variables(&meta, ElementClass::Point, Some(&renamed))
            .into_iter()
            .map(|(v, _, _)| v.id)
            .collect();
        assert!(ids.contains(&"pollutant:TSS".to_string()));
        assert!(
            !ids.iter().any(|i| i.starts_with("pollutant:P")),
            "a pollutant the file does not carry has no column: {ids:?}"
        );

        // Changing a pollutant's units retires it too: the stored numbers
        // are the old unit's, and publishing them under the new one would
        // read µg/L as mg/L rather than say nothing.
        let mut redeclared = network.clone();
        redeclared.constituents[0].units = hydra::uds::model::ConcentrationUnits::UgPerL;
        let ids: Vec<String> = resolved_variables(&meta, ElementClass::Point, Some(&redeclared))
            .into_iter()
            .map(|(v, _, _)| v.id)
            .collect();
        assert!(!ids.contains(&"pollutant:TSS".to_string()), "{ids:?}");
        assert!(ids.contains(&"pollutant:Lead".to_string()), "{ids:?}");

        // With no model to name them, only the fixed catalog can be
        // published — the file's columns are still there, but nothing
        // knows what they are called.
        let ids: Vec<String> = resolved_variables(&meta, ElementClass::Point, None)
            .into_iter()
            .map(|(v, _, _)| v.id)
            .collect();
        assert_eq!(ids.len(), 6, "{ids:?}");
    }

    /// Run a tiny drainage model end-to-end and check the provider's two
    /// halves against each other: the catalog meta's variable counts and
    /// unit labels, and the period payload's header + length against the
    /// snapshot view the canvas renders.
    #[test]
    fn generic_meta_and_period_payload_line_up_with_the_snapshot() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\nFLOW_ROUTING DYNWAVE\n\
                     START_DATE 01/01/2024\nSTART_TIME 00:00:00\n\
                     END_DATE 01/01/2024\nEND_TIME 01:00:00\nREPORT_STEP 00:05:00\n\
                     [JUNCTIONS]\nJ1 100 4\n[OUTFALLS]\nO1 98 FREE\n\
                     [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
                     [REPORT]\nNODES ALL\nLINKS ALL\n\
                     [COORDINATES]\nJ1 0 0\nO1 100 0\n";
        let (sim, _diags, _findings) = hydra::swmm::session::open(model).expect("open uds model");
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let (_es, err, _wall, _steps) = crate::commands::simulation::run_sim_loops(
            hydra::engines::EngineSession::from_uds(sim),
            Some(out.clone()),
            crate::commands::simulation::RunContext {
                duration_seconds: 3600.0,
                network_digest: None,
                pre_run_warnings: Vec::new(),
            },
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "uds run must succeed: {err:?}");

        let meta = hydra::swmm::out_reader::read_metadata(&out).expect("readable");
        let (network, _diags) = hydra::swmm::objects::parse_network(model);
        let gm = generic_meta(&out, &meta, Some(&network)).expect("generic meta");
        // The §6 catalog: 6 point, 4 polyline, 3 region variables.
        assert_eq!(gm.point_vars.len(), 6);
        assert_eq!(gm.polyline_vars.len(), 4);
        assert_eq!(gm.region_vars.len(), 3);
        // Values are served in SI with the §5 quantity descriptor embedded
        // — the frontend converts to the display system with it. A CFS
        // file's node depths therefore arrive as metres: the junction sits
        // 2 ft below ground (invert 100, rim 102 in the model → max depth
        // 4 ft would bound depth), so no depth may exceed ~1.3 m even
        // though the file stores feet.
        let depth = gm.point_vars.iter().find(|v| v.id == "depth").unwrap();
        assert_eq!(depth.quantity.unwrap().key, "depth");
        assert!(
            depth.max < 4.0 * 0.3048 + 1e-6,
            "depth range must be SI metres, got {}",
            depth.max
        );
        let flow = gm.polyline_vars.iter().find(|v| v.id == "flow").unwrap();
        assert_eq!(flow.quantity.unwrap().key, "flow");
        // Capacity is served ×100 against its percent quantity.
        let capacity = gm
            .polyline_vars
            .iter()
            .find(|v| v.id == "capacity")
            .unwrap();
        assert_eq!(capacity.quantity.unwrap().key, "percent");
        assert!(capacity.max <= 100.0 + 1e-6, "fraction scaled to percent");
        // Ranges came from a real scan: ordered and finite.
        assert!(depth.min <= depth.max && depth.max.is_finite());

        let view = super::super::uds_view::build_view(&network);
        assert_eq!(view.points.len(), 2);
        assert_eq!(view.polylines.len(), 1);
        let rec = hydra::swmm::out_reader::read_period(&out, &meta, 0).unwrap();
        let payload = encode_generic_period(&view, &meta, &rec, &network);
        let header: Vec<u32> = payload[..24]
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        // Var counts are the catalog's, independent of element counts —
        // region vars stay 3 even when the model has no subcatchments.
        assert_eq!(header, vec![2, 1, 0, 6, 4, 3]);
        assert_eq!(payload.len(), 24 + 4 * (6 * 2 + 4));
        // Both reported nodes have a finite depth (variable 0, snapshot order).
        let val =
            |i: usize| f32::from_le_bytes(payload[24 + 4 * i..28 + 4 * i].try_into().unwrap());
        assert!(val(0).is_finite() && val(1).is_finite());

        // CSV streaming: catalog-order headers, one row per (element,
        // period), and no subcatchments file for a model without any.
        let nodes_csv = dir.path().join("r-nodes.csv");
        let links_csv = dir.path().join("r-links.csv");
        let subs_csv = dir.path().join("r-subcatchments.csv");
        stream_uds_results_csv(
            &out,
            &meta,
            Some(&network),
            &nodes_csv,
            &links_csv,
            &subs_csv,
        )
        .unwrap();
        let nodes = std::fs::read_to_string(&nodes_csv).unwrap();
        let mut lines = nodes.lines();
        assert_eq!(
            lines.next().unwrap(),
            "id,time_s,depth,head,volume,lateralInflow,totalInflow,flooding"
        );
        assert_eq!(lines.count(), meta.node_ids.len() * meta.n_periods);
        let links = std::fs::read_to_string(&links_csv).unwrap();
        assert_eq!(
            links.lines().next().unwrap(),
            "id,time_s,flow,depth,velocity,capacity"
        );
        assert!(!subs_csv.exists(), "no subcatchments → no third file");

        // Element series: snapshot-indexed, catalog-named fields, one
        // value per period, sim-relative times.
        let series = element_series(&out, &network, "node", 0)
            .unwrap()
            .expect("J1 series");
        assert_eq!(series.times.len(), meta.n_periods);
        assert_eq!(series.times[0], meta.report_step_s as u32);
        let names: Vec<&str> = series.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "depth",
                "head",
                "volume",
                "lateralInflow",
                "totalInflow",
                "flooding"
            ]
        );
        assert!(series
            .fields
            .iter()
            .all(|f| f.values.len() == meta.n_periods));
        // Out-of-range snapshot index answers None, not an error.
        assert!(element_series(&out, &network, "node", 99)
            .unwrap()
            .is_none());
    }

    /// Every element class the snapshot carries can be asked for a series.
    ///
    /// The areal arm was missing: the reader has always known how to
    /// address subcatchment records, but `element_series` matched only
    /// `"node"` and `"link"`, so the inspector showed a Time series
    /// section for a junction and a conduit and nothing at all for the
    /// subcatchment draining into them. Nothing failed loudly — the
    /// request errored into the frontend's `null` and the section simply
    /// did not render, which reads as "this engine does not record runoff
    /// over time".
    #[test]
    fn every_snapshot_class_can_be_asked_for_a_series() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\nFLOW_ROUTING DYNWAVE\nINFILTRATION HORTON\n\
                     START_DATE 01/01/2024\nSTART_TIME 00:00:00\n\
                     END_DATE 01/01/2024\nEND_TIME 01:00:00\nREPORT_STEP 00:05:00\n\
                     [RAINGAGES]\nG1 INTENSITY 0:15 1.0 TIMESERIES RAIN1\n\
                     [SUBCATCHMENTS]\nS1 G1 J1 10 25 500 0.5 0\n\
                     [SUBAREAS]\nS1 0.012 0.1 0.05 0.05 25 OUTLET\n\
                     [INFILTRATION]\nS1 3.0 0.5 4 7 0\n\
                     [JUNCTIONS]\nJ1 100 4\n[OUTFALLS]\nO1 98 FREE\n\
                     [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
                     [TIMESERIES]\nRAIN1 0:00 0.5\nRAIN1 1:00 1.0\n\
                     [REPORT]\nSUBCATCHMENTS ALL\nNODES ALL\nLINKS ALL\n\
                     [COORDINATES]\nJ1 0 0\nO1 100 0\n\
                     [POLYGONS]\nS1 0 0\nS1 10 0\nS1 10 10\nS1 0 10\n";
        let (sim, _diags, _findings) = hydra::swmm::session::open(model).expect("open uds model");
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let (_es, err, _wall, _steps) = crate::commands::simulation::run_sim_loops(
            hydra::engines::EngineSession::from_uds(sim),
            Some(out.clone()),
            crate::commands::simulation::RunContext {
                duration_seconds: 3600.0,
                network_digest: None,
                pre_run_warnings: Vec::new(),
            },
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "uds run must succeed: {err:?}");

        let meta = hydra::swmm::out_reader::read_metadata(&out).expect("readable");
        let (network, _diags) = hydra::swmm::objects::parse_network(model);
        let view = super::super::uds_view::build_view(&network);
        assert_eq!(view.regions.len(), 1, "the model has one subcatchment");

        for kind in ["node", "link", "region"] {
            let series = element_series(&out, &network, kind, 0)
                .unwrap()
                .unwrap_or_else(|| panic!("{kind} index 0 must have a series"));
            assert_eq!(series.times.len(), meta.n_periods, "{kind} period count");
            assert!(
                !series.fields.is_empty(),
                "{kind} must carry its catalog's variables"
            );
            assert!(
                series
                    .fields
                    .iter()
                    .all(|f| f.values.len() == meta.n_periods),
                "{kind} fields must be one value per period"
            );
        }

        // The areal series is the region catalog's, not a neighbouring
        // class's — the failure mode of a missing match arm falling through
        // to the wrong one.
        let region = element_series(&out, &network, "region", 0)
            .unwrap()
            .expect("S1 series");
        let names: Vec<&str> = region.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["rainfall", "runoff", "infiltration"]);

        // Same contract as the other classes at the edges.
        assert!(element_series(&out, &network, "region", 99)
            .unwrap()
            .is_none());
        assert!(element_series(&out, &network, "parcel", 0).is_err());
    }
}
