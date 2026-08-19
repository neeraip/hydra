//! Street inlets (§7.8): HEC-22 capture — gutter spread from Izzard's
//! form, frontal and side grate efficiencies with splash-over, curb
//! full-capture length, the on-sag weir/orifice forms with their
//! published transition depths, and custom capture curves. The fitted
//! coefficients are the published standard's, evaluated in its foot
//! units with $g$ exact per §2.11; captured flow moves from bypass
//! vertex to sewer vertex each routing step, surcharge returning as
//! backflow apportioned by open-area ratio among standard inlets and by
//! count among custom ones.

use crate::hydraulics::routing::Router;
use crate::model::{
    CurveKind, GrateKind, InletPlacement, LinkKind, Network, ThroatAngle, XsectReferent,
};

const FT: f64 = 0.3048;
const CFS: f64 = 0.028_316_846_592;
/// Exact §2.11 gravity in ft/s².
const G_FT: f64 = 9.806_65 / FT;
/// The predecessor's minimum runoff flow (cfs).
const MIN_Q: f64 = 0.001;

/// HEC-22 splash-over cubic coefficients per standard grate.
const SPLASH: [[f64; 4]; 7] = [
    [2.22, 4.03, 0.65, 0.06],
    [0.74, 2.44, 0.27, 0.02],
    [1.76, 3.12, 0.45, 0.03],
    [0.30, 4.85, 1.31, 0.15],
    [0.99, 2.64, 0.36, 0.03],
    [0.51, 2.34, 0.20, 0.01],
    [0.28, 2.28, 0.18, 0.01],
];
/// Grate opening ratios (HEC-22 chart 9B), `Generic` last.
const OPEN_RATIO: [f64; 8] = [0.90, 0.80, 0.60, 0.35, 0.17, 0.34, 0.80, 1.00];

fn grate_index(k: GrateKind) -> usize {
    match k {
        GrateKind::PBar50 => 0,
        GrateKind::PBar50x100 => 1,
        GrateKind::PBar30 => 2,
        GrateKind::CurvedVane => 3,
        GrateKind::TiltBar45 => 4,
        GrateKind::TiltBar30 => 5,
        GrateKind::Reticuline => 6,
        GrateKind::Generic => 7,
    }
}

/// Street (or fallback conduit) geometry, in feet.
#[derive(Clone, Copy)]
struct Geo {
    sl: f64,
    sx: f64,
    sw: f64,
    a: f64,
    w: f64,
    tcrown: f64,
    nsides: f64,
    qfactor: f64,
    /// 1.486·√S/n, the drop-inlet conveyance factor (ft units).
    beta: f64,
}

/// One placed inlet, precompiled to HEC-22's units.
struct InletState {
    design: usize,
    link: usize,
    bypass: usize,
    capture: usize,
    on_grade: bool,
    count: u32,
    clog: f64,
    q_limit: f64,
    geo: Geo,
    backflow_ratio: f64,
}

/// The §7.8 runtime capture system.
pub struct Inlets {
    list: Vec<InletState>,
}

