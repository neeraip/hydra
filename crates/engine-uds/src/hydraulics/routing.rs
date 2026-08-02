//! §6 Network Flow: the one solver.
//!
//! Saint-Venant on every channel, continuity at every vertex, closed by
//! the §6.2 pressurisation slot — a width derived from a stated celerity,
//! one equation set in every regime. The scheme is the §6.3 staggered
//! discretisation iterated to self-consistency (§6.4), advanced by
//! transactional trial steps under a real local error estimate (§6.5):
//! a non-converged or inaccurate trial is discarded and retried at half
//! the step, and no state is reported that is not a solution of the
//! equations, except at the step floor, where it says so.
//!
//! This stage routes channels, junctions, storage, dividers-as-junctions,
//! and outfalls; the §7 structures splice in next.

use super::section::Section;
use super::{tables, GRAVITY};
use crate::io::options::NormalFlowCriteria;
use crate::model::{
    CurveKind, LinkKind, Network, Offset, OrificeOrientation, OutfallStage, OutletHeadBasis,
    OutletRating, StorageGeometry, VertexKind, WeirForm, XsectShape,
};

/// A storage vertex's area relation, resolved at build.
enum StoreArea {
    /// $A = c + a\,y^{b}$.
    Functional {
        coeff: f64,
        exponent: f64,
        constant: f64,
    },
    /// $A = a_0 + a_1 y + a_2 y^2$.
    Shape { a0: f64, a1: f64, a2: f64 },
    /// Interpolated points, extended flat.
    Table(Vec<(f64, f64)>),
}

impl StoreArea {
    fn resolve(g: &StorageGeometry, net: &Network) -> StoreArea {
        match g {
            StorageGeometry::Functional {
                coeff,
                exponent,
                constant,
            } => StoreArea::Functional {
                coeff: *coeff,
                exponent: *exponent,
                constant: *constant,
            },
            StorageGeometry::Shape { a0, a1, a2, .. } => StoreArea::Shape {
                a0: *a0,
                a1: *a1,
                a2: *a2,
            },
            StorageGeometry::Tabular { curve } => {
                StoreArea::Table(net.curves[*curve].points.clone())
            }
        }
    }

    /// Integrated volume to depth `y` (m³).
    fn volume(&self, y: f64) -> f64 {
        let y = y.max(0.0);
        match self {
            StoreArea::Functional {
                coeff,
                exponent,
                constant,
            } => constant * y + coeff * y.powf(exponent + 1.0) / (exponent + 1.0),
            StoreArea::Shape { a0, a1, a2 } => a0 * y + a1 * y * y / 2.0 + a2 * y * y * y / 3.0,
            StoreArea::Table(pts) => {
                let mut vol = 0.0;
                let mut y0 = 0.0;
                let mut a0 = pts.first().map_or(0.0, |p| p.1);
                for &(yy, aa) in pts {
                    if yy <= 0.0 {
                        a0 = aa;
                        continue;
                    }
                    let y1 = yy.min(y);
                    if y1 > y0 {
                        let a1 = a0 + (aa - a0) * (y1 - y0) / (yy - y0);
                        vol += 0.5 * (a0 + a1) * (y1 - y0);
                        y0 = y1;
                    }
                    a0 = aa;
                    if y0 >= y {
                        break;
                    }
                }
                if y > y0 {
                    vol += a0 * (y - y0);
                }
                vol
            }
        }
    }

    fn area(&self, y: f64) -> f64 {
        let y = y.max(0.0);
        match self {
            StoreArea::Functional {
                coeff,
                exponent,
                constant,
            } => constant + coeff * y.powf(*exponent),
            StoreArea::Shape { a0, a1, a2 } => a0 + a1 * y + a2 * y * y,
            StoreArea::Table(pts) => {
                if pts.is_empty() {
                    return 0.0;
                }
                if y <= pts[0].0 {
                    return pts[0].1;
                }
                for w in pts.windows(2) {
                    if y <= w[1].0 {
                        let f = (y - w[0].0) / (w[1].0 - w[0].0);
                        return w[0].1 + f * (w[1].1 - w[0].1);
                    }
                }
                pts[pts.len() - 1].1
            }
        }
    }
}

/// Nearly-dry depth (the predecessor's 0.001 ft).
const DRY: f64 = 3.048e-4;
/// Velocity cap in the momentum terms (m/s), §6.3.
const V_MAX: f64 = 15.24;
/// Flow substituted when a relaxed flow reverses sign (m³/s), §6.4.
const Q_REVERSAL: f64 = 2.832e-5;
/// Clamp on flow out of an essentially dry vertex (m³/s), §6.6.
const Q_DRY: f64 = 2.832e-6;
/// Under-relaxation factor, §6.4.
const OMEGA: f64 = 0.5;
/// Step floor (s), §6.5; the run opens here.
const DT_FLOOR: f64 = 0.5;

/// Why a network cannot be routed by this build stage.
#[derive(Debug, Clone, PartialEq)]
pub enum RouterRefusal {
    /// An element class the §6 core does not route yet.
    Unsupported(&'static str),
    /// A section that failed to build (§14.7 would have refused it).
    Geometry(String),
}

/// Slot-modified geometry (§6.2): the section's true properties with the
/// top width floored at the slot width and the closure band above the
/// crown, continuous everywhere.
#[derive(Debug, Clone)]
struct SlotGeom {
    sec: Section,
    /// Slot width (m); zero for open sections.
    w_slot: f64,
    /// The depth where the closed section's falling width crosses the
    /// slot width.
    y_x: f64,
    /// Area correction accumulated across the crown band at full depth.
    band_full: f64,
}

impl SlotGeom {
    fn build(sec: Section, celerity: f64) -> SlotGeom {
        if !sec.is_closed() || sec.a_full() <= 0.0 {
            return SlotGeom {
                sec,
                w_slot: 0.0,
                y_x: f64::INFINITY,
                band_full: 0.0,
            };
        }
        let w_slot = GRAVITY * sec.a_full() / (celerity * celerity);
        // The crossing depth on the falling branch of W(y).
        let (yw, _) = sec.w_max();
        let mut lo = yw;
        let mut hi = sec.y_full();
        if sec.top_width(hi) >= w_slot {
            // A section whose width never falls to the slot width (a
            // wide flat lid): no band.
            let y_x = hi;
            return SlotGeom {
                sec,
                w_slot,
                y_x,
                band_full: 0.0,
            };
        }
        for _ in 0..80 {
            let m = 0.5 * (lo + hi);
            if sec.top_width(m) > w_slot {
                lo = m;
            } else {
                hi = m;
            }
        }
        let y_x = 0.5 * (lo + hi);
        let band_full = w_slot * (sec.y_full() - y_x) - (sec.area(sec.y_full()) - sec.area(y_x));
        SlotGeom {
            sec,
            w_slot,
            y_x,
            band_full,
        }
    }

    fn width(&self, y: f64) -> f64 {
        if self.w_slot == 0.0 {
            return self.sec.top_width(y);
        }
        if y >= self.sec.y_full() {
            return self.w_slot;
        }
        self.sec.top_width(y).max(self.w_slot)
    }

    fn area(&self, y: f64) -> f64 {
        let y_full = self.sec.y_full();
        if self.w_slot == 0.0 || y <= self.y_x {
            return self.sec.area(y.min(y_full));
        }
        if y <= y_full {
            return self.sec.area(self.y_x) + self.w_slot * (y - self.y_x);
        }
        self.sec.area(y_full) + self.band_full + self.w_slot * (y - y_full)
    }

