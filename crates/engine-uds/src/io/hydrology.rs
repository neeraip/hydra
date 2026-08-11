//! Surface-compartment parsing (§3): gages, parcels, sub-areas, and
//! infiltration, grammar for grammar from the predecessor's readers.
//!
//! A parcel assembles from three sections: `[SUBCATCHMENTS]` declares it,
//! `[SUBAREAS]` and `[INFILTRATION]` fill it in by reference. Areas convert
//! from acres or hectares, depths from inches or millimetres, rates from
//! in/hr or mm/hr — the surface compartment's own units, distinct from the
//! conveyance lengths.

use std::collections::HashMap;

use super::keywords::match_keyword;
use super::objects::UnitConverter;
use super::options::{clock_or_hours_to_seconds, InfiltrationModel};
use super::survey::{Diagnostic, DiagnosticKind, ObjectKind, Survey, TokenLine};
use crate::io::lex::FiniteParse;
use crate::model::{
    Gage, GageSource, Infiltration, Parcel, ParcelOutlet, RainForm, SubareaRouting, Subareas,
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

fn number(t: &[String], i: usize, diags: &mut Vec<Diagnostic>, l: usize) -> Option<f64> {
    match t[i].finite_f64() {
        Ok(v) => Some(v),
        Err(_) => {
            diags.push(bad(l, &t[i]));
            None
        }
    }
}

/// Parse `[RAINGAGES]`.
pub(crate) fn parse_gages(
    lines: &[TokenLine],
    s: &Survey,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Gage> {
    const FORMS: &[&str] = &["INTENSITY", "VOLUME", "CUMULATIVE"];
    let mut gages = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 6 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(form_i) = match_keyword(FORMS, &t[1]) else {
            diags.push(bad(l, &t[1]));
            continue;
        };
        let form = [RainForm::Intensity, RainForm::Volume, RainForm::Cumulative][form_i];
        // Interval: decimal hours or a clock string, rounded to seconds.
        let Some(interval) = clock_or_hours_to_seconds(&t[2]) else {
            diags.push(bad(l, &t[2]));
            continue;
        };
        if interval <= 0.0 {
            diags.push(bad(l, &t[2]));
            continue;
        }
        let Ok(scf) = t[3].finite_f64() else {
            diags.push(bad(l, &t[3]));
            continue;
        };
        let source = if t[4].eq_ignore_ascii_case("TIMESERIES") {
            let Some(map) = s.ids.get(&ObjectKind::TimeSeries) else {
                diags.push(err(
                    l,
                    DiagnosticKind::UnresolvedReference { id: t[5].clone() },
                ));
                continue;
            };
            let Some(&ts) = map.get(t[5].to_ascii_uppercase().as_str()) else {
                diags.push(err(
                    l,
                    DiagnosticKind::UnresolvedReference { id: t[5].clone() },
                ));
                continue;
            };
            GageSource::Series { series: ts }
        } else if t[4].eq_ignore_ascii_case("FILE") {
            // Trailing unit token: the record's own depth unit (§2.4).
            let unit = match t.get(7).map(|u| u.to_ascii_uppercase()) {
                None => None,
                Some(u) if u == "IN" => Some(crate::model::RainFileUnit::Inches),
                Some(u) if u == "MM" => Some(crate::model::RainFileUnit::Millimetres),
                Some(_) => {
                    diags.push(bad(l, &t[7]));
                    continue;
                }
            };
            GageSource::File {
                file: t[5].clone(),
                station: t.get(6).cloned().unwrap_or_default(),
                unit,
            }
        } else {
            diags.push(bad(l, &t[4]));
            continue;
        };
        gages.push(Gage {
            id: t[0].clone(),
            form,
            interval,
            catch_factor: scf,
            source,
        });
    }
    gages
}

/// Parse `[SUBCATCHMENTS]` into parcels (their other two sections fill in
/// afterwards).
pub(crate) fn parse_parcels(
    lines: &[TokenLine],
    s: &Survey,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Parcel> {
    let mut parcels = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 8 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(gage) = s.resolve(ObjectKind::Gage, &t[1]) else {
            diags.push(err(
                l,
                DiagnosticKind::UnresolvedReference { id: t[1].clone() },
            ));
            continue;
        };
        // The outlet may be a vertex or another parcel; either resolves.
        let vertex = s.resolve(ObjectKind::Vertex, &t[2]);
        let parcel = s.resolve(ObjectKind::Parcel, &t[2]);
        let outlet = match (vertex, parcel) {
            (Some(&v), _) => ParcelOutlet::Vertex(v),
            (None, Some(&p)) => ParcelOutlet::Parcel(p),
            (None, None) => {
                diags.push(err(
                    l,
                    DiagnosticKind::UnresolvedReference { id: t[2].clone() },
                ));
                continue;
            }
        };
        let mut x = [0.0; 5]; // area, %imperv, width, %slope, curb length
        let mut ok = true;
        for (i, xi) in x.iter_mut().enumerate() {
            let Some(v) = number(t, 3 + i, diags, l) else {
                ok = false;
                break;
            };
            if v < 0.0 {
                diags.push(bad(l, &t[3 + i]));
                ok = false;
                break;
            }
            *xi = v;
        }
        if !ok {
            continue;
        }
        let snowpack = match t.get(8) {
            Some(tok) => {
                let Some(&sp) = s.resolve(ObjectKind::Snowpack, tok) else {
                    diags.push(err(
                        l,
                        DiagnosticKind::UnresolvedReference { id: tok.clone() },
                    ));
                    continue;
                };
                Some(sp)
            }
            None => None,
        };
        if x[1] > 100.0 {
            diags.push(err(
                l,
                DiagnosticKind::CappedValue {
                    what: "imperviousness",
                    token: t[4].clone(),
                },
            ));
        }
        parcels.push(Parcel {
            id: t[0].clone(),
            gage: *gage,
            outlet,
            area: x[0] * cv.land_area,
            // Above 100 % capped, not rejected, and reported (§14.7).
            frac_imperv: (x[1].min(100.0)) / 100.0,
            width: x[2] * cv.len,
            slope: x[3] / 100.0,
            curb_length: x[4] * cv.len,
            snowpack,
            land_cover: Vec::new(),
            init_buildup: Vec::new(),
            subareas: None,
            infiltration: None,
            groundwater: None,
            n_perv_pattern: None,
            dstore_pattern: None,
            infil_pattern: None,
        });
    }
    parcels
}

/// Fill `[SUBAREAS]` parameters into their parcels.
pub(crate) fn parse_subareas(
    lines: &[TokenLine],
    ids: &HashMap<String, usize>,
    parcels: &mut [Parcel],
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) {
    const ROUTING: &[&str] = &["OUTLET", "IMPERV", "PERV"];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 7 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&idx) = ids.get(t[0].to_ascii_uppercase().as_str()) else {
            diags.push(err(
                l,
                DiagnosticKind::UnresolvedReference { id: t[0].clone() },
            ));
            continue;
        };
        let mut x = [0.0; 5];
        let mut ok = true;
        for (i, xi) in x.iter_mut().enumerate() {
            let Some(v) = number(t, 1 + i, diags, l) else {
                ok = false;
                break;
            };
            if v < 0.0 {
                diags.push(bad(l, &t[1 + i]));
                ok = false;
                break;
            }
            *xi = v;
        }
        if !ok {
            continue;
        }
        let Some(r) = match_keyword(ROUTING, &t[6]) else {
            diags.push(bad(l, &t[6]));
            continue;
        };
        let mut frac_routed = 1.0;
        if let Some(tok) = t.get(7) {
            let Ok(v) = tok.finite_f64() else {
                diags.push(bad(l, tok));
                continue;
            };
            if !(0.0..=100.0).contains(&v) {
                diags.push(bad(l, tok));
                continue;
            }
            frac_routed = v / 100.0;
        }
        if let Some(p) = parcels.get_mut(idx) {
            p.subareas = Some(Subareas {
                n_imperv: x[0],
                n_perv: x[1],
                dstore_imperv: x[2] * cv.rain_depth,
                dstore_perv: x[3] * cv.rain_depth,
                frac_zero_store: x[4] / 100.0,
                routing: [
                    SubareaRouting::Outlet,
                    SubareaRouting::Impervious,
                    SubareaRouting::Pervious,
                ][r],
                frac_routed,
            });
        }
    }
}

