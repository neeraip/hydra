//! Model export (§14.13): a §2 model written as predecessor input text.
//!
//! The columns written here are the columns [`super::objects`] and its
//! siblings read, so this module defines only what the direction itself
//! decides. Three properties define correctness, in ascending strength
//! (§14.13.2): import-export-import yields an identical model; the
//! second export is byte-identical to the first; and re-importing an
//! exported file raises none of §14.7's mutation warnings.
//!
//! **What is written is the post-validation model.** Import refuses,
//! rewrites and derives, so a model in memory has already been through
//! all three, and export writes the results of those rewrites as plain
//! values. Export is not a round trip through the original file.

use std::fmt::Write as _;

use crate::io::options::{AnalysisOptions, Date, FlowUnits};
use crate::model::Network;

/// ft → m, the same exact factor import uses (§14.13.3 inverts it).
const FT: f64 = 0.3048;

/// Everything export needs to invert a §14.6 conversion, resolved once
/// from the model's own flow-units selection.
pub(crate) struct Units {
    /// True when the file's unit system is US customary.
    us: bool,
    /// m³/s per file flow unit.
    flow: f64,
    /// The weir-coefficient factor import multiplies by (`objects.rs`):
    /// every weir form shares one dimension, so one factor inverts them
    /// all.
    weir_coeff: f64,
}

impl Units {
    fn of(options: &AnalysisOptions) -> Units {
        Units {
            us: options.flow_units.is_us(),
            weir_coeff: if options.flow_units.is_us() {
                0.3048_f64.sqrt()
            } else {
                1.0
            },
            flow: match options.flow_units {
                FlowUnits::Cfs => 0.028_316_846_592,
                FlowUnits::Gpm => 6.309_019_64e-5,
                FlowUnits::Mgd => 0.043_812_636_4,
                FlowUnits::Cms => 1.0,
                FlowUnits::Lps => 1.0e-3,
                FlowUnits::Mld => 1.0 / 86.4,
            },
        }
    }
    /// m → ft | m.
    fn len(&self, v: f64) -> f64 {
        if self.us {
            v / FT
        } else {
            v
        }
    }
    /// m² → ft² | m².
    fn area(&self, v: f64) -> f64 {
        if self.us {
            v / (FT * FT)
        } else {
            v
        }
    }
    /// m³ → ft³ | m³, the volume a Pump1 curve is indexed by.
    fn vol(&self, v: f64) -> f64 {
        if self.us {
            v / (FT * FT * FT)
        } else {
            v
        }
    }
    /// m^½/s → the file's weir-coefficient unit.
    fn weir(&self, v: f64) -> f64 {
        v / self.weir_coeff
    }
    /// m³/s → the file's flow unit.
    fn flow(&self, v: f64) -> f64 {
        v / self.flow
    }
    /// m → in | mm, the rain-depth unit.
    fn depth(&self, v: f64) -> f64 {
        if self.us {
            v / 0.0254
        } else {
            v * 1000.0
        }
    }
    /// m/s → in/h | mm/h.
    fn rate(&self, v: f64) -> f64 {
        self.depth(v) * 3600.0
    }
}

// ── Formatting primitives ───────────────────────────────────────────────

/// A number in shortest round-trip decimal form (§14.13.3).
///
/// Rust's own `f64` display is already the shortest form that re-reads as
/// the same value, which is exactly the contract: fixed precision would
/// serialise a programmatically-set tolerance as zero.
fn num(v: f64) -> String {
    if v == 0.0 {
        // Negative zero re-reads identically but writes as `-0`, which
        // makes a second export differ from the first for no reason.
        return "0".into();
    }
    format!("{v}")
}

/// An identifier, quoted when it holds anything §14.2's lexer would split
/// on, so it survives a cycle whatever it contains.
fn id(s: &str) -> String {
    if s.is_empty() || s.chars().any(|c| c.is_whitespace() || c == '"' || c == ';') {
        format!("\"{}\"", s.replace('"', ""))
    } else {
        s.to_string()
    }
}

/// A duration or clock time as `HH:MM:SS` (§14.13.3): a bare number
/// re-reads as decimal hours and would multiply by 3600 each cycle.
fn hms(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// A calendar date as `MM/DD/YYYY`.
fn date(d: Date) -> String {
    format!("{:02}/{:02}/{:04}", d.month, d.day, d.year)
}

/// A section body under construction: rows of pre-formatted fields, so
/// the section can be dropped whole when it turns out to be empty
/// (§14.13.5) and its columns aligned once at the end.
struct Rows {
    header: &'static str,
    columns: &'static [&'static str],
    rows: Vec<Vec<String>>,
}

impl Rows {
    fn new(header: &'static str, columns: &'static [&'static str]) -> Rows {
        Rows {
            header,
            columns,
            rows: Vec::new(),
        }
    }

    fn push<const N: usize>(&mut self, fields: [String; N]) {
        self.rows.push(fields.to_vec());
    }

    /// Emit the section, or nothing when it has no rows. Columns are
    /// padded to the widest entry so a written file reads like a
    /// hand-authored one; the padding is derived from the content, so a
    /// second export of the same model reproduces it exactly.
    fn write(self, out: &mut String) {
        if self.rows.is_empty() {
            return;
        }
        let n = self.rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut width = vec![0usize; n];
        for (i, w) in width.iter_mut().enumerate() {
            *w = self
                .rows
                .iter()
                .filter_map(|r| r.get(i))
                .map(|f| f.chars().count())
                .chain(self.columns.get(i).map(|c| c.chars().count() + 1))
                .max()
                .unwrap_or(0);
        }
        let _ = writeln!(out, "\n{}", self.header);
        if !self.columns.is_empty() {
            let mut line = String::from(";;");
            for (i, c) in self.columns.iter().enumerate() {
                let pad = width.get(i).copied().unwrap_or(0);
                let _ = write!(line, "{c:<pad$} ");
            }
            let _ = writeln!(out, "{}", line.trim_end());
        }
        for row in &self.rows {
            let mut line = String::new();
            for (i, f) in row.iter().enumerate() {
                let pad = width.get(i).copied().unwrap_or(0);
                let _ = write!(line, "{f:<pad$} ");
            }
            let _ = writeln!(out, "{}", line.trim_end());
        }
    }
}

/// Something in the model with no spelling in the format (§14.13.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRefusal {
    /// The element the offending state belongs to.
    pub element: String,
    /// What about it cannot be written.
    pub reason: String,
}

