//! The §14.9 binary-results reader against the engine's own writer: what a
//! run writes, the reader must locate, validate, and serve back — metadata,
//! whole periods, and per-element series must all agree.

use std::path::PathBuf;

use hydra_interop_swmm::out_reader::{
    read_element_series, read_metadata, read_period, ElementKind,
};

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
    let (mut sim, _diags, _findings) = hydra_interop_swmm::session::open(&text).expect("open");
    while sim.step() {}

    // Unique per call, not per fixture: cargo runs these tests in parallel
    // threads of one process, so a path keyed by fixture name and pid is
    // shared by every test using the same fixture — and `File::create`
    // truncates, so one test's write empties the file another is midway
    // through reading ("failed to fill whole buffer").
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "hydra-uds-roundtrip-{}-{}-{}.out",
        name.trim_end_matches(".inp"),
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
    let mut w = std::io::BufWriter::new(std::fs::File::create(&path).expect("create"));
    hydra_interop_swmm::session::write_out(&sim, &mut w).expect("write out");
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

/// The §14.9 static property tables, decoded from the bytes a run writes.
///
/// The reader serves results, not properties, so a test that wants to know
/// what a consumer of the file would read has to decode them itself.
struct Properties {
    node_ids: Vec<String>,
    link_ids: Vec<String>,
    /// Per node: (type code, invert, max depth).
    nodes: Vec<(i32, f32, f32)>,
    /// Per link: (type code, offset1, offset2, full depth, length).
    links: Vec<(i32, f32, f32, f32, f32)>,
}

impl Properties {
    fn decode(b: &[u8]) -> Properties {
        let i32_at = |p: usize| i32::from_le_bytes(b[p..p + 4].try_into().unwrap());
        let f32_at = |p: usize| f32::from_le_bytes(b[p..p + 4].try_into().unwrap());
        let (ns, nn, nl, npol) = (
            i32_at(12) as usize,
            i32_at(16) as usize,
            i32_at(20) as usize,
            i32_at(24) as usize,
        );
        let mut p = 28;
        let mut ids = Vec::new();
        for _ in 0..ns + nn + nl + npol {
            let n = i32_at(p) as usize;
            p += 4;
            ids.push(String::from_utf8(b[p..p + n].to_vec()).expect("ascii id"));
            p += n;
        }
        p += 4 * npol; // pollutant unit codes
        let skip_table = |p: &mut usize| {
            let count = i32_at(*p) as usize;
            *p += 4 + 4 * count;
        };
        skip_table(&mut p);
        p += 4 * ns; // subcatchment areas
        skip_table(&mut p);
        let mut nodes = Vec::with_capacity(nn);
        for _ in 0..nn {
            nodes.push((i32_at(p), f32_at(p + 4), f32_at(p + 8)));
            p += 12;
        }
        skip_table(&mut p);
        let mut links = Vec::with_capacity(nl);
        for _ in 0..nl {
            links.push((
                i32_at(p),
                f32_at(p + 4),
                f32_at(p + 8),
                f32_at(p + 12),
                f32_at(p + 16),
            ));
            p += 20;
        }
        Properties {
            node_ids: ids[ns..ns + nn].to_vec(),
            link_ids: ids[ns + nn..ns + nn + nl].to_vec(),
            nodes,
            links,
        }
    }
    fn node(&self, id: &str) -> (i32, f32, f32) {
        let i = self.node_ids.iter().position(|x| x == id).expect("node");
        self.nodes[i]
    }
    fn link(&self, id: &str) -> (i32, f32, f32, f32, f32) {
        let i = self.link_ids.iter().position(|x| x == id).expect("link");
        self.links[i]
    }
}

fn properties_of(model: &str) -> Properties {
    let (mut sim, diags, _) = hydra_interop_swmm::session::open(model).expect("open");
    assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
    while sim.step() {}
    let mut buf = Vec::new();
    hydra_interop_swmm::session::write_out(&sim, &mut buf).expect("write");
    Properties::decode(&buf)
}

/// A junction, an outfall, one conduit, and one regulator of each kind
/// sharing an offset well above their vertices' inverts.
const REGULATOR_MODEL: &str = "\
[OPTIONS]
FLOW_UNITS    CMS
FLOW_ROUTING  DYNWAVE
START_DATE    01/15/2024
START_TIME    00:00
END_DATE      01/15/2024
END_TIME      00:20
ROUTING_STEP  5
REPORT_STEP   0:05:00

[JUNCTIONS]
J1  10  4  0  0  0
J2  10  4  0  0  0

[OUTFALLS]
O1  8  FREE
O2  8  FREE
O3  8  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[WEIRS]
W1  J1  O2  TRANSVERSE  1.25  1.7  NO  0  0  NO

[ORIFICES]
R1  J2  O3  SIDE  0.75  0.65  NO  0

[XSECTIONS]
C1  CIRCULAR     1.5  0    0  0
W1  RECT_OPEN    2.0  3.0  0  0
R1  RECT_CLOSED  0.6  0.6  0  0

[REPORT]
NODES  ALL
LINKS  ALL
";

#[test]
fn a_regulator_writes_its_one_offset_into_both_columns() {
    // §14.9: the predecessor mirrors offset1 into offset2 for orifices,
    // weirs and outlets (link.c:366 and :375). Writing zero downstream
    // places every regulator at its downstream vertex's invert as far as
    // a reader of the file can tell.
    let p = properties_of(REGULATOR_MODEL);
    let (_, w1, w2, ..) = p.link("W1");
    assert!(
        (w1 - 1.25).abs() < 1e-5 && (w2 - 1.25).abs() < 1e-5,
        "weir offsets {w1} and {w2}, both should be its crest"
    );
    let (_, r1, r2, ..) = p.link("R1");
    assert!(
        (r1 - 0.75).abs() < 1e-5 && (r2 - 0.75).abs() < 1e-5,
        "orifice offsets {r1} and {r2}, both should be its opening"
    );
    // A conduit keeps two genuinely independent offsets.
    let (_, c1, c2, ..) = p.link("C1");
    assert!((c1 - 0.0).abs() < 1e-5 && (c2 - 0.0).abs() < 1e-5);
}

#[test]
fn an_outfall_carries_the_crown_of_its_connecting_link() {
    // §14.7's crown raising exempts storage without a surcharge
    // allowance and nothing else, so a terminal vertex is raised like
    // any other. It could not be: the model had nowhere to put the
    // depth, so every outfall published zero and a reader asking how
    // full one was divided by it.
    let p = properties_of(REGULATOR_MODEL);
    // O1 sits below C1's crown: a 1.5 m circular pipe at zero offset.
    let (kind, _, depth) = p.node("O1");
    assert_eq!(kind, 1, "O1 must be written as an outfall");
    assert!(
        (depth - 1.5).abs() < 1e-5,
        "outfall depth {depth}, should be the conduit crown 1.5"
    );
    // A weir raises its upstream vertex only; the downstream one is
    // raised by conduits alone, so O2 stays at zero.
    let (_, _, o2) = p.node("O2");
    assert!((o2 - 0.0).abs() < 1e-5, "weir raised its outfall to {o2}");
}
