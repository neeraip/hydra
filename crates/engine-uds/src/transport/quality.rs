//! Network transport (§8.4) with the §8.1 non-surface mass sources:
//! every channel and storage vertex a completely-mixed reactor updated by
//! the robust mixing form with exact exponential decay, links delivering
//! their previous-step concentration downstream, dry elements flushing to
//! final storage, and evaporation concentrating what remains.

use crate::hydraulics::routing::Router;
use crate::model::{Network, TreatmentKind};
use crate::simulation::expression::Expression;

/// The §8.4 dry-volume threshold (m³): one litre.
const ZERO_VOL: f64 = 1.0e-3;
/// The §8.4 dry-depth threshold (m): one millimetre.
const DRY_DEPTH: f64 = 1.0e-3;

/// Concentration state across the network, per constituent.
pub struct NetworkQuality {
    /// `[constituent][vertex]` concentration, in the declared unit.
    pub c_vertex: Vec<Vec<f64>>,
    /// `[constituent][channel slot]` concentration, slots parallel to
    /// the router's channel order.
    pub c_channel: Vec<Vec<f64>>,
    /// Mass flushed to final storage per constituent (unit·m³).
    pub final_storage: Vec<f64>,
    /// Mass discharged at outfalls and negative inflows (unit·m³).
    pub outfall_mass: Vec<f64>,
    /// Mass lost to first-order reaction (unit·m³).
    pub reacted: Vec<f64>,
    /// Mass carried out through bed seepage (unit·m³), §11.1.
    pub seepage_mass: Vec<f64>,
    /// Mass carried out with flooded volume (unit·m³), §11.1.
    pub flooded_mass: Vec<f64>,
    /// Mass admitted from §8.1 sources (unit·m³).
    pub inflow_mass: Vec<f64>,
    /// Mass present at the start (unit·m³), §11.1.
    pub initial_mass: Vec<f64>,
    /// §11.2 per-outfall discharged mass `[constituent][vertex]`.
    pub outfall_load: Vec<Vec<f64>>,
    /// Vertex volumes at the previous step (m³).
    vol_prev: Vec<f64>,
    /// Channel volumes at the previous step (m³).
    chan_vol_prev: Vec<f64>,
    /// §8.5 treatment: per vertex, the compiled (constituent,
    /// removal-kind, expression) set; empty for untreated vertices.
    treatments: Vec<Vec<(usize, bool, Expression)>>,
    /// §8.5 storage residence time per vertex (s).
    pub hrt: Vec<f64>,
    /// File-unit factors for the §8.5 process variables.
    cv_len: f64,
    cv_flow: f64,
}

