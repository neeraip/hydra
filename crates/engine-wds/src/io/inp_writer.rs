// inp_writer — Serialize a `Network` back to EPANET 2.3 INP format.
//
// Internal values are stored in SI units (m, m³/s, W).
// The writer converts them back to the user-declared unit system using `make_ucf`
// (the inverse of what `units::apply_unit_conversion` does on load).
//
// Conversion summary (internal → user):
//   elevation / length  :  internal_m    * ucf.elev        (m → ft/m)
//   pipe diameter       :  internal_m    * ucf.diam        (m → inches/mm)
//   tank/valve diameter :  internal_m    * ucf.elev        (m → ft/m; tank diam uses elev conv)
//   flow / demand       :  internal_m3s  * ucf.flow        (m³/s → gpm/lps/…)
//   pump power          :  internal_W    * ucf.power       (W → HP/kW)
//   bulk/wall coeff     :  internal_per_s * 86400           (per-s → per-day)
//   DW roughness        :  internal_m     * 1000 * ucf.elev (m → mm / milli-ft)
//   minor loss K        :  reverse of K_m = 0.08262*K_v/D⁴ → K_v = K_m*D⁴/0.08262
//   pressure head       :  internal_m    * ucf.pressure    (m → psi/m)
//   valve setting       :  pressure or flow conv (type-dependent)
//   tank elevation INP  :  node.base.elevation - tank.min_level (bottom elevation)

use std::fmt::Write as _;

use super::units::make_ucf;
use crate::{
    ActionValue, CurveKind, HeadLossFormula, LinkKind, LinkStatus, LogicOp, MixModel, Network,
    NodeKind, PremiseAttribute, PremiseObject, PremiseOperator, QualityMode, ReportSelection,
    ReportStatus, SourceType, StatisticType, TriggerType, ValveType, WallOrder,
};

