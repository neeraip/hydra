//! Golden-snapshot regression tests over real benchmark networks.
//!
//! Each test parses a network from `tests/benchmarks/` (workspace root), runs
//! the full simulation through the public session API, and compares a compact
//! result fingerprint — heads/flows (and quality where enabled) at fixed
//! object IDs and timesteps, plus network-wide sums — against hard-coded
//! golden values.
//!
//! **These goldens are Hydra-vs-Hydra regression anchors, NOT EPANET parity
//! values.** Per the crate-level correctness stance (`lib.rs`), agreement with
//! EPANET's numerical output is not a correctness criterion; the goldens were
//! produced by running the current Hydra code (baseline captured after the
//! 2026-07 solver bug-fixes) and exist only to detect unintended behavioural
//! drift in the solver, quality engine, or session plumbing.
//!
//! # Regenerating goldens
//!
//! After an *intentional* solver/behaviour change, regenerate with:
//!
//! ```text
//! cargo test -p hydra-engine-wds --test benchmark_snapshots \
//!     -- --ignored print_golden_fingerprints --nocapture
//! ```
//!
//! and paste the printed `GOLDEN_*` constants over the ones below.

use hydra_engine_wds::{io, LinkQuantity, NodeQuantity, QualityMode, Simulation};
use std::path::PathBuf;

/// Relative tolerance for all golden comparisons.
const REL_TOL: f64 = 1e-6;
/// Absolute floor: values whose golden magnitude is below this are compared
/// absolutely. 1e-6 m³/s is the solver's `Q_CLOSED` convergence-noise level,
/// so relative comparison below it would be meaningless.
const ABS_FLOOR: f64 = 1e-6;

fn benchmark_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("tests/benchmarks")
        .join(format!("{name}.inp"))
}

/// Parse and fully run a benchmark network (hydraulics + quality).
/// Returns `None` when the fixture is absent (e.g. packaged crate builds).
fn run_benchmark(name: &str) -> Option<Simulation> {
    let path = benchmark_path(name);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            eprintln!("skipping {name}: fixture not found at {}", path.display());
            return None;
        }
    };
    let network = io::parse(&bytes).unwrap_or_else(|e| panic!("parse {name}: {e}"));
    let mut sim = Simulation::from_network(network).unwrap_or_else(|e| panic!("load {name}: {e}"));
    sim.run().unwrap_or_else(|e| panic!("run {name}: {e}"));
    Some(sim)
}

/// Sample times: first, middle, and last recorded snapshot.
fn sample_times(sim: &Simulation) -> Vec<f64> {
    let times = sim.snapshot_times();
    assert!(!times.is_empty(), "no snapshots recorded");
    let mut picks = vec![times[0]];
    if times.len() > 2 {
        picks.push(times[times.len() / 2]);
    }
    if times.len() > 1 {
        picks.push(*times.last().expect("non-empty"));
    }
    picks
}

/// Evenly spread sample indices: first, quartiles, last.
fn sample_indices(len: usize) -> Vec<usize> {
    assert!(len > 0);
    let mut idx = vec![0, len / 4, len / 2, (3 * len) / 4, len - 1];
    idx.dedup();
    idx
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    if expected.abs() < ABS_FLOOR {
        assert!(
            (actual - expected).abs() < ABS_FLOOR,
            "{what}: actual {actual:e} vs golden {expected:e} (abs)"
        );
    } else {
        let rel = ((actual - expected) / expected).abs();
        assert!(
            rel < REL_TOL,
            "{what}: actual {actual:.12e} vs golden {expected:.12e} (rel err {rel:.3e})"
        );
    }
}

/// Network-wide sums at the final snapshot: (Σ head over all nodes, Σ |flow|
/// over all links).
fn network_sums(sim: &Simulation) -> (f64, f64) {
    let times = sample_times(sim);
    let t_final = *times.last().expect("non-empty");
    let head_sum: f64 = sim
        .all_node_results_at(t_final)
        .expect("node results")
        .iter()
        .map(|r| r.head)
        .sum();
    let abs_flow_sum: f64 = sim
        .all_link_results_at(t_final)
        .expect("link results")
        .iter()
        .map(|r| r.flow.abs())
        .sum();
    (head_sum, abs_flow_sum)
}