impl NetworkQuality {
    /// Seed the state: initial concentrations only on elements wet at
    /// start (§8.4), and compile the §8.5 treatment expressions against
    /// the process-variable and constituent vocabulary.
    pub fn build(router: &Router, net: &Network) -> Result<NetworkQuality, String> {
        let np = net.constituents.len();
        let nv = net.vertices.len();
        let chans = router.channel_transport();
        let vol_prev: Vec<f64> = (0..nv).map(|v| router.vertex_volume_now(v)).collect();
        let chan_vol_prev: Vec<f64> = chans.iter().map(|c| c.4).collect();
        let c_vertex = net
            .constituents
            .iter()
            .map(|c| {
                (0..nv)
                    .map(|v| {
                        if vol_prev[v] > ZERO_VOL {
                            c.c_init
                        } else {
                            0.0
                        }
                    })
                    .collect()
            })
            .collect();
        let c_channel = net
            .constituents
            .iter()
            .map(|c| {
                chan_vol_prev
                    .iter()
                    .map(|&v| if v > ZERO_VOL { c.c_init } else { 0.0 })
                    .collect()
            })
            .collect();
        // §8.5 vocabulary: hrt dt flow depth area, then constituent
        // concentrations, then their removals as `r_<name>`.
        let mut treatments: Vec<Vec<(usize, bool, Expression)>> = vec![Vec::new(); nv];
        for t in &net.treatments {
            let resolve = |name: &str| -> Option<usize> {
                match name {
                    "hrt" => Some(0),
                    "dt" => Some(1),
                    "flow" => Some(2),
                    "depth" => Some(3),
                    "area" => Some(4),
                    _ => {
                        if let Some(r) = name.strip_prefix("r_") {
                            net.constituents
                                .iter()
                                .position(|c| c.id.eq_ignore_ascii_case(r))
                                .map(|ci| 5 + np + ci)
                        } else {
                            net.constituents
                                .iter()
                                .position(|c| c.id.eq_ignore_ascii_case(name))
                                .map(|ci| 5 + ci)
                        }
                    }
                }
            };
            let expr = Expression::compile(&t.expression, resolve).map_err(|e| {
                format!(
                    "{}: treatment of {}: {e}",
                    net.vertices[t.vertex].id, net.constituents[t.constituent].id
                )
            })?;
            treatments[t.vertex].push((t.constituent, t.kind == TreatmentKind::Removal, expr));
        }
        let us = net.options.flow_units.is_us();
        let c_vertex: Vec<Vec<f64>> = c_vertex;
        let c_channel: Vec<Vec<f64>> = c_channel;
        let initial_mass: Vec<f64> = (0..np)
            .map(|p| {
                let cv: &Vec<f64> = &c_vertex[p];
                let cc: &Vec<f64> = &c_channel[p];
                cv.iter().zip(&vol_prev).map(|(c, v)| c * v).sum::<f64>()
                    + cc.iter()
                        .zip(&chan_vol_prev)
                        .map(|(c, v)| c * v)
                        .sum::<f64>()
            })
            .collect();
        Ok(NetworkQuality {
            c_vertex,
            c_channel,
            final_storage: vec![0.0; np],
            outfall_mass: vec![0.0; np],
            reacted: vec![0.0; np],
            seepage_mass: vec![0.0; np],
            flooded_mass: vec![0.0; np],
            inflow_mass: vec![0.0; np],
            initial_mass,
            outfall_load: vec![vec![0.0; nv]; np],
            vol_prev,
            chan_vol_prev,
            treatments,
            hrt: vec![0.0; nv],
            cv_len: if us { 0.3048 } else { 1.0 },
            cv_flow: match net.options.flow_units {
                crate::io::options::FlowUnits::Cfs => 0.028_316_846_592,
                crate::io::options::FlowUnits::Gpm => 6.309_019_64e-5,
                crate::io::options::FlowUnits::Mgd => 0.043_812_636_4,
                crate::io::options::FlowUnits::Cms => 1.0,
                crate::io::options::FlowUnits::Lps => 1.0e-3,
                crate::io::options::FlowUnits::Mld => 1.0 / 86.4,
            },
        })
    }