impl std::fmt::Display for ExportRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.element, self.reason)
    }
}

impl std::error::Error for ExportRefusal {}

/// Write `network` as predecessor input text (§14.13).
///
/// Refuses rather than writing state the grammar has no form for: a file
/// that imports cleanly and means something else is the one outcome
/// worse than failing to write.
pub fn write_inp(network: &Network) -> Result<String, ExportRefusal> {
    check_writable(network)?;
    let u = Units::of(&network.options);
    let mut out = String::new();

    write_title(network, &mut out);
    write_options(network, &u, &mut out);
    write_vertices(network, &u, &mut out);
    write_links(network, &u, &mut out);
    write_transects(network, &u, &mut out);
    write_streets(network, &u, &mut out);
    write_xsections(network, &mut out);
    write_losses(network, &u, &mut out);
    write_curves(network, &u, &mut out);
    write_tables(network, &u, &mut out);
    write_timeseries(network, &mut out);
    write_display(network, &mut out);
    out.trim_start_matches('\n')
        .to_string()
        .clone_into(&mut out);
    Ok(out)
}

/// Refuse the states §14.13.6 names before writing anything, so a
/// refusal never leaves a half-written file behind.
fn check_writable(network: &Network) -> Result<(), ExportRefusal> {
    let bad_id = |s: &str| s.contains('\n') || s.contains('\r');
    for v in &network.vertices {
        if bad_id(&v.id) {
            return Err(ExportRefusal {
                element: v.id.replace(['\n', '\r'], "⏎"),
                reason: "identifier contains a line break, which no quoting survives".into(),
            });
        }
        if !v.invert.is_finite() {
            return Err(ExportRefusal {
                element: v.id.clone(),
                reason: "invert elevation is not a finite number".into(),
            });
        }
    }
    for l in &network.links {
        if bad_id(&l.id) {
            return Err(ExportRefusal {
                element: l.id.replace(['\n', '\r'], "⏎"),
                reason: "identifier contains a line break, which no quoting survives".into(),
            });
        }
    }
    Ok(())
}

// ── Title and options ───────────────────────────────────────────────────

fn write_title(network: &Network, out: &mut String) {
    if network.title.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n[TITLE]");
    for line in &network.title {
        let _ = writeln!(out, "{line}");
    }
}

fn write_options(network: &Network, u: &Units, out: &mut String) {
    let o = &network.options;
    let d = AnalysisOptions::default();
    let mut rows = Rows::new("[OPTIONS]", &["Option", "Value"]);
    let mut put = |k: &str, v: String| rows.push([k.to_string(), v]);

    put("FLOW_UNITS", format!("{:?}", o.flow_units).to_uppercase());
    put(
        "INFILTRATION",
        match o.infiltration {
            crate::io::options::InfiltrationModel::Horton => "HORTON",
            crate::io::options::InfiltrationModel::ModifiedHorton => "MODIFIED_HORTON",
            crate::io::options::InfiltrationModel::GreenAmpt => "GREEN_AMPT",
            crate::io::options::InfiltrationModel::ModifiedGreenAmpt => "MODIFIED_GREEN_AMPT",
            crate::io::options::InfiltrationModel::CurveNumber => "CURVE_NUMBER",
        }
        .to_string(),
    );
    // §14.4 substitutes every reduced form onto the one solver, and the
    // model records what was asked for; export writes that request back
    // so a cycle does not silently promote the file to dynamic wave.
    put(
        "FLOW_ROUTING",
        match o.routing_request {
            crate::io::options::RoutingRequest::Steady => "STEADY",
            crate::io::options::RoutingRequest::KinematicWave => "KINWAVE",
            crate::io::options::RoutingRequest::DynamicWave => "DYNWAVE",
        }
        .to_string(),
    );
    put("START_DATE", date(o.start_date));
    put("START_TIME", hms(o.start_time));
    put("END_DATE", date(o.end_date));
    put("END_TIME", hms(o.end_time));
    if let Some((rd, rt)) = o.report_start {
        put("REPORT_START_DATE", date(rd));
        put("REPORT_START_TIME", hms(rt));
    }
    if o.sweep_start != d.sweep_start || o.sweep_end != d.sweep_end {
        put(
            "SWEEP_START",
            format!("{:02}/{:02}", o.sweep_start / 100, o.sweep_start % 100),
        );
        put(
            "SWEEP_END",
            format!("{:02}/{:02}", o.sweep_end / 100, o.sweep_end % 100),
        );
    }
    if o.dry_days != d.dry_days {
        put("DRY_DAYS", num(o.dry_days));
    }
    put("WET_STEP", hms(o.wet_step));
    put("DRY_STEP", hms(o.dry_step));
    put("REPORT_STEP", hms(o.report_step));
    // Written always: the routing step's own default is derived from the
    // wet step by §14.4's interlocks, so omitting it makes the re-read
    // value depend on a neighbour (§14.13.4).
    put("ROUTING_STEP", num(o.routing_step));
    if o.rule_step > 0.0 {
        put("RULE_STEP", hms(o.rule_step));
    }
    put("ALLOW_PONDING", yes_no(o.allow_ponding));
    put(
        "LINK_OFFSETS",
        match o.link_offsets {
            crate::io::options::LinkOffsets::Depth => "DEPTH",
            crate::io::options::LinkOffsets::Elevation => "ELEVATION",
        }
        .to_string(),
    );
    if o.min_slope != d.min_slope {
        put("MIN_SLOPE", num(o.min_slope * 100.0));
    }
    put("VARIABLE_STEP", num(o.courant_factor));
    if o.min_routing_step != d.min_routing_step {
        put("MINIMUM_STEP", num(o.min_routing_step));
    }
    if o.max_trials != d.max_trials {
        put("MAX_TRIALS", o.max_trials.to_string());
    }
    if o.head_tol != d.head_tol {
        put("HEAD_TOLERANCE", num(u.len(o.head_tol)));
    }
    if o.min_surface_area != d.min_surface_area {
        put("MIN_SURFAREA", num(u.area(o.min_surface_area)));
    }
    if o.threads != d.threads {
        put("THREADS", o.threads.to_string());
    }
    for (flag, word) in [
        (o.ignore_rainfall, "IGNORE_RAINFALL"),
        (o.ignore_snowmelt, "IGNORE_SNOWMELT"),
        (o.ignore_groundwater, "IGNORE_GROUNDWATER"),
        (o.ignore_rdii, "IGNORE_RDII"),
        (o.ignore_routing, "IGNORE_ROUTING"),
        (o.ignore_quality, "IGNORE_QUALITY"),
    ] {
        if flag {
            put(word, "YES".into());
        }
    }
    if let Some(dir) = &o.temp_dir {
        put("TEMPDIR", id(dir));
    }
    rows.write(out);
}

