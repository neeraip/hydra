//! Surface quality (§8.2–§8.3): accumulation whose state is mass — each
//! dry step inverting the chosen form to equivalent time, advancing, and
//! re-evaluating — street cleaning within its seasonal window, the three
//! mobilisation relations, co-pollutant potency, and the per-parcel
//! ponded store that mixes wet deposition and run-on loads with a closed
//! mass balance.

use crate::hydrology::runoff::QStep;
use crate::model::{
    Buildup, BuildupForm, BuildupNormalizer, ConcentrationUnits, Network, ParcelOutlet, WashoffForm,
};

/// The predecessor's minimum runoff intensity (m/s): 0.001 in/hr.
const MIN_RUNOFF: f64 = 7.055_6e-9;

/// Internal mass unit: concentration unit × m³ (e.g. g for mg/L).
/// One file mass unit (lbs or kg, counts for count-type constituents)
/// converted per constituent at build.
struct LandState {
    /// Per-(cover slot, constituent) accumulated mass (U).
    buildup: Vec<Vec<f64>>,
    /// Per-cover-slot last-swept instant (epoch days).
    last_swept: Vec<f64>,
    /// Per-constituent ponded mass (U).
    ponded: Vec<f64>,
    /// Per-constituent run-on mass arriving this step (U).
    runon_mass: Vec<f64>,
}

/// The §8.2–§8.3 surface-quality state across all parcels.
pub struct SurfaceQuality {
    parcels: Vec<LandState>,
    /// Outflow concentration per parcel per constituent (conc units).
    pub conc: Vec<Vec<f64>>,
    /// Run-on mass booked for next step, per parcel per constituent (U).
    runon_next: Vec<Vec<f64>>,
    /// U per file mass unit, per constituent.
    mass_cv: Vec<f64>,
    /// File land-area unit (m² per acre/ha).
    cv_area: f64,
    /// File rain-rate unit (m/s per in/hr | mm/hr).
    cv_rain: f64,
    /// File flow unit (m³/s per unit).
    cv_flow: f64,
    // §11 ledger accounts, per constituent (U).
    pub buildup_in: Vec<f64>,
    pub deposition: Vec<f64>,
    pub swept: Vec<f64>,
    pub infiltrated: Vec<f64>,
    pub bmp_removed: Vec<f64>,
    pub to_final: Vec<f64>,
    /// Buildup present at start (U), §11.1.
    pub initial_buildup: Vec<f64>,
    /// Mass delivered off the parcels into the network (U), §11.1.
    pub washed_off: Vec<f64>,
    /// §11.2 per-parcel delivered washoff `[parcel][constituent]` (U).
    pub washed_by_parcel: Vec<Vec<f64>>,
}