    fn hyd_radius(&self, y: f64) -> f64 {
        if y >= self.sec.y_full() {
            return self.sec.r_full();
        }
        self.sec.hyd_radius(y)
    }
}

/// How a pump draws its flow (§7.1).
enum PumpKind {
    /// Stepwise on wet-well volume.
    Volume(Vec<(f64, f64)>),
    /// Stepwise on inlet depth.
    Depth(Vec<(f64, f64)>),
    /// Linear on head difference; `affinity` scales per §7.1 Type 5.
    Head {
        points: Vec<(f64, f64)>,
        affinity: bool,
    },
    /// Linear on inlet depth (Type 4).
    InlineDepth(Vec<(f64, f64)>),
    /// The ideal transfer pump.
    Ideal,
}

/// A §7 structure spliced into the graph.
enum StructKind {
    Pump {
        kind: PumpKind,
        startup: f64,
        shutoff: f64,
    },
    Orifice {
        bottom: bool,
        cd: f64,
        sec: Section,
        flap: bool,
    },
    Weir {
        form: WeirForm,
        cd1: f64,
        cd2: f64,
        sec: Section,
        flap: bool,
        end_contractions: f64,
        can_surcharge: bool,
        coeff_curve: Option<Vec<(f64, f64)>>,
        road_width: f64,
        /// Paved or gravel, when the FHWA coefficient tables apply.
        road_paved: Option<bool>,
    },
    Outlet {
        rating: OutRating,
        by_head_difference: bool,
        flap: bool,
    },
    /// A zero-geometry connector: passes its upstream vertex's inflow.
    Dummy { q_limit: f64 },
}

enum OutRating {
    Functional { coeff: f64, exponent: f64 },
    Table(Vec<(f64, f64)>),
}

struct Structure {
    from: usize,
    to: usize,
    link: usize,
    /// Crest offset above the upstream invert (m).
    off1: f64,
    /// Equivalent-pipe length (m), §7.2.
    eq_length: f64,
    kind: StructKind,
}

/// Stepwise lookup: the value at the first point whose abscissa exceeds
/// the argument (§7.1); the last value beyond the table.
fn interval_lookup(points: &[(f64, f64)], x: f64) -> f64 {
    for p in points {
        if p.0 > x {
            return p.1;
        }
    }
    points.last().map_or(0.0, |p| p.1)
}

/// Linear interpolation clamped to the table's ends.
fn linear_lookup(points: &[(f64, f64)], x: f64) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    if x <= points[0].0 {
        return points[0].1;
    }
    for w in points.windows(2) {
        if x <= w[1].0 {
            let f = (x - w[0].0) / (w[1].0 - w[0].0);
            return w[0].1 + f * (w[1].1 - w[0].1);
        }
    }
    points[points.len() - 1].1
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FlowClass {
    Subcritical,
    UpCritical,
    DownCritical,
    UpDry,
    DownDry,
    Dry,
}

/// A channel's friction law (§7.7): Manning, or a pressurised relation
/// while a force main flows full.
enum Friction {
    Manning,
    /// Hazen–Williams, by the C-factor.
    HazenWilliams {
        c: f64,
    },
    /// Darcy–Weisbach, by the roughness height (m), with Swamee–Jain.
    DarcyWeisbach {
        e: f64,
    },
}

/// Kinematic viscosity of water (m²/s) — the predecessor's constant,
/// converted.
const VISCOSITY: f64 = 1.1e-5 * 0.3048 * 0.3048;

impl Friction {
    /// The §6.3 friction denominator term for a full force main, or
    /// `None` to use Manning.
    fn pressurised_dq(&self, v: f64, r: f64, dt: f64) -> Option<f64> {
        match self {
            Friction::Manning => None,
            Friction::HazenWilliams { c } => Some(
                dt * GRAVITY * v.abs().powf(0.852) / (0.849 * c).powf(1.852) / r.powf(1.166_67),
            ),
            Friction::DarcyWeisbach { e } => {
                let re = (4.0 * r * v.abs() / VISCOSITY).max(10.0);
                let f = swamee_jain(*e, r, re);
                Some(dt * f * v.abs() / (8.0 * r))
            }
        }
    }
}

/// The Swamee–Jain friction factor with the predecessor's laminar and
/// transitional treatment (§7.7).
fn swamee_jain(e: f64, hrad: f64, re: f64) -> f64 {
    if re <= 2000.0 {
        return 64.0 / re;
    }
    if re < 4000.0 {
        let f4000 = swamee_jain(e, hrad, 4000.0);
        return 0.032 + (f4000 - 0.032) * (re - 2000.0) / 2000.0;
    }
    let mut x = e / 3.7 / (4.0 * hrad);
    if re < 1.0e10 {
        x += 5.74 / re.powf(0.9);
    }
    let l = x.log10();
    0.25 / (l * l)
}

struct Chan {
    from: usize,
    to: usize,
    /// Model link index, for reporting.
    link: usize,
    /// Invert offsets above the end-vertex inverts (m).
    off1: f64,
    off2: f64,
    /// True length (m), meander-adjusted for transect channels (§6.5).
    length: f64,
    /// Manning n.
    n: f64,
    /// Bed slope magnitude, from the §14.7 rules.
    slope: f64,
    barrels: f64,
    q_limit: f64,
    loss_inlet: f64,
    loss_outlet: f64,
    loss_avg: f64,
    flap_gate: bool,
    /// Seepage rate through the bed (m/s).
    seepage: f64,
    friction: Friction,
    /// FHWA culvert inlet code; 0 = not a culvert (§7.6).
    culvert: usize,
    geom: SlotGeom,
}

impl Chan {
    fn z1(&self, verts: &[Vert]) -> f64 {
        verts[self.from].invert + self.off1
    }
    fn z2(&self, verts: &[Vert]) -> f64 {
        verts[self.to].invert + self.off2
    }
}

enum Boundary {
    /// Depth follows min(critical, normal) of the connecting channel.
    Free,
    /// Normal depth of the connecting channel.
    Normal,
    /// A fixed stage elevation (m).
    Fixed(f64),
}

enum VertClass {
    Junction,
    Storage(StoreArea),
    Outfall(Boundary),
}

struct Vert {
    invert: f64,
    /// Full (rim) depth (m), post-§14.7.
    y_max: f64,
    surcharge: f64,
    ponded_area: f64,
    /// Highest connecting crown above the invert (m).
    crown: f64,
    class: VertClass,
}

/// Per-step outcome counters and the running mass ledger (§11 formalises
/// these; the router keeps the raw sums).
#[derive(Debug, Clone, Default)]
pub struct RoutingReport {
    /// Accepted trial steps.
    pub accepted: u64,
    /// Rejected trials (error test or convergence budget).
    pub rejected: u64,
    /// Steps accepted at the floor with the degraded-accuracy warning,
    /// each naming its worst vertex.
    pub degraded: Vec<(f64, String)>,
    /// Total lateral inflow volume (m³).
    pub inflow: f64,
    /// Total outfall discharge volume (m³).
    pub outflow: f64,
    /// Total flooding volume (m³).
    pub flooding: f64,
    /// Total channel evaporation and seepage volume (m³).
    pub losses: f64,
}

/// The §6 router over a validated network.
pub struct Router {
    chans: Vec<Chan>,
    verts: Vec<Vert>,
    structs: Vec<Structure>,
    ids: Vec<String>,
    // Options.
    dt_user: f64,
    courant_factor: f64,
    max_trials: u32,
    head_tol: f64,
    min_surface_area: f64,
    continuity_tol: f64,
    err_tol: f64,
    allow_ponding: bool,
    normal_flow: NormalFlowCriteria,
    // State (accepted).
    t: f64,
    y: Vec<f64>,
    q: Vec<f64>,
    /// Structure flows (m³/s).
    sq: Vec<f64>,
    /// Pump on/off latches (§7.1).
    pump_on: Vec<bool>,
    /// §9 operational settings per structure: pump speed, orifice/weir
    /// opening fraction, outlet scale. Default 1.
    sett: Vec<f64>,
    /// §9 channel open/closed states. Default open.
    chan_open: Vec<bool>,
    /// Time of the last §9 status flip, per structure and per channel
    /// (router clock s), for the TIMEOPEN/TIMECLOSED premises.
    struct_flip_t: Vec<f64>,
    chan_flip_t: Vec<f64>,
    a_mid: Vec<f64>,
    net_flow: Vec<f64>,
    /// Per-vertex flooding rate of the last accepted step (m³/s).
    flood_now: Vec<f64>,
    // Head history for the error estimate: the two previous accepted
    // (time, heads) records, oldest first.
    hist: Vec<(f64, Vec<f64>)>,
    dt_prev: f64,
    quiet_streak: u32,
    /// Potential evaporation rate applied to open channels (m/s); set by
    /// the session forcing (§10), default 0.
    pub evap_rate: f64,
    /// The report accumulated across `advance` calls.
    pub report: RoutingReport,
}

/// A candidate state computed by one trial.
struct Trial {
    y: Vec<f64>,
    q: Vec<f64>,
    sq: Vec<f64>,
    loss_rate: f64,
    a_mid: Vec<f64>,
    net_flow: Vec<f64>,
    converged: bool,
    flood_rate: Vec<f64>,
    worst_vertex: usize,
    err: f64,
}

impl Router {
    /// Build a router from a **validated** network (§14.7 already applied).
    pub fn build(net: &Network) -> Result<Router, RouterRefusal> {
        let len = if net.options.flow_units.is_us() {
            0.3048
        } else {
            1.0
        };
        let celerity = net.options.pressure_celerity.max(5.0);

        let mut verts = Vec::with_capacity(net.vertices.len());
        for v in &net.vertices {
            let (y_max, surcharge, ponded, class) = match &v.kind {
                VertexKind::Junction {
                    max_depth,
                    surcharge_depth,
                    ponded_area,
                    ..
                } => (
                    *max_depth,
                    *surcharge_depth,
                    *ponded_area,
                    VertClass::Junction,
                ),
                VertexKind::Storage {
                    max_depth,
                    surcharge_depth,
                    geometry,
                    ..
                } => (
                    *max_depth,
                    *surcharge_depth,
                    0.0,
                    VertClass::Storage(StoreArea::resolve(geometry, net)),
                ),
                // A divider routes as a junction under the one solver
                // (§7.5); its rule is an import record.
                VertexKind::Divider { .. } => (f64::MAX, 0.0, 0.0, VertClass::Junction),
                VertexKind::Outfall { stage, .. } => {
                    let b = match stage {
                        OutfallStage::Free => Boundary::Free,
                        OutfallStage::Normal => Boundary::Normal,
                        OutfallStage::Fixed(e) => Boundary::Fixed(*e),
                        // Tidal and series stages are dynamic fixed
                        // stages the session updates each period (§10.1).
                        OutfallStage::Tidal { .. } | OutfallStage::Series { .. } => {
                            Boundary::Fixed(v.invert)
                        }
                    };
                    (f64::MAX, 0.0, 0.0, VertClass::Outfall(b))
                }
            };
            verts.push(Vert {
                invert: v.invert,
                y_max,
                surcharge,
                ponded_area: ponded,
                crown: 0.0,
                class,
            });
        }

        let mut chans = Vec::new();
        for (li, link) in net.links.iter().enumerate() {
            #[allow(clippy::single_match)] // the other kinds build below
            match &link.kind {
                LinkKind::Channel {
                    length,
                    roughness,
                    offset1,
                    offset2,
                    max_flow,
                    loss_inlet,
                    loss_outlet,
                    loss_avg,
                    flap_gate,
                    seepage_rate,
                    ..
                } => {
                    let xs =
                        link.cross_section
                            .as_ref()
                            .ok_or(RouterRefusal::Geometry(format!(
                                "{}: no cross-section",
                                link.id
                            )))?;
                    if xs.shape == XsectShape::Dummy {
                        // Routed as a pass-through structure below.
                        continue;
                    }

                    let built = crate::io::validate::build_for_link(net, li, len)
                        .ok_or(RouterRefusal::Geometry(format!(
                            "{}: no cross-section",
                            link.id
                        )))?
                        .map_err(|e| RouterRefusal::Geometry(format!("{}: {e:?}", link.id)))?;
                    let sec = built.section;
                    // Effective roughness and length (§5.6, §14.7).
                    let (mut n, mut eff_len) = (*roughness, *length);
                    if let Some(crate::model::XsectReferent::Transect(t)) = xs.referent {
                        let tr = &net.transects[t];
                        n = tr.n_channel * tr.meander_factor.sqrt();
                        if tr.meander_factor > 0.0 {
                            eff_len = length / tr.meander_factor;
                        }
                    }
                    if let Some(crate::model::XsectReferent::Street(s)) = xs.referent {
                        n = net.streets[s].roughness;
                    }
                    let (off1, off2) = (offset_height(offset1), offset_height(offset2));
                    // Slope per the §14.7 rules (warnings already given).
                    let elev1 = net.vertices[link.from].invert + off1;
                    let elev2 = net.vertices[link.to].invert + off2;
                    let delta = (elev1 - elev2).abs().max(0.001 * 0.3048);
                    let slope = if delta >= eff_len {
                        delta / eff_len
                    } else {
                        delta / (eff_len * eff_len - delta * delta).sqrt()
                    }
                    .max(net.options.min_slope.max(0.0));
                    // Force mains: the pressurised relation while full and
                    // the predecessor's equivalent Manning n otherwise,
                    // converted to SI (§7.7).
                    let mut friction = Friction::Manning;
                    if xs.shape == XsectShape::ForceMain {
                        let coeff = xs.geom_user[1];
                        if coeff <= 0.0 {
                            return Err(RouterRefusal::Geometry(format!(
                                "{}: force-main coefficient must be positive",
                                link.id
                            )));
                        }
                        let d = sec.y_full();
                        match net.options.force_main {
                            crate::io::options::ForceMainEquation::HazenWilliams => {
                                n = 1.119 / coeff * (d / slope).powf(0.04);
                                friction = Friction::HazenWilliams { c: coeff };
                            }
                            crate::io::options::ForceMainEquation::DarcyWeisbach => {
                                let e = coeff
                                    * if net.options.flow_units.is_us() {
                                        0.0254
                                    } else {
                                        0.001
                                    };
                                let f = swamee_jain(e, d / 4.0, 1.0e12);
                                n = (f / 124.55).sqrt() * d.powf(1.0 / 6.0);
                                friction = Friction::DarcyWeisbach { e };
                            }
                        }
                    }
                    chans.push(Chan {
                        from: link.from,
                        to: link.to,
                        link: li,
                        off1,
                        off2,
                        length: eff_len,
                        n,
                        slope,
                        barrels: f64::from(xs.barrels.max(1)),
                        q_limit: *max_flow,
                        loss_inlet: *loss_inlet,
                        loss_outlet: *loss_outlet,
                        loss_avg: *loss_avg,
                        flap_gate: *flap_gate,
                        seepage: *seepage_rate,
                        friction,
                        culvert: xs.culvert_code as usize,
                        geom: SlotGeom::build(sec, celerity),
                    });
                }
                _ => {}
            }
        }

        let mut structs = Vec::new();
        let mut pump_on = Vec::new();
        for (li, link) in net.links.iter().enumerate() {
            let off1 = match &link.kind {
                LinkKind::Orifice { offset, .. }
                | LinkKind::Weir { offset, .. }
                | LinkKind::Outlet { offset, .. } => offset_height(offset),
                _ => 0.0,
            };
            // Equivalent-pipe length from the section, §7.2.
            let eq_length = |sec: &Section| {
                (2.0 * net.options.routing_step * (GRAVITY * sec.y_full()).sqrt()).max(60.96)
            };
            let kind = match &link.kind {
                LinkKind::Channel { max_flow, .. } => {
                    let dummy = link
                        .cross_section
                        .as_ref()
                        .is_some_and(|x| x.shape == XsectShape::Dummy);
                    if !dummy {
                        continue;
                    }
                    StructKind::Dummy { q_limit: *max_flow }
                }
                LinkKind::Pump {
                    curve,
                    initial_on,
                    startup_depth,
                    shutoff_depth,
                } => {
                    let kind = match curve {
                        None => {
                            // An ideal pump must be its vertex's only
                            // outlet (§7.1).
                            let outlets = net.links.iter().filter(|l| l.from == link.from).count();
                            if outlets > 1 {
                                return Err(RouterRefusal::Geometry(format!(
                                    "{}: an ideal pump must be its vertex's only outlet",
                                    link.id
                                )));
                            }
                            PumpKind::Ideal
                        }
                        Some(c) => {
                            let pts = net.curves[*c].points.clone();
                            match net.curves[*c].kind {
                                CurveKind::Pump1 => PumpKind::Volume(pts),
                                CurveKind::Pump2 => PumpKind::Depth(pts),
                                CurveKind::Pump3 => PumpKind::Head {
                                    points: pts,
                                    affinity: false,
                                },
                                CurveKind::Pump4 => PumpKind::InlineDepth(pts),
                                CurveKind::Pump5 => PumpKind::Head {
                                    points: pts,
                                    affinity: true,
                                },
                                _ => {
                                    return Err(RouterRefusal::Geometry(format!(
                                        "{}: pump curve has the wrong role",
                                        link.id
                                    )));
                                }
                            }
                        }
                    };
                    pump_on.push(*initial_on);
                    structs.push(Structure {
                        from: link.from,
                        to: link.to,
                        link: li,
                        off1,
                        eq_length: 0.0,
                        kind: StructKind::Pump {
                            kind,
                            startup: *startup_depth,
                            shutoff: *shutoff_depth,
                        },
                    });
                    continue;
                }
                LinkKind::Orifice {
                    orientation,
                    discharge_coeff,
                    flap_gate,
                    ..
                } => {
                    let sec = build_struct_section(net, li, len, &link.id)?;
                    StructKind::Orifice {
                        bottom: *orientation == OrificeOrientation::Bottom,
                        cd: *discharge_coeff,
                        flap: *flap_gate,
                        sec,
                    }
                }
                LinkKind::Weir {
                    form,
                    discharge_coeff,
                    end_coeff,
                    flap_gate,
                    end_contractions,
                    can_surcharge,
                    coeff_curve,
                    road_width,
                    road_surface,
                    ..
                } => {
                    let sec = build_struct_section(net, li, len, &link.id)?;
                    StructKind::Weir {
                        form: *form,
                        cd1: *discharge_coeff,
                        cd2: *end_coeff,
                        flap: *flap_gate,
                        end_contractions: *end_contractions,
                        can_surcharge: *can_surcharge,
                        coeff_curve: coeff_curve.map(|c| net.curves[c].points.clone()),
                        road_width: *road_width,
                        road_paved: match road_surface {
                            crate::model::RoadSurface::Paved => Some(true),
                            crate::model::RoadSurface::Gravel => Some(false),
                            crate::model::RoadSurface::Unspecified => None,
                        },
                        sec,
                    }
                }
                LinkKind::Outlet {
                    rating,
                    head_basis,
                    flap_gate,
                    ..
                } => StructKind::Outlet {
                    rating: match rating {
                        OutletRating::Functional { coeff, exponent } => OutRating::Functional {
                            coeff: *coeff,
                            exponent: *exponent,
                        },
                        OutletRating::Tabular { curve } => {
                            OutRating::Table(net.curves[*curve].points.clone())
                        }
                    },
                    by_head_difference: *head_basis == OutletHeadBasis::Head,
                    flap: *flap_gate,
                },
            };
            let eq = match &kind {
                StructKind::Orifice { sec, .. } | StructKind::Weir { sec, .. } => eq_length(sec),
                _ => 0.0,
            };
            pump_on.push(true);
            structs.push(Structure {
                from: link.from,
                to: link.to,
                link: li,
                off1,
                eq_length: eq,
                kind,
            });
        }

        // Crowns.
        for c in &chans {
            let crown1 = c.off1 + c.geom.sec.y_full();
            let crown2 = c.off2 + c.geom.sec.y_full();
            verts[c.from].crown = verts[c.from].crown.max(crown1);
            verts[c.to].crown = verts[c.to].crown.max(crown2);
        }

        let nv = verts.len();
        let nc = chans.len();
        let ns = structs.len();
        let mut r = Router {
            chans,
            verts,
            structs,
            ids: net.vertices.iter().map(|v| v.id.clone()).collect(),
            dt_user: net.options.routing_step,
            courant_factor: net.options.courant_factor,
            max_trials: net.options.max_trials.max(2),
            head_tol: net.options.head_tol,
            min_surface_area: net.options.min_surface_area,
            continuity_tol: net.options.continuity_tol,
            err_tol: net.options.routing_err_tol,
            allow_ponding: net.options.allow_ponding,
            normal_flow: net.options.normal_flow,
            t: 0.0,
            y: vec![0.0; nv],
            q: vec![0.0; nc],
            sq: vec![0.0; ns],
            pump_on,
            sett: vec![1.0; ns],
            chan_open: vec![true; nc],
            struct_flip_t: vec![0.0; ns],
            chan_flip_t: vec![0.0; nc],
            a_mid: vec![0.0; nc],
            net_flow: vec![0.0; nv],
            flood_now: vec![0.0; nv],
            hist: Vec::new(),
            dt_prev: DT_FLOOR,
            quiet_streak: 0,
            evap_rate: 0.0,
            report: RoutingReport::default(),
        };
        r.seed_initial_state(net);
        Ok(r)
    }