impl Inlets {
    /// Compile the inlet placements; `None` when the model has none.
    pub fn build(net: &Network, router: &Router) -> Option<Inlets> {
        if net.inlet_usage.is_empty() {
            return None;
        }
        let mut list = Vec::new();
        for u in &net.inlet_usage {
            let link = &net.links[u.link];
            let slope = router.chan_full_attrs(u.link).map_or(0.01, |x| x.3);
            let roughness = match &link.kind {
                LinkKind::Channel { roughness, .. } => *roughness,
                _ => 0.013,
            };
            let street = link
                .cross_section
                .as_ref()
                .and_then(|cs| cs.referent)
                .and_then(|r| match r {
                    XsectReferent::Street(s) => Some(&net.streets[s]),
                    _ => None,
                });
            let geo = match street {
                Some(st) => {
                    let n = st.roughness;
                    let sx = st.cross_slope;
                    let (mut a, mut w) = (st.gutter_depression / FT, st.gutter_width / FT);
                    if u.local_depression * u.local_width > 0.0 {
                        a += u.local_depression / FT;
                        w = u.local_width / FT;
                    }
                    Geo {
                        sl: slope.max(1.0e-5),
                        sx,
                        sw: if w * a > 0.0 { sx + a / w } else { sx },
                        a,
                        w,
                        tcrown: st.crown_width / FT,
                        nsides: f64::from(st.sides.max(1)),
                        qfactor: (0.56 / n) * slope.max(1.0e-5).powf(0.5) * sx.powf(1.67),
                        beta: 1.486 * slope.max(1.0e-5).sqrt() / n,
                    }
                }
                None => Geo {
                    sl: slope.max(1.0e-5),
                    sx: 0.01,
                    sw: 0.01,
                    a: 0.0,
                    w: 0.0,
                    tcrown: f64::MAX,
                    nsides: 1.0,
                    qfactor: (0.56 / roughness) * slope.max(1.0e-5).powf(0.5) * 0.01_f64.powf(1.67),
                    beta: 1.486 * slope.max(1.0e-5).sqrt() / roughness,
                },
            };
            // AUTOMATIC resolves by the bypass vertex's topology: any
            // outgoing link means on-grade (§7.8).
            let on_grade = match u.placement {
                InletPlacement::OnGrade => true,
                InletPlacement::OnSag => false,
                InletPlacement::Automatic => net.links.iter().any(|l| l.from == link.to),
            };
            list.push(InletState {
                design: u.design,
                link: u.link,
                bypass: link.to,
                capture: u.capture_vertex,
                on_grade,
                count: u.count.max(1),
                clog: 1.0 - u.pct_clogged / 100.0,
                q_limit: if u.flow_limit > 0.0 {
                    u.flow_limit / CFS
                } else {
                    f64::MAX
                },
                geo,
                backflow_ratio: 0.0,
            });
        }
        // Backflow apportionment at shared capture vertices (§7.8).
        let ratios: Vec<f64> = {
            let mut out = vec![0.0; list.len()];
            for i in 0..list.len() {
                let n = list[i].capture;
                let all: Vec<usize> = (0..list.len()).filter(|&j| list[j].capture == n).collect();
                let std_links = all
                    .iter()
                    .filter(|&&j| inlet_area(net, &list[j]) > 0.0)
                    .count();
                let f = std_links as f64 / all.len() as f64;
                let area = inlet_area(net, &list[i]);
                if area > 0.0 {
                    let total: f64 = all.iter().map(|&j| inlet_area(net, &list[j])).sum();
                    out[i] = area / total * f;
                } else {
                    let customs: u32 = all
                        .iter()
                        .filter(|&&j| inlet_area(net, &list[j]) == 0.0)
                        .map(|&j| list[j].count)
                        .sum();
                    out[i] = f64::from(list[i].count) / f64::from(customs.max(1)) * (1.0 - f);
                }
            }
            out
        };
        for (inlet, r) in list.iter_mut().zip(ratios) {
            inlet.backflow_ratio = r;
        }
        Some(Inlets { list })
    }

    /// Compute captures at the current state and adjust the lateral flow
    /// and mass vectors: captured flow moves bypass → sewer at the
    /// bypass concentration, backflow returns at the capture vertex's
    /// concentration (§7.8, §8.1).
    pub fn apply(
        &mut self,
        router: &Router,
        net: &Network,
        lat: &mut [f64],
        mass: &mut crate::transport::quality::SourceMass,
        np: usize,
        conc: &dyn Fn(usize, usize) -> f64,
    ) {
        for inlet in &mut self.list {
            let q_si = router.flow(inlet.link, net).abs();
            let d_ft = router.depth(inlet.bypass) / FT;
            let design = &net.inlets[inlet.design];
            let q_cfs = q_si / CFS;
            // Drop inlets in ordinary conduits read the link's own flow
            // geometry rather than a street section (§7.8).
            let link_state = (
                router.link_depth(inlet.link).unwrap_or(0.0) / FT,
                router
                    .link_velocity(inlet.link)
                    .map(|v| v.abs() / FT)
                    .unwrap_or(0.0),
            );
            let mut captured = if design.custom_curve.is_some() {
                custom_capture(inlet, design, net, q_si, router.depth(inlet.bypass)) / CFS
            } else if inlet.on_grade {
                on_grade_capture(inlet, design, q_cfs, d_ft, link_state)
            } else {
                on_sag_capture(inlet, design, d_ft)
            };
            if captured < 1e-8 {
                captured = 0.0;
            }
            let captured_si = captured * CFS;
            let backflow_si = router.flood_rate(inlet.capture) * inlet.backflow_ratio;
            lat[inlet.bypass] -= captured_si - backflow_si;
            lat[inlet.capture] += captured_si;
            for p in 0..np {
                // The transfer carries the bypass concentration, clamped
                // to the lateral mass actually present (§8.1). It moves
                // between vertices without changing where the load
                // entered, so the origin split rides along with it
                // (§11.2).
                let present = mass.total(p, inlet.bypass).max(0.0);
                let m = (captured_si * conc(p, inlet.bypass)).min(present);
                mass.transfer(p, inlet.bypass, inlet.capture, m);
                mass.add_mixed(
                    p,
                    inlet.bypass,
                    inlet.capture,
                    backflow_si * conc(p, inlet.capture),
                );
            }
        }
    }
}

