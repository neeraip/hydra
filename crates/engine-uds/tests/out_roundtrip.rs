//! The §14.9 binary-results reader against the engine's own writer: what a
//! run writes, the reader must locate, validate, and serve back — metadata,
//! whole periods, and per-element series must all agree.

use std::path::PathBuf;

use hydra_engine_uds::io::out_reader::{
    read_element_series, read_metadata, read_period, ElementKind,
};
use hydra_engine_uds::simulation::Simulation;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/uds")
        .join(name);
    std::fs::read_to_string(path).expect("read fixture")
}

/// Run a fixture with every object reported over a two-hour horizon and
/// write its results file. (Fixtures pin parse/build behaviour and mostly
/// declare no clock of their own.)
fn run_to_out(name: &str) -> PathBuf {
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
        "hydra-uds-roundtrip-{}-{}.out",
        name.trim_end_matches(".inp"),
        std::process::id()
    ));
    let mut w = std::io::BufWriter::new(std::fs::File::create(&path).expect("create"));
    sim.write_out(&mut w).expect("write out");
    use std::io::Write as _;
    w.flush().expect("flush");
    path
}

#[test]
fn metadata_periods_and_series_agree_with_the_writer() {
    let path = run_to_out("single_conduit.inp");
    let meta = read_metadata(&path).expect("metadata");

    assert!(meta.n_periods > 0, "run produced no periods");
    assert!(!meta.node_ids.is_empty(), "no nodes reported");
    assert!(!meta.link_ids.is_empty(), "no links reported");
    assert!(meta.report_step_s > 0);
    assert_eq!(meta.n_node_vars, 6 + meta.pollutant_ids.len());
    assert_eq!(meta.n_link_vars, 5 + meta.pollutant_ids.len());

    // Every period record carries its own true timestamp, matching the
    // clock the metadata reconstructs from the backdated header (§14.9).
    let first = read_period(&path, &meta, 0).expect("first period");
    let last = read_period(&path, &meta, meta.n_periods - 1).expect("last period");
    assert!((first.epoch_s - meta.period_epoch_s(0)).abs() < 1.0);
    assert!(
        (last.epoch_s - meta.period_epoch_s(meta.n_periods - 1)).abs() < 1.0,
        "last record time drifted from the metadata clock"
    );
    assert_eq!(first.nodes.len(), meta.node_ids.len() * meta.n_node_vars);
    assert_eq!(first.links.len(), meta.link_ids.len() * meta.n_link_vars);

    // A link's series must equal the same values sliced out of the whole
    // periods — one addressing scheme, verified against the other.
    let series = read_element_series(&path, &meta, ElementKind::Link, 0).expect("series");
    assert_eq!(series.epochs_s.len(), meta.n_periods);
    assert_eq!(series.vars.len(), meta.n_link_vars);
    for (p, want) in [(0usize, &first), (meta.n_periods - 1, &last)] {
        for v in 0..meta.n_link_vars {
            assert_eq!(
                series.vars[v][p], want.links[v],
                "period {p} var {v} disagrees between series and period reads"
            );
        }
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn subcatchment_series_round_trip() {
    let path = run_to_out("runoff_parcel.inp");
    let meta = read_metadata(&path).expect("metadata");
    assert!(
        !meta.subcatchment_ids.is_empty(),
        "no subcatchments reported"
    );
    assert_eq!(meta.n_subcatch_vars, 8 + meta.pollutant_ids.len());

    let series = read_element_series(&path, &meta, ElementKind::Subcatchment, 0).expect("series");
    let mid = meta.n_periods / 2;
    let period = read_period(&path, &meta, mid).expect("period");
    for v in 0..meta.n_subcatch_vars {
        assert_eq!(series.vars[v][mid], period.subcatchments[v]);
    }
    // The rain series (variable 0) should not be identically zero in a
    // runoff fixture — a guard against reading the wrong offsets.
    assert!(
        series.vars[0].iter().any(|r| *r > 0.0),
        "rainfall series is all zero"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_truncated_or_corrupted_file_is_refused_by_name() {
    let path = run_to_out("single_conduit.inp");
    let bytes = std::fs::read(&path).expect("read back");

    // Truncation breaks the epilog geometry.
    let mut short = std::env::temp_dir();
    short.push(format!(
        "hydra-uds-roundtrip-short-{}.out",
        std::process::id()
    ));
    std::fs::write(&short, &bytes[..bytes.len() - 10]).expect("write");
    let err = read_metadata(&short).expect_err("truncated file must be refused");
    assert!(
        err.contains("magic") || err.contains("length") || err.contains("fit"),
        "unhelpful refusal: {err}"
    );

    // A recorded error code is a refusal even when the geometry is fine.
    let mut errored = bytes.clone();
    let at = bytes.len() - 8;
    errored[at..at + 4].copy_from_slice(&7i32.to_le_bytes());
    let mut bad = std::env::temp_dir();
    bad.push(format!(
        "hydra-uds-roundtrip-err-{}.out",
        std::process::id()
    ));
    std::fs::write(&bad, &errored).expect("write");
    let err = read_metadata(&bad).expect_err("errored run must be refused");
    assert!(err.contains("error code 7"), "unhelpful refusal: {err}");

    for p in [&path, &short, &bad] {
        let _ = std::fs::remove_file(p);
    }
}
