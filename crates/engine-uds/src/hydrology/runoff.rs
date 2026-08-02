//! Overland flow (§3.2): each parcel sub-area is a nonlinear reservoir
//! integrated under the §3.5 embedded-pair error-controlled integrator,
//! with the filling phase handled analytically. Parcel runoff is the
//! area-weighted sum; internal re-routing and one-step-delayed run-on
//! follow the predecessor's semantics exactly.

use super::infiltration::{InfilFactors, InfilState};
use super::lid::LidUnit;
use super::snow::{SnowClimate, SnowPack};
use crate::model::{
    GageSource, Network, ParcelOutlet, RainForm, SeriesTime, SubareaRouting, TimeSeriesSource,
};
use crate::simulation::time::days_from_civil as time_days_from_civil;

/// §3.5: local error tolerance on each integrated state (m).
const INTEGRATOR_TOL: f64 = 1.0e-5;
/// §3.5: the integrator's own step floor (s).
const INTEGRATOR_FLOOR: f64 = 1.0e-3;

/// Why the surface compartment cannot be built yet.
#[derive(Debug, Clone, PartialEq)]
pub enum SurfaceRefusal {
    /// A compartment this build stage does not evaluate.
    Unsupported(&'static str),
    /// A parcel missing its sub-area or infiltration data.
    Incomplete(String),
}

/// A gage's precipitation record, resolved to absolute seconds and SI
/// rates at build.
struct GageRain {
    /// (start-of-interval epoch s, rate m/s) in time order.
    intervals: Vec<(f64, f64)>,
    /// Recording interval (s).
    interval: f64,
    /// Snow catch factor.
    scf: f64,
}

impl GageRain {
    /// The rain rate at an absolute epoch time.
    fn rate(&self, epoch: f64) -> f64 {
        let mut r = 0.0;
        for &(t0, v) in &self.intervals {
            if epoch < t0 {
                break;
            }
            r = if epoch < t0 + self.interval { v } else { 0.0 };
        }
        r
    }