impl SurfaceQuality {
    /// Seed buildup from user loadings or the antecedent dry days (§8.2).
    pub fn build(net: &Network) -> SurfaceQuality {
        let np = net.constituents.len();
        let us = net.options.flow_units.is_us();
        let mass_cv = net
            .constituents
            .iter()
            .map(|c| match c.units {
                // U = 1000 concentration-mass units.
                ConcentrationUnits::MgPerL => {
                    if us {
                        453.592_37
                    } else {
                        1000.0
                    }
                }
                ConcentrationUnits::UgPerL => {
                    if us {
                        453_592.37
                    } else {
                        1.0e6
                    }
                }
                // Count constituents carry file mass in count units.
                ConcentrationUnits::CountPerL => 1.0e-3,
            })
            .collect::<Vec<_>>();
        let cv_area = if us { 4_046.856_422_4 } else { 10_000.0 };
        let start_day = crate::simulation::time::days_from_civil(net.options.start_date) as f64;
        let mut parcels = Vec::new();
        for p in &net.parcels {
            let mut buildup = Vec::new();
            let mut last_swept = Vec::new();
            for &(lu, f) in &p.land_cover {
                let land = &net.land_uses[lu];
                let per_area = f * p.area / cv_area;
                let per_curb = f * p.curb_length;
                let mut row = vec![0.0; np];
                for ci in 0..np {
                    // A user loading overrides — with or without an
                    // accumulation relation; else the form evaluates
                    // over the antecedent dry days (§8.2).
                    let init = p.init_buildup.iter().find(|(c, _)| *c == ci);
                    row[ci] = match init {
                        Some(&(_, load)) => load * per_area * mass_cv[ci],
                        // The external form bypasses the mechanism: it
                        // starts empty absent a user loading (§8.2).
                        None => land.buildup[ci]
                            .as_ref()
                            .filter(|b| b.form != BuildupForm::External)
                            .map_or(0.0, |b| {
                                buildup_mass(b, net.options.dry_days)
                                    * normalizer(b, per_area, per_curb)
                                    * mass_cv[ci]
                            }),
                    };
                }
                buildup.push(row);
                last_swept.push(start_day - land.sweep_days_since);
            }
            parcels.push(LandState {
                buildup,
                last_swept,
                ponded: vec![0.0; np],
                runon_mass: vec![0.0; np],
            });
        }
        let n = net.parcels.len();
        let initial_buildup = (0..np)
            .map(|ci| {
                parcels
                    .iter()
                    .flat_map(|st| st.buildup.iter().map(move |row| row[ci]))
                    .sum()
            })
            .collect();
        SurfaceQuality {
            parcels,
            conc: vec![vec![0.0; np]; n],
            runon_next: vec![vec![0.0; np]; n],
            mass_cv,
            cv_area,
            cv_rain: if us { 0.0254 } else { 1.0e-3 } / 3600.0,
            cv_flow: if us { 0.028_316_846_592 } else { 1.0 },
            buildup_in: vec![0.0; np],
            deposition: vec![0.0; np],
            swept: vec![0.0; np],
            infiltrated: vec![0.0; np],
            bmp_removed: vec![0.0; np],
            to_final: vec![0.0; np],
            initial_buildup,
            washed_off: vec![0.0; np],
            washed_by_parcel: vec![vec![0.0; np]; n],
        }
    }

    /// Export parcel `pi`'s §14.8 quality state: ponded masses and, per
    /// cover slot, the buildup row and last-swept day.
    pub fn hotstart_get(&self, pi: usize) -> (Vec<f64>, Vec<(Vec<f64>, f64)>) {
        let st = &self.parcels[pi];
        (
            st.ponded.clone(),
            st.buildup
                .iter()
                .zip(&st.last_swept)
                .map(|(row, sw)| (row.clone(), *sw))
                .collect(),
        )
    }

    /// Restore parcel `pi`'s §14.8 quality state.
    pub fn hotstart_set(&mut self, pi: usize, ponded: Vec<f64>, slots: Vec<(Vec<f64>, f64)>) {
        let st = &mut self.parcels[pi];
        if ponded.len() == st.ponded.len() {
            st.ponded = ponded;
        }
        for (slot, (row, sw)) in slots.into_iter().enumerate() {
            if slot < st.buildup.len() && row.len() == st.buildup[slot].len() {
                st.buildup[slot] = row;
                st.last_swept[slot] = sw;
            }
        }
    }

    /// Re-take the §11.1 opening buildup from the state now held.
    ///
    /// A restore (§14.8) replaces the buildup the model was built with;
    /// the loading ledger's inflow side must then open from what was
    /// restored, not from what was discarded. Without this the ledger
    /// counts the built buildup as an inflow that never washes off — a
    /// hotstarted run reports near-total loading error while the cold run
    /// of the same model closes.
    pub fn rebase_initial_buildup(&mut self) {
        let np = self.initial_buildup.len();
        self.initial_buildup = (0..np)
            .map(|ci| {
                self.parcels
                    .iter()
                    .flat_map(|st| st.buildup.iter().map(move |row| row[ci]))
                    .sum()
            })
            .collect();
    }

    /// Buildup and ponded mass currently on the surfaces (U), §11.1.
    pub fn stored_mass(&self, ci: usize) -> f64 {
        self.parcels
            .iter()
            .map(|st| {
                st.buildup.iter().map(|row| row[ci]).sum::<f64>()
                    + st.ponded[ci]
                    + st.runon_mass[ci]
            })
            .sum::<f64>()
            + self.runon_next.iter().map(|row| row[ci]).sum::<f64>()
    }