/// The unclogged open area of an inlet placement (ft²); zero marks a
/// custom inlet for the backflow rule.
fn inlet_area(net: &Network, inlet: &InletState) -> f64 {
    let d = &net.inlets[inlet.design];
    let mut area = 0.0;
    if let Some(g) = &d.grate {
        area = (g.length / FT)
            * (g.width / FT)
            * if g.grate == GrateKind::Generic {
                g.area_ratio
            } else {
                OPEN_RATIO[grate_index(g.grate)]
            };
    }
    if let Some(c) = &d.curb {
        let sweep = (c.length - d.grate.as_ref().map_or(0.0, |g| g.length)) / FT;
        if sweep > 0.0 {
            area += sweep * (c.height / FT);
        }
    }
    if let Some(s) = &d.slotted {
        area = (s.length / FT) * (s.width / FT);
    }
    area * f64::from(inlet.count) * inlet.clog
}

/// HEC-22 Eq (4-4) solved for the gutter flow ratio.
fn eo(sr: f64, ts: f64, w: f64) -> f64 {
    let x = sr / (ts / w);
    let x = (1.0 + x).powf(2.67) - 1.0;
    1.0 / (1.0 + sr / x)
}

/// Flow spread across the street section (ft), HEC-22 Eq (4-2)/(4-6).
fn flow_spread(g: &Geo, q: f64) -> f64 {
    let f = g.qfactor;
    let ts1 = if g.a == 0.0 {
        (q / f).powf(0.375)
    } else {
        let f1 = f * ((g.a / g.w) / g.sx).powf(1.67);
        let tw = (q / f1).powf(0.375);
        if tw <= g.w {
            tw
        } else {
            let sr = (g.sx + g.a / g.w) / g.sx;
            let mut ts1 = (q / f).powf(0.375) - g.w;
            if ts1 <= 0.0 {
                ts1 = tw - g.w;
            }
            let mut ts2 = ts1;
            for _ in 0..10 {
                let e = eo(sr, ts1, g.w);
                ts2 = (((1.0 - e) * q) / f).powf(0.375);
                if (ts2 - ts1).abs() < 0.01 {
                    break;
                }
                ts1 = ts2;
            }
            ts2 + g.w
        }
    };
    ts1.min(g.tcrown)
}

fn gutter_flow_ratio(g: &Geo, t: f64, w: f64) -> f64 {
    if t <= w {
        1.0
    } else if g.a > 0.0 {
        eo(g.sw / g.sx, t - w, w)
    } else {
        1.0 - (1.0 - w / t).powf(2.67)
    }
}

/// HEC-22 Eq (4-20a) area correction for a grate narrower than the
/// depressed gutter.
fn gutter_area_ratio(g: &Geo, t: f64, wg: f64, area: f64) -> f64 {
    if wg >= g.w || t <= wg {
        return 1.0;
    }
    if t <= g.w {
        return wg / t;
    }
    let a_side = 0.5 * (t - g.w) * (t - g.w) * g.sx;
    let a_grate = wg * ((t * g.sx) + g.a - (wg * g.sw / 2.0));
    a_grate / (area - a_side)
}

/// On-grade capture for one inlet of the design (cfs in, cfs out).
fn on_grade_capture(
    inlet: &InletState,
    design: &crate::model::InletDesign,
    q: f64,
    d: f64,
    link_state: (f64, f64),
) -> f64 {
    if q < MIN_Q {
        return 0.0;
    }
    let g = inlet.geo;
    let mut approach = q / g.nsides;
    let mut captured = 0.0;
    for _ in 0..inlet.count {
        let qc = single_on_grade(inlet, design, approach, d, link_state) * inlet.clog;
        let qc = qc.min(inlet.q_limit).min(approach);
        captured += qc;
        approach -= qc;
        if approach < MIN_Q {
            break;
        }
    }
    captured * g.nsides
}

