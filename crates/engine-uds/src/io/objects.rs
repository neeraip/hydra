//! Object-section parsing: the conveyance graph into §2 domain types.
//!
//! Grammars are the predecessor's, field for field, read from its source.
//! Values convert to SI at this boundary — lengths by exact 0.3048 for
//! US-unit files (the predecessor's US path never converts, so the factor
//! is this engine's to define, and it is exact per §1.7), flows by the
//! exact factor of the file's flow unit — except where §14.6 declares a
//! coefficient unit-dependent, which stays as written for its owning
//! section's treatment.

use super::keywords::{match_keyword, Section};
use super::options::{parse_options, AnalysisOptions, FlowUnits, LinkOffsets};
use super::survey::{survey, Diagnostic, DiagnosticKind, ObjectKind, Survey, TokenLine};
use crate::model::{
    CrossSection, DividerRule, Link, LinkKind, Network, Offset, OrificeOrientation, OutfallStage,
    OutletHeadBasis, OutletRating, RoadSurface, StorageGeometry, StorageSeepage, StorageShapeKind,
    Vertex, VertexKind, WeirForm, XsectReferent, XsectShape,
};

/// Parse a complete input into a [`Network`], reporting every diagnostic.
///
/// Sections outside the conveyance graph are surveyed and retained but not
/// yet interpreted (their increments follow); their identifiers are already
/// registered and resolvable.
pub fn parse_network(input: &str) -> (Network, Vec<Diagnostic>) {
    let s = survey(input);
    let mut diagnostics = s.diagnostics.clone();

    // Options first, wherever the section sits: offset conventions and unit
    // conversions below depend on them.
    let mut options = AnalysisOptions::default();
    for (sec, lines) in &s.sections {
        if *sec == Section::Options {
            options = parse_options(lines, &mut diagnostics);
        }
    }

    let mut net = Network {
        title: s.title.clone(),
        curve_ids: ordered_ids(&s, ObjectKind::Curve),
        timeseries_ids: ordered_ids(&s, ObjectKind::TimeSeries),
        parcel_ids: ordered_ids(&s, ObjectKind::Parcel),
        transect_ids: ordered_ids(&s, ObjectKind::Transect),
        street_ids: ordered_ids(&s, ObjectKind::Street),
        ..Default::default()
    };

    let cv = Converter::new(options.flow_units, options.link_offsets);

    // Vertices and links, in file order (identical to registration order).
    for (sec, lines) in &s.sections {
        for line in lines {
            let d_before = diagnostics.len();
            match sec {
                Section::Junctions => parse_junction(&mut net, &cv, line, &mut diagnostics),
                Section::Outfalls => parse_outfall(&mut net, &s, &cv, line, &mut diagnostics),
                Section::Storage => parse_storage(&mut net, &s, &cv, line, &mut diagnostics),
                Section::Dividers => parse_divider(&mut net, &s, &cv, line, &mut diagnostics),
                Section::Conduits => parse_conduit(&mut net, &s, &cv, line, &mut diagnostics),
                Section::Pumps => parse_pump(&mut net, &s, &cv, line, &mut diagnostics),
                Section::Orifices => parse_orifice(&mut net, &s, &cv, line, &mut diagnostics),
                Section::Weirs => parse_weir(&mut net, &s, &cv, line, &mut diagnostics),
                Section::Outlets => parse_outlet(&mut net, &s, &cv, line, &mut diagnostics),
                _ => {}
            }
            // A failed object line must still occupy its registered slot, so
            // later indices stay aligned; a placeholder keeps the invariant
            // and the error diagnostics already refuse the file.
            if diagnostics.len() > d_before {
                realign(&mut net, &s, *sec, line);
            }
        }
    }

    // Cross-sections attach to parsed links.
    for (sec, lines) in &s.sections {
        if *sec == Section::XSections {
            for line in lines {
                parse_xsection(&mut net, &s, line, &mut diagnostics);
            }
        }
    }

    net.options = options;
    (net, diagnostics)
}

fn ordered_ids(s: &Survey, kind: ObjectKind) -> Vec<String> {
    let Some(map) = s.ids.get(&kind) else {
        return Vec::new();
    };
    let mut v = vec![String::new(); map.len()];
    for (id, &i) in map {
        v[i].clone_from(id);
    }
    v
}

/// Unit conversion at the import boundary.
struct Converter {
    /// m per file length unit (exact 0.3048 for US-unit files).
    len: f64,
    /// m³/s per file flow unit.
    flow: f64,
    /// m per file suction-head unit (inches or millimetres).
    suction: f64,
    /// m/s per file conductivity unit (in/hr or mm/hr).
    conductivity: f64,
    offsets: LinkOffsets,
}

impl Converter {
    fn new(units: FlowUnits, offsets: LinkOffsets) -> Self {
        let us = units.is_us();
        Converter {
            len: if us { 0.3048 } else { 1.0 },
            flow: match units {
                FlowUnits::Cfs => 0.028_316_846_592,
                FlowUnits::Gpm => 6.309_019_64e-5,
                FlowUnits::Mgd => 0.043_812_636_4,
                FlowUnits::Cms => 1.0,
                FlowUnits::Lps => 1.0e-3,
                FlowUnits::Mld => 1.0 / 86.4,
            },
            suction: if us { 0.0254 } else { 1.0e-3 },
            conductivity: if us { 0.0254 / 3600.0 } else { 1.0e-3 / 3600.0 },
            offsets,
        }
    }