/// Write `network` to EPANET 2.3 INP bytes.
///
/// All values are converted from the internal unit system back to the user unit
/// system declared by `network.options.flow_units`.
pub fn write_inp(network: &Network) -> Vec<u8> {
    let mut out = String::with_capacity(64 * 1024);
    let ucf = make_ucf(network.options.flow_units, network.options.specific_gravity);
    let is_dw = network.options.head_loss_formula == HeadLossFormula::DarcyWeisbach;

    // Build fast lookup tables for 1-based indices → IDs.
    let node_id: Vec<&str> = {
        let mut v = vec![""; network.nodes.len() + 1]; // index 0 unused
        for n in &network.nodes {
            if n.base.index < v.len() {
                v[n.base.index] = &n.base.id;
            }
        }
        v
    };
    let link_id: Vec<&str> = {
        let mut v = vec![""; network.links.len() + 1];
        for l in &network.links {
            if l.base.index < v.len() {
                v[l.base.index] = &l.base.id;
            }
        }
        v
    };

    // ── [TITLE] ──────────────────────────────────────────────────────────────
    if !network.title.is_empty() {
        out.push_str("[TITLE]\n");
        for line in &network.title {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }

    // ── [JUNCTIONS] ──────────────────────────────────────────────────────────
    {
        let junctions: Vec<_> = network
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Junction(_)))
            .collect();
        if !junctions.is_empty() {
            out.push_str("[JUNCTIONS]\n");
            out.push_str(";ID               Elev          Demand        Pattern\n");
            for n in &junctions {
                if let NodeKind::Junction(ref j) = n.kind {
                    let elev = n.base.elevation * ucf.elev;
                    let base_demand = j
                        .demands
                        .first()
                        .map(|d| d.base_demand * ucf.flow)
                        .unwrap_or(0.0);
                    let pattern = j
                        .demands
                        .first()
                        .and_then(|d| d.pattern.as_deref())
                        .unwrap_or("");
                    let _ = writeln!(
                        out,
                        " {:<16} {:>12.4} {:>12.4}   {}",
                        n.base.id, elev, base_demand, pattern
                    );
                }
            }
            out.push('\n');
        }
    }

    // ── [RESERVOIRS] ─────────────────────────────────────────────────────────
    {
        let reservoirs: Vec<_> = network
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Reservoir(_)))
            .collect();
        if !reservoirs.is_empty() {
            out.push_str("[RESERVOIRS]\n");
            out.push_str(";ID               Head          Pattern\n");
            for n in &reservoirs {
                if let NodeKind::Reservoir(ref r) = n.kind {
                    let head = n.base.elevation * ucf.elev;
                    let pattern = r.head_pattern.as_deref().unwrap_or("");
                    let _ = writeln!(out, " {:<16} {:>12.4}   {}", n.base.id, head, pattern);
                }
            }
            out.push('\n');
        }
    }

    // ── [TANKS] ──────────────────────────────────────────────────────────────
    {
        let tanks: Vec<_> = network
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Tank(_)))
            .collect();
        if !tanks.is_empty() {
            out.push_str("[TANKS]\n");
            out.push_str(";ID               Elevation     InitLevel     MinLevel      MaxLevel      Diameter      MinVol        VolCurve      Overflow\n");
            for n in &tanks {
                if let NodeKind::Tank(ref t) = n.kind {
                    // INP bottom elevation = node elevation (m) − min_level (m)
                    let bottom_ft = n.base.elevation - t.min_level;
                    let bottom_user = bottom_ft * ucf.elev;
                    let init = t.initial_level * ucf.elev;
                    let min_l = t.min_level * ucf.elev;
                    let max_l = t.max_level * ucf.elev;
                    // Tank diameter uses length conversion, not pipe-diameter conv.
                    let diam = t.diameter * ucf.elev;
                    let min_v = t.min_volume * ucf.vol;
                    // When the overflow flag must be emitted but there is no
                    // volume curve, fill the VolCurve column with the `*`
                    // placeholder (spec §4.3) so whitespace splitting keeps
                    // YES in the 9th (Overflow) field on re-parse.
                    let vol_curve =
                        t.volume_curve
                            .as_deref()
                            .unwrap_or(if t.overflow { "*" } else { "" });
                    let overflow = if t.overflow { "YES" } else { "" };
                    let _ =
                        writeln!(out,
                        " {:<16} {:>12.4} {:>12.4} {:>12.4} {:>12.4} {:>12.4} {:>12.4}   {:<14}{}",
                        n.base.id, bottom_user, init, min_l, max_l, diam, min_v,
                        vol_curve, overflow);
                }
            }
            out.push('\n');
        }
    }

    // ── [PIPES] ──────────────────────────────────────────────────────────────
    {
        let pipes: Vec<_> = network
            .links
            .iter()
            .filter(|l| matches!(l.kind, LinkKind::Pipe(_)))
            .collect();
        if !pipes.is_empty() {
            out.push_str("[PIPES]\n");
            out.push_str(";ID               Node1         Node2         Length        Diameter      Roughness     MinorLoss     Status\n");
            for l in &pipes {
                if let LinkKind::Pipe(ref p) = l.kind {
                    let from = node_id.get(l.base.from_node).copied().unwrap_or("");
                    let to = node_id.get(l.base.to_node).copied().unwrap_or("");
                    let len = p.length * ucf.elev;
                    // Pipe diameter uses pipe-diameter conversion (inches/mm).
                    let diam = p.diameter * ucf.diam;
                    // DW roughness is in mm (SI) or milli-ft (US); HW is dimensionless.
                    let rough = if is_dw {
                        p.roughness * 1000.0 * ucf.elev
                    } else {
                        p.roughness
                    };
                    // Minor loss: reverse K_m = 0.08262 * K_v / D⁴
                    let minor = if p.minor_loss > 0.0 {
                        let d4 = p.diameter.powi(4);
                        p.minor_loss * d4 / 0.08262
                    } else {
                        0.0
                    };
                    // Check-valve pipes are always CV status.
                    let status = if p.check_valve {
                        "CV"
                    } else {
                        link_status_str(l.base.initial_status)
                    };
                    let _ = writeln!(
                        out,
                        " {:<16} {:<14} {:<14} {:>12.4} {:>12.4} {:>12.4} {:>12.4}  {}",
                        l.base.id, from, to, len, diam, rough, minor, status
                    );
                }
            }
            out.push('\n');
        }
    }

    // ── [PUMPS] ──────────────────────────────────────────────────────────────
    {
        let pumps: Vec<_> = network
            .links
            .iter()
            .filter(|l| matches!(l.kind, LinkKind::Pump(_)))
            .collect();
        if !pumps.is_empty() {
            out.push_str("[PUMPS]\n");
            out.push_str(";ID               Node1         Node2         Parameters\n");
            for l in &pumps {
                if let LinkKind::Pump(ref p) = l.kind {
                    let from = node_id.get(l.base.from_node).copied().unwrap_or("");
                    let to = node_id.get(l.base.to_node).copied().unwrap_or("");
                    // Build keyword-value pairs.
                    let mut params = String::new();
                    if let Some(ref curve_id) = p.head_curve {
                        let _ = write!(params, " HEAD {}", curve_id);
                    }
                    if let Some(pw) = p.power {
                        let pw_user = pw * ucf.power;
                        let _ = write!(params, " POWER {:.4}", pw_user);
                    }
                    if let Some(speed) = l.base.initial_setting {
                        if (speed - 1.0).abs() > 1e-9 {
                            let _ = write!(params, " SPEED {:.4}", speed);
                        }
                    }
                    if let Some(ref pat) = p.speed_pattern {
                        let _ = write!(params, " PATTERN {}", pat);
                    }
                    let _ = writeln!(out, " {:<16} {:<14} {:<14}{}", l.base.id, from, to, params);
                }
            }
            out.push('\n');
        }
    }

    // ── [VALVES] ─────────────────────────────────────────────────────────────
    {
        let valves: Vec<_> = network
            .links
            .iter()
            .filter(|l| matches!(l.kind, LinkKind::Valve(_)))
            .collect();
        if !valves.is_empty() {
            out.push_str("[VALVES]\n");
            out.push_str(";ID               Node1         Node2         Diameter      Type      Setting       MinorLoss\n");
            for l in &valves {
                if let LinkKind::Valve(ref v) = l.kind {
                    let from = node_id.get(l.base.from_node).copied().unwrap_or("");
                    let to = node_id.get(l.base.to_node).copied().unwrap_or("");
                    let diam = v.diameter * ucf.diam;
                    let vtype = valve_type_str(v.valve_type);
                    // Setting: convert back to user units depending on type.
                    let setting_user = l
                        .base
                        .initial_setting
                        .map(|s| {
                            match v.valve_type {
                                ValveType::Prv | ValveType::Psv | ValveType::Pbv => {
                                    s * ucf.pressure
                                }
                                ValveType::Fcv => s * ucf.flow,
                                _ => s, // TCV, GPV, PCV: dimensionless or curve-based
                            }
                        })
                        .unwrap_or(0.0);
                    // GPV: the Setting column holds the head-loss curve ID, not
                    // a number (spec §4.3); the reader parses it as a curve ref.
                    let setting_str = match (v.valve_type, v.curve.as_deref()) {
                        (ValveType::Gpv, Some(curve_id)) => curve_id.to_string(),
                        _ => format!("{:.4}", setting_user),
                    };
                    // Minor loss reverse.
                    let minor = if v.minor_loss > 0.0 {
                        let d4 = v.diameter.powi(4);
                        v.minor_loss * d4 / 0.08262
                    } else {
                        0.0
                    };
                    // PCV: the loss-ratio curve ID is the optional 8th column.
                    let pcv_curve = match (v.valve_type, v.curve.as_deref()) {
                        (ValveType::Pcv, Some(curve_id)) => format!("  {}", curve_id),
                        _ => String::new(),
                    };
                    let _ = writeln!(
                        out,
                        " {:<16} {:<14} {:<14} {:>12.4} {:<8} {:>12} {:>12.4}{}",
                        l.base.id, from, to, diam, vtype, setting_str, minor, pcv_curve
                    );
                }
            }
            out.push('\n');
        }
    }

    // ── [TAGS] ───────────────────────────────────────────────────────────────
    if !network.node_tags.is_empty() || !network.link_tags.is_empty() {
        out.push_str("[TAGS]\n");
        for n in &network.nodes {
            if let Some(tag) = network.node_tags.get(&n.base.id) {
                let _ = writeln!(out, " NODE  {:<16} {}", n.base.id, tag);
            }
        }
        for l in &network.links {
            if let Some(tag) = network.link_tags.get(&l.base.id) {
                let _ = writeln!(out, " LINK  {:<16} {}", l.base.id, tag);
            }
        }
        out.push('\n');
    }

    // ── [DEMANDS] ────────────────────────────────────────────────────────────
    // Junctions with ≥ 2 demand categories emit ALL categories here, including
    // the first (which also lives in [JUNCTIONS]). The reader's EPANET
    // semantics make the first [DEMANDS] line REPLACE the [JUNCTIONS]-derived
    // category and subsequent lines APPEND, so the same category list is
    // reconstructed (spec §4.3). Single-category junctions stay
    // [JUNCTIONS]-only.
    {
        let mut any = false;
        for n in &network.nodes {
            if let NodeKind::Junction(ref j) = n.kind {
                if j.demands.len() > 1 {
                    any = true;
                    break;
                }
            }
        }
        if any {
            out.push_str("[DEMANDS]\n");
            out.push_str(";Junction        Demand        Pattern       Category\n");
            for n in &network.nodes {
                if let NodeKind::Junction(ref j) = n.kind {
                    if j.demands.len() < 2 {
                        continue;
                    }
                    for d in &j.demands {
                        let demand = d.base_demand * ucf.flow;
                        let pattern = d.pattern.as_deref().unwrap_or("");
                        // The category name is the 4th whitespace-delimited
                        // field; with an empty Pattern column it would shift
                        // into the pattern position, so it is only written
                        // when a pattern is present (spec §4.3).
                        let name = if pattern.is_empty() {
                            ""
                        } else {
                            d.name.as_deref().unwrap_or("")
                        };
                        let _ = writeln!(
                            out,
                            " {:<16} {:>12.4}   {:<14}{}",
                            n.base.id, demand, pattern, name
                        );
                    }
                }
            }
            out.push('\n');
        }
    }

    // ── [STATUS] ─────────────────────────────────────────────────────────────
    // Only emit links that are explicitly Closed (pipes) or have a pump/valve
    // that differs from the default Open status.
    {
        let mut status_lines: Vec<String> = Vec::new();
        for l in &network.links {
            match &l.kind {
                LinkKind::Pipe(_) => {
                    if l.base.initial_status == LinkStatus::Closed {
                        status_lines.push(format!(" {:<16} Closed", l.base.id));
                    }
                }
                LinkKind::Pump(_) => {
                    if l.base.initial_status == LinkStatus::Closed {
                        status_lines.push(format!(" {:<16} Closed", l.base.id));
                    }
                }
                LinkKind::Valve(_) => {
                    // Valves default to Active; only emit if explicitly Open/Closed.
                    match l.base.initial_status {
                        LinkStatus::Open => status_lines.push(format!(" {:<16} Open", l.base.id)),
                        LinkStatus::Closed => {
                            status_lines.push(format!(" {:<16} Closed", l.base.id))
                        }
                        _ => {}
                    }
                }
            }
        }
        if !status_lines.is_empty() {
            out.push_str("[STATUS]\n");
            out.push_str(";ID               Status\n");
            for line in status_lines {
                out.push_str(&line);
                out.push('\n');
            }
            out.push('\n');
        }
    }

    // ── [PATTERNS] ───────────────────────────────────────────────────────────
    if !network.patterns.is_empty() {
        out.push_str("[PATTERNS]\n");
        out.push_str(";ID               Multipliers\n");
        for pat in &network.patterns {
            // Write 6 factors per line.
            let chunks: Vec<_> = pat.factors.chunks(6).collect();
            for chunk in chunks.iter() {
                let vals: Vec<String> = chunk.iter().map(|f| format!("{:.4}", f)).collect();
                let _ = writeln!(out, " {:<16} {}", pat.id, vals.join("   "));
            }
        }
        out.push('\n');
    }

    // ── [CURVES] ─────────────────────────────────────────────────────────────
    if !network.curves.is_empty() {
        out.push_str("[CURVES]\n");
        out.push_str(";ID               X-Value       Y-Value\n");
        for curve in &network.curves {
            // Emit a type comment for known curve kinds.
            let kind_comment = match curve.kind {
                CurveKind::PumpHead => Some(";PUMP"),
                CurveKind::PumpEfficiency => Some(";EFFICIENCY"),
                CurveKind::TankVolume => Some(";VOLUME"),
                CurveKind::GpvHeadloss => Some(";HEADLOSS"),
                _ => None,
            };
            if let Some(cmt) = kind_comment {
                let _ = writeln!(out, "{}", cmt);
            }
            for pt in &curve.points {
                // Convert back from internal to user units per curve kind.
                let (xu, yu) = match curve.kind {
                    CurveKind::PumpHead => (pt.x * ucf.flow, pt.y * ucf.elev),
                    CurveKind::PumpEfficiency => (pt.x * ucf.flow, pt.y),
                    CurveKind::TankVolume => (pt.x * ucf.elev, pt.y * ucf.vol),
                    CurveKind::GpvHeadloss => (pt.x * ucf.flow, pt.y * ucf.elev),
                    _ => (pt.x, pt.y),
                };
                let _ = writeln!(out, " {:<16} {:>12.4} {:>12.4}", curve.id, xu, yu);
            }
        }
        out.push('\n');
    }

    // ── [CONTROLS] ───────────────────────────────────────────────────────────
    if !network.controls.is_empty() {
        out.push_str("[CONTROLS]\n");
        for ctrl in &network.controls {
            if !ctrl.enabled {
                continue;
            }
            let link_id_str = link_id.get(ctrl.link).copied().unwrap_or("?");
            // Action part.
            let action_str = match (ctrl.action_status, ctrl.action_setting) {
                (Some(LinkStatus::Open), _) => "OPEN".to_string(),
                (Some(LinkStatus::Closed), _) => "CLOSED".to_string(),
                (_, Some(s)) => {
                    // Setting: reverse valve conversion if applicable.
                    let link_setting_user =
                        if let Some(link) = network.links.get(ctrl.link.saturating_sub(1)) {
                            if let LinkKind::Valve(ref v) = link.kind {
                                match v.valve_type {
                                    ValveType::Prv | ValveType::Psv | ValveType::Pbv => {
                                        s * ucf.pressure
                                    }
                                    ValveType::Fcv => s * ucf.flow,
                                    _ => s,
                                }
                            } else {
                                s // pump speed — dimensionless
                            }
                        } else {
                            s
                        };
                    format!("{:.4}", link_setting_user)
                }
                _ => continue,
            };

            let trigger_str = match ctrl.trigger_type {
                TriggerType::Timer => {
                    let secs = ctrl.trigger_time.unwrap_or(0.0);
                    format!("AT TIME {}", fmt_duration_hm(secs))
                }
                TriggerType::TimeOfDay => {
                    let secs = ctrl.trigger_time.unwrap_or(0.0);
                    format!("AT CLOCKTIME {}", fmt_clocktime(secs))
                }
                TriggerType::HiLevel | TriggerType::LowLevel => {
                    let node_idx = ctrl.trigger_node.unwrap_or(0);
                    let node_id_str = node_id.get(node_idx).copied().unwrap_or("?");
                    let dir = if ctrl.trigger_type == TriggerType::HiLevel {
                        "ABOVE"
                    } else {
                        "BELOW"
                    };
                    let grade_internal = ctrl.trigger_grade.unwrap_or(0.0);
                    // Convert back to user units.
                    let grade_user =
                        if let Some(node) = network.nodes.get(node_idx.saturating_sub(1)) {
                            match &node.kind {
                                NodeKind::Tank(ref t) => {
                                    let bottom = node.base.elevation - t.min_level;
                                    (grade_internal - bottom) * ucf.elev
                                }
                                _ => (grade_internal - node.base.elevation) * ucf.pressure,
                            }
                        } else {
                            grade_internal
                        };
                    format!("IF NODE {} {} {:.4}", node_id_str, dir, grade_user)
                }
            };

            let _ = writeln!(out, " LINK {} {} {}", link_id_str, action_str, trigger_str);
        }
        out.push('\n');
    }

    // ── [RULES] ──────────────────────────────────────────────────────────────
    if !network.rules.is_empty() {
        out.push_str("[RULES]\n");
        for (ri, rule) in network.rules.iter().enumerate() {
            let _ = writeln!(out, " RULE R{}", ri + 1);
            for (pi, prem) in rule.premises.iter().enumerate() {
                let connective = if pi == 0 {
                    "IF"
                } else {
                    match prem.connective {
                        Some(LogicOp::And) | None => "AND",
                        Some(LogicOp::Or) => "OR",
                    }
                };
                let obj_str = match prem.object {
                    PremiseObject::Node(idx) => {
                        let nid = node_id.get(idx).copied().unwrap_or("?");
                        format!("NODE {}", nid)
                    }
                    PremiseObject::Link(idx) => {
                        let lid = link_id.get(idx).copied().unwrap_or("?");
                        format!("LINK {}", lid)
                    }
                    PremiseObject::Clock => "SYSTEM".to_string(),
                };
                let attr_str = premise_attr_str(prem.attribute);
                let op_str = premise_op_str(prem.operator);
                let value_user = convert_premise_value(prem, &ucf);
                let _ = writeln!(
                    out,
                    " {} {} {} {} {:.4}",
                    connective, obj_str, attr_str, op_str, value_user
                );
            }
            for action in &rule.then_actions {
                let lid = link_id.get(action.link).copied().unwrap_or("?");
                let val = rule_action_str(&action.value, action.link, &network.links, &ucf);
                let _ = writeln!(out, " THEN LINK {} {}", lid, val);
            }
            for action in &rule.else_actions {
                let lid = link_id.get(action.link).copied().unwrap_or("?");
                let val = rule_action_str(&action.value, action.link, &network.links, &ucf);
                let _ = writeln!(out, " ELSE LINK {} {}", lid, val);
            }
            if rule.priority != 0.0 {
                let _ = writeln!(out, " PRIORITY {:.4}", rule.priority);
            }
            out.push('\n');
        }
        out.push('\n');
    }

    // ── [ENERGY] ─────────────────────────────────────────────────────────────
    {
        let opts = &network.options;
        let mut energy_lines: Vec<String> = Vec::new();
        let default_eff = opts.energy_efficiency * 100.0; // fraction → %
        if (default_eff - 75.0).abs() > 1e-6 {
            energy_lines.push(format!(" Global Efficiency   {:.4}", default_eff));
        }
        if opts.energy_price > 0.0 {
            energy_lines.push(format!(" Global Price        {:.4}", opts.energy_price));
        }
        if let Some(ref pat) = opts.energy_price_pattern {
            energy_lines.push(format!(" Global Pattern      {}", pat));
        }
        if opts.peak_demand_charge > 0.0 {
            energy_lines.push(format!(
                " Demand Charge       {:.4}",
                opts.peak_demand_charge
            ));
        }
        // Per-pump energy parameters.
        for l in &network.links {
            if let LinkKind::Pump(ref p) = l.kind {
                if let Some(price) = p.energy_price {
                    energy_lines.push(format!(" Pump  {}  Price  {:.4}", l.base.id, price));
                }
                if let Some(ref pat) = p.price_pattern {
                    energy_lines.push(format!(" Pump  {}  Pattern  {}", l.base.id, pat));
                }
                if let Some(ref eff) = p.efficiency_curve {
                    energy_lines.push(format!(" Pump  {}  Efficiency  {}", l.base.id, eff));
                }
            }
        }
        if !energy_lines.is_empty() {
            out.push_str("[ENERGY]\n");
            for line in energy_lines {
                out.push_str(&line);
                out.push('\n');
            }
            out.push('\n');
        }
    }

    // ── [REACTIONS] ──────────────────────────────────────────────────────────
    {
        let opts = &network.options;
        let mut rxn_lines: Vec<String> = Vec::new();
        // Wall-type coefficients carry a wall-order-dependent length factor on
        // disk (quality spec §6.5.2). Apply the exact inverse of the read
        // conversion in units.rs (order 1: /86400/ucf.elev; order 0:
        // /86400*ucf.elev²) so values are stable across save/load cycles.
        let wall_to_user = |kw: f64| match opts.wall_order {
            WallOrder::One => kw * 86400.0 * ucf.elev,
            WallOrder::Zero => kw * 86400.0 / (ucf.elev * ucf.elev),
        };
        if opts.bulk_order != 1.0 {
            rxn_lines.push(format!(" Order Bulk   {:.4}", opts.bulk_order));
        }
        if opts.tank_order != 1.0 {
            rxn_lines.push(format!(" Order Tank   {:.4}", opts.tank_order));
        }
        if opts.wall_order != WallOrder::One {
            rxn_lines.push(format!(
                " Order Wall   {}",
                match opts.wall_order {
                    WallOrder::Zero => 0,
                    WallOrder::One => 1,
                }
            ));
        }
        if opts.bulk_coeff != 0.0 {
            rxn_lines.push(format!(" Global Bulk  {:.4}", opts.bulk_coeff * 86400.0));
        }
        if opts.wall_coeff != 0.0 {
            rxn_lines.push(format!(
                " Global Wall  {:.4}",
                wall_to_user(opts.wall_coeff)
            ));
        }
        if opts.conc_limit != 0.0 {
            rxn_lines.push(format!(" Limiting Potential  {:.4}", opts.conc_limit));
        }
        if opts.roughness_reaction_factor != 0.0 {
            // Shares kw's dimensions, so the same inverse conversion applies.
            rxn_lines.push(format!(
                " Roughness Correlation  {:.4}",
                wall_to_user(opts.roughness_reaction_factor)
            ));
        }
        // Per-pipe reactions.
        for l in &network.links {
            if let LinkKind::Pipe(ref p) = l.kind {
                if let Some(kb) = p.bulk_coeff {
                    rxn_lines.push(format!(" Bulk  {:<16} {:.4}", l.base.id, kb * 86400.0));
                }
                if let Some(kw) = p.wall_coeff {
                    rxn_lines.push(format!(" Wall  {:<16} {:.4}", l.base.id, wall_to_user(kw)));
                }
            }
        }
        // Per-tank reactions.
        for n in &network.nodes {
            if let NodeKind::Tank(ref t) = n.kind {
                if t.bulk_coeff != 0.0 {
                    rxn_lines.push(format!(
                        " Tank  {:<16} {:.4}",
                        n.base.id,
                        t.bulk_coeff * 86400.0
                    ));
                }
            }
        }
        if !rxn_lines.is_empty() {
            out.push_str("[REACTIONS]\n");
            for line in rxn_lines {
                out.push_str(&line);
                out.push('\n');
            }
            out.push('\n');
        }
    }

    // ── [SOURCES] ────────────────────────────────────────────────────────────
    {
        let sources: Vec<_> = network
            .nodes
            .iter()
            .filter(|n| n.source.is_some())
            .collect();
        if !sources.is_empty() {
            out.push_str("[SOURCES]\n");
            out.push_str(";Node             Type          Quality       Pattern\n");
            for n in &sources {
                if let Some(ref src) = n.source {
                    let src_type = match src.kind {
                        SourceType::Concentration => "CONCEN",
                        SourceType::Mass => "MASS",
                        SourceType::Setpoint => "SETPOINT",
                        SourceType::FlowPaced => "FLOWPACED",
                    };
                    let pattern = src.pattern.as_deref().unwrap_or("");
                    let _ = writeln!(
                        out,
                        " {:<16} {:<14} {:>12.4}   {}",
                        n.base.id, src_type, src.base_value, pattern
                    );
                }
            }
            out.push('\n');
        }
    }

    // ── [MIXING] ─────────────────────────────────────────────────────────────
    {
        let non_default: Vec<_> = network
            .nodes
            .iter()
            .filter(|n| {
                if let NodeKind::Tank(ref t) = n.kind {
                    t.mix_model != MixModel::Cstr || t.mix_fraction != 0.0
                } else {
                    false
                }
            })
            .collect();
        if !non_default.is_empty() {
            out.push_str("[MIXING]\n");
            out.push_str(";Tank             Model         Fraction\n");
            for n in &non_default {
                if let NodeKind::Tank(ref t) = n.kind {
                    let model_str = match t.mix_model {
                        MixModel::Cstr => "MIXED",
                        MixModel::TwoCompartment => "2COMP",
                        MixModel::Fifo => "FIFO",
                        MixModel::Lifo => "LIFO",
                    };
                    if t.mix_model == MixModel::TwoCompartment {
                        let _ = writeln!(
                            out,
                            " {:<16} {:<14} {:.4}",
                            n.base.id, model_str, t.mix_fraction
                        );
                    } else {
                        let _ = writeln!(out, " {:<16} {}", n.base.id, model_str);
                    }
                }
            }
            out.push('\n');
        }
    }

    // ── [EMITTERS] ───────────────────────────────────────────────────────────
    {
        let emitters: Vec<_> = network
            .nodes
            .iter()
            .filter(|n| {
                if let NodeKind::Junction(ref j) = n.kind {
                    j.emitter_coeff > 0.0
                } else {
                    false
                }
            })
            .collect();
        if !emitters.is_empty() {
            out.push_str("[EMITTERS]\n");
            out.push_str(";Junction        Flow Coefficient\n");
            for n in &emitters {
                if let NodeKind::Junction(ref j) = n.kind {
                    // Reverse the emitter conversion:
                    // internal = ucf_emit / C^qexp  →  C = (ucf_emit / internal)^emitter_exp
                    let qexp = 1.0 / j.emitter_exp;
                    let ucf_emit = ucf.flow.powf(qexp) / ucf.pressure;
                    let coeff_user = (ucf_emit / j.emitter_coeff).powf(j.emitter_exp);
                    let _ = writeln!(out, " {:<16} {:>12.4}", n.base.id, coeff_user);
                }
            }
            out.push('\n');
        }
    }

    // ── [LEAKAGE] ────────────────────────────────────────────────────────────
    // FAVAD coefficients are stored internally as per-pipe discharge
    // coefficients K1/K2; write back the on-disk C1 (mm²) / C2 (mm) values by
    // inverting the load conversion (spec §4.3, units.rs):
    //   K1 = Cd·sqrt(2g) · 1e-6 · C1 · (L/100)
    //   K2 = Cd·sqrt(2g) · 1e-3 · C2 · (L/100)
    {
        let mut leak_lines: Vec<String> = Vec::new();
        for l in &network.links {
            if let LinkKind::Pipe(ref p) = l.kind {
                if (p.leak_coeff_1 > 0.0 || p.leak_coeff_2 > 0.0) && p.length > 0.0 {
                    const CD_SQRT2G: f64 = 2.65734; // Cd·sqrt(2g), Cd = 0.6 (SI)
                    let len_100 = p.length / 100.0; // p.length is internal (m)
                    let c1 = p.leak_coeff_1 / (CD_SQRT2G * 1e-6 * len_100);
                    let c2 = p.leak_coeff_2 / (CD_SQRT2G * 1e-3 * len_100);
                    leak_lines.push(format!(" {:<16} {:>12.4} {:>12.4}", l.base.id, c1, c2));
                }
            }
        }
        if !leak_lines.is_empty() {
            out.push_str("[LEAKAGE]\n");
            out.push_str(";Pipe             Coeff1        Coeff2\n");
            for line in leak_lines {
                out.push_str(&line);
                out.push('\n');
            }
            out.push('\n');
        }
    }

    // ── [QUALITY] ────────────────────────────────────────────────────────────
    {
        let non_zero: Vec<_> = network
            .nodes
            .iter()
            .filter(|n| n.base.initial_quality != 0.0)
            .collect();
        if !non_zero.is_empty() {
            out.push_str("[QUALITY]\n");
            out.push_str(";Node             InitQuality\n");
            for n in &non_zero {
                let _ = writeln!(out, " {:<16} {:>12.4}", n.base.id, n.base.initial_quality);
            }
            out.push('\n');
        }
    }

    // ── [OPTIONS] ────────────────────────────────────────────────────────────
    {
        out.push_str("[OPTIONS]\n");
        let opts = &network.options;
        let _ = writeln!(out, " Units           {}", flow_units_str(opts.flow_units));
        let _ = writeln!(
            out,
            " Headloss        {}",
            match opts.head_loss_formula {
                HeadLossFormula::HazenWilliams => "H-W",
                HeadLossFormula::DarcyWeisbach => "D-W",
                HeadLossFormula::ChezyManning => "C-M",
            }
        );
        if opts.specific_gravity != 1.0 {
            let _ = writeln!(out, " Specific Gravity {:.4}", opts.specific_gravity);
        }
        // Viscosity / diffusivity: write as EPANET multipliers if non-default.
        const VISCOS: f64 = 1.022e-6; // m²/s
        const DIFFUS: f64 = 1.208e-9; // m²/s
        if (opts.viscosity - VISCOS).abs() > 1e-20 {
            let len2 = ucf.elev * ucf.elev;
            let v_user = opts.viscosity * len2;
            let _ = writeln!(out, " Viscosity       {:.6}", v_user);
        }
        if (opts.diffusivity - DIFFUS).abs() > 1e-20 {
            let len2 = ucf.elev * ucf.elev;
            let d_user = opts.diffusivity * len2;
            let _ = writeln!(out, " Diffusivity     {:.6}", d_user);
        }
        let _ = writeln!(out, " Trials          {}", opts.max_iter);
        let _ = writeln!(out, " Accuracy        {:.6}", opts.flow_tol);
        if opts.head_error_limit > 0.0 {
            let _ = writeln!(
                out,
                " HEADERROR       {:.6}",
                opts.head_error_limit * ucf.elev
            );
        }
        if opts.flow_change_limit > 0.0 {
            let _ = writeln!(
                out,
                " FLOWCHANGE      {:.6}",
                opts.flow_change_limit * ucf.flow
            );
        }
        if opts.damp_limit > 0.0 {
            let _ = writeln!(out, " DAMPLIMIT       {:.6}", opts.damp_limit);
        }
        let _ = writeln!(
            out,
            " Unbalanced      {}",
            if opts.extra_iter < 0 {
                "STOP".to_string()
            } else {
                format!("CONTINUE {}", opts.extra_iter)
            }
        );
        let demand_model_str = match opts.demand_model {
            crate::DemandModel::DemandDriven => "DDA",
            crate::DemandModel::PressureDriven => "PDA",
        };
        let _ = writeln!(out, " Demand Model    {}", demand_model_str);
        if opts.demand_multiplier != 1.0 {
            let _ = writeln!(out, " Demand Multiplier {:.4}", opts.demand_multiplier);
        }
        if let Some(ref pat) = opts.default_pattern {
            // EPANET keyword is `PATTERN <id>` (spec §4.3); the reader does not
            // recognise a "Default Pattern" spelling.
            let _ = writeln!(out, " Pattern         {}", pat);
        }
        if opts.demand_model == crate::DemandModel::PressureDriven {
            let _ = writeln!(
                out,
                " Minimum Pressure {:.4}",
                opts.pda_min_pressure * ucf.pressure
            );
            let _ = writeln!(
                out,
                " Required Pressure {:.4}",
                opts.pda_required_pressure * ucf.pressure
            );
            let _ = writeln!(out, " Pressure Exponent {:.4}", opts.pda_pressure_exponent);
        }
        // Quality.
        match opts.quality_mode {
            QualityMode::None => {
                let _ = writeln!(out, " Quality         None");
            }
            QualityMode::Chemical => {
                if opts.chem_name.is_empty() {
                    let _ = writeln!(out, " Quality         Chemical");
                } else {
                    let _ = writeln!(
                        out,
                        " Quality         {} {}",
                        opts.chem_name, opts.chem_units
                    );
                }
            }
            QualityMode::Age => {
                let _ = writeln!(out, " Quality         Age");
            }
            QualityMode::Trace => {
                let trace = opts.trace_node.as_deref().unwrap_or("");
                let _ = writeln!(out, " Quality         Trace {}", trace);
            }
        }
        if opts.quality_tolerance != 0.01 {
            let _ = writeln!(out, " Tolerance       {:.6}", opts.quality_tolerance);
        }
        // Emitter exponent: a single global value in INP, applied to every
        // junction at load time. Emit it (from the junctions' shared value)
        // when it differs from the default 0.5.
        let emitter_exp = network.nodes.iter().find_map(|n| match &n.kind {
            NodeKind::Junction(j) => Some(j.emitter_exp),
            _ => None,
        });
        if let Some(exp) = emitter_exp {
            if (exp - 0.5).abs() > 1e-12 {
                let _ = writeln!(out, " Emitter Exponent {:.4}", exp);
            }
        }
        // Emitter backflow: reader default is YES; only NO needs writing.
        if !opts.emitter_backflow {
            let _ = writeln!(out, " BACKFLOW ALLOWED NO");
        }
        if opts.check_freq != 2 {
            let _ = writeln!(out, " CHECKFREQ       {}", opts.check_freq);
        }
        if opts.max_check != 10 {
            let _ = writeln!(out, " MAXCHECK        {}", opts.max_check);
        }
        out.push('\n');
    }

    // ── [TIMES] ──────────────────────────────────────────────────────────────
    {
        out.push_str("[TIMES]\n");
        let opts = &network.options;
        let _ = writeln!(
            out,
            " Duration           {}",
            fmt_duration_hm(opts.duration)
        );
        let _ = writeln!(
            out,
            " Hydraulic Timestep {}",
            fmt_duration_hm(opts.hyd_step)
        );
        let _ = writeln!(
            out,
            " Quality Timestep   {}",
            fmt_duration_hm(opts.qual_step)
        );
        let _ = writeln!(
            out,
            " Report Timestep    {}",
            fmt_duration_hm(opts.report_step)
        );
        if opts.report_start > 0.0 {
            let _ = writeln!(
                out,
                " Report Start       {}",
                fmt_duration_hm(opts.report_start)
            );
        }
        if opts.pattern_step != opts.hyd_step {
            let _ = writeln!(
                out,
                " Pattern Timestep   {}",
                fmt_duration_hm(opts.pattern_step)
            );
        }
        if opts.pattern_start > 0.0 {
            let _ = writeln!(
                out,
                " Pattern Start      {}",
                fmt_duration_hm(opts.pattern_start)
            );
        }
        if opts.start_clocktime > 0.0 {
            let _ = writeln!(
                out,
                " Start Clocktime    {}",
                fmt_clocktime(opts.start_clocktime)
            );
        }
        if opts.statistic != StatisticType::Series {
            let stat_str = match opts.statistic {
                StatisticType::Average => "AVERAGE",
                StatisticType::Minimum => "MINIMUM",
                StatisticType::Maximum => "MAXIMUM",
                StatisticType::Range => "RANGE",
                StatisticType::Series => "NONE",
            };
            let _ = writeln!(out, " Statistic          {}", stat_str);
        }
        out.push('\n');
    }

    // ── [REPORT] ─────────────────────────────────────────────────────────────
    {
        let rep = &network.report;
        let mut rep_lines: Vec<String> = Vec::new();
        if rep.page_size > 0 {
            rep_lines.push(format!(" Pagesize   {}", rep.page_size));
        }
        let status_str = match rep.status {
            ReportStatus::No => None,
            ReportStatus::Yes => Some("Yes"),
            ReportStatus::Full => Some("Full"),
        };
        if let Some(s) = status_str {
            rep_lines.push(format!(" Status     {}", s));
        }
        if !rep.summary {
            rep_lines.push(" Summary    No".to_string());
        }
        if !rep.messages {
            rep_lines.push(" Messages   No".to_string());
        }
        if rep.energy {
            rep_lines.push(" Energy     Yes".to_string());
        }
        let nodes_str = match &rep.nodes {
            ReportSelection::None => None,
            ReportSelection::All => Some("ALL".to_string()),
            ReportSelection::Some(ids) => Some(ids.join(" ")),
        };
        if let Some(s) = nodes_str {
            rep_lines.push(format!(" Nodes      {}", s));
        }
        let links_str = match &rep.links {
            ReportSelection::None => None,
            ReportSelection::All => Some("ALL".to_string()),
            ReportSelection::Some(ids) => Some(ids.join(" ")),
        };
        if let Some(s) = links_str {
            rep_lines.push(format!(" Links      {}", s));
        }
        if let Some(ref file) = rep.file {
            rep_lines.push(format!(" File       {}", file));
        }
        // Per-field options (spec §4.3), in sorted field-name order so the
        // output is deterministic (the map has no iteration order).
        let mut field_names: Vec<&String> = rep.fields.keys().collect();
        field_names.sort();
        for fname in field_names {
            let f = &rep.fields[fname];
            rep_lines.push(format!(
                " {:<10} {}",
                fname,
                if f.enabled { "Yes" } else { "No" }
            ));
            if let Some(p) = f.precision {
                rep_lines.push(format!(" {:<10} Precision {}", fname, p));
            }
            if let Some(v) = f.below {
                rep_lines.push(format!(" {:<10} Below {:.4}", fname, v));
            }
            if let Some(v) = f.above {
                rep_lines.push(format!(" {:<10} Above {:.4}", fname, v));
            }
        }
        if !rep_lines.is_empty() {
            out.push_str("[REPORT]\n");
            for line in rep_lines {
                out.push_str(&line);
                out.push('\n');
            }
            out.push('\n');
        }
    }

    // ── [COORDINATES] ────────────────────────────────────────────────────────
    if !network.coordinates.is_empty() {
        out.push_str("[COORDINATES]\n");
        out.push_str(";Node             X-Coord       Y-Coord\n");
        for n in &network.nodes {
            if let Some(&(x, y)) = network.coordinates.get(&n.base.id) {
                let _ = writeln!(out, " {:<16} {:>16.4} {:>16.4}", n.base.id, x, y);
            }
        }
        out.push('\n');
    }

    // ── [VERTICES] ───────────────────────────────────────────────────────────
    if !network.vertices.is_empty() {
        out.push_str("[VERTICES]\n");
        out.push_str(";Link             X-Coord       Y-Coord\n");
        for l in &network.links {
            if let Some(pts) = network.vertices.get(&l.base.id) {
                for &(x, y) in pts {
                    let _ = writeln!(out, " {:<16} {:>16.4} {:>16.4}", l.base.id, x, y);
                }
            }
        }
        out.push('\n');
    }

    out.push_str("[END]\n");
    out.into_bytes()
}

