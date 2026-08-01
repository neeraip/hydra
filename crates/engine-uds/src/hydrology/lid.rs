//! Control measures (§3.4): layered moisture-accounting units — surface,
//! optional pavement, soil, and storage with an optional underdrain — the
//! eight unit types as configurations of one template, advanced by the
//! normative limiter cascade: every flux clipped to the volume actually
//! present, mass-conserving by construction. This is the spec's recorded
//! exception to §3.5 integration: the clipped fluxes are discontinuous in
//! state, and the balance form is exact for the rates given.

use super::infiltration::{InfilFactors, InfilState};
use crate::io::options::InfiltrationModel;
use crate::model::{Infiltration, LidControl, LidKind, LidUsage};

/// One deployed unit's state (depths m, moisture as content).
pub struct LidUnit {
    kind: LidKind,
    /// Total deployed area (count × unit area, m²).
    pub area: f64,
    /// Fractions of the parcel's impervious and pervious runoff captured.
    pub from_imperv: f64,
    pub from_perv: f64,
    /// Surface outflow returns to the pervious sub-area instead of
    /// leaving the parcel.
    pub to_pervious: bool,
    /// Drain routing: None = the parcel's outlet.
    pub drain_to: Option<crate::model::ParcelOutlet>,
    // Layer parameters (absent layers zeroed).
    surf_berm: f64,
    surf_void: f64,
    surf_alpha: f64,
    pave_thick: f64,
    pave_ksat: f64,
    soil_thick: f64,
    soil_por: f64,
    soil_fc: f64,
    soil_wp: f64,
    soil_ksat: f64,
    soil_kslope: f64,
    soil_suction: f64,
    stor_thick: f64,
    stor_void: f64,
    stor_ksat: f64,
    sealed: bool,
    drain: Option<DrainParams>,
    mat_thick: f64,
    mat_void: f64,
    /// Pavement clogging capacity as a treatable inflow depth (m):
    /// clogging factor × thickness × void fraction × pervious paver
    /// fraction. 0 = never clogs.
    pave_clog: f64,
    /// Pavement permeability regeneration cycle (days) and degree.
    regen_days: f64,
    regen_degree: f64,
    /// Storage clogging capacity as a treatable inflow depth (m).
    stor_clog: f64,
    /// Drain multiplier curve, raw file points; empty = none.
    drain_curve: Vec<(f64, f64)>,
    /// The file's rain-depth unit (m), the curve's head abscissa (§14.6).
    head_unit: f64,
    /// Trapezoidal geometry, swales only.
    swale: Option<SwaleGeom>,
    /// The swale's own Green–Ampt clone of the parent parcel's
    /// parameters; absent, the parcel's native rate is used (§3.4).
    swale_infil: Option<InfilState>,
    /// The swale's converged depth rate from the previous step (m/s),
    /// the trapezoid's start weight.
    swale_f_old: f64,
    // State.
    d1: f64,
    theta2: f64,
    d3: f64,
    drain_open: bool,
    drain_delay_left: f64,
    /// Cumulative unit inflow depth (m) against the pavement clogging
    /// account — discounted by each regeneration.
    vol_treated: f64,
    /// Cumulative unit inflow depth (m) against the storage clogging
    /// account — never discounted.
    total_inflow: f64,
    /// The next regeneration boundary (elapsed days).
    next_regen: f64,
    /// Cumulative Green–Ampt-style intake head state: event volume (m).
    f_intake: f64,
    /// Rates from the last step (m/s over the unit area).
    pub overflow: f64,
    pub drain_flow: f64,
    pub exfiltration: f64,
}

/// A swale's trapezoidal section, per deployed unit (§3.4): widths
/// floored at 0.1524 m with the side slope recomputed to keep the
/// section consistent.
#[derive(Clone, Copy)]
struct SwaleGeom {
    /// Top width at the berm (m).
    top: f64,
    /// Bottom width (m).
    bot: f64,
    /// Side slope (run per rise), recomputed if the bottom floors.
    slope: f64,
    /// Unit length (m): unit area over top width.
    len: f64,
}

struct DrainParams {
    coeff: f64,
    exponent: f64,
    offset: f64,
    delay: f64,
    h_open: f64,
    h_close: f64,
}

