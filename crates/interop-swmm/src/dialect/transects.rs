//! Transect parsing (§5.6): the NC/X1/GR record grammar, with the
//! predecessor's shared-state defects closed by construction.
//!
//! The reader keeps a rolling roughness state that `NC` lines update (a
//! zero inheriting the prior value) and each `X1` captures — that much is
//! the file grammar. What is *not* carried: a transect here is complete
//! when its record ends, so a missing trailing `NC` cannot leave it empty;
//! and the meander factor belongs to the one transect that declared it,
//! so the $\sqrt{L}$ roughness inflation cannot compound into later
//! transects (§5.6's CORRESPONDENCE notes).

use crate::dialect::lex::FiniteParse;
use crate::dialect::objects::UnitConverter;
use crate::dialect::survey::{Diagnostic, DiagnosticKind, TokenLine};
use crate::engine_api::model::Transect;

/// The predecessor's station cap per transect.
const MAX_STATIONS: usize = 1500;

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

/// Parse a `[TRANSECTS]` section.
pub(crate) fn parse_transects(
    lines: &[TokenLine<'_>],
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Transect> {
    let mut out: Vec<Transect> = Vec::new();
    // Rolling roughness state: NC updates it, zero inherits.
    let mut n = (0.0_f64, 0.0_f64, 0.0_f64);
    // Per-current-transect station transforms.
    let mut x_factor = 1.0;
    let mut y_offset = 0.0;

    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        let key = t[0].to_ascii_uppercase();
        match key.as_str() {
            "NC" => {
                if t.len() < 4 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let mut v = [0.0; 3];
                let mut ok = true;
                for (i, vi) in v.iter_mut().enumerate() {
                    let Ok(x) = t[1 + i].finite_f64() else {
                        diags.push(bad(l, t[1 + i]));
                        ok = false;
                        break;
                    };
                    if x < 0.0 {
                        diags.push(bad(l, t[1 + i]));
                        ok = false;
                        break;
                    }
                    *vi = x;
                }
                if !ok {
                    continue;
                }
                // Zero inherits the prior value — the predecessor's rule.
                if v[0] > 0.0 {
                    n.0 = v[0];
                }
                if v[1] > 0.0 {
                    n.1 = v[1];
                }
                if v[2] > 0.0 {
                    n.2 = v[2];
                }
            }
            "X1" => {
                if t.len() < 10 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let mut x = [0.0; 8]; // tok2..tok9
                let mut ok = true;
                for (i, xi) in x.iter_mut().enumerate() {
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
                // tok2 = station count (informational), tok3/4 = bank
                // stations, tok5/6 unused, tok7 = meander factor, tok8 =
                // station multiplier, tok9 = elevation offset.
                x_factor = if x[6] == 0.0 { 1.0 } else { x[6] };
                y_offset = x[7] * cv.len;
                let meander = if x[5] == 0.0 { 1.0 } else { x[5] };
                out.push(Transect {
                    id: t[1].to_string(),
                    n_left: n.0,
                    n_right: n.1,
                    n_channel: n.2,
                    x_left: x[1] * x_factor * cv.len,
                    x_right: x[2] * x_factor * cv.len,
                    meander_factor: meander,
                    stations: Vec::new(),
                });
            }
            "GR" => {
                if (t.len() - 1) % 2 != 0 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let Some(current) = out.last_mut() else {
                    // A GR before any X1: the predecessor would be
                    // sectionless here; refuse the line.
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                };
                let mut k = 1;
                while k + 1 < t.len() {
                    let (Ok(elev), Ok(station)) = (t[k].finite_f64(), t[k + 1].finite_f64()) else {
                        diags.push(bad(l, t[k]));
                        break;
                    };
                    if current.stations.len() >= MAX_STATIONS {
                        diags.push(bad(l, t[k]));
                        break;
                    }
                    current
                        .stations
                        .push((elev * cv.len + y_offset, station * x_factor * cv.len));
                    k += 2;
                }
            }
            _ => {
                diags.push(bad(l, t[0]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::dialect::objects::parse_network;

    const FIXTURE: &str = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  100  3
J2  99   3

[CONDUITS]
C1  J1  J2  500  0.04  0  0

[XSECTIONS]
C1  IRREGULAR  T1

[TRANSECTS]
NC  0.05  0.06  0.03
X1  T1  4  10  30  0  0  1.5  2.0  1.0
GR  8  0   2  10  2  30  8  40
NC  0     0     0.04
X1  T2  2  0  0  0  0  0  0  0
GR  5  0  1  10
";

    #[test]
    fn nc_state_rolls_with_zero_inheriting() {
        let (net, diags) = parse_network(FIXTURE);
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "{:?}",
            diags
                .iter()
                .filter(|d| d.kind.is_error())
                .collect::<Vec<_>>()
        );
        let t1 = &net.transects[0];
        assert_eq!((t1.n_left, t1.n_right, t1.n_channel), (0.05, 0.06, 0.03));
        // T2's NC gave zeros for the overbanks: inherited; channel updated.
        let t2 = &net.transects[1];
        assert_eq!((t2.n_left, t2.n_right, t2.n_channel), (0.05, 0.06, 0.04));
    }

    #[test]
    fn a_transect_is_complete_without_a_trailing_nc() {
        // T2 is the last record and no NC follows — under the predecessor
        // it would be left untabulated; here it is complete.
        let (net, _) = parse_network(FIXTURE);
        assert_eq!(net.transects.len(), 2);
        assert_eq!(net.transects[1].stations.len(), 2);
    }

    #[test]
    fn multipliers_and_offsets_apply_per_transect() {
        let (net, _) = parse_network(FIXTURE);
        let t1 = &net.transects[0];
        assert!((t1.meander_factor - 1.5).abs() < 1e-12);
        // Station 10 × multiplier 2 × ft→m; elevation 8 + offset 1, ft→m.
        assert!((t1.x_left - 10.0 * 2.0 * 0.3048).abs() < 1e-12);
        let (elev0, sta0) = t1.stations[0];
        assert!((elev0 - (8.0 + 1.0) * 0.3048).abs() < 1e-12);
        assert!((sta0 - 0.0).abs() < 1e-12);
        let (_, sta1) = t1.stations[1];
        assert!((sta1 - 10.0 * 2.0 * 0.3048).abs() < 1e-12);
        // T2 declared zero multiplier/offset: defaults, per the predecessor.
        let t2 = &net.transects[1];
        assert_eq!(t2.meander_factor, 1.0);
    }
}