    /// Advance one hydrology step for every parcel. `qsteps[pi]` is the
    /// §3 volume context, `date_days` the civil-day clock, and
    /// `in_sweep_season` the §8.2 seasonal window test for today.
    #[allow(clippy::needless_range_loop)] // parallel per-parcel state rows
    pub fn step(
        &mut self,
        net: &Network,
        qsteps: &[QStep],
        date_days: f64,
        in_sweep_season: bool,
        series_value: &dyn Fn(usize) -> f64,
    ) {
        let np = net.constituents.len();
        // Run-on booked last step arrives now, accumulating so a parcel
        // skipped by the guards below keeps its pending mass.
        for (pi, state) in self.parcels.iter_mut().enumerate() {
            for (ci, m) in std::mem::replace(&mut self.runon_next[pi], vec![0.0; np])
                .into_iter()
                .enumerate()
            {
                state.runon_mass[ci] += m;
            }
        }
        for pi in 0..net.parcels.len() {
            let q = &qsteps[pi];
            let dt = q.dt;
            if dt <= 0.0 {
                continue;
            }
            let parcel = &net.parcels[pi];
            let area = parcel.area;
            if area <= 0.0 || np == 0 {
                continue;
            }

            // §8.2: accumulation while runoff is negligible; sweeping in
            // season while it is not raining.
            if q.runoff_rate < MIN_RUNOFF {
                self.grow_buildup(net, pi, dt, q.snow_cover, series_value);
            }
            if in_sweep_season && q.rain_rate <= MIN_RUNOFF && !q.snow_cover {
                self.sweep(net, pi, date_days);
            }

            // §8.3: washoff, ponded store, and control-measure paths.
            let mut outflow_load = vec![0.0; np];
            self.washoff_loads(net, pi, q, &mut outflow_load);
            self.ponded_loads(net, pi, q, &mut outflow_load);
            // Direct rain on the control-measure footprint joins at the
            // rain concentration.
            for (ci, c) in net.constituents.iter().enumerate() {
                let w = c.c_rain * q.lid_rain_vol;
                outflow_load[ci] += w;
                self.deposition[ci] += w;
            }

            // Outflow concentration: total load over the pre-measure
            // outflow volume, zero when outflow is negligible (§8.3).
            let v_out1 = q.v_outflow + q.lid_rain_vol;
            let has_outflow = q.v_out2 > MIN_RUNOFF * area * dt;
            let has_lids = net.lid_usage.iter().any(|u| u.parcel == pi);
            // The parcel-outlet share cascades as run-on when the outlet
            // is another parcel; only network-bound mass books as
            // wash-off, so a cascading load counts once (§11.1).
            let outlet_is_parcel = matches!(parcel.outlet, ParcelOutlet::Parcel(_));
            let to_network = if outlet_is_parcel {
                q.v_vertex_drains
            } else {
                q.v_out2
            };
            for ci in 0..np {
                let load = outflow_load[ci];
                let c_out = if v_out1 > 0.0 { load / v_out1 } else { 0.0 };
                if has_outflow {
                    if q.v_out2 < v_out1 {
                        self.bmp_removed[ci] += c_out * (v_out1 - q.v_out2);
                    }
                    self.washed_off[ci] += c_out * to_network;
                    self.washed_by_parcel[pi][ci] += c_out * to_network;
                    self.conc[pi][ci] = c_out;
                } else {
                    // Nothing leaves: the whole load stays on-site —
                    // absorbed by the measures when present, written off
                    // otherwise, so the ledger closes (§8.3, §11.1).
                    if has_lids {
                        self.bmp_removed[ci] += load;
                    } else {
                        self.to_final[ci] += load;
                    }
                    self.conc[pi][ci] = 0.0;
                }
            }

            // Loads to another parcel become its run-on next step (§8.3);
            // vertex-routed drain mass goes to the network instead.
            if let ParcelOutlet::Parcel(target) = parcel.outlet {
                for ci in 0..np {
                    self.runon_next[target][ci] +=
                        self.conc[pi][ci] * (q.v_out2 - q.v_vertex_drains).max(0.0);
                }
            }
        }
    }