/// Why a unit cannot be evaluated by this build stage.
#[derive(Debug, Clone, PartialEq)]
pub enum LidRefusal {
    /// A configuration this stage does not evaluate yet.
    Unsupported(&'static str),
    /// A design the data cannot support at all.
    Invalid(String),
}

/// Per-step forcing shared by every unit on a parcel (§3.4).
pub struct LidForcing {
    /// Direct rain plus captured runoff (m/s over the unit area).
    pub inflow: f64,
    /// The parcel's rainfall rate alone (m/s) — the rain barrel's
    /// dryness clock.
    pub rain: f64,
    /// Potential evapotranspiration (m/s).
    pub evap: f64,
    /// The parcel's native infiltration rate (m/s) — the swale's
    /// fallback bed rate.
    pub native_infil: f64,
    /// Monthly infiltration scaling handles.
    pub fac: InfilFactors,
    /// Elapsed simulation time (days), for regeneration cycles.
    pub elapsed_days: f64,
}

impl LidUnit {
    /// Build a deployed unit from its design and usage records. A
    /// vegetative swale also clones the hosting parcel's infiltration
    /// where that model is (modified) Green–Ampt (§3.4); `curves` and
    /// `us_units` resolve the drain multiplier curve and its file-unit
    /// head abscissa (§14.6).
    pub fn build(
        ctl: &LidControl,
        usage: &LidUsage,
        parcel_infil: Option<&Infiltration>,
        model: InfiltrationModel,
        curves: &[crate::model::Curve],
        us_units: bool,
    ) -> Result<LidUnit, LidRefusal> {
        let kind = ctl.kind.ok_or(LidRefusal::Unsupported(
            "a control measure without a declared type",
        ))?;
        let mut swale = None;
        let mut swale_infil = None;
        if kind == LidKind::VegetativeSwale {
            let s = ctl.surface.as_ref();
            let thickness = s.map_or(0.0, |x| x.thickness);
            if thickness <= 0.0 || usage.width <= 0.0 {
                return Err(LidRefusal::Invalid(format!(
                    "swale design {} needs a positive berm height and unit width",
                    ctl.id
                )));
            }
            // Widths floor at 0.1524 m; a floored bottom recomputes the
            // side slope to keep the section consistent (§3.4).
            let top = usage.width.max(0.1524);
            let mut slope = s.map_or(0.0, |x| x.side_slope);
            let mut bot = top - 2.0 * slope * thickness;
            if bot < 0.1524 {
                bot = 0.1524;
                slope = 0.5 * (top - 0.1524) / thickness;
            }
            swale = Some(SwaleGeom {
                top,
                bot,
                slope,
                len: usage.area / top,
            });
            if matches!(
                model,
                InfiltrationModel::GreenAmpt | InfiltrationModel::ModifiedGreenAmpt
            ) {
                swale_infil = parcel_infil
                    .map(|inf| InfilState::build(inf, InfiltrationModel::ModifiedGreenAmpt));
            }
        }
        let area = f64::from(usage.count) * usage.area;
        let s = ctl.surface.as_ref();
        let (mut d1, mut theta2, mut d3) = (0.0, 0.0, 0.0);
        let soil = ctl.soil.as_ref();
        let stor = ctl.storage.as_ref();
        // Initial saturation pre-fills soil and storage (§3.4).
        if usage.init_saturation > 0.0 {
            if let Some(so) = soil {
                theta2 =
                    so.wilting_point + usage.init_saturation * (so.porosity - so.wilting_point);
            }
            if let Some(st) = stor {
                d3 = usage.init_saturation * st.thickness;
            }
        } else if let Some(so) = soil {
            theta2 = so.wilting_point;
        }
        let _ = &mut d1;
        let surf_alpha = match s {
            Some(sf) if sf.roughness > 0.0 && sf.slope > 0.0 => sf.slope.sqrt() / sf.roughness,
            _ => 0.0,
        };
        Ok(LidUnit {
            kind,
            area,
            from_imperv: usage.from_impervious,
            from_perv: usage.from_pervious,
            to_pervious: usage.to_pervious,
            drain_to: usage.drain_to,
            surf_berm: s.map_or(0.0, |x| x.thickness),
            surf_void: s.map_or(1.0, |x| x.void_frac.max(1e-6)),
            surf_alpha,
            pave_thick: ctl.pavement.as_ref().map_or(0.0, |x| x.thickness),
            pave_ksat: ctl.pavement.as_ref().map_or(0.0, |x| x.k_sat),
            soil_thick: soil.map_or(0.0, |x| x.thickness),
            soil_por: soil.map_or(0.0, |x| x.porosity),
            soil_fc: soil.map_or(0.0, |x| x.field_capacity),
            soil_wp: soil.map_or(0.0, |x| x.wilting_point),
            soil_ksat: soil.map_or(0.0, |x| x.k_sat),
            soil_kslope: soil.map_or(0.0, |x| x.k_slope),
            soil_suction: soil.map_or(0.0, |x| x.suction),
            stor_thick: stor.map_or(0.0, |x| x.thickness),
            stor_void: stor.map_or(0.0, |x| x.void_frac.max(1e-6)),
            stor_ksat: stor.map_or(0.0, |x| x.k_sat),
            // Green roofs and rain barrels are sealed (§3.4).
            sealed: matches!(kind, LidKind::GreenRoof | LidKind::RainBarrel)
                || stor.is_some_and(|x| x.covered),
            drain: ctl.drain.as_ref().map(|d| DrainParams {
                coeff: d.coeff,
                exponent: d.exponent,
                offset: d.offset,
                delay: d.delay,
                h_open: d.h_open,
                h_close: d.h_close,
            }),
            mat_thick: ctl.drain_mat.as_ref().map_or(0.0, |x| x.thickness),
            mat_void: ctl
                .drain_mat
                .as_ref()
                .map_or(0.0, |x| x.void_frac.max(1e-6)),
            // Clogging factors scale each layer's void depth into a
            // treatable inflow depth (§3.4).
            pave_clog: ctl.pavement.as_ref().map_or(0.0, |x| {
                if x.thickness > 0.0 {
                    x.clog_factor * x.thickness * x.void_frac * (1.0 - x.imperv_frac)
                } else {
                    0.0
                }
            }),
            regen_days: ctl.pavement.as_ref().map_or(0.0, |x| x.regen_days),
            regen_degree: ctl.pavement.as_ref().map_or(0.0, |x| x.regen_degree),
            stor_clog: stor.map_or(0.0, |x| {
                if x.thickness > 0.0 {
                    x.clog_factor * x.thickness * x.void_frac
                } else {
                    0.0
                }
            }),
            drain_curve: ctl
                .drain
                .as_ref()
                .and_then(|d| d.curve)
                .map_or(Vec::new(), |ci| curves[ci].points.clone()),
            head_unit: if us_units { 0.0254 } else { 0.001 },
            swale,
            swale_infil,
            swale_f_old: 0.0,
            d1,
            theta2,
            d3,
            drain_open: false,
            drain_delay_left: 0.0,
            vol_treated: 0.0,
            total_inflow: 0.0,
            next_regen: ctl.pavement.as_ref().map_or(0.0, |x| x.regen_days),
            f_intake: 0.0,
            overflow: 0.0,
            drain_flow: 0.0,
            exfiltration: 0.0,
        })
    }

