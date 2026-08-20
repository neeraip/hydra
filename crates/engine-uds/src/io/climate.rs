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
use crate::io::lex::FiniteParse;
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
fn twelve(t: &[&str], at: usize, diags: &mut Vec<Diagnostic>, l: usize) -> Option<[f64; 12]> {
    if t.len() < at + 12 {
        diags.push(err(l, DiagnosticKind::MissingItems));
        return None;
    }
    let mut x = [0.0; 12];
    for (i, xi) in x.iter_mut().enumerate() {
        let Ok(v) = t[at + i].finite_f64() else {
            diags.push(bad(l, t[at + i]));
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
    lines: &[TokenLine<'_>],
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
        let Some(k) = match_keyword(KEYS, t[0]) else {
            diags.push(bad(l, t[0]));
            continue;
        };
        note_prefix(KEYS, k, t[0], diags, l);
        match k {
            // TIMESERIES name
            0 => {
                if t.len() < 2 {
                    diags.push(err(l, DiagnosticKind::MissingItems));
                    continue;
                }
                let Some(&ts) = s.resolve(ObjectKind::TimeSeries, t[1]) else {
                    diags.push(unresolved(l, t[1]));
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
                    let Some(d) = parse_date_token(t[2]) else {
                        diags.push(bad(l, t[2]));
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
                    let Some(u) = match_keyword(UNITS, t[3]) else {
                        diags.push(bad(l, t[3]));
                        continue;
                    };
                    note_prefix(UNITS, u, t[3], diags, l);
                    units = [
                        FileTempUnits::TenthsCelsius,
                        FileTempUnits::Celsius,
                        FileTempUnits::Fahrenheit,
                    ][u];
                }
                climate.temperature = Some(TemperatureSource::File {
                    name: t[1].to_string(),
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
                    match t[1 + i].finite_f64() {
                        Ok(v) => *xi = v,
                        Err(_) => {
                            diags.push(bad(l, t[1 + i]));
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
                let Some(c) = match_keyword(COVERS, t[1]) else {
                    diags.push(bad(l, t[1]));
                    continue;
                };
                note_prefix(COVERS, c, t[1], diags, l);
                let mut v = [0.0_f64; 10];
                let mut ok = true;
                for (i, vi) in v.iter_mut().enumerate() {
                    match t[2 + i].finite_f64() {
                        Ok(x) if (0.0..=1.0).contains(&x) => *vi = x,
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
    lines: &[TokenLine<'_>],
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
        let Some(k) = match_keyword(KEYS, t[0]) else {
            diags.push(bad(l, t[0]));
            continue;
        };
        note_prefix(KEYS, k, t[0], diags, l);
        // TEMPERATURE alone needs no value token.
        if k != 3 && k != 4 && t.len() < 2 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        match k {
            0 => {
                let Ok(v) = t[1].finite_f64() else {
                    diags.push(bad(l, t[1]));
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
                let Some(&ts) = s.resolve(ObjectKind::TimeSeries, t[1]) else {
                    diags.push(unresolved(l, t[1]));
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
                let Some(&p) = s.resolve(ObjectKind::TimePattern, t[1]) else {
                    diags.push(unresolved(l, t[1]));
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
                    diags.push(bad(l, t[1]));
                }
            }
        }
    }
}

/// Parse an `[ADJUSTMENTS]` section into the network.
pub(crate) fn parse_adjustments(
    lines: &[TokenLine<'_>],
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
        let Some(k) = match_keyword(PREFIXES, t[0]) else {
            diags.push(bad(l, t[0]));
            continue;
        };
        note_prefix(CANONICAL, k, t[0], diags, l);
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
                                    requested: t[1 + i].to_string(),
                                    used: "the month is left unadjusted",
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
                let Some(&parcel) = s.resolve(ObjectKind::Parcel, t[1]) else {
                    diags.push(unresolved(l, t[1]));
                    continue;
                };
                let Some(&pat) = s.resolve(ObjectKind::TimePattern, t[2]) else {
                    diags.push(unresolved(l, t[2]));
                    continue;
                };
                // The survey registry can be ahead of `net.parcels` when a
                // `[SUBCATCHMENTS]` line failed to parse (the file is
                // already being refused); indexing would panic.
                let Some(p) = net.parcels.get_mut(parcel) else {
                    continue;
                };
                match k {
                    4 => p.n_perv_pattern = Some(pat),
                    5 => p.dstore_pattern = Some(pat),
                    _ => p.infil_pattern = Some(pat),
                }
            }
        }
    }
}

/// The climate-file layouts this engine reads (§14.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClimateLayout {
    /// The predecessor's user-prepared format (§14.14).
    UserPrepared,
    /// NCDC TD-3200 fixed-field (§14.14.1).
    Td3200,
    /// Environment-Canada DLY02/DLY04 (§14.14.1).
    Dly0204,
    /// NCDC GHCN-Daily, header-positioned columns (§14.14.1).
    Ghcnd,
}

impl ClimateLayout {
    /// The layout's name, for a refusal that says what a file was read as.
    fn name(self) -> &'static str {
        match self {
            ClimateLayout::UserPrepared => "user-prepared",
            ClimateLayout::Td3200 => "TD-3200",
            ClimateLayout::Dly0204 => "DLY02/DLY04",
            ClimateLayout::Ghcnd => "GHCN-Daily",
        }
    }
}

/// A fixed-column field, trimmed, empty when the line is shorter than the
/// column (§14.14.1). These layouts are ASCII by construction.
fn field(line: &[u8], from: usize, to: usize) -> &str {
    let to = to.min(line.len());
    if from >= to {
        return "";
    }
    std::str::from_utf8(&line[from..to]).unwrap_or("").trim()
}

/// Which layout a climate file is written in (§14.14.1).
///
/// The tests are applied in a fixed order because they are not mutually
/// exclusive: a TD-3200 line is also long enough to be a Canadian one,
/// and a GHCN-Daily header can read as a user-prepared record. Only the
/// first line decides.
pub fn recognise_climate(text: &str) -> Option<ClimateLayout> {
    let first = text
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with(';'))?;
    let b = first.as_bytes();

    // TD-3200: `DLY` and the 9999 filler at its own column.
    if field(b, 0, 3) == "DLY" && field(b, 23, 27) == "9999" {
        return Some(ClimateLayout::Td3200);
    }
    // DLY02/DLY04: long enough, and an element code this engine serves.
    if b.len() >= 233 && matches!(field(b, 13, 16).parse::<i32>(), Ok(1 | 2 | 151)) {
        return Some(ClimateLayout::Dly0204);
    }
    // User-prepared: a station and a whole date, then a value.
    let t: Vec<&str> = first.split_whitespace().collect();
    if t.len() >= 5
        && t[1].parse::<i32>().is_ok()
        && t[2].parse::<u32>().is_ok()
        && t[3].parse::<u32>().is_ok()
    {
        return Some(ClimateLayout::UserPrepared);
    }
    // GHCN-Daily: a header naming a date column and at least one quantity.
    if first.contains("DATE")
        && ["TMAX", "TMIN", "EVAP", "WDMV", "AWND"]
            .iter()
            .any(|w| first.contains(w))
    {
        return Some(ClimateLayout::Ghcnd);
    }
    None
}

/// One day's readings while a file is being gathered.
#[derive(Default, Clone, Copy)]
struct DayValues {
    tmax: Option<f64>,
    tmin: Option<f64>,
    evap: Option<f64>,
    wind: Option<f64>,
}

/// Which quantity a line carries.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Quantity {
    Tmax,
    Tmin,
    Evap,
    Wind,
}

/// Gathers days from any layout, so a file whose lines are out of order
/// reads the same as one whose lines are not.
#[derive(Default)]
struct Days(std::collections::BTreeMap<(i32, u32, u32), DayValues>);

impl Days {
    fn put(&mut self, y: i32, m: u32, d: u32, q: Quantity, v: f64) {
        let e = self.0.entry((y, m, d)).or_default();
        match q {
            Quantity::Tmax => e.tmax = Some(v),
            Quantity::Tmin => e.tmin = Some(v),
            Quantity::Evap => e.evap = Some(v),
            Quantity::Wind => e.wind = Some(v),
        }
    }

    fn finish(self) -> Vec<crate::model::DailyClimate> {
        self.0
            .into_iter()
            .map(|((year, month, day), v)| crate::model::DailyClimate {
                date: crate::io::options::Date { year, month, day },
                tmax: v.tmax,
                tmin: v.tmin,
                evap: v.evap,
                wind: v.wind,
            })
            .collect()
    }
}

/// Fahrenheit to the model's own temperature unit (§14.14).
fn temp_to_user(f: f64, us: bool) -> f64 {
    if us {
        f
    } else {
        (f - 32.0) * 5.0 / 9.0
    }
}

/// Inches to the model's own depth unit (§14.14).
fn depth_to_user(inches: f64, us: bool) -> f64 {
    if us {
        inches
    } else {
        inches * 25.4
    }
}

/// Parse an NCDC TD-3200 file (§14.14.1): one line per station, quantity
/// and month, values in twelve-character groups from column 30.
fn parse_td3200(text: &str, us: bool, days: &mut Days) {
    for line in text.lines() {
        let b = line.as_bytes();
        if field(b, 0, 3) != "DLY" {
            continue;
        }
        let q = match field(b, 11, 15) {
            "TMAX" => Quantity::Tmax,
            "TMIN" => Quantity::Tmin,
            "EVAP" => Quantity::Evap,
            "WDMV" => Quantity::Wind,
            _ => continue,
        };
        let (Ok(year), Ok(month)) = (
            field(b, 17, 21).parse::<i32>(),
            field(b, 21, 23).parse::<u32>(),
        ) else {
            continue;
        };
        let count: usize = field(b, 27, 30).parse().unwrap_or(0);
        for j in 0..count {
            let k = 30 + j * 12;
            let Ok(day) = field(b, k, k + 2).parse::<u32>() else {
                continue;
            };
            let raw = field(b, k + 5, k + 10);
            // A flag that is neither 0 nor 1 marks a reading the record
            // does not stand behind, and 99999 marks one it does not have.
            let flag = field(b, k + 11, k + 12);
            if raw == "99999" || !matches!(flag, "0" | "1") || !(1..=31).contains(&day) {
                continue;
            }
            let Ok(mut v) = raw.parse::<f64>() else {
                continue;
            };
            if field(b, k + 4, k + 5) == "-" {
                v = -v;
            }
            let v = match q {
                // Whole degrees Fahrenheit.
                Quantity::Tmax | Quantity::Tmin => temp_to_user(v, us),
                // Hundredths of an inch.
                Quantity::Evap => depth_to_user(v / 100.0, us),
                // Miles per day, and the column is miles per hour (§14.14).
                Quantity::Wind => v / 24.0,
            };
            days.put(year, month, day, q, v);
        }
    }
}

/// Parse an Environment-Canada DLY02/DLY04 file (§14.14.1): thirty-one
/// seven-character groups from column 16, one per day of the month.
fn parse_dly0204(text: &str, us: bool, days: &mut Days) {
    for line in text.lines() {
        let b = line.as_bytes();
        if b.len() < 233 {
            continue;
        }
        let q = match field(b, 13, 16).parse::<i32>() {
            Ok(1) => Quantity::Tmax,
            Ok(2) => Quantity::Tmin,
            Ok(151) => Quantity::Evap,
            _ => continue,
        };
        let (Ok(year), Ok(month)) = (
            field(b, 7, 11).parse::<i32>(),
            field(b, 11, 13).parse::<u32>(),
        ) else {
            continue;
        };
        for day in 1..=31u32 {
            let k = 16 + (day as usize - 1) * 7;
            let raw = field(b, k + 1, k + 6);
            // Blank is missing here as well as 99999.
            if raw.is_empty() || raw == "99999" {
                continue;
            }
            let Ok(mut v) = raw.parse::<f64>() else {
                continue;
            };
            if field(b, k, k + 1) == "-" {
                v = -v;
            }
            let v = match q {
                // Tenths of a degree Celsius.
                Quantity::Tmax | Quantity::Tmin => {
                    let c = v / 10.0;
                    if us {
                        c * 9.0 / 5.0 + 32.0
                    } else {
                        c
                    }
                }
                // Tenths of a millimetre.
                Quantity::Evap => depth_to_user(v / 10.0 / 25.4, us),
                Quantity::Wind => continue,
            };
            days.put(year, month, day, q, v);
        }
    }
}

/// Parse an NCDC GHCN-Daily export (§14.14.1): a header names the columns
/// and each value is read from the column its name begins at.
fn parse_ghcnd(text: &str, us: bool, units: FileTempUnits, days: &mut Days) {
    let mut lines = text.lines();
    let Some(header) = lines.find(|l| !l.trim().is_empty()) else {
        return;
    };
    let Some(date_at) = header.find("DATE") else {
        return;
    };
    // Daily movement if the header names it, average speed otherwise.
    let movement = header.contains("WDMV");
    let cols: Vec<(Quantity, usize)> = [
        (Quantity::Tmax, "TMAX"),
        (Quantity::Tmin, "TMIN"),
        (Quantity::Evap, "EVAP"),
        (Quantity::Wind, if movement { "WDMV" } else { "AWND" }),
    ]
    .into_iter()
    .filter_map(|(q, w)| header.find(w).map(|at| (q, at)))
    .collect();

    // A value begins at its heading's own column and runs to the next gap.
    let at = |line: &str, from: usize| -> Option<f64> {
        line.get(from..)?
            .split_whitespace()
            .next()?
            .parse::<f64>()
            .ok()
    };
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let Some(stamp) = line.get(date_at..).map(|s| s.trim_start()) else {
            continue;
        };
        if stamp.len() < 8 {
            continue;
        }
        let (Ok(year), Ok(month), Ok(day)) = (
            stamp[0..4].parse::<i32>(),
            stamp[4..6].parse::<u32>(),
            stamp[6..8].parse::<u32>(),
        ) else {
            continue;
        };
        if !(1..=31).contains(&day) {
            continue;
        }
        for &(q, from) in &cols {
            let Some(v) = at(line, from) else { continue };
            if v.abs() >= 9999.0 {
                continue;
            }
            let v = match q {
                Quantity::Tmax | Quantity::Tmin => temp_to_user(
                    match units {
                        FileTempUnits::TenthsCelsius => v / 10.0 * 9.0 / 5.0 + 32.0,
                        FileTempUnits::Celsius => v * 9.0 / 5.0 + 32.0,
                        FileTempUnits::Fahrenheit => v,
                    },
                    us,
                ),
                Quantity::Evap => depth_to_user(
                    match units {
                        FileTempUnits::TenthsCelsius => v / 10.0 / 25.4,
                        FileTempUnits::Celsius => v / 25.4,
                        FileTempUnits::Fahrenheit => v,
                    },
                    us,
                ),
                // Every arm lands on miles per hour (§14.14).
                Quantity::Wind => match (units, movement) {
                    (FileTempUnits::TenthsCelsius, true) => v * 0.62137 / 24.0,
                    (FileTempUnits::TenthsCelsius, false) => v / 10.0 / 1000.0 * 0.62137 * 3600.0,
                    (FileTempUnits::Celsius, true) => v * 0.62137 / 24.0,
                    (FileTempUnits::Celsius, false) => v / 1000.0 * 0.62137 * 3600.0,
                    (FileTempUnits::Fahrenheit, true) => v / 24.0,
                    (FileTempUnits::Fahrenheit, false) => v,
                },
            };
            days.put(year, month, day, q, v);
        }
    }
}

/// Parse a climate file in whichever layout it is written in (§14.14).
///
/// `us` is the model's unit system and `units` the declared units word,
/// which governs the GHCN-Daily exports alone. Values come back in the
/// model's own units, wind in miles per hour (§14.14).
pub fn parse_any_climate_file(
    text: &str,
    us: bool,
    units: FileTempUnits,
) -> Result<(Vec<crate::model::DailyClimate>, Vec<String>), String> {
    let Some(layout) = recognise_climate(text) else {
        return Err("climate file is in none of the served layouts \
                    (user-prepared, TD-3200, DLY02/DLY04, GHCN-Daily)"
            .into());
    };
    let mut notices = Vec::new();
    // §14.14: the units word governs the GHCN-Daily exports alone. The
    // other layouts carry their own units, so a word declared beside one
    // of them changes nothing and is said rather than silently dropped.
    // Only a declared word can differ from what this model would default
    // to, so a default never speaks.
    let default_units = if us {
        FileTempUnits::Fahrenheit
    } else {
        FileTempUnits::Celsius
    };
    if units != default_units && matches!(layout, ClimateLayout::Td3200 | ClimateLayout::Dly0204) {
        notices.push(format!(
            "the declared units word has no effect on a {} file, which \
             carries its own units",
            layout.name()
        ));
    }
    if layout == ClimateLayout::UserPrepared {
        return parse_climate_file(text).map(|r| (r, notices));
    }
    let mut days = Days::default();
    match layout {
        ClimateLayout::Td3200 => parse_td3200(text, us, &mut days),
        ClimateLayout::Dly0204 => parse_dly0204(text, us, &mut days),
        ClimateLayout::Ghcnd => parse_ghcnd(text, us, units, &mut days),
        ClimateLayout::UserPrepared => unreachable!("served above"),
    }
    let out = days.finish();
    // §14.14.1: a file recognised as a layout that yields nothing was
    // read at the wrong columns. The predecessor reports nothing and runs
    // the whole simulation at its default weather.
    if out.is_empty() {
        return Err(format!(
            "climate file was read as {} and holds no readings: its values \
             are not in the columns that layout puts them in",
            layout.name()
        ));
    }
    Ok((out, notices))
}

/// Parse a user-format daily climate file (§14.14): one record per line,
/// `station year month day tmax tmin (evap) (wind)`, `*` for a missing
/// value. Use [`parse_any_climate_file`] to accept the archival layouts
/// of §14.14.1 as well.
pub fn parse_climate_file(text: &str) -> Result<Vec<crate::model::DailyClimate>, String> {
    let mut out = Vec::new();
    for (ln, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        // A file in one of the archival layouts reaching this reader is a
        // caller using the wrong door, not a malformed record: say which
        // layout it is rather than complain about its tokens.
        let wrong_door = || match recognise_climate(text) {
            Some(l) if l != ClimateLayout::UserPrepared => Some(format!(
                "climate file is in the {} layout; read it with the layout-detecting \
                 reader rather than the user-prepared one",
                l.name()
            )),
            _ => None,
        };
        if t.len() < 6 {
            if let Some(e) = wrong_door() {
                return Err(e);
            }
            return Err(format!("climate line {}: too few values", ln + 1));
        }
        if t[1].parse::<i32>().is_err() {
            if let Some(e) = wrong_door() {
                return Err(e);
            }
        }
        let num = |s: &str| -> Result<Option<f64>, String> {
            if s == "*" {
                return Ok(None);
            }
            s.finite_f64()
                .map(Some)
                .map_err(|_| format!("climate line {}: bad value '{s}'", ln + 1))
        };
        let year: i32 = t[1]
            .parse()
            .map_err(|_| format!("climate line {}: bad year", ln + 1))?;
        let month: u32 = t[2]
            .parse()
            .map_err(|_| format!("climate line {}: bad month", ln + 1))?;
        let day: u32 = t[3]
            .parse()
            .map_err(|_| format!("climate line {}: bad day", ln + 1))?;
        out.push(crate::model::DailyClimate {
            date: crate::io::options::Date { year, month, day },
            tmax: num(t[4])?,
            tmin: num(t[5])?,
            evap: t.get(6).map(|s| num(s)).transpose()?.flatten(),
            wind: t.get(7).map(|s| num(s)).transpose()?.flatten(),
        });
    }
    Ok(out)
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

#[cfg(test)]
mod archive_tests {
    use super::*;
    use crate::model::DailyClimate;

    /// Each fixture was run through SWMM 5 with `[TEMPERATURE] FILE` and
    /// `[EVAPORATION] FILE`, and the values asserted here are what its
    /// binary output carried in the system air-temperature and
    /// potential-evaporation series. See
    /// `tests/fixtures/uds/climate/README.txt`.
    fn fixture(name: &str) -> String {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/uds/climate")
            .join(name);
        std::fs::read_to_string(path).expect("fixture readable")
    }

    fn read(name: &str) -> Vec<DailyClimate> {
        parse_any_climate_file(&fixture(name), true, FileTempUnits::Fahrenheit)
            .expect(name)
            .0
    }

    fn close(a: Option<f64>, b: f64, what: &str) {
        let got = a.unwrap_or_else(|| panic!("{what}: no reading"));
        assert!((got - b).abs() < 1e-4, "{what}: {got} not {b}");
    }

    // ── Recognition ─────────────────────────────────────────────────────

    /// The four tests are order-dependent: a TD-3200 line is long enough
    /// to be a Canadian one, and a GHCN-Daily header reads as a
    /// user-prepared record if the user test is tried first.
    #[test]
    fn each_layout_is_recognised_from_its_own_first_line() {
        for (name, want) in [
            ("user.dat", ClimateLayout::UserPrepared),
            ("td3200.dat", ClimateLayout::Td3200),
            ("dly0204.dat", ClimateLayout::Dly0204),
            ("ghcnd.dat", ClimateLayout::Ghcnd),
        ] {
            assert_eq!(Some(want), recognise_climate(&fixture(name)), "{name}");
        }
    }

    #[test]
    fn a_file_in_no_layout_is_recognised_as_none() {
        assert_eq!(None, recognise_climate(&fixture("unknown.dat")));
        let err = parse_any_climate_file(&fixture("unknown.dat"), true, FileTempUnits::Fahrenheit)
            .unwrap_err();
        assert!(err.contains("none of the served layouts"), "{err}");
    }

    /// A Canadian line is 233 characters and a TD-3200 line is 54, but
    /// nothing stops a TD-3200 file having long lines: the `9999` filler
    /// is what separates them, and it is tested first.
    #[test]
    fn a_long_td3200_line_is_not_a_canadian_one() {
        let padded = fixture("td3200.dat")
            .lines()
            .map(|l| format!("{l:<300}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(Some(ClimateLayout::Td3200), recognise_climate(&padded));
    }

    // ── TD-3200 ─────────────────────────────────────────────────────────

    /// The reference run: this file and `user.dat` produce an identical
    /// simulation, so the columns are read where SWMM reads them.
    #[test]
    fn td3200_reads_its_columns_where_the_predecessor_reads_them() {
        let r = read("td3200.dat");
        assert_eq!(2, r.len(), "two days");
        assert_eq!(
            (2024, 1, 15),
            (r[0].date.year, r[0].date.month, r[0].date.day)
        );
        assert_eq!(
            (2024, 1, 16),
            (r[1].date.year, r[1].date.month, r[1].date.day)
        );
        close(r[0].tmax, 55.0, "day 1 tmax");
        close(r[0].tmin, 33.0, "day 1 tmin");
        close(r[0].evap, 0.20, "day 1 evaporation, hundredths of an inch");
        close(r[1].tmax, 61.0, "day 2 tmax");
        close(r[1].tmin, 39.0, "day 2 tmin");
        close(r[1].evap, 0.30, "day 2 evaporation");
    }

    /// A missing reading is absent, not zero: the session holds the last
    /// value it had, which is what the predecessor's run does.
    #[test]
    fn a_td3200_reading_of_99999_is_absent() {
        let r = read("td3200_missing.dat");
        close(r[0].tmax, 55.0, "day 1 tmax");
        // A day with nothing to say produces no record, so the session's
        // "inherit the most recent" (§3.1) leaves the previous day's
        // weather standing, which is what the predecessor's run does.
        assert_eq!(1, r.len(), "only the day with readings is a record");
        assert_eq!(15, r[0].date.day);
    }

    /// The flag rule on its own: only the flagged quantity is dropped,
    /// and only for its own day.
    #[test]
    fn a_td3200_reading_whose_flag_is_neither_zero_nor_one_is_absent() {
        let r = read("td3200_badflag.dat");
        close(r[0].tmax, 55.0, "day 1 is unflagged and reads");
        assert_eq!(None, r[1].tmax, "day 2's maximum is flagged away");
        close(r[1].tmin, 39.0, "but its minimum is not");
    }

    // ── DLY02/DLY04 ─────────────────────────────────────────────────────

    /// Tenths of a degree Celsius and tenths of a millimetre, both
    /// converted into this model's units. The evaporation values are the
    /// ones SWMM's run reported to five decimal places.
    #[test]
    fn dly0204_reads_tenths_of_a_degree_and_of_a_millimetre() {
        let r = read("dly0204.dat");
        assert_eq!(2, r.len());
        assert_eq!(
            (2024, 1, 15),
            (r[0].date.year, r[0].date.month, r[0].date.day)
        );
        close(r[0].tmax, 12.8 * 9.0 / 5.0 + 32.0, "day 1 tmax, 12.8 C");
        close(r[0].tmin, 0.6 * 9.0 / 5.0 + 32.0, "day 1 tmin, 0.6 C");
        close(r[0].evap, 5.0 / 25.4, "day 1 evaporation, 5.0 mm");
        close(r[1].tmax, 15.0 * 9.0 / 5.0 + 32.0, "day 2 tmax, 15.0 C");
        close(r[1].evap, 7.6 / 25.4, "day 2 evaporation, 7.6 mm");
    }

    #[test]
    fn a_blank_dly0204_field_is_absent_like_99999() {
        let r = read("dly0204_blank.dat");
        close(r[0].tmax, 12.8 * 9.0 / 5.0 + 32.0, "day 1 reads");
        assert!(
            r.len() == 1 || r[1].tmax.is_none(),
            "day 2's blank field is not a reading"
        );
    }

    // ── GHCN-Daily ──────────────────────────────────────────────────────

    #[test]
    fn ghcnd_reads_each_column_from_its_headings_position() {
        let r = read("ghcnd.dat");
        assert_eq!(2, r.len());
        assert_eq!(
            (2024, 1, 15),
            (r[0].date.year, r[0].date.month, r[0].date.day)
        );
        assert_eq!(
            (2024, 1, 16),
            (r[1].date.year, r[1].date.month, r[1].date.day)
        );
        close(r[0].tmax, 55.0, "day 1 tmax");
        close(r[0].tmin, 33.0, "day 1 tmin");
        close(r[0].evap, 0.20, "day 1 evaporation");
        close(r[1].tmax, 61.0, "day 2 tmax");
    }

    /// Missing here is a magnitude rather than a sentinel string, so the
    /// threshold itself is the decision.
    #[test]
    fn a_ghcnd_value_of_9999_or_more_is_absent() {
        let r = read("ghcnd_missing.dat");
        close(r[0].tmax, 55.0, "day 1 reads");
        assert_eq!(1, r.len(), "day 2 has nothing to say");
        // And a value just inside the threshold is a reading.
        let inside = fixture("ghcnd_missing.dat").replace("9999   ", "9998   ");
        let (r, _) =
            parse_any_climate_file(&inside, true, FileTempUnits::Fahrenheit).expect("parse");
        assert_eq!(2, r.len(), "9998 is a temperature, not an absence");
        close(r[1].tmax, 9998.0, "day 2 reads");
    }

    /// The units word is an input token, not a file field, and the same
    /// temperatures written in Celsius read as the same weather.
    #[test]
    fn the_units_word_governs_the_ghcnd_exports() {
        let (c, _) = parse_any_climate_file(&fixture("ghcnd_c.dat"), true, FileTempUnits::Celsius)
            .expect("celsius");
        close(c[0].tmax, 12.8 * 9.0 / 5.0 + 32.0, "12.8 C as F");
        close(c[0].tmin, 0.6 * 9.0 / 5.0 + 32.0, "0.6 C as F");

        // Read as Fahrenheit the same file is 12.8 °F, which is the point
        // of declaring the word at all.
        let (f, _) =
            parse_any_climate_file(&fixture("ghcnd_c.dat"), true, FileTempUnits::Fahrenheit)
                .expect("fahrenheit");
        close(f[0].tmax, 12.8, "the same column read as Fahrenheit");
    }

    // ── Units of the model, and the refusal ─────────────────────────────

    /// Values come back in the model's own units, so a metric model reads
    /// the same file as degrees Celsius and millimetres.
    #[test]
    fn values_arrive_in_the_models_own_units() {
        let (si, _) =
            parse_any_climate_file(&fixture("td3200.dat"), false, FileTempUnits::Fahrenheit)
                .expect("metric");
        close(si[0].tmax, (55.0 - 32.0) * 5.0 / 9.0, "55 F as C");
        close(si[0].evap, 0.20 * 25.4, "0.20 in as mm");
    }

    /// §14.14.1: a file recognised as a layout that yields nothing was
    /// read at the wrong columns. The predecessor reports nothing and runs
    /// the whole simulation at its default weather.
    #[test]
    fn a_recognised_layout_that_yields_nothing_is_refused() {
        // A TD-3200 file whose value groups sit one column late. The
        // columns recognition reads are untouched, so it is still read as
        // TD-3200, and every day parses out of the wrong place.
        let shifted = fixture("td3200.dat")
            .lines()
            .map(|l| format!("{} {}", &l[..30], &l[30..]))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(Some(ClimateLayout::Td3200), recognise_climate(&shifted));
        let err = parse_any_climate_file(&shifted, true, FileTempUnits::Fahrenheit).unwrap_err();
        assert!(err.contains("TD-3200"), "names the layout: {err}");
        assert!(err.contains("no readings"), "{err}");
    }

    /// §14.14: the units word governs the GHCN-Daily exports alone. A
    /// modeller who declares one beside a file that carries its own units
    /// has said something with no effect, and is told rather than left to
    /// believe it worked.
    #[test]
    fn a_units_word_declared_beside_a_layout_that_carries_its_own_is_reported() {
        let (_, notices) =
            parse_any_climate_file(&fixture("td3200.dat"), true, FileTempUnits::TenthsCelsius)
                .expect("parse");
        assert_eq!(1, notices.len(), "{notices:?}");
        assert!(notices[0].contains("TD-3200"), "{}", notices[0]);
        assert!(notices[0].contains("no effect"), "{}", notices[0]);

        // The same word on a GHCN-Daily file does have an effect, so it
        // says nothing.
        let (_, quiet) =
            parse_any_climate_file(&fixture("ghcnd.dat"), true, FileTempUnits::TenthsCelsius)
                .expect("parse");
        assert!(quiet.is_empty(), "{quiet:?}");

        // And a word this model would have defaulted to anyway was never
        // declared, so it says nothing either.
        let (_, defaulted) =
            parse_any_climate_file(&fixture("td3200.dat"), true, FileTempUnits::Fahrenheit)
                .expect("parse");
        assert!(defaulted.is_empty(), "{defaulted:?}");
    }

    // ── Signs, wind, and the rest of the units table ────────────────────

    /// A below-freezing minimum is ordinary winter weather, and the sign
    /// is a column of its own ahead of the digits in both fixed-column
    /// layouts. Until this existed the negation could be deleted outright
    /// and every fixture still read.
    #[test]
    fn a_negative_reading_reads_negative() {
        let r = read("td3200_negative.dat");
        close(r[0].tmax, 20.0, "day 1 tmax");
        close(r[0].tmin, -5.0, "day 1 tmin, below zero");
        close(r[1].tmin, -10.0, "day 2 tmin");

        let r = read("dly0204_negative.dat");
        close(r[0].tmax, 12.8 * 9.0 / 5.0 + 32.0, "12.8 C");
        close(r[0].tmin, -5.5 * 9.0 / 5.0 + 32.0, "-5.5 C");
    }

    /// §14.14: every wind arm lands on miles per hour, whatever the
    /// layout and whatever the declared units. TD-3200 writes miles per
    /// day.
    #[test]
    fn td3200_wind_converts_from_miles_a_day() {
        let line = fixture("td3200.dat")
            .lines()
            .next()
            .expect("a line")
            .replace("TMAX", "WDMV");
        let (r, _) = parse_any_climate_file(&line, true, FileTempUnits::Fahrenheit).expect("parse");
        // The TMAX column held 55; as daily movement that is 55 miles a
        // day, which is 55/24 miles an hour.
        close(r[0].wind, 55.0 / 24.0, "miles a day to miles an hour");
    }

    /// The GHCN-Daily units word selects a family for all three
    /// quantities at once, and wind additionally depends on which
    /// heading the file carries. Every arm is a different conversion.
    #[test]
    fn the_ghcnd_units_word_converts_every_quantity() {
        let file = |wind_word: &str, v: f64| {
            format!(
                "STATION    DATE       TMAX       EVAP       {wind_word}\n\
                 US1        20240115   100        50         {v}\n"
            )
        };
        let read1 = |units, wind_word: &str, v: f64| {
            let (r, _) = parse_any_climate_file(&file(wind_word, v), true, units).expect("parse");
            r
        };

        // Tenths of a degree Celsius, tenths of a millimetre.
        let r = read1(FileTempUnits::TenthsCelsius, "WDMV", 24.0);
        close(r[0].tmax, 10.0 * 9.0 / 5.0 + 32.0, "100 tenths C is 10 C");
        close(r[0].evap, 5.0 / 25.4, "50 tenths mm is 5 mm");
        close(
            r[0].wind,
            24.0 * 0.62137 / 24.0,
            "km a day to miles an hour",
        );

        // Whole degrees Celsius, whole millimetres.
        let r = read1(FileTempUnits::Celsius, "WDMV", 24.0);
        close(r[0].tmax, 100.0 * 9.0 / 5.0 + 32.0, "100 C");
        close(r[0].evap, 50.0 / 25.4, "50 mm");
        close(r[0].wind, 24.0 * 0.62137 / 24.0, "km a day");

        // Fahrenheit and inches, and miles a day.
        let r = read1(FileTempUnits::Fahrenheit, "WDMV", 24.0);
        close(r[0].tmax, 100.0, "100 F");
        close(r[0].evap, 50.0, "50 inches");
        close(r[0].wind, 1.0, "24 miles a day is one mile an hour");

        // Average speed rather than daily movement: a different column
        // name and a different conversion in every unit family.
        let r = read1(FileTempUnits::Fahrenheit, "AWND", 7.0);
        close(r[0].wind, 7.0, "miles an hour is already miles an hour");
        let r = read1(FileTempUnits::Celsius, "AWND", 10.0);
        close(
            r[0].wind,
            10.0 / 1000.0 * 0.62137 * 3600.0,
            "metres a second",
        );
        let r = read1(FileTempUnits::TenthsCelsius, "AWND", 100.0);
        close(
            r[0].wind,
            100.0 / 10.0 / 1000.0 * 0.62137 * 3600.0,
            "tenths of a metre a second",
        );
    }

    // ── Recognition, condition by condition ─────────────────────────────

    /// Each half of each recognition test is doing work: a file with the
    /// stamp but not the filler is not TD-3200, and one with the filler
    /// but not the stamp is not either.
    #[test]
    fn both_halves_of_each_recognition_test_are_necessary() {
        let td = fixture("td3200.dat");
        let first = td.lines().next().expect("a line");
        // The stamp without the filler.
        let no_filler = format!("{}0000{}", &first[..23], &first[27..]);
        assert_ne!(
            Some(ClimateLayout::Td3200),
            recognise_climate(&no_filler),
            "the 9999 filler is half the test"
        );
        // The filler without the stamp.
        let no_stamp = format!("XXX{}", &first[3..]);
        assert_ne!(
            Some(ClimateLayout::Td3200),
            recognise_climate(&no_stamp),
            "the DLY stamp is the other half"
        );

        // A Canadian line needs both its length and an element code.
        let dly = fixture("dly0204.dat");
        let line = dly.lines().next().expect("a line");
        assert_eq!(Some(ClimateLayout::Dly0204), recognise_climate(line));
        assert_ne!(
            Some(ClimateLayout::Dly0204),
            recognise_climate(&format!("{}999{}", &line[..13], &line[16..])),
            "an element code this engine does not serve is not this layout"
        );
        assert_ne!(
            Some(ClimateLayout::Dly0204),
            recognise_climate(line[..232].trim_end()),
            "a line short of 233 characters is not this layout"
        );

        // A GHCN-Daily header needs a date column and a quantity.
        assert_eq!(
            None,
            recognise_climate("STATION    DATE"),
            "a date column alone names no quantity"
        );
        assert_eq!(
            None,
            recognise_climate("STATION    TMAX"),
            "and a quantity alone has no dates"
        );
    }

    /// The user-prepared reader tells a caller that used the wrong door
    /// which layout the file actually is, rather than complaining about
    /// tokens that were never meant for it.
    #[test]
    fn the_user_reader_names_the_layout_it_was_handed() {
        let err = parse_climate_file(&fixture("td3200.dat")).unwrap_err();
        assert!(err.contains("TD-3200"), "{err}");
        let err = parse_climate_file(&fixture("ghcnd.dat")).unwrap_err();
        assert!(err.contains("GHCN-Daily"), "{err}");
        // A genuinely malformed user record still reports itself.
        let err = parse_climate_file(
            "STA01 2024 1 15
",
        )
        .unwrap_err();
        assert!(err.contains("too few values"), "{err}");
    }

    /// Recognition looks at the first line that says something, so a file
    /// opening with a blank line or a comment is recognised by its first
    /// record rather than by the blank.
    #[test]
    fn a_leading_blank_or_comment_is_not_the_line_that_decides() {
        for lead in ["", "\n", "; a note about this station\n", "\n;note\n\n"] {
            let text = format!("{lead}{}", fixture("td3200.dat"));
            assert_eq!(
                Some(ClimateLayout::Td3200),
                recognise_climate(&text),
                "leading {lead:?}"
            );
        }
        // And the user-prepared reader skips them as records too.
        let (r, _) = parse_any_climate_file(
            "; station STA01\n\nSTA01 2024 1 15 55.0 33.0\n",
            true,
            FileTempUnits::Fahrenheit,
        )
        .expect("parse");
        assert_eq!(1, r.len(), "the comment and the blank are not records");
        close(r[0].tmax, 55.0, "the record after them reads");
    }

    /// A Canadian line is at least 233 characters, not exactly 233: a
    /// file carrying anything after its last day still reads, and one
    /// cut short does not.
    #[test]
    fn a_canadian_line_needs_its_length_and_may_exceed_it() {
        let text = fixture("dly0204.dat");
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();

        // Trailing content beyond the thirty-first day changes nothing.
        let padded: Vec<String> = lines.iter().map(|l| format!("{l}   trailing")).collect();
        let (r, _) = parse_any_climate_file(&padded.join("\n"), true, FileTempUnits::Fahrenheit)
            .expect("padded");
        close(r[0].tmax, 12.8 * 9.0 / 5.0 + 32.0, "still reads");

        // A line one character short is not a record of this layout.
        lines[1].truncate(232);
        let (r, _) = parse_any_climate_file(&lines.join("\n"), true, FileTempUnits::Fahrenheit)
            .expect("short second line");
        close(r[0].tmax, 12.8 * 9.0 / 5.0 + 32.0, "the whole line reads");
        assert_eq!(None, r[0].tmin, "the short one does not");
    }

    /// A GHCN-Daily stamp is eight characters, and eight is enough: the
    /// date may be the last column on the line.
    #[test]
    fn a_ghcnd_date_of_exactly_eight_characters_is_a_date() {
        let text = "TMAX       DATE\n55         20240115\n";
        let (r, _) = parse_any_climate_file(text, true, FileTempUnits::Fahrenheit).expect("parse");
        assert_eq!(1, r.len());
        assert_eq!(
            (2024, 1, 15),
            (r[0].date.year, r[0].date.month, r[0].date.day)
        );
        close(r[0].tmax, 55.0, "the reading beside it");
    }

    /// A user-prepared record that is merely short reports itself. The
    /// wrong-door message is for files in another layout, and saying it
    /// here would send someone looking for a layout that is not there.
    #[test]
    fn a_short_user_record_reports_itself_not_another_layout() {
        // Five tokens: enough to be recognised as user-prepared, one
        // short of a record.
        let err = parse_climate_file("STA01 2024 1 15 55\n").unwrap_err();
        assert!(err.contains("too few values"), "{err}");
        assert!(!err.contains("layout"), "{err}");
    }

    /// Lines out of order read the same as lines in order. The
    /// predecessor stops its month's read at the first line that looks
    /// later, so an out-of-order file loses everything after it.
    #[test]
    fn lines_out_of_order_read_the_same_as_lines_in_order() {
        let ordered = read("td3200.dat");
        let text = fixture("td3200.dat");
        let reversed_text = {
            let mut l: Vec<&str> = text.lines().collect();
            l.reverse();
            l.join("\n")
        };
        let (reversed, _) =
            parse_any_climate_file(&reversed_text, true, FileTempUnits::Fahrenheit).expect("parse");
        assert_eq!(ordered.len(), reversed.len());
        for (a, b) in ordered.iter().zip(&reversed) {
            assert_eq!(
                (a.date.day, a.tmax, a.tmin, a.evap),
                (b.date.day, b.tmax, b.tmin, b.evap)
            );
        }
    }
}

#[cfg(test)]
mod section_grammar_tests {
    use crate::io::objects::parse_network;
    use crate::io::survey::{Diagnostic, DiagnosticKind};
    use crate::model::{EvaporationSource, Network};

    /// A model just large enough for the climate sections to refer into.
    fn model(sections: &str) -> (Network, Vec<Diagnostic>) {
        parse_network(&format!(
            "\
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
S1  3  0.5  4  7  0

{sections}
"
        ))
    }

    /// Every token a `BadValue` diagnostic named, in order.
    fn bad_tokens(d: &[Diagnostic]) -> Vec<String> {
        d.iter()
            .filter_map(|x| match &x.kind {
                DiagnosticKind::BadValue { token } => Some(token.clone()),
                _ => None,
            })
            .collect()
    }

    fn missing(d: &[Diagnostic]) -> usize {
        d.iter()
            .filter(|x| matches!(x.kind, DiagnosticKind::MissingItems))
            .count()
    }

    // ── Which token a diagnostic names ──────────────────────────────────

    /// A diagnostic names the token that was wrong, not its neighbour.
    ///
    /// Each of these reads its values at an offset from the line's start,
    /// and the offset is repeated in the diagnostic. Nothing asserted
    /// which token came back, so the two could disagree and every test
    /// still passed: the bad value is put *second* in each list, because
    /// that is the position where a wrong offset names a different token.
    #[test]
    fn a_bad_value_is_reported_by_its_own_token() {
        let (_, d) = model("[TEMPERATURE]\nWINDSPEED  MONTHLY  1 x 3 4 5 6 7 8 9 10 11 12");
        assert_eq!(vec!["x"], bad_tokens(&d), "the monthly wind list");

        let (_, d) = model("[TEMPERATURE]\nSNOWMELT  0.5  x  0.6  100  45  -75");
        assert_eq!(vec!["x"], bad_tokens(&d), "the snowmelt list");

        let (_, d) = model("[TEMPERATURE]\nADC  IMPERV  1 x 1 1 1 1 1 1 1 1");
        assert_eq!(vec!["x"], bad_tokens(&d), "the depletion curve");
    }

    /// The substituted-conductivity notice names the month's own value.
    #[test]
    fn a_substituted_conductivity_names_the_value_it_replaced() {
        let (_, d) = model("[ADJUSTMENTS]\nCONDUCTIVITY  1 0 1 1 1 1 1 1 1 1 1 1");
        let requested: Vec<&str> = d
            .iter()
            .filter_map(|x| match &x.kind {
                DiagnosticKind::SubstitutedOption {
                    keyword, requested, ..
                } if *keyword == "CONDUCTIVITY" => Some(requested.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(vec!["0"], requested, "February's zero, not January's one");
    }

    // ── Arity ───────────────────────────────────────────────────────────

    /// A list of twelve months is twelve long, and a short one is
    /// reported rather than read past its end.
    #[test]
    fn a_short_monthly_list_is_missing_items() {
        let (_, d) = model("[TEMPERATURE]\nWINDSPEED  MONTHLY  1 2 3");
        assert_eq!(1, missing(&d), "a list of three is not a year");
        let (_, d) = model("[ADJUSTMENTS]\nRAINFALL  1 1 1 1 1 1 1 1 1 1 1");
        assert_eq!(1, missing(&d), "eleven months is not a year either");
        let (net, d) = model("[ADJUSTMENTS]\nRAINFALL  1 1 1 1 1 1 1 1 1 1 1 2");
        assert_eq!(0, missing(&d), "and twelve is");
        assert!((net.climate.adjust_rainfall[11] - 2.0).abs() < 1e-12);
    }

    /// A parcel pattern needs the parcel and the pattern both.
    #[test]
    fn a_parcel_pattern_needs_both_of_its_names() {
        let (_, d) = model("[ADJUSTMENTS]\nN-PERV  S1");
        assert_eq!(1, missing(&d));
        let (net, d) = model("[ADJUSTMENTS]\nN-PERV  S1  P1");
        assert_eq!(0, missing(&d));
        assert_eq!(Some(0), net.parcels[0].n_perv_pattern);
    }

    /// Pan coefficients are optional, so a bare `FILE` is a whole line
    /// and must not be reported as one short.
    #[test]
    fn evaporation_from_a_file_needs_no_pan_coefficients() {
        let (net, d) = model("[EVAPORATION]\nFILE");
        assert_eq!(0, missing(&d), "a bare FILE is complete");
        let EvaporationSource::File { pan } = net.climate.evaporation else {
            panic!("not file evaporation");
        };
        assert_eq!([1.0; 12], pan, "and its coefficients default to one");

        // Given, they are read.
        let (net, _) = model("[EVAPORATION]\nFILE  0.7 1 1 1 1 1 1 1 1 1 1 1");
        let EvaporationSource::File { pan } = net.climate.evaporation else {
            panic!("not file evaporation");
        };
        assert!((pan[0] - 0.7).abs() < 1e-12);
    }

    // ── Each keyword does its own thing ─────────────────────────────────

    /// Evaporation from daily temperatures is its own source, and an arm
    /// that vanished would leave a model silently evaporating nothing.
    #[test]
    fn evaporation_from_temperature_is_its_own_source() {
        let (net, d) = model("[EVAPORATION]\nTEMPERATURE");
        assert_eq!(0, missing(&d), "TEMPERATURE needs no value");
        assert_eq!(EvaporationSource::Temperature, net.climate.evaporation);
    }

    /// The three parcel patterns are three different fields.
    #[test]
    fn each_parcel_pattern_lands_on_its_own_field() {
        let (net, _) = model("[ADJUSTMENTS]\nN-PERV  S1  P1");
        let p = &net.parcels[0];
        assert_eq!(
            (Some(0), None, None),
            (p.n_perv_pattern, p.dstore_pattern, p.infil_pattern)
        );

        let (net, _) = model("[ADJUSTMENTS]\nDSTORE  S1  P1");
        let p = &net.parcels[0];
        assert_eq!(
            (None, Some(0), None),
            (p.n_perv_pattern, p.dstore_pattern, p.infil_pattern)
        );

        let (net, _) = model("[ADJUSTMENTS]\nINFIL  S1  P1");
        let p = &net.parcels[0];
        assert_eq!(
            (None, None, Some(0)),
            (p.n_perv_pattern, p.dstore_pattern, p.infil_pattern)
        );
    }

    /// The monthly evaporation offset is a rate and converts like one.
    #[test]
    fn the_evaporation_adjustment_converts_to_a_rate() {
        let (net, _) = model("[ADJUSTMENTS]\nEVAPORATION  1 0 0 0 0 0 0 0 0 0 0 0");
        // An inch a day, this model being in US units.
        let inch_per_day = 0.0254 / 86_400.0;
        assert!(
            (net.climate.adjust_evaporation[0] - inch_per_day).abs() < 1e-18,
            "{}",
            net.climate.adjust_evaporation[0]
        );
        assert_eq!(0.0, net.climate.adjust_evaporation[1]);
    }

    /// Each `[TEMPERATURE]` form declares how many tokens it needs, and a
    /// line short of that is reported rather than read past its end.
    #[test]
    fn each_temperature_form_reports_a_line_too_short_for_it() {
        for (line, what) in [
            ("TIMESERIES", "a series with no name"),
            (
                "SNOWMELT  0.5  0.5  0.6  100  45",
                "five of six snowmelt values",
            ),
            (
                "ADC  IMPERV  1 1 1 1 1 1 1 1 1",
                "nine of ten depletion values",
            ),
        ] {
            let (_, d) = model(&format!("[TEMPERATURE]\n{line}"));
            assert_eq!(1, missing(&d), "{what}");
        }
        // And each is complete at its own length.
        for line in [
            "TIMESERIES  TS1",
            "SNOWMELT  0.5  0.5  0.6  100  45  -75",
            "ADC  IMPERV  1 1 1 1 1 1 1 1 1 1",
        ] {
            let (_, d) = model(&format!("[TEMPERATURE]\n{line}"));
            assert_eq!(0, missing(&d), "{line}");
        }
    }

    /// A climate-file declaration takes an optional start date and an
    /// optional units word, in that order, and reads each only when it is
    /// there. Every one of these is a separate decision about how long
    /// the line is.
    #[test]
    fn a_climate_file_declaration_reads_its_optional_tokens() {
        use crate::io::options::Date;
        use crate::model::{FileTempUnits, TemperatureSource};
        let src = |text: &str| {
            let (net, d) = model(&format!("[TEMPERATURE]\n{text}"));
            assert_eq!(0, missing(&d), "{text}");
            net.climate.temperature.clone()
        };
        let file = |start, units| {
            Some(TemperatureSource::File {
                name: "CLIMATE.DAT".into(),
                start,
                units,
            })
        };
        let jan15 = Date {
            year: 2024,
            month: 1,
            day: 15,
        };

        assert_eq!(
            file(None, FileTempUnits::Fahrenheit),
            src("FILE  CLIMATE.DAT"),
            "the name alone, and this model's own default units"
        );
        assert_eq!(
            file(Some(jan15), FileTempUnits::Fahrenheit),
            src("FILE  CLIMATE.DAT  01/15/2024"),
            "a start date when one is given"
        );
        assert_eq!(
            file(Some(jan15), FileTempUnits::Celsius),
            src("FILE  CLIMATE.DAT  01/15/2024  C"),
            "and a units word after it"
        );
        assert_eq!(
            file(None, FileTempUnits::Celsius),
            src("FILE  CLIMATE.DAT  *  C"),
            "a star holds the date's place without setting one"
        );
    }

    // ── Ranges and keyword matching ─────────────────────────────────────

    /// A depletion curve is a fraction: a value outside zero to one is
    /// not a curve, and is reported rather than stored.
    #[test]
    fn a_depletion_fraction_outside_zero_to_one_is_refused() {
        let (_, d) = model("[TEMPERATURE]\nADC  IMPERV  0 0.5 1 1 1 1 1 1 1 2");
        assert_eq!(vec!["2"], bad_tokens(&d), "two is not a fraction");
        let (_, d) = model("[TEMPERATURE]\nADC  IMPERV  0 0.5 1 1 1 1 1 1 1 1");
        assert!(bad_tokens(&d).is_empty(), "the ends of the range are in it");
    }

    /// §14.3: a keyword is matched when the table's entry is a prefix of
    /// the token, so a token that merely *begins* with a keyword is
    /// accepted. That is the predecessor's rule and it is kept, but it
    /// accepts things nobody meant, so every such match is said out loud.
    /// A token spelled exactly is not one of them.
    #[test]
    fn a_keyword_matched_by_prefix_is_reported_and_an_exact_one_is_not() {
        let prefixed = |text: &str| -> Vec<String> {
            let (_, d) = model(text);
            d.iter()
                .filter_map(|x| match &x.kind {
                    DiagnosticKind::PrefixMatched { token, .. } => Some(token.clone()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            vec!["TIMESERIESX"],
            prefixed("[TEMPERATURE]\nTIMESERIESX  TS1"),
            "a token extending a keyword is accepted and noted"
        );
        assert!(
            prefixed("[TEMPERATURE]\nTIMESERIES  TS1").is_empty(),
            "the exact spelling is not a prefix match"
        );
        // And it really was accepted, not merely noted.
        let (net, _) = model("[TEMPERATURE]\nTIMESERIESX  TS1");
        assert!(
            matches!(
                net.climate.temperature,
                Some(crate::model::TemperatureSource::Series(_))
            ),
            "the prefixed keyword still selected the series source"
        );
    }
}
