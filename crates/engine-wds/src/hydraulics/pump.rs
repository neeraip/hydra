// pump — §3.2.5 pump curves, §3.3 generic link coefficients, §3.10 initialisation

use crate::{
    Curve, HeadLossFormula, Link, LinkKind, LinkStatus, Network, PumpCurveType, ValveType,
};

use super::shared::{HydraulicError, PumpCoeffs, Py, C_INF, GAMMA_WATER, G_MIN, Q_CLOSED};
use super::{pipe_total_hg, valve::valve_py, HW_EXP};

/// Initialises link flows from current statuses and settings (§3.10).
///
/// Status and setting have already been set (by `init_link_states` during
/// session build, then potentially overridden by simple/rule controls before
/// the first solve call). This function only sets the initial flow based
/// on the current status and must not overwrite status or setting.
pub(super) fn initialise_flows(
    network: &Network,
    flows: &mut [f64],
    statuses: &[LinkStatus],
    settings: &[f64],
    pump_coeffs: &[Option<PumpCoeffs>],
    pump_curve_idx: &[Option<usize>],
) {
    for (k, link) in network.links.iter().enumerate() {
        let is_closed = matches!(
            statuses[k],
            LinkStatus::Closed
                | LinkStatus::XPressure
                | LinkStatus::XFcv
                | LinkStatus::XHead
                | LinkStatus::TempClosed
        );

        flows[k] = if is_closed {
            Q_CLOSED
        } else {
            match &link.kind {
                LinkKind::Pump(_) => {
                    let omega = settings[k];
                    let q_design =
                        pump_design_flow(k, link, pump_coeffs, pump_curve_idx, &network.curves);
                    omega * q_design
                }
                LinkKind::Pipe(pipe) => std::f64::consts::PI * pipe.diameter * pipe.diameter / 4.0,
                LinkKind::Valve(v) => std::f64::consts::PI * v.diameter * v.diameter / 4.0,
            }
        };
    }
}

/// Returns the design-point flow for a pump at initialisation (§3.10).
fn pump_design_flow(
    k: usize,
    link: &Link,
    pump_coeffs: &[Option<PumpCoeffs>],
    pump_curve_idx: &[Option<usize>],
    curves: &[Curve],
) -> f64 {
    let LinkKind::Pump(pump) = &link.kind else {
        return 0.028317; // 1 ft³/s expressed in m³/s
    };
    match pump.curve_type {
        PumpCurveType::ConstHp => 0.028317, // 1 ft³/s initial guess in m³/s
        PumpCurveType::PowerFunction => {
            // Use precomputed index instead of linear scan (§2.9).
            if let Some(idx) = pump_curve_idx[k] {
                let curve = &curves[idx];
                if curve.points.len() >= 2 {
                    return curve.points[curve.points.len() / 2].x;
                }
            }
            if let Some(c) = &pump_coeffs[k] {
                (c.h0 / c.r).powf(1.0 / c.n)
            } else {
                0.028317 // fallback: 1 ft³/s in m³/s
            }
        }
        PumpCurveType::Custom => {
            if let Some(idx) = pump_curve_idx[k] {
                let curve = &curves[idx];
                if curve.points.len() >= 2 {
                    let x0 = curve.points.first().unwrap().x;
                    let xl = curve.points.last().unwrap().x;
                    (x0 + xl) / 2.0
                } else {
                    0.028317 // fallback: 1 ft³/s in m³/s
                }
            } else {
                0.028317 // fallback: 1 ft³/s in m³/s
            }
        }
    }
}

/// Tries to fit `PumpCoeffs` from a pump head curve.
pub(super) fn fit_pump_coeffs(curve: &Curve) -> Option<PumpCoeffs> {
    let pts = &curve.points;
    match pts.len() {
        0 => None,
        1 => {
            let (q1, h1) = (pts[0].x, pts[0].y);
            let h0 = 1.33334 * h1;
            let r = if q1 > 0.0 {
                (h0 - h1) / (q1 * q1)
            } else {
                return None;
            };
            if r > 0.0 {
                Some(PumpCoeffs { h0, r, n: 2.0 })
            } else {
                None
            }
        }
        2 => {
            let h0 = if pts[0].x == 0.0 {
                pts[0].y
            } else {
                let slope = (pts[1].y - pts[0].y) / (pts[1].x - pts[0].x);
                pts[0].y - slope * pts[0].x
            };
            let (q2, h2) = (pts[1].x, pts[1].y);
            let r = if q2 > 0.0 {
                (h0 - h2) / (q2 * q2)
            } else {
                return None;
            };
            if r > 0.0 && h0 > 0.0 {
                Some(PumpCoeffs { h0, r, n: 2.0 })
            } else {
                None
            }
        }
        _ => {
            let h0 = if pts[0].x == 0.0 {
                pts[0].y
            } else {
                let slope = (pts[1].y - pts[0].y) / (pts[1].x - pts[0].x);
                pts[0].y - slope * pts[0].x
            };
            let mid = pts.len() / 2;
            let (q1, h1) = (pts[mid].x, pts[mid].y);
            let (q2, h2) = (pts[pts.len() - 1].x, pts[pts.len() - 1].y);
            PumpCoeffs::from_three_points(h0, q1, h1, q2, h2)
        }
    }
}

