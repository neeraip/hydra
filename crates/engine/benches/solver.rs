//! Criterion benchmarks for the hydraulic solver.
//!
//! Two scenarios are measured per network:
//!
//! - **cold**: every iteration starts from the network's initial conditions
//!   (initial flow guesses, initial node heads).  This exercises the full
//!   Newton-Raphson convergence path.
//!
//! - **warm**: every iteration starts from the already-converged state
//!   produced by the previous call.  The solver typically converges in 1–2
//!   iterations, representing the steady-state cost of each hydraulic
//!   timestep in a multi-period simulation.
//!
//! Networks benchmarked (ascending link count):
//!
//! | Name      | Junctions | Links  |
//! |-----------|----------:|-------:|
//! | ky10      |       920 |  1,061 |
//! | exnet     |     1,891 |  2,467 |
//! | ky9       |     1,242 |  1,343 |
//! | ky8       |     1,325 |  1,618 |
//! | micropolis|     1,574 |  1,619 |
//! | bwsn2     |    12,523 | 14,831 |

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use hydra_engine::{build_solver_context, solve_hydraulic_step};
use hydra_engine::{LinkKind, LinkState, LinkStatus, Network, NodeKind, NodeState};
use std::path::PathBuf;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root not found")
}

/// No-op pswitch: no pressure-dependent simple controls in these networks.
fn no_pswitch(
    _network: &Network,
    _node_states: &[NodeState],
    _statuses: &mut [LinkStatus],
    _settings: &mut [f64],
) -> bool {
    false
}

/// Initialise per-node state from static network data.
///
/// Mirrors `init_node_states` in `crates/simulation/src/engine.rs`.
fn init_node_states(network: &Network) -> Vec<NodeState> {
    network
        .nodes
        .iter()
        .map(|n| {
            let head = match &n.kind {
                NodeKind::Junction(_) => 0.0,
                NodeKind::Reservoir(_) => n.base.elevation,
                NodeKind::Tank(t) => t.head_from_level(n.base.elevation, t.initial_level),
            };
            let level = match &n.kind {
                NodeKind::Tank(t) => t.initial_level,
                _ => 0.0,
            };
            NodeState {
                head,
                level,
                ..NodeState::default()
            }
        })
        .collect()
}

/// Initialise per-link state from static network data.
///
/// Mirrors `init_link_states` in `crates/simulation/src/engine.rs`.
fn init_link_states(network: &Network) -> Vec<LinkState> {
    network
        .links
        .iter()
        .map(|l| {
            let status = l.base.initial_status;
            let setting = l.base.initial_setting.unwrap_or(f64::NAN);
            let flow = if status == LinkStatus::Closed {
                1.0e-6
            } else {
                match &l.kind {
                    LinkKind::Pipe(p) => std::f64::consts::PI * p.diameter * p.diameter / 4.0,
                    LinkKind::Pump(_) => 1.0,
                    LinkKind::Valve(v) => std::f64::consts::PI * v.diameter * v.diameter / 4.0,
                }
            };
            LinkState {
                flow,
                status,
                setting,
                quality: 0.0,
                reaction_rate: 0.0,
            }
        })
        .collect()
}

// ── Per-network benchmark ─────────────────────────────────────────────────────

fn bench_network(c: &mut Criterion, name: &str) {
    let inp = workspace_root().join(format!("tests/benchmarks/{name}.inp"));
    let bytes = match std::fs::read(&inp) {
        Ok(b) => b,
        Err(_) => {
            eprintln!("bench: skipping {name} (file not found: {})", inp.display());
            return;
        }
    };

    let network =
        hydra_engine::io::parse(&bytes).unwrap_or_else(|e| panic!("parse failed for {name}: {e}"));
    let favad = network.compute_favad();

    let init_nodes = init_node_states(&network);
    let init_links = init_link_states(&network);

    // Build a single SolverContext — it is reused across benchmark iterations
    // (same as production: built once per session, reused per timestep).
    let mut ctx = build_solver_context(&network, &favad)
        .unwrap_or_else(|e| panic!("context failed for {name}: {e}"));

    // Prime the warm state: one full solve to convergence.
    let mut warm_nodes = init_nodes.clone();
    let mut warm_links = init_links.clone();
    solve_hydraulic_step(
        &network,
        &favad,
        &mut ctx,
        &mut warm_nodes,
        &mut warm_links,
        0.0,
        no_pswitch,
    )
    .unwrap_or_else(|e| panic!("warmup failed for {name}: {e}"));

    let mut group = c.benchmark_group(name);

    // ── Warm solve ────────────────────────────────────────────────────────────
    // Measures steady-state per-timestep cost (converged initial guess).
    // `ctx.flows` and the states all start at the converged solution; the
    // solver typically needs just 1–2 Newton iterations.
    group.bench_function("warm", |b| {
        b.iter(|| {
            black_box(
                solve_hydraulic_step(
                    &network,
                    &favad,
                    &mut ctx,
                    &mut warm_nodes,
                    &mut warm_links,
                    0.0,
                    no_pswitch,
                )
                .unwrap(),
            )
        })
    });

    // ── Cold solve ────────────────────────────────────────────────────────────
    // Measures full cold-start convergence cost.
    // Each iteration clones fresh initial node/link state before the call,
    // so the solver always starts from the network's un-converged initial
    // conditions.  ctx.flows is overwritten from `link_states.flow` at the
    // start of every `solve_hydraulic_step` call, so the internal flow vector
    // is also reset to the initial guesses on each iteration.
    group.bench_function("cold", |b| {
        b.iter_batched(
            || (init_nodes.clone(), init_links.clone()),
            |(mut ns, mut ls)| {
                black_box(
                    solve_hydraulic_step(
                        &network, &favad, &mut ctx, &mut ns, &mut ls, 0.0, no_pswitch,
                    )
                    .unwrap(),
                )
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ── Benchmark groups ──────────────────────────────────────────────────────────

fn benchmark_solver(c: &mut Criterion) {
    for name in &["ky10", "exnet", "ky8", "ky9", "micropolis", "bwsn2"] {
        bench_network(c, name);
    }
}

criterion_group!(benches, benchmark_solver);
criterion_main!(benches);