fn yes_no(b: bool) -> String {
    if b { "YES" } else { "NO" }.to_string()
}

// ── Vertices ────────────────────────────────────────────────────────────

fn write_vertices(network: &Network, u: &Units, out: &mut String) {
    use crate::model::VertexKind as K;
    let mut junctions = Rows::new(
        "[JUNCTIONS]",
        &[
            "Name",
            "Elevation",
            "MaxDepth",
            "InitDepth",
            "SurDepth",
            "Aponded",
        ],
    );
    let mut outfalls = Rows::new(
        "[OUTFALLS]",
        &["Name", "Elevation", "Type", "StageData", "Gated", "RouteTo"],
    );
    let mut storage = Rows::new(
        "[STORAGE]",
        &[
            "Name",
            "Elev",
            "MaxDepth",
            "InitDepth",
            "Shape",
            "CurveName/Params",
            "",
            "",
            "SurDepth",
            "Fevap",
            "Psi",
            "Ksat",
            "IMD",
        ],
    );
    let mut dividers = Rows::new(
        "[DIVIDERS]",
        &[
            "Name",
            "Elevation",
            "DivLink",
            "Type",
            "Parameters",
            "",
            "",
            "MaxDepth",
            "InitDepth",
            "SurDepth",
            "Aponded",
        ],
    );

    for v in &network.vertices {
        let e = num(u.len(v.invert));
        match &v.kind {
            K::Junction {
                max_depth,
                init_depth,
                surcharge_depth,
                ponded_area,
            } => junctions.push([
                id(&v.id),
                e,
                num(u.len(*max_depth)),
                num(u.len(*init_depth)),
                num(u.len(*surcharge_depth)),
                num(u.area(*ponded_area)),
            ]),
            K::Outfall {
                stage,
                flap_gate,
                route_to_parcel,
            } => {
                let (kind, data) = outfall_stage(stage, network, u);
                outfalls.push([
                    id(&v.id),
                    e,
                    kind,
                    data,
                    yes_no(*flap_gate),
                    route_to_parcel.map_or(String::new(), |p| id(&network.parcels[p].id)),
                ]);
            }
            K::Storage {
                max_depth,
                init_depth,
                geometry,
                surcharge_depth,
                evap_fraction,
                seepage,
            } => {
                let (shape, a, b, c) = storage_shape(geometry, network, u);
                let (psi, ksat, imd) = match seepage {
                    Some(s) => (
                        num(u.depth(s.suction)),
                        num(u.rate(s.conductivity)),
                        num(s.initial_deficit),
                    ),
                    None => (String::new(), String::new(), String::new()),
                };
                storage.push([
                    id(&v.id),
                    e,
                    num(u.len(*max_depth)),
                    num(u.len(*init_depth)),
                    shape,
                    a,
                    b,
                    c,
                    num(u.len(*surcharge_depth)),
                    num(*evap_fraction),
                    psi,
                    ksat,
                    imd,
                ]);
            }
            K::Divider {
                diverted_link,
                rule,
                max_depth,
                init_depth,
                surcharge_depth,
                ponded_area,
            } => {
                let (kind, p1, p2, p3) = divider_rule(rule, network, u);
                dividers.push([
                    id(&v.id),
                    e,
                    diverted_link.map_or("*".into(), |l| id(&network.links[l].id)),
                    kind,
                    p1,
                    p2,
                    p3,
                    num(u.len(*max_depth)),
                    num(u.len(*init_depth)),
                    num(u.len(*surcharge_depth)),
                    num(u.area(*ponded_area)),
                ]);
            }
        }
    }
    junctions.write(out);
    outfalls.write(out);
    storage.write(out);
    dividers.write(out);
}

fn outfall_stage(
    stage: &crate::model::OutfallStage,
    network: &Network,
    u: &Units,
) -> (String, String) {
    use crate::model::OutfallStage as S;
    match stage {
        S::Free => ("FREE".into(), String::new()),
        S::Normal => ("NORMAL".into(), String::new()),
        S::Fixed(elevation) => ("FIXED".into(), num(u.len(*elevation))),
        S::Tidal { curve } => ("TIDAL".into(), id(&network.curves[*curve].id)),
        S::Series { series } => ("TIMESERIES".into(), id(&network.timeseries[*series].id)),
    }
}