    /// The next interval boundary strictly after `epoch`, for hydrology
    /// step truncation (§10.1).
    fn next_boundary(&self, epoch: f64) -> Option<f64> {
        for &(t0, _) in &self.intervals {
            if t0 > epoch {
                return Some(t0);
            }
            if t0 + self.interval > epoch {
                return Some(t0 + self.interval);
            }
        }
        None
    }
}

/// One sub-area's reservoir.
#[derive(Debug, Clone, Copy)]
struct Subarea {
    /// Plan area (m²); zero disables the sub-area.
    area: f64,
    /// The Manning α = W√S/(A·n); zero means the n = 0 bypass (§3.2).
    alpha: f64,
    /// Depression storage (m).
    dstore: f64,
    /// Ponded depth (m).
    depth: f64,
}

impl Subarea {
    /// Advance the reservoir by `dt` under a constant net input rate
    /// (m/s) and return the runoff volume (m³). Mass-exact: runoff is
    /// input minus storage change.
    fn advance(&mut self, net: f64, dt: f64, degraded: &mut bool) -> f64 {
        if self.area <= 0.0 {
            return 0.0;
        }
        let d0 = self.depth;
        if self.alpha <= 0.0 {
            // n = 0 bypasses routing: excess above depression storage
            // converts to runoff each step (§3.2).
            let d = (self.depth + net * dt).max(0.0);
            let out = (d - self.dstore).max(0.0);
            self.depth = d - out;
            return out * self.area;
        }
        let mut t = 0.0;
        // Analytic filling (or draining) phase while below depression
        // storage: no outflow term.
        if self.depth <= self.dstore {
            if net <= 0.0 {
                self.depth = (self.depth + net * dt).max(0.0);
                return 0.0;
            }
            let t_fill = (self.dstore - self.depth) / net;
            if t_fill >= dt {
                self.depth += net * dt;
                return 0.0;
            }
            self.depth = self.dstore;
            t = t_fill;
        }
        // §3.5 embedded-pair integration of d' = net − α(d−ds)^{5/3}.
        let f = |d: f64| net - self.alpha * (d - self.dstore).max(0.0).powf(5.0 / 3.0);
        let mut h = dt - t;
        let mut d = self.depth;
        while t < dt - 1e-12 {
            h = h.min(dt - t);
            let (d_new, err) = cash_karp(d, h, &f);
            if err <= INTEGRATOR_TOL || h <= INTEGRATOR_FLOOR {
                if err > INTEGRATOR_TOL {
                    *degraded = true;
                }
                t += h;
                d = d_new.max(0.0);
                let grow = if err > 0.0 {
                    0.9 * (INTEGRATOR_TOL / err).powf(0.2)
                } else {
                    5.0
                };
                h *= grow.clamp(0.1, 5.0);
            } else {
                let shrink = 0.9 * (INTEGRATOR_TOL / err).powf(0.25);
                h = (h * shrink.clamp(0.1, 5.0)).max(INTEGRATOR_FLOOR);
            }
        }
        self.depth = d;
        // Runoff = net input − storage change, over the whole step.
        ((net * dt - (d - d0)) * self.area).max(0.0)
    }
}

/// The classic Cash–Karp 4(5) embedded step on a scalar ODE: returns the
/// fifth-order estimate and the local error magnitude.
fn cash_karp(y: f64, h: f64, f: &dyn Fn(f64) -> f64) -> (f64, f64) {
    let k1 = f(y);
    let k2 = f(y + h * (0.2 * k1));
    let k3 = f(y + h * (0.075 * k1 + 0.225 * k2));
    let k4 = f(y + h * (0.3 * k1 - 0.9 * k2 + 1.2 * k3));
    let k5 = f(y + h * (-11.0 / 54.0 * k1 + 2.5 * k2 - 70.0 / 27.0 * k3 + 35.0 / 27.0 * k4));
    let k6 = f(y + h
        * (1631.0 / 55_296.0 * k1
            + 175.0 / 512.0 * k2
            + 575.0 / 13_824.0 * k3
            + 44_275.0 / 110_592.0 * k4
            + 253.0 / 4096.0 * k5));
    let y5 =
        y + h * (37.0 / 378.0 * k1 + 250.0 / 621.0 * k3 + 125.0 / 594.0 * k4 + 512.0 / 1771.0 * k6);
    let y4 = y + h
        * (2825.0 / 27_648.0 * k1
            + 18_575.0 / 48_384.0 * k3
            + 13_525.0 / 55_296.0 * k4
            + 277.0 / 14_336.0 * k5
            + 0.25 * k6);
    (y5, (y5 - y4).abs())
}

/// The per-step volumes §8.2–§8.3 surface quality consumes.
#[derive(Debug, Clone, Copy, Default)]
pub struct QStep {
    /// Precipitation rate on the parcel (m/s), gage-adjusted.
    pub rain_rate: f64,
    /// Precipitation volume over the non-measure area (m³).
    pub rain_vol: f64,
    /// Precipitation volume over the control-measure footprint (m³).
    pub lid_rain_vol: f64,
    /// Ponded-store inflow: run-on + initial ponded + precipitation (m³).
    pub v_inflow: f64,
    /// Run-on volume (m³).
    pub runon_vol: f64,
    /// Infiltration volume (m³).
    pub v_infil: f64,
    /// Runoff volume leaving before control measures (m³).
    pub v_outflow: f64,
    /// Outflow volume after control measures, drains included (m³).
    pub v_out2: f64,
    /// End-of-step ponded volume on the non-measure area (m³).
    pub ponded_end: f64,
    /// Runoff rate over the parcel before re-routing (m/s).
    pub runoff_rate: f64,
    /// Whether snow covers the parcel (snow-only accumulation).
    pub snow_cover: bool,
    /// Mean snow water equivalent (m), for the §14.9 records.
    pub snow_depth: f64,
    /// The §10.1 hydrology step length (s).
    pub dt: f64,
}

/// One parcel's live surface state.
struct ParcelState {
    gage: usize,
    outlet: ParcelOutlet,
    /// [impervious with storage, impervious zero-storage, pervious].
    sub: [Subarea; 3],
    infil: Option<InfilState>,
    routing: SubareaRouting,
    frac_routed: f64,
    /// The §4.2 snow pack, when assigned.
    snow: Option<SnowPack>,
    /// §3.4 control measures deployed in the parcel.
    lids: Vec<LidUnit>,
    /// Drain flow routed to a vertex this step (m³/s), by vertex.
    lid_vertex_drain: Vec<(usize, f64)>,
    /// Run-on rate (m/s over the whole parcel), one step delayed.
    runon: f64,
    runon_next_vol: f64,
    /// This step's arriving run-on volume (m³), for the §3.4
    /// full-footprint gate.
    runon_vol: f64,
    /// Current outlet runoff (m³/s).
    pub runoff: f64,
    /// Infiltration rate this step (m/s over the whole parcel).
    pub infil_rate: f64,
    /// Pervious surface evaporation exerted this step (m/s over the
    /// whole parcel).
    pub evap_rate: f64,
    /// The §8 quality context recorded for this step.
    pub qstep: QStep,
}

/// The surface compartment: every gage and parcel, advanced on the
/// hydrology clock, supplying vertex laterals and parcel run-on.
pub struct Surface {
    gages: Vec<GageRain>,
    parcels: Vec<ParcelState>,
    /// The run's absolute start instant (s), anchoring elapsed clocks.
    start_epoch: f64,
    /// Whether any §3.5 integration ran degraded this step.
    pub degraded: bool,
    /// Cumulative infiltration + evaporation losses (m³).
    pub losses: f64,
    /// Cumulative rainfall volume (m³).
    pub rainfall: f64,
    // §11.1 surface-ledger accounts (m³).
    /// Evaporation exerted (approximated per sub-area availability).
    pub evap_vol: f64,
    /// Infiltration, control-measure exfiltration included.
    pub infil_vol: f64,
    /// Run-on arriving on parcels.
    pub runon_in: f64,
    /// Runoff and drain volume leaving parcels.
    pub runoff_out: f64,
    /// Snow ploughed out of the system.
    pub snow_plowed: f64,
    /// Ponded, held, and snow storage at start.
    pub initial_storage: f64,
}

impl Surface {
    /// Build the surface compartment from a validated network; elapsed
    /// series times anchor at `start_epoch` (s). Parcels invoking
    /// compartments this stage does not evaluate are refused.
    pub fn build(net: &Network, start_epoch: f64) -> Result<Option<Surface>, SurfaceRefusal> {
        // RDII-only models still need the gage records resolved.
        if net.parcels.is_empty() && net.rdii.is_empty() {
            return Ok(None);
        }

        // Gage records resolved to absolute intervals and SI rates.
        let mut gages = Vec::with_capacity(net.gages.len());
        for g in &net.gages {
            let mut intervals = Vec::new();
            if let GageSource::Series { series } = g.source {
                if let TimeSeriesSource::Points(points) = &net.timeseries[series].source {
                    let depth_unit = if net.options.flow_units.is_us() {
                        0.0254
                    } else {
                        0.001
                    };
                    let abs = |st: &SeriesTime| match st {
                        SeriesTime::Elapsed(s) => start_epoch + s,
                        SeriesTime::Absolute { date, seconds } => {
                            time_days_from_civil(*date) as f64 * 86_400.0 + seconds
                        }
                    };
                    match g.form {
                        RainForm::Intensity => {
                            for p in points {
                                intervals.push((abs(&p.time), p.value * depth_unit / 3600.0));
                            }
                        }
                        RainForm::Volume => {
                            for p in points {
                                intervals.push((abs(&p.time), p.value * depth_unit / g.interval));
                            }
                        }
                        RainForm::Cumulative => {
                            let mut prev = 0.0;
                            for p in points {
                                let dv = (p.value - prev).max(0.0);
                                prev = p.value;
                                intervals.push((abs(&p.time), dv * depth_unit / g.interval));
                            }
                        }
                    }
                }
            }
            gages.push(GageRain {
                intervals,
                interval: g.interval,
                scf: g.catch_factor,
            });
        }

        let mut parcels = Vec::with_capacity(net.parcels.len());
        for p in &net.parcels {
            let Some(sa) = &p.subareas else {
                return Err(SurfaceRefusal::Incomplete(format!(
                    "{}: no sub-area data",
                    p.id
                )));
            };
            let a_imp = p.area * p.frac_imperv;
            let a_perv = p.area - a_imp;
            let a_imp0 = a_imp * sa.frac_zero_store;
            let a_imp1 = a_imp - a_imp0;
            // Impervious sub-areas share a prorated α over their combined
            // area; α carries no unit constant in SI (§3.2).
            let alpha = |area_total: f64, n: f64| -> f64 {
                if n <= 0.0 || area_total <= 0.0 || p.area <= 0.0 {
                    0.0
                } else {
                    p.width * p.slope.sqrt() / (area_total * n)
                }
            };
            let alpha_imp = alpha(a_imp, sa.n_imperv);
            let alpha_perv = alpha(a_perv, sa.n_perv);
            let infil = if a_perv > 0.0 {
                let Some(inf) = &p.infiltration else {
                    return Err(SurfaceRefusal::Incomplete(format!(
                        "{}: no infiltration data",
                        p.id
                    )));
                };
                Some(InfilState::build(inf, net.options.infiltration))
            } else {
                None
            };
            parcels.push(ParcelState {
                gage: p.gage,
                outlet: p.outlet,
                sub: [
                    Subarea {
                        area: a_imp1,
                        alpha: alpha_imp,
                        dstore: sa.dstore_imperv,
                        depth: 0.0,
                    },
                    Subarea {
                        area: a_imp0,
                        alpha: alpha_imp,
                        dstore: 0.0,
                        depth: 0.0,
                    },
                    Subarea {
                        area: a_perv,
                        alpha: alpha_perv,
                        dstore: sa.dstore_perv,
                        depth: 0.0,
                    },
                ],
                infil,
                routing: sa.routing,
                frac_routed: sa.frac_routed,
                snow: p
                    .snowpack
                    .map(|sp| SnowPack::build(&net.snowpacks[sp], p.frac_imperv)),
                lids: Vec::new(),
                lid_vertex_drain: Vec::new(),
                runon: 0.0,
                runon_next_vol: 0.0,
                runon_vol: 0.0,
                qstep: QStep::default(),
                runoff: 0.0,
                infil_rate: 0.0,
                evap_rate: 0.0,
            });
        }
        // §3.4 deployment: units attach to their parcels; the combined
        // footprint shrinks the ordinary sub-areas proportionally, and a
        // footprint within 0.1 % of the parcel snaps equal to it.
        for u in &net.lid_usage {
            let unit = LidUnit::build(
                &net.lid_controls[u.control],
                u,
                net.parcels[u.parcel].infiltration.as_ref(),
                net.options.infiltration,
                &net.curves,
                net.options.flow_units.is_us(),
            )
            .map_err(|e| match e {
                super::lid::LidRefusal::Unsupported(m) => SurfaceRefusal::Unsupported(m),
                super::lid::LidRefusal::Invalid(m) => SurfaceRefusal::Incomplete(m),
            })?;
            parcels[u.parcel].lids.push(unit);
        }
        for (pi, p) in parcels.iter_mut().enumerate() {
            if p.lids.is_empty() {
                continue;
            }
            let parcel_area = net.parcels[pi].area;
            let mut foot: f64 = p.lids.iter().map(|u| u.area).sum();
            if parcel_area > 0.0 && (foot - parcel_area).abs() <= 0.001 * parcel_area {
                foot = parcel_area;
            }
            if foot > parcel_area {
                return Err(SurfaceRefusal::Incomplete(format!(
                    "{}: control-measure footprint exceeds the parcel",
                    net.parcels[pi].id
                )));
            }
            let scale = if parcel_area > 0.0 {
                (parcel_area - foot) / parcel_area
            } else {
                0.0
            };
            for s in &mut p.sub {
                s.area *= scale;
            }
        }
        let mut surface = Surface {
            gages,
            parcels,
            start_epoch,
            degraded: false,
            losses: 0.0,
            rainfall: 0.0,
            evap_vol: 0.0,
            infil_vol: 0.0,
            runon_in: 0.0,
            runoff_out: 0.0,
            snow_plowed: 0.0,
            initial_storage: 0.0,
        };
        surface.initial_storage = surface.stored_volume();
        Ok(Some(surface))
    }

