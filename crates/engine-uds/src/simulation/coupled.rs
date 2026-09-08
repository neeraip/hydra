//! §15.6: the surface↔network co-advance.
//!
//! One struct owns the overland marcher and the per-period exchange
//! bookkeeping: the network advances a §10 routing period with the
//! surface frozen, the surface advances the same interval with the node
//! grades frozen, and the exchanged volumes queue as network lateral
//! inflow delivered over the next period. Everything here is SI.

use crate::hydraulics::routing::Router;
use crate::model::Network;
use crate::overland::marcher::Marcher;

/// A coupled node the marcher's slot order could not resolve against
/// the network's vertex identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownCouplingNode {
    pub node: String,
}

/// Why an overland mesh could not be attached to a session.
#[derive(Debug)]
pub enum AttachError {
    /// The mesh failed §15.2 validation.
    Mesh(Vec<crate::overland::MeshError>),
    /// A coupling row names a network node the model does not have.
    UnknownNode(String),
    /// A boundary row names a time series the model does not have.
    UnknownSeries(String),
    /// A boundary row names a curve the model does not have.
    UnknownCurve(String),
    /// A coupling point names an inlet design the model does not have.
    UnknownInlet(String),
    /// A coupling point's inlet cannot serve there (§15.6), and why.
    InletUnserved { design: String, why: String },
    /// Two inlets resolve to one cell and would capture one pond twice.
    InletsShareCell { cell: u32 },
    /// A runoff row names a parcel the model does not have.
    UnknownRunoffParcel(String),
    /// A surface-outlet parcel cannot be served (§15.7), and why.
    RunoffUnserved { parcel: String, why: String },
}

/// §15.6: the overland surface coupled to the routed network.
pub struct CoupledSurface {
    pub marcher: Marcher,
    /// Router vertex per marcher slot.
    slot_vertex: Vec<usize>,
    /// Exchanged volume awaiting delivery as next period's lateral
    /// (m³ per slot, positive = into the node).
    pending: Vec<f64>,
    /// §15.6: outfall discharge awaiting injection over the next batch
    /// (m³ per slot, positive = onto the surface).
    outfall_pending: Vec<f64>,
    /// §15.8 running totals of the exchange as DELIVERED to the network
    /// ledger: surface drainage in, surface spill drawn back out (m³).
    pub delivered_in: f64,
    pub delivered_out: f64,
    /// §14.16: per-point exchanged volume since the last reporting
    /// instant (m³).
    report_exchange: Vec<f64>,
    /// §15.11: exchanged mass awaiting delivery (unit·m³ per
    /// constituent per slot, positive = into the node).
    pending_mass: Vec<Vec<f64>>,
}

impl CoupledSurface {
    /// Resolve the marcher's coupled node names against the network and
    /// mark each router vertex coupled: its ponded area becomes the
    /// §15.6 footprint (the median-dual $\sum A_i/3$ of a vertex
    /// point's stencil; a cell point's whole cell), summed over the
    /// points naming it.
    pub fn new(
        marcher: Marcher,
        net: &Network,
        router: &mut Router,
    ) -> Result<CoupledSurface, UnknownCouplingNode> {
        let mut slot_vertex = Vec::with_capacity(marcher.coupling_nodes().len());
        for name in marcher.coupling_nodes() {
            let Some(vi) = net.vertices.iter().position(|v| &v.id == name) else {
                return Err(UnknownCouplingNode { node: name.clone() });
            };
            slot_vertex.push(vi);
        }
        let mut footprint = vec![0.0; slot_vertex.len()];
        for cp in marcher.coupling_points() {
            let a: f64 = cp.stencil.iter().map(|&ci| marcher.area[ci as usize]).sum();
            footprint[cp.node_slot as usize] += if cp.vertex.is_some() { a / 3.0 } else { a };
        }
        for (slot, &vi) in slot_vertex.iter().enumerate() {
            router.set_coupled(vi, footprint[slot]);
        }
        // §15.6: outfall coupling is asymmetric — mark those slots so
        // the marcher leaves them to the stage and injection paths.
        let mut marcher = marcher;
        for (slot, &vi) in slot_vertex.iter().enumerate() {
            if matches!(
                net.vertices[vi].kind,
                crate::model::VertexKind::Outfall { .. }
            ) {
                marcher.mark_outfall_slot(slot);
            }
        }
        // §15.4.5: the march takes the model's §6.4 worker width.
        #[cfg(feature = "threads")]
        marcher.set_width(net.options.threads as usize);
        let pending = vec![0.0; slot_vertex.len()];
        let outfall_pending = vec![0.0; slot_vertex.len()];
        Ok(CoupledSurface {
            marcher,
            slot_vertex,
            pending,
            outfall_pending,
            delivered_in: 0.0,
            delivered_out: 0.0,
            report_exchange: Vec::new(),
            pending_mass: Vec::new(),
        })
    }

    /// §15.6: the surface tailwater at each coupled outfall — the
    /// deepest wet stencil cell's surface, with the wetness ramp keyed
    /// on depth in excess of the drying depth — pushed to the router
    /// for the coming period's boundary evaluations.
    fn set_outfall_tailwaters(&self, router: &mut Router) {
        let m = &self.marcher;
        for (slot, &vi) in self.slot_vertex.iter().enumerate() {
            if !m.is_outfall_slot(slot) {
                continue;
            }
            let (mut h_2d, mut depth) = (f64::NEG_INFINITY, 0.0_f64);
            for cp in m.coupling_points() {
                if cp.node_slot as usize != slot {
                    continue;
                }
                for &ci in &cp.stencil {
                    let ci = ci as usize;
                    if m.depth[ci] > depth {
                        depth = m.depth[ci];
                        h_2d = m.eta[ci];
                    }
                }
            }
            let t = ((depth - m.dry_depth()) / m.dry_depth()).clamp(0.0, 1.0);
            let ramp = t * t * (3.0 - 2.0 * t);
            router.set_outfall_tailwater(vi, h_2d, ramp);
        }
    }

    /// §15.6: deliver the last period's exchanged volumes into this
    /// period's lateral vector as constant rates — positive exchange
    /// (surface drained into the node) is lateral inflow, a spill is
    /// its negative, drawing the node down by what it spilled.
    pub fn deliver_laterals(&mut self, lat: &mut [f64], plane: &mut [Vec<f64>], period: f64) {
        // §15.11: drained mass delivers on the surface plane; spilled
        // mass leaves the node through its negative lateral at the
        // node's own concentration, so nothing is delivered for it.
        for (p, row) in self.pending_mass.iter_mut().enumerate() {
            for (slot, m) in row.iter_mut().enumerate() {
                if *m > 0.0 {
                    if let Some(cell) = plane
                        .get_mut(p)
                        .and_then(|r| r.get_mut(self.slot_vertex[slot]))
                    {
                        *cell += *m / period;
                    }
                }
                *m = 0.0;
            }
        }
        for (slot, p) in self.pending.iter_mut().enumerate() {
            if *p != 0.0 {
                lat[self.slot_vertex[slot]] += *p / period;
                // §15.8: the exchange is its own named ledger pair on
                // the network side, never folded into external inflow.
                if *p > 0.0 {
                    self.delivered_in += *p;
                } else {
                    self.delivered_out += -*p;
                }
                *p = 0.0;
            }
        }
    }