    /// §8.2 accumulation over a dry step: invert to equivalent time,
    /// advance, re-evaluate; never lose mass.
    fn grow_buildup(
        &mut self,
        net: &Network,
        pi: usize,
        dt: f64,
        snow_cover: bool,
        series_value: &dyn Fn(usize) -> f64,
    ) {
        let parcel = &net.parcels[pi];
        for (slot, &(lu, f)) in parcel.land_cover.iter().enumerate() {
            if f <= 0.0 {
                continue;
            }
            let land = &net.land_uses[lu];
            let per_area = f * parcel.area / self.cv_area;
            let per_curb = f * parcel.curb_length;
            for (ci, b) in land.buildup.iter().enumerate() {
                let Some(b) = b else { continue };
                if b.form == BuildupForm::None {
                    continue;
                }
                if net.constituents[ci].snow_only && !snow_cover {
                    continue;
                }
                let per_unit = normalizer(b, per_area, per_curb);
                if per_unit <= 0.0 {
                    continue;
                }
                let old = self.parcels[pi].buildup[slot][ci];
                let per = old / per_unit / self.mass_cv[ci];
                let new_per = if b.form == BuildupForm::External {
                    // A scaled loading series (mass per unit per day),
                    // capped at the maximum; beyond its range the series
                    // reads zero (§8.2, §10.1).
                    let rate = b.series.map_or(0.0, series_value) * b.coeffs[1];
                    (per + rate * dt / 86_400.0).min(b.coeffs[0])
                } else {
                    let days = buildup_days(b, per) + dt / 86_400.0;
                    buildup_mass(b, days)
                };
                let new = (new_per * per_unit * self.mass_cv[ci]).max(old);
                self.parcels[pi].buildup[slot][ci] = new;
                self.buildup_in[ci] += new - old;
            }
        }
    }

    /// §8.2 street cleaning: availability × efficiency of current mass.
    fn sweep(&mut self, net: &Network, pi: usize, date_days: f64) {
        let parcel = &net.parcels[pi];
        for (slot, &(lu, f)) in parcel.land_cover.iter().enumerate() {
            if f <= 0.0 {
                continue;
            }
            let land = &net.land_uses[lu];
            if land.sweep_interval <= 0.0 {
                continue;
            }
            if date_days - self.parcels[pi].last_swept[slot] < land.sweep_interval {
                continue;
            }
            self.parcels[pi].last_swept[slot] = date_days;
            for (ci, w) in land.washoff.iter().enumerate() {
                let effic = w.as_ref().map_or(0.0, |x| x.sweep_efficiency / 100.0);
                let old = self.parcels[pi].buildup[slot][ci];
                let new = (old * (1.0 - land.sweep_removal * effic)).clamp(0.0, old);
                self.parcels[pi].buildup[slot][ci] = new;
                self.swept[ci] += old - new;
            }
        }
    }

    /// §8.3 mobilisation from each land use, source-limited, with
    /// removal credits and co-pollutant potency.
    fn washoff_loads(&mut self, net: &Network, pi: usize, q: &QStep, out: &mut [f64]) {
        if q.runoff_rate < MIN_RUNOFF {
            return;
        }
        let parcel = &net.parcels[pi];
        let np = out.len();
        let mut loads = vec![0.0; np];
        for (slot, &(lu, f)) in parcel.land_cover.iter().enumerate() {
            if f <= 0.0 {
                continue;
            }
            let land = &net.land_uses[lu];
            for (ci, w) in land.washoff.iter().enumerate() {
                let Some(w) = w else { continue };
                let buildup = self.parcels[pi].buildup[slot][ci];
                let has_buildup_fn = land.buildup[ci]
                    .as_ref()
                    .is_some_and(|b| b.form != BuildupForm::None);
                if has_buildup_fn && buildup <= 0.0 {
                    continue;
                }
                // The load over the step (U), per form (§8.3).
                let mut load = washoff_load(
                    w,
                    &WashoffInputs {
                        buildup,
                        share: f,
                        runoff_rate: q.runoff_rate,
                        v_outflow: q.v_outflow,
                        dt: q.dt,
                        parcel_area: parcel.area,
                        cv_rain: self.cv_rain,
                        cv_flow: self.cv_flow,
                    },
                );
                if load <= 0.0 {
                    continue;
                }
                // Source-limit against buildup, or book the load as an
                // equal, simultaneous accumulation input (§8.3).
                if has_buildup_fn || buildup > load {
                    load = load.min(buildup);
                    self.parcels[pi].buildup[slot][ci] = buildup - load;
                } else {
                    self.buildup_in[ci] += load - buildup;
                    self.parcels[pi].buildup[slot][ci] = 0.0;
                }
                // BMP removal credits the mobilised stream (§8.3).
                let removed = w.bmp_efficiency / 100.0 * load;
                if removed > 0.0 {
                    self.bmp_removed[ci] += removed;
                }
                loads[ci] += load - removed;
            }
        }
        // Co-pollutant potency on mobilised loads, booked as an
        // accumulation input so the ledger closes (§8.1).
        for (ci, c) in net.constituents.iter().enumerate() {
            if let Some(co) = c.co_constituent {
                let w = c.co_fraction * loads[co];
                self.buildup_in[ci] += w;
                loads[ci] += w;
            }
        }
        for ci in 0..np {
            out[ci] += loads[ci];
        }
    }