    /// Water currently held per unit area (m).
    pub fn stored_depth(&self) -> f64 {
        self.d1 * self.surf_void + self.theta2 * self.soil_thick + self.d3 * self.stor_void
    }

    /// Advance one hydrology step under the shared parcel forcing.
    /// Outflow rates land in `overflow`, `drain_flow`, `exfiltration`.
    pub fn step(&mut self, f: &LidForcing, dt: f64) {
        match self.kind {
            LidKind::RooftopDisconnection => self.step_rooftop(f.inflow, dt),
            LidKind::RainBarrel => self.step_rain_barrel(f.inflow, f.rain, dt),
            LidKind::VegetativeSwale => {
                self.step_swale(f.inflow, f.evap, f.native_infil, f.fac, dt)
            }
            _ => self.step_layered(f, dt),
        }
        // Both clogging accounts run on cumulative unit inflow, booked
        // after the step so this step's rates saw the old totals (§3.4).
        self.vol_treated += f.inflow * dt;
        self.total_inflow += f.inflow * dt;
    }

    /// Rooftop disconnection: a lone surface whose gutter-capacity drain
    /// pre-empts overflow (§3.4).
    fn step_rooftop(&mut self, inflow: f64, dt: f64) {
        let cap =
            self.drain
                .as_ref()
                .map_or(f64::MAX, |d| if d.coeff > 0.0 { d.coeff } else { f64::MAX });
        let drained = inflow.min(cap);
        self.drain_flow = drained;
        self.overflow = inflow - drained;
        self.exfiltration = 0.0;
        let _ = dt;
    }

