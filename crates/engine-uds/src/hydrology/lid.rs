//! Control measures (§3.4): layered moisture-accounting units — surface,
//! optional pavement, soil, and storage with an optional underdrain — the
//! eight unit types as configurations of one template, advanced by the
//! normative limiter cascade: every flux clipped to the volume actually
//! present, mass-conserving by construction. This is the spec's recorded
//! exception to §3.5 integration: the clipped fluxes are discontinuous in
//! state, and the balance form is exact for the rates given.

use super::infiltration::{InfilFactors, InfilState};
use crate::model::options::InfiltrationModel;
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
    stor_thick: f64,
    stor_void: f64,
    stor_ksat: f64,
    sealed: bool,
    /// The predecessor's rain-barrel-only cover flag: excludes direct
    /// rainfall from the barrel's intake and nothing else (§3.4).
    covered: bool,
    /// Pavement pervious-paver fraction; scales sub-surface ET (§3.4).
    pave_void: f64,
    pave_perv_frac: f64,
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
    // State.
    d1: f64,
    /// Water depth in the pavement course's voids (m; §3.4).
    d_pave: f64,
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
    /// §11.2 per-unit water-balance totals.
    pub balance: LidBalance,
    /// This step's surface intake and soil percolation (m/s), for the
    /// §14.8.4 record; zero for kinds without those fluxes.
    surf_infil_step: f64,
    pave_perc_step: f64,
    soil_perc_step: f64,
    /// This step's unit inflow (m/s), captured for the same record.
    last_inflow: f64,
    /// The next regeneration boundary (elapsed days).
    next_regen: f64,
    /// §3.4 surface-to-soil intake: a modified Green–Ampt state on the
    /// soil layer's parameters; `None` without a soil layer.
    soil_ga: Option<InfilState>,
    /// Rates from the last step (m/s over the unit area).
    pub overflow: f64,
    pub drain_flow: f64,
    pub exfiltration: f64,
    /// Evapotranspiration exerted last step (m/s over the unit area),
    /// for the §11.1 surface ledger.
    pub evap_used: f64,
    /// Per-constituent drain-load removal fractions (§8.1).
    removals: Vec<(usize, f64)>,
    /// Green-roof mat Manning factor √S/n over the mat roughness; zero
    /// means a roughness-free mat passing percolation through (§3.4).
    mat_alpha: f64,
    /// Unit top width over area (1/m): the α scale for surface and
    /// mat Manning outflow (§3.2, §3.4).
    width_per_area: f64,
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

/// A control measure's underdrain, as a caller sets it mid-run (§12.4).
///
/// All six together, because they describe one drain: a caller changing
/// its opening head without its closing head has described a drain that
/// may never shut, and the six read as a set in the model too.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrainSetting {
    /// Discharge coefficient.
    pub coeff: f64,
    /// Discharge exponent.
    pub exponent: f64,
    /// Offset above the storage floor (m).
    pub offset: f64,
    /// Delay after rainfall ends before it may open (s).
    pub delay: f64,
    /// Head at which it opens (m); 0 = no head control.
    pub h_open: f64,
    /// Head at which it closes (m).
    pub h_close: f64,
}

impl LidUnit {
    /// This unit's underdrain as a caller sees it, or `None` for a unit
    /// that has no drain to set.
    pub fn drain_setting(&self) -> Option<DrainSetting> {
        self.drain.as_ref().map(|d| DrainSetting {
            coeff: d.coeff,
            exponent: d.exponent,
            offset: d.offset,
            delay: d.delay,
            h_open: d.h_open,
            h_close: d.h_close,
        })
    }