fn single_on_grade(
    inlet: &InletState,
    design: &crate::model::InletDesign,
    q: f64,
    d: f64,
    link_state: (f64, f64),
) -> f64 {
    let g = inlet.geo;
    // Drop inlets in non-street conduits operate on their own modes.
    if design.drop_curb {
        return on_sag_single(inlet, design, d).min(q);
    }
    if design.drop_grate {
        return drop_grate_capture(inlet, design, q, link_state).min(q);
    }
    let mut t = flow_spread(&g, q);
    if let Some(s) = &design.slotted {
        return curb_capture(&g, q, s.length / FT, t).min(q);
    }
    let l_grate = design.grate.as_ref().map_or(0.0, |x| x.length / FT);
    let l_curb = design.curb.as_ref().map_or(0.0, |x| x.length / FT);
    let mut q1 = q;
    let mut qc = 0.0;
    if l_curb > 0.0 {
        let sweep = l_curb - l_grate;
        if sweep > 0.0 {
            qc = curb_capture(&g, q1, sweep, t);
            q1 -= qc;
        }
    }
    if l_grate > 0.0 && q1 > 0.0 {
        if q1 != q {
            t = flow_spread(&g, q1);
        }
        qc += grate_capture(inlet, design, q1, t);
    }
    qc
}

/// On-grade drop-grate capture: the frontal-flow ratio from the
/// conduit's own section, Eo = β(Y·Wg)^1.67/(Wg+2Y)^0.67/Q, with the
/// grate efficiencies of Eqs (4-18)/(4-19) (§7.8).
fn drop_grate_capture(
    inlet: &InletState,
    design: &crate::model::InletDesign,
    q: f64,
    link_state: (f64, f64),
) -> f64 {
    let Some(grate) = design.grate.as_ref() else {
        return 0.0;
    };
    if q <= 0.0 {
        return 0.0;
    }
    let g = inlet.geo;
    let (lg, wg) = (grate.length / FT, grate.width / FT);
    let (y, v) = link_state;
    if y <= 0.0 {
        return 0.0;
    }
    let mut e_o = (g.beta * (y * wg).powf(1.67) / (wg + 2.0 * y).powf(0.67) / q).min(1.0);
    if e_o.is_nan() {
        e_o = 1.0;
    }
    let vo = if grate.grate == GrateKind::Generic {
        grate.splash_velocity / FT
    } else {
        let c = SPLASH[grate_index(grate.grate)];
        c[0] + c[1] * lg - c[2] * lg * lg + c[3] * lg * lg * lg
    };
    let rf = if v > vo { 1.0 - 0.09 * (v - vo) } else { 1.0 };
    let rs = if e_o < 1.0 {
        1.0 / (1.0 + 0.15 * v.powf(1.8) / g.sx / lg.powf(2.3))
    } else {
        0.0
    };
    q * (rf.clamp(0.0, 1.0) * e_o + rs * (1.0 - e_o))
}

/// HEC-22 grate capture, Eqs (4-16)–(4-21).
fn grate_capture(inlet: &InletState, design: &crate::model::InletDesign, q: f64, t: f64) -> f64 {
    let g = inlet.geo;
    let Some(grate) = design.grate.as_ref() else {
        return 0.0;
    };
    let (lg, wg) = (grate.length / FT, grate.width / FT);
    let mut qo = q;
    let (area, mut e_o);
    if g.a == 0.0 {
        area = t * t * g.sx / 2.0;
        e_o = gutter_flow_ratio(&g, t, wg);
        if t >= g.tcrown {
            qo = g.qfactor * g.tcrown.powf(2.67);
        }
    } else {
        area = if t <= g.w {
            t * t * g.sw / 2.0
        } else {
            (t * t * g.sx + g.a * g.w) / 2.0
        };
        e_o = gutter_flow_ratio(&g, t, g.w);
        if e_o < 1.0 {
            if t >= g.tcrown {
                qo = g.qfactor * g.tcrown.powf(2.67) / (1.0 - e_o);
            }
            e_o *= gutter_area_ratio(&g, t, wg, area);
        }
    }
    let v = qo / area.max(1e-9);
    let vo = if grate.grate == GrateKind::Generic {
        grate.splash_velocity / FT
    } else {
        let c = SPLASH[grate_index(grate.grate)];
        c[0] + c[1] * lg - c[2] * lg * lg + c[3] * lg * lg * lg
    };
    let rf = if v > vo { 1.0 - 0.09 * (v - vo) } else { 1.0 };
    let rs = if e_o < 1.0 {
        1.0 / (1.0 + 0.15 * v.powf(1.8) / g.sx / lg.powf(2.3))
    } else {
        0.0
    };
    q * (rf.clamp(0.0, 1.0) * e_o + rs * (1.0 - e_o))
}

