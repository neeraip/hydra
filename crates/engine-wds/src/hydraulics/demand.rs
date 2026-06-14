// demand — §3.3.1 emitter, §3.3.2 PDA, §3.3.3 FAVAD leakage helpers

use crate::{FavadCoeffs, Network, NodeKind};

use super::shared::{lower_barrier, upper_barrier, G_MIN};
use super::SparseSolver;

/// Leakage secondary-convergence tolerance (§3.8): 2.83e-6 m³/s (= 1e-4 ft³/s ≈ 0.2 lpm).
const Q_LEAK_TOL: f64 = 2.83e-6;

// ═══════════════════════════════════════════════════════════════════════════════
// §3.3.1 — Emitter coefficients
// ═══════════════════════════════════════════════════════════════════════════════

/// Computes emitter head loss and gradient for junction `i` (§3.3.1).
///
/// Uses the inverted power-law: `h = Ke * |Q|^Qexp` where `Qexp = 1/n_e`.
/// Matches EPANET `emitterheadloss()`.
fn emitter_headloss(ke: f64, qexp: f64, q: f64, emit_backflow: bool, rq_tol: f64) -> (f64, f64) {
    let ke = ke.max(G_MIN);
    let mut hgrad = qexp * ke * q.abs().powf(qexp - 1.0);
    let mut hloss;
    if hgrad < rq_tol {
        hgrad = rq_tol / qexp;
        hloss = hgrad * q;
    } else {
        hloss = hgrad * q / qexp;
    }
    if !emit_backflow {
        // Lower barrier: enforce Q_e >= 0 (§3.3.4)
        let a = 1.0e9 * q;
        let b = (a * a + 1.0e-6).sqrt();
        hloss += (a - b) / 2.0;
        hgrad += 1.0e9 / 2.0 * (1.0 - a / b);
    }
    (hloss, hgrad)
}

/// Assembles emitter coefficient contributions into the sparse matrix (§3.3.1 + §3.4).
///
/// For each emitter node: adds diagonal and RHS terms, subtracts emitter flow from Xflow.
/// Matches EPANET `emittercoeffs()`.
pub(super) fn apply_emitter_coeffs(
    network: &Network,
    sparse: &mut SparseSolver,
    node_junc_step_opt: &[Option<usize>],
    emitter_indices: &[usize],
    emitter_flows: &[f64],
    xflow: &mut [f64],
) {
    if emitter_indices.is_empty() {
        return;
    }
    let emit_backflow = network.options.emitter_backflow;
    let rq_tol = network.options.rq_tol;
    for &i in emitter_indices {
        let node = &network.nodes[i];
        let NodeKind::Junction(j) = &node.kind else {
            continue;
        };
        let Some(ji) = node_junc_step_opt[i] else {
            continue;
        };

        let qexp = 1.0 / j.emitter_exp;
        let flow = emitter_flows[i];
        let (hloss, hgrad) = emitter_headloss(j.emitter_coeff, qexp, flow, emit_backflow, rq_tol);

        let pr = sparse.row[ji];
        sparse.aii[pr] += 1.0 / hgrad;
        sparse.f[pr] += (hloss + node.base.elevation) / hgrad;

        xflow[i] -= emitter_flows[i];
    }
}