    /// Rain barrel: pure sealed storage; intake limited by freeboard plus
    /// concurrent drain outflow; the drain opens after its dry delay.
    fn step_rain_barrel(&mut self, inflow: f64, rain: f64, dt: f64) {
        let h = self.stor_thick;
        // Drain state: opens once dry weather has run the delay down.
        // Dryness is judged by the parcel's rainfall alone against the
        // 0.001 in/hr minimum-runoff rate (§3.4) — captured tributary
        // runoff's Manning tail must not hold the drain shut.
        let wet = rain > 7.055_6e-9;
        let mut q3 = 0.0;
        if let Some(d) = &self.drain {
            if wet {
                self.drain_delay_left = d.delay;
                self.drain_open = false;
            } else if !self.drain_open {
                self.drain_delay_left -= dt;
                if self.drain_delay_left <= 0.0 {
                    self.drain_open = true;
                }
            }
            if self.drain_open && self.d3 > d.offset {
                let h_rel = self.d3 - d.offset;
                q3 = d.coeff * h_rel.powf(d.exponent);
                if !self.drain_curve.is_empty() {
                    q3 *= curve_multiplier(&self.drain_curve, h_rel / self.head_unit);
                }
                q3 = q3.min(self.d3 * self.stor_void / dt);
            }
        }
        let intake = inflow.min((h - self.d3).max(0.0) * self.stor_void / dt + q3);
        self.overflow = inflow - intake;
        self.d3 = (self.d3 + (intake - q3) * dt / self.stor_void).clamp(0.0, h);
        self.drain_flow = q3;
        self.exfiltration = 0.0;
    }

    /// Vegetative swale: a trapezoidal channel whose ponded geometry
    /// varies with depth, advanced by the iterated trapezoidal method —
    /// equally weighted start- and end-of-step rates, a 1 mm depth
    /// tolerance, at most twenty passes, the final pass accepted as-is
    /// (§3.4).
    fn step_swale(
        &mut self,
        inflow: f64,
        evap: f64,
        native_infil: f64,
        fac: InfilFactors,
        dt: f64,
    ) {
        let Some(g) = self.swale else {
            return;
        };
        let berm = self.surf_berm;
        let unit_area = g.len * g.top;
        // Infiltration to native soil: the unit's own Green–Ampt clone of
        // the parcel's parameters, else the parcel's native rate (§3.4).
        let f_infil = match &mut self.swale_infil {
            Some(st) => st.step(dt, inflow, self.d1, fac),
            None => native_infil,
        };
        // Flux rate on depth (m/s) plus the outflow components (m³/s) at
        // a trial depth.
        let void = self.surf_void;
        let alpha = self.surf_alpha;
        let rates = move |d: f64| -> (f64, f64, f64) {
            let depth = d.min(berm);
            let surf_width = g.bot + 2.0 * g.slope * depth;
            let surf_area = g.len * surf_width;
            let flow_area = depth * (g.bot + g.slope * depth) * void;
            let volume = g.len * flow_area;
            let q_in = inflow * unit_area;
            let q_evap = (evap * surf_area).min(volume / dt);
            let q_exfil = f_infil * surf_area;
            let mut q_out = 0.0;
            if depth > 0.0 && flow_area > 0.0 {
                let wetted = g.bot + 2.0 * depth * (1.0 + g.slope * g.slope).sqrt();
                let r = flow_area / wetted;
                q_out = alpha * flow_area * r.powf(2.0 / 3.0);
            }
            let mut dvdt = q_in - q_evap - q_exfil - q_out;
            // At the berm, any net positive inflow spills onward.
            if depth >= berm && dvdt > 0.0 {
                q_out += dvdt;
                dvdt = 0.0;
            }
            (dvdt / surf_area, q_exfil, q_out)
        };
        let d_old = self.d1;
        let f_old = self.swale_f_old;
        let mut d = d_old;
        let mut out = (0.0, 0.0, 0.0);
        for _ in 0..20 {
            out = rates(d);
            let d_new = (d_old + 0.5 * (f_old + out.0) * dt).clamp(0.0, berm);
            let done = (d_new - d).abs() <= 1e-3;
            d = d_new;
            if done {
                break;
            }
        }
        self.swale_f_old = out.0;
        self.d1 = d;
        self.overflow = out.2 / unit_area;
        self.exfiltration = out.1 / unit_area;
        self.drain_flow = 0.0;
    }