    /// Set this unit's underdrain (§12.4). `false` for a unit with no
    /// drain: a drain setting on a control measure that has none is not a
    /// thing to set, and accepting it would leave a caller believing a
    /// barrel could empty.
    pub fn set_drain_setting(&mut self, s: DrainSetting) -> bool {
        let Some(d) = &mut self.drain else {
            return false;
        };
        d.coeff = s.coeff;
        d.exponent = s.exponent;
        d.offset = s.offset;
        d.delay = s.delay;
        d.h_open = s.h_open;
        d.h_close = s.h_close;
        true
    }
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

/// §11.2 per-unit water-balance totals, each a depth over the unit's
/// own footprint (m). The §14.9 performance table is defined against
/// these plus the live stored depth.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LidBalance {
    /// Captured runoff plus direct rainfall.
    pub inflow: f64,
    /// Evapotranspiration.
    pub evap: f64,
    /// Native infiltration out of the unit.
    pub infil: f64,
    /// Surface outflow.
    pub surface: f64,
    /// Drain outflow.
    pub drain: f64,
    /// Stored water when the run began.
    pub initial: f64,
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
        let (mut d1, mut d_pave, mut theta2, mut d3) = (0.0, 0.0, 0.0, 0.0);
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
            if let Some(pv) = ctl.pavement.as_ref() {
                d_pave = usage.init_saturation * pv.thickness;
            }
        } else if let Some(so) = soil {
            theta2 = so.wilting_point;
        }
        let _ = &mut d1;
        let surf_alpha = match s {
            Some(sf) if sf.roughness > 0.0 && sf.slope > 0.0 => sf.slope.sqrt() / sf.roughness,
            _ => 0.0,
        };
        let mut unit = LidUnit {
            kind,
            area,
            from_imperv: usage.from_impervious,
            from_perv: usage.from_pervious,
            to_pervious: usage.to_pervious,
            drain_to: usage.drain_to,
            surf_berm: s.map_or(0.0, |x| x.thickness),
            // §3.4: a roof ponds on its full plan area — the surface
            // line's vegetation fraction is read but not applied.
            surf_void: if matches!(kind, LidKind::RooftopDisconnection) {
                1.0
            } else {
                s.map_or(1.0, |x| x.void_frac.max(1e-6))
            },
            surf_alpha,
            pave_thick: ctl.pavement.as_ref().map_or(0.0, |x| x.thickness),
            pave_ksat: ctl.pavement.as_ref().map_or(0.0, |x| x.k_sat),
            soil_thick: soil.map_or(0.0, |x| x.thickness),
            soil_por: soil.map_or(0.0, |x| x.porosity),
            soil_fc: soil.map_or(0.0, |x| x.field_capacity),
            soil_wp: soil.map_or(0.0, |x| x.wilting_point),
            soil_ksat: soil.map_or(0.0, |x| x.k_sat),
            soil_kslope: soil.map_or(0.0, |x| x.k_slope),
            stor_thick: stor.map_or(0.0, |x| x.thickness),
            // §3.4: a barrel is an empty vessel — its layer's void ratio
            // is read but not applied, so stored volume is stored depth.
            stor_void: if matches!(kind, LidKind::RainBarrel) {
                1.0
            } else {
                stor.map_or(0.0, |x| x.void_frac.max(1e-6))
            },
            stor_ksat: stor.map_or(0.0, |x| x.k_sat),
            // Green roofs and rain barrels are sealed (§3.4); cover is
            // the barrel's rain exclusion, never a seal.
            sealed: matches!(kind, LidKind::GreenRoof | LidKind::RainBarrel),
            covered: stor.is_some_and(|x| x.covered),
            // §3.4: the course stores water in its voids over the
            // pervious paver share of its plan area.
            pave_void: ctl.pavement.as_ref().map_or(0.0, |x| {
                (x.void_frac * (1.0 - x.imperv_frac).max(0.0)).max(1e-6)
            }),
            pave_perv_frac: ctl
                .pavement
                .as_ref()
                .map_or(1.0, |x| (1.0 - x.imperv_frac).max(0.0)),
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
            d1,
            d_pave,
            theta2,
            d3,
            drain_open: false,
            // §3.4: the delay clock starts full, so a run beginning dry
            // opens the drain only after the configured dry time.
            drain_delay_left: ctl.drain.as_ref().map_or(0.0, |d| d.delay),
            vol_treated: 0.0,
            total_inflow: 0.0,
            balance: LidBalance::default(),
            surf_infil_step: 0.0,
            pave_perc_step: 0.0,
            soil_perc_step: 0.0,
            last_inflow: 0.0,
            next_regen: ctl.pavement.as_ref().map_or(0.0, |x| x.regen_days),
            // §3.4: the intake state is modified Green–Ampt on the soil
            // layer's parameters, its deficit shrunk by initial
            // saturation. Pavement units intake through the pavement.
            soil_ga: match (soil, ctl.pavement.as_ref()) {
                (Some(so), None) => Some(InfilState::build(
                    &Infiltration::GreenAmpt {
                        suction: so.suction,
                        conductivity: so.k_sat,
                        initial_deficit: (1.0 - usage.init_saturation.clamp(0.0, 1.0))
                            * (so.porosity - so.wilting_point).max(0.0),
                    },
                    InfiltrationModel::ModifiedGreenAmpt,
                )),
                _ => None,
            },
            overflow: 0.0,
            drain_flow: 0.0,
            exfiltration: 0.0,
            evap_used: 0.0,
            removals: ctl.removals.clone(),
            mat_alpha: match (ctl.drain_mat.as_ref(), s) {
                (Some(m), Some(sf)) if m.roughness > 0.0 && sf.slope > 0.0 => {
                    sf.slope.sqrt() / m.roughness
                }
                _ => 0.0,
            },
            width_per_area: if usage.area > 0.0 {
                usage.width / usage.area
            } else {
                0.0
            },
        };
        // §11.2: the balance opens on the water the unit began holding.
        unit.balance.initial = unit.stored_depth();
        Ok(unit)
    }

    /// The drain-load removal fraction for constituent `ci` (§8.1).
    pub fn drain_removal(&self, ci: usize) -> f64 {
        self.removals
            .iter()
            .find(|(c, _)| *c == ci)
            .map_or(0.0, |(_, r)| *r)
    }

    /// Water currently held per unit area (m).
    pub fn stored_depth(&self) -> f64 {
        // A swale's ponded volume follows its trapezoidal section, not
        // the flat-layer product (§11.1 exactness).
        let surface = match self.swale {
            Some(g) if g.top > 0.0 => {
                self.d1 * (g.bot + g.slope * self.d1) * self.surf_void / g.top
            }
            _ => self.d1 * self.surf_void,
        };
        surface
            + self.d_pave * self.pave_void
            + self.theta2 * self.soil_thick
            + self.d3 * self.stor_void
    }

    /// Advance one hydrology step under the shared parcel forcing.
    /// Outflow rates land in `overflow`, `drain_flow`, `exfiltration`.
    pub fn step(&mut self, f: &LidForcing, dt: f64) {
        self.evap_used = 0.0;
        self.surf_infil_step = 0.0;
        self.pave_perc_step = 0.0;
        self.soil_perc_step = 0.0;
        self.last_inflow = f.inflow;
        match self.kind {
            LidKind::RooftopDisconnection => self.step_rooftop(f.inflow, f.evap, dt),
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
        // §11.2: the unit's own running balance.
        self.balance.inflow += f.inflow * dt;
        self.balance.evap += self.evap_used * dt;
        self.balance.infil += self.exfiltration * dt;
        self.balance.surface += self.overflow * dt;
        self.balance.drain += self.drain_flow * dt;
    }

    /// Rooftop disconnection: a lone surface whose gutter-capacity drain
    /// pre-empts overflow (§3.4).
    fn step_rooftop(&mut self, inflow: f64, evap: f64, dt: f64) {
        // §3.4: the lone-surface φ₁ balance — ponding, evaporation, and
        // Manning outflow — with the gutter-capacity drain pre-empting
        // overflow.
        let e1 = evap.min(self.d1 * self.surf_void / dt + inflow).max(0.0);
        self.d1 = (self.d1 + (inflow - e1) * dt / self.surf_void).max(0.0);
        let mut over = 0.0;
        if self.d1 > self.surf_berm {
            let excess = self.d1 - self.surf_berm;
            over = if self.surf_alpha > 0.0 && self.width_per_area > 0.0 {
                (self.surf_alpha * self.width_per_area * excess.powf(5.0 / 3.0))
                    .min(excess * self.surf_void / dt)
            } else {
                excess * self.surf_void / dt
            };
            self.d1 -= over * dt / self.surf_void;
        }
        // §3.4: the drain coefficient is the gutter's capacity, a plain
        // rate with the power law's depth factor undone; zero or absent
        // is a gutter with no capacity, everything shed going onward as
        // surface outflow.
        let cap = self
            .drain
            .as_ref()
            .map_or(0.0, |d| d.coeff * self.head_unit.powf(d.exponent));
        let drained = over.min(cap);
        self.drain_flow = drained;
        self.overflow = over - drained;
        self.exfiltration = 0.0;
        self.evap_used = e1;
    }

    /// Rain barrel: pure sealed storage; intake limited by freeboard plus
    /// concurrent drain outflow; the drain opens after its dry delay.
    fn step_rain_barrel(&mut self, inflow: f64, rain: f64, dt: f64) {
        // §3.4: a covered barrel excludes direct rainfall from its
        // intake — the captured tributary share still enters.
        let inflow = if self.covered {
            (inflow - rain).max(0.0)
        } else {
            inflow
        };
        let h = self.stor_thick;
        // Drain state: opens once dry weather has run the delay down.
        // Dryness is judged by the parcel's rainfall alone against the
        // 0.001 in/hr minimum-runoff rate (§3.4) — captured tributary
        // runoff's Manning tail must not hold the drain shut.
        let wet = rain > 7.055_6e-9;
        let mut q3 = 0.0;
        if let Some(d) = &self.drain {
            if d.delay <= 0.0 {
                // §3.4: a zero delay never latches the drain — it
                // discharges during rain.
                self.drain_open = true;
            } else if wet {
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
    /// varies with depth, advanced by the iterated trapezoidal method on
    /// its stored volume — equally weighted start- and end-of-step rates
    /// under this step's forcing, a 1 mm depth tolerance, at most twenty
    /// passes, the final pass accepted as-is. The booked fluxes are the
    /// same averages the advance uses, so the balance closes identically
    /// at any step (§3.4).
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
        let void = self.surf_void;
        let alpha = self.surf_alpha;
        // Stored volume (m³) at a depth, and the depth holding a volume:
        // V = L·void·d·(b + s·d), inverted by the quadratic.
        let vol_of = |d: f64| g.len * void * d * (g.bot + g.slope * d);
        let depth_of = |v: f64| -> f64 {
            let c = (v / (g.len * void)).max(0.0);
            if g.slope <= 0.0 {
                c / g.bot
            } else {
                let disc = g.bot * g.bot + 4.0 * g.slope * c;
                (disc.sqrt() - g.bot) / (2.0 * g.slope)
            }
        };
        // Draw components (m³/s) at a trial depth: evaporation capped by
        // the water present, exfiltration, Manning outflow.
        let rates = |d: f64| -> (f64, f64, f64) {
            let depth = d.min(berm);
            let surf_width = g.bot + 2.0 * g.slope * depth;
            let surf_area = g.len * surf_width;
            let flow_area = depth * (g.bot + g.slope * depth) * void;
            let volume = g.len * flow_area;
            let q_evap = (evap * surf_area).min(volume / dt);
            let q_exfil = f_infil * surf_area;
            let mut q_out = 0.0;
            if depth > 0.0 && flow_area > 0.0 {
                let wetted = g.bot + 2.0 * depth * (1.0 + g.slope * g.slope).sqrt();
                let r = flow_area / wetted;
                q_out = alpha * flow_area * r.powf(2.0 / 3.0);
            }
            (q_evap, q_exfil, q_out)
        };
        let q_in = inflow * unit_area;
        let d_old = self.d1;
        let v_old = vol_of(d_old);
        let v_max = vol_of(berm);
        let r0 = rates(d_old);
        let mut d = d_old;
        let mut r1 = r0;
        for _ in 0..20 {
            r1 = rates(d);
            let net = q_in - 0.5 * ((r0.0 + r1.0) + (r0.1 + r1.1) + (r0.2 + r1.2));
            let d_new = depth_of((v_old + net * dt).clamp(0.0, v_max));
            let done = (d_new - d).abs() <= 1e-3;
            d = d_new;
            if done {
                break;
            }
        }
        // Booked fluxes: the averages the advance used, adjusted only
        // where the clamp bit — draws scaled to the water present at
        // empty, the surplus spilling onward at the berm — so that
        // q_in·dt − booked·dt is the volume change, identically.
        let (mut q_evap, mut q_exfil, mut q_out) = (
            0.5 * (r0.0 + r1.0),
            0.5 * (r0.1 + r1.1),
            0.5 * (r0.2 + r1.2),
        );
        let v_raw = v_old + (q_in - q_evap - q_exfil - q_out) * dt;
        let v_new = v_raw.clamp(0.0, v_max);
        if v_raw < 0.0 {
            let draw = q_evap + q_exfil + q_out;
            if draw > 0.0 {
                let scale = ((v_old / dt + q_in) / draw).max(0.0);
                q_evap *= scale;
                q_exfil *= scale;
                q_out *= scale;
            }
        } else if v_raw > v_max {
            q_out += (v_raw - v_max) / dt;
        }
        self.d1 = depth_of(v_new);
        self.overflow = q_out / unit_area;
        self.exfiltration = q_exfil / unit_area;
        self.drain_flow = 0.0;
        self.evap_used = q_evap / unit_area;
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
        let paved = self.pave_thick > 0.0;
        let avail = inflow + self.d1 * self.surf_void / dt;
        // Clog-reduced permeability: conductivity falls linearly to zero
        // as the treated-volume account approaches the clogging capacity;
        // a regeneration boundary discounts the account first. It governs
        // the pervious paver share of the plan area, on both of the
        // course's faces (§3.4).
        let k_pave = if paved {
            let mut k = self.pave_ksat;
            if self.pave_clog > 0.0 {
                if self.regen_days > 0.0 && forcing.elapsed_days >= self.next_regen {
                    self.vol_treated *= 1.0 - self.regen_degree;
                    self.next_regen += self.regen_days;
                }
                k *= 1.0 - (self.vol_treated / self.pave_clog).min(1.0);
            }
            k * self.pave_perv_frac
        } else {
            0.0
        };
        let mut f1 = if paved {
            k_pave
        } else if has_soil {
            // §3.4: surface-to-soil intake is modified Green–Ampt on the
            // soil layer's parameters; saturated soil passes K₂S.
            if (self.soil_por - self.theta2).max(0.0) <= 0.0 {
                self.soil_ksat
            } else {
                match &mut self.soil_ga {
                    Some(ga) => ga.step(dt, inflow.max(0.0), self.d1, forcing.fac),
                    None => self.soil_ksat,
                }
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
        // §3.4: the predecessor's suppression rules — sub-surface ET
        // scales by the pervious paver fraction under pavement, and a
        // green roof's mat still evaporates.
        let perv = if paved { self.pave_perv_frac } else { 1.0 };
        // §3.4: the pavement course evaporates between the surface and
        // the soil in the top-down cascade.
        let ep = if paved {
            ((evap - e1) * perv)
                .min(self.d_pave * self.pave_void / dt)
                .max(0.0)
        } else {
            0.0
        };
        let mut e2 = 0.0;
        if has_soil {
            e2 = ((evap - e1) * perv - ep)
                .min((self.theta2 - self.soil_wp).max(0.0) * self.soil_thick / dt)
                .max(0.0);
        }
        let e3 = ((evap - e1) * perv - ep - e2)
            .min(self.d3 * stor_void / dt)
            .max(0.0);

        // §3.4: the course percolates at its own permeability, clipped
        // by the water it holds plus this step's intake.
        let mut fp = if paved {
            k_pave
                .min(self.d_pave * self.pave_void / dt + f1 - ep)
                .max(0.0)
        } else {
            f1
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
            fp
        };

        // A green-roof mat with no roughness passes percolation through;
        // a rough mat drains by Manning flow on the surface slope (§3.4).
        let mat_pass = self.mat_thick > 0.0 && self.mat_alpha == 0.0;

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
                // percolation to exfiltration at the native soil's
                // conductivity — the storage line's kSat, whose zero
                // seals the bottom (§3.4).
                f2 = f2.min(if self.sealed { 0.0 } else { self.stor_ksat });
                f2
            }
        } else {
            stor_ksat_eff.min(f2 + self.d3 * stor_void / dt)
        };

        // Underdrain, clipped by standing volume, with hysteresis. The
        // head is the storage depth; only once storage is full does it
        // stack upward through the saturated-excess soil fraction, a
        // full pavement course, and, with everything above saturated,
        // the ponded surface (§3.4).
        let mut q3 = 0.0;
        if has_stor {
            if let Some(d) = &self.drain {
                let mut head = self.d3;
                if self.d3 >= stor_thick && has_soil && self.theta2 > self.soil_fc {
                    head += (self.theta2 - self.soil_fc) / (self.soil_por - self.soil_fc)
                        * self.soil_thick;
                    if self.theta2 >= self.soil_por {
                        // §3.4: the stack passes through a full pavement
                        // course to reach the ponded surface.
                        if paved {
                            head += self.d_pave;
                            if self.d_pave >= self.pave_thick {
                                head += self.d1;
                            }
                        } else {
                            head += self.d1;
                        }
                    }
                } else if self.d3 >= stor_thick && !has_soil && paved {
                    head += self.d_pave;
                    if self.d_pave >= self.pave_thick {
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
            } else if self.mat_thick > 0.0 {
                // Manning flow through the drainage mat (§3.4), clipped
                // by the standing water plus this step's percolation.
                q3 = self.mat_alpha * self.d3.powf(5.0 / 3.0) * self.width_per_area * stor_void;
                q3 = q3.min((self.d3 * stor_void / dt + f2 - f3).max(0.0));
            }
            // Percolation re-capped by storage freeboard plus outflow.
            f2 = f2.min((stor_thick - self.d3).max(0.0) * stor_void / dt + f3 + q3);
        }

        // Intake re-capped last by soil voids plus soil outflow; a paved
        // course takes those caps on its lower face and offers the
        // surface its own freeboard plus outflow, which is what lets it
        // buffer a storm the layer beneath cannot pass (§3.4).
        if paved {
            if has_soil {
                fp =
                    fp.min((self.soil_por - self.theta2).max(0.0) * self.soil_thick / dt + f2 + e2);
            } else if has_stor {
                fp = fp.min((stor_thick - self.d3).max(0.0) * stor_void / dt + f3 + q3);
            }
            f1 = f1.min((self.pave_thick - self.d_pave).max(0.0) * self.pave_void / dt + fp + ep);
        } else if has_soil {
            f1 = f1.min((self.soil_por - self.theta2).max(0.0) * self.soil_thick / dt + f2 + e2);
        } else if has_stor {
            f1 = f1.min((stor_thick - self.d3).max(0.0) * stor_void / dt + f3 + q3);
        }

        // ── Balance advance, clipped to what is present ─────────────────
        let net1 = inflow - e1 - f1;
        self.d1 = (self.d1 + net1 * dt / self.surf_void).max(0.0);
        let mut over = 0.0;
        if self.d1 > self.surf_berm {
            // §3.4: Manning at the §3.2 α — width over area included; a
            // widthless unit spills its excess directly.
            let excess = self.d1 - self.surf_berm;
            over = if self.surf_alpha > 0.0 && self.width_per_area > 0.0 {
                (self.surf_alpha * self.width_per_area * excess.powf(5.0 / 3.0))
                    .min(excess * self.surf_void / dt)
            } else {
                excess * self.surf_void / dt
            };
            self.d1 -= over * dt / self.surf_void;
        }
        if paved {
            self.d_pave =
                (self.d_pave + (f1 - ep - fp) * dt / self.pave_void).clamp(0.0, self.pave_thick);
        }
        if has_soil {
            self.theta2 =
                (self.theta2 + (fp - e2 - f2) * dt / self.soil_thick).clamp(0.0, self.soil_por);
        }
        if has_stor {
            let inflow3 = if has_soil { f2 } else { fp };
            self.d3 = (self.d3 + (inflow3 - e3 - f3 - q3) * dt / stor_void).clamp(0.0, stor_thick);
        } else if !has_soil {
            f3 = fp;
        }
        self.overflow = over;
        self.drain_flow = q3;
        self.exfiltration = f3;
        self.evap_used = e1 + ep + e2 + e3;
        // §14.8.4: the record's internal fluxes.
        self.surf_infil_step = f1;
        self.pave_perc_step = fp;
        self.soil_perc_step = if has_soil { f2 } else { 0.0 };
    }

    /// The §14.8.4 per-step record: this step's fluxes (m/s) and the
    /// current layer states, mapped per the template's column semantics.
    pub fn step_record(&self) -> LidStepRecord {
        let paved = self.pave_thick > 0.0;
        let swale = matches!(self.kind, LidKind::VegetativeSwale);
        LidStepRecord {
            inflow: self.last_inflow,
            evap: self.evap_used,
            // A swale's ground loss is its surface infiltration; its
            // storage exfiltration is zero.
            surf_infil: if swale {
                self.exfiltration
            } else {
                self.surf_infil_step
            },
            pave_perc: if paved { self.pave_perc_step } else { 0.0 },
            soil_perc: self.soil_perc_step,
            stor_exfil: if swale { 0.0 } else { self.exfiltration },
            surf_outflow: self.overflow,
            drain: self.drain_flow,
            surf_level: self.d1,
            pave_level: self.d_pave,
            soil_moisture: self.theta2,
            stor_level: self.d3,
        }
    }
}

/// One §14.8.4 report-file row: fluxes in m/s, levels in m, soil
/// moisture as a content fraction.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LidStepRecord {
    pub inflow: f64,
    pub evap: f64,
    pub surf_infil: f64,
    pub pave_perc: f64,
    pub soil_perc: f64,
    pub stor_exfil: f64,
    pub surf_outflow: f64,
    pub drain: f64,
    pub surf_level: f64,
    pub pave_level: f64,
    pub soil_moisture: f64,
    pub stor_level: f64,
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

// ── Checkpointing (§12.3) ────────────────────────────────────────────────────

impl LidUnit {
    /// Write this state (§12.3). Its geometry, its layers' properties and its drain's are the model's and are rebuilt; the water it holds, its clogging, and its drain's open state are not.
    ///
    /// Exhaustive by design: a field added here fails to compile until it
    /// is written or declared a parameter the model rebuilds.
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        #[allow(unused_imports)]
        use crate::simulation::checkpoint::{put_b, put_f, put_fs, put_u};
        let LidUnit {
            kind: _,
            area: _,
            from_imperv: _,
            from_perv: _,
            to_pervious: _,
            drain_to: _,
            surf_berm: _,
            surf_void: _,
            surf_alpha: _,
            pave_thick: _,
            pave_ksat: _,
            soil_thick: _,
            soil_por: _,
            soil_fc: _,
            soil_wp: _,
            soil_ksat: _,
            soil_kslope: _,
            stor_thick: _,
            stor_void: _,
            stor_ksat: _,
            sealed: _,
            covered: _,
            pave_void: _,
            pave_perv_frac: _,
            drain: _,
            mat_thick: _,
            mat_void: _,
            pave_clog,
            regen_days: _,
            regen_degree: _,
            stor_clog,
            drain_curve: _,
            head_unit: _,
            swale: _,
            swale_infil,
            d1,
            d_pave,
            theta2,
            d3,
            drain_open,
            drain_delay_left,
            balance,
            surf_infil_step: _,
            pave_perc_step: _,
            soil_perc_step: _,
            last_inflow: _,
            vol_treated,
            total_inflow,
            next_regen,
            soil_ga,
            overflow,
            drain_flow,
            exfiltration,
            evap_used,
            removals,
            mat_alpha: _,
            width_per_area: _,
        } = self;
        put_f(w, *pave_clog)?;
        put_f(w, *stor_clog)?;
        put_b(w, swale_infil.is_some())?;
        if let Some(s) = swale_infil {
            s.checkpoint_put(w)?;
        }
        put_f(w, *d1)?;
        put_f(w, *d_pave)?;
        put_f(w, *theta2)?;
        put_f(w, *d3)?;
        put_b(w, *drain_open)?;
        put_f(w, *drain_delay_left)?;
        put_f(w, balance.inflow)?;
        put_f(w, balance.evap)?;
        put_f(w, balance.infil)?;
        put_f(w, balance.surface)?;
        put_f(w, balance.drain)?;
        put_f(w, balance.initial)?;
        put_f(w, *vol_treated)?;
        put_f(w, *total_inflow)?;
        put_f(w, *next_regen)?;
        put_b(w, soil_ga.is_some())?;
        if let Some(s) = soil_ga {
            s.checkpoint_put(w)?;
        }
        put_f(w, *overflow)?;
        put_f(w, *drain_flow)?;
        put_f(w, *exfiltration)?;
        put_f(w, *evap_used)?;
        put_u(w, removals.len() as u64)?;
        for (i, v) in removals {
            put_u(w, *i as u64)?;
            put_f(w, *v)?;
        }
        Ok(())
    }

    /// Read back what `checkpoint_put` wrote.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        self.pave_clog = r.f()?;
        self.stor_clog = r.f()?;
        if r.b()? {
            match &mut self.swale_infil {
                Some(s) => s.checkpoint_get(r)?,
                None => return Err("checkpoint infiltrates where this model does not".into()),
            }
        } else if self.swale_infil.is_some() {
            return Err("this model infiltrates where the checkpoint does not".into());
        }
        self.d1 = r.f()?;
        self.d_pave = r.f()?;
        self.theta2 = r.f()?;
        self.d3 = r.f()?;
        self.drain_open = r.b()?;
        self.drain_delay_left = r.f()?;
        self.balance.inflow = r.f()?;
        self.balance.evap = r.f()?;
        self.balance.infil = r.f()?;
        self.balance.surface = r.f()?;
        self.balance.drain = r.f()?;
        self.balance.initial = r.f()?;
        self.vol_treated = r.f()?;
        self.total_inflow = r.f()?;
        self.next_regen = r.f()?;
        if r.b()? {
            match &mut self.soil_ga {
                Some(s) => s.checkpoint_get(r)?,
                None => return Err("checkpoint infiltrates where this model does not".into()),
            }
        } else if self.soil_ga.is_some() {
            return Err("this model infiltrates where the checkpoint does not".into());
        }
        self.overflow = r.f()?;
        self.drain_flow = r.f()?;
        self.exfiltration = r.f()?;
        self.evap_used = r.f()?;
        let n = r.u()? as usize;
        self.removals = Vec::with_capacity(n);
        for _ in 0..n {
            let i = r.u()? as usize;
            self.removals.push((i, r.f()?));
        }
        Ok(())
    }
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

    fn roof_control(veg: f64, drain: Option<LidDrain>) -> LidControl {
        LidControl {
            id: "RD".into(),
            kind: Some(LidKind::RooftopDisconnection),
            surface: Some(LidSurface {
                thickness: 0.15,
                void_frac: 1.0 - veg,
                roughness: 0.1,
                slope: 0.01,
                side_slope: 0.0,
            }),
            soil: None,
            pavement: None,
            storage: None,
            drain,
            drain_mat: None,
            removals: Vec::new(),
        }
    }

    #[test]
    fn a_roof_ponds_on_its_full_plan_area() {
        // §3.4: the surface line's vegetation fraction is read but not
        // applied to a roof. Two roofs differing only in that template
        // value must shed identically, and each ends holding its full
        // storage depth of water.
        let step_out = |veg: f64| {
            let mut u = LidUnit::build(
                &roof_control(veg, None),
                &swale_usage(),
                None,
                InfiltrationModel::Horton,
                &[],
                false,
            )
            .expect("build");
            let mut shed = 0.0;
            for step in 0..120 {
                let q = if step < 30 { 2.0e-4 } else { 0.0 };
                u.step(&forcing(q), 60.0);
                shed += (u.overflow + u.drain_flow) * 60.0;
            }
            (shed, u.d1)
        };
        let (shed_bare, held_bare) = step_out(0.0);
        let (shed_veg, held_veg) = step_out(0.25);
        assert!(shed_bare > 0.0, "the pulse never overtopped the storage");
        assert_eq!(
            shed_veg, shed_bare,
            "a template vegetation fraction changed what a roof shed"
        );
        assert_eq!(held_veg, held_bare);
        // What stays behind is the full storage depth of water, plus
        // the polynomial tail of the Manning recession still draining.
        assert!(
            held_bare >= 0.15 && held_bare - 0.15 < 5.0e-3,
            "roof holds {held_bare} m against its 0.15 m storage"
        );
    }

    #[test]
    fn a_roofs_gutter_capacity_is_a_plain_rate() {
        // §3.4: the drain coefficient is the gutter's capacity with the
        // power law's depth factor undone, and zero is a gutter with no
        // capacity: everything shed goes onward as surface outflow.
        let head_unit = 0.001_f64; // SI build below.
        let cap = 1.0e-6;
        let drain = LidDrain {
            coeff: cap / head_unit.powf(0.5),
            exponent: 0.5,
            offset: 0.0,
            delay: 0.0,
            h_open: 0.0,
            h_close: 0.0,
            curve: None,
        };
        let mut u = LidUnit::build(
            &roof_control(0.0, Some(drain)),
            &swale_usage(),
            None,
            InfiltrationModel::Horton,
            &[],
            false,
        )
        .expect("build");
        // Flood the roof so it sheds far above the gutter's capacity.
        for _ in 0..60 {
            u.step(&forcing(2.0e-4), 60.0);
        }
        assert!(
            (u.drain_flow - cap).abs() < 1e-3 * cap,
            "gutter carries {} against its capacity {cap}",
            u.drain_flow
        );
        assert!(
            u.overflow > 10.0 * cap,
            "the excess must spill as surface outflow, got {}",
            u.overflow
        );
        // And with no drain line at all, nothing is gutter flow.
        let mut bare = LidUnit::build(
            &roof_control(0.0, None),
            &swale_usage(),
            None,
            InfiltrationModel::Horton,
            &[],
            false,
        )
        .expect("build");
        for _ in 0..60 {
            bare.step(&forcing(2.0e-4), 60.0);
        }
        assert_eq!(bare.drain_flow, 0.0);
        assert!(bare.overflow > 0.0);
    }

    fn buffered_pavement_control() -> LidControl {
        LidControl {
            id: "PP".into(),
            kind: Some(LidKind::PermeablePavement),
            surface: Some(LidSurface {
                thickness: 0.05,
                void_frac: 1.0,
                roughness: 0.1,
                slope: 0.01,
                side_slope: 0.0,
            }),
            soil: None,
            // A permeable course over a storage bed whose exfiltration
            // is the bottleneck.
            pavement: Some(LidPavement {
                thickness: 0.15,
                void_frac: 0.25,
                imperv_frac: 0.0,
                k_sat: 1.0e-4,
                clog_factor: 0.0,
                regen_days: 0.0,
                regen_degree: 0.0,
            }),
            // A bed small enough to fill: 10 mm of capacity against a
            // 36 mm pulse, draining at a trickle.
            storage: Some(LidStorage {
                thickness: 0.02,
                void_frac: 0.5,
                k_sat: 2.0e-6,
                clog_factor: 0.0,
                covered: false,
            }),
            drain: None,
            drain_mat: None,
            removals: Vec::new(),
        }
    }

    #[test]
    fn a_pavement_course_buffers_what_its_bed_cannot_pass() {
        // §3.4: the course stores water in its voids, so when the
        // storage bed beneath is the bottleneck a storm backs up into
        // the pavement before the surface ponds. Without the buffer the
        // template shed 8% of the standard porous-pavement test's
        // inflow off the top while the predecessor shed none.
        let mut u = LidUnit::build(
            &buffered_pavement_control(),
            &swale_usage(),
            None,
            InfiltrationModel::Horton,
            &[],
            false,
        )
        .expect("build");
        // A pulse below the permeability but far above the bed's
        // exfiltration: it must land in the pavement, not run off.
        let q = 2.0e-5;
        let mut shed = 0.0;
        let mut inflow = 0.0;
        for _ in 0..30 {
            u.step(&forcing(q), 60.0);
            shed += u.overflow * 60.0;
            inflow += q * 60.0;
        }
        let rec = u.step_record();
        assert!(
            rec.pave_level > 0.05,
            "the course holds {} m against a bottlenecked bed",
            rec.pave_level
        );
        assert!(
            shed < 0.02 * inflow,
            "the surface shed {shed} m of {inflow} m while the course had voids"
        );
        // And the unit's own books close over the episode.
        let out = u.balance.evap + u.balance.infil + u.balance.surface + u.balance.drain;
        let held = u.stored_depth() - u.balance.initial;
        assert!(
            (u.balance.inflow - out - held).abs() < 1e-9 * u.balance.inflow,
            "the unit leaked: in {} out {out} held {held}",
            u.balance.inflow
        );
    }

    #[test]
    fn a_vegetated_berm_overtops_after_its_void_of_water() {
        // §3.4: the ponded surface stores water in its voids, so a
        // vegetated berm overtops after berm x void of water — the free
        // surface rises through the vegetation. The predecessor advances
        // depth by the raw flux beside a volume counted through the
        // void, which cannot both hold; this pins the volume-conserving
        // reading. A bottlenecked bed forces the ponding.
        let unit = |veg: f64| {
            let mut c = buffered_pavement_control();
            c.surface = Some(LidSurface {
                thickness: 0.10,
                void_frac: 1.0 - veg,
                roughness: 0.1,
                slope: 0.01,
                side_slope: 0.0,
            });
            // Seal the course and the bed down to a trickle so the
            // surface must pond.
            c.pavement.as_mut().unwrap().k_sat = 1.0e-7;
            let mut u = LidUnit::build(
                &c,
                &swale_usage(),
                None,
                InfiltrationModel::Horton,
                &[],
                false,
            )
            .expect("build");
            // Feed just enough water to fill a bare berm exactly:
            // 0.10 m over the inflow, minus the trickle that percolates.
            let dt = 60.0;
            let q = 1.0e-4_f64;
            let steps = (0.10 / (q * dt)).ceil() as usize;
            let mut shed = 0.0;
            for _ in 0..steps {
                u.step(&forcing(q), dt);
                shed += u.overflow * dt;
            }
            // Let the Manning tail finish: what stands above the berm
            // when the pulse ends still belongs to the shed.
            for _ in 0..200 {
                u.step(&forcing(0.0), dt);
                shed += u.overflow * dt;
            }
            shed
        };
        let bare = unit(0.0);
        let vegetated = unit(0.5);
        // The bare berm holds nearly all of it; the half-vegetated berm
        // holds half the water, so roughly the other half must shed.
        assert!(bare < 0.01, "a bare berm shed {bare} m of 0.10 m in");
        assert!(
            vegetated > 0.03,
            "a half-vegetated berm held water in space its vegetation occupies: shed {vegetated} m"
        );
    }

    fn barrel_control() -> LidControl {
        LidControl {
            id: "RB".into(),
            kind: Some(LidKind::RainBarrel),
            surface: None,
            soil: None,
            pavement: None,
            // A void ratio on the storage line, as the predecessor's GUI
            // writes by default. A barrel must ignore it.
            storage: Some(LidStorage {
                thickness: 1.2,
                void_frac: 0.75 / 1.75,
                k_sat: 0.0,
                clog_factor: 0.0,
                covered: false,
            }),
            drain: Some(LidDrain {
                // q = C h^{1/2} with C chosen so drawdown spans hours.
                coeff: 2.0e-4,
                exponent: 0.5,
                offset: 0.0,
                delay: 0.0,
                h_open: 0.0,
                h_close: 0.0,
                curve: None,
            }),
            drain_mat: None,
            removals: Vec::new(),
        }
    }

    #[test]
    fn a_full_barrel_drains_dry_on_the_closed_form_clock() {
        // A barrel holding h0 over q = C sqrt(h) empties at exactly
        // t = 2 sqrt(h0) / C: dh/dt = -C sqrt(h) has the closed form
        // h(t) = (sqrt(h0) - C t / 2)^2, because a barrel is an empty
        // vessel and stored volume is stored depth (§3.4). Scaling that
        // clock by the storage layer's void fraction is the defect this
        // pins: with the fixture's 0.75 void ratio the vessel would run
        // dry at 0.43 of the true time.
        let mut u = LidUnit::build(
            &barrel_control(),
            &LidUsage {
                init_saturation: 1.0,
                ..swale_usage()
            },
            None,
            InfiltrationModel::Horton,
            &[],
            false,
        )
        .expect("build");
        let (h0, c) = (1.2_f64, 2.0e-4);
        let t_dry = 2.0 * h0.sqrt() / c;
        let dt = 10.0;
        let mut t = 0.0;
        // Mid-drawdown, the outflow matches the closed form's rate.
        while t < 0.5 * t_dry {
            u.step(&forcing(0.0), dt);
            t += dt;
        }
        let q_want = c * (h0.sqrt() - c * t / 2.0);
        assert!(
            (u.drain_flow - q_want).abs() < 0.01 * q_want,
            "drain {} vs closed form {q_want} at t {t}",
            u.drain_flow
        );
        // And the vessel runs dry on the closed form's clock, not
        // earlier: still draining at 90% of it, empty just past it.
        while t < 0.9 * t_dry {
            u.step(&forcing(0.0), dt);
            t += dt;
        }
        assert!(u.drain_flow > 0.0, "dry too early at t {t}");
        while t < 1.02 * t_dry {
            u.step(&forcing(0.0), dt);
            t += dt;
        }
        assert!(
            u.drain_flow < 1e-3 * c * h0.sqrt(),
            "still draining {} past the closed-form clock",
            u.drain_flow
        );
    }

    #[test]
    fn a_swales_booked_fluxes_are_its_volume_change() {
        // §3.4: the booked fluxes are the same averages the volume
        // advance uses, so inflow minus bookings equals the volume
        // change identically at ANY step — even one so coarse the
        // Manning outflow swings across it. Booking one instant's rates
        // against an averaged advance leaked the half-difference every
        // step: 5.6% of a storm at a five-minute step.
        let mut u = LidUnit::build(
            &swale_control(),
            &swale_usage(),
            None,
            InfiltrationModel::Horton,
            &[],
            false,
        )
        .expect("build");
        let g = u.swale.expect("geometry");
        let unit_area = g.len * g.top;
        let vol = |u: &LidUnit| {
            let d = u.d1;
            g.len * d * (g.bot + g.slope * d)
        };
        let dt = 300.0;
        let mut booked = 0.0;
        let mut inflow_total = 0.0;
        let v0 = vol(&u);
        // A storm pulse, a recession, then a refill: the rates swing
        // hard between steps, which is what the leak fed on.
        for step in 0..60 {
            let q = match step {
                0..=11 => 4.0e-5,
                12..=35 => 0.0,
                _ => 1.5e-5,
            };
            u.step(&forcing(q), dt);
            inflow_total += q * unit_area * dt;
            booked += (u.overflow + u.exfiltration + u.evap_used) * unit_area * dt;
        }
        let dv = vol(&u) - v0;
        let residual = inflow_total - booked - dv;
        assert!(
            residual.abs() < 1e-9 * inflow_total.max(1.0),
            "the swale leaked {residual} m3 of {inflow_total} m3 in"
        );
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