/// Fill `[INFILTRATION]` parameters into their parcels. A trailing model
/// token overrides the global selection for that parcel (5.2).
pub(crate) fn parse_infiltration(
    lines: &[TokenLine],
    ids: &HashMap<String, usize>,
    parcels: &mut [Parcel],
    global: InfiltrationModel,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) {
    const MODELS: &[&str] = &[
        "HORTON",
        "MODIFIED_HORTON",
        "GREEN_AMPT",
        "MODIFIED_GREEN_AMPT",
        "CURVE_NUMBER",
    ];
    for line in lines {
        let mut t = line.tokens.clone();
        let l = line.line;
        if t.len() < 2 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&idx) = ids.get(t[0].to_ascii_uppercase().as_str()) else {
            diags.push(err(
                l,
                DiagnosticKind::UnresolvedReference { id: t[0].clone() },
            ));
            continue;
        };
        // Per-parcel override: the LAST token naming a model selects it.
        let mut model = global;
        if let Some(m) = match_keyword(MODELS, t.last().map_or("", |x| x.as_str())) {
            model = [
                InfiltrationModel::Horton,
                InfiltrationModel::ModifiedHorton,
                InfiltrationModel::GreenAmpt,
                InfiltrationModel::ModifiedGreenAmpt,
                InfiltrationModel::CurveNumber,
            ][m];
            t.pop();
        }
        let n = match model {
            InfiltrationModel::Horton | InfiltrationModel::ModifiedHorton => 5,
            _ => 4,
        };
        if t.len() < n {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let mut x = [0.0; 5];
        let mut ok = true;
        for i in 1..n {
            let Some(v) = number(&t, i, diags, l) else {
                ok = false;
                break;
            };
            x[i - 1] = v;
        }
        if !ok {
            continue;
        }
        // Horton's optional fifth parameter (maximum infiltration volume).
        if matches!(
            model,
            InfiltrationModel::Horton | InfiltrationModel::ModifiedHorton
        ) && t.len() > n
        {
            let Some(v) = number(&t, n, diags, l) else {
                continue;
            };
            x[n - 1] = v;
        }
        let infil = match model {
            InfiltrationModel::Horton | InfiltrationModel::ModifiedHorton => Infiltration::Horton {
                f0: x[0] * cv.conductivity,
                f_min: x[1] * cv.conductivity,
                decay: x[2] / 3600.0,
                dry_time: x[3] * 86_400.0,
                f_max: x[4] * cv.rain_depth,
            },
            InfiltrationModel::GreenAmpt | InfiltrationModel::ModifiedGreenAmpt => {
                Infiltration::GreenAmpt {
                    suction: x[0] * cv.suction,
                    conductivity: x[1] * cv.conductivity,
                    initial_deficit: x[2],
                }
            }
            InfiltrationModel::CurveNumber => Infiltration::CurveNumber {
                curve_number: x[0],
                dry_time: x[2] * 86_400.0,
            },
        };
        if let Some(p) = parcels.get_mut(idx) {
            p.infiltration = Some(infil);
        }
    }
}