    /// §6.7 initial conditions.
    fn seed_initial_state(&mut self, net: &Network) {
        // Channel initial flows imply normal depth.
        let mut end_depth = vec![0.0_f64; self.chans.len()];
        for (ci, c) in self.chans.iter().enumerate() {
            let LinkKind::Channel { init_flow, .. } = &net.links[c.link].kind else {
                continue;
            };
            if *init_flow > 0.0 {
                self.q[ci] = *init_flow;
                let per_barrel = init_flow / c.barrels;
                let psi = c.n * per_barrel / c.slope.sqrt();
                end_depth[ci] = c.geom.sec.normal_depth(psi).unwrap_or(c.geom.sec.y_full());
            }
        }
        // Vertex seeding at non-outfall, non-storage vertices without a
        // supplied depth.
        let mut sum = vec![0.0_f64; self.verts.len()];
        let mut count = vec![0_u32; self.verts.len()];
        for (ci, c) in self.chans.iter().enumerate() {
            sum[c.from] += end_depth[ci] + c.off1;
            count[c.from] += 1;
            sum[c.to] += end_depth[ci] + c.off2;
            count[c.to] += 1;
        }
        for (vi, v) in net.vertices.iter().enumerate() {
            let supplied = match &v.kind {
                VertexKind::Junction { init_depth, .. }
                | VertexKind::Storage { init_depth, .. } => *init_depth,
                _ => 0.0,
            };
            if supplied > 0.0 {
                self.y[vi] = supplied;
            } else if count[vi] > 0 && matches!(self.verts[vi].class, VertClass::Junction) {
                self.y[vi] = sum[vi] / f64::from(count[vi]);
            }
        }
        // Channels without an initial flow take the mean of their end
        // depths; every channel records its starting mid area.
        for (ci, c) in self.chans.iter().enumerate() {
            let y1 = (self.y[c.from] - c.off1).max(0.0);
            let y2 = (self.y[c.to] - c.off2).max(0.0);
            let y_mid = 0.5 * (y1 + y2);
            self.a_mid[ci] = c.geom.area(y_mid.max(DRY)).max(DRY);
        }
    }

    /// Current simulation time (s).
    pub fn time(&self) -> f64 {
        self.t
    }

    /// Set a fixed-stage outfall's stage elevation (m) — the session's
    /// handle for tidal and series boundaries (§10.1).
    pub fn set_outfall_stage(&mut self, vi: usize, elev: f64) {
        if let VertClass::Outfall(Boundary::Fixed(e)) = &mut self.verts[vi].class {
            *e = elev;
        }
    }

    /// Advance the clock without routing: the §10.3 between-events freeze.
    /// State and ledgers hold; the head history resets so the error
    /// estimate restarts cleanly after the gap.
    pub fn skip_to(&mut self, t: f64) {
        if t > self.t {
            self.t = t;
            self.hist.clear();
            self.dt_prev = DT_FLOOR;
        }
    }

    /// Depth at a vertex (m).
    pub fn depth(&self, v: usize) -> f64 {
        self.y[v]
    }

    /// The last accepted step size (s), the §9.2 PID clock.
    pub fn last_dt(&self) -> f64 {
        self.dt_prev
    }

    /// Channel transport states for §8.4: (model link, from-vertex,
    /// to-vertex, flow m³/s in router orientation, stored volume m³).
    pub fn channel_transport(&self) -> Vec<(usize, usize, usize, f64, f64)> {
        self.chans
            .iter()
            .enumerate()
            .map(|(ci, c)| {
                let vol = self.a_mid[ci] * c.length * c.barrels;
                (c.link, c.from, c.to, self.q[ci], vol)
            })
            .collect()
    }

    /// Structure transport states for §8.4: (model link, from-vertex,
    /// to-vertex, flow m³/s). Structures hold no volume — they pass their
    /// upstream vertex's concentration through.
    pub fn structure_transport(&self) -> Vec<(usize, usize, usize, f64)> {
        self.structs
            .iter()
            .enumerate()
            .map(|(si, st)| (st.link, st.from, st.to, self.sq[si]))
            .collect()
    }

    /// Vertex `vi`'s flooding rate at the last accepted step (m³/s).
    pub fn flood_rate(&self, vi: usize) -> f64 {
        self.flood_now.get(vi).copied().unwrap_or(0.0)
    }

    /// The system outflow rate at the last accepted step (m³/s): the sum
    /// of positive net flows into outfall vertices.
    pub fn outflow_rate(&self) -> f64 {
        self.verts
            .iter()
            .enumerate()
            .filter(|(_, v)| matches!(v.class, VertClass::Outfall(_)))
            .map(|(vi, _)| self.net_flow[vi].max(0.0))
            .sum()
    }

    /// Whether vertex `vi` is an outfall: discharge leaves the system.
    pub fn is_outfall(&self, vi: usize) -> bool {
        matches!(self.verts[vi].class, VertClass::Outfall(_))
    }

    /// Whether vertex `vi` is a storage unit (§8.5 residence time).
    pub fn is_storage(&self, vi: usize) -> bool {
        matches!(self.verts[vi].class, VertClass::Storage(_))
    }

    /// Vertex `vi`'s current free-surface area (m²): the storage
    /// geometry's, else the §6.3 nominal minimum.
    pub fn vertex_area_now(&self, vi: usize) -> f64 {
        match &self.verts[vi].class {
            VertClass::Storage(g) => g.area(self.y[vi]),
            _ => self.min_surface_area,
        }
    }

    /// Water depth in model link `li` (m): a channel's mid-depth, a
    /// structure's head above its crest, for the §9.1 premises.
    pub fn link_depth(&self, li: usize) -> Option<f64> {
        if let Some((ci, c)) = self.chans.iter().enumerate().find(|(_, c)| c.link == li) {
            let _ = ci;
            let y1 = (self.y[c.from] - c.off1).max(0.0);
            let y2 = (self.y[c.to] - c.off2).max(0.0);
            return Some((0.5 * (y1 + y2)).min(c.geom.sec.y_full()));
        }
        self.structs
            .iter()
            .find(|s| s.link == li)
            .map(|st| (self.y[st.from] - st.off1).max(0.0))
    }

    /// Flow velocity in channel `li` (m/s); `None` for structures, whose
    /// premise then reads inapplicable (§9.1).
    pub fn link_velocity(&self, li: usize) -> Option<f64> {
        self.chans
            .iter()
            .enumerate()
            .find(|(_, c)| c.link == li)
            .map(|(ci, c)| {
                let a = self.a_mid[ci].max(DRY);
                (self.q[ci] / c.barrels / a).clamp(-V_MAX, V_MAX)
            })
    }

    /// Channel-only §9.1 observables: Manning full-section flow, full
    /// depth, length, and slope.
    pub fn chan_full_attrs(&self, li: usize) -> Option<(f64, f64, f64, f64)> {
        self.chans.iter().find(|c| c.link == li).map(|c| {
            let y_full = c.geom.sec.y_full();
            let a = c.geom.sec.area(y_full);
            let r = c.geom.sec.hyd_radius(y_full);
            let q_full = a * r.powf(2.0 / 3.0) * c.slope.sqrt() / c.n * c.barrels;
            (q_full, y_full, c.length, c.slope)
        })
    }

    /// Stored volume at vertex `vi` (m³): storage geometry, else depth
    /// times the assembled surface area of the last accepted step.
    pub fn vertex_volume_now(&self, vi: usize) -> f64 {
        match &self.verts[vi].class {
            VertClass::Storage(g) => g.volume(self.y[vi]),
            _ => {
                // Junctions hold the §6.3 nominal store.
                self.y[vi].max(0.0) * self.min_surface_area
            }
        }
    }

    /// The vertex's §14.7 maximum (rim) depth (m).
    pub fn vertex_max_depth(&self, vi: usize) -> f64 {
        self.verts[vi].y_max
    }

    /// The vertex invert elevation (m).
    pub fn vertex_invert(&self, vi: usize) -> f64 {
        self.verts[vi].invert
    }

    /// Set model link `li`'s operational target (§9): pump speed,
    /// orifice/weir opening fraction, outlet scale, or channel
    /// open (1) / closed (0). Returns `Some(changed)`, `None` for a link
    /// the router does not carry.
    pub fn set_setting(&mut self, li: usize, value: f64) -> Option<bool> {
        if let Some(ci) = self.chans.iter().position(|c| c.link == li) {
            let open = value > 0.0;
            let changed = self.chan_open[ci] != open;
            if changed {
                self.chan_open[ci] = open;
                self.chan_flip_t[ci] = self.t;
            }
            return Some(changed);
        }
        if let Some(si) = self.structs.iter().position(|s| s.link == li) {
            let changed = self.sett[si] != value;
            if changed {
                if (self.sett[si] > 0.0) != (value > 0.0) {
                    self.struct_flip_t[si] = self.t;
                }
                self.sett[si] = value;
            }
            return Some(changed);
        }
        None
    }

    /// Model link `li`'s current §9 setting.
    pub fn setting(&self, li: usize) -> Option<f64> {
        if let Some(ci) = self.chans.iter().position(|c| c.link == li) {
            return Some(if self.chan_open[ci] { 1.0 } else { 0.0 });
        }
        self.structs
            .iter()
            .position(|s| s.link == li)
            .map(|si| self.sett[si])
    }

    /// Time model link `li` has spent in its current open/closed status
    /// (s), for the TIMEOPEN/TIMECLOSED premises (§9.1). Pump latches
    /// count as status flips too.
    pub fn time_in_status(&self, li: usize) -> Option<f64> {
        if let Some(ci) = self.chans.iter().position(|c| c.link == li) {
            return Some(self.t - self.chan_flip_t[ci]);
        }
        self.structs
            .iter()
            .position(|s| s.link == li)
            .map(|si| self.t - self.struct_flip_t[si])
    }

    /// Whether model link `li` is currently open: a positive setting,
    /// and for pumps the §7.1 latch as well.
    pub fn is_open(&self, li: usize) -> Option<bool> {
        if let Some(ci) = self.chans.iter().position(|c| c.link == li) {
            return Some(self.chan_open[ci]);
        }
        self.structs.iter().position(|s| s.link == li).map(|si| {
            let latched_off =
                matches!(self.structs[si].kind, StructKind::Pump { .. }) && !self.pump_on[si];
            self.sett[si] > 0.0 && !latched_off
        })
    }

    /// Flow in the element routed for model link `li` (m³/s), in the
    /// user's orientation.
    pub fn flow(&self, li: usize, net: &Network) -> f64 {
        if let Some(ci) = self.chans.iter().position(|c| c.link == li) {
            let q = self.q[ci];
            return match &net.links[li].kind {
                LinkKind::Channel { reversed: true, .. } => -q,
                _ => q,
            };
        }
        if let Some(si) = self.structs.iter().position(|s| s.link == li) {
            return self.sq[si];
        }
        0.0
    }

    /// Advance to `t_end`, drawing per-vertex lateral inflows (m³/s) from
    /// `lateral(t, &mut flows)` at the start of each trial.
    pub fn advance(&mut self, t_end: f64, lateral: &dyn Fn(f64, &mut [f64])) {
        let mut lat = vec![0.0; self.verts.len()];
        while self.t < t_end - 1e-9 {
            lateral(self.t, &mut lat);
            self.step_once(t_end, &lat);
        }
    }

    /// Advance by exactly one accepted step toward `t_end`, under the
    /// §6.5 transaction rules, with `lat` the per-vertex lateral inflows
    /// held for the step. Rule evaluation (§9) sits between calls.
    pub fn step_once(&mut self, t_end: f64, lat: &[f64]) {
        self.update_pump_latches();
        let mut dt = self.seed_step().min(t_end - self.t);
        loop {
            let trial = self.run_trial(dt, lat);
            let at_floor = dt <= DT_FLOOR + 1e-12;
            let ok = trial.converged && (self.err_tol <= 0.0 || trial.err <= self.err_tol);
            if ok || at_floor {
                if !ok {
                    self.report
                        .degraded
                        .push((self.t + dt, self.ids[trial.worst_vertex].clone()));
                }
                self.accept(dt, trial, lat);
                break;
            }
            self.report.rejected += 1;
            self.quiet_streak = 0;
            dt = (0.5 * dt).max(DT_FLOOR);
        }
    }