/// Computes P/Y for a single link (extracted for serial/parallel reuse).
#[inline]
pub(super) fn link_py(
    link: &Link,
    q: f64,
    status: LinkStatus,
    setting: f64,
    pipe_r_k: f64,
    pump_coeffs_k: &Option<PumpCoeffs>,
    curves: &[Curve],
    formula: HeadLossFormula,
    viscosity: f64,
    sp_grav: f64,
    pump_curve_idx_k: Option<usize>,
    rq_tol: f64,
) -> Result<Py, HydraulicError> {
    // PRV/PSV/FCV with a live setting are ALWAYS handled exclusively by
    // apply_valve_coefficients (matching EPANET's "else hyd->P[k] = 0.0"
    // branch in the coeff calculation loop).  This check must come before the
    // generic closed-status guard so that a transitionally-CLOSED control
    // valve is not also assembled via assemble_links (which would double the
    // conductance and incorrectly add the valve to xflow).
    if let LinkKind::Valve(v) = &link.kind {
        if matches!(
            v.valve_type,
            ValveType::Prv | ValveType::Psv | ValveType::Fcv
        ) && !setting.is_nan()
        {
            return Ok(Py { p: 0.0, y: q });
        }
    }

    if matches!(
        status,
        LinkStatus::Closed | LinkStatus::XHead | LinkStatus::TempClosed
    ) {
        return Ok(Py {
            p: 1.0 / C_INF,
            y: q,
        });
    }

    if status == LinkStatus::Active {
        let self_computes = matches!(
            &link.kind,
            LinkKind::Valve(v)
                if matches!(
                    v.valve_type,
                    ValveType::Pbv | ValveType::Tcv | ValveType::Gpv | ValveType::Pcv
                )
        );
        if !self_computes {
            return Ok(Py { p: 0.0, y: 0.0 });
        }
    }

    match &link.kind {
        LinkKind::Pipe(pipe) => {
            let (h, g) = pipe_total_hg(
                pipe_r_k,
                q,
                pipe.roughness,
                pipe.diameter,
                pipe.minor_loss,
                formula,
                viscosity,
            );
            // §3.3 guard conditions apply to every formula: from_hg clamps a
            // degenerate gradient (g < g_min) using the formula's flow
            // exponent n_f (1.852 for HW, 2 for DW and CM).
            let n_f = match formula {
                HeadLossFormula::HazenWilliams => HW_EXP,
                _ => 2.0,
            };
            Ok(Py::from_hg(h, g, n_f, q))
        }
        LinkKind::Pump(pump) => {
            if setting == 0.0 {
                Ok(Py::closed(q))
            } else {
                Ok(match pump.curve_type {
                    PumpCurveType::ConstHp => {
                        let power = pump.power.unwrap_or(0.0);
                        let gamma = sp_grav * GAMMA_WATER;
                        if q.abs() < 1.0e-10 {
                            Py::closed(q)
                        } else {
                            let r_adj = power * setting * setting * setting / gamma;
                            let hgrad = r_adj / (q * q);
                            if hgrad > C_INF {
                                // Treat as closed link if gradient too large
                                Py::closed(q)
                            } else if hgrad < G_MIN {
                                // Treat as open valve if gradient too small
                                // (matches EPANET: P=1/CSMALL, Y=q)
                                Py {
                                    p: 1.0 / G_MIN,
                                    y: q,
                                }
                            } else {
                                let hloss = -r_adj / q.abs() * q.signum();
                                Py {
                                    p: 1.0 / hgrad,
                                    y: hloss / hgrad,
                                }
                            }
                        }
                    }
                    PumpCurveType::PowerFunction => {
                        let coeffs = match pump_coeffs_k.as_ref() {
                            Some(c) => c,
                            None => {
                                return Err(HydraulicError::NoPumpCoeffs {
                                    pump_id: link.base.id.clone(),
                                });
                            }
                        };
                        let omega = setting;
                        let n = coeffs.n;
                        let r_adj = coeffs.r * omega.powf(2.0 - n);
                        let h0_adj = -(omega * omega * coeffs.h0);
                        let abs_q = q.abs().max(1.0e-12);
                        let hgrad = n * r_adj * abs_q.powf(n - 1.0);
                        if (n - 1.0).abs() < 1.0e-10 {
                            let hloss = h0_adj + r_adj * q;
                            Py {
                                p: 1.0 / r_adj.max(G_MIN),
                                y: hloss / r_adj.max(G_MIN),
                            }
                        } else if hgrad < rq_tol {
                            let hloss = h0_adj + rq_tol * q;
                            Py {
                                p: 1.0 / rq_tol,
                                y: hloss / rq_tol,
                            }
                        } else {
                            let hloss = h0_adj + hgrad * q / n;
                            Py {
                                p: 1.0 / hgrad,
                                y: hloss / hgrad,
                            }
                        }
                    }
                    PumpCurveType::Custom => {
                        if let Some(curve) = pump_curve_idx_k.and_then(|idx| curves.get(idx)) {
                            let omega = setting;
                            let q_adj = q.abs().max(1.0e-12) / omega;
                            let (h0_seg, slope) = curve_segment(curve, q_adj);
                            let hgrad = (-slope * omega).max(G_MIN);
                            let hloss = -h0_seg * omega * omega + hgrad * q;
                            Py {
                                p: 1.0 / hgrad,
                                y: hloss / hgrad,
                            }
                        } else {
                            Py::closed(q)
                        }
                    }
                })
            }
        }
        LinkKind::Valve(v) => valve_py(v, q, setting, link, status, curves),
    }
}

