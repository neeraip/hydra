//! Quality and inflow parsing (§2.8, §8.1): constituents, land uses,
//! accumulation and mobilisation relations, land cover, initial loadings,
//! external inflows, and sanitary inflows.
//!
//! Quality masses and concentrations stay in the file's units — the §8
//! relations own their interpretation — while decay converts from per-day
//! to per-second and flow-valued inflows convert to m³/s.

use super::keywords::match_keyword;
use super::objects::UnitConverter;
use super::survey::{Diagnostic, DiagnosticKind, ObjectKind, Survey, TokenLine};
use crate::io::lex::FiniteParse;
use crate::model::{
    Buildup, BuildupForm, BuildupNormalizer, ConcentrationUnits, Constituent, DryWeatherInflow,
    ExternalInflow, InflowKind, LandUse, Network, Washoff, WashoffForm,
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

/// The predecessor tests the FLOW sentinel with its prefix matcher, but a
/// pollutant lookup runs FIRST — so a pollutant named FLOWX wins over the
/// sentinel, and only a non-pollutant token falls through to the prefix
/// test. Mirrored here by call order; this helper is the fallthrough.
fn is_flow_sentinel(token: &str) -> bool {
    match_keyword(&["FLOW"], token).is_some()
}

/// Parse `[POLLUTANTS]`.
pub(crate) fn parse_constituents(
    lines: &[TokenLine<'_>],
    s: &Survey,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Constituent> {
    const UNITS: &[&str] = &["MG/L", "UG/L", "#/L"];
    let mut out = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 6 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(u) = match_keyword(UNITS, t[1]) else {
            diags.push(bad(l, t[1]));
            continue;
        };
        let mut x = [0.0; 4]; // c_rain, c_gw, c_rdii, decay
        let mut ok = true;
        for (i, xi) in x.iter_mut().enumerate() {
            let Ok(v) = t[2 + i].finite_f64() else {
                diags.push(bad(l, t[2 + i]));
                ok = false;
                break;
            };
            // The three concentrations are non-negative; decay may be
            // negative (growth).
            if i < 3 && v < 0.0 {
                diags.push(bad(l, t[2 + i]));
                ok = false;
                break;
            }
            *xi = v;
        }
        if !ok {
            continue;
        }
        let mut snow_only = false;
        if let Some(tok) = t.get(6) {
            let Some(v) = match_keyword(&["NO", "YES"], tok) else {
                diags.push(bad(l, tok));
                continue;
            };
            snow_only = v == 1;
        }
        let mut co_constituent = None;
        let mut co_fraction = 0.0;
        if t.len() >= 9 && t[7] != "*" {
            let Some(&co) = s.resolve(ObjectKind::Constituent, t[7]) else {
                diags.push(unresolved(l, t[7]));
                continue;
            };
            let Ok(f) = t[8].finite_f64() else {
                diags.push(bad(l, t[8]));
                continue;
            };
            if f < 0.0 {
                diags.push(bad(l, t[8]));
                continue;
            }
            co_constituent = Some(co);
            co_fraction = f;
        }
        let mut tail = [0.0; 2]; // c_dwf, c_init
        for (i, ti) in tail.iter_mut().enumerate() {
            if let Some(tok) = t.get(9 + i) {
                let Ok(v) = tok.finite_f64() else {
                    diags.push(bad(l, tok));
                    ok = false;
                    break;
                };
                if v < 0.0 {
                    diags.push(bad(l, tok));
                    ok = false;
                    break;
                }
                *ti = v;
            }
        }
        if !ok {
            continue;
        }
        out.push(Constituent {
            id: t[0].to_string(),
            units: [
                ConcentrationUnits::MgPerL,
                ConcentrationUnits::UgPerL,
                ConcentrationUnits::CountPerL,
            ][u],
            c_rain: x[0],
            c_groundwater: x[1],
            c_rdii: x[2],
            decay: x[3] / 86_400.0,
            snow_only,
            co_constituent,
            co_fraction,
            c_dwf: tail[0],
            c_init: tail[1],
        });
    }
    out
}

/// Parse `[LANDUSES]`, sized for the constituent count.
pub(crate) fn parse_land_uses(
    lines: &[TokenLine<'_>],
    n_constituents: usize,
    diags: &mut Vec<Diagnostic>,
) -> Vec<LandUse> {
    let mut out = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        let mut sweep = [0.0; 3];
        if t.len() > 1 {
            if t.len() < 4 {
                diags.push(err(l, DiagnosticKind::MissingItems));
                continue;
            }
            let mut ok = true;
            for (i, si) in sweep.iter_mut().enumerate() {
                let Ok(v) = t[1 + i].finite_f64() else {
                    diags.push(bad(l, t[1 + i]));
                    ok = false;
                    break;
                };
                *si = v;
            }
            if !ok {
                continue;
            }
            if !(0.0..=1.0).contains(&sweep[1]) {
                diags.push(bad(l, t[2]));
                continue;
            }
        }
        out.push(LandUse {
            id: t[0].to_string(),
            sweep_interval: sweep[0],
            sweep_removal: sweep[1],
            sweep_days_since: sweep[2],
            buildup: vec![None; n_constituents],
            washoff: vec![None; n_constituents],
        });
    }
    out
}

/// Fill `[BUILDUP]` relations into their land uses.
pub(crate) fn parse_buildup(
    lines: &[TokenLine<'_>],
    s: &Survey,
    land_uses: &mut [LandUse],
    diags: &mut Vec<Diagnostic>,
) {
    const FORMS: &[&str] = &["NONE", "POW", "EXP", "SAT", "EXT"];
    const NORMALIZERS: &[&str] = &["AREA", "CURB"];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            continue; // the predecessor ignores short lines here
        }
        let Some(&lu) = s.resolve(ObjectKind::LandUse, t[0]) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        let Some(&p) = s.resolve(ObjectKind::Constituent, t[1]) else {
            diags.push(unresolved(l, t[1]));
            continue;
        };
        let Some(form_i) = match_keyword(FORMS, t[2]) else {
            diags.push(bad(l, t[2]));
            continue;
        };
        let form = [
            BuildupForm::None,
            BuildupForm::Power,
            BuildupForm::Exponential,
            BuildupForm::Saturation,
            BuildupForm::External,
        ][form_i];
        let mut coeffs = [0.0; 3];
        let mut normalizer = BuildupNormalizer::PerArea;
        let mut series = None;
        if form != BuildupForm::None {
            if t.len() < 7 {
                diags.push(err(l, DiagnosticKind::MissingItems));
                continue;
            }
            let mut ok = true;
            if form == BuildupForm::External {
                // Maximum, scale, and the loading series (§8.2).
                for (i, ci) in coeffs.iter_mut().take(2).enumerate() {
                    match t[3 + i].finite_f64() {
                        Ok(v) if v >= 0.0 => *ci = v,
                        _ => {
                            diags.push(bad(l, t[3 + i]));
                            ok = false;
                            break;
                        }
                    }
                }
                if ok {
                    match s.resolve(ObjectKind::TimeSeries, t[5]) {
                        Some(&ts) => series = Some(ts),
                        None => {
                            diags.push(unresolved(l, t[5]));
                            ok = false;
                        }
                    }
                }
            } else {
                for (i, ci) in coeffs.iter_mut().enumerate() {
                    let Ok(v) = t[3 + i].finite_f64() else {
                        diags.push(bad(l, t[3 + i]));
                        ok = false;
                        break;
                    };
                    if v < 0.0 {
                        diags.push(bad(l, t[3 + i]));
                        ok = false;
                        break;
                    }
                    *ci = v;
                }
            }
            if !ok {
                continue;
            }
            let Some(n) = match_keyword(NORMALIZERS, t[6]) else {
                diags.push(bad(l, t[6]));
                continue;
            };
            normalizer = [BuildupNormalizer::PerArea, BuildupNormalizer::PerCurb][n];
            // The predecessor's power-exponent range check: {0} ∪ [0.01, 10].
            if form == BuildupForm::Power && coeffs[2] > 0.0 && !(0.01..=10.0).contains(&coeffs[2])
            {
                diags.push(bad(l, t[5]));
                continue;
            }
        }
        if let Some(land_use) = land_uses.get_mut(lu) {
            if let Some(slot) = land_use.buildup.get_mut(p) {
                *slot = Some(Buildup {
                    form,
                    coeffs,
                    normalizer,
                    series,
                });
            }
        }
    }
}

/// Fill `[WASHOFF]` relations into their land uses.
pub(crate) fn parse_washoff(
    lines: &[TokenLine<'_>],
    s: &Survey,
    land_uses: &mut [LandUse],
    diags: &mut Vec<Diagnostic>,
) {
    const FORMS: &[&str] = &["NONE", "EXP", "RC", "EMC"];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            continue;
        }
        let Some(&lu) = s.resolve(ObjectKind::LandUse, t[0]) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        let Some(&p) = s.resolve(ObjectKind::Constituent, t[1]) else {
            diags.push(unresolved(l, t[1]));
            continue;
        };
        let Some(form_i) = match_keyword(FORMS, t[2]) else {
            diags.push(bad(l, t[2]));
            continue;
        };
        let form = [
            WashoffForm::None,
            WashoffForm::Exponential,
            WashoffForm::RatingCurve,
            WashoffForm::Emc,
        ][form_i];
        let mut x = [0.0; 4];
        if form != WashoffForm::None {
            if t.len() < 5 {
                diags.push(err(l, DiagnosticKind::MissingItems));
                continue;
            }
            let mut ok = true;
            for (i, xi) in x.iter_mut().enumerate() {
                let Some(tok) = t.get(3 + i) else { break };
                let Ok(v) = tok.finite_f64() else {
                    diags.push(bad(l, tok));
                    ok = false;
                    break;
                };
                *xi = v;
            }
            if !ok {
                continue;
            }
        }
        if let Some(land_use) = land_uses.get_mut(lu) {
            if let Some(slot) = land_use.washoff.get_mut(p) {
                *slot = Some(Washoff {
                    form,
                    coeff: x[0],
                    exponent: x[1],
                    sweep_efficiency: x[2],
                    bmp_efficiency: x[3],
                });
            }
        }
    }
}