    /// The swale's current ponded depth (m), for tests.
    #[cfg(test)]
    fn swale_depth(&self) -> f64 {
        self.d1
    }

    /// The layered template under the limiter cascade (§3.4).
    fn step_layered(&mut self, forcing: &LidForcing, dt: f64) {
        let (inflow, evap) = (forcing.inflow, forcing.evap);
        let has_soil = self.soil_thick > 0.0;
        let has_stor = self.stor_thick > 0.0 || self.mat_thick > 0.0;
        let stor_thick = if self.mat_thick > 0.0 {
            self.mat_thick
        } else {
            self.stor_thick
        };
        let stor_void = if self.mat_thick > 0.0 {
            self.mat_void
        } else {
            self.stor_void
        };

        // ── Nominal fluxes, then the cascade's clips in order ───────────
        // Surface intake: modified Green–Ampt on the soil parameters, the
        // pavement's clog-reduced permeability, or the storage limit.
        let avail = inflow + self.d1 * self.surf_void / dt;
        let mut f1 = if self.pave_thick > 0.0 {
            // Clog-reduced permeability: conductivity falls linearly to
            // zero as the treated-volume account approaches the clogging
            // capacity; a regeneration boundary discounts the account
            // first (§3.4).
            let mut k = self.pave_ksat;
            if self.pave_clog > 0.0 {
                if self.regen_days > 0.0 && forcing.elapsed_days >= self.next_regen {
                    self.vol_treated *= 1.0 - self.regen_degree;
                    self.next_regen += self.regen_days;
                }
                k *= 1.0 - (self.vol_treated / self.pave_clog).min(1.0);
            }
            k
        } else if has_soil {
            let deficit = (self.soil_por - self.theta2).max(0.0);
            if deficit <= 0.0 {
                self.soil_ksat
            } else {
                self.f_intake = (self.f_intake + avail.max(0.0) * dt).max(1e-9);
                self.soil_ksat * (1.0 + (self.soil_suction + self.d1) * deficit / self.f_intake)
            }
        } else {
            // Infiltration trench: one end-limited surface-to-storage flux.
            f64::MAX
        };
        f1 = f1.min(avail).max(0.0);

        // Evapotranspiration cascades top-down.
        let e1 = evap
            .min(self.d1 * self.surf_void / dt + inflow - f1)
            .max(0.0);
        let mut e2 = 0.0;
        if has_soil {
            e2 = (evap - e1)
                .min((self.theta2 - self.soil_wp).max(0.0) * self.soil_thick / dt)
                .max(0.0);
        }
        let e3 = if self.sealed {
            0.0
        } else {
            (evap - e1 - e2).min(self.d3 * stor_void / dt).max(0.0)
        };

        // Soil percolation, clipped by drainable water.
        let mut f2 = if has_soil {
            if self.theta2 > self.soil_fc {
                let k = self.soil_ksat * (-(self.soil_por - self.theta2) * self.soil_kslope).exp();
                k.min((self.theta2 - self.soil_fc) * self.soil_thick / dt)
            } else {
                0.0
            }
        } else {
            f1
        };

        // A green-roof mat with no roughness passes percolation through.
        let mat_pass = self.mat_thick > 0.0 && self.surf_alpha == 0.0;

        // Exfiltration: the storage bed's saturated conductivity —
        // clog-reduced on the never-regenerating inflow account — clipped
        // by delivery plus store; sealed units exfiltrate nothing.
        let stor_ksat_eff = if self.stor_clog > 0.0 {
            self.stor_ksat * (1.0 - (self.total_inflow / self.stor_clog).min(1.0))
        } else {
            self.stor_ksat
        };
        let mut f3 = if self.sealed || !has_stor {
            if has_stor {
                0.0
            } else {
                // Rain garden: the unconditional equal-flux rule binds
                // percolation to exfiltration.
                f2 = f2.min(self.stor_ksat.max(self.soil_ksat));
                f2
            }
        } else {
            stor_ksat_eff.min(f2 + self.d3 * stor_void / dt)
        };

        // Underdrain, clipped by standing volume, with hysteresis. The
        // head is the storage depth; only once storage is full does it
        // stack upward through the saturated-excess soil fraction and,
        // with the soil fully saturated, the ponded surface (§3.4). The
        // pavement layer holds no water in this template, so a pavement
        // above saturated soil caps the stack instead of extending it.
        let mut q3 = 0.0;
        if has_stor {
            if let Some(d) = &self.drain {
                let mut head = self.d3;
                if self.d3 >= stor_thick && has_soil && self.theta2 > self.soil_fc {
                    head += (self.theta2 - self.soil_fc) / (self.soil_por - self.soil_fc)
                        * self.soil_thick;
                    if self.theta2 >= self.soil_por && self.pave_thick <= 0.0 {
                        head += self.d1;
                    }
                }
                if self.drain_open {
                    if d.h_close > 0.0 && head <= d.h_close {
                        self.drain_open = false;
                    }
                } else if d.h_open <= 0.0 || head > d.h_open {
                    self.drain_open = true;
                }
                if self.drain_open && head > d.offset {
                    let h_rel = head - d.offset;
                    q3 = d.coeff * h_rel.powf(d.exponent);
                    // The multiplier curve reads the offset-relative head
                    // in the file's rain-depth unit (§14.6).
                    if !self.drain_curve.is_empty() {
                        q3 *= curve_multiplier(&self.drain_curve, h_rel / self.head_unit);
                    }
                    q3 = q3.min((self.d3 * stor_void / dt + f2 - f3).max(0.0));
                }
            } else if mat_pass {
                q3 = f2;
            }
            // Percolation re-capped by storage freeboard plus outflow.
            f2 = f2.min((stor_thick - self.d3).max(0.0) * stor_void / dt + f3 + q3);
        }

        // Intake re-capped last by soil voids plus soil outflow.
        if has_soil {
            f1 = f1.min((self.soil_por - self.theta2).max(0.0) * self.soil_thick / dt + f2 + e2);
        } else if has_stor {
            f1 = f1.min((stor_thick - self.d3).max(0.0) * stor_void / dt + f3 + q3);
        }

        // ── Balance advance, clipped to what is present ─────────────────
        let net1 = inflow - e1 - f1;
        self.d1 = (self.d1 + net1 * dt / self.surf_void).max(0.0);
        let mut over = 0.0;
        if self.d1 > self.surf_berm {
            let excess = self.d1 - self.surf_berm;
            over = if self.surf_alpha > 0.0 {
                (self.surf_alpha * excess.powf(5.0 / 3.0)).min(excess * self.surf_void / dt)
            } else {
                excess * self.surf_void / dt
            };
            self.d1 -= over * dt / self.surf_void;
        }
        if has_soil {
            self.theta2 =
                (self.theta2 + (f1 - e2 - f2) * dt / self.soil_thick).clamp(0.0, self.soil_por);
        }
        if has_stor {
            let inflow3 = if has_soil { f2 } else { f1 };
            self.d3 = (self.d3 + (inflow3 - e3 - f3 - q3) * dt / stor_void).clamp(0.0, stor_thick);
        } else if !has_soil {
            f3 = f1;
        }
        // Dry spell resets the intake event state.
        if inflow <= 0.0 && self.d1 <= 0.0 {
            self.f_intake = 0.0;
        }
        self.overflow = over;
        self.drain_flow = q3;
        self.exfiltration = f3;
    }
}

/// Linear interpolation on the multiplier curve, ends held (§14.6).
fn curve_multiplier(points: &[(f64, f64)], x: f64) -> f64 {
    let Some(&(x0, y0)) = points.first() else {
        return 0.0;
    };
    if x <= x0 {
        return y0;
    }
    let (mut x1, mut y1) = (x0, y0);
    for &(x2, y2) in &points[1..] {
        if x <= x2 {
            return y1 + (y2 - y1) * (x - x1) / (x2 - x1);
        }
        (x1, y1) = (x2, y2);
    }
    y1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LidDrain, LidPavement, LidSoil, LidStorage, LidSurface, ParcelOutlet};