    /// Whether any gage is raining or any sub-area holds water above its
    /// depression storage — the wet-step condition (§10.1).
    pub fn is_wet(&self, epoch: f64) -> bool {
        self.gages.iter().any(|g| g.rate(epoch) > 0.0)
            || self.parcels.iter().any(|p| {
                p.sub.iter().any(|s| s.depth > s.dstore + 1e-9)
                    || p.snow.as_ref().is_some_and(|sp| sp.stored_depth() > 0.0)
            })
    }

    /// The earliest gage interval boundary after `epoch` (§10.1 step
    /// truncation).
    pub fn next_gage_boundary(&self, epoch: f64) -> Option<f64> {
        self.gages
            .iter()
            .filter_map(|g| g.next_boundary(epoch))
            .min_by(|a, b| a.total_cmp(b))
    }

    /// Advance every parcel one hydrology step: `epoch` is the absolute
    /// step-start instant, `evap` the potential surface evaporation
    /// (m/s), `dry_only` the §3.1 suppression switch, `rain_factor` the
    /// monthly adjustment. Updates each parcel's runoff rate and hands
    /// run-on one step delayed.
    #[allow(clippy::needless_range_loop)] // paired sub[i]/runoff[i] indexing
    #[allow(clippy::too_many_arguments)] // the step's climate inputs
    pub fn step(
        &mut self,
        epoch: f64,
        dt: f64,
        evap: f64,
        dry_only: bool,
        rain_factor: f64,
        fac: InfilFactors,
        snow_cl: Option<&SnowClimate>,
    ) {
        self.degraded = false;
        let mut runon_arrived = 0.0;
        let elapsed_days = (epoch - self.start_epoch) / 86_400.0;
        // Run-on volumes booked last step arrive as this step's rates,
        // spread over the non-measure area; the volume is kept for the
        // §3.4 full-footprint gate.
        for p in &mut self.parcels {
            let ordinary: f64 = p.sub.iter().map(|s| s.area).sum();
            p.runon_vol = p.runon_next_vol;
            runon_arrived += p.runon_vol;
            p.runon = if ordinary > 0.0 {
                p.runon_vol / dt / ordinary
            } else {
                0.0
            };
            p.runon_next_vol = 0.0;
        }
        self.runon_in += runon_arrived;

        let mut runon_to_parcel = vec![0.0_f64; self.parcels.len()];
        let mut snow_transfers: Vec<(usize, f64)> = Vec::new();
        for pi in 0..self.parcels.len() {
            let gage = &self.gages[self.parcels[pi].gage];
            let precip = gage.rate(epoch) * rain_factor;
            let scf = gage.scf;
            self.rainfall += precip * dt * self.parcels[pi].sub.iter().map(|s| s.area).sum::<f64>();
            let e = if dry_only && precip > 0.0 { 0.0 } else { evap };
            let p = &mut self.parcels[pi];
            let area_total_pre: f64 = p.sub.iter().map(|s| s.area).sum();

            // §4.2: split precipitation at the rain/snow temperature and
            // route it through the pack; a parcel without a pack receives
            // catch-scaled snowfall as immediate liquid.
            let (imp_precip, perv_precip) = match (&mut p.snow, snow_cl) {
                (Some(pack), Some(cl)) => {
                    let (rain, snowf) = if cl.ta <= cl.snow_temp {
                        (0.0, precip * scf)
                    } else {
                        (precip, 0.0)
                    };
                    pack.plow(snowf, dt, area_total_pre);
                    self.snow_plowed += std::mem::take(&mut pack.exported);
                    snow_transfers.extend(pack.transfer_out.iter().copied());
                    let (imp, perv, _) = pack.melt(rain, snowf, dt, cl);
                    (imp, perv)
                }
                (None, Some(cl)) if cl.ta <= cl.snow_temp => {
                    let liquid = precip * scf;
                    (liquid, liquid)
                }
                _ => (precip, precip),
            };
            // §8: record the quality context as the step unfolds.
            let a_imp_q = p.sub[0].area + p.sub[1].area;
            let a_perv_q = p.sub[2].area;
            let ponded_start: f64 = p.sub.iter().map(|s| s.depth * s.area).sum();
            let lid_area_q: f64 = p.lids.iter().map(|u| u.area).sum();
            p.qstep = QStep {
                rain_rate: precip,
                rain_vol: (imp_precip * a_imp_q + perv_precip * a_perv_q) * dt,
                lid_rain_vol: imp_precip * lid_area_q * dt,
                v_inflow: p.runon_vol
                    + ponded_start
                    + (imp_precip * a_imp_q + perv_precip * a_perv_q) * dt,
                runon_vol: p.runon_vol,
                v_infil: 0.0,
                v_outflow: 0.0,
                v_out2: 0.0,
                ponded_end: 0.0,
                runoff_rate: 0.0,
                snow_cover: p.snow.as_ref().is_some_and(|pk| pk.has_cover()),
                snow_depth: p.snow.as_ref().map_or(0.0, |pk| pk.mean_depth()),
                dt,
            };

            let input = imp_precip + p.runon;
            let perv_input = perv_precip + p.runon;

            // Pervious infiltration capacity for this step (§3.3).
            let perv_depth = p.sub[2].depth;
            let f_rate = match &mut p.infil {
                Some(state) if p.sub[2].area > 0.0 => {
                    state.step(dt, (perv_input - e).max(0.0), perv_depth, fac)
                }
                _ => 0.0,
            };

            // §11.1: evaporation exerted, approximated per sub-area by
            // availability before the reservoirs advance.
            for (i, sub) in p.sub.iter().enumerate() {
                if sub.area <= 0.0 || e <= 0.0 {
                    continue;
                }
                let supply = if i < 2 { input } else { perv_input };
                let used = e.min(sub.depth / dt + supply.max(0.0));
                self.evap_vol += used.max(0.0) * dt * sub.area;
            }

            // Sub-area order honours the internal re-routing direction:
            // the router computes the source first (§3.2).
            let mut degraded = false;
            let (imp_in, perv_in) = (input, perv_input);
            let mut runoff = [0.0_f64; 3];
            match p.routing {
                SubareaRouting::Pervious => {
                    // Impervious first; a fraction of its runoff becomes
                    // extra input on the pervious plane.
                    for i in 0..2 {
                        runoff[i] = p.sub[i].advance(imp_in - e, dt, &mut degraded);
                    }
                    let routed = (runoff[0] + runoff[1]) * p.frac_routed;
                    let extra = if p.sub[2].area > 0.0 {
                        routed / dt / p.sub[2].area
                    } else {
                        0.0
                    };
                    runoff[2] = p.sub[2].advance(perv_in + extra - e - f_rate, dt, &mut degraded);
                    runoff[0] *= 1.0 - p.frac_routed;
                    runoff[1] *= 1.0 - p.frac_routed;
                }
                SubareaRouting::Impervious => {
                    runoff[2] = p.sub[2].advance(perv_in - e - f_rate, dt, &mut degraded);
                    let routed = runoff[2] * p.frac_routed;
                    let a_imp = p.sub[0].area + p.sub[1].area;
                    let extra = if a_imp > 0.0 {
                        routed / dt / a_imp
                    } else {
                        0.0
                    };
                    for i in 0..2 {
                        runoff[i] = p.sub[i].advance(imp_in + extra - e, dt, &mut degraded);
                    }
                    runoff[2] *= 1.0 - p.frac_routed;
                }
                SubareaRouting::Outlet => {
                    for i in 0..2 {
                        runoff[i] = p.sub[i].advance(imp_in - e, dt, &mut degraded);
                    }
                    runoff[2] = p.sub[2].advance(perv_in - e - f_rate, dt, &mut degraded);
                }
            }
            if degraded {
                self.degraded = true;
            }
            self.losses += f_rate * dt * p.sub[2].area;
            self.infil_vol += f_rate * dt * p.sub[2].area;
            p.qstep.v_infil = f_rate * dt * p.sub[2].area;
            p.qstep.v_outflow = runoff.iter().sum::<f64>();

            // §3.4: units take their captured share of sub-area runoff
            // plus direct rainfall; overflow rejoins parcel runoff,
            // exfiltration joins the losses, drains route separately.
            if !p.lids.is_empty() {
                p.lid_vertex_drain.clear();
                let imp_runoff = runoff[0] + runoff[1];
                let perv_runoff = runoff[2];
                // Upstream run-on reaches units only when the footprint
                // equals the whole parcel — the ordinary sub-areas have
                // shrunk to nothing, so the volume has nowhere else to
                // land (§3.4).
                let ordinary: f64 = p.sub.iter().map(|s| s.area).sum();
                let lid_area: f64 = p.lids.iter().map(|u| u.area).sum();
                let runon_rate = if ordinary <= 0.0 && lid_area > 0.0 {
                    p.runon_vol / dt / lid_area
                } else {
                    0.0
                };
                let mut captured_imp = 0.0;
                let mut captured_perv = 0.0;
                let mut lid_return = 0.0;
                let mut drains_out = 0.0;
                for u in &mut p.lids {
                    if u.area <= 0.0 {
                        continue;
                    }
                    let take_imp = imp_runoff * u.from_imperv;
                    let take_perv = perv_runoff * u.from_perv;
                    captured_imp += take_imp;
                    captured_perv += take_perv;
                    let unit_in = imp_precip + runon_rate + (take_imp + take_perv) / dt / u.area;
                    u.step(
                        &super::lid::LidForcing {
                            inflow: unit_in,
                            rain: imp_precip,
                            evap: e,
                            native_infil: f_rate,
                            fac,
                            elapsed_days,
                        },
                        dt,
                    );
                    self.losses += u.exfiltration * u.area * dt;
                    self.infil_vol += u.exfiltration * u.area * dt;
                    lid_return += u.overflow * u.area * dt;
                    let drain_vol = u.drain_flow * u.area;
                    drains_out += drain_vol * dt;
                    match u.drain_to {
                        Some(ParcelOutlet::Vertex(v)) => {
                            p.lid_vertex_drain.push((v, drain_vol));
                        }
                        Some(ParcelOutlet::Parcel(target)) => {
                            runon_to_parcel[target] += drain_vol * dt;
                        }
                        None => lid_return += drain_vol * dt,
                    }
                }
                runoff[0] -= captured_imp * (runoff[0] / imp_runoff.max(1e-30));
                runoff[1] -= captured_imp * (runoff[1] / imp_runoff.max(1e-30));
                runoff[2] -= captured_perv;
                runoff[2] += 0.0;
                runoff[0] += lid_return; // overflow and default drains join
                p.qstep.v_out2 = drains_out;
            }
            let area_total: f64 = p.sub.iter().map(|s| s.area).sum();
            if area_total > 0.0 {
                p.infil_rate = f_rate * p.sub[2].area / area_total;
                let wet_perv = p.sub[2].depth > 0.0 || perv_depth > 0.0;
                p.evap_rate = if wet_perv {
                    e * p.sub[2].area / area_total
                } else {
                    0.0
                };
            }
            let total: f64 = runoff.iter().sum();
            p.runoff = total / dt;
            p.qstep.v_out2 += total;
            self.runoff_out += p.qstep.v_out2;
            p.qstep.ponded_end = p.sub.iter().map(|s| s.depth * s.area).sum();
            let area_q: f64 = p.sub.iter().map(|s| s.area).sum();
            if area_q > 0.0 {
                p.qstep.runoff_rate = p.qstep.v_outflow / dt / area_q;
            }

            if let ParcelOutlet::Parcel(target) = p.outlet {
                runon_to_parcel[target] += total;
            }
        }
        for (pi, vol) in runon_to_parcel.into_iter().enumerate() {
            self.parcels[pi].runon_next_vol += vol;
        }
        // Plowed transfers land on their targets' pervious packs for the
        // next step.
        for (target, vol) in snow_transfers {
            let area: f64 = self.parcels[target].sub.iter().map(|s| s.area).sum();
            if let Some(pack) = &mut self.parcels[target].snow {
                pack.receive(vol, area);
            }
        }
    }