/// Fill `[COVERAGES]` (land-use fractions) into parcels: pairs of
/// (land use, percent) after the parcel identifier.
pub(crate) fn parse_coverages(
    lines: &[TokenLine<'_>],
    s: &Survey,
    net: &mut Network,
    diags: &mut Vec<Diagnostic>,
) {
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&pc) = s.resolve(ObjectKind::Parcel, t[0]) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        let mut k = 1;
        while k + 1 < t.len() + 1 {
            let Some(name) = t.get(k) else { break };
            let Some(&lu) = s.resolve(ObjectKind::LandUse, name) else {
                diags.push(unresolved(l, name));
                break;
            };
            let Some(vtok) = t.get(k + 1) else {
                diags.push(err(l, DiagnosticKind::MissingItems));
                break;
            };
            let Ok(f) = vtok.finite_f64() else {
                diags.push(bad(l, vtok));
                break;
            };
            if let Some(p) = net.parcels.get_mut(pc) {
                // §14.5: last coverage line for the land use wins.
                match p.land_cover.iter_mut().find(|(u, _)| *u == lu) {
                    Some(e) => {
                        diags.push(Diagnostic {
                            line: l,
                            kind: DiagnosticKind::OverriddenDefinition {
                                what: "land-cover",
                                id: t[0].to_string(),
                            },
                        });
                        e.1 = f / 100.0;
                    }
                    None => p.land_cover.push((lu, f / 100.0)),
                }
            }
            k += 2;
        }
    }
}