/// The storage geometry's file form.
///
/// The analytical shapes compile at import to $A = a_0 + a_1 y + a_2
/// y^2$, keeping only the shape's name, so the author's axes and side
/// slope are no longer in the model. They do not need to be: behaviour
/// depends on $A(y)$ alone, and re-import recompiles whatever is
/// written. Export therefore solves for **a** parameter set reproducing
/// the stored coefficients rather than *the* one the author wrote.
///
/// For the pyramid that set is unique up to swapping the two axes. For
/// the cone it is not — its coefficients satisfy $a_1^2 = 4a_0a_2$
/// identically, leaving two independent equations in three unknowns, so
/// an infinite family of cones shares one area relation. Export picks
/// the symmetric member: the circular cone. Every member is the same
/// storage unit as far as the engine is concerned, and the round trip is
/// exact because the coefficients recompile unchanged (§14.13.2).
fn storage_shape(
    geometry: &crate::model::StorageGeometry,
    network: &Network,
    u: &Units,
) -> (String, String, String, String) {
    use crate::model::StorageGeometry as G;
    use crate::model::StorageShapeKind as K;
    use std::f64::consts::PI;
    match geometry {
        G::Tabular { curve } => (
            "TABULAR".into(),
            id(&network.curves[*curve].id),
            String::new(),
            String::new(),
        ),
        G::Functional {
            coeff,
            exponent,
            constant,
        } => (
            "FUNCTIONAL".into(),
            num(functional_area_coeff(*coeff, *exponent, u)),
            num(*exponent),
            num(u.area(*constant)),
        ),
        G::Shape { kind, a0, a1, a2 } => {
            let (word, la, wb, third) = match kind {
                // A = pi·a·b with a = b: one circle, no slope.
                K::Cylindrical => {
                    let a = (a0 / PI).max(0.0).sqrt();
                    ("CYLINDRICAL", 2.0 * a, 2.0 * a, 0.0)
                }
                // The circular member of the family sharing this A(y).
                K::Conical => {
                    let a = (a0 / PI).max(0.0).sqrt();
                    let z = if *a0 > 0.0 { a1 * a / (2.0 * a0) } else { 0.0 };
                    ("CONICAL", 2.0 * a, 2.0 * a, z)
                }
                // A = pi·a·b·y/h, so the height is free; taking it as one
                // metre fixes the axes and keeps the written value finite
                // whatever the coefficient's magnitude.
                K::Paraboloid => {
                    let a = (a1 / PI).max(0.0).sqrt();
                    ("PARABOLIC", 2.0 * a, 2.0 * a, 1.0)
                }
                // Uniquely invertible: the side slope comes from a2, the
                // axes are the roots of t^2 - (la+wb)t + la·wb.
                K::Pyramidal => {
                    let z = a2.max(0.0).sqrt() / 2.0;
                    let (la, wb) = if z > 0.0 {
                        let sum = a1 / (2.0 * z);
                        // Non-negative for any real pyramid, by AM-GM on
                        // the two axes; clamped against rounding alone.
                        let root = (sum * sum - 4.0 * a0).max(0.0).sqrt();
                        (0.5 * (sum + root), 0.5 * (sum - root))
                    } else {
                        let side = a0.max(0.0).sqrt();
                        (side, side)
                    };
                    ("PYRAMIDAL", la, wb, z)
                }
            };
            // The third column is a length for the paraboloid and a
            // dimensionless side slope for the other two.
            let third = if matches!(kind, K::Paraboloid) {
                u.len(third)
            } else {
                third
            };
            (word.into(), num(u.len(la)), num(u.len(wb)), num(third))
        }
    }
}

/// The §14.6 inverse for a storage area relation $A = c + a\,y^{b}$: the
/// coefficient carries an area over a length raised to the exponent.
fn functional_area_coeff(coeff: f64, exponent: f64, u: &Units) -> f64 {
    if u.us {
        u.area(coeff) * FT.powf(exponent)
    } else {
        coeff
    }
}

fn divider_rule(
    rule: &crate::model::DividerRule,
    network: &Network,
    u: &Units,
) -> (String, String, String, String) {
    use crate::model::DividerRule as R;
    match rule {
        R::Overflow => (
            "OVERFLOW".into(),
            String::new(),
            String::new(),
            String::new(),
        ),
        R::Cutoff { min_flow } => (
            "CUTOFF".into(),
            num(u.flow(*min_flow)),
            String::new(),
            String::new(),
        ),
        R::Tabular { curve } => (
            "TABULAR".into(),
            id(&network.curves[*curve].id),
            String::new(),
            String::new(),
        ),
        R::Weir {
            min_flow,
            max_depth,
            coeff,
        } => (
            "WEIR".into(),
            num(u.flow(*min_flow)),
            num(u.len(*max_depth)),
            num(*coeff),
        ),
    }
}

// ── Links ───────────────────────────────────────────────────────────────

fn write_links(network: &Network, u: &Units, out: &mut String) {
    use crate::model::LinkKind as K;
    let mut conduits = Rows::new(
        "[CONDUITS]",
        &[
            "Name",
            "FromNode",
            "ToNode",
            "Length",
            "Roughness",
            "InOffset",
            "OutOffset",
            "InitFlow",
            "MaxFlow",
        ],
    );
    let mut pumps = Rows::new(
        "[PUMPS]",
        &[
            "Name",
            "FromNode",
            "ToNode",
            "PumpCurve",
            "Status",
            "Startup",
            "Shutoff",
        ],
    );
    let mut orifices = Rows::new(
        "[ORIFICES]",
        &[
            "Name",
            "FromNode",
            "ToNode",
            "Type",
            "Offset",
            "Cd",
            "Gated",
            "CloseTime",
        ],
    );
    let mut weirs = Rows::new(
        "[WEIRS]",
        &[
            "Name",
            "FromNode",
            "ToNode",
            "Type",
            "CrestHt",
            "Cd",
            "Gated",
            "EndCon",
            "EndCoeff",
            "Surcharge",
            "RoadWidth",
            "RoadSurf",
            "CoeffCurve",
        ],
    );
    let mut outlets = Rows::new(
        "[OUTLETS]",
        &[
            "Name",
            "FromNode",
            "ToNode",
            "Offset",
            "Type",
            "QCoeff/QTable",
            "QExpon",
            "Gated",
        ],
    );

    let vid = |i: usize| id(&network.vertices[i].id);
    for l in &network.links {
        match &l.kind {
            K::Channel {
                length,
                roughness,
                offset1,
                offset2,
                init_flow,
                max_flow,
                reversed,
                ..
            } => conduits.push([
                id(&l.id),
                // §14.13.1: a reversed adverse-slope channel is written
                // in the user's orientation, ends and offsets together.
                vid(if *reversed { l.to } else { l.from }),
                vid(if *reversed { l.from } else { l.to }),
                num(u.len(*length)),
                num(*roughness),
                offset(if *reversed { *offset2 } else { *offset1 }, u),
                offset(if *reversed { *offset1 } else { *offset2 }, u),
                num(u.flow(*init_flow)),
                num(u.flow(*max_flow)),
            ]),
            K::Pump {
                curve,
                initial_on,
                startup_depth,
                shutoff_depth,
                ..
            } => pumps.push([
                id(&l.id),
                vid(l.from),
                vid(l.to),
                curve.map_or("*".into(), |c| id(&network.curves[c].id)),
                if *initial_on { "ON" } else { "OFF" }.to_string(),
                num(u.len(*startup_depth)),
                num(u.len(*shutoff_depth)),
            ]),
            K::Orifice {
                orientation,
                offset: off,
                discharge_coeff,
                flap_gate,
                open_close_time,
            } => orifices.push([
                id(&l.id),
                vid(l.from),
                vid(l.to),
                match orientation {
                    crate::model::OrificeOrientation::Bottom => "BOTTOM",
                    crate::model::OrificeOrientation::Side => "SIDE",
                }
                .to_string(),
                offset(*off, u),
                num(*discharge_coeff),
                yes_no(*flap_gate),
                num(*open_close_time / 3600.0),
            ]),
            K::Weir {
                form,
                offset: off,
                discharge_coeff,
                flap_gate,
                end_contractions,
                end_coeff,
                can_surcharge,
                road_width,
                road_surface,
                coeff_curve,
            } => weirs.push([
                id(&l.id),
                vid(l.from),
                vid(l.to),
                weir_form(*form),
                offset(*off, u),
                num(u.weir(*discharge_coeff)),
                yes_no(*flap_gate),
                num(*end_contractions),
                num(u.weir(*end_coeff)),
                yes_no(*can_surcharge),
                num(u.len(*road_width)),
                match road_surface {
                    crate::model::RoadSurface::Paved => "PAVED",
                    crate::model::RoadSurface::Gravel => "GRAVEL",
                    crate::model::RoadSurface::Unspecified => "",
                }
                .to_string(),
                coeff_curve.map_or(String::new(), |c| id(&network.curves[c].id)),
            ]),
            K::Outlet {
                offset: off,
                rating,
                head_basis,
                flap_gate,
            } => {
                let (kind, a, b) = outlet_rating(rating, *head_basis, network);
                outlets.push([
                    id(&l.id),
                    vid(l.from),
                    vid(l.to),
                    offset(*off, u),
                    kind,
                    a,
                    b,
                    yes_no(*flap_gate),
                ]);
            }
        }
    }
    conduits.write(out);
    pumps.write(out);
    orifices.write(out);
    weirs.write(out);
    outlets.write(out);
}

