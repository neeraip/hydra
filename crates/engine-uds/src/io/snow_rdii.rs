//! Snow pack, unit-hydrograph, sewer-inflow, and treatment parsing (§4.2,
//! §4.3, §8.5), grammar for grammar from the predecessor's readers —
//! including the legacy nine-value unit-hydrograph line format its reader
//! still accepts.

use super::keywords::match_keyword;
use super::objects::UnitConverter;
use super::survey::{Diagnostic, DiagnosticKind, ObjectKind, Survey, TokenLine};
use crate::io::lex::FiniteParse;
use crate::model::{
    RdiiInflow, SnowRemoval, SnowSurface, Snowpack, Treatment, TreatmentKind, UhResponse,
    UnitHydrographGroup,
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

/// Temperature-difference factor: melt coefficients arrive per °F in US
/// files, per °C in SI files.
fn temp_factor(us: bool) -> f64 {
    if us {
        9.0 / 5.0
    } else {
        1.0
    }
}

/// Parse `[SNOWPACKS]`.
pub(crate) fn parse_snowpacks(
    lines: &[TokenLine<'_>],
    s: &Survey,
    cv: &UnitConverter,
    us: bool,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Snowpack> {
    const SURFACES: &[&str] = &["PLOWABLE", "IMPERV", "PERV", "REMOVAL"];
    let ids = s.ids.get(&ObjectKind::Snowpack);
    let n_packs = ids.map_or(0, |m| m.len());
    let mut packs: Vec<Snowpack> = vec![Snowpack::default(); n_packs];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 8 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&idx) = ids.and_then(|m| m.get(t[0].to_ascii_uppercase().as_str())) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        let pack = &mut packs[idx];
        if pack.id.is_empty() {
            pack.id = t[0].to_string();
        }
        let Some(kind) = match_keyword(SURFACES, t[1]) else {
            diags.push(bad(l, t[1]));
            continue;
        };
        let n = if kind == 3 { 6 } else { 7 };
        if t.len() < n + 2 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let mut x = [0.0; 7];
        let mut ok = true;
        for (i, xi) in x.iter_mut().enumerate().take(n) {
            let Ok(v) = t[2 + i].finite_f64() else {
                diags.push(bad(l, t[2 + i]));
                ok = false;
                break;
            };
            *xi = v;
        }
        if !ok {
            continue;
        }
        if kind == 3 {
            // REMOVAL: trigger depth + five fractions + optional parcel.
            let to_parcel = match t.get(8) {
                Some(tok) => {
                    let Some(&p) = s.resolve(ObjectKind::Parcel, tok) else {
                        diags.push(unresolved(l, tok));
                        continue;
                    };
                    Some(p)
                }
                None => None,
            };
            pack.removal = Some(SnowRemoval {
                trigger_depth: x[0] * cv.rain_depth,
                fractions: [x[1], x[2], x[3], x[4], x[5]],
                to_parcel,
            });
            continue;
        }
        // Melt coefficients: depth per hour per degree → m/s per °C; the
        // base temperature converts to °C for US files.
        let tf = temp_factor(us);
        let dh = cv.conductivity * tf;
        let t_base = if us { (x[2] - 32.0) * 5.0 / 9.0 } else { x[2] };
        // Initial free water is clamped to capacity, as the predecessor
        // clamps it (an over-specified value is reduced, not refused).
        let fw = x[5].min(x[3] * x[4]);
        let surface = SnowSurface {
            dh_min: x[0] * dh,
            dh_max: x[1] * dh,
            t_base,
            fw_frac: x[3],
            init_depth: x[4] * cv.rain_depth,
            init_free_water: fw * cv.rain_depth,
            full_cover_depth: if kind == 0 {
                None
            } else {
                Some(x[6] * cv.rain_depth)
            },
        };
        match kind {
            0 => {
                pack.plow_fraction = x[6];
                pack.plowable = Some(surface);
            }
            1 => pack.impervious = Some(surface),
            _ => pack.pervious = Some(surface),
        }
    }
    packs
}

const MONTHS: &[&str] = &[
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

/// Parse `[HYDROGRAPHS]` — the two-token gage form, the modern per-month
/// response form, and the predecessor's legacy nine-value format.
pub(crate) fn parse_unit_hydrographs(
    lines: &[TokenLine<'_>],
    s: &Survey,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<UnitHydrographGroup> {
    const CLASSES: &[&str] = &["SHORT", "MEDIUM", "LONG"];
    let ids = s.ids.get(&ObjectKind::UnitHydrographGroup);
    let n_groups = ids.map_or(0, |m| m.len());
    let mut groups: Vec<UnitHydrographGroup> = (0..n_groups)
        .map(|_| UnitHydrographGroup {
            id: String::new(),
            gage: None,
            months: Box::new([[None; 3]; 12]),
        })
        .collect();

    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 2 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&idx) = ids.and_then(|m| m.get(t[0].to_ascii_uppercase().as_str())) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        let group = &mut groups[idx];
        if group.id.is_empty() {
            group.id = t[0].to_string();
        }
        // Two tokens: the gage assignment.
        if t.len() == 2 {
            let Some(&g) = s.resolve(ObjectKind::Gage, t[1]) else {
                diags.push(unresolved(l, t[1]));
                continue;
            };
            group.gage = Some(g);
            continue;
        }
        if t.len() < 6 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        // Month: a name, or ALL (prefix-matched, per the predecessor).
        let up = t[1].to_ascii_uppercase();
        let month: Option<usize> = MONTHS.iter().position(|m| up.starts_with(m));
        if month.is_none() && match_keyword(&["ALL"], t[1]).is_none() {
            diags.push(bad(l, t[1]));
            continue;
        }
        let read_response =
            |from: usize, t: &[&str], diags: &mut Vec<Diagnostic>| -> Option<UhResponse> {
                let mut x = [0.0; 6];
                for (i, xi) in x.iter_mut().enumerate().take(3) {
                    let Ok(v) = t.get(from + i)?.finite_f64() else {
                        diags.push(bad(l, t[from + i]));
                        return None;
                    };
                    *xi = v;
                }
                for (i, xi) in x.iter_mut().enumerate().skip(3) {
                    if let Some(tok) = t.get(from + i) {
                        let Ok(v) = tok.finite_f64() else {
                            diags.push(bad(l, tok));
                            return None;
                        };
                        *xi = v;
                    }
                }
                Some(UhResponse {
                    r: x[0],
                    t_peak: x[1] * 3600.0,
                    k: x[2],
                    // Predecessor field order: IAmax, IArecovery, IAinit.
                    ia_max: x[3] * cv.rain_depth,
                    ia_recovery: x[4] * cv.rain_depth,
                    ia_init: x[5] * cv.rain_depth,
                })
            };
        let assign = |group: &mut UnitHydrographGroup, class: usize, resp: UhResponse| match month {
            Some(m) => group.months[m][class] = Some(resp),
            None => {
                for m in 0..12 {
                    group.months[m][class] = Some(resp);
                }
            }
        };
        if let Some(class) = match_keyword(CLASSES, t[2]) {
            // Modern format: one response of one duration class.
            let Some(resp) = read_response(3, t, diags) else {
                continue;
            };
            assign(group, class, resp);
        } else {
            // Legacy format: nine R/T/K values (three responses) plus up
            // to three shared initial-abstraction parameters.
            if t.len() < 11 {
                diags.push(err(l, DiagnosticKind::MissingItems));
                continue;
            }
            let mut p = [0.0; 9];
            let mut ok = true;
            for (i, pi) in p.iter_mut().enumerate() {
                let Ok(v) = t[2 + i].finite_f64() else {
                    diags.push(bad(l, t[2 + i]));
                    ok = false;
                    break;
                };
                *pi = v;
            }
            if !ok {
                continue;
            }
            let mut ia = [0.0; 3];
            for (i, ii) in ia.iter_mut().enumerate() {
                if let Some(tok) = t.get(11 + i) {
                    let Ok(v) = tok.finite_f64() else {
                        diags.push(bad(l, tok));
                        ok = false;
                        break;
                    };
                    *ii = v;
                }
            }
            if !ok {
                continue;
            }
            for class in 0..3 {
                let resp = UhResponse {
                    r: p[3 * class],
                    t_peak: p[3 * class + 1] * 3600.0,
                    k: p[3 * class + 2],
                    // Predecessor field order: IAmax, IArecovery, IAinit,
                    // the recovery rate converting as a depth per day.
                    ia_max: ia[0] * cv.rain_depth,
                    ia_recovery: ia[1] * cv.rain_depth,
                    ia_init: ia[2] * cv.rain_depth,
                };
                assign(group, class, resp);
            }
        }
    }
    groups
}

/// Parse `[RDII]` assignments.
pub(crate) fn parse_rdii(
    lines: &[TokenLine<'_>],
    s: &Survey,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<RdiiInflow> {
    let mut out = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&vx) = s.resolve(ObjectKind::Vertex, t[0]) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        let Some(&g) = s.resolve(ObjectKind::UnitHydrographGroup, t[1]) else {
            diags.push(unresolved(l, t[1]));
            continue;
        };
        let Ok(area) = t[2].finite_f64() else {
            diags.push(bad(l, t[2]));
            continue;
        };
        if area < 0.0 {
            diags.push(bad(l, t[2]));
            continue;
        }
        let entry = RdiiInflow {
            vertex: vx,
            group: g,
            area: area * cv.land_area,
        };
        // §14.5: last definition for the vertex wins.
        match out.iter_mut().find(|e: &&mut RdiiInflow| e.vertex == vx) {
            Some(e) => {
                diags.push(Diagnostic {
                    line: l,
                    kind: DiagnosticKind::OverriddenDefinition {
                        what: "sewer-inflow",
                        id: t[0].to_string(),
                    },
                });
                *e = entry;
            }
            None => out.push(entry),
        }
    }
    out
}

/// Parse `[TREATMENT]` expressions: `vertex constituent R|C = expression`,
/// the expression retained as written.
pub(crate) fn parse_treatment(
    lines: &[TokenLine<'_>],
    s: &Survey,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Treatment> {
    let mut out = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&vx) = s.resolve(ObjectKind::Vertex, t[0]) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        let Some(&p) = s.resolve(ObjectKind::Constituent, t[1]) else {
            diags.push(unresolved(l, t[1]));
            continue;
        };
        // The predecessor joins the remaining tokens, keys the kind on the
        // first character (R or C), and takes everything past `=`.
        let joined = t[2..].join(" ");
        let kind = match joined.chars().next().map(|c| c.to_ascii_uppercase()) {
            Some('R') => TreatmentKind::Removal,
            Some('C') => TreatmentKind::Concentration,
            _ => {
                diags.push(bad(l, t[2]));
                continue;
            }
        };
        let Some(eq) = joined.find('=') else {
            diags.push(bad(l, t[2]));
            continue;
        };
        let entry = Treatment {
            vertex: vx,
            constituent: p,
            kind,
            expression: joined[eq + 1..].trim().to_string(),
        };
        // §14.5: last treatment line for the vertex/constituent wins —
        // both expressions applying would double-treat.
        match out
            .iter_mut()
            .find(|e: &&mut Treatment| e.vertex == vx && e.constituent == p)
        {
            Some(e) => {
                diags.push(Diagnostic {
                    line: l,
                    kind: DiagnosticKind::OverriddenDefinition {
                        what: "treatment",
                        id: t[0].to_string(),
                    },
                });
                *e = entry;
            }
            None => out.push(entry),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::io::objects::parse_network;
    use crate::model::TreatmentKind;

    const FIXTURE: &str = "\
[OPTIONS]
FLOW_UNITS  CFS

[RAINGAGES]
G1  VOLUME  1.0  1.0  FILE  f.dat

[JUNCTIONS]
J1  100  3

[SUBCATCHMENTS]
S1  G1  J1  10  25  500  0.5  0  SP1

[SUBAREAS]
S1  0.01  0.1  0.05  0.05  25  OUTLET

[POLLUTANTS]
TSS  MG/L  0  0  0  0

[SNOWPACKS]
SP1  PLOWABLE  0.001  0.003  32  0.10  0  0  0.5
SP1  IMPERV    0.001  0.003  32  0.10  1.0  0.5  4.0
SP1  REMOVAL   6.0  0.3  0.2  0.1  0.1  0.3  S1

[HYDROGRAPHS]
UH1  G1
UH1  ALL  SHORT  0.033  1.0  2.0
UH1  JUL  MEDIUM  0.05  4.0  2.0  0.1  0  0.5

[RDII]
J1  UH1  40

[TREATMENT]
J1  TSS  R = 0.2 * FLOW
";

    #[test]
    fn snowpacks_assemble_across_lines_with_conversions() {
        let (net, diags) = parse_network(FIXTURE);
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "{:?}",
            diags
                .iter()
                .filter(|d| d.kind.is_error())
                .collect::<Vec<_>>()
        );
        let sp = &net.snowpacks[0];
        assert_eq!(sp.plow_fraction, 0.5);
        let plow = sp.plowable.as_ref().unwrap();
        // 32 °F → 0 °C; 0.001 in/hr/°F → m/s/°C.
        assert!(plow.t_base.abs() < 1e-12);
        assert!((plow.dh_min - 0.001 * 0.0254 / 3600.0 * 1.8).abs() < 1e-18);
        assert_eq!(plow.full_cover_depth, None);
        let imp = sp.impervious.as_ref().unwrap();
        // Free water 0.5 clamped to fw_frac × depth = 0.1.
        assert!((imp.init_free_water - 0.1 * 0.0254).abs() < 1e-12);
        assert!((imp.full_cover_depth.unwrap() - 4.0 * 0.0254).abs() < 1e-12);
        let rem = sp.removal.as_ref().unwrap();
        assert!((rem.trigger_depth - 6.0 * 0.0254).abs() < 1e-12);
        assert_eq!(rem.to_parcel, Some(0));
        // The parcel references the pack.
        assert_eq!(net.parcels[0].snowpack, Some(0));
    }

    #[test]
    fn unit_hydrographs_take_all_months_and_single_month_forms() {
        let (net, _) = parse_network(FIXTURE);
        let uh = &net.unit_hydrographs[0];
        assert_eq!(uh.gage, Some(0));
        // ALL SHORT: every month's class 0 filled.
        assert!(uh.months.iter().all(|m| m[0].is_some()));
        let short = uh.months[0][0].unwrap();
        assert_eq!(short.r, 0.033);
        assert_eq!(short.t_peak, 3600.0);
        // JUL MEDIUM only in month 7 (index 6).
        assert!(uh.months[6][1].is_some());
        assert!(uh.months[0][1].is_none());
        let med = uh.months[6][1].unwrap();
        assert!((med.ia_max - 0.1 * 0.0254).abs() < 1e-12);
    }

    #[test]
    fn rdii_assignments_convert_their_sewershed_area() {
        let (net, _) = parse_network(FIXTURE);
        let r = &net.rdii[0];
        assert_eq!(r.vertex, 0);
        assert_eq!(r.group, 0);
        assert!((r.area - 40.0 * 4_046.856_422_4).abs() < 1e-6);
    }

    #[test]
    fn treatment_expressions_are_retained_as_written() {
        let (net, _) = parse_network(FIXTURE);
        let tr = &net.treatments[0];
        assert_eq!(tr.kind, TreatmentKind::Removal);
        assert_eq!(tr.expression, "0.2 * FLOW");
    }
}
