//! Street and inlet parsing (§7.8): `[STREETS]` cross-sections,
//! `[INLETS]` capture designs, and `[INLET_USAGE]` placements.
//!
//! Lengths convert from feet, capture caps from the flow unit; slopes are
//! written as percentages and stored as fractions.

use crate::dialect::keywords::match_keyword;
use crate::dialect::lex::FiniteParse;
use crate::dialect::objects::UnitConverter;
use crate::dialect::survey::{Diagnostic, DiagnosticKind, ObjectKind, Survey, TokenLine};
use crate::engine_api::model::{
    CurbInlet, GrateInlet, GrateKind, InletDesign, InletPlacement, InletUsage, SlottedInlet,
    Street, ThroatAngle,
};

fn err(line: usize, kind: DiagnosticKind) -> Diagnostic {
    Diagnostic { line, kind }
}

fn bad(line: usize, token: &str) -> Diagnostic {
    err(
        line,
        DiagnosticKind::BadValue {
            token: token.to_string(),
        },
    )
}

fn unresolved(line: usize, id: &str) -> Diagnostic {
    err(
        line,
        DiagnosticKind::UnresolvedReference { id: id.to_string() },
    )
}

/// Parse a `[STREETS]` section.
pub(crate) fn parse_streets(
    lines: &[TokenLine<'_>],
    s: &Survey,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Street> {
    let ids = s.ids.get(&ObjectKind::Street);
    let n = ids.map_or(0, |m| m.len());
    let mut out: Vec<Street> = vec![Street::default(); n];
    'line: for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 5 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&idx) = ids.and_then(|m| m.get(t[0].to_ascii_uppercase().as_str())) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        // Crown width, curb height, cross slope (%), roughness: all > 0.
        let mut x = [0.0_f64; 11];
        for k in 1..=4 {
            match t[k].finite_f64() {
                Ok(v) if v > 0.0 => x[k] = v,
                _ => {
                    diags.push(bad(l, t[k]));
                    continue 'line;
                }
            }
        }
        // Optional gutter depression and width: ≥ 0.
        for k in 5..=6 {
            if t.len() > k {
                match t[k].finite_f64() {
                    Ok(v) if v >= 0.0 => x[k] = v,
                    _ => {
                        diags.push(bad(l, t[k]));
                        continue 'line;
                    }
                }
            }
        }
        let mut sides = 2_u8;
        if t.len() > 7 {
            match t[7].parse::<u8>() {
                Ok(v @ 1..=2) => sides = v,
                _ => {
                    diags.push(bad(l, t[7]));
                    continue;
                }
            }
        }
        // Backing: a positive width demands its slope and roughness.
        if t.len() > 8 {
            match t[8].finite_f64() {
                Ok(v) if v >= 0.0 => x[8] = v,
                _ => {
                    diags.push(bad(l, t[8]));
                    continue;
                }
            }
            if x[8] > 0.0 {
                if t.len() < 11 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                for k in 9..=10 {
                    match t[k].finite_f64() {
                        Ok(v) if v > 0.0 => x[k] = v,
                        _ => {
                            diags.push(bad(l, t[k]));
                            continue 'line;
                        }
                    }
                }
            }
        }
        out[idx] = Street {
            id: t[0].to_string(),
            crown_width: x[1] * cv.len,
            curb_height: x[2] * cv.len,
            cross_slope: x[3] / 100.0,
            roughness: x[4],
            gutter_depression: x[5] * cv.len,
            gutter_width: x[6] * cv.len,
            sides,
            backing_width: x[8] * cv.len,
            backing_slope: x[9] / 100.0,
            backing_roughness: x[10],
        };
    }
    out
}

/// Design-type keywords. The predecessor's table carries an unnameable
/// placeholder for the combination type at index 2 — a combination arises
/// only from a grate line and a curb line sharing one design.
const INLET_TYPES: &[&str] = &[
    "GRATE",
    "CURB",
    "SLOTTED",
    "DROP_GRATE",
    "DROP_CURB",
    "CUSTOM",
];

/// Grate families. The longer `P_BAR-50x100` sits *before* its prefix
/// `P_BAR-50`, unlike the predecessor's table — there, first-prefix-wins
/// matching makes the canonical spelling `P_BAR-50x100` resolve to
/// `P_BAR-50`, leaving the 50x100 grate unreachable from an input file.
/// Reordering accepts every token the predecessor accepts while letting
/// each canonical spelling reach its own family.
const GRATE_TYPES: &[&str] = &[
    "P_BAR-50x100",
    "P_BAR-50",
    "P_BAR-30",
    "CURVED_VANE",
    "TILT_BAR-45",
    "TILT_BAR-30",
    "RETICULINE",
    "GENERIC",
];
const GRATE_KINDS: [GrateKind; 8] = [
    GrateKind::PBar50x100,
    GrateKind::PBar50,
    GrateKind::PBar30,
    GrateKind::CurvedVane,
    GrateKind::TiltBar45,
    GrateKind::TiltBar30,
    GrateKind::Reticuline,
    GrateKind::Generic,
];

const THROAT_ANGLES: &[&str] = &["HORIZONTAL", "INCLINED", "VERTICAL"];

/// Two positive lengths at tokens 2 and 3.
fn two_lengths(t: &[&str], diags: &mut Vec<Diagnostic>, l: usize) -> Option<(f64, f64)> {
    let mut v = [0.0; 2];
    for (i, vi) in v.iter_mut().enumerate() {
        match t[2 + i].finite_f64() {
            Ok(x) if x > 0.0 => *vi = x,
            _ => {
                diags.push(bad(l, t[2 + i]));
                return None;
            }
        }
    }
    Some((v[0], v[1]))
}

/// Parse an `[INLETS]` section.
pub(crate) fn parse_inlets(
    lines: &[TokenLine<'_>],
    s: &Survey,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<InletDesign> {
    let ids = s.ids.get(&ObjectKind::Inlet);
    let n = ids.map_or(0, |m| m.len());
    let mut out: Vec<InletDesign> = vec![InletDesign::default(); n];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&idx) = ids.and_then(|m| m.get(t[0].to_ascii_uppercase().as_str())) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        let design = &mut out[idx];
        if design.id.is_empty() {
            design.id = t[0].to_string();
        }
        let Some(kind) = match_keyword(INLET_TYPES, t[1]) else {
            diags.push(bad(l, t[1]));
            continue;
        };
        if !t[1].eq_ignore_ascii_case(INLET_TYPES[kind]) {
            diags.push(err(
                l,
                DiagnosticKind::PrefixMatched {
                    token: t[1].to_string(),
                    matched: INLET_TYPES[kind],
                },
            ));
        }
        match INLET_TYPES[kind] {
            "GRATE" | "DROP_GRATE" => {
                if t.len() < 5 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let Some((length, width)) = two_lengths(t, diags, l) else {
                    continue;
                };
                let Some(g) = match_keyword(GRATE_TYPES, t[4]) else {
                    diags.push(bad(l, t[4]));
                    continue;
                };
                if !t[4].eq_ignore_ascii_case(GRATE_TYPES[g]) {
                    diags.push(err(
                        l,
                        DiagnosticKind::PrefixMatched {
                            token: t[4].to_string(),
                            matched: GRATE_TYPES[g],
                        },
                    ));
                }
                let grate = GRATE_KINDS[g];
                let mut area_ratio = 0.0;
                let mut splash_velocity = 0.0;
                if grate == GrateKind::Generic {
                    if t.len() < 6 {
                        diags.push(err(l, DiagnosticKind::MissingItems));
                        continue;
                    }
                    match t[5].finite_f64() {
                        Ok(v) if v > 0.0 && v <= 1.0 => area_ratio = v,
                        _ => {
                            diags.push(bad(l, t[5]));
                            continue;
                        }
                    }
                    if t.len() > 6 {
                        match t[6].finite_f64() {
                            Ok(v) if v >= 0.0 => splash_velocity = v * cv.len,
                            _ => {
                                diags.push(bad(l, t[6]));
                                continue;
                            }
                        }
                    }
                }
                design.grate = Some(GrateInlet {
                    length: length * cv.len,
                    width: width * cv.len,
                    grate,
                    area_ratio,
                    splash_velocity,
                });
                design.drop_grate = INLET_TYPES[kind] == "DROP_GRATE";
            }
            "CURB" | "DROP_CURB" => {
                if t.len() < 4 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let Some((length, height)) = two_lengths(t, diags, l) else {
                    continue;
                };
                let mut throat = ThroatAngle::Vertical;
                if INLET_TYPES[kind] == "CURB" && t.len() > 4 {
                    let Some(a) = match_keyword(THROAT_ANGLES, t[4]) else {
                        diags.push(bad(l, t[4]));
                        continue;
                    };
                    throat = [
                        ThroatAngle::Horizontal,
                        ThroatAngle::Inclined,
                        ThroatAngle::Vertical,
                    ][a];
                }
                design.curb = Some(CurbInlet {
                    length: length * cv.len,
                    height: height * cv.len,
                    throat,
                });
                design.drop_curb = INLET_TYPES[kind] == "DROP_CURB";
            }
            "SLOTTED" => {
                if t.len() < 4 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let Some((length, width)) = two_lengths(t, diags, l) else {
                    continue;
                };
                design.slotted = Some(SlottedInlet {
                    length: length * cv.len,
                    width: width * cv.len,
                });
            }
            _ => {
                // CUSTOM: a capture/diversion curve reference.
                match s.resolve(ObjectKind::Curve, t[2]) {
                    Some(&c) => design.custom_curve = Some(c),
                    None => diags.push(unresolved(l, t[2])),
                }
            }
        }
    }
    out
}

const PLACEMENTS: &[&str] = &["AUTOMATIC", "ON_GRADE", "ON_SAG"];

/// Parse an `[INLET_USAGE]` section. A link carries at most one placement:
/// a later line for the same link replaces the earlier, as the
/// predecessor's single per-link slot does.
pub(crate) fn parse_inlet_usage(
    lines: &[TokenLine<'_>],
    s: &Survey,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<InletUsage> {
    let mut out: Vec<InletUsage> = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&link) = s.resolve(ObjectKind::Link, t[0]) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        let Some(&design) = s.resolve(ObjectKind::Inlet, t[1]) else {
            diags.push(unresolved(l, t[1]));
            continue;
        };
        let Some(&capture_vertex) = s.resolve(ObjectKind::Vertex, t[2]) else {
            diags.push(unresolved(l, t[2]));
            continue;
        };
        let mut count = 1_u32;
        if t.len() > 3 {
            match t[3].parse::<u32>() {
                Ok(v) if v >= 1 => count = v,
                _ => {
                    diags.push(bad(l, t[3]));
                    continue;
                }
            }
        }
        let mut pct_clogged = 0.0;
        if t.len() > 4 {
            match t[4].finite_f64() {
                Ok(v) if (0.0..=99.0).contains(&v) => pct_clogged = v,
                _ => {
                    diags.push(bad(l, t[4]));
                    continue;
                }
            }
        }
        let mut x = [0.0_f64; 3]; // flow limit, local depression, width
        let mut ok = true;
        for (i, xi) in x.iter_mut().enumerate() {
            if t.len() > 5 + i {
                match t[5 + i].finite_f64() {
                    Ok(v) if v >= 0.0 => *xi = v,
                    _ => {
                        diags.push(bad(l, t[5 + i]));
                        ok = false;
                        break;
                    }
                }
            }
        }
        if !ok {
            continue;
        }
        let mut placement = InletPlacement::Automatic;
        if t.len() > 8 {
            let Some(p) = match_keyword(PLACEMENTS, t[8]) else {
                diags.push(bad(l, t[8]));
                continue;
            };
            placement = [
                InletPlacement::Automatic,
                InletPlacement::OnGrade,
                InletPlacement::OnSag,
            ][p];
        }
        let usage = InletUsage {
            link,
            design,
            capture_vertex,
            count,
            pct_clogged,
            flow_limit: x[0] * cv.flow,
            local_depression: x[1] * cv.len,
            local_width: x[2] * cv.len,
            placement,
        };
        match out.iter_mut().find(|u| u.link == link) {
            Some(slot) => *slot = usage,
            None => out.push(usage),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::dialect::objects::parse_network;
    use crate::engine_api::model::{GrateKind, InletPlacement, ThroatAngle, XsectReferent};

    const FIXTURE: &str = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  100  3
J2  99   3
SEW  90  5

[OUTFALLS]
O1  95  FREE

[CONDUITS]
GUT1  J1  J2  300  0.016  0  0
C2    J2  O1  300  0.016  0  0

[XSECTIONS]
GUT1  STREET  ST1
C2    CIRCULAR  1  0  0  0

[CURVES]
CAP1  DIVERSION  0  0  1  0.5

[STREETS]
ST1  20  0.5  2  0.016  0.1  2  1  10  4  0.02

[INLETS]
CB1  GRATE  2  2  P_BAR-50x100
CB1  CURB   2  0.5  INCLINED
GEN  DROP_GRATE  3  1  GENERIC  0.8  4
CUS  CUSTOM  CAP1

[INLET_USAGE]
GUT1  CB1  SEW  2  25  1.5  0.1  1  ON_GRADE
GUT1  GEN  SEW
";

    fn net_ok() -> crate::engine_api::model::Network {
        let (net, diags) = parse_network(FIXTURE);
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "{:?}",
            diags
                .iter()
                .filter(|d| d.kind.is_error())
                .collect::<Vec<_>>()
        );
        net
    }

    #[test]
    fn street_converts_lengths_and_slopes() {
        let net = net_ok();
        let st = &net.streets[0];
        assert!((st.crown_width - 20.0 * 0.3048).abs() < 1e-12);
        assert!((st.cross_slope - 0.02).abs() < 1e-12);
        assert_eq!(st.sides, 1);
        assert!((st.backing_width - 10.0 * 0.3048).abs() < 1e-12);
        assert!((st.backing_slope - 0.04).abs() < 1e-12);
        // The street cross-section referent lands on this street.
        assert_eq!(
            net.links[0].cross_section.as_ref().unwrap().referent,
            Some(XsectReferent::Street(0))
        );
    }

    #[test]
    fn a_grate_and_curb_line_form_one_combination_design() {
        let net = net_ok();
        let cb = &net.inlets[0];
        let grate = cb.grate.as_ref().unwrap();
        // The canonical 50x100 spelling reaches its own family — under the
        // predecessor's table order it would resolve to P_BAR-50.
        assert_eq!(grate.grate, GrateKind::PBar50x100);
        let curb = cb.curb.as_ref().unwrap();
        assert_eq!(curb.throat, ThroatAngle::Inclined);
        assert!((curb.height - 0.5 * 0.3048).abs() < 1e-12);
        assert!(!cb.drop_grate && !cb.drop_curb);
        // Generic drop grate keeps its open-area ratio and splash velocity.
        let gen = &net.inlets[1];
        assert!(gen.drop_grate);
        let g = gen.grate.as_ref().unwrap();
        assert_eq!(g.grate, GrateKind::Generic);
        assert!((g.area_ratio - 0.8).abs() < 1e-12);
        assert!((g.splash_velocity - 4.0 * 0.3048).abs() < 1e-12);
        // Custom design references its curve.
        assert_eq!(net.inlets[2].custom_curve, Some(0));
    }

    #[test]
    fn later_usage_replaces_the_links_slot() {
        let net = net_ok();
        assert_eq!(net.inlet_usage.len(), 1);
        let u = &net.inlet_usage[0];
        // The second line won: design GEN, defaults throughout.
        assert_eq!(u.design, 1);
        assert_eq!(u.count, 1);
        assert_eq!(u.placement, InletPlacement::Automatic);
    }

    #[test]
    fn usage_tail_converts_flow_and_lengths() {
        // A single-usage variant exercising the full tail.
        let single = FIXTURE.replace("\nGUT1  GEN  SEW\n", "\n");
        let (net, diags) = parse_network(&single);
        assert!(!diags.iter().any(|d| d.kind.is_error()));
        let u = &net.inlet_usage[0];
        assert_eq!((u.count, u.placement), (2, InletPlacement::OnGrade));
        assert!((u.pct_clogged - 25.0).abs() < 1e-12);
        assert!((u.flow_limit - 1.5 * 0.028_316_846_592).abs() < 1e-15);
        assert!((u.local_depression - 0.1 * 0.3048).abs() < 1e-12);
        assert!((u.local_width - 1.0 * 0.3048).abs() < 1e-12);
    }
}