    /// §15.6: co-advance the surface over the period the network just
    /// finished — node grades frozen at the router's current state —
    /// bank the exchange for next period's laterals, and refresh each
    /// coupled vertex's damping conductance for the coming period.
    pub fn co_advance(
        &mut self,
        router: &mut Router,
        quality: Option<&crate::transport::quality::NetworkQuality>,
        period: f64,
    ) {
        // §15.11: node concentrations freeze for the advance beside the
        // grades, before anything is injected at them.
        if let Some(q) = quality {
            for (slot, &vi) in self.slot_vertex.iter().enumerate() {
                for p in 0..self.marcher.constituents() {
                    self.marcher
                        .set_node_concentration(slot, p, q.c_vertex[p][vi]);
                }
            }
        }
        // §15.6: last batch's outfall discharge injects as a constant
        // rate over this batch, scattered down the surface slope.
        self.marcher.clear_injection();
        let points: Vec<(usize, usize)> = self
            .marcher
            .coupling_points()
            .iter()
            .enumerate()
            .map(|(k, cp)| (k, cp.node_slot as usize))
            .collect();
        for (slot, p) in self.outfall_pending.iter_mut().enumerate() {
            if *p == 0.0 {
                continue;
            }
            let mine: Vec<usize> = points
                .iter()
                .filter(|(_, s)| *s == slot)
                .map(|(k, _)| *k)
                .collect();
            let share = *p / period / mine.len().max(1) as f64;
            for k in mine {
                self.marcher.inject(k, share);
            }
            *p = 0.0;
        }
        for (slot, &vi) in self.slot_vertex.iter().enumerate() {
            if self.marcher.is_outfall_slot(slot) {
                continue;
            }
            let invert = router.vertex_invert(vi);
            let y = router.depth(vi);
            let rim = invert + router.vertex_max_depth(vi);
            self.marcher
                .set_node_drive(slot, invert + y, y, rim, router.vertex_volume_now(vi));
        }
        self.marcher.advance(period);
        if self.report_exchange.len() != self.marcher.exchanged().len() {
            self.report_exchange = vec![0.0; self.marcher.exchanged().len()];
        }
        for (k, &dv) in self.marcher.exchanged().iter().enumerate() {
            let slot = self.marcher.coupling_points()[k].node_slot as usize;
            self.pending[slot] += dv;
            self.report_exchange[k] += dv;
        }
        // §15.11: the exchanged mass banks beside the volume.
        let nq = self.marcher.constituents();
        if self.pending_mass.len() != nq {
            self.pending_mass = vec![vec![0.0; self.slot_vertex.len()]; nq];
        }
        for (p, row) in self.marcher.exchanged_mass().iter().enumerate() {
            for (k, &dm) in row.iter().enumerate() {
                let slot = self.marcher.coupling_points()[k].node_slot as usize;
                self.pending_mass[p][slot] += dm;
            }
        }
        self.refresh_conductances(router);
        // §15.6: bank each coupled outfall's net discharge over this
        // period for the next batch's injection. A withdrawal (the
        // surface pushing into the network through the boundary) is
        // capped against the β share of the stencil's frozen volume.
        for (slot, &vi) in self.slot_vertex.iter().enumerate() {
            if !self.marcher.is_outfall_slot(slot) {
                continue;
            }
            let mut vol = router.vertex_net_link_inflow(vi) * period;
            if vol < 0.0 {
                let held: f64 = self
                    .marcher
                    .coupling_points()
                    .iter()
                    .filter(|cp| cp.node_slot as usize == slot)
                    .flat_map(|cp| cp.stencil.iter())
                    .map(|&ci| self.marcher.vol[ci as usize].max(0.0))
                    .sum();
                vol = vol.max(-0.8 * held);
            }
            self.outfall_pending[slot] += vol;
        }
        // The surface the network sees next period.
        self.set_outfall_tailwaters(router);
    }

    /// §6.4 damping for the coming period: the summed conductance of
    /// every point naming the vertex, against the live surface.
    fn refresh_conductances(&self, router: &mut Router) {
        let mut g = vec![0.0; self.slot_vertex.len()];
        for k in 0..self.marcher.coupling_points().len() {
            let slot = self.marcher.coupling_points()[k].node_slot as usize;
            if !self.marcher.is_outfall_slot(slot) {
                g[slot] += self.marcher.coupling_conductance(k);
            }
        }
        for (slot, &vi) in self.slot_vertex.iter().enumerate() {
            router.set_coupling_conductance(vi, g[slot]);
        }
    }

    /// §12.3: re-derive what a checkpoint does not carry because the
    /// restored halves already hold it — the marcher's node drives from
    /// the restored router, and the router's damping conductances and
    /// outfall tailwaters from the restored surface — exactly the
    /// values the interrupted run's last co-advance left in place.
    pub fn resync_network(&mut self, router: &mut Router) {
        for (slot, &vi) in self.slot_vertex.iter().enumerate() {
            if self.marcher.is_outfall_slot(slot) {
                continue;
            }
            let invert = router.vertex_invert(vi);
            let y = router.depth(vi);
            let rim = invert + router.vertex_max_depth(vi);
            self.marcher
                .set_node_drive(slot, invert + y, y, rim, router.vertex_volume_now(vi));
        }
        self.refresh_conductances(router);
        self.set_outfall_tailwaters(router);
    }