    fn offset(&self, token: &str, diags: &mut Vec<Diagnostic>, line: usize) -> Option<Offset> {
        if self.offsets == LinkOffsets::Elevation && token == "*" {
            return Some(Offset::Missing);
        }
        let v = number(token, diags, line)?;
        Some(match self.offsets {
            LinkOffsets::Depth => Offset::Depth(v * self.len),
            LinkOffsets::Elevation => Offset::Elevation(v * self.len),
        })
    }
}

fn err(line: usize, kind: DiagnosticKind) -> Diagnostic {
    Diagnostic { line, kind }
}

fn number(token: &str, diags: &mut Vec<Diagnostic>, line: usize) -> Option<f64> {
    match token.parse::<f64>() {
        Ok(v) => Some(v),
        Err(_) => {
            diags.push(err(
                line,
                DiagnosticKind::BadValue {
                    token: token.to_string(),
                },
            ));
            None
        }
    }
}

fn opt_number(
    tokens: &[String],
    i: usize,
    default: f64,
    diags: &mut Vec<Diagnostic>,
    line: usize,
) -> Option<f64> {
    match tokens.get(i) {
        Some(t) => number(t, diags, line),
        None => Some(default),
    }
}

fn need(tokens: &[String], n: usize, diags: &mut Vec<Diagnostic>, line: usize) -> bool {
    if tokens.len() < n {
        diags.push(err(line, DiagnosticKind::MissingItems));
        return false;
    }
    true
}

fn resolve(
    s: &Survey,
    kind: ObjectKind,
    id: &str,
    diags: &mut Vec<Diagnostic>,
    line: usize,
) -> Option<usize> {
    match s.ids.get(&kind).and_then(|m| m.get(id)) {
        Some(&i) => Some(i),
        None => {
            diags.push(err(
                line,
                DiagnosticKind::UnresolvedReference { id: id.to_string() },
            ));
            None
        }
    }
}

