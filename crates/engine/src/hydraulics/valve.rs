// valve — §3.5 valve coefficients, §3.9 link/valve status helpers

use crate::{
    Link, LinkKind, LinkStatus, Network, NodeState, PumpCurveType, ValveType,
};

use super::pump::curve_segment;
use super::shared::{HydraulicError, PumpCoeffs, Py, C_INF, G_MIN};
use super::SparseSolver;

// ═══════════════════════════════════════════════════════════════════════════════
// §3.5 — PRV/PSV/FCV coefficient assembly
// ═══════════════════════════════════════════════════════════════════════════════

/// Applies PRV/PSV/FCV matrix contributions for all statuses (§3.5).
/// Matches EPANET prvcoeff/psvcoeff/fcvcoeff: these valves are always
/// excluded from linkcoeffs (P=0 in headlosscoeffs) and handled here.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_valve_coefficients(
    network: &Network,
    sparse: &mut SparseSolver,
    node_junc_step_opt: &[Option<usize>],
    p: &mut [f64],
    y: &mut [f64],
    flows: &[f64],
    statuses: &[LinkStatus],
    settings: &[f64],
    node_elevations: &[f64],
    xflow: &[f64],
    link_aij_pos: &[Option<usize>],
    node_heads: &[f64],
) {
    for (k, link) in network.links.iter().enumerate() {
        let status = statuses[k];
        let setting = settings[k];
        let LinkKind::Valve(v) = &link.kind else {
            continue;
        };
        if !matches!(
            v.valve_type,
            ValveType::Prv | ValveType::Psv | ValveType::Fcv
        ) {
            continue;
        }
        // Skip fixed-status valves (setting = NaN).
        if setting.is_nan() {
            continue;
        }

        let from_node_index = link.base.from_idx();
        let to_node_index = link.base.to_idx();
        let flow = flows[k];
        let km = v.minor_loss;

        match v.valve_type {
            ValveType::Prv => {
                if status == LinkStatus::Active {
                    // PRV active: pin downstream junction to H_s = z₂ + setting.
                    // Matches EPANET prvcoeff Active branch.
                    let hs = node_elevations[to_node_index] + setting;
                    if let Some(ji2) = node_junc_step_opt[to_node_index] {
                        let pr2 = sparse.row[ji2];
                        sparse.aii[pr2] += C_INF;
                        sparse.f[pr2] += hs * C_INF;
                    }
                    if xflow[to_node_index] < 0.0 {
                        if let Some(ji) = node_junc_step_opt[from_node_index] {
                            let pr = sparse.row[ji];
                            sparse.f[pr] += xflow[to_node_index];
                        }
                    }
                    p[k] = 0.0;
                    y[k] = flow + xflow[to_node_index];
                } else {
                    // OPEN / CLOSED / XPRESSURE: compute P/Y via valvecoeff,
                    // then assemble with F += (Y-Q) (matches EPANET prvcoeff).
                    let (pk, yk) = valve_open_py(flow, km, status);
                    assemble_valve_link(
                        sparse,
                        node_junc_step_opt,
                        link_aij_pos,
                        k,
                        from_node_index,
                        to_node_index,
                        pk,
                        yk,
                        flow,
                        node_heads,
                    );
                    p[k] = pk;
                    y[k] = yk;
                }
            }
            ValveType::Psv => {
                if status == LinkStatus::Active {
                    // PSV active: pin upstream junction to H_s = z₁ + setting.
                    let hs = node_elevations[from_node_index] + setting;
                    if let Some(ji) = node_junc_step_opt[from_node_index] {
                        let pr = sparse.row[ji];
                        sparse.aii[pr] += C_INF;
                        sparse.f[pr] += hs * C_INF;
                    }
                    if xflow[from_node_index] > 0.0 {
                        if let Some(ji2) = node_junc_step_opt[to_node_index] {
                            let pr2 = sparse.row[ji2];
                            sparse.f[pr2] += xflow[from_node_index];
                        }
                    }
                    // PSV Active: preserve weak connectivity (EPANET adds 1/CBIG terms)
                    if let Some(pos) = link_aij_pos[k] {
                        sparse.aij[pos] -= 1.0 / C_INF;
                    }
                    if let Some(ji2) = node_junc_step_opt[to_node_index] {
                        let pr2 = sparse.row[ji2];
                        sparse.aii[pr2] += 1.0 / C_INF;
                    }
                    p[k] = 0.0;
                    y[k] = flow - xflow[from_node_index];
                } else {
                    let (pk, yk) = valve_open_py(flow, km, status);
                    assemble_valve_link(
                        sparse,
                        node_junc_step_opt,
                        link_aij_pos,
                        k,
                        from_node_index,
                        to_node_index,
                        pk,
                        yk,
                        flow,
                        node_heads,
                    );
                    p[k] = pk;
                    y[k] = yk;
                }
            }
            ValveType::Fcv => {
                if status == LinkStatus::Active {
                    // FCV active: impose fixed flow Q_s = setting.
                    // Matches EPANET fcvcoeff Active branch.
                    let qs = setting;
                    if let Some(ji) = node_junc_step_opt[from_node_index] {
                        let pr = sparse.row[ji];
                        sparse.f[pr] -= qs;
                    }
                    if let Some(ji2) = node_junc_step_opt[to_node_index] {
                        let pr2 = sparse.row[ji2];
                        sparse.f[pr2] += qs;
                    }
                    let pk = 1.0 / C_INF;
                    if let Some(pos) = link_aij_pos[k] {
                        sparse.aij[pos] -= pk;
                    }
                    if let Some(ji) = node_junc_step_opt[from_node_index] {
                        let pr = sparse.row[ji];
                        sparse.aii[pr] += pk;
                    }
                    if let Some(ji2) = node_junc_step_opt[to_node_index] {
                        let pr2 = sparse.row[ji2];
                        sparse.aii[pr2] += pk;
                    }
                    p[k] = pk;
                    y[k] = flow - qs;
                } else {
                    let (pk, yk) = valve_open_py(flow, km, status);
                    assemble_valve_link(
                        sparse,
                        node_junc_step_opt,
                        link_aij_pos,
                        k,
                        from_node_index,
                        to_node_index,
                        pk,
                        yk,
                        flow,
                        node_heads,
                    );
                    p[k] = pk;
                    y[k] = yk;
                }
            }
            _ => {}
        }
    }
}