    /// §6.5 step seeding.
    fn seed_step(&mut self) -> f64 {
        if self.report.accepted == 0 {
            return DT_FLOOR;
        }
        let mut dt = self.dt_user.min(2.0 * self.dt_prev);
        // Vertex quarter-crown rate constraint.
        if let Some((t_prev, y_prev)) = self.hist.last() {
            let span = self.t - t_prev;
            if span > 0.0 {
                for (vi, v) in self.verts.iter().enumerate() {
                    if matches!(v.class, VertClass::Outfall(_)) {
                        continue;
                    }
                    let y = self.y[vi];
                    if y <= DRY || (v.crown > 0.0 && y > v.crown) {
                        continue;
                    }
                    let rate = (y - y_prev[vi]).abs() / span;
                    if rate > 1e-12 && v.crown > 0.0 {
                        dt = dt.min(0.25 * v.crown / rate);
                    }
                }
            }
        }
        // Channel Courant term, unless quiescence released it.
        if self.courant_factor > 0.0 && self.quiet_streak < 3 {
            for (ci, c) in self.chans.iter().enumerate() {
                let q = self.q[ci] / c.barrels;
                let a = self.a_mid[ci];
                if a <= DRY || q.abs() <= Q_DRY {
                    continue;
                }
                let y1 = (self.y[c.from] - c.off1).max(0.0);
                let y2 = (self.y[c.to] - c.off2).max(0.0);
                // Closed channels flowing full are exempt: their wave is
                // the slot celerity itself (§6.5).
                if c.geom.w_slot > 0.0 && y1 >= c.geom.sec.y_full() && y2 >= c.geom.sec.y_full() {
                    continue;
                }
                let y_mid = 0.5 * (y1 + y2);
                let w = c.geom.width(y_mid.max(DRY)).max(1e-9);
                let u = (q / a).abs();
                let cel = (GRAVITY * a / w).sqrt();
                let fr = u / (GRAVITY * (a / w)).sqrt().max(1e-12);
                if fr <= 0.01 {
                    continue;
                }
                dt = dt.min(self.courant_factor * c.length / (u + cel));
            }
        }
        dt.max(DT_FLOOR)
    }

    fn accept(&mut self, dt: f64, trial: Trial, lat: &[f64]) {
        // Ledger.
        for (vi, l) in lat.iter().enumerate() {
            self.report.inflow += l * dt;
            self.report.flooding += trial.flood_rate[vi] * dt;
        }
        self.flood_now.clone_from(&trial.flood_rate);
        // Outfall discharge integrates the same trapezoid the vertex
        // update used.
        for (vi, v) in self.verts.iter().enumerate() {
            if matches!(v.class, VertClass::Outfall(_)) {
                let old = self.net_flow[vi].max(0.0);
                let new = trial.net_flow[vi].max(0.0);
                self.report.outflow += 0.5 * (old + new) * dt;
            }
        }
        // Quiescence bookkeeping (§6.5).
        if self.err_tol > 0.0 && trial.err < 0.25 * self.err_tol {
            self.quiet_streak = self.quiet_streak.saturating_add(1);
        } else {
            self.quiet_streak = 0;
        }
        // History for the error estimate.
        self.hist.push((self.t, std::mem::take(&mut self.y)));
        if self.hist.len() > 2 {
            self.hist.remove(0);
        }
        self.t += dt;
        self.dt_prev = dt;
        self.y = trial.y;
        self.q = trial.q;
        self.sq = trial.sq;
        self.a_mid = trial.a_mid;
        self.net_flow = trial.net_flow;
        self.report.losses += trial.loss_rate * dt;
        self.report.accepted += 1;
    }

    /// One §6.4 trial: iterate channel and vertex phases to
    /// self-consistency over the interval `dt`.
    fn run_trial(&self, dt: f64, lat: &[f64]) -> Trial {
        let nv = self.verts.len();
        let nc = self.chans.len();
        let mut y = self.y.clone();
        let mut q = self.q.clone();
        let mut sq = self.sq.clone();
        let mut a_mid_new = self.a_mid.clone();
        let mut converged = false;
        let mut net_new = vec![0.0; nv];
        let mut flood = vec![0.0; nv];
        let mut surf = vec![0.0; nv];
        let mut loss_total = 0.0;

        for step in 0..self.max_trials {
            // ── Channel phase (∥): flows from the last iterate ─────────
            surf.iter_mut().for_each(|s| *s = 0.0);
            net_new.iter_mut().for_each(|s| *s = 0.0);
            let mut q_next = vec![0.0; nc];
            loss_total = 0.0;
            for ci in 0..nc {
                let (qn, a_mid, s1, s2, loss) = self.channel_flow(ci, &y, q[ci], dt, step);
                q_next[ci] = qn;
                a_mid_new[ci] = a_mid;
                let c = &self.chans[ci];
                surf[c.from] += s1;
                surf[c.to] += s2;
                let qt = qn; // total flow (barrels folded inside)
                net_new[c.from] -= qt;
                net_new[c.to] += qt;
                // Evaporation and seepage debit the end vertices, halved
                // between them; outfalls do not share (§7.7).
                if loss > 0.0 {
                    loss_total += loss;
                    let out1 = matches!(self.verts[c.from].class, VertClass::Outfall(_));
                    let out2 = matches!(self.verts[c.to].class, VertClass::Outfall(_));
                    match (out1, out2) {
                        (false, false) => {
                            net_new[c.from] -= loss / 2.0;
                            net_new[c.to] -= loss / 2.0;
                        }
                        (false, true) => net_new[c.from] -= loss,
                        (true, false) => net_new[c.to] -= loss,
                        (true, true) => {}
                    }
                }
            }
            for (vi, l) in lat.iter().enumerate() {
                net_new[vi] += l;
            }

            // ── Structure phase: against the last iterate's state ──────
            // Positive arrivals per vertex from this iterate's channel
            // flows, the laterals, and the *previous* iterate's structure
            // flows — no structure sees another's running result (§6.4).
            let mut pos_in = vec![0.0; nv];
            for (vi, l) in lat.iter().enumerate() {
                pos_in[vi] += l.max(0.0);
            }
            for (ci, c) in self.chans.iter().enumerate() {
                let qt = q_next[ci];
                if qt >= 0.0 {
                    pos_in[c.to] += qt;
                } else {
                    pos_in[c.from] -= qt;
                }
            }
            for (si, st) in self.structs.iter().enumerate() {
                let qt = sq[si];
                if qt >= 0.0 {
                    pos_in[st.to] += qt;
                } else {
                    pos_in[st.from] -= qt;
                }
            }
            let mut sq_next = vec![0.0; self.structs.len()];
            for si in 0..self.structs.len() {
                let (qn, s1, s2) =
                    self.structure_flow(si, &y, sq[si], dt, step, &pos_in, &net_new, &surf);
                sq_next[si] = qn;
                let st = &self.structs[si];
                surf[st.from] += s1;
                surf[st.to] += s2;
                net_new[st.from] -= qn;
                net_new[st.to] += qn;
            }
            q = q_next;
            sq = sq_next;

            // ── Vertex phase ───────────────────────────────────────────
            let mut max_dy = 0.0_f64;
            let mut residual = 0.0_f64;
            let mut flow_scale = 0.0_f64;
            flood.iter_mut().for_each(|f| *f = 0.0);
            for vi in 0..nv {
                let v = &self.verts[vi];
                match &v.class {
                    VertClass::Outfall(b) => {
                        // Boundary depth from the connecting channel.
                        let y_new = self.outfall_depth(vi, b, &q);
                        max_dy = max_dy.max((y_new - y[vi]).abs());
                        y[vi] = y_new;
                    }
                    _ => {
                        let mut area = surf[vi];
                        if let VertClass::Storage(g) = &v.class {
                            area += g.area(y[vi]);
                        }
                        let ponded = self.allow_ponding && v.ponded_area > 0.0 && y[vi] > v.y_max;
                        if ponded {
                            area = v.ponded_area;
                        }
                        let area = area.max(self.min_surface_area);
                        let dv = 0.5 * (self.net_flow[vi] + net_new[vi]) * dt;
                        let mut y_new = self.y[vi] + dv / area;
                        // Under-relax below the crown (§6.4).
                        if step > 0 && !(v.crown > 0.0 && y[vi] > v.crown) {
                            y_new = (1.0 - OMEGA) * y[vi] + OMEGA * y_new;
                        }
                        if y_new < 0.0 {
                            y_new = 0.0;
                        }
                        // Flooding: pin and report the surplus (§6.6).
                        let cap = v.y_max + v.surcharge;
                        if y_new > cap && !(self.allow_ponding && v.ponded_area > 0.0) {
                            flood[vi] = (y_new - cap) * area / dt;
                            y_new = cap;
                        }
                        max_dy = max_dy.max((y_new - y[vi]).abs());
                        y[vi] = y_new;
                        // Continuity residual for criterion 2 (§6.4).
                        let stored = area * (y_new - self.y[vi]) / dt;
                        residual +=
                            (0.5 * (self.net_flow[vi] + net_new[vi]) - stored - flood[vi]).abs();
                    }
                }
            }
            for qi in &q {
                flow_scale += qi.abs();
            }
            for qi in &sq {
                flow_scale += qi.abs();
            }
            for l in lat {
                flow_scale += l.abs();
            }
            if step >= 1
                && max_dy <= self.head_tol
                && residual <= self.continuity_tol * flow_scale.max(Q_DRY)
            {
                converged = true;
                break;
            }
        }

        // §6.5 error estimate from the head history.
        let (err, worst) = self.error_estimate(dt, &y);
        Trial {
            y,
            q,
            sq,
            loss_rate: loss_total,
            a_mid: a_mid_new,
            net_flow: net_new,
            converged,
            flood_rate: flood,
            worst_vertex: worst,
            err,
        }
    }

    fn error_estimate(&self, dt: f64, y_new: &[f64]) -> (f64, usize) {
        // Zero until two steps have been accepted (§6.5): the estimate
        // spans the previous accepted state, the current one, and the
        // candidate.
        let Some((ta, ya)) = self.hist.last() else {
            return (0.0, 0);
        };
        let (ta, ya) = (*ta, ya);
        let (tb, yb) = (self.t, &self.y);
        let tc = self.t + dt;
        let mut worst = 0;
        let mut e_max = 0.0;
        for vi in 0..y_new.len() {
            if matches!(self.verts[vi].class, VertClass::Outfall(_)) {
                continue;
            }
            let d1 = (yb[vi] - ya[vi]) / (tb - ta);
            let d2 = (y_new[vi] - yb[vi]) / (tc - tb);
            let second = 2.0 * (d2 - d1) / (tc - ta);
            let e = 0.5 * dt * dt * second.abs();
            if e > e_max {
                e_max = e;
                worst = vi;
            }
        }
        (e_max, worst)
    }

    /// §7.1 startup/shutoff latching, evaluated from the accepted state
    /// so retried trials see the same pump states.
    fn update_pump_latches(&mut self) {
        for (si, st) in self.structs.iter().enumerate() {
            let StructKind::Pump {
                startup, shutoff, ..
            } = &st.kind
            else {
                continue;
            };
            let y1 = self.y[st.from];
            if *shutoff > 0.0 && self.pump_on[si] && y1 < *shutoff {
                self.pump_on[si] = false;
                self.struct_flip_t[si] = self.t;
            }
            if *startup > 0.0 && !self.pump_on[si] && y1 > *startup {
                self.pump_on[si] = true;
                self.struct_flip_t[si] = self.t;
            }
        }
    }

    /// One §7 structure's flow from the last iterate's state. Returns the
    /// flow and the equivalent-pipe surface-area contributions.
    #[allow(clippy::too_many_arguments)]
    fn structure_flow(
        &self,
        si: usize,
        y_vert: &[f64],
        sq_last: f64,
        dt: f64,
        step: u32,
        pos_in: &[f64],
        net_chan: &[f64],
        surf: &[f64],
    ) -> (f64, f64, f64) {
        let st = &self.structs[si];
        let h1v = self.verts[st.from].invert + y_vert[st.from];
        let h2v = self.verts[st.to].invert + y_vert[st.to];

        let (mut q, s1, s2) = match &st.kind {
            StructKind::Dummy { q_limit } => {
                let mut q = pos_in[st.from];
                if *q_limit > 0.0 {
                    q = q.min(*q_limit);
                }
                (q, 0.0, 0.0)
            }
            StructKind::Pump { kind, .. } => {
                let speed = self.sett[si];
                if !self.pump_on[si] || speed <= 0.0 {
                    return (0.0, 0.0, 0.0);
                }
                let y1 = y_vert[st.from];
                let mut q = match kind {
                    PumpKind::Ideal => pos_in[st.from],
                    PumpKind::Volume(pts) => {
                        interval_lookup(pts, self.vertex_volume(st.from, y1, surf))
                    }
                    PumpKind::Depth(pts) => interval_lookup(pts, y1),
                    PumpKind::InlineDepth(pts) => linear_lookup(pts, y1),
                    PumpKind::Head { points, affinity } => {
                        // Type 5 rescales its rated curve by the affinity
                        // laws: head by ω², flow by ω (§7.1).
                        let sp = if *affinity { speed } else { 1.0 };
                        let head = (h2v - h1v).max(0.0) / (sp * sp);
                        linear_lookup(points, head)
                    }
                };
                // Reverse flow is never admitted (§7.1); the speed
                // setting scales the characteristic (§7.1).
                q = q.max(0.0) * speed;
                // Inlet clamps (§7.1): a storage vertex — or the virtual
                // wet well of a volume-driven pump — cannot be drawn
                // below empty; other types fall back to the inflow when
                // the projected depth would go negative. The clamp
                // covers every depth- and head-driven type.
                let is_storage = matches!(self.verts[st.from].class, VertClass::Storage(_));
                if is_storage || matches!(kind, PumpKind::Volume(_)) {
                    let vol = self.vertex_volume(st.from, y1, surf);
                    q = q.min(pos_in[st.from] + vol / dt).max(0.0);
                } else if !matches!(kind, PumpKind::Ideal) {
                    let area = surf[st.from].max(self.min_surface_area);
                    let net = net_chan[st.from] - q;
                    let projected =
                        self.y[st.from] + 0.5 * (self.net_flow[st.from] + net) * dt / area;
                    if projected <= 0.0 {
                        q = pos_in[st.from];
                    }
                }
                // Pumps are exempt from relaxation and contribute no
                // surface area (§7.1, §6.4).
                return (q, 0.0, 0.0);
            }
            StructKind::Orifice {
                bottom,
                cd,
                sec,
                flap,
            } => self.orifice_flow(
                st,
                *bottom,
                *cd,
                sec,
                *flap,
                self.sett[si],
                h1v,
                h2v,
                y_vert,
            ),
            StructKind::Weir {
                form,
                cd1,
                cd2,
                sec,
                flap,
                end_contractions,
                can_surcharge,
                coeff_curve,
                road_width,
                road_paved,
            } => {
                if *form == WeirForm::Roadway {
                    self.roadway_flow(st, *cd1, sec, *flap, *road_width, *road_paved, h1v, h2v)
                } else {
                    self.weir_flow(
                        st,
                        *form,
                        *cd1,
                        *cd2,
                        sec,
                        *flap,
                        *end_contractions,
                        *can_surcharge,
                        coeff_curve.as_deref(),
                        self.sett[si],
                        h1v,
                        h2v,
                    )
                }
            }
            StructKind::Outlet {
                rating,
                by_head_difference,
                flap,
            } => {
                let dir = if h1v >= h2v { 1.0 } else { -1.0 };
                let (h1, h2, y1) = if dir > 0.0 {
                    (h1v, h2v, y_vert[st.from])
                } else {
                    (h2v, h1v, y_vert[st.to])
                };
                let hcrest = self.verts[st.from].invert + st.off1;
                let head = if *by_head_difference {
                    h1 - h2.max(hcrest)
                } else {
                    h1 - hcrest
                };
                if head <= DRY || y1 <= DRY || (*flap && dir < 0.0) {
                    (0.0, 0.0, 0.0)
                } else {
                    let q = match rating {
                        OutRating::Functional { coeff, exponent } => coeff * head.powf(*exponent),
                        OutRating::Table(pts) => linear_lookup(pts, head),
                    };
                    // Scaled by the §9 setting (§7.4).
                    (dir * q * self.sett[si], 0.0, 0.0)
                }
            }
        };

        // Under-relaxation with the zero-crossing rule; pumps returned
        // above (§6.4).
        if step > 0 {
            q = (1.0 - OMEGA) * sq_last + OMEGA * q;
            if q * sq_last < 0.0 {
                q = Q_REVERSAL * q.signum();
            }
        }
        (q, s1, s2)
    }