/// Parse `[AQUIFERS]`.
pub(crate) fn parse_aquifers(
    lines: &[TokenLine],
    s: &Survey,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<crate::model::Aquifer> {
    let mut out = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 13 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let mut x = [0.0; 12];
        let mut ok = true;
        for (i, xi) in x.iter_mut().enumerate() {
            let Ok(v) = t[1 + i].finite_f64() else {
                diags.push(bad(l, &t[1 + i]));
                ok = false;
                break;
            };
            *xi = v;
        }
        if !ok {
            continue;
        }
        let evap_pattern = match t.get(13) {
            Some(tok) => {
                let Some(&p) = s.resolve(ObjectKind::TimePattern, tok) else {
                    diags.push(err(
                        l,
                        DiagnosticKind::UnresolvedReference { id: tok.clone() },
                    ));
                    continue;
                };
                Some(p)
            }
            None => None,
        };
        out.push(crate::model::Aquifer {
            id: t[0].clone(),
            porosity: x[0],
            wilting_point: x[1],
            field_capacity: x[2],
            conductivity: x[3] * cv.conductivity,
            conductivity_slope: x[4],
            tension_slope: x[5] * cv.len,
            upper_evap_frac: x[6],
            lower_evap_depth: x[7] * cv.len,
            lower_loss_coeff: x[8] * cv.conductivity,
            bottom_elev: x[9] * cv.len,
            water_table_elev: x[10] * cv.len,
            upper_moisture: x[11],
            evap_pattern,
        });
    }
    out
}