// ── Formatting helpers ────────────────────────────────────────────────────────

/// Format seconds as `H:MM` (no sub-minute precision needed for INP).
fn fmt_duration_hm(secs: f64) -> String {
    if secs == 0.0 {
        return "0:00".to_string();
    }
    let total_min = (secs / 60.0).round() as u64;
    let h = total_min / 60;
    let m = total_min % 60;
    format!("{}:{:02}", h, m)
}

/// Format clock-time seconds-from-midnight as `H AM/PM`.
fn fmt_clocktime(secs: f64) -> String {
    let total_min = (secs / 60.0).round() as u32 % (24 * 60);
    let h24 = total_min / 60;
    let m = total_min % 60;
    let (h12, ampm) = if h24 < 12 {
        (if h24 == 0 { 12 } else { h24 }, "AM")
    } else {
        (if h24 == 12 { 12 } else { h24 - 12 }, "PM")
    };
    if m == 0 {
        format!("{} {}", h12, ampm)
    } else {
        format!("{}:{:02} {}", h12, m, ampm)
    }
}

fn link_status_str(status: LinkStatus) -> &'static str {
    match status {
        LinkStatus::Open => "Open",
        LinkStatus::Closed => "Closed",
        _ => "Open",
    }
}

fn valve_type_str(vtype: ValveType) -> &'static str {
    match vtype {
        ValveType::Prv => "PRV",
        ValveType::Psv => "PSV",
        ValveType::Pbv => "PBV",
        ValveType::Fcv => "FCV",
        ValveType::Tcv => "TCV",
        ValveType::Gpv => "GPV",
        ValveType::Pcv => "PCV",
    }
}