fn check_against_golden(
    sim: &Simulation,
    golden_nodes: &[(&str, f64, f64, f64)],
    golden_links: &[(&str, f64, f64)],
    golden_sums: (f64, f64),
    quality_mode: QualityMode,
) {
    for &(id, t, head, quality) in golden_nodes {
        let actual_head = sim
            .get_node_result(id, NodeQuantity::Head, t)
            .unwrap_or_else(|e| panic!("head {id}@{t}: {e}"));
        assert_close(actual_head, head, &format!("head {id}@{t}"));
        if quality_mode != QualityMode::None {
            let actual_q = sim
                .get_node_result(id, NodeQuantity::Quality, t)
                .unwrap_or_else(|e| panic!("quality {id}@{t}: {e}"));
            assert_close(actual_q, quality, &format!("quality {id}@{t}"));
        }
    }
    for &(id, t, flow) in golden_links {
        let actual_flow = sim
            .get_link_result(id, LinkQuantity::Flow, t)
            .unwrap_or_else(|e| panic!("flow {id}@{t}: {e}"));
        assert_close(actual_flow, flow, &format!("flow {id}@{t}"));
    }

    let (head_sum, abs_flow_sum) = network_sums(sim);
    assert_close(head_sum, golden_sums.0, "Σ head at t_final");
    assert_close(abs_flow_sum, golden_sums.1, "Σ |flow| at t_final");
}

// ── Golden values (regenerate via `print_golden_fingerprints`, see above) ────

// nytunnels: 19 junctions / 42 pipes, CFS, H-W, 119 h EPS, chlorine quality.
const GOLDEN_NYTUNNELS_NODES: &[(&str, f64, f64, f64)] = &[
    ("2", 0.0, 91.32301329181222, 0.0),
    ("2", 216000.0, 91.19987804450477, 0.46073430301223206),
    ("2", 428400.0, 91.03012799346143, 0.461467494863194),
    ("7", 0.0, 90.96319346021717, 0.0),
    ("7", 216000.0, 90.46489298821008, 0.25261956267567365),
    ("7", 428400.0, 89.77795293415367, 0.25852806208174806),
    ("12", 0.0, 90.92108671720685, 0.0),
    ("12", 216000.0, 90.37888341983268, 0.27389536197372727),
    ("12", 428400.0, 89.63142065371933, 0.2900158467041196),
    ("17", 0.0, 90.89443376402265, 0.0),
    ("17", 216000.0, 90.32444116368772, 3.463535236514e-5),
    ("17", 428400.0, 89.5386687879048, 0.011186725845519876),
    ("1", 0.0, 91.44111192392099, 0.5),
    ("1", 216000.0, 91.44111192392099, 0.5),
    ("1", 428400.0, 91.44111192392099, 0.5),
];
const GOLDEN_NYTUNNELS_LINKS: &[(&str, f64, f64)] = &[
    ("1", 0.0, 8.072310409130516),
    ("1", 216000.0, 11.87104383343551),
    ("1", 428400.0, 15.828058444570622),
    ("11", 0.0, -2.911059908347934),
    ("11", 216000.0, -4.280971215070513),
    ("11", 428400.0, -5.707961620089466),
    ("22", 0.0, 5.808047474905461),
    ("22", 216000.0, 8.541245649237071),
    ("22", 428400.0, 11.38832753230886),
    ("32", 0.0, -2.911059908347934),
    ("32", 216000.0, -4.280971215070513),
    ("32", 428400.0, -5.707961620089466),
    ("42", 0.0, -0.12906262844515548),
    ("42", 216000.0, -0.18852505521995785),
    ("42", 428400.0, -0.2513667402931906),
];
const GOLDEN_NYTUNNELS_SUMS: (f64, f64) = (1799.5215944030185, 285.5696826150175);

// balerma: 443 junctions / 454 pipes, LPS, single-period, quality None.
const GOLDEN_BALERMA_NODES: &[(&str, f64, f64, f64)] = &[
    ("179001", 0.0, 80.16273719795774, 0.0),
    ("42", 0.0, 62.62067653776446, 0.0),
    ("266", 0.0, 116.95546564348216, 0.0),
    ("334", 0.0, 106.59556729273501, 0.0),
    ("88", 0.0, 112.0, 0.0),
];
const GOLDEN_BALERMA_LINKS: &[(&str, f64, f64)] = &[
    ("1", 0.0, -0.0024975000000001116),
    ("155", 0.0, -0.07133361378686695),
    ("361", 0.0, 0.004995000000000133),
    ("548", 0.0, 0.004995000000000095),
    ("5", 0.0, -0.0013319399974312156),
];
const GOLDEN_BALERMA_SUMS: (f64, f64) = (40112.147462634064, 11.418886099446283);