/// Returns `(h0, slope)` for the piecewise-linear curve segment bracketing `q`.
///
/// Equivalent to EPANET's `curvecoeff`. Extrapolates linearly beyond endpoints.
pub(super) fn curve_segment(curve: &Curve, q: f64) -> (f64, f64) {
    let pts = &curve.points;
    if pts.len() < 2 {
        return (pts.first().map_or(0.0, |p| p.y), 0.0);
    }
    let k2 = pts.partition_point(|p| p.x < q);
    let (k1, k2) = if k2 == 0 {
        (0, 1)
    } else if k2 >= pts.len() {
        (pts.len() - 2, pts.len() - 1)
    } else {
        (k2 - 1, k2)
    };
    let slope = (pts[k2].y - pts[k1].y) / (pts[k2].x - pts[k1].x);
    let h0 = pts[k1].y - slope * pts[k1].x;
    (h0, slope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CurveKind, CurvePoint};

    fn curve(points: &[(f64, f64)]) -> Curve {
        Curve {
            id: "curve".into(),
            kind: CurveKind::PumpHead,
            points: points
                .iter()
                .map(|(x, y)| CurvePoint { x: *x, y: *y })
                .collect(),
        }
    }

    #[test]
    fn dw_pipe_degenerate_gradient_uses_from_hg_clamp() {
        use crate::test_support::TestNetworkBuilder;

        let (net, _ns, _ls) = TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 10.0)
            .hw_pipe("P1", "R1", "J1", 10.0, 36.0, 100.0)
            .build();
        // Zero flow + tiny resistance → laminar DW gradient far below G_MIN.
        let py = link_py(
            &net.links[0],
            0.0,
            LinkStatus::Open,
            1.0,
            1.0e-8,
            &None,
            &[],
            HeadLossFormula::DarcyWeisbach,
            1.0e-6,
            1.0,
            None,
            1.0e-7,
        )
        .expect("link_py");
        // §3.3 guard: g < g_min → g2 = g_min / n_f with n_f = 2 for DW,
        // giving P = n_f / g_min and Y = Q (= 0 here).
        let expected_p = 2.0 / G_MIN;
        assert!((py.p - expected_p).abs() / expected_p < 1e-12);
        assert!(py.y.abs() < 1e-12);
    }

    #[test]
    fn fit_pump_coeffs_single_point_curve_is_quadratic() {
        let coeffs = fit_pump_coeffs(&curve(&[(10.0, 90.0)])).expect("fit coeffs");
        assert!(coeffs.h0 > 90.0);
        assert_eq!(coeffs.n, 2.0);
        assert!(coeffs.r > 0.0);
    }

    #[test]
    fn fit_pump_coeffs_two_point_curve_uses_extrapolated_shutoff_head() {
        let coeffs = fit_pump_coeffs(&curve(&[(5.0, 100.0), (10.0, 80.0)])).expect("fit coeffs");
        assert!(coeffs.h0 > 100.0);
        assert_eq!(coeffs.n, 2.0);
    }

    #[test]
    fn curve_segment_extrapolates_beyond_endpoints() {
        let c = curve(&[(0.0, 100.0), (10.0, 80.0), (20.0, 40.0)]);
        let (_, low_slope) = curve_segment(&c, -1.0);
        let (_, high_slope) = curve_segment(&c, 25.0);
        assert!((low_slope + 2.0).abs() < 1e-12);
        assert!((high_slope + 4.0).abs() < 1e-12);
    }
}