    /// A vertex's stored volume: the storage integral, or depth times the
    /// assembled area for the virtual wet well at a junction.
    fn vertex_volume(&self, vi: usize, y: f64, surf: &[f64]) -> f64 {
        match &self.verts[vi].class {
            VertClass::Storage(g) => g.volume(y),
            _ => y.max(0.0) * surf[vi].max(self.min_surface_area),
        }
    }

    /// §7.2: Torricelli with the derived weir transition below the
    /// changeover, Villemonte submergence, and the Armco flap loss.
    #[allow(clippy::too_many_arguments)] // the §7.2 head assembly
    fn orifice_flow(
        &self,
        st: &Structure,
        bottom: bool,
        cd: f64,
        sec: &Section,
        flap: bool,
        setting: f64,
        h1v: f64,
        h2v: f64,
        y_vert: &[f64],
    ) -> (f64, f64, f64) {
        let dir = if h1v >= h2v { 1.0 } else { -1.0 };
        let (h1, h2, y1) = if dir > 0.0 {
            (h1v, h2v, y_vert[st.from])
        } else {
            (h2v, h1v, y_vert[st.to])
        };
        let hcrest = self.verts[st.from].invert + st.off1;
        // A partial §9 setting recomputes the opening from the §5
        // geometry of the open fraction (§7.2).
        let opening = sec.y_full() * setting.clamp(0.0, 1.0);
        if opening <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let a_full = sec.area(opening);
        let root_2g = (2.0 * GRAVITY).sqrt();

        // The changeover height and the derived weir coefficient (§7.2).
        let (h_crit, c_weir);
        if bottom {
            let a_over_l = if sec.is_closed() && sec.w_max().1 == opening {
                // Circular opening.
                opening / 4.0
            } else {
                let w = sec.w_max().1;
                (opening * w) / (2.0 * (opening + w))
            };
            h_crit = cd / 0.414 * a_over_l;
            c_weir = cd * h_crit.sqrt() * a_full * root_2g;
        } else {
            h_crit = opening;
            c_weir = cd * (opening / 2.0).sqrt() * a_full * root_2g;
        }
        let c_orif = cd * a_full * root_2g;

        // Head and submergence fraction.
        let (head, f);
        if bottom {
            head = if h1 < hcrest {
                0.0
            } else if h2 > hcrest {
                h1 - h2
            } else {
                h1 - hcrest
            };
            f = (head / h_crit).min(1.0);
        } else {
            let hcrown = hcrest + opening;
            let hmid = 0.5 * (hcrest + hcrown);
            f = if h1 < hcrown && hcrown > hcrest {
                ((h1 - hcrest) / (hcrown - hcrest)).max(0.0)
            } else {
                1.0
            };
            head = if f < 1.0 {
                h1 - hcrest
            } else if h2 < hmid {
                h1 - hmid
            } else {
                h1 - h2
            };
        }
        if head <= DRY || y1 <= DRY || (flap && dir < 0.0) {
            let s = DRY * st.eq_length / 2.0;
            return (0.0, s, s);
        }

        let flow_at = |head: f64, f: f64| -> f64 {
            if f < 1.0 {
                c_weir * f.powf(1.5)
            } else {
                c_orif * head.sqrt()
            }
        };
        let mut q = flow_at(head, f);
        // Armco flap-gate loss, subtracted and re-solved (§7.2).
        if flap && q > 0.0 {
            let v = q / a_full;
            let h_loss = (4.0 / GRAVITY) * v * v * (-1.15 * v / head.sqrt()).exp();
            if f < 1.0 {
                q = flow_at(head, (f - h_loss / h_crit).max(0.0));
            } else {
                q = flow_at((head - h_loss).max(0.0), f);
            }
        }
        let mut q = dir * q;
        // Villemonte submergence on the weir regime.
        if f < 1.0 && h2 > hcrest && h1 > hcrest {
            let ratio = (h2 - hcrest) / (h1 - hcrest);
            q *= (1.0 - ratio.powf(1.5)).max(0.0).powf(0.385);
        }

        // Equivalent-pipe surface area, halved to each end (§7.2); a
        // storage end supplies its own.
        let area = if bottom {
            a_full
        } else {
            sec.top_width((f * opening).max(DRY)) * st.eq_length
        };
        let mut s1 = area / 2.0;
        let mut s2 = s1;
        if matches!(self.verts[st.from].class, VertClass::Storage(_)) {
            s1 = 0.0;
        }
        if matches!(self.verts[st.to].class, VertClass::Storage(_)) {
            s2 = 0.0;
        }
        (q, s1, s2)
    }

    /// §7.6: the roadway weir — FHWA head-dependent coefficient when the
    /// road width and surface are given, the user's constant otherwise;
    /// submergence lives in the tables' factors, not Villemonte; never
    /// surcharges; contributes no surface area (an embankment stores
    /// nothing).
    #[allow(clippy::too_many_arguments)]
    fn roadway_flow(
        &self,
        st: &Structure,
        cd1: f64,
        sec: &Section,
        flap: bool,
        road_width: f64,
        road_paved: Option<bool>,
        h1v: f64,
        h2v: f64,
    ) -> (f64, f64, f64) {
        let dir = if h1v > h2v { 1.0 } else { -1.0 };
        let (h1, h2) = if dir > 0.0 { (h1v, h2v) } else { (h2v, h1v) };
        let h_road = self.verts[st.from].invert + st.off1;
        let h_up = h1 - h_road;
        if h_up <= DRY || (flap && dir < 0.0) {
            return (0.0, 0.0, 0.0);
        }
        let cd = match road_paved {
            Some(paved) if road_width > 0.0 => {
                roadway_cd(h_up, (h2 - h_road).max(0.0), road_width, paved)
            }
            _ => cd1,
        };
        (dir * cd * sec.w_max().1 * h_up.powf(1.5), 0.0, 0.0)
    }

    /// §7.3: the weir families with end contractions, surcharge to the
    /// equivalent orifice, and Villemonte submergence.
    #[allow(clippy::too_many_arguments)]
    fn weir_flow(
        &self,
        st: &Structure,
        form: WeirForm,
        cd1: f64,
        cd2: f64,
        sec: &Section,
        flap: bool,
        end_contractions: f64,
        can_surcharge: bool,
        coeff_curve: Option<&[(f64, f64)]>,
        setting: f64,
        h1v: f64,
        h2v: f64,
    ) -> (f64, f64, f64) {
        let dir = if h1v > h2v { 1.0 } else { -1.0 };
        let (h1, h2) = if dir > 0.0 { (h1v, h2v) } else { (h2v, h1v) };
        // A partial §9 setting raises the crest by the closed fraction
        // of the opening; the crown stays put (§7.3).
        let setting = setting.clamp(0.0, 1.0);
        let full_opening = sec.y_full();
        let hcrest = self.verts[st.from].invert + st.off1 + (1.0 - setting) * full_opening;
        let opening = full_opening * setting;
        let hcrown = hcrest + opening;
        let mut head = h1 - hcrest;
        if head <= DRY || opening <= 0.0 || (flap && dir < 0.0) {
            return (0.0, 0.0, 0.0);
        }

        // Equivalent-pipe surface area from the wetted opening width
        // (§7.3 — weirs contribute exactly as orifices do).
        let y_open = opening - (hcrown - h1.min(hcrown));
        let area = sec.top_width(y_open.max(DRY)) * st.eq_length;
        let mut s1 = area / 2.0;
        let mut s2 = s1;
        if matches!(self.verts[st.from].class, VertClass::Storage(_)) {
            s1 = 0.0;
        }
        if matches!(self.verts[st.to].class, VertClass::Storage(_)) {
            s2 = 0.0;
        }

        let eval = |head: f64, dir: f64| -> (f64, f64) {
            let cd1 = match coeff_curve {
                Some(pts) => linear_lookup(pts, head),
                None => cd1,
            };
            match form {
                WeirForm::Transverse | WeirForm::Roadway => {
                    let le = (sec.w_max().1 - 0.1 * end_contractions * head).max(0.0);
                    (cd1 * le * head.powf(1.5), 0.0)
                }
                WeirForm::SideFlow => {
                    let le = (sec.w_max().1 - 0.1 * end_contractions * head).max(0.0);
                    if dir < 0.0 {
                        // Reverts to the transverse form under reverse
                        // flow (§7.3).
                        (cd1 * le * head.powf(1.5), 0.0)
                    } else {
                        (cd1 * le.powf(0.83) * head.powf(1.67), 0.0)
                    }
                }
                WeirForm::VNotch => {
                    let slope = sec.w_max().1 / (2.0 * full_opening);
                    (cd1 * slope * head.powf(2.5), 0.0)
                }
                WeirForm::Trapezoidal => {
                    let bottom = sec.top_width(0.0);
                    let slope = (sec.w_max().1 - bottom) / (2.0 * full_opening);
                    (cd1 * bottom * head.powf(1.5), cd2 * slope * head.powf(2.5))
                }
            }
        };

        // Above the crown: the equivalent orifice, its coefficient fixed
        // by the changeover (§7.3).
        if h1 >= hcrown {
            if can_surcharge {
                let hmid = 0.5 * (hcrest + hcrown);
                head = if h2 < hmid { h1 - hmid } else { h1 - h2 };
                let (q1f, q2f) = eval(opening, dir);
                let c_surcharge = (q1f + q2f) / (opening / 2.0).sqrt();
                return (dir * c_surcharge * head.max(0.0).sqrt(), s1, s2);
            }
            head = opening;
        }

        let (mut q1, mut q2) = eval(head, dir);
        // Villemonte submergence, the end sections always on the V-notch
        // exponent (§7.3).
        if h2 > hcrest {
            let ratio = (h2 - hcrest) / (h1 - hcrest);
            let p = match form {
                WeirForm::SideFlow => 5.0 / 3.0,
                WeirForm::VNotch => 2.5,
                _ => 1.5,
            };
            q1 *= (1.0 - ratio.powf(p)).max(0.0).powf(0.385);
            if q2 > 0.0 {
                q2 *= (1.0 - ratio.powf(2.5)).max(0.0).powf(0.385);
            }
        }
        (dir * (q1 + q2), s1, s2)
    }

    fn outfall_depth(&self, vi: usize, b: &Boundary, q: &[f64]) -> f64 {
        match b {
            Boundary::Fixed(stage) => (stage - self.verts[vi].invert).max(0.0),
            Boundary::Free | Boundary::Normal => {
                // The single connecting channel governs.
                let Some((ci, c)) = self
                    .chans
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.from == vi || c.to == vi)
                else {
                    return 0.0;
                };
                let per_barrel = (q[ci] / c.barrels).abs();
                if per_barrel <= Q_DRY {
                    return 0.0;
                }
                let yc = c.geom.sec.critical_depth(per_barrel);
                let yn = c
                    .geom
                    .sec
                    .normal_depth(c.n * per_barrel / c.slope.sqrt())
                    .unwrap_or(c.geom.sec.y_full());
                let y = match b {
                    Boundary::Free => yc.min(yn),
                    _ => yn,
                };
                y.min(c.geom.sec.y_full())
            }
        }
    }