/// Updates emitter flows after head solve (§3.7).
///
/// Returns (sum_abs_flow, sum_abs_change) for convergence tracking.
/// Matches EPANET `newemitterflows()`.
pub(super) fn update_emitter_flows(
    network: &Network,
    node_heads: &[f64],
    emitter_indices: &[usize],
    emitter_flows: &mut [f64],
) -> (f64, f64) {
    if emitter_indices.is_empty() {
        return (0.0, 0.0);
    }
    let emit_backflow = network.options.emitter_backflow;
    let rq_tol = network.options.rq_tol;
    let mut qsum = 0.0f64;
    let mut dqsum = 0.0f64;

    for &i in emitter_indices {
        let node = &network.nodes[i];
        let NodeKind::Junction(j) = &node.kind else {
            continue;
        };

        let qexp = 1.0 / j.emitter_exp;
        let flow = emitter_flows[i];
        let (hloss, hgrad) = emitter_headloss(j.emitter_coeff, qexp, flow, emit_backflow, rq_tol);

        let pressure_head = node_heads[i] - node.base.elevation;
        let dq = (hloss - pressure_head) / hgrad;
        emitter_flows[i] -= dq;

        qsum += emitter_flows[i].abs();
        dqsum += dq.abs();
    }

    (qsum, dqsum)
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.3.3 — FAVAD leakage coefficients
// ═══════════════════════════════════════════════════════════════════════════════

/// Assembles FAVAD leakage diagonal/RHS contributions into the sparse matrix
/// (§3.3.3 + §3.4) and subtracts leakage flows from xflow.
///
/// For each junction with nonzero c_fa or c_va, computes the linearised
/// gradient using the inverted power-law form, applies a lower barrier to
/// enforce non-negative leakage, then adds 1/g to the diagonal and
/// (h + z_i)/g to the RHS (matching the emitter pattern from §3.3.1).
pub(super) fn apply_favad_leakage_coeffs(
    network: &Network,
    sparse: &mut SparseSolver,
    node_junc_step_opt: &[Option<usize>],
    favad: &FavadCoeffs,
    favad_indices: &[usize],
    leak_fa: &[f64],
    leak_va: &[f64],
    xflow: &mut [f64],
) {
    if favad_indices.is_empty() {
        return;
    }
    for &i in favad_indices {
        let node = &network.nodes[i];
        let Some(ji) = node_junc_step_opt[i] else {
            continue;
        };
        let pr = sparse.row[ji];
        let z = node.base.elevation;

        // Fixed-area component
        if favad.c_fa[i] > 0.0 {
            let flow = leak_fa[i];
            let abs_q = flow.abs().max(1.0e-12);
            // g_fa = 2 * c_fa * |q_fa|  (§3.3.3, 1/n = 2)
            let mut hgrad = 2.0 * favad.c_fa[i] * abs_q;
            let mut hloss = hgrad * flow / 2.0; // h = g * q * n = g * q / (1/n) where n = 0.5
                                                // Lower barrier: enforce q >= 0 (§3.3.4)
            let (barrier_head, barrier_grad) = lower_barrier(flow);
            hloss += barrier_head;
            hgrad += barrier_grad;
            if hgrad > G_MIN {
                sparse.aii[pr] += 1.0 / hgrad;
                sparse.f[pr] += (hloss + z) / hgrad;
            }
            xflow[i] -= leak_fa[i];
        }

        // Variable-area component
        if favad.c_va[i] > 0.0 {
            let flow = leak_va[i];
            let abs_q = flow.abs().max(1.0e-12);
            // g_va = (2/3) * c_va * |q_va|^(-1/3)  (§3.3.3, 1/n = 2/3)
            let mut hgrad = (2.0 / 3.0) * favad.c_va[i] / abs_q.cbrt();
            // h = g * q * n = g * q * 1.5 = g * q / (2/3)
            let mut hloss = hgrad * flow * 1.5;
            // Lower barrier: enforce q >= 0 (§3.3.4)
            let (barrier_head, barrier_grad) = lower_barrier(flow);
            hloss += barrier_head;
            hgrad += barrier_grad;
            if hgrad > G_MIN {
                sparse.aii[pr] += 1.0 / hgrad;
                sparse.f[pr] += (hloss + z) / hgrad;
            }
            xflow[i] -= leak_va[i];
        }
    }
}

/// Updates FAVAD leakage flows after head solve (§3.7).
///
/// Returns (sum_abs_flow, sum_abs_change) for convergence tracking.
pub(super) fn update_leakage_flows(
    network: &Network,
    node_heads: &[f64],
    favad: &FavadCoeffs,
    favad_indices: &[usize],
    leak_fa: &mut [f64],
    leak_va: &mut [f64],
) -> (f64, f64) {
    if favad_indices.is_empty() {
        return (0.0, 0.0);
    }
    let mut qsum = 0.0_f64;
    let mut dqsum = 0.0_f64;

    for &i in favad_indices {
        let node = &network.nodes[i];
        let z = node.base.elevation;
        let pressure_head = node_heads[i] - z;

        // Fixed-area: q_fa = P_fa * (H_i - z_i) + Y_fa (§3.7)
        if favad.c_fa[i] > 0.0 {
            let previous_flow = leak_fa[i];
            let abs_q = previous_flow.abs().max(1.0e-12);
            let mut hgrad = 2.0 * favad.c_fa[i] * abs_q;
            let mut hloss = hgrad * previous_flow / 2.0;
            let (barrier_head, barrier_grad) = lower_barrier(previous_flow);
            hloss += barrier_head;
            hgrad += barrier_grad;
            if hgrad > G_MIN {
                let dq = (hloss - pressure_head) / hgrad;
                leak_fa[i] = (leak_fa[i] - dq).max(0.0);
            }
            qsum += leak_fa[i].abs();
            dqsum += (leak_fa[i] - previous_flow).abs();
        }

        // Variable-area: q_va = P_va * (H_i - z_i) + Y_va (§3.7)
        if favad.c_va[i] > 0.0 {
            let previous_flow = leak_va[i];
            let abs_q = previous_flow.abs().max(1.0e-12);
            let mut hgrad = (2.0 / 3.0) * favad.c_va[i] / abs_q.cbrt();
            let mut hloss = hgrad * previous_flow * 1.5;
            let (barrier_head, barrier_grad) = lower_barrier(previous_flow);
            hloss += barrier_head;
            hgrad += barrier_grad;
            if hgrad > G_MIN {
                let dq = (hloss - pressure_head) / hgrad;
                leak_va[i] = (leak_va[i] - dq).max(0.0);
            }
            qsum += leak_va[i].abs();
            dqsum += (leak_va[i] - previous_flow).abs();
        }
    }

    (qsum, dqsum)
}

/// Leakage secondary convergence check (§3.8).
///
/// After main convergence, evaluates leakage directly from the converged heads
/// using the forward (non-inverted) formula and compares with the linearised
/// leakage flows. Returns `true` if all junctions pass the Q_LEAK_TOL check.
pub(super) fn leakage_converged(
    network: &Network,
    node_heads: &[f64],
    favad: &FavadCoeffs,
    favad_indices: &[usize],
    leak_fa: &[f64],
    leak_va: &[f64],
) -> bool {
    for &i in favad_indices {
        let node = &network.nodes[i];
        let pressure = node_heads[i] - node.base.elevation;
        let hp = pressure.max(0.0);

        let q_ref_fa = if favad.c_fa[i] > 0.0 {
            (hp / favad.c_fa[i]).sqrt()
        } else {
            0.0
        };
        let q_ref_va = if favad.c_va[i] > 0.0 {
            let ratio = hp / favad.c_va[i];
            ratio * ratio.sqrt()
        } else {
            0.0
        };

        let q_ref = q_ref_fa + q_ref_va;
        let q_lin = leak_fa[i] + leak_va[i];

        if (q_ref - q_lin).abs() > Q_LEAK_TOL {
            return false;
        }
    }
    true
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.3.2 — PDA demand coefficients
// ═══════════════════════════════════════════════════════════════════════════════

/// Assembles PDA demand diagonal/RHS contributions into the sparse matrix
/// (§3.3.2 + §3.4) and subtracts PDA demand flows from xflow.
///
/// Active only when `demand_model = PDA`. Replaces the fixed demand subtraction
/// for junctions where full_demand > 0 with pressure-dependent demand flows.
pub(super) fn apply_pda_demand_coeffs(
    network: &Network,
    sparse: &mut SparseSolver,
    node_junc_step_opt: &[Option<usize>],
    pda_indices: &[usize],
    junction_demands: &[f64],
    pda_demand_flows: &[f64],
    xflow: &mut [f64],
    pda_pmin: f64,
    pda_preq: f64,
    pda_pexp: f64,
) {
    let n_d = 1.0 / pda_pexp; // exponent n_d = 1/n_P (§3.3.2)
    let dp = pda_preq - pda_pmin;
    if dp <= 0.0 {
        return;
    }

    for &i in pda_indices {
        let node = &network.nodes[i];
        let d_full = junction_demands[i];
        if d_full <= 0.0 {
            continue;
        }
        let Some(ji) = node_junc_step_opt[i] else {
            continue;
        };
        let pr = sparse.row[ji];
        let z = node.base.elevation;

        let d = pda_demand_flows[i];
        let abs_d = d.abs().max(1.0e-12);

        // g_d = n_d * (P_req - P_min) / D_full * (|D_i| / D_full)^(n_d - 1)
        let mut hgrad = n_d * dp / d_full * (abs_d / d_full).powf(n_d - 1.0);
        // h_d = g_d * D_i / n_d
        let mut hloss = hgrad * d / n_d;

        // Lower barrier at D_i = 0: enforces D_i >= 0
        let (dh_lo, dg_lo) = lower_barrier(d);
        hloss += dh_lo;
        hgrad += dg_lo;

        // Upper barrier at D_i = D_full: enforces D_i <= D_full
        let (dh_hi, dg_hi) = upper_barrier(d - d_full);
        hloss += dh_hi;
        hgrad += dg_hi;

        if hgrad <= 0.0 {
            continue;
        }

        // Assembly: ΔA_ii = 1/g_d, ΔF_i = (h_d + z_i + P_min) / g_d (§3.4)
        sparse.aii[pr] += 1.0 / hgrad;
        sparse.f[pr] += (hloss + z + pda_pmin) / hgrad;

        // Replace fixed demand with PDA demand flow in xflow
        xflow[i] += junction_demands[i]; // undo the demand that will be subtracted later
        xflow[i] -= pda_demand_flows[i];
    }
}

/// Updates PDA demand flows after head solve (§3.7).
///
/// Returns (sum_abs_flow, sum_abs_change) for convergence tracking.
pub(super) fn update_pda_demand_flows(
    network: &Network,
    node_heads: &[f64],
    pda_indices: &[usize],
    junction_demands: &[f64],
    pda_demand_flows: &mut [f64],
    pda_pmin: f64,
    pda_preq: f64,
    pda_pexp: f64,
) -> (f64, f64) {
    let n_d = 1.0 / pda_pexp;
    let dp = pda_preq - pda_pmin;
    let mut qsum = 0.0_f64;
    let mut dqsum = 0.0_f64;

    if dp <= 0.0 {
        return (qsum, dqsum);
    }

    for &i in pda_indices {
        let node = &network.nodes[i];
        let d_full = junction_demands[i];
        if d_full <= 0.0 {
            continue;
        }

        let d_old = pda_demand_flows[i];
        let abs_d = d_old.abs().max(1.0e-12);

        let mut hgrad = n_d * dp / d_full * (abs_d / d_full).powf(n_d - 1.0);
        let mut hloss = hgrad * d_old / n_d;

        let (dh_lo, dg_lo) = lower_barrier(d_old);
        hloss += dh_lo;
        hgrad += dg_lo;

        let (dh_hi, dg_hi) = upper_barrier(d_old - d_full);
        hloss += dh_hi;
        hgrad += dg_hi;

        if hgrad <= 0.0 {
            continue;
        }

        // D_i = P_d * (H_i - z_i - P_min) + Y_d, clamped to [0, D_full]
        let pressure_head = node_heads[i] - node.base.elevation - pda_pmin;
        let dq = (hloss - pressure_head) / hgrad;
        pda_demand_flows[i] = (pda_demand_flows[i] - dq).clamp(0.0, d_full);

        qsum += pda_demand_flows[i].abs();
        dqsum += (pda_demand_flows[i] - d_old).abs();
    }

    (qsum, dqsum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emitter_headloss_allows_backflow_when_enabled() {
        let (h, g) = emitter_headloss(2.0, 2.0, -1.0, true, 1.0e-7);
        assert!(h < 0.0);
        assert!(g > 0.0);
    }

    #[test]
    fn emitter_headloss_barrier_opposes_backflow_when_disabled() {
        let (h_free, g_free) = emitter_headloss(2.0, 2.0, -1.0, true, 1.0e-7);
        let (h_blocked, g_blocked) = emitter_headloss(2.0, 2.0, -1.0, false, 1.0e-7);
        assert!(h_blocked < h_free);
        assert!(g_blocked > g_free);
    }
}