fn flow_units_str(units: crate::FlowUnits) -> &'static str {
    match units {
        crate::FlowUnits::Cfs => "CFS",
        crate::FlowUnits::Gpm => "GPM",
        crate::FlowUnits::Mgd => "MGD",
        crate::FlowUnits::Imgd => "IMGD",
        crate::FlowUnits::Afd => "AFD",
        crate::FlowUnits::Lps => "LPS",
        crate::FlowUnits::Lpm => "LPM",
        crate::FlowUnits::Mld => "MLD",
        crate::FlowUnits::Cmh => "CMH",
        crate::FlowUnits::Cmd => "CMD",
        crate::FlowUnits::Cms => "CMS",
    }
}

fn premise_attr_str(attr: PremiseAttribute) -> &'static str {
    match attr {
        PremiseAttribute::Head => "HEAD",
        PremiseAttribute::Pressure => "PRESSURE",
        PremiseAttribute::Demand => "DEMAND",
        PremiseAttribute::Level => "LEVEL",
        PremiseAttribute::Flow => "FLOW",
        PremiseAttribute::Status => "STATUS",
        PremiseAttribute::Setting => "SETTING",
        PremiseAttribute::Power => "POWER",
        PremiseAttribute::FillTime => "FILLTIME",
        PremiseAttribute::DrainTime => "DRAINTIME",
        PremiseAttribute::ClockTime => "CLOCKTIME",
        PremiseAttribute::Time => "TIME",
    }
}