    /// The §6.3 channel update: one flow value from the last iterate's
    /// heads. Returns (total flow, mid area per barrel, upstream and
    /// downstream surface-area contributions, total loss rate).
    #[allow(clippy::too_many_lines)]
    fn channel_flow(
        &self,
        ci: usize,
        y_vert: &[f64],
        q_total_last: f64,
        dt: f64,
        step: u32,
    ) -> (f64, f64, f64, f64, f64) {
        let c = &self.chans[ci];
        let sec = &c.geom;
        let y_full = sec.sec.y_full();
        let q_old = self.q[ci] / c.barrels;
        let q_last = q_total_last / c.barrels;
        let z1 = c.z1(&self.verts);
        let z2 = c.z2(&self.verts);
        let mut h1 = (self.verts[c.from].invert + y_vert[c.from]).max(z1);
        let mut h2 = (self.verts[c.to].invert + y_vert[c.to]).max(z2);
        let mut y1 = (h1 - z1).max(DRY);
        let mut y2 = (h2 - z2).max(DRY);
        let a_old = self.a_mid[ci].max(DRY);

        // ── Flow classification and surface-area assembly (§6.6) ───────
        let class = self.flow_class(ci, q_last, h1, h2, y1, y2);
        let (s1, s2);
        {
            let (cls_s1, cls_s2, ny1, ny2, nh1, nh2) =
                self.assemble_surface(ci, class, y1, y2, q_last);
            s1 = cls_s1 * c.barrels;
            s2 = cls_s2 * c.barrels;
            y1 = ny1;
            y2 = ny2;
            if let Some(h) = nh1 {
                h1 = h;
            }
            if let Some(h) = nh2 {
                h2 = h;
            }
        }

        let a1 = sec.area(y1);
        let a2 = sec.area(y2);
        let r1 = sec.hyd_radius(y1);
        let y_mid = 0.5 * (y1 + y2);
        let a_mid = sec.area(y_mid);
        let r_mid = sec.hyd_radius(y_mid);
        let is_full = y1 >= y_full && y2 >= y_full;

        // Dry channels carry no flow this trial (§6.6); a channel closed
        // by operational control behaves identically (§9.1).
        if matches!(
            class,
            FlowClass::Dry | FlowClass::UpDry | FlowClass::DownDry
        ) && !is_full
            || a_mid <= DRY
            || !self.chan_open[ci]
        {
            return (0.0, 0.5 * (a1 + a2), s1, s2, 0.0);
        }

        // Velocity, capped (§6.3).
        let mut v = q_last / a_mid;
        if v.abs() > V_MAX {
            v = V_MAX * v.signum();
        }
        let fr = froude(v, a_mid, sec.width(y_mid.max(DRY)));

        // Inertial damping and upstream weighting.
        let mut sigma = if fr <= 0.5 {
            1.0
        } else if fr >= 1.0 {
            0.0
        } else {
            2.0 * (1.0 - fr)
        };
        let rho = if !is_full && q_last > 0.0 && h1 >= h2 {
            sigma
        } else {
            1.0
        };
        let a_wtd = a1 + (a_mid - a1) * rho;
        let r_wtd = r1 + (r_mid - r1) * rho;
        // A closed channel flowing full: slot-wave inertia is not a
        // modelled quantity (§6.3).
        if is_full && sec.w_slot > 0.0 {
            sigma = 0.0;
        }

        // Momentum terms (§6.3); a full force main substitutes its
        // pressurised relation (§7.7).
        let dq_friction = if is_full {
            c.friction
                .pressurised_dq(v, r_mid, dt)
                .unwrap_or_else(|| dt * GRAVITY * c.n * c.n / r_wtd.powf(4.0 / 3.0) * v.abs())
        } else {
            dt * GRAVITY * c.n * c.n / r_wtd.powf(4.0 / 3.0) * v.abs()
        };
        let dq_pressure = dt * GRAVITY * a_wtd * (h2 - h1) / c.length;
        let (mut dq_in1, mut dq_in2) = (0.0, 0.0);
        if sigma > 0.0 {
            dq_in1 = 2.0 * v * (a_mid - a_old) * sigma;
            dq_in2 = dt * v * v * (a2 - a1) / c.length * sigma;
        }
        let mut dq_losses = 0.0;
        if c.loss_inlet > 0.0 || c.loss_outlet > 0.0 || c.loss_avg > 0.0 {
            let qa = q_last.abs();
            let mut losses = 0.0;
            if a1 > DRY {
                losses += c.loss_inlet * qa / a1;
            }
            if a2 > DRY {
                losses += c.loss_outlet * qa / a2;
            }
            if a_mid > DRY {
                losses += c.loss_avg * qa / a_mid;
            }
            dq_losses = losses / 2.0 / c.length * dt;
        }

        // Channel evaporation and seepage (§7.7): uniform lateral
        // losses, capped by the channel's volume this step, with
        // Strelkoff's lateral-outflow momentum term.
        let mut loss_rate = 0.0;
        if c.seepage > 0.0 || self.evap_rate > 0.0 {
            let mut evap = 0.0;
            if self.evap_rate > 0.0 && !c.geom.sec.is_closed() {
                evap = self.evap_rate * c.geom.sec.top_width(y_mid.max(DRY)) * c.length;
            }
            let mut seep = 0.0;
            if c.seepage > 0.0 {
                // Seepage is vertical: its width caps at the depth of
                // maximum width.
                let (yw, _) = c.geom.sec.w_max();
                let w = c.geom.sec.top_width(y_mid.min(yw).max(DRY));
                seep = c.seepage * w * c.length;
            }
            loss_rate = evap + seep;
            let cap = a_mid * c.length / dt;
            if loss_rate > cap {
                loss_rate = cap;
            }
        }
        let dq_seep = 2.5 * v * loss_rate * dt / c.length;

        let denom = 1.0 + dq_friction + dq_losses;
        let mut q_new = (q_old - dq_pressure + dq_in1 + dq_in2 + dq_seep) / denom;

        // Culvert inlet control caps positive, non-full flow (§7.6);
        // otherwise the normal-flow limit applies (§6.6).
        if q_new > 0.0 && !is_full && c.culvert > 0 && c.culvert < tables::CULVERT_PARAMS.len() {
            q_new = culvert_inlet_cap(c, q_new / c.barrels, y1) * c.barrels;
        } else if q_new > 0.0 && y1 < y_full && matches!(class, FlowClass::Subcritical) {
            q_new = self.normal_flow_limit(ci, q_new, y1, y2, a1, r1, fr);
        }

        // Under-relaxation with the zero-crossing rule (§6.4).
        if step > 0 {
            q_new = (1.0 - OMEGA) * q_last + OMEGA * q_new;
            if q_new * q_last < 0.0 {
                q_new = Q_REVERSAL * q_new.signum();
            }
        }
        if c.q_limit > 0.0 && q_new.abs() > c.q_limit {
            q_new = c.q_limit * q_new.signum();
        }
        // Flap gate blocks reverse flow.
        if c.flap_gate && q_new < 0.0 {
            q_new = 0.0;
        }
        // No flow out of a dry vertex (§6.6).
        if q_new > Q_DRY && y_vert[c.from] <= DRY {
            q_new = Q_DRY;
        }
        if q_new < -Q_DRY && y_vert[c.to] <= DRY {
            q_new = -Q_DRY;
        }

        (q_new * c.barrels, a_mid, s1, s2, loss_rate * c.barrels)
    }

    fn flow_class(&self, ci: usize, q: f64, h1: f64, h2: f64, y1: f64, y2: f64) -> FlowClass {
        let c = &self.chans[ci];
        if y1 >= c.geom.sec.y_full() && y2 >= c.geom.sec.y_full() {
            return FlowClass::Subcritical;
        }
        // Outfall ends measure their offsets against the outfall stage.
        let mut z1 = c.off1;
        let mut z2 = c.off2;
        if matches!(self.verts[c.from].class, VertClass::Outfall(_)) {
            z1 = (z1 - self.y[c.from]).max(0.0);
        }
        if matches!(self.verts[c.to].class, VertClass::Outfall(_)) {
            z2 = (z2 - self.y[c.to]).max(0.0);
        }
        let wet1 = y1 > DRY;
        let wet2 = y2 > DRY;
        if wet1 && wet2 {
            if q < 0.0 && z1 > 0.0 {
                let (yn, yc) = self.char_depths(ci, q);
                if y1 < yn.min(yc) {
                    return FlowClass::UpCritical;
                }
            } else if q >= 0.0 && z2 > 0.0 {
                let (yn, yc) = self.char_depths(ci, q);
                if y2 < yn.min(yc) {
                    return FlowClass::DownCritical;
                }
            }
            FlowClass::Subcritical
        } else if !wet1 && !wet2 {
            FlowClass::Dry
        } else if wet2 {
            if h2 < self.verts[c.from].invert + c.off1 {
                FlowClass::UpDry
            } else if z1 > 0.0 {
                FlowClass::UpCritical
            } else {
                FlowClass::Subcritical
            }
        } else if h1 < self.verts[c.to].invert + c.off2 {
            FlowClass::DownDry
        } else if z2 > 0.0 {
            FlowClass::DownCritical
        } else {
            FlowClass::Subcritical
        }
    }

    fn char_depths(&self, ci: usize, q: f64) -> (f64, f64) {
        let c = &self.chans[ci];
        let per_barrel = (q / c.barrels).abs();
        let yn = c
            .geom
            .sec
            .normal_depth(c.n * per_barrel / c.slope.sqrt())
            .unwrap_or(c.geom.sec.y_full());
        let yc = c.geom.sec.critical_depth(per_barrel);
        (yn, yc)
    }

    /// §6.6 surface-area assembly per flow class. Returns per-barrel end
    /// contributions, the class-adjusted end depths, and any substituted
    /// end heads.
    #[allow(clippy::type_complexity)]
    fn assemble_surface(
        &self,
        ci: usize,
        class: FlowClass,
        y1: f64,
        y2: f64,
        q: f64,
    ) -> (f64, f64, f64, f64, Option<f64>, Option<f64>) {
        let c = &self.chans[ci];
        let w = |y: f64| c.geom.width(y.max(DRY));
        let half = c.length / 4.0;
        match class {
            FlowClass::Subcritical => {
                // The fraction between normal and critical depth ramps
                // the downstream contribution.
                let mut fasnh = 1.0;
                if c.off2 > 0.0 && q >= 0.0 && y1 > DRY && y2 > DRY {
                    let (yn, yc) = self.char_depths(ci, q);
                    let (lo, hi) = (yn.min(yc), yn.max(yc));
                    if y2 < hi && y2 >= lo {
                        fasnh = if hi - lo < DRY {
                            0.0
                        } else {
                            (hi - y2) / (hi - lo)
                        };
                    }
                }
                let y_mid = (0.5 * (y1 + y2)).max(DRY);
                (
                    (w(y1) + w(y_mid)) * half,
                    (w(y_mid) + w(y2)) * half * fasnh,
                    y1,
                    y2,
                    None,
                    None,
                )
            }
            FlowClass::UpCritical => {
                let (yn, yc) = self.char_depths(ci, q);
                let y1n = yn.min(yc).max(DRY);
                let y_mid = (0.5 * (y1n + y2)).max(DRY);
                let h1 = self.verts[c.from].invert + c.off1 + y1n;
                (
                    0.0,
                    (w(y_mid) + w(y2)) * c.length * 0.5,
                    y1n,
                    y2,
                    Some(h1),
                    None,
                )
            }
            FlowClass::DownCritical => {
                let (yn, yc) = self.char_depths(ci, q);
                let y2n = yn.min(yc).max(DRY);
                let y_mid = (0.5 * (y1 + y2n)).max(DRY);
                let h2 = self.verts[c.to].invert + c.off2 + y2n;
                (
                    (w(y1) + w(y_mid)) * c.length * 0.5,
                    0.0,
                    y1,
                    y2n,
                    None,
                    Some(h2),
                )
            }
            FlowClass::UpDry => {
                let y1n = DRY;
                let y_mid = (0.5 * (y1n + y2)).max(DRY);
                let s1 = if c.off1 <= 0.0 {
                    (w(y1n) + w(y_mid)) * half
                } else {
                    0.0
                };
                ((s1), (w(y_mid) + w(y2)) * half, y1n, y2, None, None)
            }
            FlowClass::DownDry => {
                let y2n = DRY;
                let y_mid = (0.5 * (y1 + y2n)).max(DRY);
                let s2 = if c.off2 <= 0.0 {
                    (w(y2n) + w(y_mid)) * half
                } else {
                    0.0
                };
                ((w(y1) + w(y_mid)) * half, s2, y1, y2n, None, None)
            }
            FlowClass::Dry => {
                let s = DRY * c.length / 2.0;
                (s, s, y1, y2, None, None)
            }
        }
    }

    #[allow(clippy::too_many_arguments)] // the §6.6 check's natural inputs
    fn normal_flow_limit(
        &self,
        ci: usize,
        q: f64,
        y1: f64,
        y2: f64,
        a1: f64,
        r1: f64,
        fr: f64,
    ) -> f64 {
        let c = &self.chans[ci];
        let has_outfall = matches!(self.verts[c.from].class, VertClass::Outfall(_))
            || matches!(self.verts[c.to].class, VertClass::Outfall(_));
        let mut check = false;
        // Water-surface slope less than bed slope.
        if matches!(
            self.normal_flow,
            NormalFlowCriteria::Slope | NormalFlowCriteria::Both
        ) || has_outfall
        {
            check = y1 < y2;
        }
        // Upstream Froude at or above 1, never at outfall channels.
        if !check
            && matches!(
                self.normal_flow,
                NormalFlowCriteria::Froude | NormalFlowCriteria::Both
            )
            && !has_outfall
            && y1 > DRY
            && y2 > DRY
        {
            check = fr >= 1.0;
        }
        if check {
            let q_norm = c.slope.sqrt() / c.n * a1 * r1.powf(2.0 / 3.0);
            if q_norm < q {
                return q_norm;
            }
        }
        q
    }
}