    fn forcing(inflow: f64) -> LidForcing {
        LidForcing {
            inflow,
            rain: inflow,
            evap: 0.0,
            native_infil: 0.0,
            fac: InfilFactors::default(),
            elapsed_days: 0.0,
        }
    }

    fn swale_control() -> LidControl {
        LidControl {
            id: "SW".into(),
            kind: Some(LidKind::VegetativeSwale),
            surface: Some(LidSurface {
                thickness: 0.5,
                void_frac: 1.0,
                roughness: 0.1,
                slope: 0.01,
                side_slope: 3.0,
            }),
            soil: None,
            pavement: None,
            storage: None,
            drain: None,
            drain_mat: None,
            removals: Vec::new(),
        }
    }

    fn swale_usage() -> LidUsage {
        LidUsage {
            parcel: 0,
            control: 0,
            count: 1,
            area: 1000.0,
            width: 10.0,
            init_saturation: 0.0,
            from_impervious: 1.0,
            from_pervious: 0.0,
            to_pervious: false,
            report_file: None,
            drain_to: None::<ParcelOutlet>,
        }
    }

    #[test]
    fn a_swale_settles_at_the_manning_steady_state() {
        let mut u = LidUnit::build(
            &swale_control(),
            &swale_usage(),
            None,
            InfiltrationModel::Horton,
            &[],
            false,
        )
        .expect("build");
        // Constant inflow, no evaporation or infiltration: depth must
        // settle where trapezoidal Manning outflow balances inflow.
        let q_in = 2.0e-5;
        for _ in 0..600 {
            u.step(&forcing(q_in), 60.0);
        }
        assert!(
            (u.overflow - q_in).abs() < 0.02 * q_in,
            "outflow {} vs inflow {q_in}",
            u.overflow
        );
        // The converged depth reproduces the section's Manning rating.
        let d = u.swale_depth();
        let g = u.swale.expect("geometry");
        let a = d * (g.bot + g.slope * d);
        let p = g.bot + 2.0 * d * (1.0 + g.slope * g.slope).sqrt();
        let q = 0.01_f64.sqrt() / 0.1 * a * (a / p).powf(2.0 / 3.0);
        let q_target = q_in * g.len * g.top;
        assert!(
            (q - q_target).abs() < 0.03 * q_target,
            "rating {q} vs {q_target}"
        );
    }