/// An offset in the convention the model declares (§14.13.1).
fn offset(o: crate::model::Offset, u: &Units) -> String {
    use crate::model::Offset as O;
    match o {
        O::Depth(v) | O::Elevation(v) => num(u.len(v)),
        O::Missing => "*".into(),
    }
}

fn weir_form(shape: crate::model::WeirForm) -> String {
    use crate::model::WeirForm as W;
    match shape {
        W::Transverse => "TRANSVERSE",
        W::SideFlow => "SIDEFLOW",
        W::VNotch => "V-NOTCH",
        W::Trapezoidal => "TRAPEZOIDAL",
        W::Roadway => "ROADWAY",
    }
    .to_string()
}

fn outlet_rating(
    rating: &crate::model::OutletRating,
    basis: crate::model::OutletHeadBasis,
    network: &Network,
) -> (String, String, String) {
    use crate::model::OutletHeadBasis as B;
    use crate::model::OutletRating as R;
    let arg = match basis {
        B::Depth => "DEPTH",
        B::Head => "HEAD",
    };
    match rating {
        // Stored as written: import applies no conversion here, so
        // neither does export (§14.13.3 inverts exactly, including by
        // doing nothing).
        R::Functional { coeff, exponent } => {
            (format!("FUNCTIONAL/{arg}"), num(*coeff), num(*exponent))
        }
        R::Tabular { curve } => (
            format!("TABULAR/{arg}"),
            id(&network.curves[*curve].id),
            String::new(),
        ),
    }
}

// ── Curves, series and patterns ─────────────────────────────────────────

/// `[TRANSECTS]`, whose survey the model holds with the file's station
/// multiplier and elevation offset already applied.
///
/// Those two are written back as identity rather than recovered: the
/// stations they scaled are what the model has, and a multiplier is a
/// convenience for entering a survey, not a property of one. Writing
/// them as 1 and 0 against the scaled stations reproduces the same
/// survey, which is §14.13.2's contract.
///
/// The section is line-oriented rather than columnar — an `NC` roughness
/// line, an `X1` header, then `GR` elevation-station pairs — so it is
/// written directly instead of through the column builder.
fn write_transects(network: &Network, u: &Units, out: &mut String) {
    if network.transects.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n[TRANSECTS]");
    for t in &network.transects {
        let _ = writeln!(
            out,
            "NC {} {} {}",
            num(t.n_left),
            num(t.n_right),
            num(t.n_channel)
        );
        // Station count, bank stations, two unused columns, the meander
        // factor, then the identity multiplier and offset.
        let _ = writeln!(
            out,
            "X1 {} {} {} {} 0 0 {} 1 0",
            id(&t.id),
            t.stations.len(),
            num(u.len(t.x_left)),
            num(u.len(t.x_right)),
            num(t.meander_factor)
        );
        // Elevation first, then station — the predecessor's order, and
        // the order the model stores the pair in.
        for chunk in t.stations.chunks(3) {
            let mut line = String::from("GR");
            for (elev, station) in chunk {
                let _ = write!(line, " {} {}", num(u.len(*elev)), num(u.len(*station)));
            }
            let _ = writeln!(out, "{line}");
        }
    }
}

/// `[STREETS]`, whose two slopes the model holds as fractions and the
/// file carries as percentages.
fn write_streets(network: &Network, u: &Units, out: &mut String) {
    let mut rows = Rows::new(
        "[STREETS]",
        &[
            "Name", "Tcrown", "Hcurb", "Sx", "nRoad", "a", "W", "Sides", "Tback", "Sback", "nBack",
        ],
    );
    for st in &network.streets {
        rows.push([
            id(&st.id),
            num(u.len(st.crown_width)),
            num(u.len(st.curb_height)),
            num(st.cross_slope * 100.0),
            num(st.roughness),
            num(u.len(st.gutter_depression)),
            num(u.len(st.gutter_width)),
            st.sides.to_string(),
            num(u.len(st.backing_width)),
            num(st.backing_slope * 100.0),
            num(st.backing_roughness),
        ]);
    }
    rows.write(out);
}

/// `[LOSSES]`, which carries a conduit's local-loss coefficients, its
/// flap gate and its bed seepage — properties of a channel that live in
/// their own section rather than beside its geometry.
///
/// A conduit with none of them is omitted: every field defaults to zero
/// or off, so a row of zeroes says exactly what silence says (§14.13.4).
fn write_losses(network: &Network, u: &Units, out: &mut String) {
    use crate::model::LinkKind as K;
    let mut rows = Rows::new(
        "[LOSSES]",
        &["Link", "Kentry", "Kexit", "Kavg", "FlapGate", "Seepage"],
    );
    for l in &network.links {
        let K::Channel {
            loss_inlet,
            loss_outlet,
            loss_avg,
            flap_gate,
            seepage_rate,
            ..
        } = &l.kind
        else {
            continue;
        };
        if *loss_inlet == 0.0
            && *loss_outlet == 0.0
            && *loss_avg == 0.0
            && !*flap_gate
            && *seepage_rate == 0.0
        {
            continue;
        }
        rows.push([
            id(&l.id),
            num(*loss_inlet),
            num(*loss_outlet),
            num(*loss_avg),
            yes_no(*flap_gate),
            num(u.rate(*seepage_rate)),
        ]);
    }
    rows.write(out);
}