/// Exact feet-to-metres, and the HDS-5 dimensionless-group scale: the
/// published coefficient forms are English-unit, and the group
/// Q/(A·√(g·D)) is unit-free, so evaluating with `AD = A_full·√(D·0.3048)`
/// reproduces the English-unit group exactly (g_SI/g_E = 0.3048).
const FT: f64 = 0.3048;

/// §7.6 culvert inlet control: the smaller of the dynamic and
/// inlet-control flows governs. `q0` and the result are per barrel; `y`
/// is the inlet depth above the culvert invert.
fn culvert_inlet_cap(c: &Chan, q0: f64, y: f64) -> f64 {
    let (form, k, m, cc, yy) = tables::CULVERT_PARAMS[c.culvert];
    let y_full = c.geom.sec.y_full();
    let ad = c.geom.sec.a_full() * (y_full * FT).sqrt();
    // Slope correction: −0.7·S for mitered inlets (codes 5, 37, 46) at
    // its published magnitude — the predecessor enters −7.0 (§7.6
    // CORRESPONDENCE) — and +0.5·S otherwise.
    let scf = match c.culvert {
        5 | 37 | 46 => -0.7 * c.slope,
        _ => 0.5 * c.slope,
    };
    // Submerged above the FHWA Q/AD > 4 criterion; unsubmerged below
    // 95 % full; a linear transition between.
    let y2 = y_full * (16.0 * cc + yy - scf);
    let q = if y >= y2 {
        culvert_submerged(y, y_full, ad, cc, yy, scf)
    } else {
        let y1 = 0.95 * y_full;
        if y <= y1 {
            culvert_unsubmerged(c, y, y_full, ad, form, k, m, scf)
        } else {
            let qa = culvert_unsubmerged(c, y1, y_full, ad, form, k, m, scf);
            let qb = culvert_submerged(y2, y_full, ad, cc, yy, scf);
            qa + (qb - qa) * (y - y1) / (y2 - y1)
        }
    };
    q0.min(q)
}

fn culvert_submerged(y: f64, y_full: f64, ad: f64, cc: f64, yy: f64, scf: f64) -> f64 {
    let arg = (y / y_full - yy + scf) / cc;
    if arg <= 0.0 {
        return f64::MAX;
    }
    arg.sqrt() * ad
}

#[allow(clippy::too_many_arguments)]
fn culvert_unsubmerged(
    c: &Chan,
    y: f64,
    y_full: f64,
    ad: f64,
    form: f64,
    k: f64,
    m: f64,
    scf: f64,
) -> f64 {
    if form >= 2.0 {
        // Form 2: the direct power law.
        return ad * (y / y_full / k).powf(1.0 / m);
    }
    // Form 1: solve the critical-energy equation for critical depth on
    // [0.01·y, y] by bisection on its bracketed residual; an unbracketed
    // residual falls back to the endpoint evaluation, stated rather than
    // silently seeded (§5.7 discipline).
    let h_plus = y / y_full + scf;
    let residual = |yc: f64| -> (f64, f64) {
        let ac = c.geom.sec.area(yc.min(c.geom.sec.y_full()));
        let wc = c.geom.sec.top_width(yc.min(c.geom.sec.y_full())).max(1e-9);
        let yh = ac / wc;
        let qc = ac * (GRAVITY * yh).sqrt();
        let r = h_plus - yc / y_full - yh / (2.0 * y_full) - k * (qc / ad).powf(m);
        (r, qc)
    };
    let (mut lo, mut hi) = (0.01 * y, y);
    let (r_lo, q_lo) = residual(lo);
    let (r_hi, q_hi) = residual(hi);
    if r_lo * r_hi > 0.0 {
        return if r_lo.abs() < r_hi.abs() { q_lo } else { q_hi };
    }
    let mut q = q_hi;
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        let (r, qc) = residual(mid);
        q = qc;
        if r * r_lo > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    q
}

/// §7.6 roadway-weir coefficient: the FHWA head-dependent value from the
/// digitised English-unit tables, converted at the boundary, with
/// submergence factors floored at their published minima by the tables'
/// own clamped ends.
fn roadway_cd(h_up: f64, h_down: f64, road_width: f64, paved: bool) -> f64 {
    let lookup = |t: &[(f64, f64)], x: f64| -> f64 {
        if x <= t[0].0 {
            return t[0].1;
        }
        for w in t.windows(2) {
            if x <= w[1].0 {
                let f = (x - w[0].0) / (w[1].0 - w[0].0);
                return w[0].1 + f * (w[1].1 - w[0].1);
            }
        }
        t[t.len() - 1].1
    };
    let hl = h_up / road_width;
    let cr_english = if hl <= 0.15 {
        // The low-head table's abscissa is head in feet.
        let h_ft = h_up / FT;
        if paved {
            lookup(&tables::ROADWAY_CR_LOW_PAVED, h_ft)
        } else {
            lookup(&tables::ROADWAY_CR_LOW_GRAVEL, h_ft)
        }
    } else if paved {
        lookup(&tables::ROADWAY_CR_HIGH_PAVED, hl)
    } else {
        lookup(&tables::ROADWAY_CR_HIGH_GRAVEL, hl)
    };
    let kt = if h_down > 0.0 {
        let ratio = h_down / h_up;
        if paved {
            lookup(&tables::ROADWAY_KT_PAVED, ratio)
        } else {
            lookup(&tables::ROADWAY_KT_GRAVEL, ratio)
        }
    } else {
        1.0
    };
    // ft^½/s to m^½/s.
    cr_english * kt * FT.sqrt()
}

/// Build a regulator's opening section.
fn build_struct_section(
    net: &Network,
    li: usize,
    len: f64,
    id: &str,
) -> Result<Section, RouterRefusal> {
    match crate::io::validate::build_for_link(net, li, len) {
        Some(Ok(b)) => Ok(b.section),
        Some(Err(e)) => Err(RouterRefusal::Geometry(format!("{id}: {e:?}"))),
        None => Err(RouterRefusal::Geometry(format!("{id}: no cross-section"))),
    }
}

fn froude(v: f64, a: f64, w: f64) -> f64 {
    if a <= 0.0 || w <= 0.0 {
        return 0.0;
    }
    v.abs() / (GRAVITY * a / w).sqrt().max(1e-12)
}

fn offset_height(o: &Offset) -> f64 {
    match o {
        Offset::Depth(h) => *h,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::objects::parse_network;
    use crate::io::validate::validate;

    pub(super) fn build(input: &str) -> (Network, Router) {
        let (mut net, diags) = parse_network(input);
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "parse refused: {diags:?}"
        );
        let v = validate(&mut net);
        assert!(
            v.iter().all(|f| !f.kind.is_error()),
            "validation refused: {v:?}"
        );
        let router = Router::build(&net).expect("router build");
        (net, router)
    }

    /// Lateral inflow closure: a constant flow at one vertex.
    pub(super) fn inflow_at(v: usize, q: f64) -> impl Fn(f64, &mut [f64]) {
        move |_t, lat: &mut [f64]| {
            lat.iter_mut().for_each(|l| *l = 0.0);
            lat[v] = q;
        }
    }

    const CHAIN: &str = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  10

[JUNCTIONS]
J1  100.4  5
J2  100.2  5

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  J2  200  0.013  0  0
C2  J2  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  3  2  0  0
C2  RECT_OPEN  3  2  0  0
";

    #[test]
    fn steady_flow_settles_at_manning_normal_depth() {
        let (_, mut r) = build(CHAIN);
        let q_in = 0.3;
        r.advance(7200.0, &inflow_at(0, q_in));
        // The whole inflow passes through both channels.
        assert!((r.q[0] - q_in).abs() < 0.01 * q_in, "C1 = {}", r.q[0]);
        assert!((r.q[1] - q_in).abs() < 0.01 * q_in, "C2 = {}", r.q[1]);
        // The interior vertex sits near Manning normal depth: b = 2 m,
        // n = 0.013, S = 0.001 → y_n ≈ 0.202 m.
        let psi = 0.013 * q_in / 0.001_f64.sqrt();
        let yn = r.chans[0].geom.sec.normal_depth(psi).unwrap();
        assert!((0.19..0.21).contains(&yn), "yn = {yn}");
        assert!((r.y[1] - yn).abs() < 0.03, "J2 depth {} vs {yn}", r.y[1]);
        // Conservation where the scheme promises it: over a steady hour,
        // outflow matches inflow to a fraction of a percent. (Across the
        // initial wetting front, state volume and the flow ledger differ
        // by the §11 closure error, as they do in the predecessor.)
        let in_before = r.report.inflow;
        let out_before = r.report.outflow;
        r.advance(10_800.0, &inflow_at(0, q_in));
        let din = r.report.inflow - in_before;
        let dout = r.report.outflow - out_before;
        assert!(
            (din - dout).abs() < 0.005 * din,
            "steady window in {din} out {dout}"
        );
        assert!(r.report.flooding == 0.0);
        // The cold slam start may leave its opening floor trial short of
        // the mass-closure criterion — warned, per §6.5; nothing after.
        assert!(
            r.report.degraded.iter().all(|(t, _)| *t <= 1.0),
            "{:?}",
            r.report.degraded
        );
    }

    #[test]
    fn a_pulse_drains_and_the_ledger_closes() {
        let (_, mut r) = build(CHAIN);
        let pulse = |t: f64, lat: &mut [f64]| {
            lat.iter_mut().for_each(|l| *l = 0.0);
            lat[0] = if t < 600.0 { 0.5 } else { 0.0 };
        };
        r.advance(14_400.0, &pulse);
        let led = &r.report;
        // The pulse edge lands inside one accepted step, so the counted
        // inflow can overshoot by at most one step at half rate.
        assert!((led.inflow - 300.0).abs() < 6.0, "inflow {}", led.inflow);
        // After four hours everything routed out: outflow matches inflow
        // to within a percent (dry remnants account for the rest).
        assert!(
            (led.outflow - led.inflow).abs() < 0.01 * led.inflow,
            "in {} out {}",
            led.inflow,
            led.outflow
        );
    }

    #[test]
    fn surcharge_rises_smoothly_through_the_slot() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.05  10

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  400  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0
";
        let (_, mut r) = build(inp);
        // Far beyond full-flow Manning capacity: over 400 m the free
        // outfall's critical-depth pull cannot make up the friction.
        let q_in = 0.15;
        r.advance(3600.0, &inflow_at(0, q_in));
        assert!((r.q[0] - q_in).abs() < 0.02 * q_in, "q = {}", r.q[0]);
        // The junction surcharges above the pipe crown, smoothly.
        assert!(r.y[0] > 0.5, "not surcharged: {}", r.y[0]);
        // Steady surcharged balance: the head difference across the pipe
        // matches the friction slope of the full pipe.
        let c = &r.chans[0];
        let a = c.geom.sec.a_full();
        let rr = c.geom.sec.r_full();
        let v = q_in / a;
        let sf = c.n * c.n * v * v / rr.powf(4.0 / 3.0);
        let h1 = 100.05 + r.y[0];
        let h2 = 100.0 + r.y[1];
        let dh = h1 - h2;
        assert!(
            (dh - sf * c.length).abs() < 0.15 * sf * c.length,
            "dh {dh} vs Sf·L {}",
            sf * c.length
        );
        // The slot width is the celerity-derived value.
        let w_expect = GRAVITY * a / (50.0 * 50.0);
        assert!((c.geom.w_slot - w_expect).abs() < 1e-12);
        // A cold slam start and the crown crossing may exhaust the trial
        // budget at the floor — accepted with the §6.5 warning; the
        // count stays bounded and the run is otherwise clean.
        assert!(r.report.degraded.len() <= 4, "{:?}", r.report.degraded);
    }

    #[test]
    fn flooding_pins_the_rim_and_reports_the_surplus() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.2  0.6

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.3  0  0  0
";
        let (_, mut r) = build(inp);
        // Far above the little pipe's capacity.
        r.advance(1800.0, &inflow_at(0, 0.2));
        // Depth pinned at the rim (0.6 m, no surcharge allowance).
        assert!((r.y[0] - 0.6).abs() < 1e-9, "y = {}", r.y[0]);
        assert!(r.report.flooding > 0.0);
        // What floods plus what discharges accounts for the inflow.
        let led = &r.report;
        let balance = led.inflow - led.outflow - led.flooding;
        assert!(
            balance.abs() < 0.05 * led.inflow,
            "in {} out {} flood {}",
            led.inflow,
            led.outflow,
            led.flooding
        );
    }

    #[test]
    fn quiet_flow_grows_the_step_to_the_user_maximum() {
        let (_, mut r) = build(CHAIN);
        r.advance(7200.0, &inflow_at(0, 0.3));
        // Steady for two hours: growth reached the user routing step for
        // the bulk of the run (the final step is clamped to the horizon,
        // so judge by the accepted count: 7200 s at 10 s is 720 steps
        // plus a short floored start-up).
        assert!(
            (700..1100).contains(&(r.report.accepted as i64)),
            "accepted {}",
            r.report.accepted
        );
    }

    #[test]
    fn storage_attenuates_a_pulse() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  10

[JUNCTIONS]
J1  101.0  3

[STORAGE]
SU1  100.5  4  0  FUNCTIONAL  0  0  500

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1   SU1  150  0.013  0  0
C2  SU1  O1   150  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0
C2  CIRCULAR   0.4  0  0  0
";
        let (_, mut r) = build(inp);
        let pulse = |t: f64, lat: &mut [f64]| {
            lat.iter_mut().for_each(|l| *l = 0.0);
            lat[0] = if t < 900.0 { 0.4 } else { 0.0 };
        };
        let mut peak_out: f64 = 0.0;
        let mut t = 0.0;
        while t < 10_800.0 {
            t += 60.0;
            r.advance(t, &pulse);
            peak_out = peak_out.max(r.q[1]);
        }
        // The 500 m² basin holds the pulse back: outflow peaks well
        // below the 0.4 m³/s inflow peak, and the water still leaves.
        assert!(peak_out < 0.2, "peak outflow {peak_out}");
        // The wide basin drains through a small pipe: a long tail keeps
        // a few centimetres back after three hours.
        let led = &r.report;
        assert!(
            led.outflow > 0.9 * led.inflow,
            "in {} out {}",
            led.inflow,
            led.outflow
        );
    }
}