/// HEC-22 curb-opening capture, Eqs (4-22a)–(4-24).
fn curb_capture(g: &Geo, q: f64, l: f64, t: f64) -> f64 {
    let mut se = g.sx;
    if g.a > 0.0 {
        let e = eo(g.sw / g.sx, t - g.w, g.w);
        se = g.sx + (g.a / g.w) * e;
    }
    // The street's Manning n is folded into qfactor; recover it.
    let n = 0.56 * g.sl.powf(0.5) * g.sx.powf(1.67) / g.qfactor;
    let lt = 0.6 * q.powf(0.42) * g.sl.powf(0.3) * (1.0 / (n * se)).powf(0.6);
    let e = if l < lt {
        1.0 - (1.0 - l / lt).powf(1.8)
    } else {
        1.0
    };
    e.clamp(0.0, 1.0) * q
}

/// On-sag capture for the whole placement (cfs).
fn on_sag_capture(inlet: &InletState, design: &crate::model::InletDesign, d: f64) -> f64 {
    let total = inlet.geo.nsides * f64::from(inlet.count);
    let qc = on_sag_single(inlet, design, d) * inlet.clog;
    qc.min(inlet.q_limit) * total
}

/// One on-sag inlet's weir/orifice capture (cfs), HEC-22 (4-26)–(4-33).
fn on_sag_single(inlet: &InletState, design: &crate::model::InletDesign, d: f64) -> f64 {
    let g = inlet.geo;
    if let Some(s) = &design.slotted {
        let (l, w) = (s.length / FT, s.width / FT);
        return if d <= 2.587 * w {
            2.48 * l * d.powf(1.5)
        } else {
            0.8 * l * w * (2.0 * G_FT * d).sqrt()
        };
    }
    let (mut qgw, mut qgo) = (0.0, 0.0);
    let l_grate = design.grate.as_ref().map_or(0.0, |x| x.length / FT);
    if let Some(grate) = &design.grate {
        let (lg, mut wg) = (grate.length / FT, grate.width / FT);
        let di;
        let p;
        if design.drop_grate {
            di = d;
            p = 2.0 * (lg + wg);
        } else {
            if d <= wg * g.sw {
                wg = d / g.sw;
            }
            di = d - (wg / 2.0) * g.sw;
            p = lg + 2.0 * wg;
        }
        let ao = lg
            * (grate.width / FT)
            * if grate.grate == GrateKind::Generic {
                grate.area_ratio
            } else {
                OPEN_RATIO[grate_index(grate.grate)]
            };
        if d <= 1.79 * ao / p {
            qgw = 3.0 * p * di.max(0.0).powf(1.5);
        } else {
            qgo = 0.67 * ao * (2.0 * G_FT * di.max(0.0)).sqrt();
        }
    }
    let (mut qsw, mut qso, mut qco) = (0.0, 0.0, 0.0);
    if let Some(curb) = &design.curb {
        let l_curb = curb.length / FT;
        let sweep = l_curb - l_grate;
        if sweep > 0.0 {
            let (w, o) = curb_sag_flows(inlet, design, d, sweep);
            qsw = w;
            qso = o;
        }
        // Behind an orifice-mode grate only the curb's orifice component
        // contributes (the predecessor's combination rule).
        if qgo > 0.0 {
            let (_, o) = curb_sag_flows(inlet, design, d, l_grate);
            qco = o;
        }
    }
    qgw + qgo + qsw + qso + qco
}