// richmond: 865 junctions, pumps/tanks/controls, LPS, 24 h EPS, quality None.
const GOLDEN_RICHMOND_NODES: &[(&str, f64, f64, f64)] = &[
    ("1", 0.0, 70.32146282519929, 0.0),
    ("1", 43200.0, 172.95086410166832, 0.0),
    ("1", 86400.0, 172.86512513275122, 0.0),
    ("219", 0.0, 187.24677896061854, 0.0),
    ("219", 43200.0, 190.11652753666982, 0.0),
    ("219", 86400.0, 190.01521363259732, 0.0),
    ("440", 0.0, 217.00846692626945, 0.0),
    ("440", 43200.0, 216.54839137557613, 0.0),
    ("440", 86400.0, 217.088016565805, 0.0),
    ("660", 0.0, 260.47355885498513, 0.0),
    ("660", 43200.0, 259.9586372769445, 0.0),
    ("660", 86400.0, 260.1544589887927, 0.0),
    ("F", 0.0, 237.67000000000002, 0.0),
    ("F", 43200.0, 237.58826823713892, 0.0),
    ("F", 86400.0, 237.60327647330672, 0.0),
];
const GOLDEN_RICHMOND_LINKS: &[(&str, f64, f64)] = &[
    ("785", 0.0, 0.015214678137398345),
    ("785", 43200.0, 0.02773992439292631),
    ("785", 86400.0, 0.015214854545880674),
    ("1047", 0.0, 0.00013284217055210574),
    ("1047", 43200.0, 0.00016193614979707042),
    ("1047", 86400.0, 0.00013285198032206833),
    ("1337", 0.0, 0.0),
    ("1337", 43200.0, -1.3050878227738926e-12),
    ("1337", 86400.0, 4.9538728522799995e-8),
    ("1601", 0.0, -5.263700586510823e-8),
    ("1601", 43200.0, 1.739825885330895e-11),
    ("1601", 86400.0, 5.882766652273501e-11),
    ("v1708", 0.0, 9.225422199256496e-5),
    ("v1708", 43200.0, 0.0001965849706960687),
    ("v1708", 86400.0, 9.214640147859395e-5),
];
const GOLDEN_RICHMOND_SUMS: (f64, f64) = (183173.11126274167, 2.9263657101967397);

// ── Regression tests ─────────────────────────────────────────────────────────

#[test]
fn nytunnels_matches_golden_fingerprint() {
    let Some(sim) = run_benchmark("nytunnels") else {
        return;
    };
    check_against_golden(
        &sim,
        GOLDEN_NYTUNNELS_NODES,
        GOLDEN_NYTUNNELS_LINKS,
        GOLDEN_NYTUNNELS_SUMS,
        QualityMode::Chemical,
    );
}

#[test]
fn balerma_matches_golden_fingerprint() {
    let Some(sim) = run_benchmark("balerma") else {
        return;
    };
    check_against_golden(
        &sim,
        GOLDEN_BALERMA_NODES,
        GOLDEN_BALERMA_LINKS,
        GOLDEN_BALERMA_SUMS,
        QualityMode::None,
    );
}

#[test]
fn richmond_matches_golden_fingerprint() {
    let Some(sim) = run_benchmark("richmond") else {
        return;
    };
    check_against_golden(
        &sim,
        GOLDEN_RICHMOND_NODES,
        GOLDEN_RICHMOND_LINKS,
        GOLDEN_RICHMOND_SUMS,
        QualityMode::None,
    );
}

// ── Golden generator ─────────────────────────────────────────────────────────

/// Prints fingerprints in paste-ready Rust syntax. Run with:
/// `cargo test -p hydra-engine-wds --test benchmark_snapshots -- --ignored print_golden_fingerprints --nocapture`
#[test]
#[ignore = "golden generator, not a regression test"]
fn print_golden_fingerprints() {
    for name in ["nytunnels", "balerma", "richmond"] {
        let start = std::time::Instant::now();
        let Some(sim) = run_benchmark(name) else {
            continue;
        };
        let times = sample_times(&sim);
        let node_ids: Vec<String> = sim.node_ids().iter().map(|s| s.to_string()).collect();
        let link_ids: Vec<String> = sim.link_ids().iter().map(|s| s.to_string()).collect();

        println!("// ── {name} (ran in {:?}) ──", start.elapsed());
        println!(
            "const GOLDEN_{}_NODES: &[(&str, f64, f64, f64)] = &[",
            name.to_uppercase()
        );
        for &i in &sample_indices(node_ids.len()) {
            for &t in &times {
                let head = sim
                    .get_node_result(&node_ids[i], NodeQuantity::Head, t)
                    .expect("head");
                let quality = sim
                    .get_node_result(&node_ids[i], NodeQuantity::Quality, t)
                    .expect("quality");
                println!("    ({:?}, {t:?}, {head:?}, {quality:?}),", node_ids[i]);
            }
        }
        println!("];");
        println!(
            "const GOLDEN_{}_LINKS: &[(&str, f64, f64)] = &[",
            name.to_uppercase()
        );
        for &i in &sample_indices(link_ids.len()) {
            for &t in &times {
                let flow = sim
                    .get_link_result(&link_ids[i], LinkQuantity::Flow, t)
                    .expect("flow");
                println!("    ({:?}, {t:?}, {flow:?}),", link_ids[i]);
            }
        }
        println!("];");
        let sums = network_sums(&sim);
        println!(
            "const GOLDEN_{}_SUMS: (f64, f64) = ({:?}, {:?});",
            name.to_uppercase(),
            sums.0,
            sums.1
        );
        println!();
    }
}
