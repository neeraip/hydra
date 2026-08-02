//! Network transport (§8.4) with the §8.1 non-surface mass sources:
//! every channel and storage vertex a completely-mixed reactor updated by
//! the robust mixing form with exact exponential decay, links delivering
//! their previous-step concentration downstream, dry elements flushing to
//! final storage, and evaporation concentrating what remains.

use crate::hydraulics::routing::Router;
use crate::model::Network;

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
    /// Mass admitted from §8.1 sources (unit·m³).
    pub inflow_mass: Vec<f64>,
    /// Vertex volumes at the previous step (m³).
    vol_prev: Vec<f64>,
    /// Channel volumes at the previous step (m³).
    chan_vol_prev: Vec<f64>,
}

impl NetworkQuality {
    /// Seed the state: initial concentrations only on elements wet at
    /// start (§8.4).
    pub fn build(router: &Router, net: &Network) -> NetworkQuality {
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
        NetworkQuality {
            c_vertex,
            c_channel,
            final_storage: vec![0.0; np],
            outfall_mass: vec![0.0; np],
            reacted: vec![0.0; np],
            inflow_mass: vec![0.0; np],
            vol_prev,
            chan_vol_prev,
        }
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
            // remove mass at the vertex's concentration below.
            for v in 0..nv {
                if lat_flow[v] > 0.0 {
                    flow_in[v] += lat_flow[v];
                    mass_in[v] += lat_mass[p][v] * dt;
                    self.inflow_mass[p] += lat_mass[p][v] * dt;
                }
            }

            // Vertex reactors (§8.4).
            for v in 0..nv {
                if router.is_outfall(v) {
                    // Discharge leaves the system at the mixture.
                    let c = if flow_in[v] > 0.0 {
                        mass_in[v] / (flow_in[v] * dt)
                    } else {
                        0.0
                    };
                    self.outfall_mass[p] += mass_in[v];
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
                    c_new = c_new.min(c_old.max(c_in));
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

            // Channel reactors (§8.4), drawing the just-updated upstream
            // vertex concentration.
            for (k, &(_, from, to, q, v_new)) in chans.iter().enumerate() {
                let v_old = self.chan_vol_prev[k];
                if v_new <= ZERO_VOL {
                    // Dry channels flush unconditionally (§8.4).
                    self.final_storage[p] += self.c_channel[p][k] * v_old.max(0.0);
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
                self.c_channel[p][k] = c_new;
            }
        }

        self.vol_prev = vol_new;
        self.chan_vol_prev = chans.iter().map(|c| c.4).collect();
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
