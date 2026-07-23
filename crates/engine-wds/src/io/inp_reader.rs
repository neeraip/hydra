// inp — EPANET 2.3 INP file parser (spec.md §4.4).
//
// Two-pass strategy (spec.md §4.2):
//   Pass 1: Split file into sections, collecting raw line buffers.
//   Pass 2: Process sections in dependency order — nodes first, then links,
//           then controls/rules that reference both.

use std::collections::{HashMap, HashSet};

use super::{units::make_ucf, ParseError};
use crate::{
    ActionValue, Curve, CurveKind, CurvePoint, DemandCategory, DemandModel, FlowUnits,
    HeadLossFormula, Junction, Link, LinkBase, LinkKind, LinkStatus, LogicOp, MixModel, Network,
    Node, NodeBase, NodeKind, Pattern, Pipe, Premise, PremiseAttribute, PremiseObject,
    PremiseOperator, Pump, PumpCurveType, QualityMode, QualitySource, ReportFieldOption,
    ReportOptions, ReportSelection, ReportStatus, Reservoir, Rule, RuleAction, SimpleControl,
    SimulationOptions, SourceType, StatisticType, Tank, TriggerType, Valve, ValveType, WallOrder,
};

/// EPANET shutoff head factor for single-point pump curve expansion.
/// A 1-point curve (Q1, H1) expands to: (0, FACTOR*H1), (Q1, H1), (2*Q1, 0).
const PUMP_SHUTOFF_HEAD_FACTOR: f64 = 1.33334;

/// Return type for `parse_tags`: `(node_tags, link_tags)` maps.
type TagMaps = (HashMap<String, String>, HashMap<String, String>);

/// A section data line paired with its 1-based line number in the source file.
type SecLine<'a> = (usize, &'a str);

// ═══════════════════════════════════════════════════════════════════════════════
// Pass 1 — split file into named sections
// ═══════════════════════════════════════════════════════════════════════════════

/// Collect section name → Vec of `(line_number, data_line)` pairs (comments
/// and blanks stripped).  Line numbers are 1-based positions in the input
/// text, retained so parse errors can point at the offending line.
fn split_sections(text: &str) -> HashMap<String, Vec<SecLine<'_>>> {
    let mut sections: HashMap<String, Vec<SecLine<'_>>> = HashMap::new();
    let mut current: Option<String> = None;

    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if let Some(end) = trimmed.find(']') {
                let name = trimmed[1..end].to_ascii_uppercase();
                if name == "END" {
                    break;
                }
                current = Some(name.clone());
                sections.entry(name).or_default();
                continue;
            }
        }
        if let Some(ref sec) = current {
            // Skip comment lines and blank lines.
            if trimmed.is_empty() || trimmed.starts_with(';') {
                continue;
            }
            // TITLE section: preserve raw lines (EPANET copies the full
            // line including any `;` characters as literal title text).
            if sec == "TITLE" {
                sections
                    .entry(sec.clone())
                    .or_default()
                    .push((line_no, trimmed));
                continue;
            }
            // Strip trailing comments (after `;`).
            let data = if let Some(pos) = trimmed.find(';') {
                trimmed[..pos].trim()
            } else {
                trimmed
            };
            if !data.is_empty() {
                sections
                    .entry(sec.clone())
                    .or_default()
                    .push((line_no, data));
            }
        }
    }
    sections
}

/// Iterate a section's data lines, attaching the section name and the line's
/// 1-based source line number to any error produced by `f`.
fn for_each_line<F>(
    lines: &[SecLine<'_>],
    section: &'static str,
    mut f: F,
) -> Result<(), ParseError>
where
    F: FnMut(&str) -> Result<(), ParseError>,
{
    for &(line_no, line) in lines {
        f(line).map_err(|e| e.at_line(section, line_no))?;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Pass 2 — process sections in dependency order
// ═══════════════════════════════════════════════════════════════════════════════

/// Parse a raw EPANET INP file into a validated [`Network`].
///
/// This is the low-level byte-slice entry point; callers in `hydra-cli` and
/// `hydra-gui` typically use the higher-level [`crate::io::parse`] wrapper
/// which handles format detection.
pub fn parse_inp(bytes: &[u8]) -> Result<Network, ParseError> {
    let text = String::from_utf8_lossy(bytes);

    let sections = split_sections(&text);

    // ── 0. Title lines (up to 3, preserving original text) ───────────────────
    let title: Vec<String> = sections
        .get("TITLE")
        .map(|v| v.iter().take(3).map(|&(_, s)| s.to_string()).collect())
        .unwrap_or_default();

    // ── 1. Patterns (no dependencies) ─────────────────────────────────────────
    let mut patterns = parse_patterns(
        sections
            .get("PATTERNS")
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
    )?;

    // ── 2. Curves (no dependencies) ───────────────────────────────────────────
    let mut curves = parse_curves(sections.get("CURVES").map(|v| v.as_slice()).unwrap_or(&[]))?;

    // ── 3. Options (no dependencies) ──────────────────────────────────────────
    let mut options = parse_options(sections.get("OPTIONS").map(|v| v.as_slice()).unwrap_or(&[]))?;

    // Reset qual_step and rule_timestep to sentinel 0.0 so that
    // adjust_timesteps() can default them to hyd_step/10 if the INP
    // doesn't explicitly set them via [TIMES].  (The struct default
    // is non-zero for direct API users, but during parsing EPANET
    // treats 0 as "not set, compute from hyd_step".)
    options.qual_step = 0.0;
    options.rule_timestep = 0.0;

    // ── 3a. TIMES section ─────────────────────────────────────────────────────
    apply_times(
        &mut options,
        sections.get("TIMES").map(|v| v.as_slice()).unwrap_or(&[]),
    )?;

    // ── 3b. REACTIONS section ─────────────────────────────────────────────────
    apply_reactions(
        &mut options,
        sections
            .get("REACTIONS")
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
    )?;

    // ── 3c. ENERGY section ────────────────────────────────────────────────────
    apply_energy(
        &mut options,
        sections.get("ENERGY").map(|v| v.as_slice()).unwrap_or(&[]),
    )?;

    // ── 4. Nodes (depend on nothing except node_id_to_idx building) ───────────
    let mut nodes: Vec<Node> = Vec::new();
    let mut node_id_to_idx: HashMap<String, usize> = HashMap::new();

    parse_junctions(
        sections
            .get("JUNCTIONS")
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
        &mut nodes,
        &mut node_id_to_idx,
    )?;
    parse_reservoirs(
        sections
            .get("RESERVOIRS")
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
        &mut nodes,
        &mut node_id_to_idx,
    )?;
    parse_tanks(
        sections.get("TANKS").map(|v| v.as_slice()).unwrap_or(&[]),
        &mut nodes,
        &mut node_id_to_idx,
    )?;

    // ── 4a. Additional demands ────────────────────────────────────────────────
    apply_demands(
        sections.get("DEMANDS").map(|v| v.as_slice()).unwrap_or(&[]),
        &mut nodes,
        &node_id_to_idx,
    )?;

    // ── 4b. Emitters ──────────────────────────────────────────────────────────
    apply_emitters(
        sections
            .get("EMITTERS")
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
        &mut nodes,
        &node_id_to_idx,
    )?;

    // ── 4c. Initial quality ───────────────────────────────────────────────────
    apply_quality(
        sections.get("QUALITY").map(|v| v.as_slice()).unwrap_or(&[]),
        &mut nodes,
        &node_id_to_idx,
    )?;

    // ── 4d. Mixing ────────────────────────────────────────────────────────────
    apply_mixing(
        sections.get("MIXING").map(|v| v.as_slice()).unwrap_or(&[]),
        &mut nodes,
        &node_id_to_idx,
    )?;

    // ── 4e. Sources ───────────────────────────────────────────────────────────
    apply_sources(
        sections.get("SOURCES").map(|v| v.as_slice()).unwrap_or(&[]),
        &mut nodes,
        &node_id_to_idx,
    )?;

    // ── 5. Links (depend on node_id_to_idx) ───────────────────────────────────
    let mut links: Vec<Link> = Vec::new();
    let mut link_id_to_idx: HashMap<String, usize> = HashMap::new();

    parse_pipes(
        sections.get("PIPES").map(|v| v.as_slice()).unwrap_or(&[]),
        &mut links,
        &mut link_id_to_idx,
        &node_id_to_idx,
    )?;
    parse_pumps(
        sections.get("PUMPS").map(|v| v.as_slice()).unwrap_or(&[]),
        &mut links,
        &mut link_id_to_idx,
        &node_id_to_idx,
    )?;
    parse_valves(
        sections.get("VALVES").map(|v| v.as_slice()).unwrap_or(&[]),
        &mut links,
        &mut link_id_to_idx,
        &node_id_to_idx,
    )?;

    // ── 5a. Status overrides ──────────────────────────────────────────────────
    apply_status(
        sections.get("STATUS").map(|v| v.as_slice()).unwrap_or(&[]),
        &mut links,
        &link_id_to_idx,
    )?;

    // ── 5b. Leakage coefficients ──────────────────────────────────────────────
    apply_leakage(
        sections.get("LEAKAGE").map(|v| v.as_slice()).unwrap_or(&[]),
        &mut links,
        &link_id_to_idx,
    )?;

    // ── 6. Controls (depend on nodes and links) ───────────────────────────────
    let mut controls = parse_controls(
        sections
            .get("CONTROLS")
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
        &node_id_to_idx,
        &link_id_to_idx,
    )?;

    // ── 7. Rules (depend on nodes and links) ──────────────────────────────────
    let mut rules = parse_rules(
        sections.get("RULES").map(|v| v.as_slice()).unwrap_or(&[]),
        &node_id_to_idx,
        &link_id_to_idx,
    )?;

    // ── Post-processing: per-pump ENERGY settings (PUMP <id> EFFIC/PRICE/PATTERN) ──
    // These lines reference pump IDs that only exist after link parsing.
    apply_pump_energy(
        sections.get("ENERGY").map(|v| v.as_slice()).unwrap_or(&[]),
        &mut links,
        &link_id_to_idx,
        &mut curves,
    )?;

    // ── Post-processing: expand single-point pump head curves ────────────────
    // EPANET allows a 1-point pump curve (Q1, H1) and internally expands to
    // three points: (0, 1.33334·H1), (Q1, H1), (2·Q1, 0).  Our validation
    // requires ≥ 2 points, so do the expansion here.
    let pump_head_curve_ids: HashSet<String> = links
        .iter()
        .filter_map(|l| match &l.kind {
            LinkKind::Pump(p) => p.head_curve.clone(),
            _ => None,
        })
        .collect();
    let pump_effic_curve_ids: HashSet<String> = links
        .iter()
        .filter_map(|l| match &l.kind {
            LinkKind::Pump(p) => p.efficiency_curve.clone(),
            _ => None,
        })
        .collect();
    for c in &mut curves {
        if pump_head_curve_ids.contains(&c.id) && c.points.len() == 1 {
            let q1 = c.points[0].x;
            let h1 = c.points[0].y;
            c.points = vec![
                CurvePoint {
                    x: 0.0,
                    y: PUMP_SHUTOFF_HEAD_FACTOR * h1,
                },
                CurvePoint { x: q1, y: h1 },
                CurvePoint {
                    x: 2.0 * q1,
                    y: 0.0,
                },
            ];
        }
        // Tag curves referenced by pumps as PumpHead kind.
        if pump_head_curve_ids.contains(&c.id) {
            c.kind = CurveKind::PumpHead;
        }
        // Tag curves referenced as efficiency curves.
        if pump_effic_curve_ids.contains(&c.id) {
            c.kind = CurveKind::PumpEfficiency;
        }
    }

    // ── Post-processing: tag valve and tank curves ─────────────────────────────
    // Curve IDs are unique, so a one-time id → index map avoids a linear scan
    // of `curves` for every link/node that references a curve.
    let curve_index: HashMap<String, usize> = curves
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), i))
        .collect();
    for link in &links {
        if let LinkKind::Valve(v) = &link.kind {
            if let Some(ref curve_id) = v.curve {
                let target_kind = match v.valve_type {
                    ValveType::Gpv => Some(CurveKind::GpvHeadloss),
                    ValveType::Pcv => Some(CurveKind::PcvLossRatio),
                    _ => None,
                };
                if let Some(kind) = target_kind {
                    if let Some(&ci) = curve_index.get(curve_id) {
                        curves[ci].kind = kind;
                    }
                }
            }
        }
    }
    for node in &nodes {
        if let NodeKind::Tank(t) = &node.kind {
            if let Some(ref curve_id) = t.volume_curve {
                if let Some(&ci) = curve_index.get(curve_id) {
                    curves[ci].kind = CurveKind::TankVolume;
                }
            }
        }
    }

    // ── Post-processing: reclassify pump curve types ─────────────────────────
    // EPANET only uses POWER_FUNC for: 1-point curves (already expanded to 3)
    // or 3-point curves with X[0]==0. Everything else is CUSTOM (piecewise
    // linear interpolation). The initial classification set PowerFunction for
    // all pumps with a head curve; correct it here using the actual curve data.
    for link in &mut links {
        if let LinkKind::Pump(pump) = &mut link.kind {
            if pump.curve_type == PumpCurveType::PowerFunction {
                if let Some(ref curve_id) = pump.head_curve {
                    if let Some(&ci) = curve_index.get(curve_id) {
                        let curve = &curves[ci];
                        let npts = curve.points.len();
                        let is_power = npts == 3 && curve.points[0].x == 0.0;
                        if !is_power {
                            pump.curve_type = PumpCurveType::Custom;
                        }
                    }
                }
            }
        }
    }

    // ── Post-processing: implicit default pattern ────────────────────────────
    // EPANET defaults to pattern "1" when no PATTERN option is specified
    // (DEFPATID = "1" in input1.c).  Apply the same default.
    if options.default_pattern.is_none() {
        options.default_pattern = Some("1".to_string());
    }
    // EPANET treats the PATTERN option as a reference to an existing pattern ID.
    // Many INP files set `PATTERN 1` without explicitly defining pattern "1";
    // EPANET implicitly creates an all-1.0 pattern.  We do the same.
    if let Some(ref pat_id) = options.default_pattern {
        let exists = patterns.iter().any(|p| p.id == *pat_id);
        if !exists {
            patterns.push(Pattern {
                id: pat_id.clone(),
                factors: vec![1.0],
            });
        }
    }

    // ── Global emitter exponent (from OPTIONS) ────────────────────────────────
    // EPANET stores a single global exponent; Hydra stores per-junction.
    // Apply the global value to all junctions before unit conversion.
    {
        let option_lines = sections.get("OPTIONS").map(|v| v.as_slice()).unwrap_or(&[]);
        let mut global_emit_exp: f64 = 0.5; // default
        for &(_, line) in option_lines {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 3
                && fields[0].eq_ignore_ascii_case("EMITTER")
                && fields[1].eq_ignore_ascii_case("EXPONENT")
            {
                if let Ok(v) = fields[2].parse::<f64>() {
                    if v > 0.0 {
                        global_emit_exp = v;
                    }
                }
            }
        }
        for node in &mut nodes {
            if let NodeKind::Junction(ref mut j) = node.kind {
                j.emitter_exp = global_emit_exp;
            }
        }
    }

    // ── Post-processing: per-element reaction coefficients ───────────────────
    apply_per_element_reactions(
        sections
            .get("REACTIONS")
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
        &mut nodes,
        &node_id_to_idx,
        &mut links,
        &link_id_to_idx,
    )?;

    // ── Unit conversion (spec.md §3): convert all values from user units
    // to internal representation (CFS, ft) ────────────────────────────────────
    super::units::apply_unit_conversion(
        &mut options,
        &mut nodes,
        &mut links,
        &mut curves,
        &mut controls,
        &mut rules,
    );

    // ── Timestep adjustment (EPANET adjustdata equivalent) ───────────────────
    // These caps mirror EPANET's adjustdata() function in input1.c.
    adjust_timesteps(&mut options);

    // ── 8. Report options ─────────────────────────────────────────────────────
    let report = parse_report(sections.get("REPORT").map(|v| v.as_slice()).unwrap_or(&[]))?;

    // ── 9. Coordinates (visual metadata, no unit conversion) ──────────────────
    let coordinates = parse_coordinates(
        sections
            .get("COORDINATES")
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
        &node_id_to_idx,
    )?;

    // ── 10. Vertices (visual metadata) ────────────────────────────────────────
    let vertices = parse_vertices(
        sections
            .get("VERTICES")
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
        &link_id_to_idx,
    )?;

    // ── 11. Tags (metadata) ──────────────────────────────────────────────────
    let (node_tags, link_tags) = parse_tags(
        sections.get("TAGS").map(|v| v.as_slice()).unwrap_or(&[]),
        &node_id_to_idx,
        &link_id_to_idx,
    )?;

    let mut network = Network {
        title,
        options,
        patterns,
        curves,
        nodes,
        links,
        controls,
        rules,
        pattern_index: std::collections::HashMap::new(),
        report,
        coordinates,
        vertices,
        node_tags,
        link_tags,
    };
    network.build_pattern_index();

    network.validate().map_err(ParseError::ValidationFailed)?;
    Ok(network)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Section parsers
// ═══════════════════════════════════════════════════════════════════════════════

// ── Patterns ──────────────────────────────────────────────────────────────────

fn parse_patterns(lines: &[SecLine<'_>]) -> Result<Vec<Pattern>, ParseError> {
    // INP patterns: continuation lines with the same ID are concatenated.
    let mut map: HashMap<String, Vec<f64>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for_each_line(lines, "PATTERNS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Ok(());
        }
        let id = fields[0].to_string();
        if !map.contains_key(&id) {
            order.push(id.clone());
            map.insert(id.clone(), Vec::new());
        }
        for &f in &fields[1..] {
            let v = parse_f64(f, "pattern multiplier")?;
            map.entry(id.clone()).or_default().push(v);
        }
        Ok(())
    })?;

    order
        .into_iter()
        .map(|id| {
            let factors = map.remove(&id).unwrap_or_default();
            Ok(Pattern { id, factors })
        })
        .collect()
}

// ── Curves ────────────────────────────────────────────────────────────────────

