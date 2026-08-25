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
        let pending = vec![0.0; slot_vertex.len()];
        let outfall_pending = vec![0.0; slot_vertex.len()];
        Ok(CoupledSurface {
            marcher,
            slot_vertex,
            pending,
            outfall_pending,
            delivered_in: 0.0,
            delivered_out: 0.0,
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
    pub fn deliver_laterals(&mut self, lat: &mut [f64], period: f64) {
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
    pub fn co_advance(&mut self, router: &mut Router, period: f64) {
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
        for (k, &dv) in self.marcher.exchanged().iter().enumerate() {
            let slot = self.marcher.coupling_points()[k].node_slot as usize;
            self.pending[slot] += dv;
        }
        // Damping for the coming period, §6.4: the summed conductance of
        // every point naming the vertex, against the live surface.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::objects::parse_network;
    use crate::io::validate::validate;
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
        });
        mesh
    }

    fn marcher_of(mesh: &OverlandMesh) -> Marcher {
        let topo = Topology::build(mesh).expect("valid mesh");
        Marcher::build(mesh, &topo)
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
            cs.deliver_laterals(&mut lat, period);
            delivered += lat[0] * period;
            t += period;
            router.advance(t, &move |_tt, l: &mut [f64]| l.copy_from_slice(&lat));
            cs.co_advance(&mut router, period);
        }

        // The pond drained through the orifice.
        assert!(
            cs.marcher.storage() < 0.05 * v0,
            "pond still holds {}",
            cs.marcher.storage()
        );
        // Surface ledger: what left the surface is what the coupling
        // booked.
        let drained = cs.marcher.coupling_out - cs.marcher.coupling_in;
        assert!(
            (v0 - cs.marcher.storage() - drained).abs() < 1e-9,
            "surface ledger"
        );
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
            cs.deliver_laterals(&mut lat, period);
            lat[0] += 0.05;
            t += period;
            router.advance(t, &move |_tt, l: &mut [f64]| l.copy_from_slice(&lat));
            cs.co_advance(&mut router, period);
        }
        assert!(
            cs.marcher.outfall_in > 0.0,
            "the outfall never reached the surface"
        );
        // §15.8: the injection path books apart from the junction
        // exchange, and the surface holds what it says.
        assert_eq!(cs.marcher.coupling_in, 0.0);
        assert!(
            (cs.marcher.storage() - cs.marcher.outfall_in).abs() < 1e-9,
            "surface {} vs injected {}",
            cs.marcher.storage(),
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
        cs.co_advance(&mut router, 5.0);
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
        let (mut sim, _, findings) = crate::simulation::Simulation::open(inp).expect("open");
        assert!(findings.iter().all(|f| !f.kind.is_error()));
        sim.attach_overland(&pond_mesh(0.3, "J1")).expect("attach");
        let v0 = sim.overland().expect("attached").marcher.storage();

        // §15.10: a mesh run refuses checkpointing, by name.
        let mut sink = Vec::new();
        let err = sim.save_checkpoint(&mut sink).expect_err("refusal");
        assert!(err.contains("overland mesh"), "{err}");

        sim.run();
        let m = &sim.overland().expect("attached").marcher;
        assert!(
            m.storage() < 0.2 * v0,
            "pond still holds {} of {v0}",
            m.storage()
        );
        assert!(
            (v0 - m.storage() - (m.coupling_out - m.coupling_in)).abs() < 1e-9,
            "surface ledger"
        );
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
            cs.deliver_laterals(&mut lat, period);
            if lat[0] < 0.0 {
                delivered_neg += -lat[0] * period;
            }
            // Far above the little pipe's capacity: the coupled vertex
            // ponds above its rim and reaches the surface.
            lat[0] += 1.0;
            t += period;
            router.advance(t, &move |_tt, l: &mut [f64]| l.copy_from_slice(&lat));
            cs.co_advance(&mut router, period);
        }
        assert!(
            cs.marcher.coupling_in > 0.0,
            "the node never reached the surface"
        );
        assert!(
            (cs.marcher.storage() - cs.marcher.coupling_in).abs() < 1e-9,
            "surface gained {} but the ledger says {}",
            cs.marcher.storage(),
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