    /// Advance one accepted routing step (§8.4): `lat_flow` the assembled
    /// lateral inflows (m³/s) and `lat_mass[p][v]` the §8.1 lateral mass
    /// rates (unit·m³/s) held for the step.
    pub fn update(
        &mut self,
        router: &Router,
        net: &Network,
        lat_flow: &[f64],
        lat_mass: &[Vec<f64>],
        dt: f64,
    ) {
        let chans = router.channel_transport();
        let structs = router.structure_transport();
        let nv = self.vol_prev.len();
        let vol_new: Vec<f64> = (0..nv).map(|v| router.vertex_volume_now(v)).collect();
        let np = net.constituents.len();
        // Influent concentrations and inflows retained for §8.5.
        let mut cin_all = vec![vec![0.0_f64; nv]; np];
        let mut flow_all = vec![0.0_f64; nv];

        for (p, constituent) in net.constituents.iter().enumerate() {
            let decay = (-constituent.decay * dt).exp();
            let mut mass_in = vec![0.0_f64; nv];
            let mut flow_in = vec![0.0_f64; nv];

            // Inflowing links deliver their previous-step concentration
            // to their downstream vertex (§8.1).
            for (k, &(_, from, to, q, _)) in chans.iter().enumerate() {
                if q > 0.0 {
                    mass_in[to] += q * self.c_channel[p][k] * dt;
                    flow_in[to] += q;
                } else if q < 0.0 {
                    mass_in[from] += -q * self.c_channel[p][k] * dt;
                    flow_in[from] += -q;
                }
            }
            // Volume-less structures pass their upstream vertex through.
            for &(_, from, to, q) in &structs {
                if q > 0.0 {
                    mass_in[to] += q * self.c_vertex[p][from] * dt;
                    flow_in[to] += q;
                } else if q < 0.0 {
                    mass_in[from] += -q * self.c_vertex[p][to] * dt;
                    flow_in[from] += -q;
                }
            }
            // §8.1 lateral sources; negative inflows are outflows and
            // remove mass at the vertex's concentration below. Mass
            // admits independently of flow — a flow-free mass load is
            // legal (§8.1) — clamped non-negative against the §7.8
            // inlet transfers.
            for v in 0..nv {
                if lat_flow[v] > 0.0 {
                    flow_in[v] += lat_flow[v];
                }
                let m = lat_mass[p][v] * dt;
                if m > 0.0 {
                    mass_in[v] += m;
                    self.inflow_mass[p] += m;
                }
            }

            // Vertex reactors (§8.4). A treatment expression overrides
            // the constituent's global decay at its vertex (§8.5).
            for v in 0..nv {
                let treated = self.treatments[v].iter().any(|(ci, _, _)| *ci == p);
                let decay = if treated { 1.0 } else { decay };
                if flow_in[v] > 0.0 {
                    cin_all[p][v] = mass_in[v] / (flow_in[v] * dt);
                }
                flow_all[v] = flow_in[v];
                if router.is_outfall(v) {
                    // Discharge leaves the system at the mixture.
                    let c = if flow_in[v] > 0.0 {
                        mass_in[v] / (flow_in[v] * dt)
                    } else {
                        0.0
                    };
                    self.outfall_mass[p] += mass_in[v];
                    self.outfall_load[p][v] += mass_in[v];
                    self.c_vertex[p][v] = c;
                    continue;
                }
                let c_old = self.c_vertex[p][v];
                let v_old = self.vol_prev[v];
                let mut c_new;
                if v_old <= ZERO_VOL {
                    // Negligible stored volume: the flow-weighted mixture.
                    c_new = if flow_in[v] > 0.0 {
                        mass_in[v] / (flow_in[v] * dt)
                    } else {
                        0.0
                    };
                } else {
                    // The robust mixing form with exact decay, clamped at
                    // the larger of the reactor and inflow concentrations.
                    let c_in = if flow_in[v] > 0.0 {
                        mass_in[v] / (flow_in[v] * dt)
                    } else {
                        0.0
                    };
                    self.reacted[p] += c_old * v_old * (1.0 - decay);
                    c_new = (c_old * v_old * decay + mass_in[v]) / (v_old + flow_in[v] * dt);
                    // The clamp bounds the mixture by its ingredients —
                    // but only when there is a flow to define c_in. A
                    // flow-free mass load (§8.1) legitimately raises the
                    // concentration above both, and clamping it against
                    // c_in = 0 would destroy the admitted mass.
                    if flow_in[v] > 0.0 {
                        c_new = c_new.min(c_old.max(c_in));
                    }
                }
                // Flooded volume leaves at the mixture concentration and
                // books to the §11.1 flooding account.
                let flood_vol = router.flood_rate(v) * dt;
                if flood_vol > 0.0 {
                    self.flooded_mass[p] += c_new * flood_vol;
                }
                // §8.4: storage-vertex losses mirror the channel rule —
                // seepage carries its volume's share out at the mixture,
                // evaporation concentrates what remains.
                let seep_vol = router.storage_seep_rates().get(v).copied().unwrap_or(0.0) * dt;
                if seep_vol > 0.0 {
                    self.seepage_mass[p] += c_new * seep_vol.min(v_old.max(0.0));
                }
                let v_evap = router.storage_evap_rates().get(v).copied().unwrap_or(0.0) * dt;
                if v_evap > 0.0 && vol_new[v] > ZERO_VOL {
                    c_new *= 1.0 + v_evap / vol_new[v];
                }
                // Below the dry thresholds with no inflow, remaining mass
                // flushes to final storage (§8.4).
                let depth = router.depth(v);
                if (vol_new[v] <= ZERO_VOL || depth <= DRY_DEPTH) && flow_in[v] <= 0.0 {
                    self.final_storage[p] += c_new * vol_new[v].max(0.0);
                    c_new = 0.0;
                }
                // A negative lateral books an outflow at the vertex's
                // concentration (§10.1).
                if lat_flow[v] < 0.0 {
                    self.outfall_mass[p] += -lat_flow[v] * dt * c_new;
                }
                self.c_vertex[p][v] = c_new;
            }
        }

        // §8.5 treatment at each treated vertex, then the channel pass
        // draws the treated concentrations.
        self.apply_treatment(router, net, &cin_all, &flow_all, dt);

        // Channel reactors (§8.4), drawing the just-updated upstream
        // vertex concentration.
        for (p, constituent) in net.constituents.iter().enumerate() {
            let decay = (-constituent.decay * dt).exp();
            for (k, &(li, from, to, q, v_new)) in chans.iter().enumerate() {
                let v_old = self.chan_vol_prev[k];
                let dry_depth = router.link_depth(li).unwrap_or(0.0) <= DRY_DEPTH;
                if v_new <= ZERO_VOL || dry_depth {
                    // Dry channels flush at either threshold (§8.4) —
                    // the *remaining* mass only: the delivery loop already
                    // sent this step's outflow share downstream at the
                    // start-of-step concentration.
                    let remaining = (v_old - q.abs() * dt).max(0.0);
                    self.final_storage[p] += self.c_channel[p][k] * remaining;
                    self.c_channel[p][k] = 0.0;
                    continue;
                }
                let upstream = if q >= 0.0 { from } else { to };
                let c_in = self.c_vertex[p][upstream];
                // Mixing inflow volume-adjusted for the storage change.
                let q_in_vol = q.abs() * dt + (v_new - v_old).max(0.0);
                let c_old = self.c_channel[p][k];
                self.reacted[p] += c_old * v_old * (1.0 - decay);
                let mut c_new = (c_old * v_old * decay + c_in * q_in_vol) / (v_old + q_in_vol);
                c_new = c_new.min(c_old.max(c_in));
                // §8.4: seepage carries its volume's share out at the
                // pre-evaporation concentration (evaporation concentrates
                // what stays; the seeping water left at the mixture), then
                // evaporation concentrates by the full 1 + V_evap/V — the
                // spec formula carries no cap, and capping it discarded
                // mass whenever evaporation exceeded the remaining volume.
                let seep_vol = router.channel_seep_rates().get(k).copied().unwrap_or(0.0) * dt;
                if seep_vol > 0.0 {
                    self.seepage_mass[p] += c_new * seep_vol.min(v_old.max(0.0));
                }
                let evap_rate = router.channel_evap_rates().get(k).copied().unwrap_or(0.0);
                let v_evap = evap_rate * dt;
                if v_evap > 0.0 && v_new > ZERO_VOL {
                    c_new *= 1.0 + v_evap / v_new;
                }
                self.c_channel[p][k] = c_new;
            }
        }

        self.vol_prev = vol_new;
        self.chan_vol_prev = chans.iter().map(|c| c.4).collect();
    }