/// A link kind's position in the export's section order, so anything
/// listing links across kinds can match it.
fn link_kind_rank(kind: &crate::model::LinkKind) -> u8 {
    use crate::model::LinkKind as K;
    match kind {
        K::Channel { .. } => 0,
        K::Pump { .. } => 1,
        K::Orifice { .. } => 2,
        K::Weir { .. } => 3,
        K::Outlet { .. } => 4,
    }
}

/// `[XSECTIONS]`, whose four geometry values the model keeps in the
/// file's own units (§2.7), so they are written back unconverted.
fn write_xsections(network: &Network, out: &mut String) {
    use crate::model::{XsectReferent as R, XsectShape};
    let mut rows = Rows::new(
        "[XSECTIONS]",
        &[
            "Link", "Shape", "Geom1", "Geom2", "Geom3", "Geom4", "Barrels", "Culvert",
        ],
    );
    // Ordered by the kind grouping the link sections use, not by
    // registration order. Export groups links by kind (§14.13.5), so an
    // xsection list following registration order would come out in one
    // order on the first export and another on the second — the model is
    // the same either way, but idempotence is not, and a file that
    // differs from its own re-export hides which of the two is canonical.
    let mut ordered: Vec<&crate::model::Link> = network.links.iter().collect();
    ordered.sort_by_key(|l| link_kind_rank(&l.kind));
    for l in ordered {
        let Some(x) = &l.cross_section else { continue };
        // A referent-carrying shape names its referent in the first
        // geometry column, where import reads it.
        let geom1 = match (&x.referent, x.shape) {
            (Some(R::Transect(t)), _) => id(&network.transects[*t].id),
            (Some(R::Street(s)), _) => id(&network.streets[*s].id),
            (Some(R::Curve(c)), _) => id(&network.curves[*c].id),
            (None, XsectShape::Irregular | XsectShape::Street | XsectShape::Custom) => {
                String::new()
            }
            (None, _) => num(x.geom_user[0]),
        };
        rows.push([
            id(&l.id),
            crate::io::objects::xsect_word(x.shape).to_string(),
            geom1,
            num(x.geom_user[1]),
            num(x.geom_user[2]),
            num(x.geom_user[3]),
            x.barrels.to_string(),
            if x.culvert_code == 0 {
                String::new()
            } else {
                x.culvert_code.to_string()
            },
        ]);
    }
    rows.write(out);
}

/// `[TIMESERIES]`, whose values the model keeps in the consumer's own
/// units as written (§2.5), so they too are written back unconverted.
fn write_timeseries(network: &Network, out: &mut String) {
    use crate::model::{SeriesTime, TimeSeriesSource};
    let mut rows = Rows::new("[TIMESERIES]", &["Name", "Date", "Time", "Value"]);
    for s in &network.timeseries {
        match &s.source {
            TimeSeriesSource::External { file } => {
                rows.push([id(&s.id), "FILE".into(), id(file), String::new()]);
            }
            TimeSeriesSource::Points(points) => {
                // A date anchors every later time until the next date,
                // so one is written only where it changes — repeating it
                // would re-read identically but is not what the reader's
                // own writer emits.
                let mut current: Option<Date> = None;
                for p in points {
                    let (d, t) = match p.time {
                        SeriesTime::Elapsed(sec) => (None, hms(sec)),
                        SeriesTime::Absolute { date: dt, seconds } => (Some(dt), hms(seconds)),
                    };
                    let stamp = match d {
                        Some(dt) if current != Some(dt) => {
                            current = Some(dt);
                            date(dt)
                        }
                        _ => String::new(),
                    };
                    rows.push([id(&s.id), stamp, t, num(p.value)]);
                }
            }
        }
    }
    rows.write(out);
}

/// `[CURVES]`, each role's points inverted by the conversion
/// `io::tables` applied to them — the roles convert differently, so the
/// inverse is chosen per role rather than per column.
fn write_curves(network: &Network, u: &Units, out: &mut String) {
    use crate::model::CurveKind as C;
    let mut rows = Rows::new("[CURVES]", &["Name", "Type", "X-Value", "Y-Value"]);
    for c in &network.curves {
        let word = match c.kind {
            C::Storage => "STORAGE",
            C::Diversion => "DIVERSION",
            C::Tidal => "TIDAL",
            C::Rating => "RATING",
            C::Control => "CONTROL",
            C::Shape => "SHAPE",
            C::WeirCoeff => "WEIR",
            C::Pump1 => "PUMP1",
            C::Pump2 => "PUMP2",
            C::Pump3 => "PUMP3",
            C::Pump4 => "PUMP4",
            C::Pump5 => "PUMP5",
        };
        for (i, (x, y)) in c.points.iter().enumerate() {
            let (fx, fy) = match c.kind {
                C::Storage => (u.len(*x), u.area(*y)),
                C::Diversion => (u.flow(*x), u.flow(*y)),
                // A tidal curve's abscissa is an hour of the day, held
                // in seconds.
                C::Tidal => (*x / 3600.0, u.len(*y)),
                C::Rating => (u.len(*x), u.flow(*y)),
                // Control and shape curves are dimensionless, or in the
                // units their consumer reads them in; either way import
                // left them alone.
                C::Control | C::Shape => (*x, *y),
                C::WeirCoeff => (u.len(*x), u.weir(*y)),
                C::Pump1 => (u.vol(*x), u.flow(*y)),
                C::Pump2 | C::Pump3 | C::Pump4 | C::Pump5 => (u.len(*x), u.flow(*y)),
            };
            // The type word rides the first line only, as the reader's
            // own writer emits it; repeating it re-reads identically but
            // is not the form the ecosystem produces.
            rows.push([
                id(&c.id),
                if i == 0 {
                    word.to_string()
                } else {
                    String::new()
                },
                num(fx),
                num(fy),
            ]);
        }
    }
    rows.write(out);
}