    /// Current per-parcel outlet runoff rates (m³/s); parcels draining to
    /// other parcels report zero at the vertex boundary.
    pub fn vertex_laterals(&self, nv: usize) -> Vec<f64> {
        let mut lat = vec![0.0; nv];
        for p in &self.parcels {
            if let ParcelOutlet::Vertex(v) = p.outlet {
                lat[v] += p.runoff;
            }
            for &(v, q) in &p.lid_vertex_drain {
                lat[v] += q;
            }
        }
        lat
    }

    /// A parcel's current runoff rate (m³/s).
    pub fn parcel_runoff(&self, pi: usize) -> f64 {
        self.parcels.get(pi).map_or(0.0, |p| p.runoff)
    }

    /// Current surface storage (m³): ponded sub-area water, control
    /// measures' held water, and snow (§11.1).
    pub fn stored_volume(&self) -> f64 {
        let mut v = 0.0;
        for p in &self.parcels {
            let area: f64 = p.sub.iter().map(|s| s.area).sum();
            v += p.sub.iter().map(|s| s.depth * s.area).sum::<f64>();
            v += p
                .lids
                .iter()
                .map(|u| u.stored_depth() * u.area)
                .sum::<f64>();
            if let Some(pack) = &p.snow {
                v += pack.stored_volume(area);
            }
            v += p.runon_next_vol;
        }
        v
    }

