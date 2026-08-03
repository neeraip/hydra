//! The uds report blocks against real fixture runs: catalog integrity,
//! production from the results file, ranked-table options, and the
//! unavailable/unknown error contract.

use std::path::PathBuf;

use hydra_common::{BlockError, FragmentItem};
use hydra_engine_uds::io::objects::parse_network;
use hydra_engine_uds::report_blocks::{produce_report_block, report_block_options, report_catalog};
use hydra_engine_uds::simulation::Simulation;

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
    let text = format!(
        "{}\n[OPTIONS]\nSTART_DATE 01/01/2024\nSTART_TIME 00:00:00\n\
         END_DATE 01/01/2024\nEND_TIME 02:00:00\nREPORT_STEP 00:05:00\n\
         [REPORT]\nSUBCATCHMENTS ALL\nNODES ALL\nLINKS ALL\n",
        fixture(name)
    );
    let (mut sim, _diags, _findings) = Simulation::open(&text).expect("open");
    while sim.step() {}

    let mut path = std::env::temp_dir();
    path.push(format!(
        "hydra-uds-blocks-{tag}-{}-{}.out",
        name.trim_end_matches(".inp"),
        std::process::id()
    ));
    let mut w = std::io::BufWriter::new(std::fs::File::create(&path).expect("create"));
    sim.write_out(&mut w).expect("write out");
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
    }
}

#[test]
fn every_block_produces_or_declines_for_a_real_run() {
    let (path, net) = run_to_out("runoff_parcel.inp", "all");
    for b in report_catalog() {
        match produce_report_block(b.id, &path, &net, None) {
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
    let f = produce_report_block("uds.run-summary", &path, &net, None).expect("produce");
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
    let f = produce_report_block("uds.node-extremes", &path, &net, Some(&one)).expect("produce");
    let FragmentItem::Table { table } = &f.items[0] else {
        panic!("node extremes is not a table");
    };
    assert_eq!(table.rows.len(), 1);

    let bad = serde_json::json!({ "rows": "many" });
    let err = produce_report_block("uds.node-extremes", &path, &net, Some(&bad))
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
    let err = produce_report_block("uds.no-such-block", &path, &net, None)
        .expect_err("unknown id must be refused");
    assert!(matches!(err, BlockError::UnknownBlock { .. }));
    let _ = std::fs::remove_file(&path);
}