    /// §8.3 ponded store: mix deposition and run-on, remove the
    /// infiltration and outflow shares in order, carry the residual.
    #[allow(clippy::needless_range_loop)] // parallel per-constituent rows
    fn ponded_loads(&mut self, net: &Network, pi: usize, q: &QStep, out: &mut [f64]) {
        let parcel = &net.parcels[pi];
        // Area-weighted mean removal over the parcel's land uses.
        let np = out.len();
        for ci in 0..np {
            let rain_mass = net.constituents[ci].c_rain * q.rain_vol;
            self.deposition[ci] += rain_mass;
            if q.v_inflow <= 0.0 {
                // A step with no inflow writes residual ponded mass off
                // to final storage (§8.3) — arrived run-on included, so
                // the accumulator cannot re-deliver it.
                self.to_final[ci] += self.parcels[pi].ponded[ci]
                    + rain_mass
                    + std::mem::take(&mut self.parcels[pi].runon_mass[ci]);
                self.parcels[pi].ponded[ci] = 0.0;
                continue;
            }
            // Run-on is consumed on arrival: taking it here is what makes
            // "one delivery, one arrival" true — a plain read would
            // re-inject the same mass every subsequent wet step.
            let mut mass = self.parcels[pi].ponded[ci]
                + rain_mass
                + std::mem::take(&mut self.parcels[pi].runon_mass[ci]);
            let c_ponded = mass / q.v_inflow;
            // Infiltration then outflow, each clamped (§8.3).
            let w_infil = (c_ponded * q.v_infil).min(mass);
            self.infiltrated[ci] += w_infil;
            mass -= w_infil;
            let mut w_out = (c_ponded * q.v_outflow).min(mass);
            mass -= w_out;
            // The area-weighted mean removal discounts the ponded stream.
            let mut effic = 0.0;
            for &(lu, f) in &parcel.land_cover {
                effic += f * net.land_uses[lu].washoff[ci]
                    .as_ref()
                    .map_or(0.0, |w| w.bmp_efficiency / 100.0);
            }
            let removed = effic * w_out;
            if removed > 0.0 {
                self.bmp_removed[ci] += removed;
                w_out -= removed;
            }
            // Evaporation leaves mass behind: the residual mass is what
            // the new ponded volume carries — the store's balance is
            // closed, unlike the predecessor's depth-based write (§8.3).
            self.parcels[pi].ponded[ci] = mass;
            out[ci] += w_out;
        }
    }
}

/// What one mobilisation relation sees of a step (§8.3).
///
/// A struct rather than eight positional arguments: every field is a
/// length or a rate and half of them are unit-conversion factors, which
/// is exactly the shape a caller gets wrong silently.
pub(crate) struct WashoffInputs {
    /// Mass currently accumulated for this (constituent, land use).
    pub buildup: f64,
    /// The land use's share of the parcel.
    pub share: f64,
    /// Runoff rate over the parcel (m/s).
    pub runoff_rate: f64,
    /// Runoff volume leaving over the step (m³).
    pub v_outflow: f64,
    /// Step length (s).
    pub dt: f64,
    /// Parcel area (m²).
    pub parcel_area: f64,
    /// Rain-rate conversion into the file's units.
    pub cv_rain: f64,
    /// Flow conversion into the file's units.
    pub cv_flow: f64,
}

