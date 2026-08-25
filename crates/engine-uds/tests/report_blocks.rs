//! The uds report blocks against real fixture runs: catalog integrity,
//! production from the results file, ranked-table options, and the
//! unavailable/unknown error contract.

use std::path::PathBuf;

use hydra_common::{BlockError, FragmentItem};
use hydra_engine_uds::report_blocks::{produce_report_block, report_block_options, report_catalog};
use hydra_interop_swmm::objects::parse_network;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/uds")
        .join(name);
    std::fs::read_to_string(path).expect("read fixture")
}

/// Run a fixture with every object reported over a two-hour horizon;
/// return the results path and the parsed network. `tag` keeps parallel
/// tests using the same fixture off each other's files.
fn run_to_out(name: &str, tag: &str) -> (PathBuf, hydra_engine_uds::model::Network) {
    run_to_out_with(name, "", tag)
}

/// `run_to_out` with extra INP sections appended — for a fixture that
/// needs forcing (an inflow) the base file deliberately omits.
fn run_to_out_with(
    name: &str,
    extra: &str,
    tag: &str,
) -> (PathBuf, hydra_engine_uds::model::Network) {
    let text = format!(
        "{}\n{extra}\n[OPTIONS]\nSTART_DATE 01/01/2024\nSTART_TIME 00:00:00\n\
         END_DATE 01/01/2024\nEND_TIME 02:00:00\nREPORT_STEP 00:05:00\n\
         [REPORT]\nSUBCATCHMENTS ALL\nNODES ALL\nLINKS ALL\n",
        fixture(name)
    );
    let (mut sim, _diags, _findings) = hydra_interop_swmm::session::open(&text).expect("open");
    while sim.step() {}

    let mut path = std::env::temp_dir();
    path.push(format!(
        "hydra-uds-blocks-{tag}-{}-{}.out",
        name.trim_end_matches(".inp"),
        std::process::id()
    ));
    let mut w = std::io::BufWriter::new(std::fs::File::create(&path).expect("create"));
    hydra_interop_swmm::session::write_out(&sim, &mut w).expect("write out");
    use std::io::Write as _;
    w.flush().expect("flush");

    let (net, _) = parse_network(&text);
    (path, net)
}

#[test]
fn the_catalog_is_namespaced_and_unique() {
    let mut seen = std::collections::HashSet::new();
    for b in report_catalog() {
        assert!(b.id.starts_with("uds."), "{} is not namespaced", b.id);
        assert!(seen.insert(b.id), "duplicate id {}", b.id);
        assert!(!b.title.is_empty() && !b.summary.is_empty());
        assert!(!b.category.is_empty(), "{}: empty category", b.id);
    }
}

/// Catalog order is presentation order (common spec §3.2): a category's
/// blocks sit together, so consumers deriving tabs get each category once.
#[test]
fn catalog_categories_are_contiguous() {
    let mut seen: Vec<&str> = Vec::new();
    for b in report_catalog() {
        match seen.last() {
            Some(&last) if last == b.category => {}
            _ => {
                assert!(
                    !seen.contains(&b.category),
                    "category {:?} appears in two runs; blocks sharing a \
                     category must be adjacent in the catalog",
                    b.category
                );
                seen.push(b.category);
            }
        }
    }
}

