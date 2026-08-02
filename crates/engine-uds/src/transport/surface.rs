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
                        None => land.buildup[ci].as_ref().map_or(0.0, |b| {
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
        }
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
        // Run-on booked last step arrives now.
        for (pi, state) in self.parcels.iter_mut().enumerate() {
            state.runon_mass = std::mem::replace(&mut self.runon_next[pi], vec![0.0; np]);
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
            for ci in 0..np {
                let c_out = if v_out1 > 0.0 && has_outflow {
                    outflow_load[ci] / v_out1
                } else {
                    0.0
                };
                // Control measures trim the outflow volume; the trimmed
                // share books as removal (§8.3).
                if q.v_out2 < v_out1 {
                    self.bmp_removed[ci] += c_out * (v_out1 - q.v_out2);
                }
                self.washed_off[ci] += c_out * q.v_out2;
                self.conc[pi][ci] = c_out;
            }

            // Loads to another parcel become its run-on next step (§8.3).
            if let ParcelOutlet::Parcel(target) = parcel.outlet {
                for ci in 0..np {
                    self.runon_next[target][ci] += self.conc[pi][ci] * q.v_out2;
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
                let mut load = match w.form {
                    WashoffForm::None => 0.0,
                    WashoffForm::Exponential => {
                        // Coefficient is per hour on the file rain-rate.
                        w.coeff / 3600.0
                            * (q.runoff_rate / self.cv_rain).powf(w.exponent)
                            * buildup
                            * q.dt
                            * (q.v_outflow / (q.runoff_rate * parcel.area * q.dt).max(1e-30))
                    }
                    WashoffForm::RatingCurve => {
                        // On the land-use share of flow, in file units;
                        // the load lands in concentration-mass per second.
                        let q_share = f * q.runoff_rate * parcel.area / self.cv_flow;
                        w.coeff * q_share.powf(w.exponent) * q.dt / 1000.0
                    }
                    WashoffForm::Emc => {
                        // Concentration on the land-use share of outflow.
                        w.coeff * f * q.v_outflow
                    }
                };
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
                // to final storage (§8.3).
                self.to_final[ci] += self.parcels[pi].ponded[ci];
                self.parcels[pi].ponded[ci] = 0.0;
                continue;
            }
            let mut mass =
                self.parcels[pi].ponded[ci] + rain_mass + self.parcels[pi].runon_mass[ci];
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
        _ => 0.0,
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