fn keyword(
    table: &[&'static str],
    token: &str,
    diags: &mut Vec<Diagnostic>,
    line: usize,
) -> Option<usize> {
    match match_keyword(table, token) {
        Some(i) => {
            if !token.eq_ignore_ascii_case(table[i]) {
                diags.push(err(
                    line,
                    DiagnosticKind::PrefixMatched {
                        token: token.to_string(),
                        matched: table[i],
                    },
                ));
            }
            Some(i)
        }
        None => {
            diags.push(err(
                line,
                DiagnosticKind::BadValue {
                    token: token.to_string(),
                },
            ));
            None
        }
    }
}

/// Keep vertex/link indices aligned with the registry when a line fails:
/// push a placeholder for the slot its identifier already registered.
fn realign(net: &mut Network, s: &Survey, sec: Section, line: &TokenLine) {
    let is_vertex = matches!(
        sec,
        Section::Junctions | Section::Outfalls | Section::Storage | Section::Dividers
    );
    let id = line.tokens.first().cloned().unwrap_or_default();
    if is_vertex {
        if let Some(&idx) = s.ids[&ObjectKind::Vertex].get(&id) {
            if net.vertices.len() == idx {
                net.vertices.push(Vertex {
                    id,
                    invert: 0.0,
                    kind: VertexKind::Junction {
                        max_depth: 0.0,
                        init_depth: 0.0,
                        surcharge_depth: 0.0,
                        ponded_area: 0.0,
                    },
                });
            }
        }
    } else if let Some(&idx) = s.ids[&ObjectKind::Link].get(&id) {
        if net.links.len() == idx {
            net.links.push(Link {
                id,
                from: 0,
                to: 0,
                kind: LinkKind::Pump {
                    curve: None,
                    initial_on: true,
                    startup_depth: 0.0,
                    shutoff_depth: 0.0,
                },
                cross_section: None,
            });
        }
    }
}

// ── Vertices ─────────────────────────────────────────────────────────────

fn parse_junction(
    net: &mut Network,
    cv: &Converter,
    line: &TokenLine,
    diags: &mut Vec<Diagnostic>,
) {
    let t = &line.tokens;
    let l = line.line;
    if !need(t, 2, diags, l) {
        return;
    }
    let Some(invert) = number(&t[1], diags, l) else {
        return;
    };
    let mut x = [0.0; 4]; // max, init, surcharge, ponded area
    for (i, xi) in x.iter_mut().enumerate() {
        let Some(v) = opt_number(t, i + 2, 0.0, diags, l) else {
            return;
        };
        if v < 0.0 {
            diags.push(err(
                l,
                DiagnosticKind::BadValue {
                    token: t[i + 2].clone(),
                },
            ));
            return;
        }
        *xi = v;
    }
    net.vertices.push(Vertex {
        id: t[0].clone(),
        invert: invert * cv.len,
        kind: VertexKind::Junction {
            max_depth: x[0] * cv.len,
            init_depth: x[1] * cv.len,
            surcharge_depth: x[2] * cv.len,
            ponded_area: x[3] * cv.len * cv.len,
        },
    });
}

fn parse_outfall(
    net: &mut Network,
    s: &Survey,
    cv: &Converter,
    line: &TokenLine,
    diags: &mut Vec<Diagnostic>,
) {
    const TYPES: &[&str] = &["FREE", "NORMAL", "FIXED", "TIDAL", "TIMESERIES"];
    let t = &line.tokens;
    let l = line.line;
    if !need(t, 3, diags, l) {
        return;
    }
    let Some(invert) = number(&t[1], diags, l) else {
        return;
    };
    let Some(kind) = keyword(TYPES, &t[2], diags, l) else {
        return;
    };
    let mut n = 3; // next token to read
    let stage = match kind {
        0 => OutfallStage::Free,
        1 => OutfallStage::Normal,
        2 => {
            if !need(t, 4, diags, l) {
                return;
            }
            n = 4;
            let Some(v) = number(&t[3], diags, l) else {
                return;
            };
            OutfallStage::Fixed(v * cv.len)
        }
        3 => {
            if !need(t, 4, diags, l) {
                return;
            }
            n = 4;
            let Some(c) = resolve(s, ObjectKind::Curve, &t[3], diags, l) else {
                return;
            };
            OutfallStage::Tidal { curve: c }
        }
        _ => {
            if !need(t, 4, diags, l) {
                return;
            }
            n = 4;
            let Some(ts) = resolve(s, ObjectKind::TimeSeries, &t[3], diags, l) else {
                return;
            };
            OutfallStage::Series { series: ts }
        }
    };
    // Optional flap gate, then optional parcel routing — positional, per the
    // predecessor: gate at token n, routing at n+1.
    let mut flap_gate = false;
    if let Some(tok) = t.get(n) {
        let Some(v) = keyword(&["NO", "YES"], tok, diags, l) else {
            return;
        };
        flap_gate = v == 1;
    }
    let mut route_to_parcel = None;
    if let Some(tok) = t.get(n + 1) {
        let Some(p) = resolve(s, ObjectKind::Parcel, tok, diags, l) else {
            return;
        };
        route_to_parcel = Some(p);
    }
    net.vertices.push(Vertex {
        id: t[0].clone(),
        invert: invert * cv.len,
        kind: VertexKind::Outfall {
            stage,
            flap_gate,
            route_to_parcel,
        },
    });
}

fn parse_storage(
    net: &mut Network,
    s: &Survey,
    cv: &Converter,
    line: &TokenLine,
    diags: &mut Vec<Diagnostic>,
) {
    const RELATIONS: &[&str] = &[
        "TABULAR",
        "FUNCTIONAL",
        "CYLINDRICAL",
        "CONICAL",
        "PARABOLIC",
        "PYRAMIDAL",
    ];
    let t = &line.tokens;
    let l = line.line;
    if !need(t, 6, diags, l) {
        return;
    }
    let Some(invert) = number(&t[1], diags, l) else {
        return;
    };
    let Some(max_depth) = number(&t[2], diags, l) else {
        return;
    };
    let Some(init_depth) = number(&t[3], diags, l) else {
        return;
    };
    let Some(rel) = keyword(RELATIONS, &t[4], diags, l) else {
        return;
    };
    let (geometry, mut n) = match rel {
        0 => {
            let Some(c) = resolve(s, ObjectKind::Curve, &t[5], diags, l) else {
                return;
            };
            (StorageGeometry::Tabular { curve: c }, 6)
        }
        _ => {
            if !need(t, 8, diags, l) {
                return;
            }
            let mut y = [0.0; 3];
            for (i, yi) in y.iter_mut().enumerate() {
                let Some(v) = number(&t[5 + i], diags, l) else {
                    return;
                };
                *yi = v;
            }
            let bad = |i: usize, diags: &mut Vec<Diagnostic>| {
                diags.push(err(
                    l,
                    DiagnosticKind::BadValue {
                        token: t[5 + i].clone(),
                    },
                ));
            };
            let g = match rel {
                1 => {
                    // FUNCTIONAL A = c + a·y^b: unit-dependent (§14.6),
                    // stored as written.
                    if y[2] < 0.0 {
                        bad(2, diags);
                        return;
                    }
                    StorageGeometry::Functional {
                        coeff: y[0],
                        exponent: y[1],
                        constant: y[2],
                    }
                }
                _ => {
                    if y[0] <= 0.0 {
                        bad(0, diags);
                        return;
                    }
                    if y[1] <= 0.0 {
                        bad(1, diags);
                        return;
                    }
                    if y[2] < 0.0 || (rel == 4 && y[2] == 0.0) {
                        bad(2, diags);
                        return;
                    }
                    // Compile to A = a0 + a1·y + a2·y², from SI dimensions.
                    let (la, wb, z) = (y[0] * cv.len, y[1] * cv.len, y[2]);
                    let (a, b) = (la / 2.0, wb / 2.0);
                    use std::f64::consts::PI;
                    let (kind, a0, a1, a2) = match rel {
                        2 => (StorageShapeKind::Cylindrical, PI * a * b, 0.0, 0.0),
                        3 => (
                            StorageShapeKind::Conical,
                            PI * a * b,
                            2.0 * PI * b * z,
                            PI * b / a * z * z,
                        ),
                        4 => {
                            // PARABOLIC relation word: the elliptical
                            // paraboloid; y[2] is the top height (a length).
                            let zh = z * cv.len;
                            (StorageShapeKind::Paraboloid, 0.0, PI * a * b / zh, 0.0)
                        }
                        _ => (
                            StorageShapeKind::Pyramidal,
                            la * wb,
                            2.0 * (la + wb) * z,
                            4.0 * z * z,
                        ),
                    };
                    StorageGeometry::Shape { kind, a0, a1, a2 }
                }
            };
            (g, 8)
        }
    };
    let Some(surcharge) = opt_number(t, n, 0.0, diags, l) else {
        return;
    };
    n += 1;
    let Some(evap) = opt_number(t, n, 0.0, diags, l) else {
        return;
    };
    n += 1;
    let seepage = if t.len() > n {
        if !need(t, n + 3, diags, l) {
            return;
        }
        let Some(psi) = number(&t[n], diags, l) else {
            return;
        };
        let Some(ksat) = number(&t[n + 1], diags, l) else {
            return;
        };
        let Some(imd) = number(&t[n + 2], diags, l) else {
            return;
        };
        Some(StorageSeepage {
            suction: psi * cv.suction,
            conductivity: ksat * cv.conductivity,
            initial_deficit: imd,
        })
    } else {
        None
    };
    net.vertices.push(Vertex {
        id: t[0].clone(),
        invert: invert * cv.len,
        kind: VertexKind::Storage {
            max_depth: max_depth * cv.len,
            init_depth: init_depth * cv.len,
            geometry,
            surcharge_depth: surcharge * cv.len,
            evap_fraction: evap,
            seepage,
        },
    });
}

fn parse_divider(
    net: &mut Network,
    s: &Survey,
    cv: &Converter,
    line: &TokenLine,
    diags: &mut Vec<Diagnostic>,
) {
    const TYPES: &[&str] = &["CUTOFF", "TABULAR", "WEIR", "OVERFLOW"];
    let t = &line.tokens;
    let l = line.line;
    if !need(t, 4, diags, l) {
        return;
    }
    let Some(invert) = number(&t[1], diags, l) else {
        return;
    };
    let diverted_link = if t[2].is_empty() || t[2] == "*" {
        None
    } else {
        let Some(k) = resolve(s, ObjectKind::Link, &t[2], diags, l) else {
            return;
        };
        Some(k)
    };
    let Some(kind) = keyword(TYPES, &t[3], diags, l) else {
        return;
    };
    let (rule, n) = match kind {
        0 => {
            if !need(t, 5, diags, l) {
                return;
            }
            let Some(q) = number(&t[4], diags, l) else {
                return;
            };
            (
                DividerRule::Cutoff {
                    min_flow: q * cv.flow,
                },
                5,
            )
        }
        1 => {
            if !need(t, 5, diags, l) {
                return;
            }
            let Some(c) = resolve(s, ObjectKind::Curve, &t[4], diags, l) else {
                return;
            };
            (DividerRule::Tabular { curve: c }, 5)
        }
        2 => {
            if !need(t, 7, diags, l) {
                return;
            }
            let Some(q) = number(&t[4], diags, l) else {
                return;
            };
            let Some(d) = number(&t[5], diags, l) else {
                return;
            };
            let Some(c) = number(&t[6], diags, l) else {
                return;
            };
            (
                DividerRule::Weir {
                    min_flow: q * cv.flow,
                    max_depth: d * cv.len,
                    coeff: c,
                },
                7,
            )
        }
        _ => (DividerRule::Overflow, 4),
    };
    let mut x = [0.0; 4]; // max, init, surcharge, ponded
    for (i, xi) in x.iter_mut().enumerate() {
        let Some(v) = opt_number(t, n + i, 0.0, diags, l) else {
            return;
        };
        *xi = v;
    }
    net.vertices.push(Vertex {
        id: t[0].clone(),
        invert: invert * cv.len,
        kind: VertexKind::Divider {
            diverted_link,
            rule,
            max_depth: x[0] * cv.len,
            init_depth: x[1] * cv.len,
            surcharge_depth: x[2] * cv.len,
            ponded_area: x[3] * cv.len * cv.len,
        },
    });
}

// ── Links ────────────────────────────────────────────────────────────────

fn link_endpoints(
    s: &Survey,
    t: &[String],
    diags: &mut Vec<Diagnostic>,
    l: usize,
) -> Option<(usize, usize)> {
    let from = resolve(s, ObjectKind::Vertex, &t[1], diags, l)?;
    let to = resolve(s, ObjectKind::Vertex, &t[2], diags, l)?;
    Some((from, to))
}

fn parse_conduit(
    net: &mut Network,
    s: &Survey,
    cv: &Converter,
    line: &TokenLine,
    diags: &mut Vec<Diagnostic>,
) {
    let t = &line.tokens;
    let l = line.line;
    if !need(t, 7, diags, l) {
        return;
    }
    let Some((from, to)) = link_endpoints(s, t, diags, l) else {
        return;
    };
    let Some(length) = number(&t[3], diags, l) else {
        return;
    };
    let Some(roughness) = number(&t[4], diags, l) else {
        return;
    };
    let Some(offset1) = cv.offset(&t[5], diags, l) else {
        return;
    };
    let Some(offset2) = cv.offset(&t[6], diags, l) else {
        return;
    };
    let Some(init_flow) = opt_number(t, 7, 0.0, diags, l) else {
        return;
    };
    let Some(max_flow) = opt_number(t, 8, 0.0, diags, l) else {
        return;
    };
    net.links.push(Link {
        id: t[0].clone(),
        from,
        to,
        kind: LinkKind::Channel {
            length: length * cv.len,
            roughness,
            offset1,
            offset2,
            init_flow: init_flow * cv.flow,
            max_flow: max_flow * cv.flow,
        },
        cross_section: None,
    });
}

fn parse_pump(
    net: &mut Network,
    s: &Survey,
    cv: &Converter,
    line: &TokenLine,
    diags: &mut Vec<Diagnostic>,
) {
    let t = &line.tokens;
    let l = line.line;
    if !need(t, 3, diags, l) {
        return;
    }
    let Some((from, to)) = link_endpoints(s, t, diags, l) else {
        return;
    };
    let curve = match t.get(3) {
        None => None,
        Some(tok) if tok == "*" => None,
        Some(tok) => {
            let Some(c) = resolve(s, ObjectKind::Curve, tok, diags, l) else {
                return;
            };
            Some(c)
        }
    };
    let mut initial_on = true;
    if let Some(tok) = t.get(4) {
        let Some(v) = keyword(&["OFF", "ON"], tok, diags, l) else {
            return;
        };
        initial_on = v == 1;
    }
    let Some(startup) = opt_number(t, 5, 0.0, diags, l) else {
        return;
    };
    let Some(shutoff) = opt_number(t, 6, 0.0, diags, l) else {
        return;
    };
    if startup < 0.0 || shutoff < 0.0 {
        diags.push(err(
            l,
            DiagnosticKind::BadValue {
                token: "negative pump depth".into(),
            },
        ));
        return;
    }
    net.links.push(Link {
        id: t[0].clone(),
        from,
        to,
        kind: LinkKind::Pump {
            curve,
            initial_on,
            startup_depth: startup * cv.len,
            shutoff_depth: shutoff * cv.len,
        },
        cross_section: None,
    });
}

fn parse_orifice(
    net: &mut Network,
    s: &Survey,
    cv: &Converter,
    line: &TokenLine,
    diags: &mut Vec<Diagnostic>,
) {
    let t = &line.tokens;
    let l = line.line;
    if !need(t, 6, diags, l) {
        return;
    }
    let Some((from, to)) = link_endpoints(s, t, diags, l) else {
        return;
    };
    let Some(orient) = keyword(&["SIDE", "BOTTOM"], &t[3], diags, l) else {
        return;
    };
    let Some(offset) = cv.offset(&t[4], diags, l) else {
        return;
    };
    let Some(cd) = number(&t[5], diags, l) else {
        return;
    };
    if cd < 0.0 {
        diags.push(err(
            l,
            DiagnosticKind::BadValue {
                token: t[5].clone(),
            },
        ));
        return;
    }
    let mut flap_gate = false;
    if let Some(tok) = t.get(6) {
        let Some(v) = keyword(&["NO", "YES"], tok, diags, l) else {
            return;
        };
        flap_gate = v == 1;
    }
    let Some(orate) = opt_number(t, 7, 0.0, diags, l) else {
        return;
    };
    net.links.push(Link {
        id: t[0].clone(),
        from,
        to,
        kind: LinkKind::Orifice {
            orientation: if orient == 0 {
                OrificeOrientation::Side
            } else {
                OrificeOrientation::Bottom
            },
            offset,
            discharge_coeff: cd,
            flap_gate,
            open_close_time: orate * 3600.0, // hours in the file
        },
        cross_section: None,
    });
}

fn parse_weir(
    net: &mut Network,
    s: &Survey,
    cv: &Converter,
    line: &TokenLine,
    diags: &mut Vec<Diagnostic>,
) {
    const TYPES: &[&str] = &[
        "TRANSVERSE",
        "SIDEFLOW",
        "V-NOTCH",
        "TRAPEZOIDAL",
        "ROADWAY",
    ];
    let t = &line.tokens;
    let l = line.line;
    if !need(t, 6, diags, l) {
        return;
    }
    let Some((from, to)) = link_endpoints(s, t, diags, l) else {
        return;
    };
    let Some(form_i) = keyword(TYPES, &t[3], diags, l) else {
        return;
    };
    let form = [
        WeirForm::Transverse,
        WeirForm::SideFlow,
        WeirForm::VNotch,
        WeirForm::Trapezoidal,
        WeirForm::Roadway,
    ][form_i];
    let Some(offset) = cv.offset(&t[4], diags, l) else {
        return;
    };
    let Some(cd) = number(&t[5], diags, l) else {
        return;
    };
    // Optional tail, `*` skipping a slot, per the predecessor.
    let starred = |i: usize| t.get(i).is_some_and(|tok| tok.starts_with('*'));
    let mut flap_gate = false;
    if t.len() > 6 && !starred(6) {
        let Some(v) = keyword(&["NO", "YES"], &t[6], diags, l) else {
            return;
        };
        flap_gate = v == 1;
    }
    let mut end_contractions = 0.0;
    if t.len() > 7 && !starred(7) {
        let Some(v) = number(&t[7], diags, l) else {
            return;
        };
        end_contractions = v;
    }
    let mut end_coeff = 0.0;
    if t.len() > 8 && !starred(8) {
        let Some(v) = number(&t[8], diags, l) else {
            return;
        };
        end_coeff = v;
    }
    let mut can_surcharge = true;
    if t.len() > 9 && !starred(9) {
        let Some(v) = keyword(&["NO", "YES"], &t[9], diags, l) else {
            return;
        };
        can_surcharge = v == 1;
    }
    let mut road_width = 0.0;
    let mut road_surface = RoadSurface::Unspecified;
    if form == WeirForm::Roadway {
        if t.len() > 10 {
            let Some(v) = number(&t[10], diags, l) else {
                return;
            };
            road_width = v * cv.len;
        }
        if t.len() > 11 {
            road_surface = if t[11].eq_ignore_ascii_case("PAVED") {
                RoadSurface::Paved
            } else if t[11].eq_ignore_ascii_case("GRAVEL") {
                RoadSurface::Gravel
            } else {
                RoadSurface::Unspecified
            };
        }
    }
    let mut coeff_curve = None;
    if t.len() > 12 && !starred(12) {
        let Some(c) = resolve(s, ObjectKind::Curve, &t[12], diags, l) else {
            return;
        };
        coeff_curve = Some(c);
    }
    net.links.push(Link {
        id: t[0].clone(),
        from,
        to,
        kind: LinkKind::Weir {
            form,
            offset,
            discharge_coeff: cd,
            flap_gate,
            end_contractions,
            end_coeff,
            can_surcharge,
            road_width,
            road_surface,
            coeff_curve,
        },
        cross_section: None,
    });
}

fn parse_outlet(
    net: &mut Network,
    s: &Survey,
    cv: &Converter,
    line: &TokenLine,
    diags: &mut Vec<Diagnostic>,
) {
    let t = &line.tokens;
    let l = line.line;
    if !need(t, 6, diags, l) {
        return;
    }
    let Some((from, to)) = link_endpoints(s, t, diags, l) else {
        return;
    };
    let Some(offset) = cv.offset(&t[3], diags, l) else {
        return;
    };
    // Relation word carries an optional /DEPTH or /HEAD qualifier.
    let (rel_word, qualifier) = match t[4].split_once('/') {
        Some((r, q)) => (r, Some(q)),
        None => (t[4].as_str(), None),
    };
    let Some(rel) = keyword(&["TABULAR", "FUNCTIONAL"], rel_word, diags, l) else {
        return;
    };
    let head_basis = match qualifier {
        Some(q) if q.eq_ignore_ascii_case("HEAD") => OutletHeadBasis::Head,
        _ => OutletHeadBasis::Depth,
    };
    let (rating, n) = if rel == 1 {
        if !need(t, 7, diags, l) {
            return;
        }
        let Some(a) = number(&t[5], diags, l) else {
            return;
        };
        let Some(b) = number(&t[6], diags, l) else {
            return;
        };
        (
            OutletRating::Functional {
                coeff: a,
                exponent: b,
            },
            7,
        )
    } else {
        let Some(c) = resolve(s, ObjectKind::Curve, &t[5], diags, l) else {
            return;
        };
        (OutletRating::Tabular { curve: c }, 6)
    };
    let mut flap_gate = false;
    if let Some(tok) = t.get(n) {
        let Some(v) = keyword(&["NO", "YES"], tok, diags, l) else {
            return;
        };
        flap_gate = v == 1;
    }
    net.links.push(Link {
        id: t[0].clone(),
        from,
        to,
        kind: LinkKind::Outlet {
            offset,
            rating,
            head_basis,
            flap_gate,
        },
        cross_section: None,
    });
}

// ── Cross-sections ───────────────────────────────────────────────────────

const XSECT_WORDS: &[&str] = &[
    "DUMMY",
    "CIRCULAR",
    "FILLED_CIRCULAR",
    "RECT_CLOSED",
    "RECT_OPEN",
    "TRAPEZOIDAL",
    "TRIANGULAR",
    "PARABOLIC",
    "POWER",
    "RECT_TRIANGULAR",
    "RECT_ROUND",
    "MODBASKETHANDLE",
    "HORIZ_ELLIPSE",
    "VERT_ELLIPSE",
    "ARCH",
    "EGG",
    "HORSESHOE",
    "GOTHIC",
    "CATENARY",
    "SEMIELLIPTICAL",
    "BASKETHANDLE",
    "SEMICIRCULAR",
    "IRREGULAR",
    "CUSTOM",
    "FORCE_MAIN",
    "STREET",
];

const XSECT_SHAPES: &[XsectShape] = &[
    XsectShape::Dummy,
    XsectShape::Circular,
    XsectShape::FilledCircular,
    XsectShape::RectClosed,
    XsectShape::RectOpen,
    XsectShape::Trapezoidal,
    XsectShape::Triangular,
    XsectShape::Parabolic,
    XsectShape::Power,
    XsectShape::RectTriangular,
    XsectShape::RectRound,
    XsectShape::ModBasketHandle,
    XsectShape::HorizEllipse,
    XsectShape::VertEllipse,
    XsectShape::Arch,
    XsectShape::Egg,
    XsectShape::Horseshoe,
    XsectShape::Gothic,
    XsectShape::Catenary,
    XsectShape::SemiElliptical,
    XsectShape::BasketHandle,
    XsectShape::SemiCircular,
    XsectShape::Irregular,
    XsectShape::Custom,
    XsectShape::ForceMain,
    XsectShape::Street,
];

fn parse_xsection(net: &mut Network, s: &Survey, line: &TokenLine, diags: &mut Vec<Diagnostic>) {
    let t = &line.tokens;
    let l = line.line;
    if !need(t, 3, diags, l) {
        return;
    }
    let Some(link_i) = resolve(s, ObjectKind::Link, &t[0], diags, l) else {
        return;
    };
    let Some(shape_i) = keyword(XSECT_WORDS, &t[1], diags, l) else {
        return;
    };
    let shape = XSECT_SHAPES[shape_i];
    let xs = match shape {
        XsectShape::Irregular => {
            let Some(tr) = resolve(s, ObjectKind::Transect, &t[2], diags, l) else {
                return;
            };
            CrossSection {
                shape,
                geom_user: [0.0; 4],
                barrels: 1,
                culvert_code: 0,
                referent: Some(XsectReferent::Transect(tr)),
            }
        }
        XsectShape::Street => {
            let Some(st) = resolve(s, ObjectKind::Street, &t[2], diags, l) else {
                return;
            };
            CrossSection {
                shape,
                geom_user: [0.0; 4],
                barrels: 1,
                culvert_code: 0,
                referent: Some(XsectReferent::Street(st)),
            }
        }
        XsectShape::Custom => {
            if !need(t, 4, diags, l) {
                return;
            }
            let Some(y_full) = number(&t[2], diags, l) else {
                return;
            };
            if y_full <= 0.0 {
                diags.push(err(
                    l,
                    DiagnosticKind::BadValue {
                        token: t[2].clone(),
                    },
                ));
                return;
            }
            let Some(c) = resolve(s, ObjectKind::Curve, &t[3], diags, l) else {
                return;
            };
            CrossSection {
                shape,
                geom_user: [y_full, 0.0, 0.0, 0.0],
                barrels: barrels(t, 4, diags, l).unwrap_or(1),
                culvert_code: 0,
                referent: Some(XsectReferent::Curve(c)),
            }
        }
        _ => {
            if !need(t, 6, diags, l) {
                return;
            }
            let mut geom = [0.0; 4];
            for (i, g) in geom.iter_mut().enumerate() {
                let Some(v) = number(&t[2 + i], diags, l) else {
                    return;
                };
                *g = v;
            }
            let b = barrels(t, 6, diags, l).unwrap_or(1);
            let culvert = match t.get(7) {
                Some(tok) => match tok.parse::<u32>() {
                    Ok(v) => v,
                    Err(_) => {
                        diags.push(err(l, DiagnosticKind::BadValue { token: tok.clone() }));
                        return;
                    }
                },
                None => 0,
            };
            CrossSection {
                shape,
                geom_user: geom,
                barrels: b,
                culvert_code: culvert,
                referent: None,
            }
        }
    };
    net.links[link_i].cross_section = Some(xs);
}

fn barrels(t: &[String], i: usize, diags: &mut Vec<Diagnostic>, l: usize) -> Option<u32> {
    match t.get(i) {
        None => Some(1),
        Some(tok) => match tok.parse::<f64>() {
            Ok(v) if v >= 1.0 => Some(v as u32),
            _ => {
                diags.push(err(l, DiagnosticKind::BadValue { token: tok.clone() }));
                None
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  100.0  3.0  0.5  0  120

[OUTFALLS]
O1  95.0  FIXED  96.5  YES

[STORAGE]
S1  90.0  10.0  1.0  FUNCTIONAL  20  1.5  100
S2  90.0  10.0  0.0  PYRAMIDAL   10  8  2  0  0.5

[DIVIDERS]
D1  98.0  C1  OVERFLOW  4.0

[CONDUITS]
C1  J1  O1  400  0.015  0.5  0  2.5  10

[PUMPS]
P1  S1  J1  *  OFF  1.2  0.3

[ORIFICES]
OR1  S1  O1  BOTTOM  0.25  0.65  YES  0.5

[WEIRS]
W1  J1  O1  V-NOTCH  1.0  3.33

[OUTLETS]
OU1  S2  O1  0.1  FUNCTIONAL/HEAD  2.5  1.5  YES

[XSECTIONS]
C1  CIRCULAR  4.0  0  0  0  2
";

    fn parse(input: &str) -> (Network, Vec<Diagnostic>) {
        parse_network(input)
    }

    fn errors(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
        diags.iter().filter(|d| d.kind.is_error()).collect()
    }

    #[test]
    fn the_fixture_parses_clean() {
        let (_, diags) = parse(FIXTURE);
        assert!(errors(&diags).is_empty(), "{:?}", errors(&diags));
    }

    #[test]
    fn junction_values_convert_to_si() {
        let (net, _) = parse(FIXTURE);
        let v = &net.vertices[0];
        assert_eq!(v.id, "J1");
        assert!((v.invert - 100.0 * 0.3048).abs() < 1e-12);
        let VertexKind::Junction {
            max_depth,
            init_depth,
            ponded_area,
            ..
        } = v.kind
        else {
            panic!("J1 is a junction")
        };
        assert!((max_depth - 3.0 * 0.3048).abs() < 1e-12);
        assert!((init_depth - 0.5 * 0.3048).abs() < 1e-12);
        assert!((ponded_area - 120.0 * 0.3048 * 0.3048).abs() < 1e-12);
    }

    #[test]
    fn outfall_fixed_stage_and_flags() {
        let (net, _) = parse(FIXTURE);
        let VertexKind::Outfall {
            ref stage,
            flap_gate,
            ..
        } = net.vertices[1].kind
        else {
            panic!()
        };
        assert!(flap_gate);
        let OutfallStage::Fixed(h) = stage else {
            panic!()
        };
        assert!((h - 96.5 * 0.3048).abs() < 1e-12);
    }

    #[test]
    fn functional_storage_keeps_user_units_per_14_6() {
        let (net, _) = parse(FIXTURE);
        let VertexKind::Storage { ref geometry, .. } = net.vertices[2].kind else {
            panic!()
        };
        assert_eq!(
            *geometry,
            StorageGeometry::Functional {
                coeff: 20.0,
                exponent: 1.5,
                constant: 100.0
            }
        );
    }

    #[test]
    fn pyramidal_storage_compiles_to_the_quadratic_in_si() {
        let (net, _) = parse(FIXTURE);
        let VertexKind::Storage { ref geometry, .. } = net.vertices[3].kind else {
            panic!()
        };
        let StorageGeometry::Shape { kind, a0, a1, a2 } = *geometry else {
            panic!()
        };
        assert_eq!(kind, StorageShapeKind::Pyramidal);
        let (l, w, z) = (10.0 * 0.3048, 8.0 * 0.3048, 2.0);
        assert!((a0 - l * w).abs() < 1e-12);
        assert!((a1 - 2.0 * (l + w) * z).abs() < 1e-12);
        assert!((a2 - 4.0 * z * z).abs() < 1e-12);
    }

    #[test]
    fn conduit_flows_convert_and_endpoints_resolve() {
        let (net, _) = parse(FIXTURE);
        let c1 = net.links.iter().find(|l| l.id == "C1").unwrap();
        assert_eq!(net.vertices[c1.from].id, "J1");
        assert_eq!(net.vertices[c1.to].id, "O1");
        let LinkKind::Channel {
            length,
            init_flow,
            max_flow,
            offset1,
            ..
        } = c1.kind
        else {
            panic!()
        };
        assert!((length - 400.0 * 0.3048).abs() < 1e-12);
        assert!((init_flow - 2.5 * 0.028316846592).abs() < 1e-12);
        assert!((max_flow - 10.0 * 0.028316846592).abs() < 1e-12);
        let Offset::Depth(o1) = offset1 else { panic!() };
        assert!((o1 - 0.5 * 0.3048).abs() < 1e-12);
    }

    #[test]
    fn a_star_pump_is_the_ideal_transfer_pump() {
        let (net, _) = parse(FIXTURE);
        let p = net.links.iter().find(|l| l.id == "P1").unwrap();
        let LinkKind::Pump {
            curve,
            initial_on,
            startup_depth,
            ..
        } = p.kind
        else {
            panic!()
        };
        assert_eq!(curve, None);
        assert!(!initial_on);
        assert!((startup_depth - 1.2 * 0.3048).abs() < 1e-12);
    }

    #[test]
    fn the_divider_references_its_link_and_keeps_its_rule() {
        let (net, _) = parse(FIXTURE);
        let VertexKind::Divider {
            diverted_link,
            ref rule,
            max_depth,
            ..
        } = net.vertices.iter().find(|v| v.id == "D1").unwrap().kind
        else {
            panic!()
        };
        assert_eq!(net.links[diverted_link.unwrap()].id, "C1");
        assert_eq!(*rule, DividerRule::Overflow);
        assert!((max_depth - 4.0 * 0.3048).abs() < 1e-12);
    }

    #[test]
    fn the_outlet_head_qualifier_parses() {
        let (net, _) = parse(FIXTURE);
        let o = net.links.iter().find(|l| l.id == "OU1").unwrap();
        let LinkKind::Outlet {
            head_basis,
            ref rating,
            flap_gate,
            ..
        } = o.kind
        else {
            panic!()
        };
        assert_eq!(head_basis, OutletHeadBasis::Head);
        assert!(flap_gate);
        assert_eq!(
            *rating,
            OutletRating::Functional {
                coeff: 2.5,
                exponent: 1.5
            }
        );
    }

    #[test]
    fn cross_sections_attach_with_barrels() {
        let (net, _) = parse(FIXTURE);
        let c1 = net.links.iter().find(|l| l.id == "C1").unwrap();
        let xs = c1.cross_section.as_ref().unwrap();
        assert_eq!(xs.shape, XsectShape::Circular);
        assert_eq!(xs.geom_user[0], 4.0, "geometry stays in file units (§5)");
        assert_eq!(xs.barrels, 2);
    }

    #[test]
    fn si_files_convert_nothing_lengthwise() {
        let (net, diags) = parse("[OPTIONS]\nFLOW_UNITS LPS\n[JUNCTIONS]\nJ1 12.5 2.0\n");
        assert!(errors(&diags).is_empty());
        assert_eq!(net.vertices[0].invert, 12.5);
    }

    #[test]
    fn an_unresolved_endpoint_is_an_error() {
        let (_, diags) = parse("[CONDUITS]\nC1 A B 100 0.013 0 0\n");
        assert!(diags.iter().any(|d| matches!(
            &d.kind,
            DiagnosticKind::UnresolvedReference { id } if id == "A"
        )));
    }

    #[test]
    fn a_failed_line_still_occupies_its_slot() {
        // J2's line is malformed; J3 must still land at index 2 so link
        // endpoint indices stay aligned with the registry.
        let (net, diags) = parse(
            "[JUNCTIONS]\nJ1 100 3\nJ2 oops 3\nJ3 99 3\n[CONDUITS]\nC1 J1 J3 100 0.013 0 0\n",
        );
        assert!(!errors(&diags).is_empty());
        assert_eq!(net.vertices.len(), 3);
        assert_eq!(net.vertices[2].id, "J3");
        let c1 = &net.links[0];
        assert_eq!(net.vertices[c1.to].id, "J3");
    }
}