fn parse_curves(lines: &[SecLine<'_>]) -> Result<Vec<Curve>, ParseError> {
    // Curves: continuation lines with the same ID add more points.
    // Curve type is inferred later based on usage context.
    let mut map: HashMap<String, Vec<CurvePoint>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for_each_line(lines, "CURVES", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            return Ok(());
        }
        let id = fields[0].to_string();
        if !map.contains_key(&id) {
            order.push(id.clone());
            map.insert(id.clone(), Vec::new());
        }
        let x = parse_f64(fields[1], "curve x")?;
        let y = parse_f64(fields[2], "curve y")?;
        map.entry(id).or_default().push(CurvePoint { x, y });
        Ok(())
    })?;

    order
        .into_iter()
        .map(|id| {
            let points = map.remove(&id).unwrap_or_default();
            // Default kind; will be re-assigned when pumps/tanks/GPVs reference it.
            Ok(Curve {
                id,
                kind: CurveKind::Generic,
                points,
            })
        })
        .collect()
}

// ── Options ───────────────────────────────────────────────────────────────────

fn parse_options(lines: &[SecLine<'_>]) -> Result<SimulationOptions, ParseError> {
    let mut opts = SimulationOptions::default();
    // Track whether HTOL/QTOL were explicitly set so we can convert them
    // from user units to internal (SI) units. Default values are
    // already in internal units and must not be converted.
    let mut htol: Option<f64> = None;
    let mut qtol: Option<f64> = None;

    for_each_line(lines, "OPTIONS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.is_empty() {
            return Ok(());
        }
        let key = fields[0].to_ascii_uppercase();
        match key.as_str() {
            "UNITS" => {
                if let Some(&val) = fields.get(1) {
                    opts.flow_units = parse_flow_units(val)?;
                }
            }
            "HEADLOSS" => {
                if let Some(&val) = fields.get(1) {
                    opts.head_loss_formula = match val.to_ascii_uppercase().as_str() {
                        "H-W" => HeadLossFormula::HazenWilliams,
                        "D-W" => HeadLossFormula::DarcyWeisbach,
                        "C-M" => HeadLossFormula::ChezyManning,
                        _ => {
                            return Err(ParseError::InvalidField {
                                field: "OPTIONS.Headloss".into(),
                                reason: format!("unknown formula '{val}'"),
                            });
                        }
                    };
                }
            }
            "VISCOSITY" => {
                opts.viscosity = opt_f64(&fields, 1, "OPTIONS.Viscosity")?;
            }
            "DIFFUSIVITY" => {
                opts.diffusivity = opt_f64(&fields, 1, "OPTIONS.Diffusivity")?;
            }
            "SPECIFIC" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "GRAVITY") =>
            {
                opts.specific_gravity = opt_f64(&fields, 2, "OPTIONS.Specific Gravity")?;
            }
            "TRIALS" => {
                // Spec §2.1: max_iter ≥ 1.  A bare `as u32` cast would turn
                // 0/negative/NaN into 0, silently disabling the solver loop.
                let v = opt_f64(&fields, 1, "OPTIONS.Trials")?;
                if v.is_nan() || v < 1.0 {
                    return Err(ParseError::InvalidField {
                        field: "OPTIONS.Trials".into(),
                        reason: format!("must be >= 1, got '{v}'"),
                    });
                }
                opts.max_iter = v as u32;
            }
            "ACCURACY" => {
                opts.flow_tol = opt_f64(&fields, 1, "OPTIONS.Accuracy")?;
            }
            "UNBALANCED" => {
                // "Continue N" where N is extra iterations.
                if let Some(&val) = fields.get(1) {
                    match val.to_ascii_uppercase().as_str() {
                        "STOP" => {
                            opts.extra_iter = -1;
                        }
                        "CONTINUE" => {
                            opts.extra_iter = fields
                                .get(2)
                                .and_then(|s| s.parse::<i32>().ok())
                                .unwrap_or(0);
                        }
                        _ => {}
                    }
                }
            }
            "PATTERN" => {
                if let Some(&val) = fields.get(1) {
                    opts.default_pattern = Some(val.to_string());
                }
            }
            "DEMAND" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "MULTIPLIER") =>
            {
                opts.demand_multiplier = opt_f64(&fields, 2, "OPTIONS.Demand Multiplier")?;
            }
            "DEMAND" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "MODEL") => {
                if let Some(&val) = fields.get(2) {
                    opts.demand_model = match val.to_ascii_uppercase().as_str() {
                        "DDA" => DemandModel::DemandDriven,
                        "PDA" => DemandModel::PressureDriven,
                        _ => {
                            return Err(ParseError::InvalidField {
                                field: "OPTIONS.Demand Model".into(),
                                reason: format!("unknown demand model '{val}'"),
                            });
                        }
                    };
                }
            }
            "MINIMUM" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "PRESSURE") =>
            {
                opts.pda_min_pressure = opt_f64(&fields, 2, "OPTIONS.Minimum Pressure")?;
            }
            "REQUIRED" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "PRESSURE") =>
            {
                opts.pda_required_pressure = opt_f64(&fields, 2, "OPTIONS.Required Pressure")?;
            }
            "PRESSURE" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "EXPONENT") =>
            {
                opts.pda_pressure_exponent = opt_f64(&fields, 2, "OPTIONS.Pressure Exponent")?;
            }
            "EMITTER" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "EXPONENT") =>
            {
                // Parsed separately in parse_inp; skip here.
            }
            "QUALITY" => {
                // "Quality Chemical mg/L" or "Quality Age" or "Quality Trace <node>"
                if let Some(&val) = fields.get(1) {
                    match val.to_ascii_uppercase().as_str() {
                        "NONE" | "NO" => {
                            opts.quality_mode = QualityMode::None;
                        }
                        "AGE" => {
                            opts.quality_mode = QualityMode::Age;
                        }
                        "TRACE" => {
                            opts.quality_mode = QualityMode::Trace;
                            opts.trace_node = fields.get(2).map(|s| s.to_string());
                        }
                        _ => {
                            // Named chemical: "Chlorine mg/L" → Chemical mode.
                            opts.quality_mode = QualityMode::Chemical;
                            opts.chem_name = val.to_string();
                            if let Some(&u) = fields.get(2) {
                                opts.chem_units = u.to_string();
                            }
                        }
                    }
                }
            }
            "TOLERANCE" => {
                opts.quality_tolerance = opt_f64(&fields, 1, "OPTIONS.Tolerance")?;
            }
            "CHECKFREQ" => {
                // Spec §2.1: check_freq ≥ 1 (see TRIALS note above).
                let v = opt_f64(&fields, 1, "OPTIONS.CHECKFREQ")?;
                if v.is_nan() || v < 1.0 {
                    return Err(ParseError::InvalidField {
                        field: "OPTIONS.CHECKFREQ".into(),
                        reason: format!("must be >= 1, got '{v}'"),
                    });
                }
                opts.check_freq = v as u32;
            }
            "MAXCHECK" => {
                // Spec §2.1: max_check ≥ check_freq ≥ 1.
                let v = opt_f64(&fields, 1, "OPTIONS.MAXCHECK")?;
                if v.is_nan() || v < 1.0 {
                    return Err(ParseError::InvalidField {
                        field: "OPTIONS.MAXCHECK".into(),
                        reason: format!("must be >= 1, got '{v}'"),
                    });
                }
                opts.max_check = v as u32;
            }
            "DAMPLIMIT" => {
                opts.damp_limit = opt_f64(&fields, 1, "OPTIONS.DAMPLIMIT")?;
            }
            "FLOWCHANGE" => {
                opts.flow_change_limit = opt_f64(&fields, 1, "OPTIONS.FLOWCHANGE")?;
            }
            "HEADERROR" => {
                opts.head_error_limit = opt_f64(&fields, 1, "OPTIONS.HEADERROR")?;
            }
            "HTOL" => {
                htol = Some(opt_f64(&fields, 1, "OPTIONS.HTOL")?);
            }
            "QTOL" => {
                qtol = Some(opt_f64(&fields, 1, "OPTIONS.QTOL")?);
            }
            "RQTOL" => {
                opts.rq_tol = opt_f64(&fields, 1, "OPTIONS.RQTOL")?;
            }
            "BACKFLOW" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "ALLOWED") => {
                if let Some(&val) = fields.get(2) {
                    match val.to_ascii_uppercase().as_str() {
                        "YES" => opts.emitter_backflow = true,
                        "NO" => opts.emitter_backflow = false,
                        _ => {
                            return Err(ParseError::InvalidField {
                                field: "OPTIONS.BACKFLOW ALLOWED".into(),
                                reason: format!("expected YES or NO, got '{val}'"),
                            });
                        }
                    }
                }
            }
            _ => {
                // Unknown option — ignore silently for forward compat
                // (deliberate leniency; see spec.md §4.3 DEVIATION note).
            }
        }
        Ok(())
    })?;

    // Convert user-specified HTOL/QTOL from user units to internal (SI)
    // units. Default values are already in internal units and are not touched
    // here.
    {
        let ucf = make_ucf(opts.flow_units, opts.specific_gravity);
        if let Some(v) = htol {
            opts.head_tol = v / ucf.elev;
        }
        if let Some(v) = qtol {
            opts.flow_change_tol = v / ucf.flow;
        }
    }

    Ok(opts)
}

// ── Times ─────────────────────────────────────────────────────────────────────

fn apply_times(opts: &mut SimulationOptions, lines: &[SecLine<'_>]) -> Result<(), ParseError> {
    for_each_line(lines, "TIMES", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.is_empty() {
            return Ok(());
        }
        let key = fields[0].to_ascii_uppercase();
        match key.as_str() {
            "DURATION" => {
                opts.duration = parse_time_value(&fields[1..], "TIMES.Duration")?;
            }
            "HYDRAULIC" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "TIMESTEP") =>
            {
                opts.hyd_step = parse_time_value(&fields[2..], "TIMES.Hydraulic Timestep")?;
            }
            "QUALITY" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "TIMESTEP") =>
            {
                opts.qual_step = parse_time_value(&fields[2..], "TIMES.Quality Timestep")?;
            }
            "PATTERN" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "TIMESTEP") =>
            {
                opts.pattern_step = parse_time_value(&fields[2..], "TIMES.Pattern Timestep")?;
            }
            "PATTERN" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "START") =>
            {
                opts.pattern_start = parse_time_value(&fields[2..], "TIMES.Pattern Start")?;
            }
            "REPORT" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "TIMESTEP") =>
            {
                opts.report_step = parse_time_value(&fields[2..], "TIMES.Report Timestep")?;
            }
            "REPORT" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "START") =>
            {
                opts.report_start = parse_time_value(&fields[2..], "TIMES.Report Start")?;
            }
            "START" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "CLOCKTIME") =>
            {
                opts.start_clocktime = parse_clocktime(&fields[2..]);
            }
            "STATISTIC" | "STATISTICS" => {
                if let Some(mode) = fields.get(1) {
                    opts.statistic = match mode.to_ascii_uppercase().as_str() {
                        "NONE" => StatisticType::Series,
                        "AVERAGE" | "AVG" => StatisticType::Average,
                        "MINIMUM" | "MIN" => StatisticType::Minimum,
                        "MAXIMUM" | "MAX" => StatisticType::Maximum,
                        "RANGE" => StatisticType::Range,
                        _ => StatisticType::Series,
                    };
                }
            }
            "RULE" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "TIMESTEP") =>
            {
                opts.rule_timestep = parse_time_value(&fields[2..], "TIMES.Rule Timestep")?;
            }
            _ => {}
        }
        Ok(())
    })
}

// ── Reactions ─────────────────────────────────────────────────────────────────

fn apply_reactions(opts: &mut SimulationOptions, lines: &[SecLine<'_>]) -> Result<(), ParseError> {
    for_each_line(lines, "REACTIONS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Ok(());
        }
        let key = fields[0].to_ascii_uppercase();
        match key.as_str() {
            "ORDER" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "BULK") =>
            {
                opts.bulk_order = opt_f64(&fields, 2, "REACTIONS.Order Bulk")?;
            }
            "ORDER" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "WALL") =>
            {
                let v = opt_f64(&fields, 2, "REACTIONS.Order Wall")?;
                opts.wall_order = if v == 0.0 {
                    WallOrder::Zero
                } else {
                    WallOrder::One
                };
            }
            "ORDER" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "TANK") =>
            {
                opts.tank_order = opt_f64(&fields, 2, "REACTIONS.Order Tank")?;
            }
            "GLOBAL" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "BULK") =>
            {
                opts.bulk_coeff = opt_f64(&fields, 2, "REACTIONS.Global Bulk")?;
            }
            "GLOBAL" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "WALL") =>
            {
                opts.wall_coeff = opt_f64(&fields, 2, "REACTIONS.Global Wall")?;
            }
            "LIMITING" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "POTENTIAL") =>
            {
                opts.conc_limit = opt_f64(&fields, 2, "REACTIONS.Limiting Potential")?;
            }
            "ROUGHNESS" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "CORRELATION") =>
            {
                opts.roughness_reaction_factor =
                    opt_f64(&fields, 2, "REACTIONS.Roughness Correlation")?;
            }
            "BULK" | "WALL" | "TANK" => {
                // Per-pipe/per-tank reaction coefficients — collected for post-processing.
            }
            _ => {}
        }
        Ok(())
    })
}

// ── Per-element reaction coefficients (post-processing) ───────────────────────
// Format:  BULK  <pipe_id>  <value>
//          WALL  <pipe_id>  <value>
//          TANK  <tank_id>  <value>

fn apply_per_element_reactions(
    lines: &[SecLine<'_>],
    nodes: &mut [Node],
    node_map: &HashMap<String, usize>,
    links: &mut [Link],
    link_map: &HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "REACTIONS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            return Ok(());
        }
        let key = fields[0].to_ascii_uppercase();
        match key.as_str() {
            "BULK" => {
                if let Some(&idx) = link_map.get(fields[1]) {
                    let val = parse_f64(fields[2], "REACTIONS.Bulk")?;
                    if let LinkKind::Pipe(ref mut p) = links[idx].kind {
                        p.bulk_coeff = Some(val);
                    }
                }
            }
            "WALL" => {
                if let Some(&idx) = link_map.get(fields[1]) {
                    let val = parse_f64(fields[2], "REACTIONS.Wall")?;
                    if let LinkKind::Pipe(ref mut p) = links[idx].kind {
                        p.wall_coeff = Some(val);
                    }
                }
            }
            "TANK" => {
                if let Some(&idx) = node_map.get(fields[1]) {
                    let val = parse_f64(fields[2], "REACTIONS.Tank")?;
                    if let NodeKind::Tank(ref mut t) = nodes[idx].kind {
                        t.bulk_coeff = val;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    })
}

// ── Energy ────────────────────────────────────────────────────────────────────

fn apply_energy(opts: &mut SimulationOptions, lines: &[SecLine<'_>]) -> Result<(), ParseError> {
    for_each_line(lines, "ENERGY", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Ok(());
        }
        let key = fields[0].to_ascii_uppercase();
        match key.as_str() {
            "GLOBAL" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "EFFICIENCY") =>
            {
                let v = opt_f64(&fields, 2, "ENERGY.Global Efficiency")?;
                opts.energy_efficiency = v / 100.0; // INP uses percent, core uses fraction.
            }
            "GLOBAL" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "PRICE") =>
            {
                opts.energy_price = opt_f64(&fields, 2, "ENERGY.Global Price")?;
            }
            "DEMAND" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "CHARGE") =>
            {
                opts.peak_demand_charge = opt_f64(&fields, 2, "ENERGY.Demand Charge")?;
            }
            "GLOBAL" if matches!(fields.get(1).map(|s| s.to_ascii_uppercase()), Some(ref s) if s == "PATTERN") =>
            {
                opts.energy_price_pattern = fields.get(2).map(|s| s.to_string());
            }
            "PUMP" => {
                // Per-pump energy settings — pump ID, field, value.
                // Handled in a second pass after links are built. Skip for now.
            }
            _ => {}
        }
        Ok(())
    })
}

/// Second-pass ENERGY parsing: apply per-pump settings (PUMP <id> EFFIC/PRICE/PATTERN).
/// Must be called after links are parsed.
fn apply_pump_energy(
    lines: &[SecLine<'_>],
    links: &mut [Link],
    link_id_to_idx: &HashMap<String, usize>,
    curves: &mut [Curve],
) -> Result<(), ParseError> {
    // One-time curve id → index map to avoid a linear scan per PUMP line.
    let curve_index: HashMap<String, usize> = curves
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), i))
        .collect();

    for_each_line(lines, "ENERGY", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            return Ok(());
        }
        let key = fields[0].to_ascii_uppercase();
        if key != "PUMP" {
            return Ok(());
        }
        let pump_id = fields[1];
        let idx = match link_id_to_idx.get(pump_id) {
            Some(&i) => i,
            None => return Ok(()), // unknown pump ID — skip silently (EPANET does)
        };
        let field_name = fields[2].to_ascii_uppercase();
        let value = fields[3];

        if let LinkKind::Pump(ref mut pump) = links[idx].kind {
            match field_name.as_str() {
                s if s.starts_with("EFFIC") => {
                    pump.efficiency_curve = Some(value.to_string());
                    // Tag this curve as PumpEfficiency if it exists.
                    if let Some(&ci) = curve_index.get(value) {
                        curves[ci].kind = CurveKind::PumpEfficiency;
                    }
                }
                "PRICE" => {
                    pump.energy_price = Some(parse_f64(value, "ENERGY.Pump Price")?);
                }
                "PATTERN" => {
                    pump.price_pattern = Some(value.to_string());
                }
                _ => {}
            }
        }
        Ok(())
    })
}

// ── Junctions ─────────────────────────────────────────────────────────────────
// Format: ID  Elev  Demand  Pattern

fn parse_junctions(
    lines: &[SecLine<'_>],
    nodes: &mut Vec<Node>,
    id_map: &mut HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "JUNCTIONS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(ParseError::InvalidField {
                field: "JUNCTIONS".into(),
                reason: format!("need at least 2 fields (ID Elev), got {}", fields.len()),
            });
        }
        let id = fields[0].to_string();
        if id_map.contains_key(&id) {
            return Err(ParseError::DuplicateId { object: "node", id });
        }
        let elevation = parse_f64(fields[1], "JUNCTIONS.Elev")?;
        let base_demand = if fields.len() > 2 {
            parse_f64(fields[2], "JUNCTIONS.Demand")?
        } else {
            0.0
        };
        let pattern = if fields.len() > 3 && !fields[3].is_empty() {
            Some(fields[3].to_string())
        } else {
            None
        };

        let demands = if base_demand != 0.0 || pattern.is_some() {
            vec![DemandCategory {
                base_demand,
                pattern,
                name: None,
            }]
        } else {
            vec![]
        };

        let idx = nodes.len();
        id_map.insert(id.clone(), idx);
        nodes.push(Node {
            base: NodeBase {
                id,
                index: idx + 1,
                elevation,
                initial_quality: 0.0,
            },
            kind: NodeKind::Junction(Junction {
                demands,
                emitter_coeff: 0.0,
                emitter_exp: 0.5,
            }),
            source: None,
        });
        Ok(())
    })
}