/// Fill `[LOADINGS]` (initial buildup) into parcels: pairs of
/// (constituent, areal load) after the parcel identifier.
pub(crate) fn parse_loadings(
    lines: &[TokenLine<'_>],
    s: &Survey,
    net: &mut Network,
    diags: &mut Vec<Diagnostic>,
) {
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&pc) = s.resolve(ObjectKind::Parcel, t[0]) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        let mut k = 1;
        while k < t.len() {
            let Some(&p) = s.resolve(ObjectKind::Constituent, t[k]) else {
                diags.push(unresolved(l, t[k]));
                break;
            };
            let Some(vtok) = t.get(k + 1) else {
                diags.push(err(l, DiagnosticKind::MissingItems));
                break;
            };
            let Ok(x) = vtok.finite_f64() else {
                diags.push(bad(l, vtok));
                break;
            };
            if let Some(parcel) = net.parcels.get_mut(pc) {
                // §14.5: last loading line for the constituent wins.
                match parcel.init_buildup.iter_mut().find(|(c, _)| *c == p) {
                    Some(e) => {
                        diags.push(Diagnostic {
                            line: l,
                            kind: DiagnosticKind::OverriddenDefinition {
                                what: "initial-loading",
                                id: t[0].to_string(),
                            },
                        });
                        e.1 = x;
                    }
                    None => parcel.init_buildup.push((p, x)),
                }
            }
            k += 2;
        }
    }
}