/// On-sag curb weir/orifice split with the published transition depths.
#[allow(clippy::approx_constant)] // 0.7071 is the standard's own literal
fn curb_sag_flows(
    inlet: &InletState,
    design: &crate::model::InletDesign,
    d: f64,
    l: f64,
) -> (f64, f64) {
    let g = inlet.geo;
    let Some(curb) = design.curb.as_ref() else {
        return (0.0, 0.0);
    };
    let h = curb.height / FT;
    let mut l = l;
    if l <= 0.0 {
        return (0.0, 0.0);
    }
    if design.drop_curb {
        l *= 4.0;
    }
    let orifice = |di: f64| {
        let dd = match curb.throat {
            ThroatAngle::Horizontal => di - h / 2.0,
            ThroatAngle::Inclined => di - (h / 2.0) * 0.7071,
            ThroatAngle::Vertical => di,
        };
        0.67 * h * l * (2.0 * G_FT * dd.max(0.0)).sqrt()
    };
    let dorif = 1.4 * h;
    if d > dorif {
        return (0.0, orifice(d));
    }
    let (qweir, dweir);
    if g.a == 0.0 || l > 12.0 {
        dweir = h;
        if d < dweir {
            return (3.0 * l * d.powf(1.5), 0.0);
        }
        qweir = 3.0 * l * dweir.powf(1.5);
    } else {
        let p = l + 1.8 * g.w;
        dweir = h + g.a;
        if d < dweir {
            return (2.3 * p * d.powf(1.5), 0.0);
        }
        qweir = 2.3 * p * dweir.powf(1.5);
    }
    // Interpolate between the weir and orifice regimes.
    let qorif = orifice(dorif);
    let r = (d - dweir) / (dorif - dweir);
    ((1.0 - r) * qweir, r * qorif)
}

/// Custom capture: a diversion curve on approach flow or a rating curve
/// on bypass depth, both SI per the model's curve conversions (§14.6).
fn custom_capture(
    inlet: &InletState,
    design: &crate::model::InletDesign,
    net: &Network,
    q_si: f64,
    d_si: f64,
) -> f64 {
    let Some(ci) = design.custom_curve else {
        return 0.0;
    };
    let curve = &net.curves[ci];
    let sides = inlet.geo.nsides;
    let mut captured = 0.0;
    match curve.kind {
        CurveKind::Diversion => {
            let mut bypassed = q_si / sides;
            for _ in 0..inlet.count {
                let mut inc = inlet.clog * lookup_ex(&curve.points, bypassed);
                inc = inc.min(inlet.q_limit * CFS).min(bypassed);
                captured += inc;
                bypassed -= inc;
                if bypassed < MIN_Q * CFS {
                    break;
                }
            }
        }
        _ => {
            captured = f64::from(inlet.count) * inlet.clog * lookup_ex(&curve.points, d_si);
        }
    }
    captured * sides
}

/// Linear interpolation with end extrapolation (the predecessor's
/// `table_lookupEx`).
fn lookup_ex(points: &[(f64, f64)], x: f64) -> f64 {
    match points {
        [] => 0.0,
        [(x0, y0)] => {
            if *x0 > 0.0 {
                y0 * x / x0
            } else {
                *y0
            }
        }
        _ => {
            let n = points.len();
            let (mut x1, mut y1) = points[0];
            if x <= x1 {
                let (x2, y2) = points[1];
                return if x2 > x1 {
                    y1 + (y2 - y1) * (x - x1) / (x2 - x1)
                } else {
                    y1
                };
            }
            for &(x2, y2) in &points[1..] {
                if x <= x2 {
                    return y1 + (y2 - y1) * (x - x1) / (x2 - x1);
                }
                (x1, y1) = (x2, y2);
            }
            let (xa, ya) = points[n - 2];
            let (xb, yb) = points[n - 1];
            if xb > xa {
                yb + (yb - ya) / (xb - xa) * (x - xb)
            } else {
                yb
            }
        }
    }
}

// ── Checkpointing (§12.3) ────────────────────────────────────────────────────

impl Inlets {
    /// Write the inlets' state (§12.3).
    ///
    /// Only the backflow ratio moves during a run; the rest is the
    /// placement and geometry the model builds.
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_f, put_u};
        let Inlets { list } = self;
        put_u(w, list.len() as u64)?;
        for inlet in list {
            let InletState {
                design: _,
                link: _,
                bypass: _,
                capture: _,
                on_grade: _,
                count: _,
                clog: _,
                q_limit: _,
                geo: _,
                backflow_ratio,
            } = inlet;
            put_f(w, *backflow_ratio)?;
        }
        Ok(())
    }

    /// Read back what `checkpoint_put` wrote.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        let n = r.u()? as usize;
        if n != self.list.len() {
            return Err(format!(
                "checkpoint holds {n} inlets where this model has {}",
                self.list.len()
            ));
        }
        for inlet in &mut self.list {
            inlet.backflow_ratio = r.f()?;
        }
        Ok(())
    }
}