// ── Reservoirs ────────────────────────────────────────────────────────────────
// Format: ID  Head  Pattern

fn parse_reservoirs(
    lines: &[SecLine<'_>],
    nodes: &mut Vec<Node>,
    id_map: &mut HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "RESERVOIRS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(ParseError::InvalidField {
                field: "RESERVOIRS".into(),
                reason: format!("need at least 2 fields (ID Head), got {}", fields.len()),
            });
        }
        let id = fields[0].to_string();
        if id_map.contains_key(&id) {
            return Err(ParseError::DuplicateId { object: "node", id });
        }
        let head = parse_f64(fields[1], "RESERVOIRS.Head")?;
        let pattern = if fields.len() > 2 && !fields[2].is_empty() {
            Some(fields[2].to_string())
        } else {
            None
        };

        let idx = nodes.len();
        id_map.insert(id.clone(), idx);
        nodes.push(Node {
            base: NodeBase {
                id,
                index: idx + 1,
                elevation: head,
                initial_quality: 0.0,
            },
            kind: NodeKind::Reservoir(Reservoir {
                head_pattern: pattern,
            }),
            source: None,
        });
        Ok(())
    })
}

// ── Tanks ─────────────────────────────────────────────────────────────────────
// Format: ID  Elevation  InitLevel  MinLevel  MaxLevel  Diameter  MinVol  VolCurve  Overflow

fn parse_tanks(
    lines: &[SecLine<'_>],
    nodes: &mut Vec<Node>,
    id_map: &mut HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "TANKS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(ParseError::InvalidField {
                field: "TANKS".into(),
                reason: format!("need at least 2 fields (ID Elev), got {}", fields.len()),
            });
        }
        let id = fields[0].to_string();
        if id_map.contains_key(&id) {
            return Err(ParseError::DuplicateId { object: "node", id });
        }
        let elevation = parse_f64(fields[1], "TANKS.Elevation")?;
        // Fields beyond ID and Elev default to 0 when omitted (EPANET compat).
        let init_level = if fields.len() > 2 {
            parse_f64(fields[2], "TANKS.InitLevel")?
        } else {
            0.0
        };
        let min_level = if fields.len() > 3 {
            parse_f64(fields[3], "TANKS.MinLevel")?
        } else {
            0.0
        };
        let max_level = if fields.len() > 4 {
            parse_f64(fields[4], "TANKS.MaxLevel")?
        } else {
            0.0
        };
        let diameter = if fields.len() > 5 {
            parse_f64(fields[5], "TANKS.Diameter")?
        } else {
            0.0
        };
        // MinVol (field 6): explicit minimum volume; overrides area*min_level
        // when > 0.  EPANET stores this directly.
        let min_volume = if fields.len() > 6 {
            let v = parse_f64(fields[6], "TANKS.MinVol")?;
            if v > 0.0 {
                v
            } else {
                0.0
            }
        } else {
            0.0
        };
        let vol_curve = if fields.len() > 7 && !fields[7].is_empty() && fields[7] != "*" {
            Some(fields[7].to_string())
        } else {
            None
        };

        // Field 8: Overflow (YES/NO). Default is NO.
        let overflow = if fields.len() > 8 {
            fields[8].eq_ignore_ascii_case("YES")
        } else {
            false
        };

        let idx = nodes.len();
        id_map.insert(id.clone(), idx);
        nodes.push(Node {
            base: NodeBase {
                id,
                index: idx + 1,
                elevation,
                initial_quality: 0.0,
            },
            kind: NodeKind::Tank(Tank {
                min_level,
                max_level,
                initial_level: init_level,
                diameter,
                min_volume,
                volume_curve: vol_curve,
                mix_model: MixModel::Cstr,
                mix_fraction: 1.0,
                bulk_coeff: 0.0,
                overflow,
                head_pattern: None,
            }),
            source: None,
        });
        Ok(())
    })
}

// ── Demands (additional categories) ───────────────────────────────────────────
// Format: Junction  Demand  Pattern  Category

fn apply_demands(
    lines: &[SecLine<'_>],
    nodes: &mut [Node],
    id_map: &HashMap<String, usize>,
) -> Result<(), ParseError> {
    // Track which junctions have had their first DEMANDS entry processed.
    // EPANET behaviour: the first [DEMANDS] entry for a junction REPLACES
    // the demand category created in [JUNCTIONS]; subsequent entries append.
    let mut first_replaced: HashSet<usize> = HashSet::new();

    for_each_line(lines, "DEMANDS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(ParseError::InvalidField {
                field: "DEMANDS".into(),
                reason: format!(
                    "need at least 2 fields (Junction Demand), got {}",
                    fields.len()
                ),
            });
        }
        let idx = resolve_node(id_map, fields[0])?;
        let demand = parse_f64(fields[1], "DEMANDS.Demand")?;
        let pattern = if fields.len() > 2 && !fields[2].is_empty() {
            Some(fields[2].to_string())
        } else {
            None
        };
        let name = if fields.len() > 3 {
            Some(fields[3..].join(" "))
        } else {
            None
        };

        if let NodeKind::Junction(ref mut j) = nodes[idx].kind {
            if !first_replaced.contains(&idx) && !j.demands.is_empty() {
                // Replace the demand category created in [JUNCTIONS]
                j.demands[0] = DemandCategory {
                    base_demand: demand,
                    pattern,
                    name,
                };
            } else {
                j.demands.push(DemandCategory {
                    base_demand: demand,
                    pattern,
                    name,
                });
            }
            first_replaced.insert(idx);
        }
        Ok(())
    })
}

// ── Emitters ──────────────────────────────────────────────────────────────────
// Format: Junction  Coefficient

fn apply_emitters(
    lines: &[SecLine<'_>],
    nodes: &mut [Node],
    id_map: &HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "EMITTERS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(ParseError::InvalidField {
                field: "EMITTERS".into(),
                reason: format!(
                    "need at least 2 fields (Junction Coefficient), got {}",
                    fields.len()
                ),
            });
        }
        let idx = resolve_node(id_map, fields[0])?;
        let coeff = parse_f64(fields[1], "EMITTERS.Coefficient")?;
        if let NodeKind::Junction(ref mut j) = nodes[idx].kind {
            j.emitter_coeff = coeff;
        }
        Ok(())
    })
}

// ── Quality (initial concentrations) ──────────────────────────────────────────
// Format: Node  InitQual

fn apply_quality(
    lines: &[SecLine<'_>],
    nodes: &mut [Node],
    id_map: &HashMap<String, usize>,
) -> Result<(), ParseError> {
    // Pre-parse each node ID as a number once, instead of re-parsing every
    // node ID for every range-format line.
    let numeric_ids: Vec<(usize, i64)> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.base.id.parse::<i64>().ok().map(|v| (i, v)))
        .collect();

    for_each_line(lines, "QUALITY", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(ParseError::InvalidField {
                field: "QUALITY".into(),
                reason: format!(
                    "need at least 2 fields (Node InitQual), got {}",
                    fields.len()
                ),
            });
        }

        if fields.len() == 2 {
            // Single node format: node initqual
            // Skip unknown node IDs (some legacy INP files reference removed nodes).
            let idx = match resolve_node(id_map, fields[0]) {
                Ok(i) => i,
                Err(_) => return Ok(()),
            };
            let qual = parse_f64(fields[1], "QUALITY.InitQual")?;
            nodes[idx].base.initial_quality = qual;
        } else {
            // Range format: node1 node2 initqual
            // EPANET assigns quality to all nodes with IDs in range [node1, node2].
            let qual = parse_f64(fields[2], "QUALITY.InitQual")?;
            let i1_opt: Option<i64> = fields[0].parse().ok();
            let i2_opt: Option<i64> = fields[1].parse().ok();

            if let (Some(i1), Some(i2)) = (i1_opt, i2_opt) {
                // Numeric range: assign to all nodes whose ID parses as a number in [i1, i2].
                for &(idx, nid) in &numeric_ids {
                    if nid >= i1 && nid <= i2 {
                        nodes[idx].base.initial_quality = qual;
                    }
                }
            } else {
                // Lexicographic range: assign to all nodes whose ID is in [tok0, tok1].
                for node in nodes.iter_mut() {
                    if node.base.id.as_str() >= fields[0] && node.base.id.as_str() <= fields[1] {
                        node.base.initial_quality = qual;
                    }
                }
            }
        }
        Ok(())
    })
}

// ── Mixing ────────────────────────────────────────────────────────────────────
// Format: Tank  Model  [Fraction]

fn apply_mixing(
    lines: &[SecLine<'_>],
    nodes: &mut [Node],
    id_map: &HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "MIXING", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(ParseError::InvalidField {
                field: "MIXING".into(),
                reason: format!("need at least 2 fields (Tank Model), got {}", fields.len()),
            });
        }
        let idx = resolve_node(id_map, fields[0])?;
        let model = match fields[1].to_ascii_uppercase().as_str() {
            "MIXED" => MixModel::Cstr,
            "2COMP" => MixModel::TwoCompartment,
            "FIFO" => MixModel::Fifo,
            "LIFO" => MixModel::Lifo,
            other => {
                return Err(ParseError::InvalidField {
                    field: "MIXING.Model".into(),
                    reason: format!("unknown mix model '{other}'"),
                });
            }
        };
        let fraction = if fields.len() > 2 {
            parse_f64(fields[2], "MIXING.Fraction")?
        } else {
            1.0
        };

        if let NodeKind::Tank(ref mut t) = nodes[idx].kind {
            t.mix_model = model;
            t.mix_fraction = fraction;
        }
        Ok(())
    })
}

// ── Sources ───────────────────────────────────────────────────────────────────
// Format: Node  Type  Quality  Pattern

fn apply_sources(
    lines: &[SecLine<'_>],
    nodes: &mut [Node],
    id_map: &HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "SOURCES", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(ParseError::InvalidField {
                field: "SOURCES".into(),
                reason: format!(
                    "need at least 2 fields (Node Type/Quality), got {}",
                    fields.len()
                ),
            });
        }
        let idx = resolve_node(id_map, fields[0])?;

        // EPANET allows omitting the type field: `Node Quality [Pattern]`.
        // Detect by trying to parse fields[1] as a source type; if it's a
        // number, assume CONCEN and shift field indices.
        let (kind, quality_idx) = match fields[1].to_ascii_uppercase().as_str() {
            "CONCEN" | "CONCENTRATION" => (SourceType::Concentration, 2),
            "MASS" => (SourceType::Mass, 2),
            "SETPOINT" => (SourceType::Setpoint, 2),
            "FLOWPACED" | "FLOW_PACED" => (SourceType::FlowPaced, 2),
            _ => {
                // Not a recognized type — treat as quality value (type=CONCEN)
                if fields[1].parse::<f64>().is_ok() {
                    (SourceType::Concentration, 1)
                } else {
                    return Err(ParseError::InvalidField {
                        field: "SOURCES.Type".into(),
                        reason: format!("unknown source type '{}'", fields[1]),
                    });
                }
            }
        };
        if fields.len() <= quality_idx {
            return Ok(());
        }
        let base_value = parse_f64(fields[quality_idx], "SOURCES.Quality")?;
        let pattern_idx = quality_idx + 1;
        let pattern = if fields.len() > pattern_idx && !fields[pattern_idx].is_empty() {
            Some(fields[pattern_idx].to_string())
        } else {
            None
        };
        let node_index = idx + 1; // 1-based
        nodes[idx].source = Some(QualitySource {
            node: node_index,
            kind,
            base_value,
            pattern,
        });
        Ok(())
    })
}

// ── Pipes ─────────────────────────────────────────────────────────────────────
// Format: ID  Node1  Node2  Length  Diameter  Roughness  MinorLoss  Status

fn parse_pipes(
    lines: &[SecLine<'_>],
    links: &mut Vec<Link>,
    link_map: &mut HashMap<String, usize>,
    node_map: &HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "PIPES", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            return Err(ParseError::InvalidField {
                field: "PIPES".into(),
                reason: format!(
                    "need at least 6 fields (ID Node1 Node2 Length Diameter Roughness), got {}",
                    fields.len()
                ),
            });
        }
        let id = fields[0].to_string();
        if link_map.contains_key(&id) {
            return Err(ParseError::DuplicateId { object: "link", id });
        }
        let from_node = resolve_node(node_map, fields[1])? + 1;
        let to_node = resolve_node(node_map, fields[2])? + 1;
        let length = parse_f64(fields[3], "PIPES.Length")?;
        let diameter = parse_f64(fields[4], "PIPES.Diameter")?;
        let roughness = parse_f64(fields[5], "PIPES.Roughness")?;
        // EPANET allows field[6] to be either a numeric minor loss OR a status
        // keyword (CV/OPEN/CLOSED).  If there are 8+ fields, field[6] is always
        // minor loss and field[7] is status.  With exactly 7 fields, try to
        // parse field[6] as a keyword first; fall back to numeric.
        let (minor_loss, status) = if fields.len() > 7 {
            // 8+ fields: field[6] = minor loss, field[7] = status
            let ml = parse_f64(fields[6], "PIPES.MinorLoss")?;
            let st = parse_link_status_inp(fields[7])?;
            (ml, st)
        } else if fields.len() > 6 {
            // 7 fields: field[6] is keyword OR numeric
            match fields[6].to_ascii_uppercase().as_str() {
                "CV" => (0.0, LinkStatus::Active),
                "OPEN" | "" => (0.0, LinkStatus::Open),
                "CLOSED" | "CLOSE" => (0.0, LinkStatus::Closed),
                _ => {
                    let ml = parse_f64(fields[6], "PIPES.MinorLoss")?;
                    (ml, LinkStatus::Open)
                }
            }
        } else {
            (0.0, LinkStatus::Open)
        };
        let check_valve = matches!(status, LinkStatus::Active);

        let idx = links.len();
        link_map.insert(id.clone(), idx);
        links.push(Link {
            base: LinkBase {
                id,
                index: idx + 1,
                from_node,
                to_node,
                initial_status: if check_valve {
                    LinkStatus::Open
                } else {
                    status
                },
                initial_setting: Some(1.0),
            },
            kind: LinkKind::Pipe(Pipe {
                length,
                diameter,
                roughness,
                minor_loss,
                check_valve,
                bulk_coeff: None,
                wall_coeff: None,
                leak_coeff_1: 0.0,
                leak_coeff_2: 0.0,
            }),
        });
        Ok(())
    })
}

// ── Pumps ─────────────────────────────────────────────────────────────────────
// Format: ID  Node1  Node2  Parameters...
// Parameters:  HEAD <curve_id>  |  POWER <value>  |  SPEED <value>  |  PATTERN <id>

fn parse_pumps(
    lines: &[SecLine<'_>],
    links: &mut Vec<Link>,
    link_map: &mut HashMap<String, usize>,
    node_map: &HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "PUMPS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            return Err(ParseError::InvalidField {
                field: "PUMPS".into(),
                reason: format!(
                    "need at least 4 fields (ID Node1 Node2 Parameters...), got {}",
                    fields.len()
                ),
            });
        }
        let id = fields[0].to_string();
        if link_map.contains_key(&id) {
            return Err(ParseError::DuplicateId { object: "link", id });
        }
        let from_node = resolve_node(node_map, fields[1])? + 1;
        let to_node = resolve_node(node_map, fields[2])? + 1;

        let mut head_curve = None;
        let mut power = None;
        let mut speed_pattern = None;
        let mut init_setting = 1.0;

        // Parse keyword-value pairs from fields[3..].
        let mut i = 3;
        while i < fields.len() {
            let kw = fields[i].to_ascii_uppercase();
            match kw.as_str() {
                "HEAD" => {
                    i += 1;
                    if i < fields.len() {
                        head_curve = Some(fields[i].to_string());
                    }
                }
                "POWER" => {
                    i += 1;
                    if i < fields.len() {
                        power = Some(parse_f64(fields[i], "PUMPS.POWER")?);
                    }
                }
                "SPEED" => {
                    i += 1;
                    if i < fields.len() {
                        init_setting = parse_f64(fields[i], "PUMPS.SPEED")?;
                    }
                }
                "PATTERN" => {
                    i += 1;
                    if i < fields.len() {
                        speed_pattern = Some(fields[i].to_string());
                    }
                }
                _ => {}
            }
            i += 1;
        }

        let curve_type = if head_curve.is_some() {
            PumpCurveType::PowerFunction
        } else if power.is_some() {
            PumpCurveType::ConstHp
        } else {
            PumpCurveType::Custom
        };

        let idx = links.len();
        link_map.insert(id.clone(), idx);
        links.push(Link {
            base: LinkBase {
                id,
                index: idx + 1,
                from_node,
                to_node,
                initial_status: LinkStatus::Open,
                initial_setting: Some(init_setting),
            },
            kind: LinkKind::Pump(Pump {
                curve_type,
                head_curve,
                power,
                efficiency_curve: None,
                default_efficiency: 0.0,
                speed_pattern,
                energy_price: None,
                price_pattern: None,
            }),
        });
        Ok(())
    })
}

// ── Valves ────────────────────────────────────────────────────────────────────
// Format: ID  Node1  Node2  Diameter  Type  Setting  MinorLoss

