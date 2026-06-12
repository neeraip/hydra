// units — shared unit conversion logic for INP parsing.
//
// All numeric values in INP files are in the user-declared unit system.
// The core requires everything in SI (m³/s for flow, m for head/length/elevation,
// m for pipe diameter, m³ for volume, W for power).
// This module converts all parsed values in-place.

use crate::{
    ActionValue, Curve, CurveKind, DemandModel, FlowUnits, HeadLossFormula, Link, LinkKind, Node,
    NodeKind, PremiseAttribute, PremiseObject, Rule, SimpleControl, SimulationOptions, ValveType,
    WallOrder,
};

/// Unit conversion factors: `value_internal = value_user / factor`.
///
/// Each field holds the number of user units per internal SI unit.  Dividing a
/// user-unit value by the corresponding factor yields the internal SI value.
pub struct Ucf {
    /// User flow units per m³/s (e.g. 15850.3 for GPM).
    pub flow: f64,
    /// User length units per m — covers head, elevation, and pipe length.
    pub elev: f64,
    /// User diameter units per m (mm → m: 1000; inches → m: 39.3701).
    pub diam: f64,
    /// User volume units per m³.
    pub vol: f64,
    /// User power units per W (kW → W: 0.001; HP → W: 0.001341).
    pub power: f64,
    /// User pressure units per m of head (metres: 1.0; psi: ~0.704).
    pub pressure: f64,
}

/// Build a [`Ucf`] from the declared flow-unit system and specific gravity.
///
/// `flow_units` determines whether SI or US-customary factors are used.
/// `specific_gravity` scales pressure conversion (typically 1.0 for water).
pub fn make_ucf(flow_units: FlowUnits, specific_gravity: f64) -> Ucf {
    let is_si = matches!(
        flow_units,
        FlowUnits::Lps
            | FlowUnits::Lpm
            | FlowUnits::Mld
            | FlowUnits::Cmh
            | FlowUnits::Cmd
            | FlowUnits::Cms
    );

    // Flow factors: user flow units per m³/s.
    // value_internal_m3s = value_user / flow.
    let flow = match flow_units {
        FlowUnits::Cfs => 35.315,
        FlowUnits::Gpm => 15_850.3,
        FlowUnits::Mgd => 22.824,
        FlowUnits::Imgd => 19.005,
        FlowUnits::Afd => 70.045,
        FlowUnits::Lps => 1_000.0,
        FlowUnits::Lpm => 60_000.0,
        FlowUnits::Mld => 86.400,
        FlowUnits::Cmh => 3_600.0,
        FlowUnits::Cmd => 86_400.0,
        FlowUnits::Cms => 1.0,
    };

    if is_si {
        Ucf {
            flow,
            elev: 1.0,     // m → m
            diam: 1_000.0, // mm → m
            vol: 1.0,      // m³ → m³
            power: 0.001,  // kW → W (÷ 0.001 = × 1000)
            pressure: 1.0, // m of head → m of head
        }
    } else {
        Ucf {
            flow,
            elev: 3.2808,                        // ft → m
            diam: 39.370,                        // inches → m
            vol: 35.315,                         // ft³ → m³
            power: 0.001_341,                    // HP → W (1 HP = 745.7 W)
            pressure: 1.4219 * specific_gravity, // PSI → m of head
        }
    }
}