/// §8.3: the load one relation mobilises over a step, before
/// source-limiting, BMP removal and co-pollutant potency.
///
/// Extracted from `washoff_loads` because the three forms are the physics
/// and everything around them is bookkeeping. Inlined, the exponent could
/// be dropped from either power form and the whole suite still passed:
/// what covers this module is a checkpoint round-trip, which asserts a run
/// resumes identically and is invariant to every formula in it being wrong.
pub(crate) fn washoff_load(w: &crate::model::Washoff, i: &WashoffInputs) -> f64 {
    match w.form {
        WashoffForm::None => 0.0,
        WashoffForm::Exponential => {
            // Coefficient is per hour on the file rain-rate.
            w.coeff / 3600.0
                * (i.runoff_rate / i.cv_rain).powf(w.exponent)
                * i.buildup
                * i.dt
                * (i.v_outflow / (i.runoff_rate * i.parcel_area * i.dt).max(1e-30))
        }
        WashoffForm::RatingCurve => {
            // On the land-use share of the actual runoff flow, in file
            // units; the load lands in concentration-mass per second.
            let q_share = i.share * (i.v_outflow / i.dt) / i.cv_flow;
            w.coeff * q_share.powf(w.exponent) * i.dt / 1000.0
        }
        // Concentration on the land-use share of outflow.
        WashoffForm::Emc => w.coeff * i.share * i.v_outflow,
    }
}

fn normalizer(b: &Buildup, per_area: f64, per_curb: f64) -> f64 {
    match b.normalizer {
        BuildupNormalizer::PerArea => per_area,
        BuildupNormalizer::PerCurb => per_curb,
    }
}

/// Days until maximum, per the predecessor's read-time rules (§8.2).
fn max_days(b: &Buildup) -> f64 {
    let [c0, c1, c2] = b.coeffs;
    match b.form {
        BuildupForm::Power => {
            if c1 * c2 == 0.0 {
                0.0
            } else if c0.log10() / c2 > 3.5 {
                3650.0
            } else {
                (c0 / c1).powf(1.0 / c2)
            }
        }
        BuildupForm::Exponential => {
            if c1 == 0.0 {
                0.0
            } else {
                -(0.001_f64.ln()) / c1
            }
        }
        BuildupForm::Saturation => 1000.0 * c2,
        // The external form has no time-to-maximum; its cap applies on
        // accumulation, never as an initial pin (§8.2).
        _ => f64::MAX,
    }
}

/// Invert the form: equivalent days for a per-unit mass (§8.2).
fn buildup_days(b: &Buildup, per_unit_mass: f64) -> f64 {
    let [c0, c1, c2] = b.coeffs;
    if per_unit_mass == 0.0 {
        return 0.0;
    }
    if per_unit_mass >= c0 {
        return max_days(b);
    }
    match b.form {
        BuildupForm::Power => {
            if c1 * c2 == 0.0 {
                0.0
            } else {
                (per_unit_mass / c1).powf(1.0 / c2)
            }
        }
        BuildupForm::Exponential => {
            if c0 * c1 == 0.0 {
                0.0
            } else {
                -(1.0 - per_unit_mass / c0).ln() / c1
            }
        }
        BuildupForm::Saturation => {
            if c0 == 0.0 {
                0.0
            } else {
                per_unit_mass * c2 / (c0 - per_unit_mass)
            }
        }
        _ => 0.0,
    }
}

/// Evaluate the form at `days` (§8.2), pinned at the maximum.
fn buildup_mass(b: &Buildup, days: f64) -> f64 {
    let [c0, c1, c2] = b.coeffs;
    if days <= 0.0 {
        return 0.0;
    }
    if days >= max_days(b) {
        return c0;
    }
    match b.form {
        BuildupForm::Power => (c1 * days.powf(c2)).min(c0),
        BuildupForm::Exponential => c0 * (1.0 - (-days * c1).exp()),
        BuildupForm::Saturation => days * c0 / (c2 + days),
        _ => 0.0,
    }
}

// ── Checkpointing (§12.3) ────────────────────────────────────────────────────