fn parse_valves(
    lines: &[SecLine<'_>],
    links: &mut Vec<Link>,
    link_map: &mut HashMap<String, usize>,
    node_map: &HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "VALVES", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            return Err(ParseError::InvalidField {
                field: "VALVES".into(),
                reason: format!(
                    "need at least 6 fields (ID Node1 Node2 Diameter Type Setting), got {}",
                    fields.len()
                ),
            });
        }
        let id = fields[0].to_string();
        if link_map.contains_key(&id) {
            return Err(ParseError::DuplicateId { object: "link", id });
        }
        let from_node = resolve_node(node_map, fields[1])? + 1;
        let to_node = resolve_node(node_map, fields[2])? + 1;
        let diameter = parse_f64(fields[3], "VALVES.Diameter")?;
        let valve_type = parse_valve_type_inp(fields[4])?;

        // GPV: setting field is a curve ID reference (string), not a number.
        let (mut curve, setting) = if valve_type == ValveType::Gpv {
            (Some(fields[5].to_string()), 0.0)
        } else {
            (None, parse_f64(fields[5], "VALVES.Setting")?)
        };
        let minor_loss = if fields.len() > 6 {
            parse_f64(fields[6], "VALVES.MinorLoss")?
        } else {
            0.0
        };
        // PCV: optional 8th field is a loss-ratio curve ID.
        if valve_type == ValveType::Pcv && fields.len() > 7 {
            curve = Some(fields[7].to_string());
        }

        // EPANET convention: valve InitStatus is always ACTIVE; the raw
        // setting (including any negative sign) is preserved.  Negative
        // settings have a physical meaning during solving (the sign
        // causes the head setpoint to fall below the elevation, which
        // drives the valve closed during status checks).
        let init_status = LinkStatus::Active;
        let init_setting = setting;

        let idx = links.len();
        link_map.insert(id.clone(), idx);
        links.push(Link {
            base: LinkBase {
                id,
                index: idx + 1,
                from_node,
                to_node,
                initial_status: init_status,
                initial_setting: Some(init_setting),
            },
            kind: LinkKind::Valve(Valve {
                valve_type,
                diameter,
                minor_loss,
                curve,
            }),
        });
        Ok(())
    })
}

// ── Status overrides ──────────────────────────────────────────────────────────
// Format: ID  Status/Setting

fn apply_status(
    lines: &[SecLine<'_>],
    links: &mut [Link],
    link_map: &HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "STATUS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            return Err(ParseError::InvalidField {
                field: "STATUS".into(),
                reason: format!(
                    "need at least 2 fields (ID Status/Setting), got {}",
                    fields.len()
                ),
            });
        }
        let idx = resolve_link(link_map, fields[0])?;
        let val = fields[1].to_ascii_uppercase();
        match val.as_str() {
            "OPEN" => {
                links[idx].base.initial_status = LinkStatus::Open;
                // EPANET sets pump speed to 1.0 when opened via [STATUS].
                if matches!(links[idx].kind, LinkKind::Pump(_)) {
                    links[idx].base.initial_setting = Some(1.0);
                }
            }
            "CLOSED" | "CLOSE" => {
                links[idx].base.initial_status = LinkStatus::Closed;
                // EPANET: pump Kc = 0.0 when closed (speed = 0).
                if matches!(links[idx].kind, LinkKind::Pump(_)) {
                    links[idx].base.initial_setting = Some(0.0);
                }
            }
            _ => {
                // Numeric setting (e.g., pump speed or valve setting).
                if let Ok(v) = fields[1].parse::<f64>() {
                    links[idx].base.initial_setting = Some(v);
                }
            }
        }
        Ok(())
    })
}

// ── Leakage ───────────────────────────────────────────────────────────────────
// Format: PipeID  Coeff1  Coeff2
// [LEAKAGE] section — assigns FAVAD leak coefficients to pipes (spec.md §4.3).

fn apply_leakage(
    lines: &[SecLine<'_>],
    links: &mut [Link],
    link_map: &HashMap<String, usize>,
) -> Result<(), ParseError> {
    for_each_line(lines, "LEAKAGE", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            return Err(ParseError::InvalidField {
                field: "LEAKAGE".into(),
                reason: format!(
                    "need at least 3 fields (PipeID Coeff1 Coeff2), got {}",
                    fields.len()
                ),
            });
        }
        let idx = resolve_link(link_map, fields[0])?;
        // Only pipes have leakage — silently skip non-pipe links (matches EPANET).
        if let LinkKind::Pipe(ref mut pipe) = links[idx].kind {
            pipe.leak_coeff_1 = parse_f64(fields[1], "LEAKAGE.Coeff1")?;
            pipe.leak_coeff_2 = parse_f64(fields[2], "LEAKAGE.Coeff2")?;
        }
        Ok(())
    })
}

// ── Controls ──────────────────────────────────────────────────────────────────
// Free-form text:  LINK <id> <status/setting> IF NODE <id> ABOVE/BELOW <value>
//                  LINK <id> <status/setting> AT TIME <value>
//                  LINK <id> <status/setting> AT CLOCKTIME <value> AM/PM

fn parse_controls(
    lines: &[SecLine<'_>],
    node_map: &HashMap<String, usize>,
    link_map: &HashMap<String, usize>,
) -> Result<Vec<SimpleControl>, ParseError> {
    let mut controls = Vec::new();

    for_each_line(lines, "CONTROLS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            return Ok(());
        }
        // LINK <link_id> <status_or_setting> ...
        if !fields[0].eq_ignore_ascii_case("LINK") {
            return Ok(());
        }
        let link_idx = resolve_link(link_map, fields[1])? + 1; // 1-based
        let (action_status, action_setting) = parse_control_action(fields[2])?;

        // Determine trigger type from the rest.
        let rest: Vec<String> = fields[3..].iter().map(|s| s.to_ascii_uppercase()).collect();

        if rest.len() >= 4 && rest[0] == "IF" && rest[1] == "NODE" {
            // IF NODE <node_id> ABOVE/BELOW <value>
            let node_id_str = fields[3 + 2]; // original case for ID
            let node_idx = resolve_node(node_map, node_id_str)? + 1;
            let trigger = match rest[3].as_str() {
                "ABOVE" => TriggerType::HiLevel,
                "BELOW" => TriggerType::LowLevel,
                other => {
                    return Err(ParseError::InvalidField {
                        field: "CONTROLS".into(),
                        reason: format!("expected ABOVE or BELOW, got '{other}'"),
                    });
                }
            };
            let grade = parse_f64(fields[3 + 4], "CONTROLS.grade")?;
            controls.push(SimpleControl {
                link: link_idx,
                trigger_type: trigger,
                trigger_time: None,
                trigger_node: Some(node_idx),
                trigger_grade: Some(grade),
                action_status,
                action_setting,
                enabled: true,
            });
        } else if rest.len() >= 3 && rest[0] == "AT" && rest[1] == "TIME" {
            // AT TIME <value>
            // parse_time_value already converts plain numbers to seconds (hours → s).
            let t = parse_time_value(&fields[5..], "CONTROLS.time")?;
            controls.push(SimpleControl {
                link: link_idx,
                trigger_type: TriggerType::Timer,
                trigger_time: Some(t),
                trigger_node: None,
                trigger_grade: None,
                action_status,
                action_setting,
                enabled: true,
            });
        } else if rest.len() >= 3 && rest[0] == "AT" && rest[1] == "CLOCKTIME" {
            // AT CLOCKTIME <value> [AM|PM]
            let ct = parse_clocktime(&fields[5..]);
            controls.push(SimpleControl {
                link: link_idx,
                trigger_type: TriggerType::TimeOfDay,
                trigger_time: Some(ct),
                trigger_node: None,
                trigger_grade: None,
                action_status,
                action_setting,
                enabled: true,
            });
        }
        Ok(())
    })?;

    Ok(controls)
}

fn parse_control_action(s: &str) -> Result<(Option<LinkStatus>, Option<f64>), ParseError> {
    match s.to_ascii_uppercase().as_str() {
        "OPEN" => Ok((Some(LinkStatus::Open), None)),
        "CLOSED" | "CLOSE" => Ok((Some(LinkStatus::Closed), None)),
        _ => {
            // Numeric setting.
            let v = parse_f64(s, "CONTROLS.action")?;
            Ok((None, Some(v)))
        }
    }
}

// ── Rules ─────────────────────────────────────────────────────────────────────
// RULE <id>
// IF/AND/OR <premise>
// THEN <action>
// ELSE <action>
// PRIORITY <value>

fn parse_rules(
    lines: &[SecLine<'_>],
    node_map: &HashMap<String, usize>,
    link_map: &HashMap<String, usize>,
) -> Result<Vec<Rule>, ParseError> {
    let mut rules = Vec::new();
    let mut current_premises: Vec<Premise> = Vec::new();
    let mut current_then: Vec<RuleAction> = Vec::new();
    let mut current_else: Vec<RuleAction> = Vec::new();
    let mut current_priority = 0.0;
    let mut in_rule = false;

    for_each_line(lines, "RULES", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.is_empty() {
            return Ok(());
        }
        let kw = fields[0].to_ascii_uppercase();

        match kw.as_str() {
            "RULE" => {
                // Finish previous rule if any.
                if in_rule && (!current_premises.is_empty()) {
                    rules.push(Rule {
                        priority: current_priority,
                        premises: std::mem::take(&mut current_premises),
                        then_actions: std::mem::take(&mut current_then),
                        else_actions: std::mem::take(&mut current_else),
                    });
                }
                current_priority = 0.0;
                in_rule = true;
            }
            "IF" | "AND" | "OR" => {
                let connective = match kw.as_str() {
                    "AND" => Some(LogicOp::And),
                    "OR" => Some(LogicOp::Or),
                    _ => None,
                };
                let premise = parse_rule_premise(&fields[1..], connective, node_map, link_map)?;
                current_premises.push(premise);
            }
            "THEN" => {
                let action = parse_rule_action(&fields[1..], link_map)?;
                current_then.push(action);
            }
            "ELSE" => {
                let action = parse_rule_action(&fields[1..], link_map)?;
                current_else.push(action);
            }
            "PRIORITY" if fields.len() > 1 => {
                current_priority = parse_f64(fields[1], "RULES.PRIORITY")?;
            }
            "PRIORITY" => {}
            _ => {}
        }
        Ok(())
    })?;

    // Finish last rule.
    if in_rule && !current_premises.is_empty() {
        rules.push(Rule {
            priority: current_priority,
            premises: current_premises,
            then_actions: current_then,
            else_actions: current_else,
        });
    }

    Ok(rules)
}

fn parse_rule_premise(
    fields: &[&str],
    connective: Option<LogicOp>,
    node_map: &HashMap<String, usize>,
    link_map: &HashMap<String, usize>,
) -> Result<Premise, ParseError> {
    // Forms:
    //   NODE <id> <attribute> <op> <value>
    //   LINK <id> <attribute> <op> <value>
    //   SYSTEM CLOCKTIME <op> <value>
    //   SYSTEM TIME <op> <value>
    //   SYSTEM DEMAND <op> <value>
    if fields.is_empty() {
        return Err(ParseError::InvalidField {
            field: "RULES premise".into(),
            reason: "empty premise".into(),
        });
    }

    let obj_type = fields[0].to_ascii_uppercase();
    match obj_type.as_str() {
        "NODE" | "JUNC" | "JUNCTION" | "RESERV" | "RESERVOIR" | "TANK" => {
            if fields.len() < 5 {
                return Err(ParseError::InvalidField {
                    field: "RULES premise".into(),
                    reason: "need: NODE <id> <attr> <op> <value>".into(),
                });
            }
            let idx = resolve_node(node_map, fields[1])? + 1;
            let attribute = parse_premise_attr(fields[2])?;
            let operator = parse_premise_op(fields[3])?;
            let value = parse_premise_value(fields[4], &attribute)?;
            Ok(Premise {
                object: PremiseObject::Node(idx),
                attribute,
                operator,
                value,
                connective,
            })
        }
        "LINK" | "PIPE" | "PUMP" | "VALVE" => {
            if fields.len() < 5 {
                return Err(ParseError::InvalidField {
                    field: "RULES premise".into(),
                    reason: "need: LINK <id> <attr> <op> <value>".into(),
                });
            }
            let idx = resolve_link(link_map, fields[1])? + 1;
            let attribute = parse_premise_attr(fields[2])?;
            let operator = parse_premise_op(fields[3])?;
            let value = parse_premise_value(fields[4], &attribute)?;
            Ok(Premise {
                object: PremiseObject::Link(idx),
                attribute,
                operator,
                value,
                connective,
            })
        }
        "SYSTEM" => {
            if fields.len() < 4 {
                return Err(ParseError::InvalidField {
                    field: "RULES premise".into(),
                    reason: "need: SYSTEM <attr> <op> <value>".into(),
                });
            }
            let attribute = parse_premise_attr(fields[1])?;
            let operator = parse_premise_op(fields[2])?;
            let value = parse_premise_value(fields[3], &attribute)?;
            Ok(Premise {
                object: PremiseObject::Clock,
                attribute,
                operator,
                value,
                connective,
            })
        }
        _ => Err(ParseError::InvalidField {
            field: "RULES premise".into(),
            reason: format!("unknown object type '{obj_type}'"),
        }),
    }
}