    #[test]
    fn a_full_swale_spills_its_net_inflow_onward() {
        let mut u = LidUnit::build(
            &swale_control(),
            &swale_usage(),
            None,
            InfiltrationModel::Horton,
            &[],
            false,
        )
        .expect("build");
        // An extreme inflow overtops the berm; once full, outflow must
        // carry the entire inflow (nothing else can leave).
        let q_in = 5.0e-3;
        for _ in 0..200 {
            u.step(&forcing(q_in), 60.0);
        }
        assert!((u.swale_depth() - 0.5).abs() < 1e-6, "not full");
        assert!(
            (u.overflow - q_in).abs() < 1e-9,
            "spill {} vs inflow {q_in}",
            u.overflow
        );
    }

    #[test]
    fn the_multiplier_curve_interpolates_and_holds_its_ends() {
        let pts = [(0.0, 0.0), (10.0, 1.0), (20.0, 0.5)];
        assert!((curve_multiplier(&pts, -1.0) - 0.0).abs() < 1e-12);
        assert!((curve_multiplier(&pts, 5.0) - 0.5).abs() < 1e-12);
        assert!((curve_multiplier(&pts, 15.0) - 0.75).abs() < 1e-12);
        assert!((curve_multiplier(&pts, 99.0) - 0.5).abs() < 1e-12);
    }