    /// §12.3: write the overland state — the marcher's physical and
    /// cadence state, the §15.8 ledger and march counters, and this
    /// struct's exchange bookkeeping — in the checkpoint's framing.
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint as cp;
        let m = &self.marcher;
        for vs in [&m.vol, &m.q, &m.facc_l, &m.facc_r, &m.qcx, &m.qcy] {
            cp::put_fs(w, vs)?;
        }
        cp::put_fs(w, &m.boundary_discharges())?;
        cp::put_u(w, m.active.len() as u64)?;
        for &on in &m.active {
            cp::put_b(w, on)?;
        }
        for &t in &m.tier {
            cp::put_u(w, u64::from(t))?;
        }
        cp::put_fs(w, &m.il_left)?;
        for v in [
            m.lazy_owed,
            m.storage0,
            m.min_dt0,
            m.advanced,
            m.rain_in,
            m.evap_out,
            m.infiltration_out,
            m.boundary_in,
            m.boundary_out,
            m.coupling_in,
            m.coupling_out,
            m.outfall_in,
            m.outfall_out,
            m.runoff_in,
        ] {
            cp::put_f(w, v)?;
        }
        for v in [m.macro_cycles, m.substeps, m.rebuilds, m.peak_active as u64] {
            cp::put_u(w, v)?;
        }
        for vs in [&self.pending, &self.outfall_pending, &self.report_exchange] {
            cp::put_fs(w, vs)?;
        }
        cp::put_f(w, self.delivered_in)?;
        cp::put_f(w, self.delivered_out)?;
        // §15.11 state: the banked mass and the marcher's layer.
        cp::put_u(w, self.pending_mass.len() as u64)?;
        for row in &self.pending_mass {
            cp::put_fs(w, row)?;
        }
        m.checkpoint_put_quality(w)
    }

    /// §12.3: restore what [`CoupledSurface::checkpoint_put`] wrote,
    /// then rebuild everything derivable. Sizes are checked against the
    /// attached mesh — the fingerprint upstream should have refused a
    /// mismatch already, so a failure here is a corrupt file.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        let m = &mut self.marcher;
        let (nc, nf, nb) = (m.vol.len(), m.q.len(), m.boundary_discharges().len());
        let vol = r.fs()?;
        let q = r.fs()?;
        let fl = r.fs()?;
        let fr = r.fs()?;
        let qcx = r.fs()?;
        let qcy = r.fs()?;
        let bq = r.fs()?;
        if vol.len() != nc
            || q.len() != nf
            || fl.len() != nf
            || fr.len() != nf
            || qcx.len() != nc
            || qcy.len() != nc
            || bq.len() != nb
        {
            return Err("checkpoint overland state does not fit this mesh".into());
        }
        m.vol = vol;
        m.q = q;
        m.facc_l = fl;
        m.facc_r = fr;
        m.qcx = qcx;
        m.qcy = qcy;
        m.set_boundary_discharges(&bq);
        let n = r.u()? as usize;
        if n != nc {
            return Err("checkpoint overland state does not fit this mesh".into());
        }
        for ci in 0..nc {
            m.active[ci] = r.b()?;
        }
        for ci in 0..nc {
            m.tier[ci] = u8::try_from(r.u()?).map_err(|_| "overland tier out of range")?;
        }
        let il_left = r.fs()?;
        if il_left.len() != nc {
            return Err("checkpoint overland state does not fit this mesh".into());
        }
        m.il_left = il_left;
        for slot in [
            &mut m.lazy_owed,
            &mut m.storage0,
            &mut m.min_dt0,
            &mut m.advanced,
            &mut m.rain_in,
            &mut m.evap_out,
            &mut m.infiltration_out,
            &mut m.boundary_in,
            &mut m.boundary_out,
            &mut m.coupling_in,
            &mut m.coupling_out,
            &mut m.outfall_in,
            &mut m.outfall_out,
            &mut m.runoff_in,
        ] {
            *slot = r.f()?;
        }
        m.macro_cycles = r.u()?;
        m.substeps = r.u()?;
        m.rebuilds = r.u()?;
        m.peak_active = r.u()? as usize;
        m.rebuild_after_restore();
        let pending = r.fs()?;
        let outfall_pending = r.fs()?;
        let report_exchange = r.fs()?;
        if pending.len() != self.pending.len()
            || outfall_pending.len() != self.outfall_pending.len()
        {
            return Err("checkpoint overland state does not fit this mesh".into());
        }
        self.pending = pending;
        self.outfall_pending = outfall_pending;
        self.report_exchange = report_exchange;
        self.delivered_in = r.f()?;
        self.delivered_out = r.f()?;
        let n = r.u()? as usize;
        self.pending_mass = (0..n).map(|_| r.fs()).collect::<Result<_, _>>()?;
        self.marcher.checkpoint_get_quality(r)
    }

    /// §14.16: the per-point exchanged volumes since the last take
    /// (m³); taking resets the accumulator.
    pub fn take_report_exchange(&mut self) -> Vec<f64> {
        if self.report_exchange.is_empty() {
            self.report_exchange = vec![0.0; self.marcher.coupling_points().len()];
        }
        std::mem::replace(
            &mut self.report_exchange,
            vec![0.0; self.marcher.coupling_points().len()],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::objects::parse_network;
    use crate::model::validate::validate;
    use crate::overland::{CouplingRow, MeshCell, MeshVertex, OverlandMesh, Topology};

    const NET: &str = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.0  2.0

[OUTFALLS]
O1  99.0  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0
";

    fn network() -> (Network, Router) {
        let (mut net, diags) = parse_network(NET);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        let v = validate(&mut net);
        assert!(v.iter().all(|f| !f.kind.is_error()), "{v:?}");
        let router = Router::build(&net).expect("router");
        (net, router)
    }

    /// A flat pond at the junction's ground elevation, coupled at one
    /// cell.
    fn pond_mesh(h0: f64, node: &str) -> OverlandMesh {
        let mut mesh = OverlandMesh::default();
        let (nx, ny, dx) = (4usize, 4usize, 1.0);
        let nvx = nx + 1;
        for j in 0..=ny {
            for i in 0..=nx {
                mesh.verts.push(MeshVertex {
                    x: dx * i as f64,
                    y: dx * j as f64,
                    z: 102.0,
                    tag: None,
                });
            }
        }
        for j in 0..ny {
            for i in 0..nx {
                let v00 = (j * nvx + i) as u32;
                let v10 = (j * nvx + i + 1) as u32;
                let v01 = ((j + 1) * nvx + i) as u32;
                let v11 = ((j + 1) * nvx + i + 1) as u32;
                for tri in [[v00, v10, v11], [v00, v11, v01]] {
                    mesh.cells.push(MeshCell {
                        v: tri,
                        n: 0.05,
                        h0,
                        tag: None,
                    });
                }
            }
        }
        mesh.cell_couplings.push(CouplingRow {
            address: "0".into(),
            node: node.into(),
            cd: 0.65,
            area: 0.05,
            area_authored: true,
            inlet: None,
        });
        // §15.7 losses on one cell, so the checkpoint and report gates
        // exercise the infiltration state alongside everything else.
        mesh.infiltration.push(crate::overland::InfiltrationRow {
            address: "5".into(),
            il: 0.02,
            cl: 2.5e-6,
        });
        mesh
    }

    fn marcher_of(mesh: &OverlandMesh) -> Marcher {
        let topo = Topology::build(mesh).expect("valid mesh");
        Marcher::build(mesh, &topo)
    }

    /// §15.7: rain reaches the mesh from the model's gages, and a
    /// rain-on-mesh model is one whose parcels do *not* capture the
    /// storm — most often because it has none at all.
    ///
    /// The surface compartment used to return nothing for a model
    /// without parcels or RDII, so the gages were never read and a whole
    /// run fell on a dry mesh with no diagnostic to say so.
    #[test]
    fn rain_reaches_a_mesh_in_a_model_with_no_parcels() {
        const INP: &str = "\
[OPTIONS]
FLOW_UNITS    CMS
FLOW_ROUTING  DYNWAVE
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      00:20
ROUTING_STEP  5
REPORT_STEP   0:05:00

[RAINGAGES]
RG1  INTENSITY  0:05  1.0  TIMESERIES  TS1

[TIMESERIES]
TS1  0:00  50.0
TS1  0:20  50.0

[JUNCTIONS]
J1  100.0  2.0

[OUTFALLS]
O1  99.0  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0

[2D_OPTIONS]
RAINFALL_MODE  SYSTEM

[2D_VERTICES]
0  0  102.0
1  0  102.0
1  1  102.0
0  1  102.0

[2D_TRIANGLES]
0  1  2  0.05
0  2  3  0.05

[2D_VERTEX_NODE_MAP]
0  J1  0.65  0.05
";
        let (mut sim, diags, _) = crate::dialect::session::open(INP).expect("open");
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "{:?}",
            diags
                .iter()
                .filter(|d| d.kind.is_error())
                .collect::<Vec<_>>()
        );
        sim.run();

        let rpt = sim.report_inputs();
        let overland = rpt.overland.expect("the run carries a surface");
        // 50 mm/h over 20 minutes is ~16.7 mm on every cell; what
        // matters here is that it is not zero, and that the ledger
        // closes around it.
        assert!(
            overland.ledger.rain_in > 0.0,
            "no rain reached the mesh: {:?}",
            overland.ledger
        );
        assert!(overland.ledger.error.abs() < 1e-6, "ledger closes");
    }

    /// An unknown coupled node name is a refusal, not a silent skip.
    #[test]
    fn an_unknown_coupling_node_refuses() {
        let (net, mut router) = network();
        let m = marcher_of(&pond_mesh(0.3, "NOPE"));
        let err = CoupledSurface::new(m, &net, &mut router)
            .err()
            .expect("refusal");
        assert_eq!(err.node, "NOPE");
    }

    /// §15.6 end to end: a surface pond drains through the coupling,
    /// arrives as next-period lateral inflow, and leaves through the
    /// outfall — every exchanged litre accounted on both sides.
    #[test]
    fn a_pond_drains_into_the_network_and_out_the_outfall() {
        let (net, mut router) = network();
        let m = marcher_of(&pond_mesh(0.3, "J1"));
        let mut cs = CoupledSurface::new(m, &net, &mut router).expect("resolves");
        let v0 = cs.marcher.storage();
        assert!(v0 > 4.7 && v0 < 4.9, "pond holds {v0}");

        let nv = 2;
        let period = 5.0;
        let mut delivered = 0.0;
        let mut t = 0.0;
        for _ in 0..240 {
            let mut lat = vec![0.0; nv];
            cs.deliver_laterals(&mut lat, &mut [], period);
            delivered += lat[0] * period;
            t += period;
            router.advance(t, &move |_tt, l: &mut [f64]| l.copy_from_slice(&lat));
            cs.co_advance(&mut router, None, period);
        }

        // The pond drained through the orifice.
        assert!(
            cs.marcher.storage() < 0.05 * v0,
            "pond still holds {}",
            cs.marcher.storage()
        );
        // Surface ledger: what left the surface is what the coupling
        // and the §15.7 losses booked.
        let drained = cs.marcher.coupling_out - cs.marcher.coupling_in;
        assert!(
            (v0 - cs.marcher.storage() - drained - cs.marcher.infiltration_out).abs() < 1e-9,
            "surface ledger"
        );
        assert!(cs.marcher.infiltration_out > 0.0, "the losses engaged");
        // Delivery: everything banked was delivered (the last period's
        // pending may still be in flight).
        assert!(
            (delivered - drained).abs() <= cs.pending.iter().map(|p| p.abs()).sum::<f64>() + 1e-9,
            "delivered {delivered} vs drained {drained}"
        );
        // Network ledger: the inflow arrived and left through the
        // outfall (5% closure over a transient run).
        let led = &router.report;
        assert!(
            (led.inflow - delivered).abs() < 1e-6,
            "router saw {} of {delivered}",
            led.inflow
        );
        assert!(
            (led.inflow - led.outflow).abs() < 0.05 * led.inflow.max(1e-9),
            "in {} out {}",
            led.inflow,
            led.outflow
        );
    }

    /// A pond mesh whose bed sits at the outfall's ground: the same
    /// helper geometry, rebased.
    fn low_mesh(h0: f64, node: &str) -> OverlandMesh {
        let mut mesh = pond_mesh(h0, node);
        for v in &mut mesh.verts {
            v.z = 99.0;
        }
        mesh
    }

    /// §15.6: a coupled outfall's discharge injects onto the surface
    /// over the following batch, and the surface books every litre.
    #[test]
    fn an_outfall_discharges_onto_the_surface() {
        let (net, mut router) = network();
        let m = marcher_of(&low_mesh(0.0, "O1"));
        let mut cs = CoupledSurface::new(m, &net, &mut router).expect("resolves");
        let period = 5.0;
        let mut t = 0.0;
        for _ in 0..120 {
            let mut lat = vec![0.0; 2];
            cs.deliver_laterals(&mut lat, &mut [], period);
            lat[0] += 0.05;
            t += period;
            router.advance(t, &move |_tt, l: &mut [f64]| l.copy_from_slice(&lat));
            cs.co_advance(&mut router, None, period);
        }
        assert!(
            cs.marcher.outfall_in > 0.0,
            "the outfall never reached the surface"
        );
        // §15.8: the injection path books apart from the junction
        // exchange, and the surface holds what it says.
        assert_eq!(cs.marcher.coupling_in, 0.0);
        assert!(
            (cs.marcher.storage() + cs.marcher.infiltration_out - cs.marcher.outfall_in).abs()
                < 1e-9,
            "surface {} + losses {} vs injected {}",
            cs.marcher.storage(),
            cs.marcher.infiltration_out,
            cs.marcher.outfall_in
        );
        // And what was injected tracks what the network discharged
        // (one batch may still be pending).
        let led = &router.report;
        assert!(
            (led.outflow - cs.marcher.outfall_in).abs()
                < 0.1 * led.outflow + cs.outfall_pending[0].abs() + 1e-9,
            "network discharged {} but the surface got {}",
            led.outflow,
            cs.marcher.outfall_in
        );
    }

    /// §15.6: a flooded surface over a coupled outfall sets its
    /// boundary stage — the network sees the pond as tailwater.
    #[test]
    fn a_flooded_surface_sets_the_outfall_tailwater() {
        let (net, mut router) = network();
        let m = marcher_of(&low_mesh(1.0, "O1"));
        let mut cs = CoupledSurface::new(m, &net, &mut router).expect("resolves");
        // One co-advance publishes the tailwater; the next network
        // period evaluates its boundary against it.
        cs.co_advance(&mut router, None, 5.0);
        let lat = vec![0.05, 0.0];
        router.advance(5.0, &move |_tt, l: &mut [f64]| l.copy_from_slice(&lat));
        // O1 (vertex 1, invert 99) reads the pond: depth ≈ 1 m.
        assert!(
            router.depth(1) > 0.9,
            "outfall depth {} ignores the pond",
            router.depth(1)
        );
    }

    /// §15.6 through the whole session: a mesh attached to a live
    /// `Simulation` drains its pond into the network across `step()`
    /// periods — delivery, damping, checkpoint refusal and all.
    #[test]
    fn a_session_runs_the_coupled_surface_through_its_steps() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      00:20
ROUTING_STEP  5
REPORT_STEP   0:05:00

[JUNCTIONS]
J1  100.0  2.0

[OUTFALLS]
O1  99.0  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0
";
        let (mut sim, _, findings) = crate::dialect::session::open(inp).expect("open");
        assert!(findings.iter().all(|f| !f.kind.is_error()));
        sim.attach_overland(pond_mesh(0.3, "J1")).expect("attach");
        let v0 = sim.overland().expect("attached").marcher.storage();

        sim.run();
        let m = &sim.overland().expect("attached").marcher;
        assert!(
            m.storage() < 0.2 * v0,
            "pond still holds {} of {v0}",
            m.storage()
        );
        assert!(
            (v0 - m.storage() - (m.coupling_out - m.coupling_in) - m.infiltration_out).abs() < 1e-9,
            "surface ledger"
        );
    }

    /// §15.9 determinism: SI and US authorings of one physical model
    /// agree to round-trip precision. The mesh is SI either way
    /// (§14.15); only the network sections convert.
    #[test]
    fn si_and_us_authorings_agree() {
        let run = |inp: &str| {
            let (mut sim, _, findings) = crate::dialect::session::open(inp).expect("open");
            assert!(findings.iter().all(|f| !f.kind.is_error()), "{findings:?}");
            sim.attach_overland(pond_mesh(0.3, "J1")).expect("attach");
            sim.run();
            let m = &sim.overland().expect("attached").marcher;
            (m.storage(), m.coupling_out)
        };
        let si = run("\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      00:20
ROUTING_STEP  5
REPORT_STEP   0:05:00

[JUNCTIONS]
J1  100.0  2.0

[OUTFALLS]
O1  99.0  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0
");
        // The same model in feet: every length is the exact conversion
        // of the metric authoring.
        let us = run("\
[OPTIONS]
FLOW_UNITS    CFS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      00:20
ROUTING_STEP  5
REPORT_STEP   0:05:00

[JUNCTIONS]
J1  328.0839895013123  6.561679790026246

[OUTFALLS]
O1  324.8031496062992  FREE

[CONDUITS]
C1  J1  O1  328.0839895013123  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.6404199475065617  0  0  0
");
        assert!(
            (si.0 - us.0).abs() <= 1e-6 * si.0.max(1e-9),
            "storage: SI {} vs US {}",
            si.0,
            us.0
        );
        assert!(
            (si.1 - us.1).abs() <= 1e-6 * si.1.max(1e-9),
            "drained: SI {} vs US {}",
            si.1,
            us.1
        );
    }

    /// §14.16 round trip through a live session: the sidecar streams at
    /// the reporting instants, the reader validates and serves it back,
    /// and the §14.9 report carries the overland blocks.
    #[test]
    fn the_overland_results_stream_round_trips() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      00:20
ROUTING_STEP  5
REPORT_STEP   0:05:00

[JUNCTIONS]
J1  100.0  2.0

[OUTFALLS]
O1  99.0  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0
";
        let (mut sim, _, _) = crate::dialect::session::open(inp).expect("open");
        sim.attach_overland(pond_mesh(0.3, "J1")).expect("attach");

        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "hydra-uds-overland-{}-{}.h2o",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        let sink = Box::new(std::fs::File::create(&path).expect("create"));
        crate::dialect::session::begin_overland_results(&mut sim, sink).expect("begin");
        sim.run();
        sim.finish_results().expect("finish");

        let r = crate::dialect::out_reader::OverlandResults::open(&path).expect("open results");
        assert_eq!(r.periods, 4, "20 min at 5-min reporting");
        assert_eq!(r.cells.len(), 32);
        assert_eq!(r.verts.len(), 25);
        assert_eq!(r.point_cells, [0]);
        assert!((r.report_step - 300.0).abs() < 1e-9);
        // The pond drains across the records: depth at the coupled cell
        // falls, the exchange rate is a drain, the ledger closes.
        let first = r.record(0).expect("first");
        let last = r.record(3).expect("last");
        assert!(last.t > first.t);
        assert!(
            f64::from(last.cells[0][0]) < f64::from(first.cells[0][0]),
            "depth must fall as the pond drains"
        );
        assert!(first.exchange[0] > 0.0, "the point drains into the node");
        assert!(last.ledger.junction_out > 0.0);
        assert!(last.ledger.error.abs() < 1e-6, "ledger closes");
        // A cell series is the records' own values re-cut.
        let series = r.cell_series(0).expect("series");
        assert_eq!(series.len(), 4);
        assert_eq!(series[0].1, first.cells[0]);
        assert_eq!(series[3].1, last.cells[0]);
        // A torso without its epilog is refused as unfinished.
        let bytes = std::fs::read(&path).expect("read");
        std::fs::write(&path, &bytes[..bytes.len() - 8]).expect("truncate");
        let err = crate::dialect::out_reader::OverlandResults::open(&path).unwrap_err();
        assert!(err.contains("did not finish"), "{err}");
        std::fs::remove_file(&path).ok();

        // §14.9: the report carries the overland blocks and the §15.8
        // named pair.
        let mut rpt = Vec::new();
        crate::dialect::session::write_report(&sim, &mut rpt).expect("report");
        let rpt = String::from_utf8(rpt).expect("utf8");
        for needle in [
            "Overland Flow Continuity",
            "Infiltration",
            "Junction Drainage",
            "Surface Drainage",
            "Surface Spill",
            "Runoff Onto Surface",
            "Overland Time Step Summary",
            "Peak Active Cells",
        ] {
            assert!(rpt.contains(needle), "report lacks {needle}");
        }
    }

    /// One parcel under a steady storm, draining to J1 by default.
    const PARCEL_INP: &str = "\
[OPTIONS]
FLOW_UNITS    CMS
FLOW_ROUTING  DYNWAVE
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      00:20
ROUTING_STEP  5
REPORT_STEP   0:05:00
WET_STEP      0:01:00

[RAINGAGES]
RG1  INTENSITY  0:05  1.0  TIMESERIES  TS1

[TIMESERIES]
TS1  0:00  50.0
TS1  0:20  50.0

[SUBCATCHMENTS]
S1  RG1  J1  1  100  100  0.5  0

[SUBAREAS]
S1  0.01  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0

[JUNCTIONS]
J1  100.0  2.0

[OUTFALLS]
O1  99.0  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0
";

    /// The pond helper as a dry, rain-free mesh with S1's runoff mapped
    /// onto cell 5 (when `surface` is set), sunk twelve metres below
    /// J1's rim so a hectare's runoff on sixteen square metres still
    /// stands below the rim gate and nothing exchanges with the network.
    fn runoff_model(surface: bool) -> Network {
        runoff_model_from(PARCEL_INP, surface)
    }

    fn runoff_model_from(inp: &str, surface: bool) -> Network {
        let (mut net, diags) = parse_network(inp);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        let mut mesh = sub_rim_mesh(0.0);
        for v in &mut mesh.verts {
            v.z = 90.0;
        }
        mesh.infiltration.clear();
        mesh.options.rainfall_mode = crate::overland::RainfallMode::None;
        if surface {
            net.parcels[0].outlet = crate::model::ParcelOutlet::Surface;
            mesh.runoff_map.push(crate::overland::RunoffRow {
                parcel: "S1".into(),
                kind: crate::overland::SurfaceKind::Cell,
                address: "5".into(),
            });
        }
        net.overland = Some(mesh);
        net
    }

    fn run_network(net: Network) -> crate::simulation::engine::Simulation {
        let (mut sim, findings) =
            crate::simulation::engine::Simulation::from_network(net, Vec::new(), Vec::new(), None)
                .expect("open");
        assert!(findings.iter().all(|f| !f.kind.is_error()), "{findings:?}");
        sim.run();
        sim
    }

    /// §15.7: a surface-outlet parcel's runoff lands on the mesh and
    /// books as runoff in; the same parcel draining to its node puts
    /// nothing on the mesh. The surface holds what it was given.
    #[test]
    fn a_surface_outlet_parcel_puts_its_runoff_on_the_mesh() {
        let control = run_network(runoff_model(false));
        let m = &control.overland().expect("mesh").marcher;
        assert_eq!(m.runoff_in, 0.0, "a node-bound parcel reaches no cell");
        assert_eq!(m.storage(), 0.0);

        let sim = run_network(runoff_model(true));
        let m = &sim.overland().expect("mesh").marcher;
        assert!(m.runoff_in > 0.0, "the parcel never reached the mesh");
        assert_eq!(m.coupling_out, 0.0, "the rim gate stayed shut");
        assert!(
            (m.storage() - m.runoff_in).abs() < 1e-9,
            "held {} of {} delivered",
            m.storage(),
            m.runoff_in
        );
        assert!(m.ledger_error().abs() < 1e-9);
        // The parcel's own continuity still calls it runoff: over 20
        // minutes of 50 mm/h on a hectare that is a few cubic metres
        // once the initial abstraction is filled.
        assert!(m.runoff_in > 1.0, "only {} m³ arrived", m.runoff_in);
    }

    /// §12.3 with a surface outlet: the runoff bound for the mesh is
    /// state, read by every co-advance until hydrology steps again, so
    /// a run checkpointed between two hydrology steps resumes
    /// bit-identically only if the checkpoint carries it.
    #[test]
    fn a_surface_outlet_run_resumes_bit_identically_from_a_checkpoint() {
        let open = |net: Network| {
            crate::simulation::engine::Simulation::from_network(net, Vec::new(), Vec::new(), None)
                .expect("open")
                .0
        };
        let mut whole = open(runoff_model(true));
        whole.run();

        let mut first = open(runoff_model(true));
        // 100 routing steps of 5 s: not on the one-minute hydrology
        // clock, so the resumed run's next eight co-advances read the
        // carried runoff before hydrology steps again.
        for _ in 0..100 {
            first.step();
        }
        let mut held = Vec::new();
        first.save_checkpoint(&mut held).expect("save");
        let mut resumed = open(runoff_model(true));
        resumed.load_checkpoint(&held).expect("load");
        resumed.run();

        let a = &whole.overland().expect("mesh").marcher;
        let b = &resumed.overland().expect("mesh").marcher;
        assert!(a.runoff_in > 0.0);
        assert_eq!(
            a.runoff_in.to_bits(),
            b.runoff_in.to_bits(),
            "runoff ledger"
        );
        let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&a.vol), bits(&b.vol), "volumes");
    }

    /// The parcel model with a pollutant carried in the rain, so the
    /// runoff has a wash-off concentration to put on the mesh.
    fn polluted_inp() -> String {
        format!("{PARCEL_INP}\n[POLLUTANTS]\nTSS  MG/L  10  0  0  0  NO\n")
    }

    /// §15.11 through the session: a polluted surface-outlet parcel
    /// puts its wash-off on the mesh, the pond drains into the node,
    /// and the mass arrives in the network as its own origin. Every
    /// ledger closes, the report names the new blocks, and the sidecar
    /// carries the concentrations.
    #[test]
    fn wash_off_rides_the_runoff_onto_the_mesh_and_into_the_network() {
        // The mesh at J1's rim, so the pond drains through the coupling.
        let mut net = runoff_model_from(&polluted_inp(), true);
        for v in &mut net.overland.as_mut().expect("mesh").verts {
            v.z = 102.0;
        }
        let (mut sim, findings) =
            crate::simulation::engine::Simulation::from_network(net, Vec::new(), Vec::new(), None)
                .expect("open");
        assert!(findings.iter().all(|f| !f.kind.is_error()), "{findings:?}");
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "hydra-uds-overland-q-{}-{}.h2o",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        let sink = Box::new(std::fs::File::create(&path).expect("create"));
        sim.attach_overland_results(Box::new(
            crate::dialect::overland_out::OverlandStream::begin(
                sink as Box<dyn std::io::Write + Send>,
                sim.network().overland.as_ref().expect("mesh"),
                &sim.overland().expect("mesh").marcher,
                0.0,
                300.0,
                0.0,
            )
            .expect("begin"),
        ))
        .expect("attach sink");
        sim.run();
        sim.finish_results().expect("finish");

        let m = &sim.overland().expect("mesh").marcher;
        let l = m.mass_ledger(0);
        assert!(l.runoff_in > 0.0, "no wash-off reached the mesh");
        assert!(l.junction_out > 0.0, "no mass drained into the node");
        let row = l.row(m.mass_of(0));
        assert!(
            row[10].abs() < 1e-9 * l.runoff_in.max(1e-12),
            "mesh mass ledger error {}",
            row[10]
        );
        let by = sim.quality_inflow_by_source("TSS").expect("split");
        assert!(by[5] > 0.0, "the network saw no surface origin");
        // What the network received is what the mesh drained, less the
        // last period's banked mass still in flight and any mass the
        // node spilled back (banked net, per slot), and never more.
        let cs = sim.overland().expect("mesh");
        let in_flight: f64 = cs.pending_mass[0].iter().map(|m| m.abs()).sum();
        assert!(by[5] <= l.junction_out + 1e-9);
        assert!(
            l.junction_out - by[5] <= l.junction_in + in_flight + 1e-9,
            "network received {} of the {} the mesh drained ({} spilled back, {} in flight)",
            by[5],
            l.junction_out,
            l.junction_in,
            in_flight
        );

        let mut rpt = Vec::new();
        crate::dialect::session::write_report(&sim, &mut rpt).expect("report");
        let rpt = String::from_utf8(rpt).expect("utf8");
        for needle in [
            "Surface Drainage Inflow",
            "Surface Spill Loss",
            "Overland Quality Continuity",
            "Runoff Onto Surface",
        ] {
            assert!(rpt.contains(needle), "report lacks {needle}");
        }

        let r = crate::dialect::out_reader::OverlandResults::open(&path).expect("open results");
        assert_eq!(r.constituents, 1);
        let last = r.record(r.periods - 1).expect("last");
        assert_eq!(last.constituents.len(), 1);
        assert_eq!(last.constituents[0].0.len(), r.cells.len());
        assert!(
            last.constituents[0].1[1] > 0.0,
            "the sidecar's runoff-in term"
        );
        let _ = std::fs::remove_file(path);
    }

    /// §12.3 with constituents on the mesh: masses, accumulators in
    /// flight and ledgers resume bit-identically.
    #[test]
    fn a_polluted_surface_run_resumes_bit_identically_from_a_checkpoint() {
        let open = || {
            let mut net = runoff_model_from(&polluted_inp(), true);
            for v in &mut net.overland.as_mut().expect("mesh").verts {
                v.z = 102.0;
            }
            crate::simulation::engine::Simulation::from_network(net, Vec::new(), Vec::new(), None)
                .expect("open")
                .0
        };
        let mut whole = open();
        whole.run();
        let mut first = open();
        for _ in 0..100 {
            first.step();
        }
        let mut held = Vec::new();
        first.save_checkpoint(&mut held).expect("save");
        let mut resumed = open();
        resumed.load_checkpoint(&held).expect("load");
        resumed.run();
        let a = &whole.overland().expect("mesh").marcher;
        let b = &resumed.overland().expect("mesh").marcher;
        assert!(a.mass_of(0) > 0.0 || a.mass_ledger(0).junction_out > 0.0);
        assert_eq!(a.mass_of(0).to_bits(), b.mass_of(0).to_bits(), "mesh mass");
        assert_eq!(
            a.mass_ledger(0).junction_out.to_bits(),
            b.mass_ledger(0).junction_out.to_bits(),
            "drained mass"
        );
        let (qa, qb) = (
            whole.quality_inflow_by_source("TSS").expect("split"),
            resumed.quality_inflow_by_source("TSS").expect("split"),
        );
        assert_eq!(qa[5].to_bits(), qb[5].to_bits(), "network surface origin");
    }

    /// §15.7's refusals, by name: a surface outlet with no mesh, a row
    /// for a node-bound parcel, a surface parcel with no row, a row
    /// naming no parcel, and a runoff point no address matches. A model
    /// that carries a pollutant is served (§15.11).
    #[test]
    fn a_surface_outlet_that_cannot_serve_is_refused_by_name() {
        let open = |net: Network| {
            crate::simulation::engine::Simulation::from_network(net, Vec::new(), Vec::new(), None)
                .err()
                .map(|e| e.to_string())
                .expect("refused")
        };
        let mut net = runoff_model(true);
        net.overland = None;
        assert!(open(net).contains("has no mesh"));

        let mut net = runoff_model(true);
        net.parcels[0].outlet = crate::model::ParcelOutlet::Vertex(0);
        assert!(open(net).contains("not the overland surface"));

        let mut net = runoff_model(true);
        net.overland.as_mut().expect("mesh").runoff_map.clear();
        assert!(open(net).contains("gives it no point"));

        let mut net = runoff_model(true);
        net.overland.as_mut().expect("mesh").runoff_map[0].parcel = "S9".into();
        assert!(open(net).contains("\"S9\""));

        let mut net = runoff_model(true);
        net.overland.as_mut().expect("mesh").runoff_map[0].address = "NOWHERE".into();
        assert!(open(net).contains("no mesh index and no tag"));

        let net = runoff_model_from(&polluted_inp(), true);
        assert!(!net.constituents.is_empty(), "the pollutant parsed");
        assert!(
            crate::simulation::engine::Simulation::from_network(net, Vec::new(), Vec::new(), None)
                .is_ok(),
            "a polluted model routes onto the mesh (§15.11)"
        );
    }

    const INLET_INP: &str = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      00:20