/// Computes P/Y for an open or closed valve (matches EPANET valvecoeff).
fn valve_open_py(flow: f64, km: f64, status: LinkStatus) -> (f64, f64) {
    // EPANET valvecoeff: only status <= CLOSED (XHEAD, TEMPCLOSED, CLOSED)
    // gets high-resistance treatment. XFCV and XPRESSURE are treated as
    // open valves (low resistance), matching EPANET's numeric ordering.
    if matches!(
        status,
        LinkStatus::Closed | LinkStatus::XHead | LinkStatus::TempClosed
    ) {
        return (1.0 / C_INF, flow);
    }
    // Open: minor loss headloss
    if km > 0.0 {
        let flow_abs = flow.abs();
        let hgrad = 2.0 * km * flow_abs;
        if hgrad < G_MIN {
            let hgrad2 = G_MIN / 2.0;
            let hloss = flow * hgrad2;
            (1.0 / hgrad2, hloss / hgrad2)
        } else {
            let hloss = flow * hgrad / 2.0;
            (1.0 / hgrad, hloss / hgrad)
        }
    } else {
        // No minor loss: low-resistance linear (EPANET CSMALL branch)
        (1.0 / G_MIN, flow)
    }
}

/// Assembles matrix contributions for a non-Active valve link.
/// Uses F += (Y-Q) instead of F += Y, since valve is excluded from xflow.
/// Matches EPANET prvcoeff/psvcoeff/fcvcoeff non-Active branch.
#[allow(clippy::too_many_arguments)]
fn assemble_valve_link(
    sparse: &mut SparseSolver,
    node_junc_step_opt: &[Option<usize>],
    link_aij_pos: &[Option<usize>],
    k: usize,
    n1: usize,
    n2: usize,
    pk: f64,
    yk: f64,
    flow: f64,
    node_heads: &[f64],
) {
    // Off-diagonal
    if let Some(pos) = link_aij_pos[k] {
        sparse.aij[pos] -= pk;
    }
    let dy = yk - flow; // (Y - Q) correction since valve not in xflow

    // n1 contributions
    match node_junc_step_opt[n1] {
        Some(ji) => {
            let pr = sparse.row[ji];
            sparse.aii[pr] += pk;
            sparse.f[pr] += dy;
        }
        None => {
            // n1 is fixed-grade
            if let Some(ji2) = node_junc_step_opt[n2] {
                let pr2 = sparse.row[ji2];
                sparse.f[pr2] += pk * node_heads[n1];
            }
        }
    }
    // n2 contributions
    match node_junc_step_opt[n2] {
        Some(ji2) => {
            let pr2 = sparse.row[ji2];
            sparse.aii[pr2] += pk;
            sparse.f[pr2] -= dy;
        }
        None => {
            // n2 is fixed-grade
            if let Some(ji) = node_junc_step_opt[n1] {
                let pr = sparse.row[ji];
                sparse.f[pr] += pk * node_heads[n2];
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.9 — Link status logic
// ═══════════════════════════════════════════════════════════════════════════════

/// Checks and updates link statuses for one pass (§3.9).
///
/// Checks PRV/PSV valve status. Runs every iteration (matches EPANET valvestatus).
/// Returns `true` if any status changed.
pub(super) fn check_valve_status(
    network: &Network,
    statuses: &mut [LinkStatus],
    settings: &[f64],
    flows: &[f64],
    node_states: &[NodeState],
    node_elevations: &[f64],
    eps_h: f64,
    eps_q: f64,
) -> bool {
    let mut changed = false;
    for (k, link) in network.links.iter().enumerate() {
        let LinkKind::Valve(v) = &link.kind else {
            continue;
        };
        let setting = settings[k];
        if setting.is_nan() {
            continue;
        }
        if !matches!(v.valve_type, ValveType::Prv | ValveType::Psv) {
            continue;
        }

        let from_node_index = link.base.from_idx();
        let to_node_index = link.base.to_idx();
        let from_head = node_states[from_node_index].head;
        let to_head = node_states[to_node_index].head;
        let flow = flows[k];

        match v.valve_type {
            ValveType::Prv => {
                let z2 = node_elevations[to_node_index];
                let km = v.minor_loss;
                changed |= prv_status(
                    statuses, k, from_head, to_head, flow, setting, z2, km, eps_h, eps_q,
                );
            }
            ValveType::Psv => {
                let z1 = node_elevations[from_node_index];
                let km = v.minor_loss;
                changed |= psv_status(
                    statuses, k, from_head, to_head, flow, setting, z1, km, eps_h, eps_q,
                );
            }
            _ => unreachable!(),
        }
    }
    changed
}

/// Checks pumps, CVs, FCVs, and tank-connected pipe status.
/// Runs at CheckFreq intervals or at convergence (matches EPANET linkstatus).
/// Returns `true` if any status changed.
#[allow(clippy::too_many_arguments)]
pub(super) fn check_link_status(
    network: &Network,
    statuses: &mut [LinkStatus],
    settings: &[f64],
    flows: &[f64],
    node_states: &[NodeState],
    node_h_min: &[f64],
    node_h_max: &[f64],
    eps_h: f64,
    eps_q: f64,
    pump_coeffs: &[Option<PumpCoeffs>],
) -> bool {
    let mut changed = false;

    for (k, link) in network.links.iter().enumerate() {
        let n1 = link.base.from_idx();
        let n2 = link.base.to_idx();
        let h1 = node_states[n1].head;
        let h2 = node_states[n2].head;
        let q = flows[k];
        let setting = settings[k];

        // Save original status — EPANET linkstatus() compares final vs original
        // at the end of each link to determine if a change occurred. This avoids
        // false positives when TEMPCLOSED is reset to OPEN then set back to
        // TEMPCLOSED by tankstatus in the same pass.
        let original_status = statuses[k];

        // §3.9: Re-open XHEAD/TEMPCLOSED links before type-specific checks.
        // EPANET linkstatus() resets these for ALL link types, not just pumps.
        if matches!(statuses[k], LinkStatus::XHead | LinkStatus::TempClosed) {
            statuses[k] = LinkStatus::Open;
        }

        match &link.kind {
            LinkKind::Pipe(pipe) if pipe.check_valve => {
                let cur = statuses[k];
                let new_status = match cur {
                    LinkStatus::Open => {
                        if (h1 - h2) < -eps_h || q < -eps_q {
                            LinkStatus::Closed
                        } else {
                            LinkStatus::Open
                        }
                    }
                    LinkStatus::Closed => {
                        if (h1 - h2) > eps_h && q >= -eps_q {
                            LinkStatus::Open
                        } else {
                            LinkStatus::Closed
                        }
                    }
                    _ => cur,
                };
                statuses[k] = new_status;
            }
            LinkKind::Pump(pump) => {
                // XHEAD/TEMPCLOSED already reset above.
                if matches!(statuses[k], LinkStatus::Open) {
                    if pump.curve_type == PumpCurveType::ConstHp {
                        // §3.9: ConstHp pump with flow below TINY → TEMPCLOSED.
                        const TINY: f64 = 1.0e-6;
                        if q < TINY {
                            statuses[k] = LinkStatus::TempClosed;
                        }
                    } else if let Some(coeffs) = &pump_coeffs[k] {
                        let omega = setting;
                        let shutoff = omega * omega * coeffs.h0;
                        if (h2 - h1) > shutoff + eps_h {
                            statuses[k] = LinkStatus::XHead;
                        }
                    }
                }
            }
            LinkKind::Valve(v) => {
                // §2.6.4: fixed valve (setting = MISSING/NaN) — skip automatic status changes.
                if setting.is_nan() {
                    continue;
                }
                match v.valve_type {
                    ValveType::Prv | ValveType::Psv => {}
                    ValveType::Fcv => {
                        // fcv_status modifies statuses[k] directly;
                        // its return value is not needed here since we compare
                        // original_status at the end.
                        fcv_status(&mut *statuses, k, h1, h2, q, setting, eps_h, eps_q, link);
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        // §3.9 Tank inlet/outlet pipe closure: after type-specific checks above,
        // close links adjacent to tanks at their level limits.
        // EPANET tankstatus: `if (LinkStatus[k] <= CLOSED) return` — only skip
        // links that are fully CLOSED; XHead/TempClosed links are still checked
        // (e.g. a pump set to XHead adjacent to a full tank).
        if !matches!(statuses[k], LinkStatus::Closed) {
            let q = flows[k];
            // n1 side — h_min/h_max are INFINITY sentinels for non-tanks and
            // overflow tanks, so neither condition fires for those nodes.
            let head_n1 = node_states[n1].head;
            if (head_n1 >= node_h_max[n1] && q < 0.0) || (head_n1 <= node_h_min[n1] && q > 0.0) {
                statuses[k] = LinkStatus::TempClosed;
            }
            // n2 side
            if !matches!(statuses[k], LinkStatus::Closed) {
                let head_n2 = node_states[n2].head;
                let q_out = -q;
                if (head_n2 >= node_h_max[n2] && q_out < 0.0) || (head_n2 <= node_h_min[n2] && q_out > 0.0) {
                    statuses[k] = LinkStatus::TempClosed;
                }
            }
        }

        // Compare final status with original — only flag a true change.
        if statuses[k] != original_status {
            changed = true;
        }
    }
    changed
}

fn prv_status(
    statuses: &mut [LinkStatus],
    k: usize,
    h1: f64,
    h2: f64,
    q: f64,
    setting: f64,
    z2: f64,
    km: f64,
    eps_h: f64,
    eps_q: f64,
) -> bool {
    let hs = z2 + setting;
    let status = statuses[k];
    let km_q2 = km * q * q;
    let new_status = match status {
        LinkStatus::Active => {
            // EPANET: reverse-flow check takes priority over head-based transition.
            if q < -eps_q {
                LinkStatus::Closed
            } else if h1 - km_q2 < hs - eps_h {
                LinkStatus::Open
            } else {
                LinkStatus::Active
            }
        }
        LinkStatus::Open => {
            if q < -eps_q {
                LinkStatus::Closed
            } else if h2 >= hs + eps_h {
                LinkStatus::Active
            } else {
                LinkStatus::Open
            }
        }
        LinkStatus::Closed => {
            if h1 >= hs + eps_h && h2 < hs - eps_h {
                LinkStatus::Active
            } else if h1 < hs - eps_h && h1 > h2 + eps_h {
                LinkStatus::Open
            } else {
                LinkStatus::Closed
            }
        }
        LinkStatus::XPressure => {
            if q < -eps_q {
                LinkStatus::Closed
            } else {
                LinkStatus::XPressure
            }
        }
        _ => status,
    };
    if new_status != status {
        statuses[k] = new_status;
        return true;
    }
    false
}

fn psv_status(
    statuses: &mut [LinkStatus],
    k: usize,
    h1: f64,
    h2: f64,
    q: f64,
    setting: f64,
    z1: f64,
    km: f64,
    eps_h: f64,
    eps_q: f64,
) -> bool {
    let hs = z1 + setting; // PSV: H_s = z_from + s_k (§3.9)
    let status = statuses[k];
    let km_q2 = km * q * q;
    let new_status = match status {
        LinkStatus::Active => {
            // EPANET: reverse-flow check takes priority over head-based transition.
            if q < -eps_q {
                LinkStatus::Closed
            } else if h2 + km_q2 > hs + eps_h {
                LinkStatus::Open
            } else {
                LinkStatus::Active
            }
        }
        LinkStatus::Open => {
            if q < -eps_q {
                LinkStatus::Closed
            } else if h1 < hs - eps_h {
                LinkStatus::Active
            } else {
                LinkStatus::Open
            }
        }
        LinkStatus::Closed => {
            if h2 > hs + eps_h && h1 > h2 + eps_h {
                LinkStatus::Open
            } else if h1 >= hs + eps_h && h1 > h2 + eps_h {
                LinkStatus::Active
            } else {
                LinkStatus::Closed
            }
        }
        LinkStatus::XPressure => {
            if q < -eps_q {
                LinkStatus::Closed
            } else {
                LinkStatus::XPressure
            }
        }
        _ => status,
    };
    if new_status != status {
        statuses[k] = new_status;
        return true;
    }
    false
}

fn fcv_status(
    statuses: &mut [LinkStatus],
    k: usize,
    h1: f64,
    h2: f64,
    q: f64,
    setting: f64,
    eps_h: f64,
    eps_q: f64,
    link: &Link,
) -> bool {
    let km = if let LinkKind::Valve(v) = &link.kind {
        v.minor_loss
    } else {
        0.0
    };
    let status = statuses[k];
    let head_drop = h1 - h2;
    let new_status = match status {
        LinkStatus::Active => {
            let km_q2 = if q.abs() > 1.0e-20 { km } else { 0.0 };
            if head_drop < -eps_h || q < -eps_q || head_drop / (q * q).max(1.0e-20) < km_q2 {
                LinkStatus::XFcv
            } else {
                LinkStatus::Active
            }
        }
        LinkStatus::XFcv => {
            // EPANET: reverse-condition guards fire before recovery check.
            if head_drop < -eps_h || q < -eps_q {
                LinkStatus::XFcv
            } else if q >= setting {
                LinkStatus::Active
            } else {
                LinkStatus::XFcv
            }
        }
        _ => status,
    };
    if new_status != status {
        statuses[k] = new_status;
        return true;
    }
    false
}

/// EPANET `badvalve`: if node `ni` is adjacent to an Active FCV/PRV/PSV,
/// demote that valve (FCV→XFcv, PRV/PSV→XPressure) and return `true`.
pub(super) fn bad_valve(network: &Network, statuses: &mut [LinkStatus], ni: usize) -> bool {
    for (k, link) in network.links.iter().enumerate() {
        let from_node_index = link.base.from_idx();
        let to_node_index = link.base.to_idx();
        if from_node_index == ni || to_node_index == ni {
            if let LinkKind::Valve(v) = &link.kind {
                if matches!(
                    v.valve_type,
                    ValveType::Prv | ValveType::Psv | ValveType::Fcv
                ) && statuses[k] == LinkStatus::Active
                {
                    statuses[k] = if v.valve_type == ValveType::Fcv {
                        LinkStatus::XFcv
                    } else {
                        LinkStatus::XPressure
                    };
                    return true;
                }
            }
            return false;
        }
    }
    false
}

// ═══════════════════════════════════════════════════════════════════════════════
// §3.2.4 — Valve P/Y coefficients
// ═══════════════════════════════════════════════════════════════════════════════

/// Computes P/Y for a valve link (§3.2.4, §3.3).
pub(super) fn valve_py(
    v: &crate::Valve,
    q: f64,
    setting: f64,
    _link: &crate::Link,
    status: LinkStatus,
    curves: &[crate::Curve],
) -> Result<Py, HydraulicError> {
    let abs_q = q.abs();
    let km = v.minor_loss;

    let py = match v.valve_type {
        ValveType::Gpv => {
            if let Some(curve) = curves.iter().find(|c| v.curve.as_deref() == Some(&c.id)) {
                let qe = abs_q.max(1.0e-14);
                let (h0, slope) = curve_segment(curve, qe);
                let slope_pos = slope.max(G_MIN);
                let sgn = if q >= 0.0 { 1.0 } else { -1.0 };
                Py {
                    p: 1.0 / slope_pos,
                    y: (h0 / slope_pos) * sgn + q,
                }
            } else {
                Py::closed(q)
            }
        }
        ValveType::Tcv => {
            // When setting = NaN (MISSING), the TCV is user-fixed OPEN/CLOSED
            // and acts as a plain valve with its original minor loss Km
            // (matches EPANET tcvcoeff: skips Km override when MISSING).
            if setting.is_nan() {
                if km > 0.0 {
                    let h = km * q * abs_q;
                    let g = (2.0 * km * abs_q).max(G_MIN);
                    Py {
                        p: 1.0 / g,
                        y: h / g,
                    }
                } else {
                    Py {
                        p: 1.0 / G_MIN,
                        y: q,
                    }
                }
            } else {
                // K_m_eff = 8·s/(π²·g·D⁴) ≈ 0.08262·s/D⁴ (SI, g = 9.81 m/s²)
                let km_eff = 0.08262 * setting / v.diameter.powi(4);
                let h = km_eff * q * abs_q;
                let g = (2.0 * km_eff * abs_q).max(G_MIN);
                Py {
                    p: 1.0 / g,
                    y: h / g,
                }
            }
        }
        ValveType::Pcv => {
            // When setting = NaN (MISSING), PCV is user-fixed OPEN/CLOSED
            // and acts as a plain valve with its original minor loss Km
            // (matches EPANET pcvcoeff: skips R override when MISSING).
            if setting.is_nan() || setting <= 0.0 {
                if km > 0.0 {
                    let h = km * q * abs_q;
                    let g = (2.0 * km * abs_q).max(G_MIN);
                    Py {
                        p: 1.0 / g,
                        y: h / g,
                    }
                } else {
                    Py {
                        p: 1.0 / G_MIN,
                        y: q,
                    }
                }
            } else {
                let kv = if let Some(curve) =
                    curves.iter().find(|c| v.curve.as_deref() == Some(&c.id))
                {
                    (curve.eval(setting) / 100.0).clamp(1.0e-6, 1.0)
                } else {
                    (setting / 100.0).clamp(1.0e-6, 1.0)
                };
                let km_eff = km / (kv * kv);
                let h = km_eff * q * abs_q;
                let g = (2.0 * km_eff * abs_q).max(G_MIN);
                Py {
                    p: 1.0 / g,
                    y: h / g,
                }
            }
        }
        ValveType::Pbv => {
            let hs = setting;
            if hs > 0.0 && km * abs_q * abs_q <= hs {
                Py {
                    p: C_INF,
                    y: hs * C_INF,
                }
            } else {
                let h = km * q * abs_q;
                let g = 2.0 * km * abs_q;
                Py::from_hg(h, g, 2.0, q)
            }
        }
        ValveType::Prv | ValveType::Psv | ValveType::Fcv => {
            if !setting.is_nan() {
                // Setting != NaN → handled entirely by apply_valve_coefficients
                // (EPANET valvecoeffs). Return P=0 so linkcoeffs/xflow skips
                // this link, matching EPANET headlosscoeffs P[k]=0 branch.
                Py { p: 0.0, y: q }
            } else {
                // Setting == NaN (MISSING): valve is user-fixed Open/Closed.
                // apply_valve_coefficients skips NaN-setting valves, so we
                // compute headloss here (EPANET headlosscoeffs → valvecoeff).
                match status {
                    LinkStatus::Open => {
                        if km > 0.0 {
                            let h = km * q * abs_q;
                            let g = 2.0 * km * abs_q;
                            Py::from_hg(h, g, 2.0, q)
                        } else {
                            Py {
                                p: 1.0 / G_MIN,
                                y: q,
                            }
                        }
                    }
                    _ => Py::closed(q),
                }
            }
        }
    };
    Ok(py)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valve_open_py_closed_status_uses_high_resistance() {
        let (p, y) = valve_open_py(1.5, 0.2, LinkStatus::Closed);
        assert!((p - 1.0 / C_INF).abs() < 1e-18);
        assert!((y - 1.5).abs() < 1e-12);
    }

    #[test]
    fn valve_open_py_open_without_minor_loss_uses_linear_branch() {
        let (p, y) = valve_open_py(2.0, 0.0, LinkStatus::Open);
        assert!((p - 1.0 / G_MIN).abs() < 1e-12);
        assert!((y - 2.0).abs() < 1e-12);
    }

    #[test]
    fn prv_status_transitions_open_to_active_when_downstream_exceeds_setting() {
        let mut statuses = vec![LinkStatus::Open];
        let changed = prv_status(
            &mut statuses,
            0,
            120.0,
            111.0,
            1.0,
            10.0,
            100.0,
            0.0,
            0.5,
            0.01,
        );
        assert!(changed);
        assert_eq!(statuses[0], LinkStatus::Active);
    }
}