    fn pavement_control(clog_factor: f64, regen_days: f64) -> LidControl {
        LidControl {
            id: "PP".into(),
            kind: Some(LidKind::PermeablePavement),
            surface: Some(LidSurface {
                thickness: 0.05,
                void_frac: 1.0,
                roughness: 0.0,
                slope: 0.0,
                side_slope: 0.0,
            }),
            soil: None,
            pavement: Some(LidPavement {
                thickness: 0.1,
                void_frac: 0.2,
                imperv_frac: 0.0,
                k_sat: 1.0e-4,
                clog_factor,
                regen_days,
                regen_degree: 1.0,
            }),
            storage: Some(LidStorage {
                thickness: 0.3,
                void_frac: 0.5,
                k_sat: 1.0e-4,
                clog_factor: 0.0,
                covered: false,
            }),
            drain: None,
            drain_mat: None,
            removals: Vec::new(),
        }
    }

    #[test]
    fn pavement_clogs_on_treated_volume_then_regenerates() {
        let usage = LidUsage {
            width: 0.0,
            ..swale_usage()
        };
        let mut u = LidUnit::build(
            &pavement_control(2.0, 1.0),
            &usage,
            None,
            InfiltrationModel::Horton,
            &[],
            false,
        )
        .expect("build");
        // Treatable depth = 2.0 x 0.1 thickness x 0.2 voids = 0.04 m of
        // cumulative inflow. Feed 6 mm per step: intake dies within a
        // dozen steps as the account fills.
        let mut fc = forcing(1.0e-5);
        for _ in 0..25 {
            u.step(&fc, 600.0);
        }
        assert!(
            u.overflow > 0.9 * 1.0e-5,
            "clogged pavement still accepts water: overflow {}",
            u.overflow
        );
        // A regeneration boundary (degree 1) clears the account: the
        // surface drains into the pavement again.
        fc.elapsed_days = 1.5;
        u.step(&fc, 600.0);
        assert!(
            u.overflow < 0.5 * 1.0e-5,
            "regeneration restored nothing: overflow {}",
            u.overflow
        );
    }

    #[test]
    fn the_drain_head_stacks_only_once_storage_is_full() {
        let ctl = LidControl {
            id: "BC".into(),
            kind: Some(LidKind::BioRetention),
            surface: Some(LidSurface {
                thickness: 0.5,
                void_frac: 1.0,
                roughness: 0.0,
                slope: 0.0,
                side_slope: 0.0,
            }),
            soil: Some(LidSoil {
                thickness: 0.4,
                porosity: 0.5,
                field_capacity: 0.2,
                wilting_point: 0.1,
                k_sat: 1.0e-5,
                k_slope: 10.0,
                suction: 0.05,
            }),
            pavement: None,
            storage: Some(LidStorage {
                thickness: 0.3,
                void_frac: 0.5,
                k_sat: 0.0,
                clog_factor: 0.0,
                covered: false,
            }),
            drain: Some(LidDrain {
                coeff: 1.0e-6,
                exponent: 1.0,
                offset: 0.0,
                delay: 0.0,
                h_open: 0.0,
                h_close: 0.0,
                curve: None,
            }),
            drain_mat: None,
            removals: Vec::new(),
        };
        let usage = LidUsage {
            width: 0.0,
            ..swale_usage()
        };
        let build = |sat: f64| {
            let mut u = LidUnit::build(&ctl, &usage, None, InfiltrationModel::Horton, &[], false)
                .expect("build");
            u.d3 = 0.3;
            u.theta2 = 0.2 + sat * 0.3;
            u.d1 = 0.2;
            u.step(&forcing(0.0), 1.0);
            u.drain_flow
        };
        // Full storage, soil at field capacity: head is the storage
        // depth alone.
        let q_base = build(0.0);
        assert!((q_base - 1.0e-6 * 0.3).abs() < 1e-10, "base {q_base}");
        // Fully saturated soil: the head stacks the soil thickness and
        // the ponded surface on top.
        let q_stack = build(1.0);
        assert!(
            (q_stack - 1.0e-6 * (0.3 + 0.4 + 0.2)).abs() < 1e-10,
            "stacked {q_stack}"
        );
    }
}