ROUTING_STEP  5
REPORT_STEP   0:05:00

[JUNCTIONS]
J1  100.0  2.0

[OUTFALLS]
O1  99.0  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0

[CURVES]
DIV  DIVERSION  0  0
DIV  1  0.5

[INLETS]
G1  GRATE  0.3  0.3  P_BAR-50
CD  CUSTOM DIV
";

    fn inlet(design: &str) -> crate::overland::CouplingInlet {
        crate::overland::CouplingInlet {
            design: design.into(),
            count: 1,
            pct_clogged: 0.0,
            flow_limit: 0.0,
            local_depression: 0.0,
            local_width: 0.0,
        }
    }

    /// The pond helper lowered a metre below J1's rim, so its surface
    /// never reaches the rim gate.
    fn sub_rim_mesh(h0: f64) -> OverlandMesh {
        let mut mesh = pond_mesh(h0, "J1");
        for v in &mut mesh.verts {
            v.z = 101.0;
        }
        mesh
    }

    /// §15.6: a pond standing below a node's rim drains only through an
    /// inlet at the point — without one, the rim gate stays shut and the
    /// pond stays put. This is the street-drainage gap §15.10 recorded.
    #[test]
    fn a_pond_below_the_rim_drains_only_through_an_inlet() {
        let run = |mesh: OverlandMesh| {
            let (mut sim, _, findings) = crate::dialect::session::open(INLET_INP).expect("open");
            assert!(findings.iter().all(|f| !f.kind.is_error()), "{findings:?}");
            sim.attach_overland(mesh).expect("attach");
            let v0 = sim.overland().expect("attached").marcher.storage();
            sim.run();
            let m = &sim.overland().expect("attached").marcher;
            (
                v0,
                m.storage(),
                m.coupling_out,
                m.coupling_in,
                m.infiltration_out,
            )
        };
        // Control: no inlet, no exchange at all below the rim.
        let (v0, held, out, back, lost) = run(sub_rim_mesh(0.3));
        assert_eq!(out, 0.0, "the rim gate opened below the rim");
        assert_eq!(back, 0.0);
        assert!(
            (v0 - held - lost).abs() < 1e-9,
            "surface ledger without an inlet"
        );

        let mut mesh = sub_rim_mesh(0.3);
        mesh.cell_couplings[0].inlet = Some(inlet("G1"));
        let (v0, held, out, back, lost) = run(mesh);
        assert!(out > 0.0, "the inlet never captured");
        assert!(held < 0.2 * v0, "pond still holds {held} of {v0}");
        assert!(
            (v0 - held - (out - back) - lost).abs() < 1e-9,
            "surface ledger with an inlet"
        );
    }

    /// §15.6's refusals, each by name: a design the model lacks, a
    /// diversion-curve design, two inlets on one cell, an inlet at an
    /// outfall.
    #[test]
    fn an_inlet_that_cannot_serve_is_refused_by_name() {
        let attach = |mesh: OverlandMesh| {
            let (mut sim, _, _) = crate::dialect::session::open(INLET_INP).expect("open");
            sim.attach_overland(mesh).expect_err("refused")
        };
        let mut mesh = sub_rim_mesh(0.3);
        mesh.cell_couplings[0].inlet = Some(inlet("NOPE"));
        assert!(matches!(attach(mesh), AttachError::UnknownInlet(n) if n == "NOPE"));

        let mut mesh = sub_rim_mesh(0.3);
        mesh.cell_couplings[0].inlet = Some(inlet("CD"));
        assert!(matches!(
            attach(mesh),
            AttachError::InletUnserved { design, why } if design == "CD" && why.contains("diversion")
        ));

        // Vertex 0's stencil collapses to cell 0, where a cell row
        // already carries an inlet.
        let mut mesh = sub_rim_mesh(0.3);
        mesh.cell_couplings[0].inlet = Some(inlet("G1"));
        mesh.vertex_couplings.push(CouplingRow {
            address: "0".into(),
            node: "J1".into(),
            cd: 0.65,
            area: 0.05,
            area_authored: true,
            inlet: Some(inlet("G1")),
        });
        assert!(matches!(
            attach(mesh),
            AttachError::InletsShareCell { cell: 0 }
        ));

        let mut mesh = sub_rim_mesh(0.3);
        mesh.cell_couplings[0].node = "O1".into();
        mesh.cell_couplings[0].inlet = Some(inlet("G1"));
        assert!(matches!(
            attach(mesh),
            AttachError::InletUnserved { why, .. } if why.contains("outfall")
        ));
    }

    /// §12.3 for the surface: a mesh run checkpoints mid-run and a
    /// restored session continues bit-identically to one never
    /// interrupted — and the mesh fingerprint refuses a checkpoint from
    /// a different terrain.
    #[test]
    fn a_mesh_run_resumes_bit_identically_from_a_checkpoint() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      00:20