/// Fill `[GROUNDWATER]` connections into their parcels.
pub(crate) fn parse_groundwater(
    lines: &[TokenLine],
    s: &Survey,
    parcels: &mut [Parcel],
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) {
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 11 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&pc) = s.resolve(ObjectKind::Parcel, &t[0]) else {
            diags.push(err(
                l,
                DiagnosticKind::UnresolvedReference { id: t[0].clone() },
            ));
            continue;
        };
        let Some(&aq) = s.resolve(ObjectKind::Aquifer, &t[1]) else {
            diags.push(err(
                l,
                DiagnosticKind::UnresolvedReference { id: t[1].clone() },
            ));
            continue;
        };
        let Some(&vx) = s.resolve(ObjectKind::Vertex, &t[2]) else {
            diags.push(err(
                l,
                DiagnosticKind::UnresolvedReference { id: t[2].clone() },
            ));
            continue;
        };
        let mut x = [0.0; 7]; // surfElev, a1, b1, a2, b2, a3, fixedDepth
        let mut ok = true;
        for (i, xi) in x.iter_mut().enumerate() {
            let Ok(v) = t[3 + i].finite_f64() else {
                diags.push(bad(l, &t[3 + i]));
                ok = false;
                break;
            };
            *xi = v;
        }
        if !ok {
            continue;
        }
        // Four optional overrides, `*` skipping: threshold, bottom, water
        // table (lengths), then moisture (a fraction, unconverted).
        let mut over = [None; 4];
        for (i, oi) in over.iter_mut().enumerate() {
            let m = 10 + i;
            if let Some(tok) = t.get(m) {
                if tok.starts_with('*') {
                    continue;
                }
                let Ok(v) = tok.finite_f64() else {
                    diags.push(bad(l, tok));
                    ok = false;
                    break;
                };
                *oi = Some(if i < 3 { v * cv.len } else { v });
            }
        }
        if !ok {
            continue;
        }
        if let Some(p) = parcels.get_mut(pc) {
            // §14.6: the lateral-flow coefficients are defined in the
            // file's units — flow per area (cfs/acre or cms/ha) against
            // heads in feet or metres — and convert per their exponents.
            let gwq = if cv.len < 1.0 {
                // US file: cfs per acre.
                0.028_316_846_592 / 4_046.856_422_4
            } else {
                // SI file: cms per hectare.
                1.0e-4
            };
            p.groundwater = Some(crate::model::GroundwaterLink {
                aquifer: aq,
                vertex: vx,
                surface_elev: x[0] * cv.len,
                a1: x[1] * gwq / cv.len.powf(x[2]),
                b1: x[2],
                a2: x[3] * gwq / cv.len.powf(x[4]),
                b2: x[4],
                a3: x[5] * gwq / (cv.len * cv.len),
                fixed_surface_depth: x[6] * cv.len,
                threshold_elev: over[0],
                bottom_elev: over[1],
                water_table_elev: over[2],
                upper_moisture: over[3],
                lateral_expression: None,
                deep_expression: None,
            });
        }
    }
}