fn write_tables(network: &Network, u: &Units, out: &mut String) {
    let _ = u;
    let mut patterns = Rows::new("[PATTERNS]", &["Name", "Type", "Multipliers"]);
    for p in &network.patterns {
        let kind = match p.kind {
            crate::model::PatternKind::Monthly => "MONTHLY",
            crate::model::PatternKind::Daily => "DAILY",
            crate::model::PatternKind::Hourly => "HOURLY",
            crate::model::PatternKind::Weekend => "WEEKEND",
        };
        // The predecessor's reader accumulates a pattern's factors over
        // however many lines carry them; one line per pattern is the
        // form its own writer emits and re-reads identically.
        let mut row = vec![id(&p.id), kind.to_string()];
        row.extend(p.factors.iter().map(|f| num(*f)));
        patterns.rows.push(row);
    }
    patterns.write(out);
}

// ── Display metadata ────────────────────────────────────────────────────

/// The nine display-metadata sections, written from what import
/// preserved (§14.5): verbatim, in their original order, neither
/// validated nor normalised.
fn write_display(network: &Network, out: &mut String) {
    for section in &network.display {
        if section.lines.is_empty() {
            continue;
        }
        let _ = writeln!(out, "\n{}", section.header);
        for line in &section.lines {
            let _ = writeln!(out, "{line}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{StorageGeometry, StorageShapeKind};
    use std::f64::consts::PI;

    /// Recompile written parameters the way import does (`objects.rs`),
    /// so the assertion below tests the inversion against the compilation
    /// rather than against itself.
    fn recompile(kind: StorageShapeKind, la: f64, wb: f64, third: f64) -> (f64, f64, f64) {
        let (a, b) = (la / 2.0, wb / 2.0);
        let z = third;
        match kind {
            StorageShapeKind::Cylindrical => (PI * a * b, 0.0, 0.0),
            StorageShapeKind::Conical => (PI * a * b, 2.0 * PI * b * z, PI * b / a * z * z),
            StorageShapeKind::Paraboloid => (0.0, PI * a * b / third, 0.0),
            StorageShapeKind::Pyramidal => (la * wb, 2.0 * (la + wb) * z, 4.0 * z * z),
        }
    }

    /// §14.13.2's semantic round trip for the shapes whose parameters
    /// import discards. The written numbers need not be the author's —
    /// for a cone they cannot be, since its coefficients satisfy
    /// `a1² = 4·a0·a2` identically and leave one degree of freedom — but
    /// they must recompile to the very same area relation, because that
    /// relation is the whole of what the engine solves.
    #[test]
    fn analytical_storage_shapes_recompile_to_their_own_area_relation() {
        let u = Units {
            us: false,
            flow: 1.0,
            weir_coeff: 1.0,
        };
        // Authored parameters, chosen asymmetric so a symmetric answer
        // cannot pass by luck.
        let cases = [
            (StorageShapeKind::Cylindrical, 30.0, 12.0, 0.0),
            (StorageShapeKind::Conical, 30.0, 12.0, 2.5),
            (StorageShapeKind::Paraboloid, 30.0, 12.0, 4.0),
            (StorageShapeKind::Pyramidal, 30.0, 12.0, 2.5),
            // Degenerate side slopes: a cone and a pyramid with vertical
            // walls, where the axes are underdetermined.
            (StorageShapeKind::Conical, 30.0, 12.0, 0.0),
            (StorageShapeKind::Pyramidal, 30.0, 12.0, 0.0),
        ];
        for (kind, la, wb, third) in cases {
            let (a0, a1, a2) = recompile(kind, la, wb, third);
            let geom = StorageGeometry::Shape { kind, a0, a1, a2 };
            let network = Network::default();
            let (word, fa, fb, fc) = storage_shape(&geom, &network, &u);
            let (pa, pb, pc): (f64, f64, f64) = (
                fa.parse().expect("major"),
                fb.parse().expect("minor"),
                fc.parse().expect("third"),
            );
            // Import refuses non-positive axes, so a written shape that
            // could not be re-read is a defect however well it inverts.
            assert!(pa > 0.0 && pb > 0.0, "{word} wrote a non-positive axis");
            assert!(pc >= 0.0, "{word} wrote a negative third parameter");
            let (b0, b1, b2) = recompile(kind, pa, pb, pc);
            let close = |x: f64, y: f64| (x - y).abs() <= 1e-9 * x.abs().max(1.0);
            assert!(
                close(a0, b0) && close(a1, b1) && close(a2, b2),
                "{word} {la}x{wb}x{third}: ({a0}, {a1}, {a2}) recompiled as ({b0}, {b1}, {b2})"
            );
        }
    }

    /// Whether two vertices agree to floating-point rounding.
    fn vertex_agrees(x: &crate::model::Vertex, y: &crate::model::Vertex) -> bool {
        use crate::model::{StorageGeometry as G, VertexKind as K};
        if x.invert != y.invert {
            return false;
        }
        let close = |p: f64, q: f64| (p - q).abs() <= 1e-12 * p.abs().max(1.0);
        match (&x.kind, &y.kind) {
            (
                K::Storage {
                    geometry:
                        G::Shape {
                            kind: ka,
                            a0,
                            a1,
                            a2,
                        },
                    max_depth: da,
                    init_depth: ia,
                    ..
                },
                K::Storage {
                    geometry:
                        G::Shape {
                            kind: kb,
                            a0: b0,
                            a1: b1,
                            a2: b2,
                        },
                    max_depth: db,
                    init_depth: ib,
                    ..
                },
            ) => {
                ka == kb
                    && da == db
                    && ia == ib
                    && close(*a0, *b0)
                    && close(*a1, *b1)
                    && close(*a2, *b2)
            }
            _ => x.kind == y.kind,
        }
    }

    /// §14.13.2's first property on a model exercising every node kind,
    /// every link kind and a cross-section: import, export, import again,
    /// and the two models must agree.
    ///
    /// The comparison is on the *models*, never on the two files: export
    /// is not a round trip through the original text (§14.13.1), so a
    /// text diff would fail on models that are correctly identical.
    #[test]
    fn a_model_survives_export_and_re_import() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CFS
START_DATE    06/01/2024
END_DATE      06/01/2024
END_TIME      04:00
ROUTING_STEP  10
REPORT_STEP   0:15:00

[JUNCTIONS]
J1  100.4  3  0.5  1  200
J2  100.2  3  0    0  0

[OUTFALLS]
O1  100.0  FREE  YES

[STORAGE]
S1  99.0  10  0  CONICAL  30  12  2.5
S2  98.0  8   0  TABULAR  SC1

[WEIRS]
W1  S1  S2  TRANSVERSE  0.5  3.33  NO  2  1.5

[CONDUITS]
C1  J1  J2  200  0.013  0.1  0.2  0.5  9
C2  J2  S1  150  0.015  0    0    0    0
C3  S2  O1  120  0.014  0    0    0    0
C4  J1  J2  90   0.02   0    0    0    0

[PUMPS]
P1  S1  S2  PC1  OFF  1.2  0.4

[LOSSES]
C1  0.5  0.8  0.1  YES  0.25

[TRANSECTS]
NC 0.03 0.04 0.02
X1 T1 5 12 28 0 0 1.4 2 1
GR 10 0  8 8  6 16  8 24  10 32

[STREETS]
ST1  20  0.5  2  0.016  2  4  1  10  4  0.02

[XSECTIONS]
C1  CIRCULAR   1.5  0  0  0  2
C2  RECT_OPEN  2    3  0  0  1
C3  CIRCULAR   1    0  0  0  1
W1  RECT_OPEN  2    4  0  0  1
C4  IRREGULAR  T1

[CURVES]
SC1  STORAGE  0  100  2  400  6  900
PC1  PUMP3    0  8    5  6    10 2
RC1  RATING   0  0    1  3.5  2  9
";
        let (a, da) = crate::io::objects::parse_network(inp);
        assert!(da.iter().all(|d| !d.kind.is_error()), "{da:?}");
        let text = write_inp(&a).expect("export");
        let (b, db) = crate::io::objects::parse_network(&text);
        assert!(
            db.iter().all(|d| !d.kind.is_error()),
            "re-import failed: {db:?}\n--- written ---\n{text}"
        );

        // Compared with a rounding tolerance, not for equality: a
        // canonical shape inversion (§14.13.5) passes through a square
        // root, so a cone's coefficients return agreeing to the last
        // ULP rather than bit-identically. Every other quantity is a
        // pure multiply and does come back exact — which is why the
        // tolerance is relative and tight rather than generous.
        assert_eq!(a.vertices.len(), b.vertices.len(), "vertex count\n{text}");
        for (x, y) in a.vertices.iter().zip(&b.vertices) {
            assert_eq!(x.id, y.id, "vertex identity\n{text}");
            assert!(
                vertex_agrees(x, y),
                "vertex {} differs\n  {:?}\n  {:?}\n{text}",
                x.id,
                x.kind,
                y.kind
            );
        }
        // Matched by identity, not by position. Export groups objects by
        // kind (§14.13.5) while a file may interleave them, so a model
        // whose author wrote weirs above conduits comes back with the
        // same links registered in a different order. The predecessor's
        // own interface does the same, and every reference between
        // objects resolves by identifier — but registration order is
        // what the results file orders elements by, so this is a
        // difference worth naming rather than hiding behind a sort.
        assert_eq!(a.links.len(), b.links.len(), "link count\n{text}");
        for x in &a.links {
            let y = b
                .links
                .iter()
                .find(|y| y.id == x.id)
                .unwrap_or_else(|| panic!("link {} missing after export\n{text}", x.id));
            assert_eq!(
                a.vertices[x.from].id, b.vertices[y.from].id,
                "link {} upstream end\n{text}",
                x.id
            );
            assert_eq!(
                a.vertices[x.to].id, b.vertices[y.to].id,
                "link {} downstream end\n{text}",
                x.id
            );
            assert_eq!(x.cross_section, y.cross_section, "link {}\n{text}", x.id);
            // Kinds carry curve indices, which the reordering also
            // shifts, so they are compared with those resolved to names.
            assert_eq!(
                format!("{:?}", x.kind).replace(char::is_numeric, ""),
                format!("{:?}", y.kind).replace(char::is_numeric, ""),
                "link {} kind\n{text}",
                x.id
            );
        }
        // Curves carry a different conversion per role, so they are
        // compared as a body rather than trusted to the link and node
        // comparisons that merely reference them.
        assert_eq!(a.transects, b.transects, "transects\n{text}");
        assert_eq!(a.streets, b.streets, "streets\n{text}");
        assert_eq!(a.curves.len(), b.curves.len(), "curve count\n{text}");
        for x in &a.curves {
            let y = b
                .curves
                .iter()
                .find(|y| y.id == x.id)
                .unwrap_or_else(|| panic!("curve {} missing\n{text}", x.id));
            assert_eq!(x.kind, y.kind, "curve {} role\n{text}", x.id);
            assert_eq!(x.points, y.points, "curve {} points\n{text}", x.id);
        }
        assert_eq!(a.options.flow_units, b.options.flow_units);
        assert_eq!(a.options.routing_step, b.options.routing_step);
        assert_eq!(a.options.report_step, b.options.report_step);

        // §14.13.2's second property: a second export is byte-identical,
        // so no value survives one cycle and drifts on the next.
        let again = write_inp(&b).expect("second export");
        assert_eq!(text, again, "second export differs");
    }

    /// The one shape whose parameters survive: a pyramid's are unique up
    /// to swapping its axes, so export recovers what the author wrote.
    #[test]
    fn a_pyramids_own_axes_come_back() {
        let u = Units {
            us: false,
            flow: 1.0,
            weir_coeff: 1.0,
        };
        let (a0, a1, a2) = recompile(StorageShapeKind::Pyramidal, 30.0, 12.0, 2.5);
        let geom = StorageGeometry::Shape {
            kind: StorageShapeKind::Pyramidal,
            a0,
            a1,
            a2,
        };
        let (_, fa, fb, fc) = storage_shape(&geom, &Network::default(), &u);
        let (pa, pb, pc): (f64, f64, f64) = (
            fa.parse().unwrap(),
            fb.parse().unwrap(),
            fc.parse().unwrap(),
        );
        assert!((pa - 30.0).abs() < 1e-9, "major {pa}");
        assert!((pb - 12.0).abs() < 1e-9, "minor {pb}");
        assert!((pc - 2.5).abs() < 1e-9, "slope {pc}");
    }
}
