//! Climate parsing (§3): `[TEMPERATURE]`, `[EVAPORATION]`, and
//! `[ADJUSTMENTS]`.
//!
//! Temperatures convert to °C (offsets as differences), evaporation rates
//! from in/day or mm/day to m/s, wind from mph or km/h to m/s. Rainfall
//! and conductivity adjustments are multiplicative and stay dimensionless.

use super::keywords::match_keyword;
use super::objects::UnitConverter;
use super::options::parse_date_token;
use super::survey::{Diagnostic, DiagnosticKind, ObjectKind, Survey, TokenLine};
use crate::model::{
    Climate, EvaporationSource, FileTempUnits, Network, SnowmeltParams, TemperatureSource,
    WindSource,
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

/// Emit the §14.3 notice when a keyword matched by prefix, not equality.
fn note_prefix(
    table: &[&'static str],
    i: usize,
    token: &str,
    diags: &mut Vec<Diagnostic>,
    l: usize,
) {
    if !token.eq_ignore_ascii_case(table[i]) {
        diags.push(err(
            l,
            DiagnosticKind::PrefixMatched {
                token: token.to_string(),
                matched: table[i],
            },
        ));
    }
}

/// Read twelve numbers starting at token `at`.
fn twelve(t: &[String], at: usize, diags: &mut Vec<Diagnostic>, l: usize) -> Option<[f64; 12]> {
    if t.len() < at + 12 {
        diags.push(err(l, DiagnosticKind::MissingItems));
        return None;
    }
    let mut x = [0.0; 12];
    for (i, xi) in x.iter_mut().enumerate() {
        let Ok(v) = t[at + i].parse::<f64>() else {
            diags.push(bad(l, &t[at + i]));
            return None;
        };
        *xi = v;
    }
    Some(x)
}

/// Rate factor for the evaporation unit: in/day or mm/day to m/s.
fn evap_factor(us: bool) -> f64 {
    (if us { 0.0254 } else { 0.001 }) / 86_400.0
}

/// Parse a `[TEMPERATURE]` section into `climate`.
pub(crate) fn parse_temperature(
    lines: &[TokenLine],
    s: &Survey,
    cv: &UnitConverter,
    us: bool,
    climate: &mut Climate,
    diags: &mut Vec<Diagnostic>,
) {
    const KEYS: &[&str] = &["TIMESERIES", "FILE", "WINDSPEED", "SNOWMELT", "ADC"];
    const UNITS: &[&str] = &["C10", "C", "F"];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        let Some(k) = match_keyword(KEYS, &t[0]) else {
            diags.push(bad(l, &t[0]));
            continue;
        };
        note_prefix(KEYS, k, &t[0], diags, l);
        match k {
            // TIMESERIES name
            0 => {
                if t.len() < 2 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let Some(&ts) = s
                    .ids
                    .get(&ObjectKind::TimeSeries)
                    .and_then(|m| m.get(&t[1]))
                else {
                    diags.push(unresolved(l, &t[1]));
                    continue;
                };
                climate.temperature = Some(TemperatureSource::Series(ts));
            }
            // FILE name (start) (units)
            1 => {
                if t.len() < 2 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let mut start = None;
                if t.len() > 2 && !t[2].starts_with('*') {
                    let Some(d) = parse_date_token(&t[2]) else {
                        diags.push(bad(l, &t[2]));
                        continue;
                    };
                    start = Some(d);
                }
                let mut units = if us {
                    FileTempUnits::Fahrenheit
                } else {
                    FileTempUnits::Celsius
                };
                if t.len() > 3 {
                    let Some(u) = match_keyword(UNITS, &t[3]) else {
                        diags.push(bad(l, &t[3]));
                        continue;
                    };
                    note_prefix(UNITS, u, &t[3], diags, l);
                    units = [
                        FileTempUnits::TenthsCelsius,
                        FileTempUnits::Celsius,
                        FileTempUnits::Fahrenheit,
                    ][u];
                }
                climate.temperature = Some(TemperatureSource::File {
                    name: t[1].clone(),
                    start,
                    units,
                });
            }
            // WINDSPEED FILE | WINDSPEED MONTHLY v1..v12 — the predecessor
            // never checks the MONTHLY word itself, only FILE by equality.
            2 => {
                if t.len() >= 2 && t[1].eq_ignore_ascii_case("FILE") {
                    climate.wind = WindSource::File;
                    continue;
                }
                let Some(v) = twelve(t, 2, diags, l) else {
                    continue;
                };
                // mph or km/h to m/s.
                let f = if us { 0.447_04 } else { 1.0 / 3.6 };
                climate.wind = WindSource::Monthly(v.map(|x| x * f));
            }
            // SNOWMELT v1..v6
            3 => {
                if t.len() < 7 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let mut x = [0.0_f64; 6];
                let mut ok = true;
                for (i, xi) in x.iter_mut().enumerate() {
                    match t[1 + i].parse::<f64>() {
                        Ok(v) => *xi = v,
                        Err(_) => {
                            diags.push(bad(l, &t[1 + i]));
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                climate.snowmelt = Some(SnowmeltParams {
                    snow_temp: if us { (x[0] - 32.0) * 5.0 / 9.0 } else { x[0] },
                    ati_weight: x[1],
                    negative_melt_ratio: x[2],
                    elevation: x[3] * cv.len,
                    latitude: x[4],
                    longitude_correction: x[5] * 60.0,
                });
            }
            // ADC IMPERV|PERV v1..v10
            _ => {
                if t.len() < 12 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                const COVERS: &[&str] = &["IMPERV", "PERV"];
                let Some(c) = match_keyword(COVERS, &t[1]) else {
                    diags.push(bad(l, &t[1]));
                    continue;
                };
                note_prefix(COVERS, c, &t[1], diags, l);
                let mut v = [0.0_f64; 10];
                let mut ok = true;
                for (i, vi) in v.iter_mut().enumerate() {
                    match t[2 + i].parse::<f64>() {
                        Ok(x) if (0.0..=1.0).contains(&x) => *vi = x,
                        _ => {
                            diags.push(bad(l, &t[2 + i]));
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                if c == 0 {
                    climate.adc_impervious = Some(v);
                } else {
                    climate.adc_pervious = Some(v);
                }
            }
        }
    }
}

/// Parse an `[EVAPORATION]` section into `climate`.
pub(crate) fn parse_evaporation(
    lines: &[TokenLine],
    s: &Survey,
    us: bool,
    climate: &mut Climate,
    diags: &mut Vec<Diagnostic>,
) {
    const KEYS: &[&str] = &[
        "CONSTANT",
        "MONTHLY",
        "TIMESERIES",
        "TEMPERATURE",
        "FILE",
        "RECOVERY",
        "DRY_ONLY",
    ];
    let f = evap_factor(us);
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        let Some(k) = match_keyword(KEYS, &t[0]) else {
            diags.push(bad(l, &t[0]));
            continue;
        };
        note_prefix(KEYS, k, &t[0], diags, l);
        // TEMPERATURE alone needs no value token.
        if k != 3 && k != 4 && t.len() < 2 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        match k {
            0 => {
                let Ok(v) = t[1].parse::<f64>() else {
                    diags.push(bad(l, &t[1]));
                    continue;
                };
                climate.evaporation = EvaporationSource::Constant(v * f);
            }
            1 => {
                let Some(v) = twelve(t, 1, diags, l) else {
                    continue;
                };
                climate.evaporation = EvaporationSource::Monthly(v.map(|x| x * f));
            }
            2 => {
                let Some(&ts) = s
                    .ids
                    .get(&ObjectKind::TimeSeries)
                    .and_then(|m| m.get(&t[1]))
                else {
                    diags.push(unresolved(l, &t[1]));
                    continue;
                };
                climate.evaporation = EvaporationSource::Series(ts);
            }
            3 => climate.evaporation = EvaporationSource::Temperature,
            4 => {
                // Pan coefficients are optional and default to 1.
                let mut pan = [1.0; 12];
                if t.len() > 1 {
                    let Some(v) = twelve(t, 1, diags, l) else {
                        continue;
                    };
                    pan = v;
                }
                climate.evaporation = EvaporationSource::File { pan };
            }
            5 => {
                let Some(&p) = s
                    .ids
                    .get(&ObjectKind::TimePattern)
                    .and_then(|m| m.get(&t[1]))
                else {
                    diags.push(unresolved(l, &t[1]));
                    continue;
                };
                climate.recovery_pattern = Some(p);
            }
            _ => {
                // DRY_ONLY YES|NO, by full comparison.
                if t[1].eq_ignore_ascii_case("YES") {
                    climate.evaporate_dry_only = true;
                } else if t[1].eq_ignore_ascii_case("NO") {
                    climate.evaporate_dry_only = false;
                } else {
                    diags.push(bad(l, &t[1]));
                }
            }
        }
    }
}

/// Parse an `[ADJUSTMENTS]` section into the network.
pub(crate) fn parse_adjustments(
    lines: &[TokenLine],
    s: &Survey,
    us: bool,
    net: &mut Network,
    diags: &mut Vec<Diagnostic>,
) {
    // The predecessor's reader accepts the shorter prefixes TEMP, EVAP,
    // RAIN, CONDUCT; the §14.3 equality standard is the canonical name.
    const PREFIXES: &[&str] = &[
        "TEMP", "EVAP", "RAIN", "CONDUCT", "N-PERV", "DSTORE", "INFIL",
    ];
    const CANONICAL: &[&str] = &[
        "TEMPERATURE",
        "EVAPORATION",
        "RAINFALL",
        "CONDUCTIVITY",
        "N-PERV",
        "DSTORE",
        "INFIL",
    ];
    let f = evap_factor(us);
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        // A single-token line is dropped without comment.
        if t.len() == 1 {
            continue;
        }
        let Some(k) = match_keyword(PREFIXES, &t[0]) else {
            diags.push(bad(l, &t[0]));
            continue;
        };
        note_prefix(CANONICAL, k, &t[0], diags, l);
        match k {
            0 => {
                if let Some(v) = twelve(t, 1, diags, l) {
                    // A temperature difference: °F offsets scale by 5/9.
                    let g = if us { 5.0 / 9.0 } else { 1.0 };
                    net.climate.adjust_temperature = v.map(|x| x * g);
                }
            }
            1 => {
                if let Some(v) = twelve(t, 1, diags, l) {
                    net.climate.adjust_evaporation = v.map(|x| x * f);
                }
            }
            2 => {
                if let Some(v) = twelve(t, 1, diags, l) {
                    net.climate.adjust_rainfall = v;
                }
            }
            3 => {
                if let Some(v) = twelve(t, 1, diags, l) {
                    // Zero or negative silently means "no adjustment" to
                    // the predecessor; reproduced, and warned (§3).
                    let mut out = [1.0; 12];
                    for (i, x) in v.iter().enumerate() {
                        if *x <= 0.0 {
                            diags.push(err(
                                l,
                                DiagnosticKind::SubstitutedOption {
                                    keyword: "CONDUCTIVITY",
                                    requested: t[1 + i].clone(),
                                },
                            ));
                        } else {
                            out[i] = *x;
                        }
                    }
                    net.climate.adjust_conductivity = out;
                }
            }
            _ => {
                // N-PERV | DSTORE | INFIL  parcel  pattern
                if t.len() < 3 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let Some(&parcel) = s.ids.get(&ObjectKind::Parcel).and_then(|m| m.get(&t[1]))
                else {
                    diags.push(unresolved(l, &t[1]));
                    continue;
                };
                let Some(&pat) = s
                    .ids
                    .get(&ObjectKind::TimePattern)
                    .and_then(|m| m.get(&t[2]))
                else {
                    diags.push(unresolved(l, &t[2]));
                    continue;
                };
                let p = &mut net.parcels[parcel];
                match k {
                    4 => p.n_perv_pattern = Some(pat),
                    5 => p.dstore_pattern = Some(pat),
                    _ => p.infil_pattern = Some(pat),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::io::objects::parse_network;
    use crate::model::{EvaporationSource, FileTempUnits, TemperatureSource, WindSource};

    const FIXTURE: &str = "\
[OPTIONS]
FLOW_UNITS  CFS

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  TS1

[TIMESERIES]
TS1  0:00  1.0

[PATTERNS]
P1  MONTHLY  1 1 1 1 1 1 1 1 1 1 1 1

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

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0

[TEMPERATURE]
FILE  climate.dat  *  C10
WINDSPEED  MONTHLY  1 2 3 4 5 6 7 8 9 10 11 12
SNOWMELT  34  0.5  0.6  100  45  -75
ADC  IMPERV  1 1 1 1 1 1 1 1 1 1

[EVAPORATION]
MONTHLY  1 2 3 4 5 6 7 8 9 10 11 12
DRY_ONLY  YES
RECOVERY  P1

[ADJUSTMENTS]
TEMPERATURE  9 0 0 0 0 0 0 0 0 0 0 0
RAINFALL     1.1 1 1 1 1 1 1 1 1 1 1 1
CONDUCTIVITY 0 1 1 1 1 1 1 1 1 1 1 2
N-PERV  S1  P1
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
    fn temperature_file_wind_and_snowmelt_convert() {
        let net = net_ok();
        let c = &net.climate;
        assert_eq!(
            c.temperature,
            Some(TemperatureSource::File {
                name: "climate.dat".into(),
                start: None,
                units: FileTempUnits::TenthsCelsius,
            })
        );
        let WindSource::Monthly(w) = &c.wind else {
            panic!("wind should be monthly");
        };
        // 2 mph in January's second slot.
        assert!((w[1] - 2.0 * 0.447_04).abs() < 1e-12);
        let sm = c.snowmelt.as_ref().unwrap();
        // 34 °F → 1.111 °C; 100 ft elevation; −75 min correction.
        assert!((sm.snow_temp - (34.0 - 32.0) * 5.0 / 9.0).abs() < 1e-12);
        assert!((sm.elevation - 100.0 * 0.3048).abs() < 1e-12);
        assert!((sm.longitude_correction - -4500.0).abs() < 1e-12);
        assert_eq!(c.adc_impervious, Some([1.0; 10]));
        assert_eq!(c.adc_pervious, None);
    }

    #[test]
    fn evaporation_monthly_converts_and_flags_read() {
        let net = net_ok();
        let c = &net.climate;
        let EvaporationSource::Monthly(m) = &c.evaporation else {
            panic!("evaporation should be monthly");
        };
        // 3 in/day in March.
        assert!((m[2] - 3.0 * 0.0254 / 86_400.0).abs() < 1e-15);
        assert!(c.evaporate_dry_only);
        assert_eq!(c.recovery_pattern, Some(0));
    }

    #[test]
    fn adjustments_convert_and_zero_conductivity_substitutes() {
        let (net, diags) = parse_network(FIXTURE);
        let c = &net.climate;
        // A 9 °F offset is a difference: ×5/9 → 5 K.
        assert!((c.adjust_temperature[0] - 5.0).abs() < 1e-12);
        assert!((c.adjust_rainfall[0] - 1.1).abs() < 1e-12);
        // January's zero became 1 with a notice; December's 2 stands.
        assert!((c.adjust_conductivity[0] - 1.0).abs() < 1e-12);
        assert!((c.adjust_conductivity[11] - 2.0).abs() < 1e-12);
        assert!(diags.iter().any(|d| matches!(
            &d.kind,
            crate::io::survey::DiagnosticKind::SubstitutedOption { keyword, .. }
                if *keyword == "CONDUCTIVITY"
        )));
        assert_eq!(net.parcels[0].n_perv_pattern, Some(0));
        assert_eq!(net.parcels[0].dstore_pattern, None);
    }
}
