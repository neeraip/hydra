//! Control-measure parsing (§3.4): `[LID_CONTROLS]` designs assembled from
//! their layer lines, and `[LID_USAGE]` deployments.
//!
//! Depths and heads convert from the surface-depth unit (inches or
//! millimetres), rates from in/hr or mm/hr, deployment areas from ft² or
//! m². The underdrain power-relation coefficient converts to SI-dimensional
//! form per its exponent (§14.6); its multiplier curve stays in file units,
//! looked up at file-unit head by the §3 evaluation.

use super::keywords::match_keyword;
use super::objects::UnitConverter;
use super::survey::{Diagnostic, DiagnosticKind, ObjectKind, Survey, TokenLine};
use crate::io::lex::FiniteParse;
use crate::model::{
    LidControl, LidDrain, LidDrainMat, LidKind, LidPavement, LidSoil, LidStorage, LidSurface,
    LidUsage, ParcelOutlet,
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

/// The unit-type codes, in the predecessor's table order.
const LID_TYPES: &[&str] = &["BC", "RG", "GR", "IT", "PP", "RB", "VS", "RD"];
const LID_KINDS: [LidKind; 8] = [
    LidKind::BioRetention,
    LidKind::RainGarden,
    LidKind::GreenRoof,
    LidKind::InfiltrationTrench,
    LidKind::PermeablePavement,
    LidKind::RainBarrel,
    LidKind::VegetativeSwale,
    LidKind::RooftopDisconnection,
];

/// The layer keywords, in the predecessor's table order (`DRAINMAT` before
/// `DRAIN`, or the mat could never be named).
const LID_LAYERS: &[&str] = &[
    "SURFACE", "SOIL", "STORAGE", "PAVEMENT", "DRAINMAT", "DRAIN", "REMOVALS",
];

/// Read `count` non-negative numbers starting at token `at`.
fn numbers<const N: usize>(
    t: &[&str],
    at: usize,
    diags: &mut Vec<Diagnostic>,
    l: usize,
) -> Option<[f64; N]> {
    let mut x = [0.0; N];
    for (i, xi) in x.iter_mut().enumerate() {
        let Ok(v) = t[at + i].finite_f64() else {
            diags.push(bad(l, t[at + i]));
            return None;
        };
        if v < 0.0 {
            diags.push(bad(l, t[at + i]));
            return None;
        }
        *xi = v;
    }
    Some(x)
}

/// Parse a `[LID_CONTROLS]` section.
pub(crate) fn parse_lid_controls(
    lines: &[TokenLine<'_>],
    s: &Survey,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<LidControl> {
    let ids = s.ids.get(&ObjectKind::ControlMeasure);
    let n = ids.map_or(0, |m| m.len());
    let mut out: Vec<LidControl> = vec![LidControl::default(); n];
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
        let ctrl = &mut out[idx];
        if ctrl.id.is_empty() {
            ctrl.id = t[0].to_string();
        }
        // A unit-type code first; a layer keyword otherwise.
        if let Some(k) = match_keyword(LID_TYPES, t[1]) {
            if !t[1].eq_ignore_ascii_case(LID_TYPES[k]) {
                diags.push(err(
                    l,
                    DiagnosticKind::PrefixMatched {
                        token: t[1].to_string(),
                        matched: LID_TYPES[k],
                    },
                ));
            }
            ctrl.kind = Some(LID_KINDS[k]);
            continue;
        }
        let Some(layer) = match_keyword(LID_LAYERS, t[1]) else {
            diags.push(bad(l, t[1]));
            continue;
        };
        if !t[1].eq_ignore_ascii_case(LID_LAYERS[layer]) {
            diags.push(err(
                l,
                DiagnosticKind::PrefixMatched {
                    token: t[1].to_string(),
                    matched: LID_LAYERS[layer],
                },
            ));
        }
        match layer {
            // SURFACE: berm height, vegetative volume fraction, roughness,
            // slope (%), swale side slope.
            0 => {
                if t.len() < 7 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let Some(mut x) = numbers::<5>(t, 2, diags, l) else {
                    continue;
                };
                if x[1] >= 1.0 {
                    diags.push(bad(l, t[3]));
                    continue;
                }
                // No berm: the vegetative fraction is meaningless.
                if x[0] == 0.0 {
                    x[1] = 0.0;
                }
                ctrl.surface = Some(LidSurface {
                    thickness: x[0] * cv.rain_depth,
                    void_frac: 1.0 - x[1],
                    roughness: x[2],
                    slope: x[3] / 100.0,
                    side_slope: x[4],
                });
            }
            // SOIL: thickness, porosity, field capacity, wilting point,
            // Ksat, Kslope, suction.
            1 => {
                if t.len() < 9 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let Some(x) = numbers::<7>(t, 2, diags, l) else {
                    continue;
                };
                ctrl.soil = Some(LidSoil {
                    thickness: x[0] * cv.rain_depth,
                    porosity: x[1],
                    field_capacity: x[2],
                    wilting_point: x[3],
                    k_sat: x[4] * cv.conductivity,
                    k_slope: x[5],
                    suction: x[6] * cv.rain_depth,
                });
            }
            // STORAGE: thickness, void ratio, Ksat, clog factor, covered.
            2 => {
                if t.len() < 6 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let Some(x) = numbers::<4>(t, 2, diags, l) else {
                    continue;
                };
                let covered = t.len() > 6 && match_keyword(&["YES"], t[6]).is_some();
                ctrl.storage = Some(LidStorage {
                    thickness: x[0] * cv.rain_depth,
                    void_frac: x[1] / (x[1] + 1.0),
                    k_sat: x[2] * cv.conductivity,
                    clog_factor: x[3],
                    covered,
                });
            }
            // PAVEMENT: thickness, void ratio, impervious fraction,
            // permeability, clog factor (+ regeneration days, degree).
            3 => {
                if t.len() < 7 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let Some(x) = numbers::<5>(t, 2, diags, l) else {
                    continue;
                };
                let mut regen_days = 0.0;
                if t.len() > 7 {
                    match t[7].finite_f64() {
                        Ok(v) if v >= 0.0 => regen_days = v,
                        _ => {
                            diags.push(bad(l, t[7]));
                            continue;
                        }
                    }
                }
                let mut regen_degree = 0.0;
                if t.len() > 8 {
                    match t[8].finite_f64() {
                        Ok(v) if (0.0..=1.0).contains(&v) => regen_degree = v,
                        _ => {
                            diags.push(bad(l, t[8]));
                            continue;
                        }
                    }
                }
                ctrl.pavement = Some(LidPavement {
                    thickness: x[0] * cv.rain_depth,
                    void_frac: x[1] / (x[1] + 1.0),
                    imperv_frac: x[2],
                    k_sat: x[3] * cv.conductivity,
                    clog_factor: x[4],
                    regen_days,
                    regen_degree,
                });
            }
            // DRAINMAT: thickness, void fraction, roughness — green roofs
            // only; the predecessor drops the line silently otherwise, and
            // the accept-set is its.
            4 => {
                if t.len() < 5 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                if ctrl.kind != Some(LidKind::GreenRoof) {
                    continue;
                }
                let Some(x) = numbers::<3>(t, 2, diags, l) else {
                    continue;
                };
                ctrl.drain_mat = Some(LidDrainMat {
                    thickness: x[0] * cv.rain_depth,
                    void_frac: x[1],
                    roughness: x[2],
                });
            }
            // DRAIN: coeff, exponent, offset, delay (+ hOpen, hClose,
            // curve). Trailing values are optional and default to zero.
            // The relation is q = C·h^e in rain-rate/rain-depth units, so
            // C converts by the rate factor over the depth factor to the
            // power e (§14.6).
            5 => {
                if t.len() < 6 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let mut x = [0.0_f64; 6];
                let mut ok = true;
                for (i, xi) in x.iter_mut().enumerate() {
                    if t.len() <= 2 + i {
                        break;
                    }
                    match t[2 + i].finite_f64() {
                        Ok(v) if v >= 0.0 => *xi = v,
                        _ => {
                            diags.push(bad(l, t[2 + i]));
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                let curve = if t.len() >= 9 {
                    match s.resolve(ObjectKind::Curve, t[8]) {
                        Some(&c) => Some(c),
                        None => {
                            diags.push(unresolved(l, t[8]));
                            continue;
                        }
                    }
                } else {
                    None
                };
                ctrl.drain = Some(LidDrain {
                    coeff: x[0] * cv.conductivity / cv.rain_depth.powf(x[1]),
                    exponent: x[1],
                    offset: x[2] * cv.rain_depth,
                    delay: x[3] * 3600.0,
                    h_open: x[4] * cv.rain_depth,
                    h_close: x[5] * cv.rain_depth,
                    curve,
                });
            }
            // REMOVALS: pollutant / % pairs; last declaration wins.
            _ => {
                if t.len() < 4 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let mut i = 2;
                while t.len() > i {
                    let Some(&p) = s.resolve(ObjectKind::Constituent, t[i]) else {
                        diags.push(unresolved(l, t[i]));
                        break;
                    };
                    i += 1;
                    if t.len() == i {
                        diags.push(err(l, DiagnosticKind::MissingItems));
                        break;
                    }
                    let rmvl = match t[i].finite_f64() {
                        Ok(v) if (0.0..=100.0).contains(&v) => v,
                        _ => {
                            diags.push(bad(l, t[i]));
                            break;
                        }
                    };
                    match ctrl.removals.iter_mut().find(|(q, _)| *q == p) {
                        Some(entry) => entry.1 = rmvl / 100.0,
                        None => ctrl.removals.push((p, rmvl / 100.0)),
                    }
                    i += 1;
                }
            }
        }
    }
    out
}

/// The predecessor's replicate-count read is C `atoi`: the leading integer
/// prefix, zero when there is none.
fn atoi(token: &str) -> i64 {
    let t = token.trim_start();
    let (sign, digits) = match t.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, t.strip_prefix('+').unwrap_or(t)),
    };
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end].parse::<i64>().map_or(0, |v| sign * v)
}

/// Parse a `[LID_USAGE]` section.
pub(crate) fn parse_lid_usage(
    lines: &[TokenLine<'_>],
    s: &Survey,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<LidUsage> {
    let mut out = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 8 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&parcel) = s.resolve(ObjectKind::Parcel, t[0]) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        let Some(&control) = s.resolve(ObjectKind::ControlMeasure, t[1]) else {
            diags.push(unresolved(l, t[1]));
            continue;
        };
        let n = atoi(t[2]);
        if n < 0 {
            diags.push(bad(l, t[2]));
            continue;
        }
        // Zero replicates: the predecessor drops the line without comment.
        if n == 0 {
            continue;
        }
        // Area, width, initial saturation %, impervious capture %, and the
        // to-pervious flag; the two percentages are capped at 100.
        let Some(x) = numbers::<5>(t, 3, diags, l) else {
            continue;
        };
        if x[2] > 100.0 {
            diags.push(bad(l, t[5]));
            continue;
        }
        if x[3] > 100.0 {
            diags.push(bad(l, t[6]));
            continue;
        }
        let report_file = (t.len() >= 9 && t[8] != "*").then(|| t[8].to_string());
        let drain_to = if t.len() >= 10 && t[9] != "*" {
            // A parcel first, then a vertex — the predecessor's order.
            if let Some(&p) = s.resolve(ObjectKind::Parcel, t[9]) {
                Some(ParcelOutlet::Parcel(p))
            } else if let Some(&v) = s.resolve(ObjectKind::Vertex, t[9]) {
                Some(ParcelOutlet::Vertex(v))
            } else {
                diags.push(unresolved(l, t[9]));
                continue;
            }
        } else {
            None
        };
        let mut from_pervious = 0.0;
        if t.len() >= 11 {
            match t[10].finite_f64() {
                Ok(v) if (0.0..=100.0).contains(&v) => from_pervious = v / 100.0,
                _ => {
                    diags.push(bad(l, t[10]));
                    continue;
                }
            }
        }
        out.push(LidUsage {
            parcel,
            control,
            count: u32::try_from(n).unwrap_or(u32::MAX),
            area: x[0] * cv.len * cv.len,
            width: x[1] * cv.len,
            init_saturation: x[2] / 100.0,
            from_impervious: x[3] / 100.0,
            to_pervious: x[4] > 0.0,
            from_pervious,
            report_file,
            drain_to,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::atoi;
    use crate::io::objects::parse_network;
    use crate::model::{LidKind, ParcelOutlet};

    const FIXTURE: &str = "\
[OPTIONS]
FLOW_UNITS  CFS

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  TS1

[TIMESERIES]
TS1  0:00  1.0

[JUNCTIONS]
J1  100  3

[OUTFALLS]
O1  95  FREE

[CONDUITS]
C1  J1  O1  400  0.01  0  0

[XSECTIONS]
C1  CIRCULAR  1  0  0  0

[SUBCATCHMENTS]
S1  G1  J1  10  25  500  0.5  0
S2  G1  J1  5   50  400  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET
S2  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0
S2  3.0  0.5  4  7  0

[POLLUTANTS]
TSS  MG/L  0  0  0  0

[CURVES]
DC1  RATING  0  0  1  2

[LID_CONTROLS]
BR1  BC
BR1  SURFACE   6    0.25  0.1   1.0  5
BR1  SOIL      24   0.5   0.2   0.1  0.5  10  3.5
BR1  STORAGE   12   0.75  0.5   0
BR1  DRAIN     2    0.5   6     0    0    0   DC1
BR1  REMOVALS  TSS  40
GRF  GR
GRF  SURFACE   3    0.1   0.15  1.0  5
GRF  DRAINMAT  2    0.5   0.1
PPV  PP
PPV  PAVEMENT  6    0.15  0     100  0    2   0.5

[LID_USAGE]
S1  BR1  2  500  10  0  35  1  *  J1  15
S1  PPV  0  100  5   0  0   0
S2  GRF  1  200  8   0  0   0  rpt.txt
";

    fn net_ok() -> crate::model::Network {
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
    fn layers_assemble_with_conversions() {
        let net = net_ok();
        let br = &net.lid_controls[0];
        assert_eq!(br.kind, Some(LidKind::BioRetention));
        let surf = br.surface.as_ref().unwrap();
        // 6 in berm; void = 1 − 0.25; slope 1% → 0.01.
        assert!((surf.thickness - 6.0 * 0.0254).abs() < 1e-12);
        assert!((surf.void_frac - 0.75).abs() < 1e-12);
        assert!((surf.slope - 0.01).abs() < 1e-12);
        let soil = br.soil.as_ref().unwrap();
        // Ksat 0.5 in/hr; suction 3.5 in.
        assert!((soil.k_sat - 0.5 * 0.0254 / 3600.0).abs() < 1e-15);
        assert!((soil.suction - 3.5 * 0.0254).abs() < 1e-12);
        let stor = br.storage.as_ref().unwrap();
        // Void ratio 0.75 → fraction 0.75/1.75.
        assert!((stor.void_frac - 0.75 / 1.75).abs() < 1e-12);
        assert!(!stor.covered);
        let drain = br.drain.as_ref().unwrap();
        // q = C·h^e in in/hr and inches converts per the exponent: the
        // rate factor over the depth factor to the power e (§14.6).
        assert_eq!(drain.exponent, 0.5);
        let expect = 2.0 * (0.0254 / 3600.0) / 0.0254_f64.powf(0.5);
        assert!((drain.coeff - expect).abs() < 1e-15);
        assert!((drain.offset - 6.0 * 0.0254).abs() < 1e-12);
        assert_eq!(drain.curve, Some(0));
        assert_eq!(br.removals, vec![(0, 0.4)]);
    }

    #[test]
    fn drainmat_applies_to_green_roofs_only() {
        let net = net_ok();
        assert!(net.lid_controls[1].drain_mat.is_some());
        // A pavement design keeps regeneration extras.
        let pv = net.lid_controls[2].pavement.as_ref().unwrap();
        assert!((pv.void_frac - 0.15 / 1.15).abs() < 1e-12);
        assert_eq!((pv.regen_days, pv.regen_degree), (2.0, 0.5));
    }

    #[test]
    fn usage_reads_flags_and_optional_tail() {
        let net = net_ok();
        // The zero-replicate line vanishes without diagnostics.
        assert_eq!(net.lid_usage.len(), 2);
        let u = &net.lid_usage[0];
        assert_eq!((u.parcel, u.control, u.count), (0, 0, 2));
        assert!((u.area - 500.0 * 0.3048 * 0.3048).abs() < 1e-9);
        assert!((u.from_impervious - 0.35).abs() < 1e-12);
        assert!(u.to_pervious);
        assert!((u.from_pervious - 0.15).abs() < 1e-12);
        assert_eq!(u.report_file, None);
        assert_eq!(u.drain_to, Some(ParcelOutlet::Vertex(0)));
        let g = &net.lid_usage[1];
        assert!(!g.to_pervious);
        assert_eq!(g.report_file.as_deref(), Some("rpt.txt"));
        assert_eq!(g.drain_to, None);
    }

    #[test]
    fn replicate_count_is_c_atoi() {
        assert_eq!(atoi("3"), 3);
        assert_eq!(atoi("3.9"), 3);
        assert_eq!(atoi("-2"), -2);
        assert_eq!(atoi("x"), 0);
        assert_eq!(atoi(""), 0);
    }
}