fn parse_rule_action(
    fields: &[&str],
    link_map: &HashMap<String, usize>,
) -> Result<RuleAction, ParseError> {
    // Forms:
    //   LINK <id> <status_or_setting> = <value>
    //   LINK <id> STATUS = OPEN/CLOSED
    //   LINK <id> SETTING = <value>
    //   PUMP <id> STATUS = OPEN/CLOSED
    //   PUMP <id> SPEED = <value>
    //   PIPE <id> STATUS = OPEN/CLOSED
    //   VALVE <id> SETTING = <value>
    if fields.len() < 4 {
        return Err(ParseError::InvalidField {
            field: "RULES action".into(),
            reason: "need: LINK <id> <property> = <value>".into(),
        });
    }
    // fields[0] is object type keyword (LINK/PIPE/PUMP/VALVE).
    let idx = resolve_link(link_map, fields[1])? + 1;
    let prop = fields[2].to_ascii_uppercase();
    // fields[3] should be "=" — skip it.
    let val_str = if fields.len() > 4 {
        fields[4]
    } else {
        fields[3]
    };

    let value = match prop.as_str() {
        "STATUS" => {
            let s = val_str.to_ascii_uppercase();
            match s.as_str() {
                "OPEN" => ActionValue::Status(LinkStatus::Open),
                "CLOSED" | "CLOSE" => ActionValue::Status(LinkStatus::Closed),
                _ => ActionValue::Status(LinkStatus::Open),
            }
        }
        "SETTING" | "SPEED" => {
            let v = parse_f64(val_str, "RULES action value")?;
            ActionValue::Setting(v)
        }
        _ => {
            return Err(ParseError::InvalidField {
                field: "RULES action".into(),
                reason: format!("unknown property '{prop}'"),
            });
        }
    };

    Ok(RuleAction { link: idx, value })
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn parse_f64(s: &str, ctx: &str) -> Result<f64, ParseError> {
    s.parse::<f64>().map_err(|_| ParseError::InvalidField {
        field: ctx.to_string(),
        reason: format!("cannot parse '{s}' as a number"),
    })
}

// ── Timestep adjustment (EPANET adjustdata equivalent) ────────────────────────
// Mirrors the timestep capping and defaulting logic in EPANET's adjustdata()
// (input1.c).  Must be called after parsing and unit conversion.
fn adjust_timesteps(opts: &mut SimulationOptions) {
    // Pattern step must be positive.
    if opts.pattern_step <= 0.0 {
        opts.pattern_step = 3600.0;
    }
    // Report step defaults to pattern step if zero.
    if opts.report_step == 0.0 {
        opts.report_step = opts.pattern_step;
    }
    // Hydraulic step must be positive.
    if opts.hyd_step <= 0.0 {
        opts.hyd_step = 3600.0;
    }
    // Cap hydraulic step to min(pattern_step, report_step).
    if opts.hyd_step > opts.pattern_step {
        opts.hyd_step = opts.pattern_step;
    }
    if opts.hyd_step > opts.report_step {
        opts.hyd_step = opts.report_step;
    }
    // Report start capped to duration.
    if opts.report_start > opts.duration {
        opts.report_start = 0.0;
    }
    // Quality step defaults to hyd_step / 10 if zero.
    if opts.qual_step == 0.0 {
        opts.qual_step = opts.hyd_step / 10.0;
    }
    // Quality step cannot exceed hydraulic step.
    if opts.qual_step > opts.hyd_step {
        opts.qual_step = opts.hyd_step;
    }
    // Rule timestep defaults to hyd_step / 10 if zero.
    if opts.rule_timestep == 0.0 {
        opts.rule_timestep = opts.hyd_step / 10.0;
    }
    // Rule timestep cannot exceed hydraulic step.
    if opts.rule_timestep > opts.hyd_step {
        opts.rule_timestep = opts.hyd_step;
    }
}

fn opt_f64(fields: &[&str], idx: usize, ctx: &str) -> Result<f64, ParseError> {
    fields
        .get(idx)
        .ok_or_else(|| ParseError::InvalidField {
            field: ctx.to_string(),
            reason: "missing value".into(),
        })
        .and_then(|s| parse_f64(s, ctx))
}

fn resolve_node(map: &HashMap<String, usize>, id: &str) -> Result<usize, ParseError> {
    map.get(id)
        .copied()
        .ok_or_else(|| ParseError::InvalidField {
            field: "node reference".into(),
            reason: format!("unknown node ID '{id}'"),
        })
}

fn resolve_link(map: &HashMap<String, usize>, id: &str) -> Result<usize, ParseError> {
    map.get(id)
        .copied()
        .ok_or_else(|| ParseError::InvalidField {
            field: "link reference".into(),
            reason: format!("unknown link ID '{id}'"),
        })
}

fn parse_flow_units(s: &str) -> Result<FlowUnits, ParseError> {
    match s.to_ascii_uppercase().as_str() {
        "CFS" => Ok(FlowUnits::Cfs),
        "GPM" => Ok(FlowUnits::Gpm),
        "MGD" => Ok(FlowUnits::Mgd),
        "IMGD" => Ok(FlowUnits::Imgd),
        "AFD" => Ok(FlowUnits::Afd),
        "LPS" | "SI" => Ok(FlowUnits::Lps),
        "LPM" => Ok(FlowUnits::Lpm),
        "MLD" => Ok(FlowUnits::Mld),
        "CMH" => Ok(FlowUnits::Cmh),
        "CMD" => Ok(FlowUnits::Cmd),
        "CMS" => Ok(FlowUnits::Cms),
        _ => Err(ParseError::InvalidField {
            field: "OPTIONS.Units".into(),
            reason: format!("unknown flow unit '{s}'"),
        }),
    }
}

fn parse_link_status_inp(s: &str) -> Result<LinkStatus, ParseError> {
    match s.to_ascii_uppercase().as_str() {
        "OPEN" | "" => Ok(LinkStatus::Open),
        "CLOSED" | "CLOSE" => Ok(LinkStatus::Closed),
        "CV" => Ok(LinkStatus::Active), // Check valve sentinel.
        _ => Err(ParseError::InvalidField {
            field: "status".into(),
            reason: format!("unknown status '{s}'"),
        }),
    }
}

fn parse_valve_type_inp(s: &str) -> Result<ValveType, ParseError> {
    match s.to_ascii_uppercase().as_str() {
        "PRV" => Ok(ValveType::Prv),
        "PSV" => Ok(ValveType::Psv),
        "FCV" => Ok(ValveType::Fcv),
        "TCV" => Ok(ValveType::Tcv),
        "GPV" => Ok(ValveType::Gpv),
        "PCV" => Ok(ValveType::Pcv),
        "PBV" => Ok(ValveType::Pbv),
        _ => Err(ParseError::InvalidField {
            field: "VALVES.Type".into(),
            reason: format!("unknown valve type '{s}'"),
        }),
    }
}

/// Parse an EPANET time value. Accepts:
///   `H:MM`, `H:MM:SS`, or decimal hours, or decimal seconds.
fn parse_time_value(fields: &[&str], ctx: &str) -> Result<f64, ParseError> {
    if fields.is_empty() {
        return Err(ParseError::InvalidField {
            field: ctx.to_string(),
            reason: "missing time value".into(),
        });
    }
    let s = fields[0];
    if let Some(colon_pos) = s.find(':') {
        // H:MM or H:MM:SS
        let hours: f64 = s[..colon_pos]
            .parse()
            .map_err(|_| ParseError::InvalidField {
                field: ctx.to_string(),
                reason: format!("invalid hours in '{s}'"),
            })?;
        let rest = &s[colon_pos + 1..];
        let (minutes, seconds) = if let Some(pos2) = rest.find(':') {
            let m: f64 = rest[..pos2].parse().map_err(|_| ParseError::InvalidField {
                field: ctx.to_string(),
                reason: format!("invalid minutes in '{s}'"),
            })?;
            let sec: f64 = rest[pos2 + 1..]
                .parse()
                .map_err(|_| ParseError::InvalidField {
                    field: ctx.to_string(),
                    reason: format!("invalid seconds in '{s}'"),
                })?;
            (m, sec)
        } else {
            let m: f64 = rest.parse().map_err(|_| ParseError::InvalidField {
                field: ctx.to_string(),
                reason: format!("invalid minutes in '{s}'"),
            })?;
            (m, 0.0)
        };
        Ok(hours * 3600.0 + minutes * 60.0 + seconds)
    } else {
        // Plain number — EPANET treats this as hours by default.
        // An optional second token may specify units: SECONDS, MINUTES, HOURS, DAYS.
        let value = parse_f64(s, ctx)?;
        if fields.len() > 1 {
            match fields[1].to_ascii_uppercase().as_str() {
                "SEC" | "SECONDS" => Ok(value),
                "MIN" | "MINUTES" => Ok(value * 60.0),
                "HOUR" | "HOURS" => Ok(value * 3600.0),
                "DAY" | "DAYS" => Ok(value * 86400.0),
                _ => Ok(value * 3600.0), // unknown unit → default hours
            }
        } else {
            Ok(value * 3600.0)
        }
    }
}

/// Parse a clocktime value like "12 am", "2:30 pm", or "13:00".
fn parse_clocktime(fields: &[&str]) -> f64 {
    if fields.is_empty() {
        return 0.0;
    }
    let s = fields[0];
    let base = if let Some(colon_pos) = s.find(':') {
        let h: f64 = s[..colon_pos].parse().unwrap_or(0.0);
        let m: f64 = s[colon_pos + 1..].parse().unwrap_or(0.0);
        h * 3600.0 + m * 60.0
    } else {
        s.parse::<f64>().unwrap_or(0.0) * 3600.0
    };

    // Check for AM/PM.
    if let Some(&suffix) = fields.get(1) {
        let u = suffix.to_ascii_uppercase();
        if u == "PM" && base < 12.0 * 3600.0 {
            return base + 12.0 * 3600.0;
        }
        if u == "AM" && base >= 12.0 * 3600.0 {
            return base - 12.0 * 3600.0;
        }
    }
    base
}

fn parse_premise_attr(s: &str) -> Result<PremiseAttribute, ParseError> {
    match s.to_ascii_uppercase().as_str() {
        "HEAD" | "GRADE" => Ok(PremiseAttribute::Head),
        "PRESSURE" => Ok(PremiseAttribute::Pressure),
        "DEMAND" => Ok(PremiseAttribute::Demand),
        "LEVEL" => Ok(PremiseAttribute::Level),
        "FLOW" => Ok(PremiseAttribute::Flow),
        "STATUS" => Ok(PremiseAttribute::Status),
        "SETTING" => Ok(PremiseAttribute::Setting),
        "POWER" => Ok(PremiseAttribute::Power),
        "FILLTIME" | "FILL_TIME" => Ok(PremiseAttribute::FillTime),
        "DRAINTIME" | "DRAIN_TIME" => Ok(PremiseAttribute::DrainTime),
        "CLOCKTIME" | "CLOCK_TIME" => Ok(PremiseAttribute::ClockTime),
        "TIME" => Ok(PremiseAttribute::Time),
        _ => Err(ParseError::InvalidField {
            field: "premise attribute".into(),
            reason: format!("unknown attribute '{s}'"),
        }),
    }
}

fn parse_premise_op(s: &str) -> Result<PremiseOperator, ParseError> {
    match s.to_ascii_uppercase().as_str() {
        "=" | "==" | "IS" | "EQUALS" => Ok(PremiseOperator::Eq),
        "<>" | "!=" | "NOT" => Ok(PremiseOperator::Neq),
        "<" | "BELOW" => Ok(PremiseOperator::Lt),
        ">" | "ABOVE" => Ok(PremiseOperator::Gt),
        "<=" => Ok(PremiseOperator::Le),
        ">=" => Ok(PremiseOperator::Ge),
        _ => Err(ParseError::InvalidField {
            field: "premise operator".into(),
            reason: format!("unknown operator '{s}'"),
        }),
    }
}

fn parse_premise_value(s: &str, attr: &PremiseAttribute) -> Result<f64, ParseError> {
    match attr {
        PremiseAttribute::Status => {
            // STATUS can be OPEN, CLOSED, or ACTIVE.
            match s.to_ascii_uppercase().as_str() {
                "OPEN" => Ok(1.0),
                "CLOSED" | "CLOSE" => Ok(0.0),
                "ACTIVE" => Ok(2.0),
                _ => parse_f64(s, "premise value"),
            }
        }
        PremiseAttribute::Time | PremiseAttribute::ClockTime => {
            // Time values may be in H:MM format.
            parse_time_value(&[s], "premise time value")
        }
        _ => parse_f64(s, "premise value"),
    }
}

// ── Coordinates ───────────────────────────────────────────────────────────────

/// Parses the [COORDINATES] section: `NodeID  X  Y`.
fn parse_coordinates(
    lines: &[SecLine<'_>],
    node_id_to_idx: &HashMap<String, usize>,
) -> Result<HashMap<String, (f64, f64)>, ParseError> {
    let mut coords = HashMap::new();
    for_each_line(lines, "COORDINATES", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            return Ok(());
        }
        let id = fields[0];
        // Only store coordinates for known nodes (silently skip unknown IDs,
        // matching EPANET behaviour).
        if !node_id_to_idx.contains_key(id) {
            return Ok(());
        }
        let x = parse_f64(fields[1], "COORDINATES X")?;
        let y = parse_f64(fields[2], "COORDINATES Y")?;
        coords.insert(id.to_string(), (x, y));
        Ok(())
    })?;
    Ok(coords)
}

// ── Vertices ──────────────────────────────────────────────────────────────────

/// Parses the [VERTICES] section: `LinkID  X  Y`.
/// Multiple lines with the same LinkID append successive bend-points.
fn parse_vertices(
    lines: &[SecLine<'_>],
    link_id_to_idx: &HashMap<String, usize>,
) -> Result<HashMap<String, Vec<(f64, f64)>>, ParseError> {
    let mut verts: HashMap<String, Vec<(f64, f64)>> = HashMap::new();
    for_each_line(lines, "VERTICES", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            return Ok(());
        }
        let id = fields[0];
        if !link_id_to_idx.contains_key(id) {
            return Ok(());
        }
        let x = parse_f64(fields[1], "VERTICES X")?;
        let y = parse_f64(fields[2], "VERTICES Y")?;
        verts.entry(id.to_string()).or_default().push((x, y));
        Ok(())
    })?;
    Ok(verts)
}

// ── Tags ──────────────────────────────────────────────────────────────────────

/// Parses the [TAGS] section: `NODE  <nodeid>  <tag>` or `LINK  <linkid>  <tag>`.
fn parse_tags(
    lines: &[SecLine<'_>],
    node_id_to_idx: &HashMap<String, usize>,
    link_id_to_idx: &HashMap<String, usize>,
) -> Result<TagMaps, ParseError> {
    let mut node_tags = HashMap::new();
    let mut link_tags = HashMap::new();
    for_each_line(lines, "TAGS", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            return Ok(());
        }
        let kind = fields[0].to_ascii_uppercase();
        let id = fields[1];
        let tag = fields[2];
        match kind.as_str() {
            "NODE" if node_id_to_idx.contains_key(id) => {
                node_tags.insert(id.to_string(), tag.to_string());
            }
            "NODE" => {}
            "LINK" if link_id_to_idx.contains_key(id) => {
                link_tags.insert(id.to_string(), tag.to_string());
            }
            "LINK" => {}
            _ => {} // silently skip unknown prefixes
        }
        Ok(())
    })?;
    Ok((node_tags, link_tags))
}

// ── Report ────────────────────────────────────────────────────────────────────

/// Known report field names (node + link fields from EPANET).
const REPORT_FIELD_NAMES: &[&str] = &[
    "ELEVATION",
    "DEMAND",
    "HEAD",
    "PRESSURE",
    "QUALITY",
    "LENGTH",
    "DIAMETER",
    "FLOW",
    "VELOCITY",
    "HEADLOSS",
    "LINKQUAL",
    "LINKSTATUS",
    "SETTING",
    "REACTRATE",
    "FRICTION",
];

/// Parses the [REPORT] section.
///
/// NOTE: the parsed options are stored in `Network::report` for API
/// completeness, but `rpt_writer` does not yet consult them — node/link
/// report tables and field filtering are not implemented.
fn parse_report(lines: &[SecLine<'_>]) -> Result<ReportOptions, ParseError> {
    let mut report = ReportOptions::default();
    for_each_line(lines, "REPORT", |line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.is_empty() {
            return Ok(());
        }
        let key = fields[0].to_ascii_uppercase();
        match key.as_str() {
            "PAGE" | "PAGESIZE" => {
                if let Some(val) = fields.get(1) {
                    if let Ok(n) = val.parse::<u32>() {
                        report.page_size = n;
                    }
                }
            }
            "STATUS" => {
                if let Some(val) = fields.get(1) {
                    report.status = match val.to_ascii_uppercase().as_str() {
                        "FULL" => ReportStatus::Full,
                        "YES" => ReportStatus::Yes,
                        _ => ReportStatus::No,
                    };
                }
            }
            "SUMMARY" => {
                if let Some(val) = fields.get(1) {
                    report.summary = !val.eq_ignore_ascii_case("NO");
                }
            }
            "MESSAGES" => {
                if let Some(val) = fields.get(1) {
                    report.messages = !val.eq_ignore_ascii_case("NO");
                }
            }
            "ENERGY" => {
                if let Some(val) = fields.get(1) {
                    report.energy = val.eq_ignore_ascii_case("YES");
                }
            }
            "NODES" => {
                if let Some(val) = fields.get(1) {
                    let upper = val.to_ascii_uppercase();
                    match upper.as_str() {
                        "NONE" => report.nodes = ReportSelection::None,
                        "ALL" => report.nodes = ReportSelection::All,
                        _ => {
                            // Collect all IDs from this line.
                            let ids: Vec<String> =
                                fields[1..].iter().map(|s| s.to_string()).collect();
                            match &mut report.nodes {
                                ReportSelection::Some(v) => v.extend(ids),
                                _ => report.nodes = ReportSelection::Some(ids),
                            }
                        }
                    }
                }
            }
            "LINKS" => {
                if let Some(val) = fields.get(1) {
                    let upper = val.to_ascii_uppercase();
                    match upper.as_str() {
                        "NONE" => report.links = ReportSelection::None,
                        "ALL" => report.links = ReportSelection::All,
                        _ => {
                            let ids: Vec<String> =
                                fields[1..].iter().map(|s| s.to_string()).collect();
                            match &mut report.links {
                                ReportSelection::Some(v) => v.extend(ids),
                                _ => report.links = ReportSelection::Some(ids),
                            }
                        }
                    }
                }
            }
            "FILE" => {
                if fields.len() > 1 {
                    report.file = Some(fields[1..].join(" "));
                }
            }
            _ => {
                // Check if this is a field-level option (e.g. "FLOW YES", "PRESSURE PRECISION 4").
                if REPORT_FIELD_NAMES.contains(&key.as_str()) {
                    let entry = report
                        .fields
                        .entry(key.clone())
                        .or_insert(ReportFieldOption {
                            enabled: true,
                            precision: None,
                            above: None,
                            below: None,
                        });
                    if let Some(val) = fields.get(1) {
                        let upper = val.to_ascii_uppercase();
                        match upper.as_str() {
                            "YES" => entry.enabled = true,
                            "NO" => entry.enabled = false,
                            "PRECISION" => {
                                if let Some(n) = fields.get(2).and_then(|s| s.parse::<u32>().ok()) {
                                    entry.precision = Some(n);
                                }
                            }
                            "BELOW" => {
                                if let Some(v) = fields.get(2).and_then(|s| s.parse::<f64>().ok()) {
                                    entry.below = Some(v);
                                }
                            }
                            "ABOVE" => {
                                if let Some(v) = fields.get(2).and_then(|s| s.parse::<f64>().ok()) {
                                    entry.above = Some(v);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(())
    })?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Attach synthetic 1-based line numbers to raw data lines, mirroring the
    /// `SecLine` shape produced by `split_sections`.
    fn secs<'a>(lines: &[&'a str]) -> Vec<SecLine<'a>> {
        lines.iter().enumerate().map(|(i, &l)| (i + 1, l)).collect()
    }

    // ── split_sections comment handling ──────────────────────────────────────

    #[test]
    fn full_line_comment_is_skipped() {
        let inp = "[JUNCTIONS]\n; this is a comment\nJ1  0  10\n";
        let sections = split_sections(inp);
        let lines = sections.get("JUNCTIONS").unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], (3, "J1  0  10"));
    }

    #[test]
    fn inline_comment_is_stripped() {
        let inp = "[JUNCTIONS]\nJ1  0  10  ; demand node\n";
        let sections = split_sections(inp);
        let lines = sections.get("JUNCTIONS").unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], (2, "J1  0  10"));
    }

    #[test]
    fn blank_lines_are_skipped() {
        let inp = "[JUNCTIONS]\n\n  \nJ1  0  10\n";
        let sections = split_sections(inp);
        let lines = sections.get("JUNCTIONS").unwrap();
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn title_preserves_semicolons() {
        let inp = "[TITLE]\nMy Network ; version 2\nSecond line\n";
        let sections = split_sections(inp);
        let lines = sections.get("TITLE").unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], (2, "My Network ; version 2"));
        assert_eq!(lines[1], (3, "Second line"));
    }

    #[test]
    fn title_skips_comment_only_and_blank_lines() {
        let inp = "[TITLE]\n; this is a comment\n\nActual title\n";
        let sections = split_sections(inp);
        let lines = sections.get("TITLE").unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], (4, "Actual title"));
    }

    #[test]
    fn parse_inp_preserves_title() {
        let inp = b"\
[TITLE]
EPANET Example Network 2
Example of modeling a 55-hour fluoride tracer study.
Measured fluoride data is in Net2-FL.dat

[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.title.len(), 3);
        assert_eq!(network.title[0], "EPANET Example Network 2");
        assert_eq!(
            network.title[1],
            "Example of modeling a 55-hour fluoride tracer study."
        );
        assert_eq!(network.title[2], "Measured fluoride data is in Net2-FL.dat");
    }

    #[test]
    fn parse_inp_extracts_chemical_name_and_units() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[OPTIONS]