fn premise_op_str(op: PremiseOperator) -> &'static str {
    match op {
        PremiseOperator::Eq => "=",
        PremiseOperator::Neq => "<>",
        PremiseOperator::Lt => "<",
        PremiseOperator::Gt => ">",
        PremiseOperator::Le => "<=",
        PremiseOperator::Ge => ">=",
    }
}

fn convert_premise_value(prem: &crate::Premise, ucf: &super::units::Ucf) -> f64 {
    match prem.attribute {
        PremiseAttribute::Demand | PremiseAttribute::Flow => prem.value * ucf.flow,
        PremiseAttribute::Head | PremiseAttribute::Level => prem.value * ucf.elev,
        PremiseAttribute::Pressure => prem.value * ucf.pressure,
        // All others (Status, Setting, Power, time-related) need no unit conversion.
        _ => prem.value,
    }
}

fn rule_action_str(
    value: &ActionValue,
    link_1based: usize,
    links: &[crate::Link],
    ucf: &super::units::Ucf,
) -> String {
    match value {
        ActionValue::Status(LinkStatus::Open) => "STATUS IS OPEN".to_string(),
        ActionValue::Status(LinkStatus::Closed) => "STATUS IS CLOSED".to_string(),
        ActionValue::Status(_) => "STATUS IS OPEN".to_string(),
        ActionValue::Setting(s) => {
            let setting_user = if let Some(link) = links.get(link_1based.saturating_sub(1)) {
                if let LinkKind::Valve(ref v) = link.kind {
                    match v.valve_type {
                        ValveType::Prv | ValveType::Psv | ValveType::Pbv => s * ucf.pressure,
                        ValveType::Fcv => s * ucf.flow,
                        _ => *s,
                    }
                } else {
                    *s // pump speed
                }
            } else {
                *s
            };
            format!("SETTING IS {:.4}", setting_user)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::parse;

    // Resolve a fixture path relative to the workspace root.
    fn fixture(name: &str) -> std::path::PathBuf {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // crates/engine-wds  →  workspace root
        let root = manifest.parent().unwrap().parent().unwrap();
        root.join("tests/fixtures").join(name)
    }

    /// Parse a fixture, write it back to INP bytes, parse again, and assert
    /// that the key network properties are preserved within a tight tolerance.
    fn round_trip_fixture(name: &str) {
        let path = fixture(name);
        let original_bytes =
            std::fs::read(&path).unwrap_or_else(|e| panic!("could not read {name}: {e}"));

        let net1 = parse(&original_bytes)
            .unwrap_or_else(|e| panic!("first parse of {name} failed: {e:?}"));
        let written = write_inp(&net1);
        let net2 = parse(&written).unwrap_or_else(|e| {
            let s = String::from_utf8_lossy(&written);
            panic!("second parse of {name} failed: {e:?}\n\nwritten INP:\n{s}");
        });

        // Node count, link count, and IDs must be identical.
        assert_eq!(
            net1.nodes.len(),
            net2.nodes.len(),
            "{name}: node count changed after round-trip"
        );
        assert_eq!(
            net1.links.len(),
            net2.links.len(),
            "{name}: link count changed after round-trip"
        );

        // Every node ID present in net1 must still be present in net2.
        for n1 in &net1.nodes {
            let n2 = net2
                .nodes
                .iter()
                .find(|n| n.base.id == n1.base.id)
                .unwrap_or_else(|| {
                    panic!("{name}: node '{}' missing after round-trip", n1.base.id)
                });

            // Elevation round-trips to within 0.1 ft (conversion noise).
            assert!(
                (n1.base.elevation - n2.base.elevation).abs() < 0.1,
                "{name}: node '{}' elevation drifted: {} → {}",
                n1.base.id,
                n1.base.elevation,
                n2.base.elevation
            );

            // Node kind must be preserved.
            assert_eq!(
                std::mem::discriminant(&n1.kind),
                std::mem::discriminant(&n2.kind),
                "{name}: node '{}' kind changed after round-trip",
                n1.base.id
            );
        }

        // Every link ID present in net1 must still be present in net2.
        for l1 in &net1.links {
            let l2 = net2
                .links
                .iter()
                .find(|l| l.base.id == l1.base.id)
                .unwrap_or_else(|| {
                    panic!("{name}: link '{}' missing after round-trip", l1.base.id)
                });

            assert_eq!(
                std::mem::discriminant(&l1.kind),
                std::mem::discriminant(&l2.kind),
                "{name}: link '{}' kind changed after round-trip",
                l1.base.id
            );
        }
    }

    // ── Universal round-trip harness ─────────────────────────────────────────

    /// Relative-or-absolute closeness for values that pass through a
    /// user-unit conversion and a `{:.4}` text format.
    fn close(a: f64, b: f64) -> bool {
        let diff = (a - b).abs();
        diff <= 1e-4 || diff <= 1e-3 * a.abs().max(b.abs())
    }

    /// Relative closeness for small-magnitude internal coefficients (reaction
    /// and leakage coefficients are O(1e-6) internally, so the 1e-4 absolute
    /// floor of `close` would hide even large relative drift).
    fn close_rel(a: f64, b: f64) -> bool {
        let diff = (a - b).abs();
        diff <= 1e-12 || diff <= 1e-3 * a.abs().max(b.abs())
    }

    /// Stricter structural equivalence between an original parse and its
    /// re-parse after `write_inp`. Extends `round_trip_fixture` with checks
    /// on topology, patterns, curves, controls, rules, title, and key options.
    fn assert_round_trip_equivalent(name: &str, net1: &Network, net2: &Network) {
        // ID → node-ID lookup helpers (1-based indices).
        let node_id_of = |net: &Network, idx: usize| -> String {
            net.nodes
                .iter()
                .find(|n| n.base.index == idx)
                .map(|n| n.base.id.clone())
                .unwrap_or_default()
        };

        // Title must be preserved verbatim.
        assert_eq!(net1.title, net2.title, "{name}: title changed");

        // Links: endpoints (by node ID), status, and setting must survive.
        for l1 in &net1.links {
            let l2 = net2
                .links
                .iter()
                .find(|l| l.base.id == l1.base.id)
                .unwrap_or_else(|| panic!("{name}: link '{}' missing", l1.base.id));
            assert_eq!(
                node_id_of(net1, l1.base.from_node),
                node_id_of(net2, l2.base.from_node),
                "{name}: link '{}' from-node changed",
                l1.base.id
            );
            assert_eq!(
                node_id_of(net1, l1.base.to_node),
                node_id_of(net2, l2.base.to_node),
                "{name}: link '{}' to-node changed",
                l1.base.id
            );
            assert_eq!(
                l1.base.initial_status, l2.base.initial_status,
                "{name}: link '{}' initial status changed",
                l1.base.id
            );
            if let (Some(s1), Some(s2)) = (l1.base.initial_setting, l2.base.initial_setting) {
                assert!(
                    close(s1, s2),
                    "{name}: link '{}' setting drifted: {s1} → {s2}",
                    l1.base.id
                );
            }
            // Pump-specific references and values must survive.
            if let (LinkKind::Pump(p1), LinkKind::Pump(p2)) = (&l1.kind, &l2.kind) {
                assert_eq!(
                    p1.head_curve, p2.head_curve,
                    "{name}: pump '{}' head curve changed",
                    l1.base.id
                );
                assert_eq!(
                    p1.efficiency_curve, p2.efficiency_curve,
                    "{name}: pump '{}' efficiency curve changed",
                    l1.base.id
                );
                assert_eq!(
                    p1.speed_pattern, p2.speed_pattern,
                    "{name}: pump '{}' speed pattern changed",
                    l1.base.id
                );
                assert_eq!(
                    p1.price_pattern, p2.price_pattern,
                    "{name}: pump '{}' price pattern changed",
                    l1.base.id
                );
                match (p1.energy_price, p2.energy_price) {
                    (Some(e1), Some(e2)) => assert!(
                        close(e1, e2),
                        "{name}: pump '{}' energy price drifted",
                        l1.base.id
                    ),
                    (e1, e2) => assert_eq!(
                        e1.is_some(),
                        e2.is_some(),
                        "{name}: pump '{}' energy price presence changed",
                        l1.base.id
                    ),
                }
                match (p1.power, p2.power) {
                    (Some(w1), Some(w2)) => assert!(
                        close(w1, w2),
                        "{name}: pump '{}' rated power drifted",
                        l1.base.id
                    ),
                    (w1, w2) => assert_eq!(
                        w1.is_some(),
                        w2.is_some(),
                        "{name}: pump '{}' rated power presence changed",
                        l1.base.id
                    ),
                }
            }
            // Valve type and curve reference (GPV headloss / PCV loss ratio)
            // must survive.
            if let (LinkKind::Valve(v1), LinkKind::Valve(v2)) = (&l1.kind, &l2.kind) {
                assert_eq!(
                    v1.valve_type, v2.valve_type,
                    "{name}: valve '{}' type changed",
                    l1.base.id
                );
                assert_eq!(
                    v1.curve, v2.curve,
                    "{name}: valve '{}' curve reference changed",
                    l1.base.id
                );
            }
            // Pipe reaction and FAVAD leakage coefficients must survive.
            if let (LinkKind::Pipe(p1), LinkKind::Pipe(p2)) = (&l1.kind, &l2.kind) {
                match (p1.bulk_coeff, p2.bulk_coeff) {
                    (Some(k1), Some(k2)) => assert!(
                        close_rel(k1, k2),
                        "{name}: pipe '{}' bulk coeff drifted: {k1} → {k2}",
                        l1.base.id
                    ),
                    (k1, k2) => assert_eq!(
                        k1.is_some(),
                        k2.is_some(),
                        "{name}: pipe '{}' bulk coeff presence changed",
                        l1.base.id
                    ),
                }
                match (p1.wall_coeff, p2.wall_coeff) {
                    (Some(k1), Some(k2)) => assert!(
                        close_rel(k1, k2),
                        "{name}: pipe '{}' wall coeff drifted: {k1} → {k2}",
                        l1.base.id
                    ),
                    (k1, k2) => assert_eq!(
                        k1.is_some(),
                        k2.is_some(),
                        "{name}: pipe '{}' wall coeff presence changed",
                        l1.base.id
                    ),
                }
                assert!(
                    close_rel(p1.leak_coeff_1, p2.leak_coeff_1)
                        && close_rel(p1.leak_coeff_2, p2.leak_coeff_2),
                    "{name}: pipe '{}' FAVAD leak coefficients drifted: ({}, {}) → ({}, {})",
                    l1.base.id,
                    p1.leak_coeff_1,
                    p1.leak_coeff_2,
                    p2.leak_coeff_1,
                    p2.leak_coeff_2
                );
            }
        }

        // Patterns: identical ID sets with equivalent factors.
        assert_eq!(
            net1.patterns.len(),
            net2.patterns.len(),
            "{name}: pattern count changed"
        );
        for p1 in &net1.patterns {
            let p2 = net2
                .patterns
                .iter()
                .find(|p| p.id == p1.id)
                .unwrap_or_else(|| panic!("{name}: pattern '{}' missing", p1.id));
            assert_eq!(
                p1.factors.len(),
                p2.factors.len(),
                "{name}: pattern '{}' length changed",
                p1.id
            );
            for (f1, f2) in p1.factors.iter().zip(&p2.factors) {
                assert!(
                    close(*f1, *f2),
                    "{name}: pattern '{}' factor drifted: {f1} → {f2}",
                    p1.id
                );
            }
        }

        // Curves: identical ID sets with equivalent points.
        assert_eq!(
            net1.curves.len(),
            net2.curves.len(),
            "{name}: curve count changed"
        );
        for c1 in &net1.curves {
            let c2 = net2
                .curves
                .iter()
                .find(|c| c.id == c1.id)
                .unwrap_or_else(|| panic!("{name}: curve '{}' missing", c1.id));
            assert_eq!(
                c1.points.len(),
                c2.points.len(),
                "{name}: curve '{}' point count changed",
                c1.id
            );
            for (pt1, pt2) in c1.points.iter().zip(&c2.points) {
                assert!(
                    close(pt1.x, pt2.x) && close(pt1.y, pt2.y),
                    "{name}: curve '{}' point drifted: ({}, {}) → ({}, {})",
                    c1.id,
                    pt1.x,
                    pt1.y,
                    pt2.x,
                    pt2.y
                );
            }
        }

        // Controls and rules survive with the same shape.
        assert_eq!(
            net1.controls.len(),
            net2.controls.len(),
            "{name}: control count changed"
        );
        assert_eq!(
            net1.rules.len(),
            net2.rules.len(),
            "{name}: rule count changed"
        );
        for (r1, r2) in net1.rules.iter().zip(&net2.rules) {
            assert_eq!(
                r1.premises.len(),
                r2.premises.len(),
                "{name}: rule premise count changed"
            );
            assert_eq!(
                r1.then_actions.len(),
                r2.then_actions.len(),
                "{name}: rule THEN action count changed"
            );
            assert_eq!(
                r1.else_actions.len(),
                r2.else_actions.len(),
                "{name}: rule ELSE action count changed"
            );
        }

        // Junction demand totals (per node) survive.
        for n1 in &net1.nodes {
            if let crate::NodeKind::Junction(ref j1) = n1.kind {
                let n2 = net2
                    .nodes
                    .iter()
                    .find(|n| n.base.id == n1.base.id)
                    .expect("node checked above");
                if let crate::NodeKind::Junction(ref j2) = n2.kind {
                    assert_eq!(
                        j1.demands.len(),
                        j2.demands.len(),
                        "{name}: junction '{}' demand category count changed",
                        n1.base.id
                    );
                    let d1: f64 = j1.demands.iter().map(|d| d.base_demand).sum();
                    let d2: f64 = j2.demands.iter().map(|d| d.base_demand).sum();
                    assert!(
                        close(d1, d2),
                        "{name}: junction '{}' total demand drifted: {d1} → {d2}",
                        n1.base.id
                    );
                    // Per-category base demand, pattern, and name must survive
                    // in order.
                    for (ci, (c1, c2)) in j1.demands.iter().zip(&j2.demands).enumerate() {
                        assert!(
                            close(c1.base_demand, c2.base_demand),
                            "{name}: junction '{}' demand category {ci} drifted: {} → {}",
                            n1.base.id,
                            c1.base_demand,
                            c2.base_demand
                        );
                        assert_eq!(
                            c1.pattern, c2.pattern,
                            "{name}: junction '{}' demand category {ci} pattern changed",
                            n1.base.id
                        );
                        assert_eq!(
                            c1.name, c2.name,
                            "{name}: junction '{}' demand category {ci} name changed",
                            n1.base.id
                        );
                    }
                    assert!(
                        close(j1.emitter_coeff, j2.emitter_coeff),
                        "{name}: junction '{}' emitter coefficient drifted: {} → {}",
                        n1.base.id,
                        j1.emitter_coeff,
                        j2.emitter_coeff
                    );
                    assert!(
                        close(j1.emitter_exp, j2.emitter_exp),
                        "{name}: junction '{}' emitter exponent drifted: {} → {}",
                        n1.base.id,
                        j1.emitter_exp,
                        j2.emitter_exp
                    );
                }
            }
            // Tank overflow flag and volume-curve reference must survive.
            if let crate::NodeKind::Tank(ref t1) = n1.kind {
                let n2 = net2
                    .nodes
                    .iter()
                    .find(|n| n.base.id == n1.base.id)
                    .expect("node checked above");
                if let crate::NodeKind::Tank(ref t2) = n2.kind {
                    assert_eq!(
                        t1.overflow, t2.overflow,
                        "{name}: tank '{}' overflow flag changed",
                        n1.base.id
                    );
                    assert_eq!(
                        t1.volume_curve, t2.volume_curve,
                        "{name}: tank '{}' volume curve changed",
                        n1.base.id
                    );
                }
            }
            // Initial quality survives.
            let n2 = net2
                .nodes
                .iter()
                .find(|n| n.base.id == n1.base.id)
                .expect("node checked above");
            assert!(
                close(n1.base.initial_quality, n2.base.initial_quality),
                "{name}: node '{}' initial quality drifted",
                n1.base.id
            );
        }

        // Key simulation options survive. Durations are written at 1-minute
        // granularity (fmt_duration_hm), so allow up to 60 s of rounding.
        let o1 = &net1.options;
        let o2 = &net2.options;
        assert_eq!(o1.flow_units, o2.flow_units, "{name}: flow units changed");
        assert_eq!(
            o1.head_loss_formula, o2.head_loss_formula,
            "{name}: headloss formula changed"
        );
        assert_eq!(
            o1.demand_model, o2.demand_model,
            "{name}: demand model changed"
        );
        assert_eq!(
            o1.quality_mode, o2.quality_mode,
            "{name}: quality mode changed"
        );
        assert_eq!(o1.trace_node, o2.trace_node, "{name}: trace node changed");
        assert!(
            (o1.duration - o2.duration).abs() <= 60.0,
            "{name}: duration drifted: {} → {}",
            o1.duration,
            o2.duration
        );
        assert!(
            (o1.hyd_step - o2.hyd_step).abs() <= 60.0,
            "{name}: hydraulic timestep drifted: {} → {}",
            o1.hyd_step,
            o2.hyd_step
        );
        assert!(
            close(o1.specific_gravity, o2.specific_gravity),
            "{name}: specific gravity drifted"
        );
        assert!(
            close(o1.demand_multiplier, o2.demand_multiplier),
            "{name}: demand multiplier drifted"
        );
        assert_eq!(
            o1.default_pattern, o2.default_pattern,
            "{name}: default pattern changed"
        );
        assert_eq!(
            o1.emitter_backflow, o2.emitter_backflow,
            "{name}: emitter backflow flag changed"
        );
        assert!(
            close_rel(o1.bulk_coeff, o2.bulk_coeff),
            "{name}: global bulk coeff drifted: {} → {}",
            o1.bulk_coeff,
            o2.bulk_coeff
        );
        assert!(
            close_rel(o1.wall_coeff, o2.wall_coeff),
            "{name}: global wall coeff drifted: {} → {}",
            o1.wall_coeff,
            o2.wall_coeff
        );
        assert!(
            close_rel(o1.roughness_reaction_factor, o2.roughness_reaction_factor),
            "{name}: roughness correlation factor drifted: {} → {}",
            o1.roughness_reaction_factor,
            o2.roughness_reaction_factor
        );
        assert_eq!(o1.wall_order, o2.wall_order, "{name}: wall order changed");

        // [REPORT] options survive verbatim.
        assert_eq!(net1.report, net2.report, "{name}: report options changed");

        // Display metadata survives.
        assert_eq!(
            net1.coordinates.len(),
            net2.coordinates.len(),
            "{name}: coordinate count changed"
        );
        assert_eq!(net1.node_tags, net2.node_tags, "{name}: node tags changed");
        assert_eq!(net1.link_tags, net2.link_tags, "{name}: link tags changed");
        assert_eq!(net1.vertices, net2.vertices, "{name}: vertices changed");
    }

    /// Parse a fixture, write, re-parse, and apply both the baseline and the
    /// strict equivalence checks.
    fn round_trip_fixture_strict(name: &str) {
        let path = fixture(name);
        let original_bytes =
            std::fs::read(&path).unwrap_or_else(|e| panic!("could not read {name}: {e}"));
        let net1 = parse(&original_bytes)
            .unwrap_or_else(|e| panic!("first parse of {name} failed: {e:?}"));
        let written = write_inp(&net1);
        let net2 = parse(&written).unwrap_or_else(|e| {
            let s = String::from_utf8_lossy(&written);
            panic!("second parse of {name} failed: {e:?}\n\nwritten INP:\n{s}");
        });

        round_trip_fixture(name); // baseline checks (counts, IDs, kinds, elevations)
        assert_round_trip_equivalent(name, &net1, &net2);

        // Writer idempotence: a second write of the re-parsed network must be
        // byte-identical to the first write. Any drift here means repeated
        // open/save cycles would keep mutating the file.
        let net2_written = write_inp(&net2);
        assert_eq!(
            String::from_utf8_lossy(&written),
            String::from_utf8_lossy(&net2_written),
            "{name}: write → parse → write is not idempotent"
        );
    }

    /// Fixtures whose `write_inp` output currently fails to re-parse at all.
    /// For a listed fixture the harness only asserts that the fixture itself
    /// parses. Add an entry (with a comment naming the writer defect) only to
    /// quarantine a known defect; remove it once the defect is fixed.
    /// Currently empty: every fixture's written form re-parses.
    const ROUND_TRIP_REPARSE_SKIP: &[(&str, &str)] = &[];

    /// Fixtures whose written form re-parses but loses or mutates data, so
    /// only the baseline (counts / IDs / kinds / elevations) checks run.
    /// Add an entry (with a comment naming the writer defect) only to
    /// quarantine a known defect; remove it once the defect is fixed.
    /// Currently empty: every fixture passes the strict checks.
    const ROUND_TRIP_STRICT_SKIP: &[(&str, &str)] = &[];

    /// Every fixture in tests/fixtures/ must survive parse → write → parse
    /// with structure intact, and the writer must be idempotent. Fixtures on
    /// the two skip lists run reduced checks (see the list docs above).
    #[test]
    fn round_trip_every_fixture() {
        let dir = fixture("");
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .expect("fixtures dir")
            .filter_map(|e| {
                let e = e.ok()?;
                if !e.file_type().ok()?.is_file() {
                    return None;
                }
                let name = e.file_name().to_string_lossy().into_owned();
                name.ends_with(".inp").then_some(name)
            })
            .collect();
        names.sort();
        assert!(
            names.len() >= 90,
            "expected the full fixture corpus, found {} files",
            names.len()
        );

        let mut failures: Vec<String> = Vec::new();
        for name in &names {
            let reparse_skipped = ROUND_TRIP_REPARSE_SKIP.iter().any(|(skip, _)| skip == name);
            let strict_skipped = ROUND_TRIP_STRICT_SKIP.iter().any(|(skip, _)| skip == name);
            let check = std::panic::AssertUnwindSafe(|| {
                if reparse_skipped {
                    // Writer output is known not to re-parse: only require
                    // that the fixture itself parses.
                    let bytes = std::fs::read(fixture(name)).expect("read fixture");
                    parse(&bytes).expect("fixture must parse");
                } else if strict_skipped {
                    round_trip_fixture(name);
                } else {
                    round_trip_fixture_strict(name);
                }
            });
            if let Err(panic) = std::panic::catch_unwind(check) {
                let msg = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "unknown panic".to_string());
                // Keep only the first line of long messages (written INP dumps).
                let first_line = msg.lines().next().unwrap_or("").to_string();
                failures.push(format!("{name}: {first_line}"));
            }
        }
        assert!(
            failures.is_empty(),
            "{} fixture(s) failed the round-trip harness:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    // ── Round-trip fixtures ──────────────────────────────────────────────────

    #[test]
    fn round_trip_four_node_loop() {
        round_trip_fixture("four_node_loop.inp");
    }

    #[test]
    fn round_trip_dual_reservoir() {
        round_trip_fixture("dual_reservoir.inp");
    }

    #[test]
    fn round_trip_multi_tank() {
        round_trip_fixture("multi_tank.inp");
    }

    #[test]
    fn round_trip_parallel_pumps() {
        round_trip_fixture("parallel_pumps.inp");
    }

    #[test]
    fn round_trip_multi_control() {
        round_trip_fixture("multi_control.inp");
    }

    #[test]
    fn round_trip_pipe_reactions() {
        round_trip_fixture("pipe_reactions.inp");
    }

    #[test]
    fn round_trip_demand_pattern() {
        round_trip_fixture("demand_pattern.inp");
    }

    #[test]
    fn round_trip_multiple_demands() {
        round_trip_fixture("multiple_demands.inp");
    }

    #[test]
    fn round_trip_emitter() {
        round_trip_fixture("emitter.inp");
    }

    #[test]
    fn round_trip_initial_quality() {
        round_trip_fixture("initial_quality.inp");
    }

    #[test]
    fn round_trip_parallel_pipes() {
        round_trip_fixture("parallel_pipes.inp");
    }

    #[test]
    fn round_trip_dead_end() {
        round_trip_fixture("dead_end.inp");
    }

    #[test]
    fn round_trip_pump_energy_full() {
        round_trip_fixture_strict("pump_energy_full.inp");
    }

    #[test]
    fn round_trip_metadata_display() {
        round_trip_fixture_strict("metadata_display.inp");
    }

    // ── Writer defect regressions ────────────────────────────────────────────

    /// Parse `inp`, then run two write→parse cycles, returning the three
    /// parsed networks (original, after one cycle, after two cycles).
    fn two_cycles(inp: &str) -> (Network, Network, Network) {
        let n1 = parse(inp.as_bytes()).expect("initial parse");
        let n2 = parse(&write_inp(&n1)).unwrap_or_else(|e| {
            panic!(
                "re-parse after first write failed: {e:?}\n{}",
                String::from_utf8_lossy(&write_inp(&n1))
            )
        });
        let n3 = parse(&write_inp(&n2)).expect("re-parse after second write");
        (n1, n2, n3)
    }

    /// Assert `a` and `b` are equal to within 1e-9 relative tolerance.
    fn assert_rel_eq(a: f64, b: f64, what: &str) {
        let tol = 1e-9 * a.abs().max(b.abs()).max(1e-30);
        assert!(
            (a - b).abs() <= tol,
            "{what}: {a} != {b} (diff {})",
            (a - b).abs()
        );
    }

    fn reaction_stability_inp(units: &str, order_wall: u32) -> String {
        format!(
            "[TITLE]\nreaction coefficient stability\n\n\
             [JUNCTIONS]\nJ1  0  10\nJ2  0  10\n\n\
             [RESERVOIRS]\nR1  100\n\n\
             [PIPES]\nP1  R1  J1  1000  12  100  0  Open\nP2  J1  J2  800  10  100  0  Open\n\n\
             [REACTIONS]\n\
             Order Wall  {order_wall}\n\
             Global Bulk  -0.5\n\
             Global Wall  -1.25\n\
             Roughness Correlation  0.75\n\
             Bulk  P1  -0.3\n\
             Wall  P1  -0.8\n\n\
             [OPTIONS]\nUnits  {units}\nHeadloss  H-W\nQuality  Chemical mg/L\n\n[END]\n"
        )
    }

    /// Reaction coefficients (bulk, first-order wall, roughness correlation,
    /// per-pipe overrides) must be numerically unchanged (~1e-9) across two
    /// save/load cycles. Exercises the length-dimension inverse (quality spec
    /// §6.5.2) that the writer previously omitted, scaling wall values by
    /// ~0.3048 per cycle in US units.
    fn assert_reaction_coeffs_stable(units: &str, order_wall: u32) {
        let inp = reaction_stability_inp(units, order_wall);
        let (n1, n2, n3) = two_cycles(&inp);
        for (label, net) in [("one cycle", &n2), ("two cycles", &n3)] {
            let o1 = &n1.options;
            let o = &net.options;
            assert_rel_eq(
                o1.bulk_coeff,
                o.bulk_coeff,
                &format!("{units} global bulk after {label}"),
            );
            assert_rel_eq(
                o1.wall_coeff,
                o.wall_coeff,
                &format!("{units} global wall after {label}"),
            );
            assert_rel_eq(
                o1.roughness_reaction_factor,
                o.roughness_reaction_factor,
                &format!("{units} roughness correlation after {label}"),
            );
            let pipe = |n: &Network| {
                n.links
                    .iter()
                    .find(|l| l.base.id == "P1")
                    .map(|l| match &l.kind {
                        LinkKind::Pipe(p) => (p.bulk_coeff.unwrap(), p.wall_coeff.unwrap()),
                        _ => unreachable!(),
                    })
                    .unwrap()
            };
            let (kb1, kw1) = pipe(&n1);
            let (kb, kw) = pipe(net);
            assert_rel_eq(kb1, kb, &format!("{units} pipe bulk after {label}"));
            assert_rel_eq(kw1, kw, &format!("{units} pipe wall after {label}"));
        }
    }

    #[test]
    fn reaction_coeffs_stable_us_units_first_order_wall() {
        assert_reaction_coeffs_stable("GPM", 1);
    }

    #[test]
    fn reaction_coeffs_stable_us_units_zero_order_wall() {
        assert_reaction_coeffs_stable("GPM", 0);
    }

    #[test]
    fn reaction_coeffs_stable_si_units_first_order_wall() {
        assert_reaction_coeffs_stable("LPS", 1);
    }

    #[test]
    fn reaction_coeffs_stable_si_units_zero_order_wall() {
        assert_reaction_coeffs_stable("LPS", 0);
    }

    /// `BACKFLOW ALLOWED NO` must be serialised as itself — not as the
    /// unrelated `Emitter Exponent` option — and survive a round-trip.
    #[test]
    fn backflow_allowed_no_round_trips() {
        let inp = "[JUNCTIONS]\nJ1  0  0\nJ2  0  0\n\n\
                   [RESERVOIRS]\nR1  100\n\n\
                   [PIPES]\nP1  R1  J1  500  12  100  0  Open\nP2  J1  J2  500  12  100  0  Open\n\n\
                   [EMITTERS]\nJ2  5.0\n\n\
                   [OPTIONS]\nUnits  GPM\nHeadloss  H-W\nBackflow Allowed  NO\n\n[END]\n";
        let n1 = parse(inp.as_bytes()).expect("parse");
        assert!(
            !n1.options.emitter_backflow,
            "fixture sets BACKFLOW ALLOWED NO"
        );
        let written = write_inp(&n1);
        let text = String::from_utf8_lossy(&written);
        assert!(
            text.contains("BACKFLOW ALLOWED NO"),
            "writer must emit BACKFLOW ALLOWED NO:\n{text}"
        );
        assert!(
            !text.contains("Emitter Exponent"),
            "default emitter exponent must not be emitted for the backflow option:\n{text}"
        );
        let n2 = parse(&written).expect("re-parse");
        assert!(
            !n2.options.emitter_backflow,
            "backflow flag lost on round-trip"
        );
    }

    /// A non-default global emitter exponent must be written (as the
    /// `EMITTER EXPONENT` option) so emitter coefficients survive re-parse.
    #[test]
    fn emitter_exponent_round_trips() {
        let inp = "[JUNCTIONS]\nJ1  0  0\nJ2  0  0\n\n\
                   [RESERVOIRS]\nR1  100\n\n\
                   [PIPES]\nP1  R1  J1  500  12  100  0  Open\nP2  J1  J2  500  12  100  0  Open\n\n\
                   [EMITTERS]\nJ2  5.0\n\n\
                   [OPTIONS]\nUnits  GPM\nHeadloss  H-W\nEmitter Exponent  0.8\n\n[END]\n";
        let (n1, _n2, n3) = two_cycles(inp);
        let emitter = |n: &Network| {
            n.nodes
                .iter()
                .find(|nd| nd.base.id == "J2")
                .map(|nd| match &nd.kind {
                    NodeKind::Junction(j) => (j.emitter_exp, j.emitter_coeff),
                    _ => unreachable!(),
                })
                .unwrap()
        };
        let (exp1, coeff1) = emitter(&n1);
        let (exp3, coeff3) = emitter(&n3);
        assert!((exp1 - 0.8).abs() < 1e-12, "exponent parsed");
        assert!((exp3 - 0.8).abs() < 1e-12, "exponent survived round-trips");
        assert_rel_eq(coeff1, coeff3, "emitter coefficient over two cycles");
    }

    /// [REPORT] per-field options (YES/NO, PRECISION, ABOVE, BELOW) and the
    /// MESSAGES flag must be serialised and re-parsed verbatim.
    #[test]
    fn report_field_options_round_trip() {
        let inp = "[JUNCTIONS]\nJ1  0  10\n\n\
                   [RESERVOIRS]\nR1  100\n\n\
                   [PIPES]\nP1  R1  J1  1000  12  100  0  Open\n\n\
                   [REPORT]\n\
                   Messages  No\n\
                   Pressure  Precision  4\n\
                   Flow  No\n\
                   Velocity  Below  0.1\n\
                   Head  Above  10\n\n\
                   [OPTIONS]\nUnits  GPM\nHeadloss  H-W\n\n[END]\n";
        let n1 = parse(inp.as_bytes()).expect("parse");
        assert!(!n1.report.messages);
        assert_eq!(n1.report.fields.len(), 4);
        let n2 = parse(&write_inp(&n1)).expect("re-parse");
        assert_eq!(n1.report, n2.report, "report options changed on round-trip");
        // A second write must be byte-identical (deterministic field order).
        assert_eq!(
            String::from_utf8_lossy(&write_inp(&n1)),
            String::from_utf8_lossy(&write_inp(&n2)),
            "report serialisation not idempotent"
        );
    }

    /// [LEAKAGE] coefficients must be written back (inverting the FAVAD load
    /// conversion) and survive two save/load cycles.
    #[test]
    fn leakage_coefficients_round_trip() {
        let inp = std::fs::read(fixture("leakage_favad.inp")).expect("read fixture");
        let n1 = parse(&inp).expect("parse");
        let written = write_inp(&n1);
        let text = String::from_utf8_lossy(&written);
        assert!(
            text.contains("[LEAKAGE]"),
            "writer must emit [LEAKAGE]:\n{text}"
        );
        let n3 = parse(&write_inp(&parse(&written).expect("re-parse"))).expect("second cycle");
        let leak = |n: &Network| {
            n.links
                .iter()
                .find(|l| l.base.id == "P1")
                .map(|l| match &l.kind {
                    LinkKind::Pipe(p) => (p.leak_coeff_1, p.leak_coeff_2),
                    _ => unreachable!(),
                })
                .unwrap()
        };
        let (k1a, k2a) = leak(&n1);
        let (k1b, k2b) = leak(&n3);
        assert!(
            k1a > 0.0 && k2a > 0.0,
            "fixture has non-zero FAVAD coefficients"
        );
        // First cycle rounds pipe length to 4 decimals in user units; the
        // internal coefficients must still match to formatting noise, and be
        // exactly stable afterwards.
        assert_rel_eq(k1a, k1b, "leak_coeff_1 over two cycles");
        assert_rel_eq(k2a, k2b, "leak_coeff_2 over two cycles");
    }

    /// The default pattern must be written with the EPANET `PATTERN` keyword
    /// (the reader does not recognise `Default Pattern`).
    #[test]
    fn default_pattern_written_with_epanet_keyword() {
        let inp = std::fs::read(fixture("demand_categories.inp")).expect("read fixture");
        let n1 = parse(&inp).expect("parse");
        assert_eq!(n1.options.default_pattern.as_deref(), Some("PAT1"));
        let written = write_inp(&n1);
        let text = String::from_utf8_lossy(&written);
        assert!(
            !text.contains("Default Pattern"),
            "non-EPANET keyword:\n{text}"
        );
        let n2 = parse(&written).expect("re-parse");
        assert_eq!(n2.options.default_pattern.as_deref(), Some("PAT1"));
        assert_eq!(n1.patterns.len(), n2.patterns.len());
    }

    /// GPV: the [VALVES] Setting column must hold the head-loss curve ID.
    #[test]
    fn gpv_setting_written_as_curve_id() {
        let inp = std::fs::read(fixture("gpv_valve.inp")).expect("read fixture");
        let n1 = parse(&inp).expect("parse");
        let n2 = parse(&write_inp(&n1)).expect("re-parse");
        let valve = n2
            .links
            .iter()
            .find_map(|l| match &l.kind {
                LinkKind::Valve(v) => Some(v),
                _ => None,
            })
            .expect("valve present");
        assert_eq!(valve.valve_type, ValveType::Gpv);
        assert_eq!(valve.curve.as_deref(), Some("HC1"));
    }

    /// PCV: the optional 8th [VALVES] field must carry the loss-ratio curve.
    #[test]
    fn pcv_loss_ratio_curve_written() {
        let inp = std::fs::read(fixture("pcv_valve.inp")).expect("read fixture");
        let n1 = parse(&inp).expect("parse");
        let n2 = parse(&write_inp(&n1)).expect("re-parse");
        let (setting, valve) = n2
            .links
            .iter()
            .find_map(|l| match &l.kind {
                LinkKind::Valve(v) => Some((l.base.initial_setting, v)),
                _ => None,
            })
            .expect("valve present");
        assert_eq!(valve.valve_type, ValveType::Pcv);
        assert_eq!(valve.curve.as_deref(), Some("LC1"));
        assert!(
            (setting.unwrap() - 50.0).abs() < 1e-6,
            "percent-open setting kept"
        );
    }

    /// Overflow tank without a volume curve: the VolCurve column must hold
    /// the `*` placeholder so YES stays in the Overflow field.
    #[test]
    fn tank_overflow_written_with_star_placeholder() {
        let inp = std::fs::read(fixture("tank_overflow.inp")).expect("read fixture");
        let n1 = parse(&inp).expect("parse");
        let n2 = parse(&write_inp(&n1)).expect("re-parse");
        let tank = n2
            .nodes
            .iter()
            .find_map(|n| match &n.kind {
                NodeKind::Tank(t) => Some(t),
                _ => None,
            })
            .expect("tank present");
        assert!(tank.overflow, "overflow flag survived");
        assert!(tank.volume_curve.is_none(), "no spurious volume curve");
    }

    /// A junction with three named demand categories keeps all of them —
    /// including the first — through a save/load cycle.
    #[test]
    fn multi_category_named_demands_round_trip() {
        let inp = std::fs::read(fixture("demand_categories_named.inp")).expect("read fixture");
        let n1 = parse(&inp).expect("parse");
        let n2 = parse(&write_inp(&n1)).expect("re-parse");
        for net in [&n1, &n2] {
            let j = net
                .nodes
                .iter()
                .find(|n| n.base.id == "J1")
                .map(|n| match &n.kind {
                    NodeKind::Junction(j) => j,
                    _ => unreachable!(),
                })
                .expect("J1 present");
            assert_eq!(j.demands.len(), 3, "three categories expected");
            let names: Vec<_> = j.demands.iter().map(|d| d.name.as_deref()).collect();
            assert_eq!(
                names,
                vec![Some("Residential"), Some("Commercial"), Some("Irrigation")]
            );
            let patterns: Vec<_> = j.demands.iter().map(|d| d.pattern.as_deref()).collect();
            assert_eq!(patterns, vec![Some("PAT1"), Some("PAT2"), Some("PAT1")]);
        }
        round_trip_fixture_strict("demand_categories_named.inp");
    }

    // ── Unit conversion spot-checks ──────────────────────────────────────────

    /// Pipe diameter (ft → mm) and back (mm → ft) must cancel exactly.
    #[test]
    fn pipe_diameter_unit_conversion_round_trips() {
        use super::super::units::make_ucf;
        use crate::FlowUnits;

        let ucf = make_ucf(FlowUnits::Lps, 1.0);
        // 0.5 ft diameter pipe.
        let d_ft = 0.5_f64;
        let d_mm = d_ft * ucf.diam; // ft → mm
        let d_ft2 = d_mm / ucf.diam; // mm → ft
        assert!((d_ft - d_ft2).abs() < 1e-12);
    }

    /// Elevation (ft → m) and back must cancel exactly.
    #[test]
    fn elevation_unit_conversion_round_trips() {
        use super::super::units::make_ucf;
        use crate::FlowUnits;

        let ucf = make_ucf(FlowUnits::Lps, 1.0);
        let elev_ft = 100.0_f64;
        let elev_m = elev_ft * ucf.elev;
        let elev_ft2 = elev_m / ucf.elev;
        assert!((elev_ft - elev_ft2).abs() < 1e-12);
    }

    /// Minor-loss round-trip: K_m → K_v → K_m must recover the original.
    #[test]
    fn minor_loss_round_trip() {
        // K_m = 0.02517 * K_v / D^4
        // K_v = K_m * D^4 / 0.02517
        let d_ft = 0.5_f64; // 6-inch pipe
        let kv_original = 0.25_f64;
        let km = 0.02517 * kv_original / d_ft.powi(4);
        let kv_recovered = km * d_ft.powi(4) / 0.02517;
        assert!((kv_original - kv_recovered).abs() < 1e-12);
    }

    /// Tank bottom-elevation invariant:
    ///   node.base.elevation = bottom_ft + min_level
    /// The writer emits bottom_ft; on re-parse the reader must reconstruct the same value.
    #[test]
    fn tank_elevation_invariant() {
        round_trip_fixture("multi_tank.inp");
        // Additionally verify the arithmetic directly.
        let bottom_ft = 10.0_f64;
        let min_level = 2.0_f64;
        let node_elev = bottom_ft + min_level;
        let bottom_recovered = node_elev - min_level;
        assert!((bottom_ft - bottom_recovered).abs() < 1e-12);
    }

    /// Reaction coefficients (per-s → per-day → per-s) must cancel exactly.
    #[test]
    fn reaction_coeff_unit_conversion() {
        let kb_per_s = -1.157e-5_f64; // representative bulk coefficient
        let kb_per_day = kb_per_s * 86400.0;
        let kb_per_s2 = kb_per_day / 86400.0;
        assert!((kb_per_s - kb_per_s2).abs() < 1e-20);
    }

    // ── fmt helpers ──────────────────────────────────────────────────────────

    #[test]
    fn fmt_duration_hm_zero() {
        assert_eq!(fmt_duration_hm(0.0), "0:00");
    }

    #[test]
    fn fmt_duration_hm_one_hour() {
        assert_eq!(fmt_duration_hm(3600.0), "1:00");
    }

    #[test]
    fn fmt_duration_hm_90_min() {
        assert_eq!(fmt_duration_hm(5400.0), "1:30");
    }

    #[test]
    fn fmt_duration_hm_24_hours() {
        assert_eq!(fmt_duration_hm(86400.0), "24:00");
    }

    #[test]
    fn fmt_clocktime_midnight() {
        assert_eq!(fmt_clocktime(0.0), "12 AM");
    }

    #[test]
    fn fmt_clocktime_noon() {
        assert_eq!(fmt_clocktime(43200.0), "12 PM");
    }

    #[test]
    fn fmt_clocktime_1pm() {
        assert_eq!(fmt_clocktime(46800.0), "1 PM");
    }

    #[test]
    fn fmt_clocktime_6_30_am() {
        assert_eq!(fmt_clocktime(6.0 * 3600.0 + 30.0 * 60.0), "6:30 AM");
    }
}