ROUTING_STEP  5
REPORT_STEP   0:05:00

[JUNCTIONS]
J1  100.0  2.0

[OUTFALLS]
O1  99.0  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0
";
        // The uninterrupted reference.
        let (mut whole, _, _) = crate::dialect::session::open(inp).expect("open");
        whole.attach_overland(pond_mesh(0.3, "J1")).expect("attach");
        whole.run();

        // The same run, checkpointed midway and resumed elsewhere.
        let (mut first, _, _) = crate::dialect::session::open(inp).expect("open");
        first.attach_overland(pond_mesh(0.3, "J1")).expect("attach");
        for _ in 0..120 {
            first.step();
        }
        let mut held = Vec::new();
        first.save_checkpoint(&mut held).expect("save");

        let (mut resumed, _, _) = crate::dialect::session::open(inp).expect("open");
        resumed
            .attach_overland(pond_mesh(0.3, "J1"))
            .expect("attach");
        resumed.load_checkpoint(&held).expect("load");
        resumed.run();

        let a = &whole.overland().expect("attached").marcher;
        let b = &resumed.overland().expect("attached").marcher;
        let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&a.vol), bits(&b.vol), "volumes");
        assert_eq!(bits(&a.eta), bits(&b.eta), "surfaces");
        assert_eq!(bits(&a.q), bits(&b.q), "discharges");
        assert_eq!(a.coupling_out.to_bits(), b.coupling_out.to_bits(), "ledger");
        assert_eq!(
            a.infiltration_out.to_bits(),
            b.infiltration_out.to_bits(),
            "infiltration ledger"
        );
        let il_bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(
            il_bits(&a.il_left),
            il_bits(&b.il_left),
            "initial-loss capacity"
        );
        assert_eq!(a.storage().to_bits(), b.storage().to_bits());
        let ca = whole.overland().expect("attached");
        let cb = resumed.overland().expect("attached");
        assert_eq!(ca.delivered_in.to_bits(), cb.delivered_in.to_bits());

        // A different terrain is a different model: the mesh
        // fingerprint refuses.
        let (mut other, _, _) = crate::dialect::session::open(inp).expect("open");
        let mut mesh = pond_mesh(0.3, "J1");
        for v in &mut mesh.verts {
            v.z += 0.5;
        }
        other.attach_overland(mesh).expect("attach");
        let err = other.load_checkpoint(&held).expect_err("refusal");
        assert!(err.contains("different mesh"), "{err}");
    }

    /// §15.6 the other direction: a surcharged node spills onto the
    /// surface, the spill draws the node's delivered lateral negative,
    /// and the surface gains exactly what the ledger booked.
    #[test]
    fn a_surcharged_node_spills_onto_the_surface() {
        let (net, mut router) = network();
        let m = marcher_of(&pond_mesh(0.0, "J1"));
        let mut cs = CoupledSurface::new(m, &net, &mut router).expect("resolves");

        let nv = 2;
        let period = 5.0;
        let mut t = 0.0;
        let mut delivered_neg = 0.0;
        for _ in 0..120 {
            let mut lat = vec![0.0; nv];
            cs.deliver_laterals(&mut lat, &mut [], period);
            if lat[0] < 0.0 {
                delivered_neg += -lat[0] * period;
            }
            // Far above the little pipe's capacity: the coupled vertex
            // ponds above its rim and reaches the surface.
            lat[0] += 1.0;
            t += period;
            router.advance(t, &move |_tt, l: &mut [f64]| l.copy_from_slice(&lat));
            cs.co_advance(&mut router, None, period);
        }
        assert!(
            cs.marcher.coupling_in > 0.0,
            "the node never reached the surface"
        );
        assert!(
            (cs.marcher.storage() + cs.marcher.infiltration_out - cs.marcher.coupling_in).abs()
                < 1e-9,
            "surface gained {} + losses {} but the ledger says {}",
            cs.marcher.storage(),
            cs.marcher.infiltration_out,
            cs.marcher.coupling_in
        );
        // What spilled was drawn back off the node as negative lateral.
        assert!(
            (delivered_neg
                - (cs.marcher.coupling_in
                    - cs.pending.iter().map(|p| p.min(0.0)).sum::<f64>().abs()))
            .abs()
                < 1e-6,
            "spill draw-down mismatch: {delivered_neg} vs {}",
            cs.marcher.coupling_in
        );
    }
}