/// Fill `[GWF]` custom expressions into their parcels' groundwater links.
/// The expression is retained as written (§14.6: expressions evaluate in
/// the file's unit system).
pub(crate) fn parse_gwf(
    lines: &[TokenLine],
    s: &Survey,
    parcels: &mut [Parcel],
    diags: &mut Vec<Diagnostic>,
) {
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&pc) = s.resolve(ObjectKind::Parcel, &t[0]) else {
            diags.push(err(
                l,
                DiagnosticKind::UnresolvedReference { id: t[0].clone() },
            ));
            continue;
        };
        let expr = t[2..].join(" ");
        let Some(gw) = parcels.get_mut(pc).and_then(|p| p.groundwater.as_mut()) else {
            diags.push(err(
                l,
                DiagnosticKind::UnresolvedReference { id: t[0].clone() },
            ));
            continue;
        };
        if t[1].eq_ignore_ascii_case("LATERAL") {
            gw.lateral_expression = Some(expr);
        } else if t[1].eq_ignore_ascii_case("DEEP") {
            gw.deep_expression = Some(expr);
        } else {
            diags.push(bad(l, &t[1]));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::io::objects::parse_network;
    use crate::model::{GageSource, Infiltration, ParcelOutlet, RainForm, SubareaRouting};

    const FIXTURE: &str = "\
[OPTIONS]
FLOW_UNITS    CFS
INFILTRATION  GREEN_AMPT

[RAINGAGES]
G1  INTENSITY  0:15  1.0  TIMESERIES  TS1
G2  VOLUME     1.0   0.8  FILE  \"rain file.dat\"  STA01

[JUNCTIONS]
J1  100  3

[SUBCATCHMENTS]
S1  G1  J1  10  25  500  0.5  200
S2  G1  S1  5   80  250  1.0  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET
S2  0.015  0.2  0.06  0.30  10  PERV  50

[INFILTRATION]
S1  3.5  0.5  0.26
S2  3.0  0.5  4  7  0  HORTON

[TIMESERIES]
TS1  0  0.5  1  0.25
";

    #[test]
    fn gages_parse_both_sources() {
        let (net, diags) = parse_network(FIXTURE);
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "{:?}",
            diags
                .iter()
                .filter(|d| d.kind.is_error())
                .collect::<Vec<_>>()
        );
        assert_eq!(net.gages.len(), 2);
        assert_eq!(net.gages[0].form, RainForm::Intensity);
        assert_eq!(net.gages[0].interval, 900.0);
        let GageSource::Series { series } = net.gages[0].source else {
            panic!()
        };
        assert_eq!(net.timeseries[series].id, "TS1");
        let GageSource::File {
            ref file,
            ref station,
            ..
        } = net.gages[1].source
        else {
            panic!()
        };
        assert_eq!(file, "rain file.dat");
        assert_eq!(station, "STA01");
    }

    #[test]
    fn parcels_convert_and_resolve_both_outlet_kinds() {
        let (net, _) = parse_network(FIXTURE);
        let s1 = &net.parcels[0];
        assert_eq!(s1.outlet, ParcelOutlet::Vertex(0));
        // 10 acres → m².
        assert!((s1.area - 10.0 * 4_046.856_422_4).abs() < 1e-6);
        assert!((s1.frac_imperv - 0.25).abs() < 1e-12);
        assert!((s1.width - 500.0 * 0.3048).abs() < 1e-9);
        assert!((s1.slope - 0.005).abs() < 1e-12);
        let s2 = &net.parcels[1];
        assert_eq!(s2.outlet, ParcelOutlet::Parcel(0), "cascade onto S1");
    }

    #[test]
    fn subareas_fill_in_with_depth_conversion() {
        let (net, _) = parse_network(FIXTURE);
        let sa = net.parcels[0].subareas.as_ref().unwrap();
        assert_eq!(sa.n_imperv, 0.012);
        // 0.05 in → m.
        assert!((sa.dstore_imperv - 0.05 * 0.0254).abs() < 1e-12);
        assert_eq!(sa.frac_zero_store, 0.25);
        assert_eq!(sa.routing, SubareaRouting::Outlet);
        assert_eq!(sa.frac_routed, 1.0);
        let sb = net.parcels[1].subareas.as_ref().unwrap();
        assert_eq!(sb.routing, SubareaRouting::Pervious);
        assert_eq!(sb.frac_routed, 0.5);
    }

    #[test]
    fn infiltration_uses_the_global_model_and_the_per_parcel_override() {
        let (net, _) = parse_network(FIXTURE);
        // S1: global GREEN_AMPT — suction 3.5 in, Ksat 0.5 in/hr, IMD 0.26.
        let Infiltration::GreenAmpt {
            suction,
            conductivity,
            initial_deficit,
        } = *net.parcels[0].infiltration.as_ref().unwrap()
        else {
            panic!()
        };
        assert!((suction - 3.5 * 0.0254).abs() < 1e-12);
        assert!((conductivity - 0.5 * 0.0254 / 3600.0).abs() < 1e-15);
        assert_eq!(initial_deficit, 0.26);
        // S2: trailing HORTON overrides — f0 3, fmin 0.5, k 4/hr, dry 7 d.
        let Infiltration::Horton {
            f0,
            decay,
            dry_time,
            ..
        } = *net.parcels[1].infiltration.as_ref().unwrap()
        else {
            panic!()
        };
        assert!((f0 - 3.0 * 0.0254 / 3600.0).abs() < 1e-15);
        assert!((decay - 4.0 / 3600.0).abs() < 1e-15);
        assert_eq!(dry_time, 7.0 * 86_400.0);
    }

    #[test]
    fn aquifers_and_groundwater_links_parse_with_overrides() {
        let (net, diags) = parse_network(
            "[OPTIONS]\nFLOW_UNITS CFS\n[RAINGAGES]\nG1 VOLUME 1.0 1.0 FILE f.dat\n\
             [JUNCTIONS]\nJ1 100 3\n[SUBCATCHMENTS]\nS1 G1 J1 10 25 500 0.5 0\n\
             [SUBAREAS]\nS1 0.01 0.1 0.05 0.05 25 OUTLET\n\
             [AQUIFERS]\nAQ1 0.5 0.15 0.30 0.5 10 15 0.35 14 0.002 0 10 0.30\n\
             [GROUNDWATER]\nS1 AQ1 J1 6 0.001 2 0 0 0 0 * 0.4\n\
             [GWF]\nS1 LATERAL 0.001 * ( Hgw - Hcb )\n",
        );
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "{:?}",
            diags
                .iter()
                .filter(|d| d.kind.is_error())
                .collect::<Vec<_>>()
        );
        let aq = &net.aquifers[0];
        // Conductivity 0.5 in/hr -> m/s; tension slope 15 ft -> m.
        assert!((aq.conductivity - 0.5 * 0.0254 / 3600.0).abs() < 1e-15);
        assert!((aq.tension_slope - 15.0 * 0.3048).abs() < 1e-12);
        let gw = net.parcels[0].groundwater.as_ref().unwrap();
        assert_eq!(gw.aquifer, 0);
        assert_eq!(gw.vertex, 0);
        assert!((gw.surface_elev - 6.0 * 0.3048).abs() < 1e-12);
        // A1 = 0.001 (cfs/ac)/ft², b1 = 2: converted per its exponent
        // (§14.6) to (m/s)/m².
        let gwq = 0.028_316_846_592 / 4_046.856_422_4;
        assert!(
            (gw.a1 - 0.001 * gwq / 0.3048_f64.powi(2)).abs() < 1e-18,
            "a1 = {}",
            gw.a1
        );
        assert_eq!(gw.threshold_elev, None, "starred slot skipped");
        assert!((gw.bottom_elev.unwrap() - 0.4 * 0.3048).abs() < 1e-12);
        assert_eq!(
            gw.lateral_expression.as_deref(),
            Some("0.001 * ( Hgw - Hcb )")
        );
    }

    #[test]
    fn si_files_use_hectares_and_millimetres() {
        let (net, diags) = parse_network(
            "[OPTIONS]\nFLOW_UNITS LPS\n[RAINGAGES]\nG1 VOLUME 1.0 1.0 FILE f.dat\n\
             [JUNCTIONS]\nJ1 10 2\n[SUBCATCHMENTS]\nS1 G1 J1 2 50 100 1 0\n\
             [SUBAREAS]\nS1 0.01 0.1 2 5 25 OUTLET\n",
        );
        assert!(!diags.iter().any(|d| d.kind.is_error()));
        // 2 ha → m².
        assert_eq!(net.parcels[0].area, 20_000.0);
        // 2 mm → m.
        let sa = net.parcels[0].subareas.as_ref().unwrap();
        assert!((sa.dstore_imperv - 0.002).abs() < 1e-15);
    }
}