/// Convert all parsed values from user units to internal SI units (m³/s, m, W).
pub fn apply_unit_conversion(
    options: &mut SimulationOptions,
    nodes: &mut [Node],
    links: &mut [Link],
    curves: &mut [Curve],
    controls: &mut [SimpleControl],
    rules: &mut [Rule],
) {
    let ucf = make_ucf(options.flow_units, options.specific_gravity);
    let is_dw = options.head_loss_formula == HeadLossFormula::DarcyWeisbach;

    // ── Viscosity & diffusivity (EPANET multiplier convention) ───────────────
    // Values > threshold are multipliers of the EPANET defaults; smaller
    // values are absolute in user length²/s units and need conversion.
    // A value equal to the data-model default (VISCOS / DIFFUS) means the
    // INP file did NOT specify the option — keep the default unchanged.
    {
        const VISCOS: f64 = 1.022e-6; // m²/s @ 20°C
        const DIFFUS: f64 = 1.208e-9; // m²/s (chlorine @ 20°C)
        let len2 = ucf.elev * ucf.elev;

        let v = options.viscosity;
        if (v - VISCOS).abs() < 1e-20 {
            // already correct — keep VISCOS
        } else if v > 1.0e-3 {
            options.viscosity = v * VISCOS;
        } else {
            options.viscosity = v / len2;
        }

        let d = options.diffusivity;
        if (d - DIFFUS).abs() < 1e-20 {
            // already correct — keep DIFFUS
        } else if d > 1.0e-4 {
            options.diffusivity = d * DIFFUS;
        } else {
            options.diffusivity = d / len2;
        }
    }

    // ── Nodes: elevations ────────────────────────────────────────────────────
    for node in nodes.iter_mut() {
        node.base.elevation /= ucf.elev;
    }

    // ── Junctions: demands, emitter coefficients ─────────────────────────────
    for node in nodes.iter_mut() {
        if let NodeKind::Junction(ref mut j) = node.kind {
            for d in &mut j.demands {
                d.base_demand /= ucf.flow;
            }
            if j.emitter_coeff > 0.0 {
                let qexp = 1.0 / j.emitter_exp; // reciprocal exponent (EPANET Qexp)
                let ucf_emit = ucf.flow.powf(qexp) / ucf.pressure;
                j.emitter_coeff = ucf_emit / j.emitter_coeff.powf(qexp);
            }
        }
    }

    // ── Tanks: levels, diameter, bulk coeff ──────────────────────────────────
    for node in nodes.iter_mut() {
        if let NodeKind::Tank(ref mut t) = node.kind {
            t.initial_level /= ucf.elev;
            t.min_level /= ucf.elev;
            t.max_level /= ucf.elev;
            t.diameter /= ucf.elev; // tank diameter is in ft/m, not inches/mm
            t.min_volume /= ucf.vol; // user volume units → internal (ft³)
            t.bulk_coeff /= 86400.0; // per-day → per-second
                                     // Adjust elevation convention: INP uses bottom elevation,
                                     // our data model uses elevation = bottom + min_level (§2.4.4).
            node.base.elevation += t.min_level;
        }
    }

    // ── Links ────────────────────────────────────────────────────────────────
    for link in links.iter_mut() {
        match &mut link.kind {
            LinkKind::Pipe(ref mut p) => {
                p.length /= ucf.elev;
                p.diameter /= ucf.diam;
                if is_dw {
                    p.roughness /= 1000.0 * ucf.elev;
                }
                // Minor loss: velocity-head K_v → Q²-form K_m = 8·K_v/(π²·g·D⁴)
                // In SI (g = 9.81 m/s²): coefficient = 8/(π²·9.81) ≈ 0.08262
                if p.minor_loss > 0.0 {
                    let d4 = p.diameter.powi(4);
                    p.minor_loss = 0.08262 * p.minor_loss / d4;
                }
                if let Some(ref mut kb) = p.bulk_coeff {
                    *kb /= 86400.0;
                }
                if let Some(ref mut kw) = p.wall_coeff {
                    // First-order kw: length/day → m/s (÷ ucf.elev for ft→m).
                    // Zero-order kw: mass/area/day → mg/(m²·s); area is in the
                    // denominator so the ft→m correction is × ucf.elev² (spec §6.5.2).
                    match options.wall_order {
                        WallOrder::One => *kw /= 86400.0 * ucf.elev,
                        WallOrder::Zero => *kw = *kw / 86400.0 * ucf.elev.powi(2),
                    }
                }
                // FAVAD leakage coefficients: convert raw INP values
                // (C1 in mm², C2 in mm) to per-pipe discharge coefficients
                // K₁ (m^2.5/s per m^0.5) and K₂ (m^0.5/s per m^1.5).
                // Formula (SI, p.length already in m):
                //   K1 = Cd · sqrt(2g) · (C1 × 1e-6 m²/mm²) · (length_m / 100)
                //   K2 = Cd · sqrt(2g) · (C2 × 1e-3 m/mm)  · (length_m / 100)
                // where Cd = 0.6, g = 9.80665 m/s².
                if p.leak_coeff_1 > 0.0 || p.leak_coeff_2 > 0.0 {
                    // 0.6 * sqrt(2 * 9.80665) = 2.65734
                    const CD_SQRT2G: f64 = 2.65734; // Cd * sqrt(2g) in SI
                    let len_100 = p.length / 100.0; // p.length already in m
                    p.leak_coeff_1 = CD_SQRT2G * 1e-6 * p.leak_coeff_1 * len_100;
                    p.leak_coeff_2 = CD_SQRT2G * 1e-3 * p.leak_coeff_2 * len_100;
                }
            }
            LinkKind::Pump(ref mut pump) => {
                if let Some(ref mut pw) = pump.power {
                    *pw /= ucf.power;
                }
            }
            LinkKind::Valve(ref mut v) => {
                v.diameter /= ucf.diam;
                if v.minor_loss > 0.0 {
                    let d4 = v.diameter.powi(4);
                    v.minor_loss = 0.08262 * v.minor_loss / d4;
                }
                if let Some(setting) = link.base.initial_setting {
                    match v.valve_type {
                        ValveType::Prv | ValveType::Psv | ValveType::Pbv => {
                            link.base.initial_setting = Some(setting / ucf.pressure);
                        }
                        ValveType::Fcv => {
                            link.base.initial_setting = Some(setting / ucf.flow);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // ── Curves ───────────────────────────────────────────────────────────────
    for curve in curves.iter_mut() {
        match curve.kind {
            CurveKind::PumpHead => {
                for pt in &mut curve.points {
                    pt.x /= ucf.flow;
                    pt.y /= ucf.elev;
                }
            }
            CurveKind::PumpEfficiency => {
                for pt in &mut curve.points {
                    pt.x /= ucf.flow;
                }
            }
            CurveKind::TankVolume => {
                for pt in &mut curve.points {
                    pt.x /= ucf.elev;
                    pt.y /= ucf.vol;
                }
            }
            CurveKind::GpvHeadloss => {
                for pt in &mut curve.points {
                    pt.x /= ucf.flow;
                    pt.y /= ucf.elev;
                }
            }
            _ => {}
        }
    }

    // ── Simple controls: convert grades and action settings ─────────────────
    for ctrl in controls.iter_mut() {
        if let (Some(node_1based), Some(ref mut grade)) =
            (ctrl.trigger_node, ctrl.trigger_grade.as_mut())
        {
            let node_idx = node_1based - 1;
            let elev = nodes[node_idx].base.elevation; // already in m
            match &nodes[node_idx].kind {
                NodeKind::Tank(ref t) => {
                    // INP grade is a level above the tank bottom.
                    // Our elevation = bottom + min_level, so bottom = elev - min_level.
                    let bottom = elev - t.min_level;
                    **grade = bottom + **grade / ucf.elev;
                }
                _ => {
                    **grade = elev + **grade / ucf.pressure;
                }
            }
        }

        // Convert action_setting to internal units (EPANET convertunits).
        // Only valve setting types need conversion; pump speed is dimensionless.
        if let Some(ref mut setting) = ctrl.action_setting {
            let link_idx = ctrl.link.wrapping_sub(1);
            if let Some(link) = links.get(link_idx) {
                if let LinkKind::Valve(ref v) = link.kind {
                    match v.valve_type {
                        ValveType::Prv | ValveType::Psv | ValveType::Pbv => {
                            *setting /= ucf.pressure;
                        }
                        ValveType::Fcv => {
                            *setting /= ucf.flow;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // ── Rule actions: convert valve settings (EPANET updateruleunits) ────────
    for rule in rules.iter_mut() {
        for action in rule
            .then_actions
            .iter_mut()
            .chain(rule.else_actions.iter_mut())
        {
            if let ActionValue::Setting(ref mut setting) = action.value {
                let link_idx = action.link.wrapping_sub(1);
                if let Some(link) = links.get(link_idx) {
                    if let LinkKind::Valve(ref v) = link.kind {
                        match v.valve_type {
                            ValveType::Prv | ValveType::Psv | ValveType::Pbv => {
                                *setting /= ucf.pressure;
                            }
                            ValveType::Fcv => {
                                *setting /= ucf.flow;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // ── Rule premises: convert threshold values to internal units ────────────
    for rule in rules.iter_mut() {
        for premise in rule.premises.iter_mut() {
            match premise.attribute {
                PremiseAttribute::Demand | PremiseAttribute::Flow => {
                    premise.value /= ucf.flow;
                }
                PremiseAttribute::Head | PremiseAttribute::Level => {
                    premise.value /= ucf.elev;
                }
                PremiseAttribute::Pressure => {
                    premise.value /= ucf.pressure;
                }
                PremiseAttribute::Setting => {
                    // Setting conversion depends on whether the premise refers
                    // to a pressure valve or a flow control valve.
                    if let PremiseObject::Link(link_1based) = premise.object {
                        let link_idx = link_1based.wrapping_sub(1);
                        if let Some(link) = links.get(link_idx) {
                            if let LinkKind::Valve(ref v) = link.kind {
                                match v.valve_type {
                                    ValveType::Prv | ValveType::Psv | ValveType::Pbv => {
                                        premise.value /= ucf.pressure;
                                    }
                                    ValveType::Fcv => {
                                        premise.value /= ucf.flow;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                // Power is in kW both user-side and internally (EPANET does
                // not convert power premises). Status, time attributes, and
                // fill/drain time need no conversion.
                PremiseAttribute::Power
                | PremiseAttribute::Status
                | PremiseAttribute::FillTime
                | PremiseAttribute::DrainTime
                | PremiseAttribute::Time
                | PremiseAttribute::ClockTime => {}
            }
        }
    }

    // ── Options ──────────────────────────────────────────────────────────────
    options.flow_change_limit /= ucf.flow;
    options.head_error_limit /= ucf.elev;
    if options.demand_model == DemandModel::PressureDriven {
        options.pda_min_pressure /= ucf.pressure;
        options.pda_required_pressure /= ucf.pressure;
    }
    options.bulk_coeff /= 86400.0;
    // Same length-dimension correction as per-pipe kw (spec §6.5.2).
    match options.wall_order {
        WallOrder::One => {
            options.wall_coeff /= 86400.0 * ucf.elev;
            options.roughness_reaction_factor /= 86400.0 * ucf.elev;
        }
        WallOrder::Zero => {
            options.wall_coeff = options.wall_coeff / 86400.0 * ucf.elev.powi(2);
            options.roughness_reaction_factor =
                options.roughness_reaction_factor / 86400.0 * ucf.elev.powi(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_ucf_us_units_uses_expected_factors() {
        let u = make_ucf(FlowUnits::Gpm, 1.0);
        assert!((u.flow - 15_850.3).abs() < 1e-3);
        assert!((u.elev - 3.2808).abs() < 1e-12);
        assert!((u.diam - 39.370).abs() < 1e-12);
        assert!((u.power - 0.001_341).abs() < 1e-12);
        assert!((u.pressure - 1.4219).abs() < 1e-9);
    }

    #[test]
    fn make_ucf_us_pressure_scales_with_specific_gravity() {
        let u = make_ucf(FlowUnits::Cfs, 0.85);
        assert!((u.pressure - (1.4219 * 0.85)).abs() < 1e-9);
    }

    #[test]
    fn make_ucf_si_units_uses_expected_factors() {
        let u = make_ucf(FlowUnits::Lps, 1.0);
        assert!((u.flow - 1_000.0).abs() < 1e-9);
        assert!((u.elev - 1.0).abs() < 1e-12);
        assert!((u.diam - 1_000.0).abs() < 1e-12);
        assert!((u.power - 0.001).abs() < 1e-12);
        assert!((u.pressure - 1.0).abs() < 1e-12);
    }
}