#[cfg(test)]
mod structure_tests {
    use super::tests::{build, inflow_at};
    use super::*;

    #[test]
    fn transverse_weir_head_follows_its_rating() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.0  4

[OUTFALLS]
O1  99.0  FREE

[WEIRS]
W1  J1  O1  TRANSVERSE  1.0  1.83  NO  0  0  YES

[XSECTIONS]
W1  RECT_OPEN  1.5  2  0  0
";
        let (_, mut r) = build(inp);
        let q_in = 0.3;
        r.advance(3600.0, &inflow_at(0, q_in));
        // Steady: Q = C·L·H^1.5 → H = (Q/(C·L))^(2/3) over the 1 m crest.
        let h_expect = (q_in / (1.83 * 2.0)).powf(2.0 / 3.0);
        assert!(
            (r.y[0] - (1.0 + h_expect)).abs() < 0.01,
            "depth {} vs {}",
            r.y[0],
            1.0 + h_expect
        );
        assert!((r.sq[0] - q_in).abs() < 0.01 * q_in);
        // The ledger closes.
        let led = &r.report;
        assert!((led.outflow - led.inflow).abs() < 0.02 * led.inflow);
    }

    #[test]
    fn side_orifice_head_follows_torricelli() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.0  4

[OUTFALLS]
O1  99.0  FREE

[ORIFICES]
R1  J1  O1  SIDE  0  0.65  NO  0

[XSECTIONS]
R1  CIRCULAR  0.2  0  0  0
";
        let (_, mut r) = build(inp);
        let q_in = 0.05;
        r.advance(3600.0, &inflow_at(0, q_in));
        // Submerged inlet, free discharge: Q = Cd·A·√(2g·(y − D/2)).
        let a = std::f64::consts::PI * 0.01;
        let head = (q_in / (0.65 * a)).powi(2) / (2.0 * GRAVITY);
        let y_expect = head + 0.1;
        assert!(
            (r.y[0] - y_expect).abs() < 0.01,
            "depth {} vs {y_expect}",
            r.y[0]
        );
        assert!((r.sq[0] - q_in).abs() < 0.01 * q_in);
    }

    #[test]
    fn outlet_functional_rating_holds() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.0  4

[OUTFALLS]
O1  99.0  FREE

[OUTLETS]
OUT1  J1  O1  0  FUNCTIONAL/DEPTH  0.1  1.5  NO
";
        let (_, mut r) = build(inp);
        let q_in = 0.05;
        r.advance(3600.0, &inflow_at(0, q_in));
        // Q = a·y^b → y = (Q/a)^(1/b).
        let y_expect = (q_in / 0.1_f64).powf(1.0 / 1.5);
        assert!(
            (r.y[0] - y_expect).abs() < 0.01,
            "depth {} vs {y_expect}",
            r.y[0]
        );
    }

    #[test]
    fn inline_pump_finds_its_operating_point() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[STORAGE]
SU1  100.0  3  0  FUNCTIONAL  0  0  20

[JUNCTIONS]
J2  103.0  2

[OUTFALLS]
O1  102.5  FREE

[PUMPS]
P1  SU1  J2  PC  ON  0  0

[CONDUITS]
C1  J2  O1  50  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0

[CURVES]
PC  PUMP4  0  0  2  0.2
";
        let (_, mut r) = build(inp);
        let q_in = 0.05;
        r.advance(7200.0, &inflow_at(0, q_in));
        // The type-4 characteristic q = 0.1·y balances the inflow at
        // y = 0.5 m, lifting water 3 m uphill.
        assert!((r.y[0] - 0.5).abs() < 0.02, "well depth {}", r.y[0]);
        assert!((r.sq[0] - q_in).abs() < 0.02 * q_in, "pump {}", r.sq[0]);
        let led = &r.report;
        assert!((led.outflow - led.inflow).abs() < 0.05 * led.inflow);
    }

    #[test]
    fn pump_latches_cycle_between_startup_and_shutoff() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[STORAGE]
SU1  100.0  3  0  FUNCTIONAL  0  0  10

[JUNCTIONS]
J2  102.0  2

[OUTFALLS]
O1  101.5  FREE

[PUMPS]
P1  SU1  J2  PC  OFF  1.0  0.2

[CONDUITS]
C1  J2  O1  50  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0

[CURVES]
PC  PUMP2  0  0.2  3  0.2
";
        let (_, mut r) = build(inp);
        // A trickle in, a strong pump: the well fills to the 1 m startup,
        // empties to the 0.2 m shutoff, and repeats.
        let mut saw_on = false;
        let mut saw_off_again = false;
        let mut t = 0.0;
        while t < 7200.0 {
            t += 30.0;
            r.advance(t, &inflow_at(0, 0.02));
            if r.pump_on[0] {
                saw_on = true;
            } else if saw_on {
                saw_off_again = true;
            }
            assert!(r.y[0] < 1.15, "well overfilled: {}", r.y[0]);
        }
        assert!(saw_on && saw_off_again, "no pump cycling");
        let led = &r.report;
        assert!((led.outflow - led.inflow).abs() < 0.1 * led.inflow);
    }

    #[test]
    fn a_dummy_channel_passes_its_inflow_through() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.0  2

[OUTFALLS]
O1  99.5  FREE

[CONDUITS]
D1  J1  O1  100  0.013  0  0

[XSECTIONS]
D1  DUMMY  0  0  0  0
";
        let (_, mut r) = build(inp);
        r.advance(600.0, &inflow_at(0, 0.07));
        assert!((r.sq[0] - 0.07).abs() < 1e-6, "dummy {}", r.sq[0]);
    }
}

#[cfg(test)]
mod loss_and_force_main_tests {
    use super::tests::{build, inflow_at};
    use super::*;

    #[test]
    fn full_hazen_williams_main_balances_its_friction_slope() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.02  20

[OUTFALLS]
O1  100.0  FIXED  100.4

[CONDUITS]
F1  J1  O1  200  0.013  0  0

[XSECTIONS]
F1  FORCE_MAIN  0.3  120  0  0
";
        let (_, mut r) = build(inp);
        // The fixed tailwater submerges the downstream end, so the main
        // runs full and the pressurised relation governs.
        let q_in = 0.15;
        r.advance(3600.0, &inflow_at(0, q_in));
        assert!((r.q[0] - q_in).abs() < 0.02 * q_in, "q {}", r.q[0]);
        assert!(r.y[0] > 0.3, "not full: {}", r.y[0]);
        // Steady: the head difference matches the SI Hazen–Williams
        // friction slope over the 200 m run.
        let a = std::f64::consts::PI * 0.15 * 0.15;
        let v: f64 = q_in / a;
        let rr: f64 = 0.075;
        let sf = (v / (0.849 * 120.0 * rr.powf(0.63))).powf(1.0 / 0.54);
        let dh = (100.02 + r.y[0]) - 100.4;
        assert!(
            (dh - sf * 200.0).abs() < 0.15 * sf * 200.0,
            "dh {dh} vs Sf·L {}",
            sf * 200.0
        );
    }

    #[test]
    fn darcy_weisbach_main_uses_swamee_jain() {
        let inp = "\
[OPTIONS]
FLOW_UNITS          CMS
ROUTING_STEP        5
FORCE_MAIN_EQUATION D-W

[JUNCTIONS]
J1  100.02  20

[OUTFALLS]
O1  100.0  FIXED  100.4

[CONDUITS]
F1  J1  O1  200  0.013  0  0

[XSECTIONS]
F1  FORCE_MAIN  0.3  0.25  0  0
";
        let (_, mut r) = build(inp);
        let q_in = 0.15;
        r.advance(3600.0, &inflow_at(0, q_in));
        assert!((r.q[0] - q_in).abs() < 0.02 * q_in, "q {}", r.q[0]);
        // Steady balance against f·v²/(8gR) with the roughness height
        // 0.25 mm.
        let a = std::f64::consts::PI * 0.15 * 0.15;
        let v: f64 = q_in / a;
        let rr: f64 = 0.075;
        let re = 4.0 * rr * v / VISCOSITY;
        let f = swamee_jain(0.25e-3, rr, re);
        let sf = f * v * v / (8.0 * GRAVITY * rr);
        let dh = (100.02 + r.y[0]) - 100.4;
        assert!(
            (dh - sf * 200.0).abs() < 0.15 * sf * 200.0,
            "dh {dh} vs Sf·L {}",
            sf * 200.0
        );
    }

    #[test]
    fn seepage_debits_the_ledger_and_shrinks_outflow() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  10

[JUNCTIONS]
J1  100.4  3
J2  100.2  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  J2  200  0.013  0  0
C2  J2  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0
C2  RECT_OPEN  2  2  0  0

[LOSSES]
C1  0  0  0  NO  36
";
        // 36 mm/h of seepage over a 2 m wide, 200 m channel ≈ 4 l/s.
        let (_, mut r) = build(inp);
        let q_in = 0.1;
        r.advance(7200.0, &inflow_at(0, q_in));
        assert!(r.report.losses > 0.0);
        // Steady window: inflow = outflow + seepage.
        let (i0, o0, l0) = (r.report.inflow, r.report.outflow, r.report.losses);
        r.advance(10_800.0, &inflow_at(0, q_in));
        let din = r.report.inflow - i0;
        let dout = r.report.outflow - o0;
        let dloss = r.report.losses - l0;
        assert!(dloss > 0.5 * 0.004 * 3600.0, "seepage {dloss}");
        assert!(
            (din - dout - dloss).abs() < 0.01 * din,
            "in {din} out {dout} loss {dloss}"
        );
        // Downstream carries less than upstream by the seepage rate.
        assert!(
            r.q[0] - r.q[1] > 0.45 * dloss / 3600.0,
            "q {} {}",
            r.q[0],
            r.q[1]
        );
    }
}

#[cfg(test)]
mod culvert_and_roadway_tests {
    use super::tests::{build, inflow_at};
    use super::*;

    fn culvert_net(code: u32) -> String {
        format!(
            "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.5  10

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  50  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.6  0  0  0  1  {code}
"
        )
    }

    #[test]
    fn culvert_inlet_control_sets_the_headwater() {
        // Without a culvert code the short steep barrel passes the flow
        // at modest headwater; with code 1 (square-edge concrete) inlet
        // control governs and the headwater rises to the submerged
        // HDS-5 rating.
        let (_, mut r) = build(&culvert_net(0));
        let q_in = 0.9;
        r.advance(1800.0, &inflow_at(0, q_in));
        let y_plain = r.y[0];

        let (_, mut r) = build(&culvert_net(1));
        r.advance(1800.0, &inflow_at(0, q_in));
        let y_culvert = r.y[0];
        assert!(
            y_culvert > y_plain + 0.05,
            "culvert {y_culvert} vs plain {y_plain}"
        );
        // The submerged form inverted for the steady flow:
        // y = D·(Y − scf + C·(Q/AD)²).
        let (_, _, _, cc, yy) = tables::CULVERT_PARAMS[1];
        let d = 0.6;
        let a = std::f64::consts::PI * d * d / 4.0;
        let ad = a * (d * FT).sqrt();
        let slope = 0.5_f64 / (50.0_f64.powi(2) - 0.25).sqrt();
        let scf = 0.5 * slope;
        let y_expect = d * (yy - scf + cc * (q_in / ad).powi(2));
        assert!(
            (y_culvert - y_expect).abs() < 0.05 * y_expect,
            "headwater {y_culvert} vs HDS-5 {y_expect}"
        );
    }

    #[test]
    fn mitered_inlets_use_the_published_slope_correction() {
        // Code 5 is the mitered corrugated-metal inlet: the correction is
        // −0.7·S at its published magnitude, not the predecessor's −7.0.
        let (_, mut r) = build(&culvert_net(5));
        let q_in = 0.9;
        r.advance(1800.0, &inflow_at(0, q_in));
        let (_, _, _, cc, yy) = tables::CULVERT_PARAMS[5];
        let d = 0.6;
        let a = std::f64::consts::PI * d * d / 4.0;
        let ad = a * (d * FT).sqrt();
        let slope = 0.5_f64 / (50.0_f64.powi(2) - 0.25).sqrt();
        let scf = -0.7 * slope;
        let y_expect = d * (yy - scf + cc * (q_in / ad).powi(2));
        assert!(
            (r.y[0] - y_expect).abs() < 0.05 * y_expect,
            "headwater {} vs HDS-5 {y_expect}",
            r.y[0]
        );
    }

    #[test]
    fn roadway_weir_uses_the_fhwa_coefficient() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.0  4

[OUTFALLS]
O1  99.0  FREE

[WEIRS]
W1  J1  O1  ROADWAY  0.5  1.83  NO  0  0  NO  10  PAVED

[XSECTIONS]
W1  RECT_OPEN  1.0  8  0  0
";
        let (_, mut r) = build(inp);
        let q_in = 0.6;
        r.advance(3600.0, &inflow_at(0, q_in));
        // Steady: Q = Cd(h)·L·h^1.5 with the FHWA paved coefficient at
        // the settled head over the 0.5 m road crest.
        let h = r.y[0] - 0.5;
        assert!(h > 0.0);
        let cd = roadway_cd(h, 0.0, 10.0, true);
        let q_check = cd * 8.0 * h.powf(1.5);
        assert!(
            (q_check - q_in).abs() < 0.02 * q_in,
            "rating {q_check} vs inflow {q_in} at head {h}"
        );
        // The coefficient sits in the converted FHWA band
        // (≈ 2.85–3.05 ft-units → ≈ 1.57–1.69 SI).
        assert!((1.5..1.75).contains(&cd), "cd {cd}");
    }
}