    /// The §8.5 treatment pass: per treated vertex, evaluate removals —
    /// recursively for cross-references, guarded against cycles — and
    /// revise the vertex concentrations, booking the lost mass as
    /// reacted.
    #[allow(clippy::needless_range_loop)] // parallel per-constituent rows
    fn apply_treatment(
        &mut self,
        router: &Router,
        net: &Network,
        cin: &[Vec<f64>],
        flow_in: &[f64],
        dt: f64,
    ) {
        let np = net.constituents.len();
        for v in 0..self.vol_prev.len() {
            // Residence time advances at storage vertices (§8.5).
            if router.is_storage(v) {
                let vol = self.vol_prev[v];
                self.hrt[v] = if vol < ZERO_VOL {
                    0.0
                } else {
                    (self.hrt[v] + dt) * vol / (vol + flow_in[v] * dt)
                };
            }
            if self.treatments[v].is_empty() {
                continue;
            }
            let q = flow_in[v];
            let v_old = self.vol_prev[v];
            let depth = router.depth(v);
            let area = router.vertex_area_now(v);
            // Removal memo: NaN = not computed, ∞ = in progress.
            let mut removal = vec![f64::NAN; np];
            for p in 0..np {
                let has = self.treatments[v].iter().any(|(ci, _, _)| *ci == p);
                let kind_removal = self.treatments[v]
                    .iter()
                    .find(|(ci, _, _)| *ci == p)
                    .map(|(_, r, _)| *r);
                if !has {
                    removal[p] = 0.0;
                    continue;
                }
                // Removal-form yields zero without inflow (§8.5).
                if kind_removal == Some(true) && q <= 1e-12 {
                    removal[p] = 0.0;
                }
            }
            let vars_base = [
                self.hrt[v] / 3600.0,
                dt,
                q / self.cv_flow,
                depth / self.cv_len,
                area / (self.cv_len * self.cv_len),
            ];
            for p in 0..np {
                self.compute_removal(v, p, &vars_base, cin, &mut removal);
            }
            for p in 0..np {
                let r = removal[p];
                if r <= 0.0 || !r.is_finite() {
                    continue;
                }
                let kind_removal = self.treatments[v]
                    .iter()
                    .find(|(ci, _, _)| *ci == p)
                    .map(|(_, k, _)| *k);
                let c_mix = self.c_vertex[p][v];
                let c_out = match kind_removal {
                    Some(true) => {
                        // Applied to the influent, capped by the mixture.
                        if cin[p][v] == 0.0 {
                            c_mix
                        } else {
                            ((1.0 - r) * cin[p][v]).min(c_mix)
                        }
                    }
                    _ => (1.0 - r) * c_mix,
                };
                // The removed mass is the concentration drop over the
                // step's inflow-augmented pool (§8.5). The influent is
                // already inside `c_mix` — mixing precedes treatment — so
                // an influent term on top would overstate removal by
                // (c_in − c_mix)·Q·dt whenever storage dilutes the inflow.
                let lost = ((c_mix - c_out) * (v_old + q * dt)).max(0.0);
                self.reacted[p] += lost;
                // The vertex pass booked an outfall's discharge at the
                // untreated mixture before this pass ran; the discharge
                // must reflect treatment (§8.5), so move the removed mass
                // from the discharge accounts to the reaction account.
                if router.is_outfall(v) {
                    self.outfall_mass[p] -= lost;
                    self.outfall_load[p][v] -= lost;
                }
                self.c_vertex[p][v] = c_out;
            }
        }
    }