/// Parse `[INFLOWS]` (direct external inflows).
pub(crate) fn parse_inflows(
    lines: &[TokenLine<'_>],
    s: &Survey,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<ExternalInflow> {
    let mut out = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&vertex) = s.resolve(ObjectKind::Vertex, t[0]) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        // Constituent lookup first, FLOW-sentinel fallthrough — the
        // predecessor's order, so a constituent named FLOW wins; the
        // sentinel itself is prefix-matched (§14.3).
        let constituent = match s.resolve(ObjectKind::Constituent, t[1]) {
            Some(&p) => Some(p),
            None if is_flow_sentinel(t[1]) => None,
            None => {
                diags.push(unresolved(l, t[1]));
                continue;
            }
        };
        let series = if t[2].is_empty() || t[2] == "\"\"" {
            None
        } else {
            let Some(&ts) = s.resolve(ObjectKind::TimeSeries, t[2]) else {
                diags.push(unresolved(l, t[2]));
                continue;
            };
            Some(ts)
        };
        let mut kind = if constituent.is_none() {
            InflowKind::Flow
        } else {
            InflowKind::Concentration
        };
        let mut units_factor = 1.0;
        if constituent.is_some() {
            if let Some(tok) = t.get(3) {
                // §14.3: the predecessor's keyword comparison is a prefix
                // match — CONCENTRATION matches CONCEN and is accepted.
                const TYPES: &[&str] = &["CONCEN", "MASS"];
                let Some(m) = match_keyword(TYPES, tok) else {
                    diags.push(bad(l, tok));
                    continue;
                };
                if !tok.eq_ignore_ascii_case(TYPES[m]) {
                    diags.push(err(
                        l,
                        DiagnosticKind::PrefixMatched {
                            token: tok.to_string(),
                            matched: TYPES[m],
                        },
                    ));
                }
                kind = [InflowKind::Concentration, InflowKind::Mass][m];
            }
            if kind == InflowKind::Mass {
                if let Some(tok) = t.get(4) {
                    let Ok(v) = tok.finite_f64() else {
                        diags.push(bad(l, tok));
                        continue;
                    };
                    if v <= 0.0 {
                        diags.push(bad(l, tok));
                        continue;
                    }
                    units_factor = v;
                }
            }
        }
        let mut scale = 1.0;
        if let Some(tok) = t.get(5) {
            let Ok(v) = tok.finite_f64() else {
                diags.push(bad(l, tok));
                continue;
            };
            scale = v;
        }
        let mut baseline = 0.0;
        if let Some(tok) = t.get(6) {
            let Ok(v) = tok.finite_f64() else {
                diags.push(bad(l, tok));
                continue;
            };
            baseline = v;
        }
        if kind == InflowKind::Flow {
            // The baseline and the series scale both carry the flow
            // conversion, so series values (stored raw) land in SI.
            baseline *= cv.flow;
            scale *= cv.flow;
        }
        let base_pattern = match t.get(7) {
            Some(tok) => {
                let Some(&pat) = s.resolve(ObjectKind::TimePattern, tok) else {
                    diags.push(unresolved(l, tok));
                    continue;
                };
                Some(pat)
            }
            None => None,
        };
        let entry = ExternalInflow {
            vertex,
            constituent,
            series,
            kind,
            units_factor,
            scale,
            baseline,
            base_pattern,
        };
        // §14.5: a later line for the same vertex and constituent slot
        // replaces the earlier one, reported.
        match out
            .iter_mut()
            .find(|e: &&mut ExternalInflow| e.vertex == vertex && e.constituent == constituent)
        {
            Some(e) => {
                diags.push(Diagnostic {
                    line: l,
                    kind: DiagnosticKind::OverriddenDefinition {
                        what: "external-inflow",
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

/// Parse `[DWF]` (sanitary inflows): an average and up to four pattern
/// slots, an empty token skipping a slot.
pub(crate) fn parse_dry_weather(
    lines: &[TokenLine<'_>],
    s: &Survey,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<DryWeatherInflow> {
    let mut out = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&vertex) = s.resolve(ObjectKind::Vertex, t[0]) else {
            diags.push(unresolved(l, t[0]));
            continue;
        };
        // Constituent lookup first, FLOW-sentinel fallthrough — the
        // predecessor's order, so a constituent named FLOW wins; the
        // sentinel itself is prefix-matched (§14.3).
        let constituent = match s.resolve(ObjectKind::Constituent, t[1]) {
            Some(&p) => Some(p),
            None if is_flow_sentinel(t[1]) => None,
            None => {
                diags.push(unresolved(l, t[1]));
                continue;
            }
        };
        let Ok(mut average) = t[2].finite_f64() else {
            diags.push(bad(l, t[2]));
            continue;
        };
        if constituent.is_none() {
            average *= cv.flow;
        }
        let mut patterns = [None; 4];
        let mut ok = true;
        for (i, slot) in patterns.iter_mut().enumerate() {
            let Some(tok) = t.get(3 + i) else { break };
            if tok.is_empty() {
                continue;
            }
            let Some(&pat) = s.resolve(ObjectKind::TimePattern, tok) else {
                diags.push(unresolved(l, tok));
                ok = false;
                break;
            };
            *slot = Some(pat);
        }
        if !ok {
            continue;
        }
        let entry = DryWeatherInflow {
            vertex,
            constituent,
            average,
            patterns,
        };
        // §14.5: last definition for the vertex/constituent slot wins.
        match out
            .iter_mut()
            .find(|e: &&mut DryWeatherInflow| e.vertex == vertex && e.constituent == constituent)
        {
            Some(e) => {
                diags.push(Diagnostic {
                    line: l,
                    kind: DiagnosticKind::OverriddenDefinition {
                        what: "sanitary-inflow",
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
    use crate::model::{
        BuildupForm, BuildupNormalizer, ConcentrationUnits, InflowKind, WashoffForm,
    };

    const FIXTURE: &str = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  100  3

[RAINGAGES]
G1  INTENSITY  1.0  1.0  TIMESERIES  TS1

[SUBCATCHMENTS]
S1  G1  J1  10  25  500  0.5  0

[POLLUTANTS]
TSS   MG/L  10  0  0  0.1
LEAD  UG/L  0   0  0  0    NO  TSS  0.25

[LANDUSES]
RES  7  0.7  3
COM

[BUILDUP]
RES  TSS  POW  50  2  0.5  AREA

[WASHOFF]
RES  TSS  EXP  0.1  1.2  0  0

[COVERAGES]
S1  RES  60  COM  40

[LOADINGS]
S1  TSS  1.5

[INFLOWS]
J1  FLOW  TS1
J1  TSS   TS1  MASS  2.5  1.0  0.0

[DWF]
J1  FLOW  0.004  DWFPAT

[PATTERNS]
DWFPAT  HOURLY  1 1 1 1 1 1

[TIMESERIES]
TS1  0  1.0  1  2.0
";

    #[test]
    fn constituents_parse_with_co_pollutants_and_decay_conversion() {
        let (net, diags) = parse_network(FIXTURE);
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "{:?}",
            diags
                .iter()
                .filter(|d| d.kind.is_error())
                .collect::<Vec<_>>()
        );
        assert_eq!(net.constituents.len(), 2);
        let tss = &net.constituents[0];
        assert_eq!(tss.units, ConcentrationUnits::MgPerL);
        assert_eq!(tss.c_rain, 10.0);
        assert!((tss.decay - 0.1 / 86_400.0).abs() < 1e-18);
        let lead = &net.constituents[1];
        assert_eq!(lead.units, ConcentrationUnits::UgPerL);
        assert_eq!(lead.co_constituent, Some(0));
        assert_eq!(lead.co_fraction, 0.25);
    }

    #[test]
    fn land_uses_own_their_relations_per_constituent() {
        let (net, _) = parse_network(FIXTURE);
        let res = &net.land_uses[0];
        assert_eq!(res.sweep_interval, 7.0);
        let b = res.buildup[0].as_ref().unwrap();
        assert_eq!(b.form, BuildupForm::Power);
        assert_eq!(b.coeffs, [50.0, 2.0, 0.5]);
        assert_eq!(b.normalizer, BuildupNormalizer::PerArea);
        let w = res.washoff[0].as_ref().unwrap();
        assert_eq!(w.form, WashoffForm::Exponential);
        assert_eq!(w.exponent, 1.2);
        // LEAD has no relations at RES; COM has none at all.
        assert!(res.buildup[1].is_none());
        let com = &net.land_uses[1];
        assert!(com.buildup.iter().all(Option::is_none));
        assert_eq!(com.sweep_interval, 0.0, "bare land-use line");
    }

    #[test]
    fn coverages_and_loadings_fill_their_parcel() {
        let (net, _) = parse_network(FIXTURE);
        let p = &net.parcels[0];
        assert_eq!(p.land_cover, vec![(0, 0.6), (1, 0.4)]);
        assert_eq!(p.init_buildup, vec![(0, 1.5)]);
    }

    #[test]
    fn inflows_convert_flow_baselines_and_carry_mass_factors() {
        let (net, _) = parse_network(FIXTURE);
        assert_eq!(net.inflows.len(), 2);
        let flow = &net.inflows[0];
        assert_eq!(flow.constituent, None);
        assert_eq!(flow.kind, InflowKind::Flow);
        assert!(flow.series.is_some());
        let mass = &net.inflows[1];
        assert_eq!(mass.kind, InflowKind::Mass);
        assert_eq!(mass.units_factor, 2.5);
    }

    #[test]
    fn dwf_converts_flow_and_fills_pattern_slots_positionally() {
        let (net, _) = parse_network(FIXTURE);
        let d = &net.dry_weather[0];
        assert_eq!(d.constituent, None);
        assert!((d.average - 0.004 * 0.028316846592).abs() < 1e-15);
        assert_eq!(d.patterns[0], Some(0), "first slot filled positionally");
        assert_eq!(d.patterns[1], None);
    }
}