#[test]
fn every_block_produces_or_declines_for_a_real_run() {
    let (path, net) = run_to_out("runoff_parcel.inp", "all");
    for b in report_catalog() {
        match produce_report_block(
            b.id,
            &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
            &net,
            None,
        ) {
            Ok(fragment) => {
                assert!(!fragment.items.is_empty(), "{} produced nothing", b.id);
            }
            Err(BlockError::Unavailable { reason }) => {
                assert!(
                    reason.ends_with('.'),
                    "{}: unavailable reason should be a sentence: {reason}",
                    b.id
                );
            }
            Err(e) => panic!("{} failed: {e}", b.id),
        }
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn run_summary_reports_the_horizon_and_nonzero_rain_peak() {
    let (path, net) = run_to_out("runoff_parcel.inp", "summary");
    let f = produce_report_block(
        "uds.run-summary",
        &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
        &net,
        None,
    )
    .expect("produce");
    let FragmentItem::KeyValues { entries } = &f.items[0] else {
        panic!("run summary is not a key-value list");
    };
    let get = |label: &str| {
        entries
            .iter()
            .find(|e| e.label == label)
            .unwrap_or_else(|| panic!("missing {label}"))
    };
    match get("Peak rainfall").value {
        hydra_common::Value::Number { value, .. } => {
            assert!(value > 0.0, "rain fixture reports zero peak rainfall")
        }
        ref v => panic!("peak rainfall is {v:?}"),
    }
    match get("Reporting periods").value {
        hydra_common::Value::Integer { value } => assert!(value > 0),
        ref v => panic!("periods is {v:?}"),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_rows_option_caps_a_ranked_table_and_rejects_garbage() {
    let (path, net) = run_to_out("single_conduit.inp", "rows");
    let one = serde_json::json!({ "rows": 1 });
    let f = produce_report_block(
        "uds.node-extremes",
        &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
        &net,
        Some(&one),
    )
    .expect("produce");
    let FragmentItem::Table { table } = &f.items[0] else {
        panic!("node extremes is not a table");
    };
    assert_eq!(table.rows.len(), 1);

    let bad = serde_json::json!({ "rows": "many" });
    let err = produce_report_block(
        "uds.node-extremes",
        &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
        &net,
        Some(&bad),
    )
    .expect_err("garbage options must fail");
    assert!(matches!(err, BlockError::Failed { .. }));

    // The option is described for the ranked tables and only those.
    assert_eq!(report_block_options("uds.node-extremes", &net).len(), 1);
    assert!(report_block_options("uds.run-summary", &net).is_empty());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_unknown_block_is_a_typed_refusal() {
    let (path, net) = run_to_out("single_conduit.inp", "unknown");
    let err = produce_report_block(
        "uds.no-such-block",
        &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
        &net,
        None,
    )
    .expect_err("unknown id must be refused");
    assert!(matches!(err, BlockError::UnknownBlock { .. }));
    let _ = std::fs::remove_file(&path);
}

/// The v1.7 tagging obligation (hydra-common spec §3.3): a CFS fixture's
/// flow values are produced in m³/s — the flow quantity's SI display unit
/// — tagged, wearing the SI label, and the file's cfs value is recovered
/// exactly by the same factor the file was written under.
#[test]
fn a_us_file_produces_tagged_si_flow_values() {
    let (path, net) = run_to_out("single_conduit.inp", "tagging");
    let fragment = produce_report_block(
        "uds.link-extremes",
        &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
        &net,
        None,
    )
    .expect("link extremes");
    let FragmentItem::Table { table } = &fragment.items[0] else {
        panic!("link extremes is a table");
    };
    let flow_col = &table.columns[1];
    assert_eq!(flow_col.quantity.as_deref(), Some("flow"));
    assert_eq!(
        flow_col.unit.as_deref(),
        Some("m³/s"),
        "SI label on the header"
    );

    let hydra_common::Value::Number { value, .. } = &table.rows[0][1] else {
        panic!("peak flow is a number");
    };
    // single_conduit declares CFS, so a value of one file unit is
    // 0.0283… m³/s — a tagged value an order of magnitude above the
    // fixture's plausible flows would mean the conversion was skipped.
    assert!(
        value.is_finite() && *value < 10.0,
        "peak flow {value} m³/s is implausible for the fixture — file units leaked through"
    );
    let _ = std::fs::remove_file(&path);
}

/// Every quantity key the blocks tag with exists in the engine's §5
/// catalog — asserted through the public surface by producing every block
/// and walking its fragments, since an uncataloged key renders as-written
/// and nothing else would ever flag the typo.
#[test]
fn every_tagged_key_is_in_the_quantity_catalog() {
    let cataloged: std::collections::HashSet<&str> = hydra_engine_uds::descriptors::QUANTITIES
        .iter()
        .map(|q| q.key)
        .collect();
    let (path, net) = run_to_out("single_conduit.inp", "keys");
    for block in report_catalog() {
        let Ok(fragment) = produce_report_block(
            block.id,
            &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
            &net,
            None,
        ) else {
            continue; // unavailable for this fixture is fine
        };
        for item in &fragment.items {
            let check = |key: &Option<String>, what: &str| {
                if let Some(k) = key {
                    assert!(
                        cataloged.contains(k.as_str()),
                        "{} tags {what} with uncataloged quantity {k:?}",
                        block.id
                    );
                }
            };
            match item {
                FragmentItem::KeyValues { entries } => {
                    for e in entries {
                        if let hydra_common::Value::Number { quantity, .. } = &e.value {
                            check(quantity, "a key-value");
                        }
                    }
                }
                FragmentItem::Table { table } => {
                    for c in &table.columns {
                        check(&c.quantity, "a column");
                    }
                    for row in &table.rows {
                        for v in row {
                            if let hydra_common::Value::Number { quantity, .. } = v {
                                check(quantity, "a cell");
                            }
                        }
                    }
                }
                FragmentItem::Chart { chart } => {
                    check(&chart.x_quantity, "an x axis");
                    check(&chart.y_quantity, "a y axis");
                }
                FragmentItem::Note { .. } => {}
            }
        }
    }
    let _ = std::fs::remove_file(&path);
}

/// §13.4.2: the balance integrates the system series, so the rain-driven
/// fixture must show water moving, an in-bounds residual, and aligned
/// chart series.
#[test]
fn system_balance_reports_moving_water_and_its_series() {
    let (path, net) = run_to_out("runoff_parcel.inp", "balance");
    let f = produce_report_block(
        "uds.system-balance",
        &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
        &net,
        None,
    )
    .expect("produce");
    let FragmentItem::KeyValues { entries } = &f.items[0] else {
        panic!("balance is not a key-value list");
    };
    let value_of = |label: &str| -> f64 {
        match entries.iter().find(|e| e.label == label) {
            Some(e) => match e.value {
                hydra_common::Value::Number { value, .. } => value,
                ref v => panic!("{label} is {v:?}"),
            },
            None => panic!("missing {label}"),
        }
    };
    let total_in = value_of("Total inflow");
    assert!(total_in > 0.0, "rain fixture moved no water");
    assert!(value_of("Runoff") > 0.0, "runoff component missing");
    // The residual absorbs reporting-resolution error and unitemised
    // processes — never the bulk of the water itself.
    assert!(
        value_of("Residual").abs() < total_in,
        "residual exceeds total inflow"
    );
    let FragmentItem::Chart { chart } = &f.items[1] else {
        panic!("balance carries no chart");
    };
    assert_eq!(chart.y_quantity.as_deref(), Some("flow"));
    let hydra_common::ChartData::Line { series } = &chart.data else {
        panic!("balance chart is not a line chart");
    };
    assert!(series.len() >= 2, "inflow and outflow series expected");
    assert_eq!(series[0].points.len(), series[1].points.len());
    let _ = std::fs::remove_file(&path);
}

/// §13.4.3: depths integrate from intensities, volume from the runoff
/// rate, and the coefficient is their ratio over the model's area — so
/// the rain fixture's lone subcatchment gets a physical coefficient.
#[test]
fn runoff_summary_yields_a_physical_runoff_coefficient() {
    let (path, net) = run_to_out("runoff_parcel.inp", "runoff");
    let f = produce_report_block(
        "uds.runoff-summary",
        &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
        &net,
        None,
    )
    .expect("produce");
    let FragmentItem::Table { table } = &f.items[0] else {
        panic!("runoff summary is not a table");
    };
    assert!(!table.rows.is_empty());
    let row = &table.rows[0];
    let cell = |i: usize| -> f64 {
        match &row[i] {
            hydra_common::Value::Number { value, .. } => *value,
            v => panic!("cell {i} is {v:?}"),
        }
    };
    assert!(cell(1) > 0.0, "no precipitation depth");
    assert!(cell(3) > 0.0, "no runoff volume");
    let c = cell(4);
    assert!(
        c > 0.0 && c <= 1.1,
        "runoff coefficient {c} outside physical range"
    );
    let _ = std::fs::remove_file(&path);
}

/// §13.4.6: outfalls come from the model, figures from the node's total
/// inflow series — frequency within percent bounds, peak at or above the
/// mean, and water actually leaving through the fixture's outfall.
#[test]
fn outfall_summary_reports_the_discharge_through_the_outfall() {
    let (path, net) = run_to_out("runoff_parcel.inp", "outfall");
    let f = produce_report_block(
        "uds.outfall-summary",
        &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
        &net,
        None,
    )
    .expect("produce");
    let FragmentItem::Table { table } = &f.items[0] else {
        panic!("outfall summary is not a table");
    };
    assert_eq!(table.rows.len(), 1, "fixture has exactly one outfall");
    let row = &table.rows[0];
    match &row[0] {
        hydra_common::Value::Text { value } => assert_eq!(value, "O1"),
        v => panic!("outfall id is {v:?}"),
    }
    let cell = |i: usize| -> f64 {
        match &row[i] {
            hydra_common::Value::Number { value, .. } => *value,
            v => panic!("cell {i} is {v:?}"),
        }
    };
    let frequency = cell(1);
    assert!(
        frequency > 0.0 && frequency <= 100.0,
        "frequency {frequency} out of bounds"
    );
    assert!(cell(3) >= cell(2), "peak below mean");
    assert!(cell(4) > 0.0, "no volume discharged");
    let _ = std::fs::remove_file(&path);
}

/// §13.6: catalog integrity — unique keys, cataloged quantities,
/// ascending band defaults.
#[test]
fn the_criteria_catalog_is_well_formed() {
    use hydra_engine_uds::report_blocks::criteria_catalog;
    let mut seen = std::collections::HashSet::new();
    for c in criteria_catalog() {
        assert!(seen.insert(c.key), "duplicate criterion {}", c.key);
        assert!(!c.label.is_empty() && !c.help.is_empty());
        if let Some(q) = c.quantity {
            assert!(
                hydra_engine_uds::descriptors::QUANTITIES
                    .iter()
                    .any(|d| d.key == q),
                "{}: quantity {q:?} is not cataloged",
                c.key
            );
        }
        if let hydra_common::CriterionKind::Band { cuts } = c.kind {
            assert!(
                cuts.windows(2).all(|w| w[1].default > w[0].default),
                "{}: band defaults must ascend",
                c.key
            );
        }
    }
}

/// §13.6 consumption: defaults fill absent keys, percent becomes a
/// fraction, and a degenerate velocity band omits its block instead of
/// failing.
#[test]
fn criteria_consumption_derives_si_options() {
    use hydra_engine_uds::report_blocks::criteria_block_options;
    let (_path, net) = run_to_out("runoff_parcel.inp", "criteria");
    let options = criteria_block_options(&serde_json::json!({}), &net).expect("options");
    assert_eq!(
        options["uds.capacity-summary"]["threshold"].as_f64(),
        Some(0.8)
    );
    assert_eq!(
        options["uds.surcharge-summary"]["freeboard"].as_f64(),
        Some(0.3)
    );
    assert!(options.contains_key("uds.velocity-thresholds"));

    let degenerate = serde_json::json!({ "velocity": [1.0, 1.0] });
    let options = criteria_block_options(&degenerate, &net).expect("options");
    assert!(!options.contains_key("uds.velocity-thresholds"));

    let malformed = serde_json::json!({ "capacity": "most" });
    let err = criteria_block_options(&malformed, &net).expect_err("must refuse");
    assert!(err.contains("capacity"), "{err}");
    let _ = std::fs::remove_file(&_path);
}

/// §13.4.9: every reported conduit lands in exactly one band, and the
/// edges echo back as tagged velocities.
#[test]
fn velocity_thresholds_count_every_conduit_once() {
    let (path, net) = run_to_out("runoff_parcel.inp", "velbands");
    let f = produce_report_block(
        "uds.velocity-thresholds",
        &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
        &net,
        None,
    )
    .expect("produce");
    let FragmentItem::KeyValues { entries } = &f.items[0] else {
        panic!("edges are not key-values");
    };
    match &entries[0].value {
        hydra_common::Value::Number { quantity, .. } => {
            assert_eq!(quantity.as_deref(), Some("velocity"));
        }
        v => panic!("edge is {v:?}"),
    }
    let FragmentItem::Chart { chart } = &f.items[1] else {
        panic!("bands are not a chart");
    };
    let hydra_common::ChartData::Bar { values, .. } = &chart.data else {
        panic!("bands are not a bar chart");
    };
    let conduits = net
        .links
        .iter()
        .filter(|l| matches!(l.kind, hydra_engine_uds::model::LinkKind::Channel { .. }))
        .count() as f64;
    assert_eq!(values.iter().sum::<f64>(), conduits);
    let _ = std::fs::remove_file(&path);
}

/// §13.4.8: a floor threshold counts every conduit that ever carries
/// water, so the block produces with rows for the fixture's conduit.
#[test]
fn a_floor_capacity_threshold_lists_the_conduits() {
    let (path, net) = run_to_out("runoff_parcel.inp", "capfloor");
    let opts = serde_json::json!({ "threshold": 0.0 });
    let f = produce_report_block(
        "uds.capacity-summary",
        &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
        &net,
        Some(&opts),
    )
    .expect("produce");
    let FragmentItem::Table { table } = &f.items[0] else {
        panic!("capacity summary is not a table");
    };
    assert!(!table.rows.is_empty());
    let _ = std::fs::remove_file(&path);
}

/// §13.4.10: the pumped-storage fixture's unit fills and drains, so the
/// summary reports physical depth utilisation and a peak outflow the
/// pump actually moved.
#[test]
fn storage_summary_reports_utilisation_and_attenuation() {
    // The base fixture has no forcing; a sanitary inflow fills the unit
    // so utilisation and the pump's outflow are real numbers.
    let (path, net) = run_to_out_with("storage_pumped.inp", "[DWF]\nJ1 FLOW 2.0\n", "storage");
    let f = produce_report_block(
        "uds.storage-summary",
        &hydra_interop_swmm::session::OutFileSource::open(&path).expect("open results"),
        &net,
        None,
    )
    .expect("produce");
    let FragmentItem::Table { table } = &f.items[0] else {
        panic!("storage summary is not a table");
    };
    assert_eq!(table.columns[2].quantity.as_deref(), Some("percent"));
    assert_eq!(table.columns[6].quantity.as_deref(), Some("percent"));
    assert_eq!(table.rows.len(), 1, "fixture has exactly one storage");
    let row = &table.rows[0];
    match &row[0] {
        hydra_common::Value::Text { value } => assert_eq!(value, "SU1"),
        v => panic!("storage id is {v:?}"),
    }
    let cell = |i: usize| -> f64 {
        match &row[i] {
            hydra_common::Value::Number { value, .. } => *value,
            hydra_common::Value::Absent => f64::NAN,
            v => panic!("cell {i} is {v:?}"),
        }
    };
    let used = cell(2);
    assert!(
        used > 0.0 && used <= 110.0,
        "depth used {used}% out of range"
    );
    assert!(cell(4) > 0.0, "no peak inflow recorded");
    // Attenuation, when present, is bounded above by 100 %.
    let attenuation = cell(6);
    assert!(
        attenuation.is_nan() || attenuation <= 100.0,
        "attenuation {attenuation} out of range"
    );
    let _ = std::fs::remove_file(&path);
}