    /// The §8 quality context recorded for parcel `pi`'s last step.
    pub fn qstep(&self, pi: usize) -> QStep {
        self.parcels[pi].qstep
    }

    /// Parcel `pi`'s control-measure drain flows routed to vertices this
    /// step: (vertex, m³/s).
    pub fn lid_drains(&self, pi: usize) -> &[(usize, f64)] {
        &self.parcels[pi].lid_vertex_drain
    }

    /// Rainfall depth (m) at gage `gi` over the `n` completed hourly
    /// buckets before `epoch` (§9.1).
    pub fn gage_past_depth(&self, gi: usize, epoch: f64, hours: u32) -> f64 {
        let g = &self.gages[gi];
        let end = (epoch / 3600.0).floor() * 3600.0;
        let start = end - f64::from(hours) * 3600.0;
        let mut v = 0.0;
        for &(t0, rate) in &g.intervals {
            let a = t0.max(start);
            let b = (t0 + g.interval).min(end);
            if b > a {
                v += rate * (b - a);
            }
        }
        v
    }

    /// A gage's rain rate at an absolute epoch time (m/s), for the §4.3
    /// convolution.
    pub fn gage_rate(&self, gi: usize, epoch: f64) -> f64 {
        self.gages.get(gi).map_or(0.0, |g| g.rate(epoch))
    }

    /// A parcel's infiltration and exerted pervious-evaporation rates
    /// this step (m/s over the whole parcel).
    pub fn parcel_infil_evap(&self, pi: usize) -> (f64, f64) {
        self.parcels
            .get(pi)
            .map_or((0.0, 0.0), |p| (p.infil_rate, p.evap_rate))
    }
}