Units    GPM
Headloss    H-W
Quality    Fluoride mg/L
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.options.chem_name, "Fluoride");
        assert_eq!(network.options.chem_units, "mg/L");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Duplicate ID detection (EPANET error 215)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Unwrap an `AtLine` error, returning `(section, line, inner)`.
    fn unwrap_at_line(err: ParseError) -> (String, usize, ParseError) {
        match err {
            ParseError::AtLine {
                section,
                line,
                source,
            } => (section, line, *source),
            other => panic!("expected AtLine error, got: {other}"),
        }
    }

    #[test]
    fn duplicate_junction_id_is_a_parse_error() {
        let inp = b"\
[JUNCTIONS]
J1    0    10
J1    5    20

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let err = parse_inp(inp).expect_err("duplicate junction ID should fail");
        let (section, line, inner) = unwrap_at_line(err);
        assert_eq!(section, "JUNCTIONS");
        assert_eq!(line, 3);
        assert!(
            matches!(inner, ParseError::DuplicateId { object: "node", ref id } if id == "J1"),
            "unexpected inner error: {inner}"
        );
    }

    #[test]
    fn duplicate_node_id_across_sections_is_a_parse_error() {
        // Same ID used for a junction and a tank.
        let inp = b"\
[JUNCTIONS]
N1    0    10

[RESERVOIRS]
R1    100

[TANKS]
N1    50    5    0    10    20    0

[PIPES]
P1    R1    N1    1000    12    100    0    Open

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let err = parse_inp(inp).expect_err("duplicate node ID across sections should fail");
        let (section, _line, inner) = unwrap_at_line(err);
        assert_eq!(section, "TANKS");
        assert!(
            matches!(inner, ParseError::DuplicateId { object: "node", ref id } if id == "N1"),
            "unexpected inner error: {inner}"
        );
    }

    #[test]
    fn duplicate_link_id_is_a_parse_error() {
        let inp = b"\
[JUNCTIONS]
J1    0    10
J2    0    5

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open
P1    J1    J2    500     12    100    0    Open

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let err = parse_inp(inp).expect_err("duplicate pipe ID should fail");
        let (section, _line, inner) = unwrap_at_line(err);
        assert_eq!(section, "PIPES");
        assert!(
            matches!(inner, ParseError::DuplicateId { object: "link", ref id } if id == "P1"),
            "unexpected inner error: {inner}"
        );
    }

    #[test]
    fn duplicate_link_id_across_sections_is_a_parse_error() {
        // Same ID used for a pipe and a valve.
        let inp = b"\
[JUNCTIONS]
J1    0    10
J2    0    5

[RESERVOIRS]
R1    100

[PIPES]
L1    R1    J1    1000    12    100    0    Open

[VALVES]
L1    J1    J2    12    PRV    50    0

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let err = parse_inp(inp).expect_err("duplicate link ID across sections should fail");
        let (section, _line, inner) = unwrap_at_line(err);
        assert_eq!(section, "VALVES");
        assert!(
            matches!(inner, ParseError::DuplicateId { object: "link", ref id } if id == "L1"),
            "unexpected inner error: {inner}"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Malformed data lines produce parse errors (spec §4.3)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn junction_line_with_too_few_fields_is_a_parse_error() {
        let inp = b"\
[JUNCTIONS]
J1

[OPTIONS]
Units    GPM
";
        let err = parse_inp(inp).expect_err("1-field junction line should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("[JUNCTIONS] line 2"),
            "error should locate the malformed line, got: {msg}"
        );
    }

    #[test]
    fn pipe_line_with_too_few_fields_is_a_parse_error() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000

[OPTIONS]
Units    GPM
";
        let err = parse_inp(inp).expect_err("4-field pipe line should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("[PIPES]"),
            "expected PIPES context, got: {msg}"
        );
        assert!(msg.contains("at least 6"), "got: {msg}");
    }

    #[test]
    fn pump_line_with_too_few_fields_is_a_parse_error() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PUMPS]
PMP1    R1    J1

[OPTIONS]
Units    GPM
";
        let err = parse_inp(inp).expect_err("3-field pump line should fail");
        assert!(err.to_string().contains("[PUMPS]"), "got: {err}");
    }

    #[test]
    fn valve_line_with_too_few_fields_is_a_parse_error() {
        let inp = b"\
[JUNCTIONS]
J1    0    10
J2    0    5

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[VALVES]
V1    J1    J2    12    PRV

[OPTIONS]
Units    GPM
";
        let err = parse_inp(inp).expect_err("5-field valve line should fail");
        assert!(err.to_string().contains("[VALVES]"), "got: {err}");
    }

    #[test]
    fn parse_error_display_includes_section_and_line_number() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    twelve    100    0    Open

[OPTIONS]
Units    GPM
";
        let err = parse_inp(inp).expect_err("non-numeric diameter should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("[PIPES] line 8"),
            "expected section+line context, got: {msg}"
        );
        assert!(msg.contains("twelve"), "got: {msg}");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // [OPTIONS] TRIALS / CHECKFREQ / MAXCHECK validation (spec §2.1)
    // ═══════════════════════════════════════════════════════════════════════════

    fn minimal_net_with_options(options: &str) -> Vec<u8> {
        format!(
            "[JUNCTIONS]\nJ1    0    10\n\n[RESERVOIRS]\nR1    100\n\n\
             [PIPES]\nP1    R1    J1    1000    12    100    0    Open\n\n\
             [OPTIONS]\nUnits    GPM\nHeadloss    H-W\n{options}\n"
        )
        .into_bytes()
    }

    #[test]
    fn trials_below_one_is_a_parse_error() {
        for bad in ["Trials 0", "Trials -3", "Trials NaN"] {
            let err = parse_inp(&minimal_net_with_options(bad))
                .expect_err("TRIALS < 1 should be rejected");
            assert!(err.to_string().contains("Trials"), "for '{bad}': {err}");
        }
        // Valid value still accepted.
        let network = parse_inp(&minimal_net_with_options("Trials 40")).unwrap();
        assert_eq!(network.options.max_iter, 40);
    }

    #[test]
    fn checkfreq_and_maxcheck_below_one_are_parse_errors() {
        let err = parse_inp(&minimal_net_with_options("CHECKFREQ 0"))
            .expect_err("CHECKFREQ 0 should be rejected");
        assert!(err.to_string().contains("CHECKFREQ"), "got: {err}");

        let err = parse_inp(&minimal_net_with_options("MAXCHECK -1"))
            .expect_err("MAXCHECK -1 should be rejected");
        assert!(err.to_string().contains("MAXCHECK"), "got: {err}");

        let network = parse_inp(&minimal_net_with_options("CHECKFREQ 4\nMAXCHECK 12")).unwrap();
        assert_eq!(network.options.check_freq, 4);
        assert_eq!(network.options.max_check, 12);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // [COORDINATES] section
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn parse_coordinates_basic() {
        let mut node_id_to_idx = HashMap::new();
        node_id_to_idx.insert("J1".to_string(), 1);
        node_id_to_idx.insert("J2".to_string(), 2);
        let lines = vec!["J1  100.0  200.0", "J2  300.0  400.0"];
        let coords = parse_coordinates(&secs(&lines), &node_id_to_idx).unwrap();
        assert_eq!(coords.len(), 2);
        assert_eq!(coords["J1"], (100.0, 200.0));
        assert_eq!(coords["J2"], (300.0, 400.0));
    }

    #[test]
    fn parse_coordinates_skips_unknown_nodes() {
        let mut node_id_to_idx = HashMap::new();
        node_id_to_idx.insert("J1".to_string(), 1);
        let lines = vec!["J1  10.0  20.0", "UNKNOWN  30.0  40.0"];
        let coords = parse_coordinates(&secs(&lines), &node_id_to_idx).unwrap();
        assert_eq!(coords.len(), 1);
        assert!(coords.contains_key("J1"));
        assert!(!coords.contains_key("UNKNOWN"));
    }

    #[test]
    fn parse_coordinates_skips_short_lines() {
        let node_id_to_idx = HashMap::new();
        let lines = vec!["J1  10.0"]; // only 2 fields, need 3
        let coords = parse_coordinates(&secs(&lines), &node_id_to_idx).unwrap();
        assert!(coords.is_empty());
    }

    #[test]
    fn parse_coordinates_negative_values() {
        let mut node_id_to_idx = HashMap::new();
        node_id_to_idx.insert("N1".to_string(), 1);
        let lines = vec!["N1  -50.5  -100.25"];
        let coords = parse_coordinates(&secs(&lines), &node_id_to_idx).unwrap();
        assert_eq!(coords["N1"], (-50.5, -100.25));
    }

    #[test]
    fn parse_coordinates_last_value_wins_for_duplicate() {
        let mut node_id_to_idx = HashMap::new();
        node_id_to_idx.insert("J1".to_string(), 1);
        let lines = vec!["J1  10.0  20.0", "J1  30.0  40.0"];
        let coords = parse_coordinates(&secs(&lines), &node_id_to_idx).unwrap();
        // Last line overwrites.
        assert_eq!(coords["J1"], (30.0, 40.0));
    }

    #[test]
    fn parse_inp_coordinates_section() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[COORDINATES]
J1   1000.00   2000.00
R1   500.00    3000.00

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.coordinates.len(), 2);
        assert_eq!(network.coordinates["J1"], (1000.0, 2000.0));
        assert_eq!(network.coordinates["R1"], (500.0, 3000.0));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // [VERTICES] section
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn parse_vertices_basic() {
        let mut link_id_to_idx = HashMap::new();
        link_id_to_idx.insert("P1".to_string(), 1);
        let lines = vec!["P1  100.0  200.0", "P1  300.0  400.0"];
        let verts = parse_vertices(&secs(&lines), &link_id_to_idx).unwrap();
        assert_eq!(verts.len(), 1);
        assert_eq!(verts["P1"], vec![(100.0, 200.0), (300.0, 400.0)]);
    }

    #[test]
    fn parse_vertices_multiple_links() {
        let mut link_id_to_idx = HashMap::new();
        link_id_to_idx.insert("P1".to_string(), 1);
        link_id_to_idx.insert("P2".to_string(), 2);
        let lines = vec!["P1  10.0  20.0", "P2  30.0  40.0", "P1  50.0  60.0"];
        let verts = parse_vertices(&secs(&lines), &link_id_to_idx).unwrap();
        assert_eq!(verts["P1"], vec![(10.0, 20.0), (50.0, 60.0)]);
        assert_eq!(verts["P2"], vec![(30.0, 40.0)]);
    }

    #[test]
    fn parse_vertices_skips_unknown_links() {
        let link_id_to_idx = HashMap::new();
        let lines = vec!["NOPE  10.0  20.0"];
        let verts = parse_vertices(&secs(&lines), &link_id_to_idx).unwrap();
        assert!(verts.is_empty());
    }

    #[test]
    fn parse_vertices_skips_short_lines() {
        let mut link_id_to_idx = HashMap::new();
        link_id_to_idx.insert("P1".to_string(), 1);
        let lines = vec!["P1  10.0"]; // only 2 fields, need 3
        let verts = parse_vertices(&secs(&lines), &link_id_to_idx).unwrap();
        assert!(verts.is_empty());
    }

    #[test]
    fn parse_inp_vertices_section() {
        let inp = b"\
[JUNCTIONS]
J1    0    10
J2    0    5

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open
P2    J1    J2    500     12    100    0    Open

[VERTICES]
P1  100.0  200.0
P1  150.0  250.0
P2  300.0  400.0

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.vertices.len(), 2);
        assert_eq!(network.vertices["P1"], vec![(100.0, 200.0), (150.0, 250.0)]);
        assert_eq!(network.vertices["P2"], vec![(300.0, 400.0)]);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // [TAGS] section
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn parse_tags_node_and_link() {
        let mut node_id_to_idx = HashMap::new();
        node_id_to_idx.insert("J1".to_string(), 1);
        let mut link_id_to_idx = HashMap::new();
        link_id_to_idx.insert("P1".to_string(), 1);
        let lines = vec!["NODE  J1  residential", "LINK  P1  main"];
        let (nt, lt) = parse_tags(&secs(&lines), &node_id_to_idx, &link_id_to_idx).unwrap();
        assert_eq!(nt["J1"], "residential");
        assert_eq!(lt["P1"], "main");
    }

    #[test]
    fn parse_tags_case_insensitive_prefix() {
        let mut node_id_to_idx = HashMap::new();
        node_id_to_idx.insert("J1".to_string(), 1);
        let link_id_to_idx = HashMap::new();
        let lines = vec!["node  J1  zone_A", "Node  J1  zone_B"]; // last wins
        let (nt, _lt) = parse_tags(&secs(&lines), &node_id_to_idx, &link_id_to_idx).unwrap();
        assert_eq!(nt["J1"], "zone_B");
    }

    #[test]
    fn parse_tags_skips_unknown_ids() {
        let node_id_to_idx = HashMap::new();
        let link_id_to_idx = HashMap::new();
        let lines = vec!["NODE  UNKNOWN  tag1", "LINK  UNKNOWN  tag2"];
        let (nt, lt) = parse_tags(&secs(&lines), &node_id_to_idx, &link_id_to_idx).unwrap();
        assert!(nt.is_empty());
        assert!(lt.is_empty());
    }

    #[test]
    fn parse_tags_skips_short_lines() {
        let node_id_to_idx = HashMap::new();
        let link_id_to_idx = HashMap::new();
        let lines = vec!["NODE  J1"]; // only 2 fields, need 3
        let (nt, lt) = parse_tags(&secs(&lines), &node_id_to_idx, &link_id_to_idx).unwrap();
        assert!(nt.is_empty());
        assert!(lt.is_empty());
    }

    #[test]
    fn parse_tags_skips_unknown_prefix() {
        let node_id_to_idx = HashMap::new();
        let link_id_to_idx = HashMap::new();
        let lines = vec!["BOGUS  J1  tag"];
        let (nt, lt) = parse_tags(&secs(&lines), &node_id_to_idx, &link_id_to_idx).unwrap();
        assert!(nt.is_empty());
        assert!(lt.is_empty());
    }

    #[test]
    fn parse_inp_tags_section() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[TAGS]