    /// Resolve constituent `p`'s removal at vertex `v`, memoised;
    /// recursion serves `r_<name>` references. A cycle — refused at
    /// validation — reads zero defensively.
    fn compute_removal(
        &self,
        v: usize,
        p: usize,
        vars_base: &[f64; 5],
        cin: &[Vec<f64>],
        removal: &mut [f64],
    ) {
        if !removal[p].is_nan() {
            return;
        }
        let Some((_, is_removal, expr)) = self.treatments[v]
            .iter()
            .find(|(ci, _, _)| *ci == p)
            .map(|(ci, r, e)| (*ci, *r, e.clone()))
        else {
            removal[p] = 0.0;
            return;
        };
        removal[p] = f64::INFINITY; // in progress
        let c0 = self.c_vertex[p][v];
        if c0 == 0.0 {
            removal[p] = 0.0;
            return;
        }
        let np = removal.len();
        let mut var = |i: usize| -> f64 {
            if i < 5 {
                vars_base[i]
            } else if i < 5 + np {
                let ci = i - 5;
                // A referenced constituent reads the combined influent
                // when its own equation here is removal-type or absent,
                // the mixture otherwise (§8.5).
                let ref_removal = self.treatments[v]
                    .iter()
                    .find(|(c, _, _)| *c == ci)
                    .map(|(_, r, _)| *r);
                match ref_removal {
                    Some(false) => self.c_vertex[ci][v],
                    _ => cin[ci][v],
                }
            } else {
                let ci = i - 5 - np;
                if removal[ci].is_infinite() {
                    // Cyclic reference: zero, defensively.
                    return 0.0;
                }
                if removal[ci].is_nan() {
                    self.compute_removal(v, ci, vars_base, cin, removal);
                }
                removal[ci]
            }
        };
        let (r, _) = expr.eval_by(&mut var);
        let r = r.max(0.0);
        removal[p] = if is_removal {
            r.min(1.0)
        } else {
            1.0 - r.min(c0) / c0
        };
    }

    /// Mass currently in the water (unit·m³) for constituent `p` (§11.1).
    pub fn stored_mass(&self, p: usize) -> f64 {
        self.c_vertex[p]
            .iter()
            .zip(&self.vol_prev)
            .map(|(c, v)| c * v)
            .sum::<f64>()
            + self.c_channel[p]
                .iter()
                .zip(&self.chan_vol_prev)
                .map(|(c, v)| c * v)
                .sum::<f64>()
    }

    /// The concentration reported for model link `li`: its channel state,
    /// or a structure's upstream-vertex pass-through.
    pub fn link_concentration(&self, router: &Router, p: usize, li: usize) -> Option<f64> {
        let chans = router.channel_transport();
        if let Some((k, _)) = chans.iter().enumerate().find(|(_, c)| c.0 == li) {
            return Some(self.c_channel[p][k]);
        }
        router
            .structure_transport()
            .iter()
            .find(|s| s.0 == li)
            .map(|&(_, from, to, q)| self.c_vertex[p][if q >= 0.0 { from } else { to }])
    }
}