impl LandState {
    /// Write one parcel's surface loading (§12.3).
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_fs, put_rows};
        let LandState {
            buildup,
            last_swept,
            ponded,
            runon_mass,
        } = self;
        put_rows(w, buildup)?;
        put_fs(w, last_swept)?;
        put_fs(w, ponded)?;
        put_fs(w, runon_mass)
    }

    /// Read back what `checkpoint_put` wrote.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        self.buildup = r.rows()?;
        self.last_swept = r.fs()?;
        self.ponded = r.fs()?;
        self.runon_mass = r.fs()?;
        Ok(())
    }
}

impl SurfaceQuality {
    /// Write the surface's constituent state (§12.3).
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_fs, put_rows, put_u};
        let SurfaceQuality {
            // Parameters: unit factors the model builds.
            mass_cv: _,
            cv_area: _,
            cv_rain: _,
            cv_flow: _,
            // State.
            parcels,
            conc,
            runon_next,
            buildup_in,
            deposition,
            swept,
            infiltrated,
            bmp_removed,
            to_final,
            initial_buildup,
            washed_off,
            washed_by_parcel,
        } = self;
        put_u(w, parcels.len() as u64)?;
        for p in parcels {
            p.checkpoint_put(w)?;
        }
        for rows in [conc, runon_next, washed_by_parcel] {
            put_rows(w, rows)?;
        }
        for vs in [
            buildup_in,
            deposition,
            swept,
            infiltrated,
            bmp_removed,
            to_final,
            initial_buildup,
            washed_off,
        ] {
            put_fs(w, vs)?;
        }
        Ok(())
    }

    /// Read back what `checkpoint_put` wrote.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        let n = r.u()? as usize;
        if n != self.parcels.len() {
            return Err(format!(
                "checkpoint holds loading for {n} parcels where this model has {}",
                self.parcels.len()
            ));
        }
        for p in &mut self.parcels {
            p.checkpoint_get(r)?;
        }
        self.conc = r.rows()?;
        self.runon_next = r.rows()?;
        self.washed_by_parcel = r.rows()?;
        for slot in [
            &mut self.buildup_in,
            &mut self.deposition,
            &mut self.swept,
            &mut self.infiltrated,
            &mut self.bmp_removed,
            &mut self.to_final,
            &mut self.initial_buildup,
            &mut self.washed_off,
        ] {
            *slot = r.fs()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Washoff, WashoffForm};

    // §8.2 accumulation and §8.3 mobilisation, in closed form.
    //
    // This module had no test of its own. The one fixture that reaches it,
    // `buildup_washoff_treatment.inp`, is used only by checkpoint
    // round-trip tests, which assert a run resumes identically and stay
    // green with any formula here replaced by any other. Both washoff
    // exponents could be deleted, and the saturation form could read the
    // wrong coefficient column, with all 494 tests passing.

    fn buildup_of(form: BuildupForm, c0: f64, c1: f64, c2: f64) -> Buildup {
        Buildup {
            form,
            coeffs: [c0, c1, c2],
            normalizer: BuildupNormalizer::PerArea,
            series: None,
        }
    }

    /// §8.2's three accumulation forms at a known dry time.
    #[test]
    fn each_buildup_form_follows_its_own_curve() {
        // Power: min(B_max, K_B t^N_B) = min(50, 2 * 9^0.5) = 6.
        let power = buildup_of(BuildupForm::Power, 50.0, 2.0, 0.5);
        assert!((buildup_mass(&power, 9.0) - 6.0).abs() < 1e-12);
        // and it pins at B_max once the curve passes it.
        assert!((buildup_mass(&power, 1.0e9) - 50.0).abs() < 1e-9);

        // Exponential: B_max(1 - e^{-K_B t}).
        let expo = buildup_of(BuildupForm::Exponential, 50.0, 0.25, 0.0);
        let want = 50.0 * (1.0 - (-0.25f64 * 4.0).exp());
        assert!((buildup_mass(&expo, 4.0) - want).abs() < 1e-12);

        // Saturation: B_max t / (K_B + t), with the half-saturation time
        // in the THIRD column. §8.2 adopts that column convention from the
        // file format deliberately; reading the second gives 50*4/(0.25+4)
        // = 47.06 instead of 25, and nothing used to notice.
        let sat = buildup_of(BuildupForm::Saturation, 50.0, 0.25, 4.0);
        assert!(
            (buildup_mass(&sat, 4.0) - 25.0).abs() < 1e-12,
            "saturation must read K_B from the third column, got {}",
            buildup_mass(&sat, 4.0)
        );
    }

    /// The inversion §8.2 relies on: buildup's state is mass, so each dry
    /// step recovers the equivalent time before advancing.
    #[test]
    fn buildup_time_inverts_buildup_mass() {
        for b in [
            buildup_of(BuildupForm::Power, 50.0, 2.0, 0.5),
            buildup_of(BuildupForm::Saturation, 50.0, 0.25, 4.0),
        ] {
            let days = 3.75;
            let mass = buildup_mass(&b, days);
            assert!(
                (buildup_days(&b, mass) - days).abs() < 1e-9,
                "{:?}: {} d -> {} kg -> {} d",
                b.form,
                days,
                mass,
                buildup_days(&b, mass)
            );
        }
    }

    fn inputs() -> WashoffInputs {
        WashoffInputs {
            buildup: 10.0,
            share: 0.5,
            runoff_rate: 2.0e-6,
            v_outflow: 3.0,
            dt: 300.0,
            parcel_area: 4000.0,
            cv_rain: 1.0e-6,
            cv_flow: 1.0e-3,
        }
    }

    fn washoff_of(form: WashoffForm, coeff: f64, exponent: f64) -> Washoff {
        Washoff {
            form,
            coeff,
            exponent,
            sweep_efficiency: 0.0,
            bmp_efficiency: 0.0,
        }
    }

    /// §8.3 exponential: the load is a power law in the file-unit runoff
    /// rate, scaled by the mass on hand and the outflow share of runoff.
    #[test]
    fn exponential_washoff_is_a_power_law_in_the_runoff_rate() {
        let i = inputs();
        let w = washoff_of(WashoffForm::Exponential, 0.6, 1.5);
        let rate = i.runoff_rate / i.cv_rain; // 2.0 in file units
        let share = i.v_outflow / (i.runoff_rate * i.parcel_area * i.dt);
        let want = 0.6 / 3600.0 * rate.powf(1.5) * i.buildup * i.dt * share;
        assert!((washoff_load(&w, &i) - want).abs() < 1e-12);

        // The exponent has to bite: at rate 2.0 an exponent of 1 gives a
        // different answer, which is what deleting it used to do silently.
        let linear = washoff_of(WashoffForm::Exponential, 0.6, 1.0);
        assert!(
            (washoff_load(&w, &i) - washoff_load(&linear, &i)).abs() > 1e-9,
            "exponent 1.5 and exponent 1 must not agree here"
        );
    }

    /// §8.3 rating curve: a power law in the land-use share of the actual
    /// runoff flow, not in the rain.
    #[test]
    fn rating_curve_washoff_is_a_power_law_in_the_flow_share() {
        let i = inputs();
        let w = washoff_of(WashoffForm::RatingCurve, 0.8, 1.25);
        let q_share = i.share * (i.v_outflow / i.dt) / i.cv_flow;
        let want = 0.8 * q_share.powf(1.25) * i.dt / 1000.0;
        assert!((washoff_load(&w, &i) - want).abs() < 1e-12);

        let linear = washoff_of(WashoffForm::RatingCurve, 0.8, 1.0);
        assert!(
            (washoff_load(&w, &i) - washoff_load(&linear, &i)).abs() > 1e-9,
            "exponent 1.25 and exponent 1 must not agree here"
        );
    }

    /// §8.3 EMC: a concentration on the land-use share of outflow, with no
    /// dependence on buildup or on the exponent.
    #[test]
    fn event_mean_concentration_washoff_ignores_buildup_and_exponent() {
        let i = inputs();
        let w = washoff_of(WashoffForm::Emc, 12.0, 9.9);
        assert!((washoff_load(&w, &i) - 12.0 * 0.5 * 3.0).abs() < 1e-12);

        let mut dry = inputs();
        dry.buildup = 0.0;
        assert!((washoff_load(&w, &dry) - washoff_load(&w, &i)).abs() < 1e-12);
    }

    #[test]
    fn a_relation_with_no_form_mobilises_nothing() {
        assert_eq!(
            0.0,
            washoff_load(&washoff_of(WashoffForm::None, 5.0, 2.0), &inputs())
        );
    }
}