NODE  J1    residential
NODE  R1    source
LINK  P1    main_trunk

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.node_tags.len(), 2);
        assert_eq!(network.node_tags["J1"], "residential");
        assert_eq!(network.node_tags["R1"], "source");
        assert_eq!(network.link_tags.len(), 1);
        assert_eq!(network.link_tags["P1"], "main_trunk");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // [REPORT] section
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn parse_report_defaults() {
        let report = parse_report(&[]).unwrap();
        assert_eq!(report.page_size, 0);
        assert_eq!(report.status, ReportStatus::No);
        assert!(report.summary);
        assert!(report.messages);
        assert!(!report.energy);
        assert_eq!(report.nodes, ReportSelection::None);
        assert_eq!(report.links, ReportSelection::None);
        assert!(report.file.is_none());
        assert!(report.fields.is_empty());
    }

    #[test]
    fn parse_report_page_size() {
        let lines = vec!["PAGE  55"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(report.page_size, 55);
    }

    #[test]
    fn parse_report_status_yes() {
        let lines = vec!["STATUS  YES"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(report.status, ReportStatus::Yes);
    }

    #[test]
    fn parse_report_status_full() {
        let lines = vec!["STATUS  FULL"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(report.status, ReportStatus::Full);
    }

    #[test]
    fn parse_report_status_no() {
        let lines = vec!["STATUS  NO"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(report.status, ReportStatus::No);
    }

    #[test]
    fn parse_report_summary_no() {
        let lines = vec!["SUMMARY  NO"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert!(!report.summary);
    }

    #[test]
    fn parse_report_summary_yes() {
        let lines = vec!["SUMMARY  YES"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert!(report.summary);
    }

    #[test]
    fn parse_report_messages_no() {
        let lines = vec!["MESSAGES  NO"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert!(!report.messages);
    }

    #[test]
    fn parse_report_energy_yes() {
        let lines = vec!["ENERGY  YES"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert!(report.energy);
    }

    #[test]
    fn parse_report_energy_no() {
        let lines = vec!["ENERGY  NO"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert!(!report.energy);
    }

    #[test]
    fn parse_report_nodes_all() {
        let lines = vec!["NODES  ALL"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(report.nodes, ReportSelection::All);
    }

    #[test]
    fn parse_report_nodes_none() {
        let lines = vec!["NODES  NONE"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(report.nodes, ReportSelection::None);
    }

    #[test]
    fn parse_report_nodes_specific() {
        let lines = vec!["NODES  J1  J2  J3"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(
            report.nodes,
            ReportSelection::Some(vec!["J1".to_string(), "J2".to_string(), "J3".to_string()])
        );
    }

    #[test]
    fn parse_report_nodes_accumulate_across_lines() {
        let lines = vec!["NODES  J1  J2", "NODES  J3"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(
            report.nodes,
            ReportSelection::Some(vec!["J1".to_string(), "J2".to_string(), "J3".to_string()])
        );
    }

    #[test]
    fn parse_report_links_all() {
        let lines = vec!["LINKS  ALL"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(report.links, ReportSelection::All);
    }

    #[test]
    fn parse_report_links_specific() {
        let lines = vec!["LINKS  P1  P2"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(
            report.links,
            ReportSelection::Some(vec!["P1".to_string(), "P2".to_string()])
        );
    }

    #[test]
    fn parse_report_file() {
        let lines = vec!["FILE  output.rpt"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(report.file, Some("output.rpt".to_string()));
    }

    #[test]
    fn parse_report_file_with_spaces() {
        let lines = vec!["FILE  my output file.rpt"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(report.file, Some("my output file.rpt".to_string()));
    }

    #[test]
    fn parse_report_field_yes_no() {
        let lines = vec!["FLOW  YES", "PRESSURE  NO"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert!(report.fields["FLOW"].enabled);
        assert!(!report.fields["PRESSURE"].enabled);
    }

    #[test]
    fn parse_report_field_precision() {
        let lines = vec!["FLOW  PRECISION  4"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(report.fields["FLOW"].precision, Some(4));
    }

    #[test]
    fn parse_report_field_above_below() {
        let lines = vec!["PRESSURE  ABOVE  20.0", "VELOCITY  BELOW  0.5"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert_eq!(report.fields["PRESSURE"].above, Some(20.0));
        assert_eq!(report.fields["VELOCITY"].below, Some(0.5));
    }

    #[test]
    fn parse_report_all_field_names_recognized() {
        // Ensure all EPANET-defined field names are accepted.
        let lines: Vec<String> = REPORT_FIELD_NAMES
            .iter()
            .map(|name| format!("{}  YES", name))
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let report = parse_report(&secs(&line_refs)).unwrap();
        assert_eq!(report.fields.len(), REPORT_FIELD_NAMES.len());
        for name in REPORT_FIELD_NAMES {
            assert!(report.fields.contains_key(*name), "Missing field: {}", name);
            assert!(report.fields[*name].enabled);
        }
    }

    #[test]
    fn parse_report_unknown_keyword_ignored() {
        let lines = vec!["BOGUS  VALUE"];
        let report = parse_report(&secs(&lines)).unwrap();
        assert!(report.fields.is_empty());
    }

    #[test]
    fn parse_inp_report_section() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[REPORT]
PAGE  55
STATUS  FULL
SUMMARY  NO
ENERGY  YES
NODES  ALL
LINKS  P1
FLOW  PRECISION  3
PRESSURE  ABOVE  10.0

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.report.page_size, 55);
        assert_eq!(network.report.status, ReportStatus::Full);
        assert!(!network.report.summary);
        assert!(network.report.energy);
        assert_eq!(network.report.nodes, ReportSelection::All);
        assert_eq!(
            network.report.links,
            ReportSelection::Some(vec!["P1".to_string()])
        );
        assert_eq!(network.report.fields["FLOW"].precision, Some(3));
        assert_eq!(network.report.fields["PRESSURE"].above, Some(10.0));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // [TIMES] STATISTIC
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn parse_statistic_average() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[TIMES]
STATISTIC  AVERAGE

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.options.statistic, StatisticType::Average);
    }

    #[test]
    fn parse_statistic_minimum() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[TIMES]
STATISTICS  MINIMUM

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.options.statistic, StatisticType::Minimum);
    }

    #[test]
    fn parse_statistic_maximum() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[TIMES]
STATISTIC  MAXIMUM

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.options.statistic, StatisticType::Maximum);
    }

    #[test]
    fn parse_statistic_range() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[TIMES]
STATISTIC  RANGE

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.options.statistic, StatisticType::Range);
    }

    #[test]
    fn parse_statistic_none_is_series() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[TIMES]
STATISTIC  NONE

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.options.statistic, StatisticType::Series);
    }

    #[test]
    fn parse_statistic_default_is_series() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.options.statistic, StatisticType::Series);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // [ROUGHNESS] / [LABELS] / [BACKDROP] — graceful no-op handling
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn parse_inp_roughness_section_ignored() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[ROUGHNESS]
P1    120

[OPTIONS]
Units    GPM
Headloss    H-W
";
        // Should parse without error — [ROUGHNESS] is collected but not consumed.
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.nodes.len(), 2);
    }

    #[test]
    fn parse_inp_labels_section_ignored() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[LABELS]
100  200  \"Junction J1\"

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.nodes.len(), 2);
    }

    #[test]
    fn parse_inp_backdrop_section_ignored() {
        let inp = b"\
[JUNCTIONS]
J1    0    10

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    12    100    0    Open

[BACKDROP]
DIMENSIONS  0  0  10000  10000
FILE  background.bmp

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();
        assert_eq!(network.nodes.len(), 2);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Combined integration test — all new sections in one file
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn parse_inp_all_new_sections_combined() {
        let inp = b"\
[TITLE]
Integration Test Network

[JUNCTIONS]
J1    100    10
J2    90     5

[RESERVOIRS]
R1    200

[TANKS]
T1    150    7    5    10    20    0

[PIPES]
P1    R1    J1    1000    12    100    0    Open
P2    J1    J2    500     8     100    0    Open
P3    J2    T1    300     10    100    0    Open

[COORDINATES]
J1    1000.00    2000.00
J2    1500.00    2500.00
R1    500.00     1000.00
T1    2000.00    3000.00

[VERTICES]
P2    1200.00    2200.00
P2    1400.00    2400.00

[TAGS]
NODE  J1    residential
NODE  J2    commercial
NODE  R1    source
LINK  P1    main
LINK  P2    branch

[REPORT]
STATUS  YES
SUMMARY  NO
NODES  ALL
LINKS  ALL
FLOW  PRECISION  2
PRESSURE  ABOVE  5.0

[TIMES]
Duration    24:00
STATISTIC  AVERAGE

[LABELS]
100  200  \"Label 1\"

[BACKDROP]
DIMENSIONS  0  0  5000  5000

[ROUGHNESS]
P1    110

[OPTIONS]
Units    GPM
Headloss    H-W
";
        let network = parse_inp(inp).unwrap();

        // Coordinates
        assert_eq!(network.coordinates.len(), 4);
        assert_eq!(network.coordinates["J1"], (1000.0, 2000.0));
        assert_eq!(network.coordinates["T1"], (2000.0, 3000.0));

        // Vertices
        assert_eq!(network.vertices.len(), 1);
        assert_eq!(
            network.vertices["P2"],
            vec![(1200.0, 2200.0), (1400.0, 2400.0)]
        );

        // Tags
        assert_eq!(network.node_tags["J1"], "residential");
        assert_eq!(network.node_tags["J2"], "commercial");
        assert_eq!(network.node_tags["R1"], "source");
        assert_eq!(network.link_tags["P1"], "main");
        assert_eq!(network.link_tags["P2"], "branch");

        // Report
        assert_eq!(network.report.status, ReportStatus::Yes);
        assert!(!network.report.summary);
        assert_eq!(network.report.nodes, ReportSelection::All);
        assert_eq!(network.report.links, ReportSelection::All);
        assert_eq!(network.report.fields["FLOW"].precision, Some(2));
        assert_eq!(network.report.fields["PRESSURE"].above, Some(5.0));

        // Statistic
        assert_eq!(network.options.statistic, StatisticType::Average);

        // [ROUGHNESS], [LABELS], [BACKDROP] — no error, no side effects
        assert_eq!(network.nodes.len(), 4);
        assert_eq!(network.links.len(), 3);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Syntactic robustness — separators, casing, ordering, line endings
    // ═══════════════════════════════════════════════════════════════════════════

    /// A minimal valid network in SI (CMS) units so that most internal values
    /// equal their user-facing values (all length/flow factors are 1.0).
    const MINIMAL_CMS: &str = "\
[JUNCTIONS]
J1    0    0.5

[RESERVOIRS]
R1    100

[PIPES]
P1    R1    J1    1000    300    100    0    Open

[OPTIONS]
Units    CMS
Headloss    H-W
";

    #[test]
    fn crlf_line_endings_parse_identically() {
        let unix = MINIMAL_CMS.as_bytes().to_vec();
        let dos = MINIMAL_CMS.replace('\n', "\r\n").into_bytes();

        let net_unix = parse_inp(&unix).expect("LF input parses");
        let net_dos = parse_inp(&dos).expect("CRLF input parses");

        assert_eq!(net_unix.nodes.len(), net_dos.nodes.len());
        assert_eq!(net_unix.links.len(), net_dos.links.len());
        assert_eq!(
            net_unix.nodes[0].base.elevation,
            net_dos.nodes[0].base.elevation
        );
    }

    #[test]
    fn crlf_error_line_numbers_match_lf_input() {
        // The same malformed junction line must be reported at the same
        // 1-based line number regardless of the line-ending convention.
        let bad = "[JUNCTIONS]\nJ1    zero    10\n";
        let err_lf = parse_inp(bad.as_bytes()).expect_err("bad elev");
        let err_crlf =
            parse_inp(bad.replace('\n', "\r\n").as_bytes()).expect_err("bad elev (CRLF)");
        let (sec_lf, line_lf, _) = unwrap_at_line(err_lf);
        let (sec_crlf, line_crlf, _) = unwrap_at_line(err_crlf);
        assert_eq!((sec_lf.as_str(), line_lf), ("JUNCTIONS", 2));
        assert_eq!((sec_crlf.as_str(), line_crlf), ("JUNCTIONS", 2));
    }

    #[test]
    fn tab_separated_fields_parse() {
        let inp = "[JUNCTIONS]\nJ1\t0\t0.25\n\n[RESERVOIRS]\nR1\t100\n\n\
                   [PIPES]\nP1\tR1\tJ1\t1000\t300\t100\t0\tOpen\n\n\
                   [OPTIONS]\nUnits\tCMS\nHeadloss\tH-W\n";
        let network = parse_inp(inp.as_bytes()).expect("tab-separated input parses");
        assert_eq!(network.nodes.len(), 2);
        assert_eq!(network.links.len(), 1);
    }

    #[test]
    fn mixed_case_section_headers_and_keywords_parse() {
        let inp = "[Junctions]\nJ1    0    0.5\n\n[reservoirs]\nR1    100\n\n\
                   [PiPeS]\nP1    R1    J1    1000    300    100    0    open\n\n\
                   [options]\nunits    cms\nheadloss    h-w\n";
        let network = parse_inp(inp.as_bytes()).expect("mixed-case input parses");
        assert_eq!(network.nodes.len(), 2);
        assert_eq!(network.options.flow_units, FlowUnits::Cms);
        assert_eq!(
            network.options.head_loss_formula,
            HeadLossFormula::HazenWilliams
        );
    }

    #[test]
    fn indented_section_headers_and_data_lines_parse() {
        let inp = "   [JUNCTIONS]\n   J1    0    0.5   \n\n\t[RESERVOIRS]\n\tR1    100\n\n\
                   [PIPES]\n  P1    R1    J1    1000    300    100    0    Open  \n\n\
                   [OPTIONS]\n  Units    CMS\n  Headloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).expect("indented input parses");
        assert_eq!(network.nodes.len(), 2);
        assert_eq!(network.links.len(), 1);
    }

    #[test]
    fn duplicated_section_headers_merge_contents() {
        // EPANET files sometimes repeat a section; lines accumulate.
        let inp = "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
                   [JUNCTIONS]\nJ2    0    0.25\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [PIPES]\nP2    J1    J2    500    300    100    0    Open\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).expect("duplicated sections parse");
        assert_eq!(network.nodes.len(), 3);
        assert_eq!(network.links.len(), 2);
    }

    #[test]
    fn sections_in_unusual_order_parse() {
        // Links physically before the nodes they reference; OPTIONS first.
        let inp = "[OPTIONS]\nUnits    CMS\nHeadloss    H-W\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n";
        let network = parse_inp(inp.as_bytes()).expect("out-of-order sections parse");
        assert_eq!(network.nodes.len(), 2);
        assert_eq!(network.links.len(), 1);
    }

    #[test]
    fn blank_sections_are_ok() {
        let inp = "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
                   [TANKS]\n\n[PUMPS]\n\n[VALVES]\n\n[DEMANDS]\n\n[STATUS]\n\n\
                   [PATTERNS]\n\n[CURVES]\n\n[CONTROLS]\n\n[RULES]\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).expect("blank sections parse");
        assert_eq!(network.nodes.len(), 2);
        assert!(network.controls.is_empty());
        assert!(network.rules.is_empty());
    }

    #[test]
    fn content_after_end_marker_is_ignored() {
        let inp = format!("{MINIMAL_CMS}\n[END]\nthis is not INP data ]][[\nJ9 xx yy\n");
        let network = parse_inp(inp.as_bytes()).expect("content after [END] ignored");
        assert_eq!(network.nodes.len(), 2);
    }

    #[test]
    fn ids_with_special_characters_parse() {
        let inp = "[JUNCTIONS]\nJ-1.a_b/2    0    0.5\n\n[RESERVOIRS]\nR&1    100\n\n\
                   [PIPES]\nP#1@x    R&1    J-1.a_b/2    1000    300    100    0    Open\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).expect("special-char IDs parse");
        assert!(network.nodes.iter().any(|n| n.base.id == "J-1.a_b/2"));
        assert!(network.nodes.iter().any(|n| n.base.id == "R&1"));
        assert!(network.links.iter().any(|l| l.base.id == "P#1@x"));
    }

    #[test]
    fn very_long_pattern_line_parses() {
        let factors: Vec<String> = (0..120).map(|i| format!("{}.5", i % 3)).collect();
        let inp = format!(
            "[JUNCTIONS]\nJ1    0    0.5    PAT1\n\n[RESERVOIRS]\nR1    100\n\n\
             [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
             [PATTERNS]\nPAT1    {}\n\n[OPTIONS]\nUnits    CMS\nHeadloss    H-W\n",
            factors.join("    ")
        );
        let network = parse_inp(inp.as_bytes()).expect("long pattern line parses");
        let pat = network
            .patterns
            .iter()
            .find(|p| p.id == "PAT1")
            .expect("PAT1 present");
        assert_eq!(pat.factors.len(), 120);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Section semantics — [DEMANDS], [SOURCES], [QUALITY], valves, tanks, pumps
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn first_demands_entry_replaces_junction_demand_then_appends() {
        // EPANET semantics: the first [DEMANDS] entry for a junction REPLACES
        // the [JUNCTIONS] demand; subsequent entries append.
        let inp = "[JUNCTIONS]\nJ1    0    10\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [DEMANDS]\nJ1    5\nJ1    3\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let junction = network
            .nodes
            .iter()
            .find_map(|n| match &n.kind {
                NodeKind::Junction(j) if n.base.id == "J1" => Some(j),
                _ => None,
            })
            .expect("J1 exists");
        assert_eq!(junction.demands.len(), 2, "replace-then-append semantics");
        assert!((junction.demands[0].base_demand - 5.0).abs() < 1e-12);
        assert!((junction.demands[1].base_demand - 3.0).abs() < 1e-12);
    }

    #[test]
    fn demand_category_name_is_captured() {
        let inp = "[JUNCTIONS]\nJ1    0    10\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [PATTERNS]\nPAT1    1.0    0.5\n\n\
                   [DEMANDS]\nJ1    5    PAT1    Domestic use\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let junction = network
            .nodes
            .iter()
            .find_map(|n| match &n.kind {
                NodeKind::Junction(j) if n.base.id == "J1" => Some(j),
                _ => None,
            })
            .expect("J1 exists");
        assert_eq!(junction.demands.len(), 1);
        assert_eq!(junction.demands[0].pattern.as_deref(), Some("PAT1"));
        assert_eq!(junction.demands[0].name.as_deref(), Some("Domestic use"));
    }

    #[test]
    fn source_type_omitted_defaults_to_concentration() {
        let inp = "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [SOURCES]\nR1    2.5\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\nQuality    Chlorine mg/L\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let src = network
            .nodes
            .iter()
            .find(|n| n.base.id == "R1")
            .and_then(|n| n.source.as_ref())
            .expect("R1 has a source");
        assert_eq!(src.kind, SourceType::Concentration);
        assert!((src.base_value - 2.5).abs() < 1e-12);
        assert!(src.pattern.is_none());
    }

    #[test]
    fn source_unknown_type_is_a_parse_error() {
        let inp = "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [SOURCES]\nR1    BOGUS    2.5\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let err = parse_inp(inp.as_bytes()).expect_err("unknown source type rejected");
        let (section, _line, inner) = unwrap_at_line(err);
        assert_eq!(section, "SOURCES");
        assert!(
            inner.to_string().contains("unknown source type"),
            "got: {inner}"
        );
    }

    #[test]
    fn quality_numeric_range_assigns_nodes_in_range() {
        let inp = "[JUNCTIONS]\n1    0    0.5\n2    0    0.25\n\n[RESERVOIRS]\n7    100\n\n\
                   [PIPES]\nP1    7    1    1000    300    100    0    Open\n\
                   P2    1    2    500    300    100    0    Open\n\n\
                   [QUALITY]\n1    2    0.8\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\nQuality    Chlorine mg/L\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let quality_of = |id: &str| {
            network
                .nodes
                .iter()
                .find(|n| n.base.id == id)
                .map(|n| n.base.initial_quality)
                .unwrap()
        };
        assert_eq!(quality_of("1"), 0.8);
        assert_eq!(quality_of("2"), 0.8);
        assert_eq!(quality_of("7"), 0.0, "node outside the range is untouched");
    }

    #[test]
    fn quality_lexicographic_range_assigns_nodes_in_range() {
        let inp = "[JUNCTIONS]\nNA    0    0.5\nNB    0    0.25\n\n[RESERVOIRS]\nNZ    100\n\n\
                   [PIPES]\nP1    NZ    NA    1000    300    100    0    Open\n\
                   P2    NA    NB    500    300    100    0    Open\n\n\
                   [QUALITY]\nNA    NB    0.6\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\nQuality    Chlorine mg/L\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let quality_of = |id: &str| {
            network
                .nodes
                .iter()
                .find(|n| n.base.id == id)
                .map(|n| n.base.initial_quality)
                .unwrap()
        };
        assert_eq!(quality_of("NA"), 0.6);
        assert_eq!(quality_of("NB"), 0.6);
        assert_eq!(quality_of("NZ"), 0.0);
    }

    #[test]
    fn gpv_setting_field_is_a_curve_id() {
        let inp = "[JUNCTIONS]\nJ1    0    0\nJ2    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    300    300    100    0    Open\n\
                   P2    J2    R1    300    300    100    0    Open\n\n\
                   [VALVES]\nV1    J1    J2    300    GPV    HC1    0\n\n\
                   [CURVES]\nHC1    0    0\nHC1    0.5    5\nHC1    1.0    5\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let (base, valve) = network
            .links
            .iter()
            .find_map(|l| match &l.kind {
                LinkKind::Valve(v) if l.base.id == "V1" => Some((&l.base, v)),
                _ => None,
            })
            .expect("V1 exists");
        assert_eq!(valve.valve_type, ValveType::Gpv);
        assert_eq!(valve.curve.as_deref(), Some("HC1"));
        assert_eq!(base.initial_status, LinkStatus::Active);
        let curve = network.curves.iter().find(|c| c.id == "HC1").unwrap();
        assert_eq!(curve.kind, CurveKind::GpvHeadloss);
    }

    #[test]
    fn pcv_eighth_field_is_a_loss_ratio_curve_id() {
        let inp = "[JUNCTIONS]\nJ1    0    0.2\nJ2    0    0.2\n\n\
                   [RESERVOIRS]\nR1    200\nR2    50\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\
                   P2    J2    R2    1000    300    100    0    Open\n\n\
                   [VALVES]\nV1    J1    J2    300    PCV    50    5    LC1\n\n\
                   [CURVES]\nLC1    0    0\nLC1    50    40\nLC1    100    100\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let (base, valve) = network
            .links
            .iter()
            .find_map(|l| match &l.kind {
                LinkKind::Valve(v) if l.base.id == "V1" => Some((&l.base, v)),
                _ => None,
            })
            .expect("V1 exists");
        assert_eq!(valve.valve_type, ValveType::Pcv);
        assert_eq!(valve.curve.as_deref(), Some("LC1"));
        assert_eq!(base.initial_setting, Some(50.0), "percent-open setting");
        let curve = network.curves.iter().find(|c| c.id == "LC1").unwrap();
        assert_eq!(curve.kind, CurveKind::PcvLossRatio);
    }

    #[test]
    fn pipe_seven_field_form_status_keyword_or_minor_loss() {
        // With exactly 7 fields, field 7 may be a status keyword (CV/OPEN/
        // CLOSED) or a numeric minor-loss value.
        let inp = "[JUNCTIONS]\nJ1    0    0.2\nJ2    0    0.2\nJ3    0    0.1\n\n\
                   [RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\n\
                   PCV1    R1    J1    1000    300    100    CV\n\
                   PCLS    J1    J2    1000    300    100    Closed\n\
                   PNUM    J2    J3    1000    300    100    2.5\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let pipe = |id: &str| {
            network
                .links
                .iter()
                .find_map(|l| match &l.kind {
                    LinkKind::Pipe(p) if l.base.id == id => Some((&l.base, p)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{id} exists"))
        };
        let (cv_base, cv) = pipe("PCV1");
        assert!(cv.check_valve);
        assert_eq!(cv_base.initial_status, LinkStatus::Open);
        assert_eq!(cv.minor_loss, 0.0);

        let (cls_base, cls) = pipe("PCLS");
        assert!(!cls.check_valve);
        assert_eq!(cls_base.initial_status, LinkStatus::Closed);
        assert_eq!(cls.minor_loss, 0.0);

        let (num_base, num) = pipe("PNUM");
        assert!(!num.check_valve);
        assert_eq!(num_base.initial_status, LinkStatus::Open);
        assert!(num.minor_loss > 0.0, "numeric field is a minor loss");
    }

    #[test]
    fn tank_star_volume_curve_placeholder_means_no_curve() {
        let inp = "[RESERVOIRS]\nR1    200\n\n\
                   [TANKS]\nT1    50    5    0    10    20    0    *    YES\n\n\
                   [PIPES]\nP1    R1    T1    500    300    100    0    Open\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let tank = network
            .nodes
            .iter()
            .find_map(|n| match &n.kind {
                NodeKind::Tank(t) if n.base.id == "T1" => Some(t),
                _ => None,
            })
            .expect("T1 exists");
        assert!(tank.volume_curve.is_none(), "'*' is an empty placeholder");
        assert!(tank.overflow, "9th field YES enables overflow");
    }

    #[test]
    fn pump_head_speed_and_pattern_keywords_combine() {
        let inp = "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    10\n\n\
                   [PUMPS]\nPU1    R1    J1    HEAD    C1    SPEED    0.8    PATTERN    SPAT\n\n\
                   [PATTERNS]\nSPAT    1.0    0.5    0.0\n\n\
                   [CURVES]\nC1    0    30\nC1    0.5    20\nC1    1.0    5\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let (base, pump) = network
            .links
            .iter()
            .find_map(|l| match &l.kind {
                LinkKind::Pump(p) if l.base.id == "PU1" => Some((&l.base, p)),
                _ => None,
            })
            .expect("PU1 exists");
        assert_eq!(pump.head_curve.as_deref(), Some("C1"));
        assert_eq!(pump.speed_pattern.as_deref(), Some("SPAT"));
        assert_eq!(base.initial_setting, Some(0.8));
        assert_eq!(pump.curve_type, PumpCurveType::PowerFunction);
    }

    #[test]
    fn single_point_pump_curve_expands_to_three_points() {
        // EPANET expands (Q1, H1) to (0, 1.33334·H1), (Q1, H1), (2·Q1, 0).
        let inp = "[JUNCTIONS]\nJ1    0    0.3\n\n[RESERVOIRS]\nR1    10\n\n\
                   [PUMPS]\nPU1    R1    J1    HEAD    C1\n\n\
                   [CURVES]\nC1    0.5    20\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let curve = network.curves.iter().find(|c| c.id == "C1").unwrap();
        assert_eq!(curve.points.len(), 3);
        assert!((curve.points[0].x - 0.0).abs() < 1e-12);
        assert!((curve.points[0].y - PUMP_SHUTOFF_HEAD_FACTOR * 20.0).abs() < 1e-9);
        assert!((curve.points[1].x - 0.5).abs() < 1e-12);
        assert!((curve.points[1].y - 20.0).abs() < 1e-12);
        assert!((curve.points[2].x - 1.0).abs() < 1e-12);
        assert!((curve.points[2].y - 0.0).abs() < 1e-12);
    }

    #[test]
    fn status_section_sets_pump_speed_and_valve_status() {
        let inp = "[JUNCTIONS]\nJ1    0    0.2\nJ2    0    0.2\n\n\
                   [RESERVOIRS]\nR1    10\nR2    50\n\n\
                   [PUMPS]\nPU1    R1    J1    HEAD    C1\n\n\
                   [VALVES]\nV1    J1    J2    300    TCV    4    0\n\n\
                   [PIPES]\nP2    J2    R2    500    300    100    0    Open\n\n\
                   [CURVES]\nC1    0    60\nC1    0.5    40\nC1    1.0    10\n\n\
                   [STATUS]\nPU1    0.5\nV1    CLOSED\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let network = parse_inp(inp.as_bytes()).unwrap();
        let link = |id: &str| {
            network
                .links
                .iter()
                .find(|l| l.base.id == id)
                .unwrap_or_else(|| panic!("{id} exists"))
        };
        assert_eq!(link("PU1").base.initial_setting, Some(0.5));
        assert_eq!(link("V1").base.initial_status, LinkStatus::Closed);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // [OPTIONS] / [TIMES] matrix
    // ═══════════════════════════════════════════════════════════════════════════

    /// Minimal CMS-units network with extra [OPTIONS] lines, so option values
    /// that pass through unit conversion keep their user-facing magnitudes.
    fn minimal_cms_with_options(options: &str) -> Vec<u8> {
        format!(
            "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
             [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
             [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n{options}\n"
        )
        .into_bytes()
    }

    /// Same, but with extra [TIMES] lines.
    fn minimal_cms_with_times(times: &str) -> Vec<u8> {
        format!(
            "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
             [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
             [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n\n[TIMES]\n{times}\n"
        )
        .into_bytes()
    }

    #[test]
    fn unbalanced_stop_and_continue_forms() {
        let net = parse_inp(&minimal_cms_with_options("Unbalanced    STOP")).unwrap();
        assert_eq!(net.options.extra_iter, -1);

        let net = parse_inp(&minimal_cms_with_options("Unbalanced    CONTINUE    5")).unwrap();
        assert_eq!(net.options.extra_iter, 5);

        let net = parse_inp(&minimal_cms_with_options("Unbalanced    CONTINUE")).unwrap();
        assert_eq!(net.options.extra_iter, 0);
    }

    #[test]
    fn htol_and_qtol_are_stored_in_internal_units() {
        // CMS: elevation and flow factors are 1.0, so values pass through.
        let net = parse_inp(&minimal_cms_with_options("HTOL    0.005\nQTOL    0.002")).unwrap();
        assert!((net.options.head_tol - 0.005).abs() < 1e-12);
        assert!((net.options.flow_change_tol - 0.002).abs() < 1e-12);
    }

    #[test]
    fn solver_tuning_options_parse() {
        let net = parse_inp(&minimal_cms_with_options(
            "RQTOL    1e-5\nDAMPLIMIT    0.01\nFLOWCHANGE    0.001\nHEADERROR    0.002\nAccuracy    0.005",
        ))
        .unwrap();
        assert!((net.options.rq_tol - 1e-5).abs() < 1e-18);
        assert!((net.options.damp_limit - 0.01).abs() < 1e-12);
        assert!((net.options.flow_change_limit - 0.001).abs() < 1e-12);
        assert!((net.options.head_error_limit - 0.002).abs() < 1e-12);
        assert!((net.options.flow_tol - 0.005).abs() < 1e-12);
    }

    #[test]
    fn tolerance_option_sets_quality_tolerance() {
        let net = parse_inp(&minimal_cms_with_options("Tolerance    0.05")).unwrap();
        assert!((net.options.quality_tolerance - 0.05).abs() < 1e-12);
    }

    #[test]
    fn backflow_allowed_yes_no_and_invalid() {
        let net = parse_inp(&minimal_cms_with_options("Backflow Allowed    YES")).unwrap();
        assert!(net.options.emitter_backflow);

        let net = parse_inp(&minimal_cms_with_options("Backflow Allowed    NO")).unwrap();
        assert!(!net.options.emitter_backflow);

        let err = parse_inp(&minimal_cms_with_options("Backflow Allowed    MAYBE"))
            .expect_err("invalid BACKFLOW ALLOWED value rejected");
        assert!(err.to_string().contains("BACKFLOW ALLOWED"), "got: {err}");
    }

    #[test]
    fn demand_multiplier_and_specific_gravity_options_parse() {
        let net = parse_inp(&minimal_cms_with_options(
            "Demand Multiplier    1.5\nSpecific Gravity    0.9",
        ))
        .unwrap();
        assert!((net.options.demand_multiplier - 1.5).abs() < 1e-12);
        assert!((net.options.specific_gravity - 0.9).abs() < 1e-12);
    }

    #[test]
    fn emitter_exponent_option_applies_to_all_junctions() {
        let net = parse_inp(&minimal_cms_with_options("Emitter Exponent    0.75")).unwrap();
        for node in &net.nodes {
            if let NodeKind::Junction(ref j) = node.kind {
                assert!((j.emitter_exp - 0.75).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn default_pattern_option_and_implicit_pattern_one() {
        // With an explicit PATTERN option referencing a defined pattern, no
        // implicit pattern "1" is created.
        let inp = "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [PATTERNS]\nPAT1    1.0    0.5\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\nPattern    PAT1\n";
        let net = parse_inp(inp.as_bytes()).unwrap();
        assert_eq!(net.options.default_pattern.as_deref(), Some("PAT1"));
        assert!(!net.patterns.iter().any(|p| p.id == "1"));

        // Without a PATTERN option, EPANET defaults to pattern "1" and
        // implicitly creates an all-1.0 pattern of that name.
        let net = parse_inp(MINIMAL_CMS.as_bytes()).unwrap();
        assert_eq!(net.options.default_pattern.as_deref(), Some("1"));
        let implicit = net.patterns.iter().find(|p| p.id == "1").unwrap();
        assert_eq!(implicit.factors, vec![1.0]);
    }

    #[test]
    fn unknown_option_keyword_is_ignored() {
        // Deliberate deviation (spec.md §4.3): unknown [OPTIONS] keywords are
        // skipped for forward compatibility.
        let net = parse_inp(&minimal_cms_with_options(
            "FROBNICATE    42\nMAP    city.map",
        ))
        .unwrap();
        assert_eq!(net.nodes.len(), 2);
    }

    #[test]
    fn times_plain_numbers_and_unit_suffixes() {
        let net = parse_inp(&minimal_cms_with_times(
            "Duration    30    MINUTES\nHydraulic Timestep    600    SECONDS",
        ))
        .unwrap();
        assert_eq!(net.options.duration, 1800.0);
        assert_eq!(net.options.hyd_step, 600.0);

        // A bare number is hours.
        let net = parse_inp(&minimal_cms_with_times("Duration    2")).unwrap();
        assert_eq!(net.options.duration, 7200.0);

        let net = parse_inp(&minimal_cms_with_times("Duration    1    DAYS")).unwrap();
        assert_eq!(net.options.duration, 86400.0);
    }

    #[test]
    fn times_hms_form_parses_seconds() {
        let net = parse_inp(&minimal_cms_with_times("Duration    1:30:30")).unwrap();
        assert_eq!(net.options.duration, 5430.0);
    }

    #[test]
    fn times_start_clocktime_am_pm() {
        let net = parse_inp(&minimal_cms_with_times("Start Clocktime    2:30 PM")).unwrap();
        assert_eq!(net.options.start_clocktime, 14.5 * 3600.0);

        let net = parse_inp(&minimal_cms_with_times("Start Clocktime    12 AM")).unwrap();
        assert_eq!(net.options.start_clocktime, 0.0);

        let net = parse_inp(&minimal_cms_with_times("Start Clocktime    12 PM")).unwrap();
        assert_eq!(net.options.start_clocktime, 12.0 * 3600.0);
    }

    #[test]
    fn times_pattern_and_report_start_parse() {
        let net = parse_inp(&minimal_cms_with_times(
            "Duration    24:00\nPattern Start    2:00\nReport Start    1:00\nRule Timestep    0:05",
        ))
        .unwrap();
        assert_eq!(net.options.pattern_start, 7200.0);
        assert_eq!(net.options.report_start, 3600.0);
        assert_eq!(net.options.rule_timestep, 300.0);
    }

    #[test]
    fn hydraulic_step_is_capped_to_pattern_and_report_steps() {
        // Mirrors EPANET adjustdata(): Hstep = min(Hstep, Pstep, Rstep).
        let net = parse_inp(&minimal_cms_with_times(
            "Duration    24:00\nHydraulic Timestep    2:00\nPattern Timestep    1:00\nReport Timestep    1:00",
        ))
        .unwrap();
        assert_eq!(net.options.hyd_step, 3600.0);
    }

    #[test]
    fn quality_and_rule_timesteps_default_to_tenth_of_hydraulic_step() {
        let net = parse_inp(&minimal_cms_with_times(
            "Duration    24:00\nHydraulic Timestep    1:00\nPattern Timestep    1:00\nReport Timestep    1:00",
        ))
        .unwrap();
        assert_eq!(net.options.qual_step, 360.0);
        assert_eq!(net.options.rule_timestep, 360.0);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Error paths — undefined references, malformed values, validation failures
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn pipe_referencing_unknown_node_is_a_parse_error() {
        let inp = "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    NOPE    1000    300    100    0    Open\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let err = parse_inp(inp.as_bytes()).expect_err("unknown node ref rejected");
        let (section, _line, inner) = unwrap_at_line(err);
        assert_eq!(section, "PIPES");
        assert!(
            inner.to_string().contains("unknown node ID 'NOPE'"),
            "got: {inner}"
        );
    }

    #[test]
    fn status_referencing_unknown_link_is_a_parse_error() {
        let inp = "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [STATUS]\nNOPE    CLOSED\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let err = parse_inp(inp.as_bytes()).expect_err("unknown link ref rejected");
        let (section, _line, inner) = unwrap_at_line(err);
        assert_eq!(section, "STATUS");
        assert!(
            inner.to_string().contains("unknown link ID 'NOPE'"),
            "got: {inner}"
        );
    }

    #[test]
    fn demands_referencing_unknown_junction_is_a_parse_error() {
        let inp = "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [DEMANDS]\nNOPE    5\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let err = parse_inp(inp.as_bytes()).expect_err("unknown junction ref rejected");
        let (section, _line, inner) = unwrap_at_line(err);
        assert_eq!(section, "DEMANDS");
        assert!(
            inner.to_string().contains("unknown node ID 'NOPE'"),
            "got: {inner}"
        );
    }

    #[test]
    fn junction_elevation_not_numeric_is_a_parse_error() {
        let inp = "[JUNCTIONS]\nJ1    zero    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let err = parse_inp(inp.as_bytes()).expect_err("non-numeric elevation rejected");
        let (section, line, inner) = unwrap_at_line(err);
        assert_eq!(section, "JUNCTIONS");
        assert_eq!(line, 2);
        let msg = inner.to_string();
        assert!(
            msg.contains("JUNCTIONS.Elev") && msg.contains("'zero'"),
            "got: {msg}"
        );
    }

    #[test]
    fn unknown_valve_type_is_a_parse_error() {
        let inp = "[JUNCTIONS]\nJ1    0    0.2\nJ2    0    0.2\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [VALVES]\nV1    J1    J2    300    XYZ    50    0\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let err = parse_inp(inp.as_bytes()).expect_err("unknown valve type rejected");
        let (section, _line, inner) = unwrap_at_line(err);
        assert_eq!(section, "VALVES");
        assert!(
            inner.to_string().contains("unknown valve type 'XYZ'"),
            "got: {inner}"
        );
    }

    #[test]
    fn unknown_flow_units_is_a_parse_error() {
        let err = parse_inp(&minimal_net_with_options("Units    FURLONGS"))
            .expect_err("unknown units rejected");
        assert!(
            err.to_string().contains("unknown flow unit 'FURLONGS'"),
            "got: {err}"
        );
    }

    #[test]
    fn unknown_headloss_formula_is_a_parse_error() {
        let err = parse_inp(&minimal_net_with_options("Headloss    X-Y"))
            .expect_err("unknown headloss formula rejected");
        assert!(
            err.to_string().contains("unknown formula 'X-Y'"),
            "got: {err}"
        );
    }

    #[test]
    fn junction_only_network_fails_validation_with_no_reservoir() {
        let inp = "[JUNCTIONS]\nJ1    0    0.5\nJ2    0    0.25\n\n\
                   [PIPES]\nP1    J1    J2    1000    300    100    0    Open\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let err = parse_inp(inp.as_bytes()).expect_err("no fixed-grade node rejected");
        match err {
            ParseError::ValidationFailed(errors) => {
                assert!(
                    errors
                        .iter()
                        .any(|e| matches!(e, crate::ValidationError::NoReservoir)),
                    "expected NoReservoir, got: {errors:?}"
                );
            }
            other => panic!("expected ValidationFailed, got: {other}"),
        }
    }

    #[test]
    fn self_loop_pipe_fails_validation() {
        let inp = "[JUNCTIONS]\nJ1    0    0.5\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\
                   P2    J1    J1    500    300    100    0    Open\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let err = parse_inp(inp.as_bytes()).expect_err("self-loop rejected");
        match err {
            ParseError::ValidationFailed(errors) => {
                assert!(
                    errors
                        .iter()
                        .any(|e| matches!(e, crate::ValidationError::LinkSelfLoop { .. })),
                    "expected LinkSelfLoop, got: {errors:?}"
                );
            }
            other => panic!("expected ValidationFailed, got: {other}"),
        }
    }

    #[test]
    fn undefined_demand_pattern_fails_validation() {
        let inp = "[JUNCTIONS]\nJ1    0    0.5    PAT9\n\n[RESERVOIRS]\nR1    100\n\n\
                   [PIPES]\nP1    R1    J1    1000    300    100    0    Open\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let err = parse_inp(inp.as_bytes()).expect_err("undefined pattern rejected");
        match err {
            ParseError::ValidationFailed(errors) => {
                assert!(
                    errors.iter().any(|e| matches!(
                        e,
                        crate::ValidationError::UnknownPatternRef { pattern_id, .. }
                            if pattern_id == "PAT9"
                    )),
                    "expected UnknownPatternRef for PAT9, got: {errors:?}"
                );
            }
            other => panic!("expected ValidationFailed, got: {other}"),
        }
    }

    #[test]
    fn undefined_tank_volume_curve_fails_validation() {
        let inp = "[RESERVOIRS]\nR1    200\n\n\
                   [TANKS]\nT1    50    5    0    10    20    0    VC9\n\n\
                   [PIPES]\nP1    R1    T1    500    300    100    0    Open\n\n\
                   [OPTIONS]\nUnits    CMS\nHeadloss    H-W\n";
        let err = parse_inp(inp.as_bytes()).expect_err("undefined volume curve rejected");
        match err {
            ParseError::ValidationFailed(errors) => {
                assert!(
                    errors.iter().any(|e| matches!(
                        e,
                        crate::ValidationError::UnknownCurveRef { curve_id, .. }
                            if curve_id == "VC9"
                    )),
                    "expected UnknownCurveRef for VC9, got: {errors:?}"
                );
            }
            other => panic!("expected ValidationFailed, got: {other}"),
        }
    }
}
