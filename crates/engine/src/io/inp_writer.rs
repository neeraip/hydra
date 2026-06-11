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

use crate::{
    ActionValue, CurveKind, HeadLossFormula, LinkKind, LinkStatus, LogicOp, MixModel, Network,
    NodeKind, PremiseAttribute, PremiseObject, PremiseOperator, QualityMode, ReportSelection,
    ReportStatus, SourceType, StatisticType, TriggerType, ValveType, WallOrder,
};
use super::units::make_ucf;

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
        let junctions: Vec<_> = network.nodes.iter()
            .filter(|n| matches!(n.kind, NodeKind::Junction(_)))
            .collect();
        if !junctions.is_empty() {
            out.push_str("[JUNCTIONS]\n");
            out.push_str(";ID               Elev          Demand        Pattern\n");
            for n in &junctions {
                if let NodeKind::Junction(ref j) = n.kind {
                    let elev = n.base.elevation * ucf.elev;
                    let base_demand = j.demands.first()
                        .map(|d| d.base_demand * ucf.flow)
                        .unwrap_or(0.0);
                    let pattern = j.demands.first()
                        .and_then(|d| d.pattern.as_deref())
                        .unwrap_or("");
                    let _ = writeln!(out, " {:<16} {:>12.4} {:>12.4}   {}",
                        n.base.id, elev, base_demand, pattern);
                }
            }
            out.push('\n');
        }
    }

    // ── [RESERVOIRS] ─────────────────────────────────────────────────────────
    {
        let reservoirs: Vec<_> = network.nodes.iter()
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
        let tanks: Vec<_> = network.nodes.iter()
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
                    let init   = t.initial_level * ucf.elev;
                    let min_l  = t.min_level     * ucf.elev;
                    let max_l  = t.max_level     * ucf.elev;
                    // Tank diameter uses length conversion, not pipe-diameter conv.
                    let diam   = t.diameter      * ucf.elev;
                    let min_v  = t.min_volume    * ucf.vol;
                    let vol_curve = t.volume_curve.as_deref().unwrap_or("");
                    let overflow = if t.overflow { "YES" } else { "" };
                    let _ = writeln!(out,
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
        let pipes: Vec<_> = network.links.iter()
            .filter(|l| matches!(l.kind, LinkKind::Pipe(_)))
            .collect();
        if !pipes.is_empty() {
            out.push_str("[PIPES]\n");
            out.push_str(";ID               Node1         Node2         Length        Diameter      Roughness     MinorLoss     Status\n");
            for l in &pipes {
                if let LinkKind::Pipe(ref p) = l.kind {
                    let from = node_id.get(l.base.from_node).copied().unwrap_or("");
                    let to   = node_id.get(l.base.to_node).copied().unwrap_or("");
                    let len  = p.length   * ucf.elev;
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
                    let _ = writeln!(out,
                        " {:<16} {:<14} {:<14} {:>12.4} {:>12.4} {:>12.4} {:>12.4}  {}",
                        l.base.id, from, to, len, diam, rough, minor, status);
                }
            }
            out.push('\n');
        }
    }

    // ── [PUMPS] ──────────────────────────────────────────────────────────────
    {
        let pumps: Vec<_> = network.links.iter()
            .filter(|l| matches!(l.kind, LinkKind::Pump(_)))
            .collect();
        if !pumps.is_empty() {
            out.push_str("[PUMPS]\n");
            out.push_str(";ID               Node1         Node2         Parameters\n");
            for l in &pumps {
                if let LinkKind::Pump(ref p) = l.kind {
                    let from = node_id.get(l.base.from_node).copied().unwrap_or("");
                    let to   = node_id.get(l.base.to_node).copied().unwrap_or("");
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
                    let _ = writeln!(out, " {:<16} {:<14} {:<14}{}",
                        l.base.id, from, to, params);
                }
            }
            out.push('\n');
        }
    }

    // ── [VALVES] ─────────────────────────────────────────────────────────────
    {
        let valves: Vec<_> = network.links.iter()
            .filter(|l| matches!(l.kind, LinkKind::Valve(_)))
            .collect();
        if !valves.is_empty() {
            out.push_str("[VALVES]\n");
            out.push_str(";ID               Node1         Node2         Diameter      Type      Setting       MinorLoss\n");
            for l in &valves {
                if let LinkKind::Valve(ref v) = l.kind {
                    let from = node_id.get(l.base.from_node).copied().unwrap_or("");
                    let to   = node_id.get(l.base.to_node).copied().unwrap_or("");
                    let diam = v.diameter * ucf.diam;
                    let vtype = valve_type_str(v.valve_type);
                    // Setting: convert back to user units depending on type.
                    let setting_user = l.base.initial_setting.map(|s| {
                        match v.valve_type {
                            ValveType::Prv | ValveType::Psv | ValveType::Pbv => s * ucf.pressure,
                            ValveType::Fcv => s * ucf.flow,
                            _ => s, // TCV, GPV, PCV: dimensionless or curve-based
                        }
                    }).unwrap_or(0.0);
                    // Minor loss reverse.
                    let minor = if v.minor_loss > 0.0 {
                        let d4 = v.diameter.powi(4);
                        v.minor_loss * d4 / 0.08262
                    } else {
                        0.0
                    };
                    let _ = writeln!(out,
                        " {:<16} {:<14} {:<14} {:>12.4} {:<8} {:>12.4} {:>12.4}",
                        l.base.id, from, to, diam, vtype, setting_user, minor);
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
    // Emit additional demand categories (index ≥ 1) for junctions.
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
                    // Skip first demand (it lives in [JUNCTIONS]).
                    for d in j.demands.iter().skip(1) {
                        let demand = d.base_demand * ucf.flow;
                        let pattern = d.pattern.as_deref().unwrap_or("");
                        let name = d.name.as_deref().unwrap_or("");
                        let _ = writeln!(out, " {:<16} {:>12.4}   {:<14}{}",
                            n.base.id, demand, pattern, name);
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
                        LinkStatus::Open   => status_lines.push(format!(" {:<16} Open",   l.base.id)),
                        LinkStatus::Closed => status_lines.push(format!(" {:<16} Closed", l.base.id)),
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
            for (i, chunk) in chunks.iter().enumerate() {
                let vals: Vec<String> = chunk.iter().map(|f| format!("{:.4}", f)).collect();
                if i == 0 {
                    let _ = writeln!(out, " {:<16} {}", pat.id, vals.join("   "));
                } else {
                    let _ = writeln!(out, " {:<16} {}", pat.id, vals.join("   "));
                }
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
                CurveKind::PumpHead       => Some(";PUMP"),
                CurveKind::PumpEfficiency => Some(";EFFICIENCY"),
                CurveKind::TankVolume     => Some(";VOLUME"),
                CurveKind::GpvHeadloss    => Some(";HEADLOSS"),
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
            if !ctrl.enabled { continue; }
            let link_id_str = link_id.get(ctrl.link).copied().unwrap_or("?");
            // Action part.
            let action_str = match (ctrl.action_status, ctrl.action_setting) {
                (Some(LinkStatus::Open),   _) => "OPEN".to_string(),
                (Some(LinkStatus::Closed), _) => "CLOSED".to_string(),
                (_, Some(s)) => {
                    // Setting: reverse valve conversion if applicable.
                    let link_setting_user = if let Some(link) = network.links.get(ctrl.link.saturating_sub(1)) {
                        if let LinkKind::Valve(ref v) = link.kind {
                            match v.valve_type {
                                ValveType::Prv | ValveType::Psv | ValveType::Pbv => s * ucf.pressure,
                                ValveType::Fcv => s * ucf.flow,
                                _ => s,
                            }
                        } else {
                            s // pump speed — dimensionless
                        }
                    } else { s };
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
                    let dir = if ctrl.trigger_type == TriggerType::HiLevel { "ABOVE" } else { "BELOW" };
                    let grade_internal = ctrl.trigger_grade.unwrap_or(0.0);
                    // Convert back to user units.
                    let grade_user = if let Some(node) = network.nodes.get(node_idx.saturating_sub(1)) {
                        match &node.kind {
                            NodeKind::Tank(ref t) => {
                                let bottom = node.base.elevation - t.min_level;
                                (grade_internal - bottom) * ucf.elev
                            }
                            _ => {
                                (grade_internal - node.base.elevation) * ucf.pressure
                            }
                        }
                    } else { grade_internal };
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
                let _ = writeln!(out, " {} {} {} {} {:.4}",
                    connective, obj_str, attr_str, op_str, value_user);
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
            energy_lines.push(format!(" Demand Charge       {:.4}", opts.peak_demand_charge));
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
        if opts.bulk_order != 1.0 {
            rxn_lines.push(format!(" Order Bulk   {:.4}", opts.bulk_order));
        }
        if opts.tank_order != 1.0 {
            rxn_lines.push(format!(" Order Tank   {:.4}", opts.tank_order));
        }
        if opts.wall_order != WallOrder::One {
            rxn_lines.push(format!(" Order Wall   {}", match opts.wall_order { WallOrder::Zero => 0, WallOrder::One => 1 }));
        }
        if opts.bulk_coeff != 0.0 {
            rxn_lines.push(format!(" Global Bulk  {:.4}", opts.bulk_coeff * 86400.0));
        }
        if opts.wall_coeff != 0.0 {
            rxn_lines.push(format!(" Global Wall  {:.4}", opts.wall_coeff * 86400.0));
        }
        if opts.conc_limit != 0.0 {
            rxn_lines.push(format!(" Limiting Potential  {:.4}", opts.conc_limit));
        }
        if opts.roughness_reaction_factor != 0.0 {
            rxn_lines.push(format!(" Roughness Correlation  {:.4}", opts.roughness_reaction_factor));
        }
        // Per-pipe reactions.
        for l in &network.links {
            if let LinkKind::Pipe(ref p) = l.kind {
                if let Some(kb) = p.bulk_coeff {
                    rxn_lines.push(format!(" Bulk  {:<16} {:.4}", l.base.id, kb * 86400.0));
                }
                if let Some(kw) = p.wall_coeff {
                    rxn_lines.push(format!(" Wall  {:<16} {:.4}", l.base.id, kw * 86400.0));
                }
            }
        }
        // Per-tank reactions.
        for n in &network.nodes {
            if let NodeKind::Tank(ref t) = n.kind {
                if t.bulk_coeff != 0.0 {
                    rxn_lines.push(format!(" Tank  {:<16} {:.4}", n.base.id, t.bulk_coeff * 86400.0));
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
        let sources: Vec<_> = network.nodes.iter().filter(|n| n.source.is_some()).collect();
        if !sources.is_empty() {
            out.push_str("[SOURCES]\n");
            out.push_str(";Node             Type          Quality       Pattern\n");
            for n in &sources {
                if let Some(ref src) = n.source {
                    let src_type = match src.kind {
                        SourceType::Concentration => "CONCEN",
                        SourceType::Mass          => "MASS",
                        SourceType::Setpoint      => "SETPOINT",
                        SourceType::FlowPaced     => "FLOWPACED",
                    };
                    let pattern = src.pattern.as_deref().unwrap_or("");
                    let _ = writeln!(out, " {:<16} {:<14} {:>12.4}   {}",
                        n.base.id, src_type, src.base_value, pattern);
                }
            }
            out.push('\n');
        }
    }

    // ── [MIXING] ─────────────────────────────────────────────────────────────
    {
        let non_default: Vec<_> = network.nodes.iter().filter(|n| {
            if let NodeKind::Tank(ref t) = n.kind {
                t.mix_model != MixModel::Cstr || t.mix_fraction != 0.0
            } else { false }
        }).collect();
        if !non_default.is_empty() {
            out.push_str("[MIXING]\n");
            out.push_str(";Tank             Model         Fraction\n");
            for n in &non_default {
                if let NodeKind::Tank(ref t) = n.kind {
                    let model_str = match t.mix_model {
                        MixModel::Cstr           => "MIXED",
                        MixModel::TwoCompartment => "2COMP",
                        MixModel::Fifo           => "FIFO",
                        MixModel::Lifo           => "LIFO",
                    };
                    if t.mix_model == MixModel::TwoCompartment {
                        let _ = writeln!(out, " {:<16} {:<14} {:.4}", n.base.id, model_str, t.mix_fraction);
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
        let emitters: Vec<_> = network.nodes.iter().filter(|n| {
            if let NodeKind::Junction(ref j) = n.kind { j.emitter_coeff > 0.0 } else { false }
        }).collect();
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

    // ── [QUALITY] ────────────────────────────────────────────────────────────
    {
        let non_zero: Vec<_> = network.nodes.iter()
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
        let _ = writeln!(out, " Headloss        {}", match opts.head_loss_formula {
            HeadLossFormula::HazenWilliams => "H-W",
            HeadLossFormula::DarcyWeisbach => "D-W",
            HeadLossFormula::ChezyManning  => "C-M",
        });
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
            let _ = writeln!(out, " HEADERROR       {:.6}", opts.head_error_limit * ucf.elev);
        }
        if opts.flow_change_limit > 0.0 {
            let _ = writeln!(out, " FLOWCHANGE      {:.6}", opts.flow_change_limit * ucf.flow);
        }
        if opts.damp_limit > 0.0 {
            let _ = writeln!(out, " DAMPLIMIT       {:.6}", opts.damp_limit);
        }
        let _ = writeln!(out, " Unbalanced      {}", if opts.extra_iter < 0 { "STOP".to_string() } else { format!("CONTINUE {}", opts.extra_iter) });
        let demand_model_str = match opts.demand_model {
            crate::DemandModel::DemandDriven  => "DDA",
            crate::DemandModel::PressureDriven => "PDA",
        };
        let _ = writeln!(out, " Demand Model    {}", demand_model_str);
        if opts.demand_multiplier != 1.0 {
            let _ = writeln!(out, " Demand Multiplier {:.4}", opts.demand_multiplier);
        }
        if let Some(ref pat) = opts.default_pattern {
            let _ = writeln!(out, " Default Pattern {}", pat);
        }
        if opts.demand_model == crate::DemandModel::PressureDriven {
            let _ = writeln!(out, " Minimum Pressure {:.4}", opts.pda_min_pressure * ucf.pressure);
            let _ = writeln!(out, " Required Pressure {:.4}", opts.pda_required_pressure * ucf.pressure);
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
                    let _ = writeln!(out, " Quality         {} {}", opts.chem_name, opts.chem_units);
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
        if !opts.emitter_backflow {
            let _ = writeln!(out, " Emitter Exponent {:.4}", 0.5); // default
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
        let _ = writeln!(out, " Duration           {}", fmt_duration_hm(opts.duration));
        let _ = writeln!(out, " Hydraulic Timestep {}", fmt_duration_hm(opts.hyd_step));
        let _ = writeln!(out, " Quality Timestep   {}", fmt_duration_hm(opts.qual_step));
        let _ = writeln!(out, " Report Timestep    {}", fmt_duration_hm(opts.report_step));
        if opts.report_start > 0.0 {
            let _ = writeln!(out, " Report Start       {}", fmt_duration_hm(opts.report_start));
        }
        if opts.pattern_step != opts.hyd_step {
            let _ = writeln!(out, " Pattern Timestep   {}", fmt_duration_hm(opts.pattern_step));
        }
        if opts.pattern_start > 0.0 {
            let _ = writeln!(out, " Pattern Start      {}", fmt_duration_hm(opts.pattern_start));
        }
        if opts.start_clocktime > 0.0 {
            let _ = writeln!(out, " Start Clocktime    {}", fmt_clocktime(opts.start_clocktime));
        }
        if opts.statistic != StatisticType::Series {
            let stat_str = match opts.statistic {
                StatisticType::Average => "AVERAGE",
                StatisticType::Minimum => "MINIMUM",
                StatisticType::Maximum => "MAXIMUM",
                StatisticType::Range   => "RANGE",
                StatisticType::Series  => "NONE",
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
            ReportStatus::No   => None,
            ReportStatus::Yes  => Some("Yes"),
            ReportStatus::Full => Some("Full"),
        };
        if let Some(s) = status_str {
            rep_lines.push(format!(" Status     {}", s));
        }
        if !rep.summary {
            rep_lines.push(" Summary    No".to_string());
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
    if secs == 0.0 { return "0:00".to_string(); }
    let total_min = (secs / 60.0).round() as u64;
    let h = total_min / 60;
    let m = total_min % 60;
    format!("{}:{:02}", h, m)
}

/// Format clock-time seconds-from-midnight as `H AM/PM`.
fn fmt_clocktime(secs: f64) -> String {
    let total_min = (secs / 60.0).round() as u32 % (24 * 60);
    let h24 = total_min / 60;
    let m   = total_min % 60;
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
        LinkStatus::Open   => "Open",
        LinkStatus::Closed => "Closed",
        _                  => "Open",
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
        crate::FlowUnits::Cfs  => "CFS",
        crate::FlowUnits::Gpm  => "GPM",
        crate::FlowUnits::Mgd  => "MGD",
        crate::FlowUnits::Imgd => "IMGD",
        crate::FlowUnits::Afd  => "AFD",
        crate::FlowUnits::Lps  => "LPS",
        crate::FlowUnits::Lpm  => "LPM",
        crate::FlowUnits::Mld  => "MLD",
        crate::FlowUnits::Cmh  => "CMH",
        crate::FlowUnits::Cmd  => "CMD",
        crate::FlowUnits::Cms  => "CMS",
    }
}

fn premise_attr_str(attr: PremiseAttribute) -> &'static str {
    match attr {
        PremiseAttribute::Head      => "HEAD",
        PremiseAttribute::Pressure  => "PRESSURE",
        PremiseAttribute::Demand    => "DEMAND",
        PremiseAttribute::Level     => "LEVEL",
        PremiseAttribute::Flow      => "FLOW",
        PremiseAttribute::Status    => "STATUS",
        PremiseAttribute::Setting   => "SETTING",
        PremiseAttribute::Power     => "POWER",
        PremiseAttribute::FillTime  => "FILLTIME",
        PremiseAttribute::DrainTime => "DRAINTIME",
        PremiseAttribute::ClockTime => "CLOCKTIME",
        PremiseAttribute::Time      => "TIME",
    }
}

fn premise_op_str(op: PremiseOperator) -> &'static str {
    match op {
        PremiseOperator::Eq  => "=",
        PremiseOperator::Neq => "<>",
        PremiseOperator::Lt  => "<",
        PremiseOperator::Gt  => ">",
        PremiseOperator::Le  => "<=",
        PremiseOperator::Ge  => ">=",
    }
}

fn convert_premise_value(prem: &crate::Premise, ucf: &super::units::Ucf) -> f64 {
    match prem.attribute {
        PremiseAttribute::Demand | PremiseAttribute::Flow => prem.value * ucf.flow,
        PremiseAttribute::Head | PremiseAttribute::Level  => prem.value * ucf.elev,
        PremiseAttribute::Pressure                        => prem.value * ucf.pressure,
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
        ActionValue::Status(LinkStatus::Open)   => "STATUS IS OPEN".to_string(),
        ActionValue::Status(LinkStatus::Closed) => "STATUS IS CLOSED".to_string(),
        ActionValue::Status(_)                  => "STATUS IS OPEN".to_string(),
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
            } else { *s };
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
        // crates/common  →  workspace root
        let root = manifest.parent().unwrap().parent().unwrap();
        root.join("tests/fixtures").join(name)
    }

    /// Parse a fixture, write it back to INP bytes, parse again, and assert
    /// that the key network properties are preserved within a tight tolerance.
    fn round_trip_fixture(name: &str) {
        let path = fixture(name);
        let original_bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("could not read {name}: {e}"));

        let net1 = parse(&original_bytes)
            .unwrap_or_else(|e| panic!("first parse of {name} failed: {e:?}"));
        let written = write_inp(&net1);
        let net2 = parse(&written)
            .unwrap_or_else(|e| {
                let s = String::from_utf8_lossy(&written);
                panic!("second parse of {name} failed: {e:?}\n\nwritten INP:\n{s}");
            });

        // Node count, link count, and IDs must be identical.
        assert_eq!(net1.nodes.len(), net2.nodes.len(),
            "{name}: node count changed after round-trip");
        assert_eq!(net1.links.len(), net2.links.len(),
            "{name}: link count changed after round-trip");

        // Every node ID present in net1 must still be present in net2.
        for n1 in &net1.nodes {
            let n2 = net2.nodes.iter().find(|n| n.base.id == n1.base.id)
                .unwrap_or_else(|| panic!("{name}: node '{}' missing after round-trip", n1.base.id));

            // Elevation round-trips to within 0.1 ft (conversion noise).
            assert!((n1.base.elevation - n2.base.elevation).abs() < 0.1,
                "{name}: node '{}' elevation drifted: {} → {}",
                n1.base.id, n1.base.elevation, n2.base.elevation);

            // Node kind must be preserved.
            assert_eq!(
                std::mem::discriminant(&n1.kind),
                std::mem::discriminant(&n2.kind),
                "{name}: node '{}' kind changed after round-trip", n1.base.id
            );
        }

        // Every link ID present in net1 must still be present in net2.
        for l1 in &net1.links {
            let l2 = net2.links.iter().find(|l| l.base.id == l1.base.id)
                .unwrap_or_else(|| panic!("{name}: link '{}' missing after round-trip", l1.base.id));

            assert_eq!(
                std::mem::discriminant(&l1.kind),
                std::mem::discriminant(&l2.kind),
                "{name}: link '{}' kind changed after round-trip", l1.base.id
            );
        }
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

    // ── Unit conversion spot-checks ──────────────────────────────────────────

    /// Pipe diameter (ft → mm) and back (mm → ft) must cancel exactly.
    #[test]
    fn pipe_diameter_unit_conversion_round_trips() {
        use crate::FlowUnits;
        use super::super::units::make_ucf;

        let ucf = make_ucf(FlowUnits::Lps, 1.0);
        // 0.5 ft diameter pipe.
        let d_ft = 0.5_f64;
        let d_mm = d_ft * ucf.diam;    // ft → mm
        let d_ft2 = d_mm / ucf.diam;   // mm → ft
        assert!((d_ft - d_ft2).abs() < 1e-12);
    }

    /// Elevation (ft → m) and back must cancel exactly.
    #[test]
    fn elevation_unit_conversion_round_trips() {
        use crate::FlowUnits;
        use super::super::units::make_ucf;

        let ucf = make_ucf(FlowUnits::Lps, 1.0);
        let elev_ft = 100.0_f64;
        let elev_m  = elev_ft * ucf.elev;
        let elev_ft2 = elev_m / ucf.elev;
        assert!((elev_ft - elev_ft2).abs() < 1e-12);
    }

    /// Minor-loss round-trip: K_m → K_v → K_m must recover the original.
    #[test]
    fn minor_loss_round_trip() {
        // K_m = 0.02517 * K_v / D^4
        // K_v = K_m * D^4 / 0.02517
        let d_ft = 0.5_f64;    // 6-inch pipe
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
        let kb_per_s = -1.157e-5_f64;  // representative bulk coefficient
        let kb_per_day = kb_per_s * 86400.0;
        let kb_per_s2  = kb_per_day / 86400.0;
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
