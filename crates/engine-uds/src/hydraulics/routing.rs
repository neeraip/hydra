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
//! and outfalls, with the §7 structures (orifices, weirs, outlets, culverts,
//! pumps) spliced into the same sweep.

use super::section::Section;
use super::{tables, GRAVITY};
use crate::hydrology::infiltration::{InfilFactors, InfilState};
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
/// The smallest step floor a model may ask for (s), §6.5.
///
/// The floor itself is `min_routing_step`, defaulting to 0.5 s. This is
/// the bound below which it cannot be set: a floor at zero would let one
/// short channel drive the step toward zero, and the run would stop
/// advancing in finite time rather than finish inaccurately.
const DT_FLOOR_MIN: f64 = 1e-3;

/// The §6.5 floor a model asks for, clamped to what is usable.
///
/// Order matters and follows the predecessor's: the user step caps it
/// first, then the absolute minimum raises it. A routing step below the
/// absolute minimum therefore ends with a floor above the user step,
/// which is the only sane reading of a model that asks for both.
pub fn step_floor(min_routing_step: f64, dt_user: f64) -> f64 {
    min_routing_step.min(dt_user).max(DT_FLOOR_MIN)
}

/// The §6.4 criterion-2 allowance: how much summed continuity residual an
/// iterate may carry and still count as conserving mass.
///
/// Two terms. The relative one, `continuity_tol · Σ|Q|`, is the accuracy
/// statement — closure to a fraction of the flow actually moving. The
/// second, `ε_H/Δt · ΣA_S`, is the *resolution* of that statement: it is
/// the flow rate that would move every vertex's head by exactly the head
/// tolerance over the step, and criterion 1 accepts iterates whose heads
/// still move by that much. An allowance without it demands mass closure
/// finer than the head gate certifies, which no amount of iteration can
/// deliver — the residual's floor is the settled iterates' own noise.
///
/// The failure that proved the term necessary: a network draining at a
/// dry-weather trickle (Σ|Q| ~ 1 L/s) got a ~1 µL/s allowance, rejected
/// every trial on this gate alone with heads static to 1e-7 m, and pinned
/// a 36-hour run at the 0.5 s step floor — ~134 000 steps, 130 000
/// rejections, 76 000 degraded-accuracy warnings, and 39 seconds of wall
/// clock for a run the fixed user step covers in ~2 000 steps.
fn continuity_allowance(
    continuity_tol: f64,
    flow_scale: f64,
    head_tol: f64,
    dt: f64,
    area_sum: f64,
) -> f64 {
    continuity_tol * flow_scale.max(Q_DRY) + head_tol / dt * area_sum
}

/// Why a network cannot be routed by this build stage.
#[derive(Debug, Clone, PartialEq)]
pub enum RouterRefusal {
    /// An element class the §6 core does not route yet.
    Unsupported(&'static str),
    /// A section that failed to build (§14.7 would have refused it).
    Geometry(String),
}

impl std::fmt::Display for RouterRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouterRefusal::Unsupported(what) => {
                write!(f, "the router does not support {what} yet")
            }
            RouterRefusal::Geometry(reason) => write!(f, "section geometry refused: {reason}"),
        }
    }
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

    /// Area, hydraulic radius and top width together (§6.3).
    ///
    /// The momentum update wants all three at the same depth and used to
    /// ask for the area and radius, then the width, which on a circular
    /// section rebuilt the filled angle for a value the first call had
    /// already found.
    fn area_radius_width(&self, y: f64) -> (f64, f64, f64) {
        let y_full = self.sec.y_full();
        if y >= y_full {
            let w = if self.w_slot == 0.0 {
                self.sec.top_width(y)
            } else {
                self.w_slot
            };
            return (self.area(y), self.sec.r_full(), w);
        }
        if self.w_slot == 0.0 || y <= self.y_x {
            return self.sec.area_radius_width(y);
        }
        // In the crown band the area is the slot's; the radius and width
        // are still the section's own, and the width is floored there.
        let (_, r, w) = self.sec.area_radius_width(y);
        (self.area(y), r, w.max(self.w_slot))
    }

    /// Area and hydraulic radius together, sharing one pass over the
    /// section where the slot logic allows it — the §6.3 update wants
    /// both at the same depth, and asking twice rebuilds the geometry
    /// twice. Above full depth the radius holds at its full value.
    fn area_and_radius(&self, y: f64) -> (f64, f64) {
        let y_full = self.sec.y_full();
        if y >= y_full {
            return (self.area(y), self.sec.r_full());
        }
        if self.w_slot == 0.0 || y <= self.y_x {
            return self.sec.area_and_radius(y);
        }
        // In the crown band the area is the slot's, but the radius is
        // still the section's own.
        (self.area(y), self.sec.hyd_radius(y))
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
        /// Open/close travel time (s); 0 slams instantly (§7.2).
        orate: f64,
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

/// The §11.2 step-size bands: six edges, largest first, spanning the
/// routing step down to the step floor and spaced logarithmically. The
/// report prints them as five intervals, so it wants the edges rather
/// than the counts alone.
pub fn step_bands(dt_user: f64, dt_floor: f64) -> [f64; 6] {
    let top = dt_user.max(dt_floor);
    let ratio = (dt_floor / top).powf(0.2);
    let mut edges = [top; 6];
    for k in 1..6 {
        edges[k] = edges[k - 1] * ratio;
    }
    edges
}

/// Which band an accepted step of `dt` falls in, largest band first.
fn step_band(dt: f64, dt_user: f64, dt_floor: f64) -> usize {
    let edges = step_bands(dt_user, dt_floor);
    (0..5).find(|&k| dt > edges[k + 1]).unwrap_or(4)
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
    /// A gated outfall: reverse flow through any connecting link is
    /// blocked (§2.6).
    gated: bool,
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
    /// Negative laterals booked as outflow by their sign (§11.1).
    pub negative_out: f64,
    /// Stored volume at the start of the run (m³).
    pub initial_storage: f64,
    /// Total channel and storage evaporation and seepage volume (m³).
    pub losses: f64,
    /// The evaporation half of `losses` (m³). Seepage is the remainder
    /// rather than its own accumulator, so the two always sum to the
    /// ledger term exactly and no rounding can open a gap between them.
    pub evaporation: f64,
    /// §11.2 step-size extremes (s). `dt_min` is zero until the first
    /// step is accepted, which is why it is not seeded to infinity.
    pub dt_min: f64,
    pub dt_max: f64,
    /// Iterations summed over accepted steps, and the count of accepted
    /// steps that exhausted the trial budget without converging.
    pub iterations: u64,
    pub nonconverged: u64,
    /// Routed time (s) — the accepted steps' summed duration, which is
    /// the mean step's denominator.
    pub elapsed: f64,
    /// Accepted steps by step-size band, largest band first: five bands
    /// spanning the step floor to the routing step, spaced
    /// logarithmically (§11.2).
    pub dt_bands: [u64; 5],
}

/// The §6 router over a validated network.
pub struct Router {
    chans: Vec<Chan>,
    verts: Vec<Vert>,
    structs: Vec<Structure>,
    ids: Vec<String>,
    // Options.
    dt_user: f64,
    /// §6.5 step floor (s): `min_routing_step`, clamped by `step_floor`.
    dt_floor: f64,
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
    /// §9 operational settings per structure: pump speed, orifice/weir
    /// opening fraction, outlet scale. Seeded from the model's initial
    /// status, 1 for everything that has none. `sett` is the target;
    /// `sett_cur` is the acting value, which slews for orifices with an
    /// open/close rate (§7.2) and follows instantly otherwise.
    ///
    /// A pump's setting is its *whole* on/off state (§7.1): the startup and
    /// shutoff depths write it, and so does control, so there is one answer
    /// to "is this pump running" rather than two that can disagree.
    sett: Vec<f64>,
    sett_cur: Vec<f64>,
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
    /// Per-channel evaporation and seepage rates of the last accepted
    /// step (m³/s).
    chan_evap_now: Vec<f64>,
    chan_seep_now: Vec<f64>,
    /// §7.7 storage losses: per-vertex evaporation realisation fraction,
    /// constant-seepage conductivity, optional Green–Ampt state, and the
    /// last accepted step's rates (m³/s).
    stor_evap_frac: Vec<f64>,
    stor_seep_ksat: Vec<f64>,
    stor_ga: Vec<Option<InfilState>>,
    stor_evap_now: Vec<f64>,
    stor_seep_now: Vec<f64>,
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
    /// §11.2 per-vertex statistics.
    pub vertex_stats: Vec<VertexStats>,
    /// Pump start-up edge detection.
    pump_prev_off: Vec<bool>,
    /// §11.2 per-model-link statistics.
    pub link_stats: Vec<LinkStats>,
    /// Worst-error vertex counts for the top-five diagnostics (§11.2).
    pub worst_counts: Vec<u64>,
    /// Per-object statistics begin here (s); numerical statistics span
    /// the whole run (§11.2).
    pub stats_start: f64,
}

/// Per-vertex §11.2 statistics, accumulated on accepted steps after
/// the report start.
#[derive(Debug, Clone, Copy, Default)]
pub struct VertexStats {
    /// Maximum depth (m) and when it occurred (s).
    pub max_depth: f64,
    pub t_max_depth: f64,
    /// Time-weighted depth integral (m·s); over `obs_time` it is the
    /// §11.2 mean depth.
    pub depth_sum: f64,
    /// Maximum depth seen at a reporting instant (m), which is not the
    /// maximum over computational steps (§11.2).
    pub reported_max_depth: f64,
    /// Maximum flooding rate (m³/s), when it occurred (s), and total
    /// flooded time (s).
    pub max_flood: f64,
    pub t_max_flood: f64,
    pub flood_time: f64,
    /// Flooded volume (m³) and the maximum ponded volume (m³).
    pub flood_volume: f64,
    pub max_ponded_volume: f64,
    /// Time above the highest connecting crown (s), the greatest height
    /// reached above it (m), and the least depth left below the rim
    /// while surcharged (m).
    pub surcharge_time: f64,
    pub max_crown_height: f64,
    pub min_rim_depth: f64,
    /// Inflow (§11.2): peak lateral and total rates (m³/s), the instant
    /// of the total peak (s), and both volumes (m³).
    pub max_lat_inflow: f64,
    pub max_total_inflow: f64,
    pub t_max_total_inflow: f64,
    pub lat_inflow_volume: f64,
    pub total_inflow_volume: f64,
    /// Volume leaving by every path (m³), for the vertex flow balance.
    pub outflow_volume: f64,
    /// Stored volume when statistics began and at the last accepted
    /// step (m³) — the storage-change term of that balance.
    pub initial_volume: f64,
    pub final_volume: f64,
    /// Storage vertices: volume integral (m³·s), peak volume (m³) and
    /// its instant (s), the loss volumes (m³), and peak outflow (m³/s).
    pub volume_sum: f64,
    pub max_volume: f64,
    pub t_max_volume: f64,
    pub evap_loss_volume: f64,
    pub exfil_loss_volume: f64,
    pub max_outflow: f64,
    /// The storage geometry's volume at its maximum depth (m³), fixed
    /// at build; zero for every other vertex, which is how the report
    /// tells storage apart. The percent-full columns divide by it.
    pub full_volume: f64,
    /// Outfalls: discharge volume (m³), peak (m³/s), and flowing time
    /// (s) — against `obs_time` for the frequency, and dividing the
    /// volume for the mean discharge while flowing.
    pub out_volume: f64,
    pub out_peak: f64,
    pub out_time: f64,
    /// Accepted steps observed and their summed duration (s).
    pub steps: u64,
    pub obs_time: f64,
}

/// Per-link §11.2 statistics.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinkStats {
    /// Maximum |flow| (m³/s) and when it occurred (s).
    pub max_flow: f64,
    pub t_max_flow: f64,
    /// Maximum velocity (m/s) and depth (m) — channels.
    pub max_velocity: f64,
    pub max_depth: f64,
    /// Time flowing full (s) — channels.
    pub full_time: f64,
    /// Observed time (s), the denominator of every fraction below.
    pub obs_time: f64,
    /// Time in each §6.3 flow class (s), in the predecessor's column
    /// order: dry, dry upstream, dry downstream, subcritical,
    /// supercritical, critical upstream, critical downstream.
    pub class_time: [f64; 7],
    /// Time the normal-flow limiter bound the flow, and time §7.6
    /// culvert inlet control capped it (s). Neither is a flow class:
    /// a step may be in both, or in neither.
    pub norm_limited_time: f64,
    pub inlet_control_time: f64,
    /// Conduit surcharge times (s): full at both ends, at the upstream
    /// end alone, at the downstream end alone, above normal flow, and
    /// capacity-limited.
    pub full_both_time: f64,
    pub full_up_time: f64,
    pub full_down_time: f64,
    pub above_normal_time: f64,
    pub capacity_limited_time: f64,
    /// The section's full depth (m) and full-flow capacity (m³/s), both
    /// fixed at build. Zero for links that are not channels, which is
    /// how the report tells the two apart.
    pub full_depth: f64,
    pub full_flow: f64,
    /// Accepted steps whose flow reversed the sign of its change while
    /// both neighbouring changes cleared the flow tolerance (§11.2),
    /// the steps observed to divide it by, and the two previous flows
    /// the test needs.
    pub instability_count: u64,
    pub steps: u64,
    pub prev_flow: f64,
    pub prev_delta: f64,
    // Pumps (§11.2): utilisation, startups, flow range, volume, energy,
    // and off-curve time booked to the correct end for every type.
    pub on_time: f64,
    pub startups: u32,
    pub min_flow: f64,
    pub max_pump_flow: f64,
    pub volume: f64,
    pub energy_kwh: f64,
    pub off_low_time: f64,
    pub off_high_time: f64,
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
    /// Iterations this trial took (§11.2).
    iterations: u32,
    flood_rate: Vec<f64>,
    /// Per-channel evaporation and seepage rates (m³/s), for §8.4.
    chan_evap: Vec<f64>,
    chan_seep: Vec<f64>,
    /// Per-vertex storage evaporation and seepage rates (m³/s), §7.7.
    stor_evap: Vec<f64>,
    stor_seep: Vec<f64>,
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
            let mut gated = false;
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
                VertexKind::Outfall {
                    stage, flap_gate, ..
                } => {
                    gated = *flap_gate;
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
                gated,
                class,
            });
        }

        // §7.7 storage losses: realisation fraction, and constant or
        // Green–Ampt seepage per storage vertex (the modified form, per
        // §3.3's designation for storage seepage).
        let nv_all = net.vertices.len();
        let mut stor_evap_frac = vec![0.0; nv_all];
        let mut stor_seep_ksat = vec![0.0; nv_all];
        let mut stor_ga: Vec<Option<InfilState>> = (0..nv_all).map(|_| None).collect();
        for (vi, v) in net.vertices.iter().enumerate() {
            if let VertexKind::Storage {
                evap_fraction,
                seepage,
                ..
            } = &v.kind
            {
                stor_evap_frac[vi] = evap_fraction.clamp(0.0, 1.0);
                if let Some(sp) = seepage {
                    stor_seep_ksat[vi] = sp.conductivity.max(0.0);
                    if sp.suction > 0.0 || sp.initial_deficit > 0.0 {
                        stor_ga[vi] = Some(InfilState::build(
                            &crate::model::Infiltration::GreenAmpt {
                                suction: sp.suction,
                                conductivity: sp.conductivity,
                                initial_deficit: sp.initial_deficit,
                            },
                            crate::io::options::InfiltrationModel::ModifiedGreenAmpt,
                        ));
                    }
                }
            }
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
        // Initial settings, in step with `structs` (§7.1).
        let mut sett0: Vec<f64> = Vec::new();
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
                    sett0.push(if *initial_on { 1.0 } else { 0.0 });
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
                    open_close_time,
                    ..
                } => {
                    let sec = build_struct_section(net, li, len, &link.id)?;
                    StructKind::Orifice {
                        bottom: *orientation == OrificeOrientation::Bottom,
                        cd: *discharge_coeff,
                        flap: *flap_gate,
                        sec,
                        orate: *open_close_time,
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
            sett0.push(1.0);
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
            dt_floor: step_floor(net.options.min_routing_step, net.options.routing_step),
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
            sett: sett0.clone(),
            sett_cur: sett0,
            chan_open: vec![true; nc],
            struct_flip_t: vec![0.0; ns],
            chan_flip_t: vec![0.0; nc],
            a_mid: vec![0.0; nc],
            net_flow: vec![0.0; nv],
            flood_now: vec![0.0; nv],
            chan_evap_now: vec![0.0; nc],
            stor_evap_frac,
            stor_seep_ksat,
            stor_ga,
            stor_evap_now: vec![0.0; nv],
            stor_seep_now: vec![0.0; nv],
            chan_seep_now: vec![0.0; nc],
            hist: Vec::new(),
            dt_prev: step_floor(net.options.min_routing_step, net.options.routing_step),
            quiet_streak: 0,
            vertex_stats: vec![VertexStats::default(); nv],
            pump_prev_off: vec![true; ns],
            link_stats: vec![
                LinkStats {
                    min_flow: f64::MAX,
                    ..LinkStats::default()
                };
                net.links.len()
            ],
            worst_counts: vec![0; nv],
            stats_start: 0.0,
            evap_rate: 0.0,
            report: RoutingReport::default(),
        };
        // §11.2's Max/Full ratios divide by these, so they are taken
        // once from the built geometry rather than re-derived per step.
        for c in &r.chans {
            let st = &mut r.link_stats[c.link];
            st.full_depth = c.geom.sec.y_full();
            st.full_flow = c.barrels
                * c.geom.sec.a_full()
                * super::section::two_thirds(c.geom.sec.r_full())
                * c.slope.sqrt()
                / c.n;
        }
        for vi in 0..r.verts.len() {
            if let VertClass::Storage(g) = &r.verts[vi].class {
                r.vertex_stats[vi].full_volume = g.volume(r.verts[vi].y_max);
            }
        }
        r.seed_initial_state(net);
        r.report.initial_storage = (0..r.verts.len())
            .map(|v| r.vertex_volume_now(v))
            .sum::<f64>()
            + r.channel_transport().iter().map(|c| c.4).sum::<f64>();
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
            // §6.7: only links carrying an initial flow seed their end
            // vertices — a dry link's offset is geometry, not water, and
            // averaging it in poured phantom depth into offset junctions.
            if end_depth[ci] <= 0.0 {
                continue;
            }
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
        // §6.7: an outfall takes the depth its own boundary imposes at the
        // initial flows, not one averaged from its neighbours. Measured
        // before the mid-areas below, because the channel reaching a
        // staged outfall holds that water from the start — left dry here,
        // it would read as volume created on the first step and the flow
        // ledger would carry the difference all run.
        let outfall_y: Vec<(usize, f64)> = self
            .verts
            .iter()
            .enumerate()
            .filter_map(|(vi, v)| match &v.class {
                VertClass::Outfall(b) => Some((vi, self.outfall_depth(vi, b, &self.q))),
                _ => None,
            })
            .collect();
        for (vi, y) in outfall_y {
            self.y[vi] = y;
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

    /// Export the §14.8 hotstart hydraulic state: vertex depths and per
    /// model link (flow m³/s, depth m, setting).
    pub fn hotstart_get(&self, n_links: usize) -> (Vec<f64>, Vec<(f64, f64, f64)>) {
        let depths: Vec<f64> = self.y.clone();
        let mut links = vec![(0.0, 0.0, 1.0); n_links];
        for (ci, c) in self.chans.iter().enumerate() {
            links[c.link] = (
                self.q[ci],
                self.link_depth(c.link).unwrap_or(0.0),
                if self.chan_open[ci] { 1.0 } else { 0.0 },
            );
        }
        for (si, st) in self.structs.iter().enumerate() {
            links[st.link] = (
                self.sq[si],
                self.link_depth(st.link).unwrap_or(0.0),
                self.sett[si],
            );
        }
        (depths, links)
    }

    /// Restore the §14.8 hotstart hydraulic state; depths seed the
    /// vertices, flows the channels and structures, settings the §9
    /// layer, and the mid-areas re-prime from the restored depths.
    pub fn hotstart_apply(&mut self, depths: &[f64], links: &[(f64, f64, f64)]) {
        for (vi, d) in depths.iter().enumerate() {
            if vi < self.y.len() {
                self.y[vi] = d.max(0.0);
            }
        }
        for (ci, c) in self.chans.iter().enumerate() {
            if let Some(&(q, _, setting)) = links.get(c.link) {
                self.q[ci] = q;
                self.chan_open[ci] = setting > 0.0;
            }
        }
        for (si, st) in self.structs.iter().enumerate() {
            if let Some(&(q, _, setting)) = links.get(st.link) {
                self.sq[si] = q;
                self.sett[si] = setting;
            }
        }
        for (ci, c) in self.chans.iter().enumerate() {
            let y1 = (self.y[c.from] - c.off1).max(0.0);
            let y2 = (self.y[c.to] - c.off2).max(0.0);
            let y_mid = 0.5 * (y1 + y2);
            self.a_mid[ci] = c.geom.area(y_mid.max(DRY)).max(DRY);
        }
        self.hist.clear();
        // Restored storage is this run's starting storage (§11.1).
        self.report.initial_storage = (0..self.verts.len())
            .map(|v| self.vertex_volume_now(v))
            .sum::<f64>()
            + self.channel_transport().iter().map(|c| c.4).sum::<f64>();
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

    /// Set a channel's entry, exit and average loss coefficients (§12.4).
    /// `None` for a model link this router does not carry as a channel.
    pub fn set_losses(&mut self, li: usize, inlet: f64, outlet: f64, average: f64) -> Option<()> {
        let ci = self.chans.iter().position(|c| c.link == li)?;
        self.chans[ci].loss_inlet = inlet;
        self.chans[ci].loss_outlet = outlet;
        self.chans[ci].loss_avg = average;
        Some(())
    }

    /// A channel's entry, exit and average loss coefficients.
    pub fn losses(&self, li: usize) -> Option<(f64, f64, f64)> {
        let ci = self.chans.iter().position(|c| c.link == li)?;
        let c = &self.chans[ci];
        Some((c.loss_inlet, c.loss_outlet, c.loss_avg))
    }

    /// Cap the flow a channel carries (m³/s); zero is no cap, as in a
    /// model (§12.4). `None` for a link this router does not carry as a
    /// channel.
    pub fn set_flow_limit(&mut self, li: usize, q_limit: f64) -> Option<()> {
        let ci = self.chans.iter().position(|c| c.link == li)?;
        self.chans[ci].q_limit = q_limit;
        Some(())
    }

    /// A channel's flow cap (m³/s); zero is no cap.
    pub fn flow_limit(&self, li: usize) -> Option<f64> {
        let ci = self.chans.iter().position(|c| c.link == li)?;
        Some(self.chans[ci].q_limit)
    }

    /// Hold a vertex's outfall boundary at an injected stage, or release
    /// it back to the one its model declares (§12.4).
    ///
    /// Unlike `set_outfall_stage`, which moves a stage the model already
    /// declares fixed, this replaces the boundary whatever it was: an
    /// injection on a free outfall that quietly did nothing would be
    /// worse than one refused.
    pub fn force_outfall_stage(&mut self, vi: usize, elev: Option<f64>, net: &Network) {
        if !matches!(self.verts[vi].class, VertClass::Outfall(_)) {
            return;
        }
        let invert = self.verts[vi].invert;
        let boundary = match elev {
            Some(e) => Boundary::Fixed(e),
            None => {
                let VertexKind::Outfall { stage, .. } = &net.vertices[vi].kind else {
                    return;
                };
                match stage {
                    OutfallStage::Free => Boundary::Free,
                    OutfallStage::Normal => Boundary::Normal,
                    OutfallStage::Fixed(e) => Boundary::Fixed(*e),
                    // As at build: a dynamic stage is a fixed one the
                    // session moves each period, and it moves it next.
                    OutfallStage::Tidal { .. } | OutfallStage::Series { .. } => {
                        Boundary::Fixed(invert)
                    }
                }
            }
        };
        self.verts[vi].class = VertClass::Outfall(boundary);
    }

    /// Advance the clock without routing: the §10.3 between-events freeze.
    /// State and ledgers hold; the head history resets so the error
    /// estimate restarts cleanly after the gap.
    pub fn skip_to(&mut self, t: f64) {
        if t > self.t {
            self.t = t;
            self.hist.clear();
            self.dt_prev = self.dt_floor;
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

    /// Per-channel evaporation rates of the last accepted step (m³/s),
    /// in `channel_transport` slot order (§8.4).
    pub fn channel_evap_rates(&self) -> &[f64] {
        &self.chan_evap_now
    }

    /// Per-vertex storage evaporation rates of the last accepted step
    /// (m³/s), §7.7.
    pub fn storage_evap_rates(&self) -> &[f64] {
        &self.stor_evap_now
    }

    /// Per-vertex storage seepage rates of the last accepted step
    /// (m³/s), §7.7.
    pub fn storage_seep_rates(&self) -> &[f64] {
        &self.stor_seep_now
    }

    /// Per-channel seepage rates of the last accepted step (m³/s), in
    /// `channel_transport` slot order (§8.4, §11.1).
    pub fn channel_seep_rates(&self) -> &[f64] {
        &self.chan_seep_now
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
        if let Some(c) = self.chans.iter().find(|c| c.link == li) {
            // Each end caps at the section's full depth before averaging
            // — a submerged outlet contributes a full pipe, not the whole
            // tailwater column (the predecessor's convention).
            let y_full = c.geom.sec.y_full();
            let y1 = (self.y[c.from] - c.off1).clamp(0.0, y_full);
            let y2 = (self.y[c.to] - c.off2).clamp(0.0, y_full);
            return Some(0.5 * (y1 + y2));
        }
        self.structs
            .iter()
            .enumerate()
            .find(|(_, s)| s.link == li)
            .map(|(si, st)| {
                // The predecessor's per-kind depth conventions: a pump
                // reports zero; an orifice its wetted opening (capped by
                // the setting's share); a weir the head over its crest
                // capped at the section height; an outlet the head.
                let up = (self.y[st.from] - st.off1).max(0.0);
                match &st.kind {
                    StructKind::Pump { .. } => 0.0,
                    StructKind::Orifice { sec, .. } => {
                        up.min(sec.y_full() * self.sett_cur[si].clamp(0.0, 1.0))
                    }
                    StructKind::Weir { sec, .. } => up.min(sec.y_full()),
                    _ => up,
                }
            })
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
            let q_full = a * super::section::two_thirds(r) * c.slope.sqrt() / c.n * c.barrels;
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
    /// which for a pump is also its §7.1 on/off state.
    pub fn is_open(&self, li: usize) -> Option<bool> {
        if let Some(ci) = self.chans.iter().position(|c| c.link == li) {
            return Some(self.chan_open[ci]);
        }
        self.structs
            .iter()
            .position(|s| s.link == li)
            .map(|si| self.sett[si] > 0.0)
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
            let at_floor = dt <= self.dt_floor + 1e-12;
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
            dt = (0.5 * dt).max(self.dt_floor);
        }
    }

    /// §6.5 step seeding.
    fn seed_step(&mut self) -> f64 {
        if self.report.accepted == 0 {
            return self.dt_floor;
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
        dt.max(self.dt_floor)
    }

    fn accept(&mut self, dt: f64, trial: Trial, lat: &[f64]) {
        // Ledger.
        for (vi, l) in lat.iter().enumerate() {
            // §11.1: signed sources place by their sign.
            if *l >= 0.0 {
                self.report.inflow += l * dt;
            } else {
                self.report.negative_out += -l * dt;
            }
            self.report.flooding += trial.flood_rate[vi] * dt;
        }
        self.flood_now.clone_from(&trial.flood_rate);
        self.chan_evap_now.clone_from(&trial.chan_evap);
        self.chan_seep_now.clone_from(&trial.chan_seep);
        // §7.7: the storage Green–Ampt states advance only on acceptance,
        // against the start-of-step depths still in `self.y`.
        for vi in 0..self.verts.len() {
            if trial.stor_seep[vi] > 0.0 {
                if let Some(ga) = &mut self.stor_ga[vi] {
                    ga.step(
                        dt,
                        0.0,
                        self.y[vi],
                        InfilFactors {
                            conductivity: 1.0,
                            recovery: 1.0,
                        },
                    );
                }
            }
        }
        self.stor_evap_now.clone_from(&trial.stor_evap);
        self.stor_seep_now.clone_from(&trial.stor_seep);
        // Outfall discharge integrates the same trapezoid the vertex
        // update used. §11.1: the system outflow places by its sign, so a
        // reversed net flow — a staged outfall feeding the network — books
        // to the inflow side rather than vanishing under a clamp.
        for (vi, v) in self.verts.iter().enumerate() {
            if matches!(v.class, VertClass::Outfall(_)) {
                let old = self.net_flow[vi];
                let new = trial.net_flow[vi];
                self.report.outflow += 0.5 * (old.max(0.0) + new.max(0.0)) * dt;
                self.report.inflow += 0.5 * ((-old).max(0.0) + (-new).max(0.0)) * dt;
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
        self.report.evaporation +=
            (trial.chan_evap.iter().sum::<f64>() + trial.stor_evap.iter().sum::<f64>()) * dt;
        self.report.accepted += 1;
        // §7.2: orifice settings slew toward their targets at the
        // open/close rate; everything else follows instantly.
        for (si, st) in self.structs.iter().enumerate() {
            let target = self.sett[si];
            match &st.kind {
                StructKind::Orifice { orate, .. } if *orate > 0.0 => {
                    let delta = target - self.sett_cur[si];
                    let step = dt / orate;
                    if step + 0.001 >= delta.abs() {
                        self.sett_cur[si] = target;
                    } else {
                        self.sett_cur[si] += delta.signum() * step;
                    }
                }
                _ => self.sett_cur[si] = target,
            }
        }
        self.worst_counts[trial.worst_vertex] += 1;
        // §11.2: numerical-performance statistics span the whole run,
        // unlike the per-object ones below.
        self.report.dt_min = if self.report.dt_min == 0.0 {
            dt
        } else {
            self.report.dt_min.min(dt)
        };
        self.report.dt_max = self.report.dt_max.max(dt);
        self.report.elapsed += dt;
        self.report.iterations += u64::from(trial.iterations);
        if !trial.converged {
            self.report.nonconverged += 1;
        }
        self.report.dt_bands[step_band(dt, self.dt_user, self.dt_floor)] += 1;
        // §11.2: per-object statistics, gated on the report start.
        if self.t >= self.stats_start {
            self.accumulate_stats(dt, lat);
        }
    }

    /// Record the depths standing at a reporting instant (§11.2).
    ///
    /// The maximum over reporting instants is not the maximum over
    /// computational steps — the peak usually falls between two reports
    /// — and the report prints both, because a reader who checks the
    /// summary against the results file finds only this one there.
    pub fn record_reported_depths(&mut self) {
        for vi in 0..self.verts.len() {
            let y = self.y[vi];
            let st = &mut self.vertex_stats[vi];
            st.reported_max_depth = st.reported_max_depth.max(y);
        }
    }

    /// Accumulate the §11.2 per-object statistics for one accepted step.
    fn accumulate_stats(&mut self, dt: f64, lat: &[f64]) {
        let t = self.t;
        let nv = self.verts.len();

        // Everything that needs to read the whole router is gathered
        // first: the statistics rows below are borrowed mutably one at a
        // time, and no `&self` method can be called while one is held.
        let mut q_in = vec![0.0; nv];
        let mut q_out = vec![0.0; nv];
        for (ci, c) in self.chans.iter().enumerate() {
            let q = self.q[ci];
            if q > 0.0 {
                q_in[c.to] += q;
                q_out[c.from] += q;
            } else if q < 0.0 {
                q_in[c.from] += -q;
                q_out[c.to] += -q;
            }
        }
        for (si, s) in self.structs.iter().enumerate() {
            let q = self.sq[si];
            if q > 0.0 {
                q_in[s.to] += q;
                q_out[s.from] += q;
            } else if q < 0.0 {
                q_in[s.from] += -q;
                q_out[s.to] += -q;
            }
        }
        let vol_now: Vec<f64> = (0..nv).map(|vi| self.vertex_volume_now(vi)).collect();
        // One classification per channel per accepted step (§11.2) — the
        // accepted state's class, not any trial's.
        let classes: Vec<(FlowClass, bool, bool)> = (0..self.chans.len())
            .map(|ci| self.classify_now(ci))
            .collect();

        for vi in 0..nv {
            let y = self.y[vi];
            let (crown, y_max, ponded_area, is_outfall, is_storage) = {
                let v = &self.verts[vi];
                (
                    v.crown,
                    v.y_max,
                    v.ponded_area,
                    matches!(v.class, VertClass::Outfall(_)),
                    matches!(v.class, VertClass::Storage(_)),
                )
            };
            let (fl, net, evap, seep) = (
                self.flood_now[vi],
                self.net_flow[vi],
                self.stor_evap_now[vi],
                self.stor_seep_now[vi],
            );
            let (qi, qo, vol, l) = (q_in[vi], q_out[vi], vol_now[vi], lat[vi]);
            let st = &mut self.vertex_stats[vi];
            if st.steps == 0 {
                st.initial_volume = vol;
            }
            st.final_volume = vol;
            st.steps += 1;
            st.obs_time += dt;
            st.depth_sum += y * dt;
            if y > st.max_depth {
                st.max_depth = y;
                st.t_max_depth = t;
            }
            if fl > 0.0 {
                st.flood_time += dt;
                st.flood_volume += fl * dt;
                if fl > st.max_flood {
                    st.max_flood = fl;
                    st.t_max_flood = t;
                }
                if ponded_area > 0.0 {
                    st.max_ponded_volume =
                        st.max_ponded_volume.max((y - y_max).max(0.0) * ponded_area);
                }
            }
            if crown > 0.0 && y > crown && !is_outfall {
                st.surcharge_time += dt;
                st.max_crown_height = st.max_crown_height.max(y - crown);
                // The least freeboard reached, and zero once it floods.
                let rim = (y_max - y).max(0.0);
                st.min_rim_depth = if st.surcharge_time <= dt {
                    rim
                } else {
                    st.min_rim_depth.min(rim)
                };
            }
            // Inflow (§11.2). The lateral is part of the total, and a
            // negative lateral is an outflow by its sign (§11.1).
            let lat_in = l.max(0.0);
            let total_in = qi + lat_in;
            st.max_lat_inflow = st.max_lat_inflow.max(lat_in);
            if total_in > st.max_total_inflow {
                st.max_total_inflow = total_in;
                st.t_max_total_inflow = t;
            }
            st.lat_inflow_volume += lat_in * dt;
            st.total_inflow_volume += total_in * dt;
            st.outflow_volume += (qo + (-l).max(0.0) + fl) * dt;
            if is_storage {
                st.volume_sum += vol * dt;
                if vol > st.max_volume {
                    st.max_volume = vol;
                    st.t_max_volume = t;
                }
                st.evap_loss_volume += evap * dt;
                st.exfil_loss_volume += seep * dt;
                st.max_outflow = st.max_outflow.max(qo);
            }
            if is_outfall {
                let q = net.max(0.0);
                st.out_volume += q * dt;
                st.out_peak = st.out_peak.max(q);
                if q > Q_DRY {
                    st.out_time += dt;
                }
            }
        }
        // Indexed rather than iterated: the body borrows `self.chans`
        // again for the section width and `self.link_stats` mutably, so
        // an iterator over `self.chans` would hold a borrow across both.
        #[allow(clippy::needless_range_loop)]
        for ci in 0..self.chans.len() {
            let c = &self.chans[ci];
            let (from, to, off1, off2, barrels, y_full) =
                (c.from, c.to, c.off1, c.off2, c.barrels, c.geom.sec.y_full());
            let q_signed = self.q[ci];
            let q = q_signed.abs();
            let a = self.a_mid[ci].max(DRY);
            let y1 = (self.y[from] - off1).max(0.0);
            let y2 = (self.y[to] - off2).max(0.0);
            let (class, norm_limited, inlet_control) = classes[ci];
            let link = c.link;
            let st = &mut self.link_stats[link];
            if q > st.max_flow {
                st.max_flow = q;
                st.t_max_flow = t;
            }
            st.obs_time += dt;
            st.max_velocity = st.max_velocity.max((q / barrels / a).min(V_MAX));
            let y_mid = (0.5 * (y1 + y2)).min(y_full);
            st.max_depth = st.max_depth.max(y_mid);
            let (up_full, down_full) = (y1 >= y_full, y2 >= y_full);
            if up_full && down_full {
                st.full_time += dt;
                st.full_both_time += dt;
            } else if up_full {
                st.full_up_time += dt;
            } else if down_full {
                st.full_down_time += dt;
            }
            // Supercritical is not a §6.3 class of its own: the
            // classification returns subcritical and the Froude number
            // separates the two (§11.2).
            let idx = match class {
                FlowClass::Dry => 0,
                FlowClass::UpDry => 1,
                FlowClass::DownDry => 2,
                FlowClass::Subcritical => {
                    let w = self.chans[ci].geom.width(y_mid.max(DRY));
                    usize::from(froude(q / barrels / a, a, w) > 1.0) + 3
                }
                FlowClass::UpCritical => 5,
                FlowClass::DownCritical => 6,
            };
            let st = &mut self.link_stats[link];
            st.class_time[idx] += dt;
            if norm_limited {
                st.norm_limited_time += dt;
            }
            if inlet_control {
                st.inlet_control_time += dt;
            }
            // The Max/Full ratios divide stored constants, so they are
            // derived at report time; only the times are accumulated.
            if st.full_flow > 0.0 && q > st.full_flow {
                st.above_normal_time += dt;
                if up_full && down_full {
                    st.capacity_limited_time += dt;
                }
            }
            // §11.2 instability: this step's change reversed the last
            // one, both clearing the flow tolerance.
            //
            // The tolerance is a fraction of the section's *capacity*,
            // not of the link's running peak. Every converged solution
            // wanders in its last digits, so an absolute floor calls a
            // quiescent link unstable; and a fraction of the peak makes
            // the test tighten as the peak grows, so a link that has
            // barely flowed is judged against a threshold near zero.
            // Capacity is fixed for the run and is the scale the
            // oscillation would have to matter against.
            let dq = q_signed - st.prev_flow;
            let tol = (0.02 * st.full_flow).max(0.02 * st.max_flow).max(Q_DRY);
            if st.prev_delta * dq < 0.0 && st.prev_delta.abs() > tol && dq.abs() > tol {
                st.instability_count += 1;
            }
            st.steps += 1;
            st.prev_delta = dq;
            st.prev_flow = q_signed;
        }
        for si in 0..self.structs.len() {
            let link = self.structs[si].link;
            let (from, to) = (self.structs[si].from, self.structs[si].to);
            let q = self.sq[si].abs();
            // Pump quantities computed before the stats row is borrowed.
            let pump = if let StructKind::Pump { kind, .. } = &self.structs[si].kind {
                let dh =
                    (self.verts[to].invert + self.y[to] - self.verts[from].invert - self.y[from])
                        .max(0.0);
                // Off-curve time books to the correct end for every pump
                // type (§11.2).
                let arg = match kind {
                    PumpKind::Volume(_) => Some(self.vertex_volume_now(from)),
                    PumpKind::Depth(_) | PumpKind::InlineDepth(_) => Some(self.y[from]),
                    // Type 5 looks its rated curve up at the
                    // affinity-scaled head (§7.1, §11.2).
                    PumpKind::Head { affinity, .. } => {
                        let sp = if *affinity {
                            self.sett[si].max(1e-6)
                        } else {
                            1.0
                        };
                        Some(dh / (sp * sp))
                    }
                    PumpKind::Ideal => None,
                };
                let ends = match kind {
                    PumpKind::Volume(p)
                    | PumpKind::Depth(p)
                    | PumpKind::InlineDepth(p)
                    | PumpKind::Head { points: p, .. } => {
                        p.first().zip(p.last()).map(|(a, b)| (a.0, b.0))
                    }
                    PumpKind::Ideal => None,
                };
                Some((dh, arg.zip(ends)))
            } else {
                None
            };
            let st = &mut self.link_stats[link];
            if q > st.max_flow {
                st.max_flow = q;
                st.t_max_flow = t;
            }
            let Some((dh, off)) = pump else {
                continue;
            };
            if q > 0.0 {
                if self.pump_prev_off[si] {
                    st.startups += 1;
                    self.pump_prev_off[si] = false;
                }
                st.on_time += dt;
                st.volume += q * dt;
                st.min_flow = st.min_flow.min(q);
                st.max_pump_flow = st.max_pump_flow.max(q);
                // Energy: ρgQΔH per §7.1 (§11.2), in kWh.
                st.energy_kwh += 1000.0 * GRAVITY * q * dh * dt / 3.6e6;
                if let Some((x, (lo, hi))) = off {
                    if x < lo {
                        st.off_low_time += dt;
                    } else if x > hi {
                        st.off_high_time += dt;
                    }
                }
            } else {
                self.pump_prev_off[si] = true;
            }
        }
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
        // §11.2 mean iterations per step; a trial that never converges
        // has run the full budget.
        let mut iterations = self.max_trials;
        let mut net_new = vec![0.0; nv];
        let mut chan_evap = vec![0.0; nc];
        let mut chan_seep = vec![0.0; nc];
        let mut flood = vec![0.0; nv];
        let mut surf = vec![0.0; nv];
        let mut loss_total = 0.0;

        // §7.7 storage losses from the start-of-step state, constant
        // through the step's trials: evaporation at the potential rate
        // times the realisation fraction on the start-of-step surface
        // area; seepage at the constant conductivity, or the probed
        // (unadvanced) modified Green–Ampt capacity, over the same area.
        // Both capped together by the stored volume.
        let mut stor_evap = vec![0.0; nv];
        let mut stor_seep = vec![0.0; nv];
        let mut stor_loss_sum = 0.0;
        for vi in 0..nv {
            let VertClass::Storage(g) = &self.verts[vi].class else {
                continue;
            };
            let y0 = self.y[vi];
            if y0 <= DRY {
                continue;
            }
            let area = g.area(y0).max(0.0);
            let mut evap = self.evap_rate * self.stor_evap_frac[vi] * area;
            let mut seep = match &self.stor_ga[vi] {
                Some(ga) => {
                    let mut probe = ga.clone();
                    probe.step(
                        dt,
                        0.0,
                        y0,
                        InfilFactors {
                            conductivity: 1.0,
                            recovery: 1.0,
                        },
                    ) * area
                }
                None => self.stor_seep_ksat[vi] * area,
            };
            let cap = g.volume(y0) / dt;
            let total = evap + seep;
            if total > cap && total > 0.0 {
                let scale = cap / total;
                evap *= scale;
                seep *= scale;
            }
            stor_evap[vi] = evap;
            stor_seep[vi] = seep;
            stor_loss_sum += evap + seep;
        }

        for step in 0..self.max_trials {
            // ── Channel phase (∥): flows from the last iterate ─────────
            surf.iter_mut().for_each(|s| *s = 0.0);
            net_new.iter_mut().for_each(|s| *s = 0.0);
            let mut q_next = vec![0.0; nc];
            loss_total = 0.0;
            chan_evap.iter_mut().for_each(|e| *e = 0.0);
            chan_seep.iter_mut().for_each(|e| *e = 0.0);
            for ci in 0..nc {
                let (qn, a_mid, s1, s2, loss, evap) = self.channel_flow(ci, &y, q[ci], dt, step);
                chan_evap[ci] = evap;
                chan_seep[ci] = (loss - evap).max(0.0);
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
            // §7.7 storage losses debit their vertex.
            for vi in 0..nv {
                let loss = stor_evap[vi] + stor_seep[vi];
                if loss > 0.0 {
                    net_new[vi] -= loss;
                }
            }
            loss_total += stor_loss_sum;

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
            // Every structure reads the same frozen pre-structure state;
            // accumulations apply afterwards (§6.4).
            let net_base = net_new.clone();
            let surf_base = surf.clone();
            let mut sq_next = vec![0.0; self.structs.len()];
            let mut areas = vec![(0.0, 0.0); self.structs.len()];
            for si in 0..self.structs.len() {
                let (qn, s1, s2) =
                    self.structure_flow(si, &y, sq[si], dt, step, &pos_in, &net_base, &surf_base);
                sq_next[si] = qn;
                areas[si] = (s1, s2);
            }
            for (si, (s1, s2)) in areas.into_iter().enumerate() {
                let st = &self.structs[si];
                surf[st.from] += s1;
                surf[st.to] += s2;
                net_new[st.from] -= sq_next[si];
                net_new[st.to] += sq_next[si];
            }
            q = q_next;
            sq = sq_next;

            // ── Vertex phase ───────────────────────────────────────────
            let mut max_dy = 0.0_f64;
            let mut residual = 0.0_f64;
            let mut flow_scale = 0.0_f64;
            let mut area_sum = 0.0_f64;
            flood.iter_mut().for_each(|f| *f = 0.0);
            for vi in 0..nv {
                let v = &self.verts[vi];
                match &v.class {
                    VertClass::Outfall(b) => {
                        // Boundary depth from the connecting channel;
                        // outfalls sit outside the §6.4 head criterion.
                        let y_new = self.outfall_depth(vi, b, &q);
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
                        // The mass balance, before relaxation touches it.
                        let y_raw = self.y[vi] + dv / area;
                        let mut y_new = y_raw;
                        // Under-relax below the crown (§6.4).
                        if step > 0 && !(v.crown > 0.0 && y[vi] > v.crown) {
                            y_new = (1.0 - OMEGA) * y[vi] + OMEGA * y_new;
                        }
                        if y_new < 0.0 {
                            y_new = 0.0;
                        }
                        // Flooding: pin and report the surplus (§6.6). The
                        // surplus is measured against `y_raw`, not the
                        // relaxed iterate: relaxation is a device for
                        // reaching the fixed point, and charging it against
                        // the overflow would discard the difference — the
                        // vertex is pinned either way, so whatever the
                        // relaxed depth hides leaves no ledger entry at
                        // all. At a vertex held at its rim for hours that
                        // is a factor of ω off the flood volume.
                        let cap = v.y_max + v.surcharge;
                        if y_new > cap && !(self.allow_ponding && v.ponded_area > 0.0) {
                            flood[vi] = (y_raw - cap).max(0.0) * area / dt;
                            y_new = cap;
                        }
                        max_dy = max_dy.max((y_new - y[vi]).abs());
                        y[vi] = y_new;
                        // Continuity residual for criterion 2 (§6.4). The
                        // same area feeds the allowance's ε_H term, so the
                        // gate and the residual measure the same storage.
                        let stored = area * (y_new - self.y[vi]) / dt;
                        residual +=
                            (0.5 * (self.net_flow[vi] + net_new[vi]) - stored - flood[vi]).abs();
                        area_sum += area;
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
                && residual
                    <= continuity_allowance(
                        self.continuity_tol,
                        flow_scale,
                        self.head_tol,
                        dt,
                        area_sum,
                    )
            {
                converged = true;
                iterations = step + 1;
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
            iterations,
            flood_rate: flood,
            chan_evap,
            chan_seep,
            stor_evap,
            stor_seep,
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
    ///
    /// The override is written back into the setting, which is what makes
    /// it latch rather than merely gate: a pump restarted by its startup
    /// depth resumes at full speed, losing any variable speed control had
    /// given it, and stays running until the shutoff depth or control says
    /// otherwise.
    fn update_pump_latches(&mut self) {
        for (si, st) in self.structs.iter().enumerate() {
            let StructKind::Pump {
                startup, shutoff, ..
            } = &st.kind
            else {
                continue;
            };
            let y1 = self.y[st.from];
            if *shutoff > 0.0 && self.sett[si] > 0.0 && y1 < *shutoff {
                self.sett[si] = 0.0;
                self.struct_flip_t[si] = self.t;
            }
            if *startup > 0.0 && self.sett[si] == 0.0 && y1 > *startup {
                self.sett[si] = 1.0;
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
                if speed <= 0.0 {
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
                ..
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
        // §2.6: an outfall's gate blocks flow entering the network from
        // the receiving water through this structure.
        if (q < 0.0 && self.verts[st.to].gated) || (q > 0.0 && self.verts[st.from].gated) {
            q = 0.0;
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
            let a_over_l = if sec.is_closed() && (sec.w_max().1 - sec.y_full()).abs() < 1e-9 {
                // Circular opening: A/P of the open portion (§7.2).
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
                    if setting < 1.0 {
                        // A partially raised crest turns the notch into
                        // a trapezoid: the cut width becomes the bottom
                        // (§7.3).
                        let bottom = sec.w_max().1 * (1.0 - setting);
                        (cd1 * bottom * head.powf(1.5), cd1 * slope * head.powf(2.5))
                    } else {
                        (cd1 * slope * head.powf(2.5), 0.0)
                    }
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
            // §2.6: a staged condition governs only where it exceeds the
            // critical-depth elevation; below that the brink controls,
            // exactly as for a free outfall.
            Boundary::Fixed(stage) => {
                let staged = (stage - self.verts[vi].invert).max(0.0);
                let yc = self
                    .chans
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.from == vi || c.to == vi)
                    .map_or(0.0, |(ci, c)| {
                        let per_barrel = (q[ci] / c.barrels).abs();
                        if per_barrel <= Q_DRY {
                            0.0
                        } else {
                            c.geom
                                .sec
                                .critical_depth(per_barrel)
                                .min(c.geom.sec.y_full())
                        }
                    });
                staged.max(yc)
            }
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
    ) -> (f64, f64, f64, f64, f64, f64) {
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
        // One characteristic-depth answer serves both, per channel per
        // update.
        let mut cd = None;
        let class = self.flow_class(ci, q_last, h1, h2, y1, y2, &mut cd);
        let (s1, s2);
        {
            let (cls_s1, cls_s2, ny1, ny2, nh1, nh2) =
                self.assemble_surface(ci, class, y1, y2, q_last, &mut cd);
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

        let (a1, r1, w1) = sec.area_radius_width(y1);
        let a2 = sec.area(y2);
        let y_mid = 0.5 * (y1 + y2);
        let (a_mid, r_mid, w_mid) = sec.area_radius_width(y_mid);
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
            return (0.0, 0.5 * (a1 + a2), s1, s2, 0.0, 0.0);
        }

        // Velocity, capped (§6.3).
        let mut v = q_last / a_mid;
        if v.abs() > V_MAX {
            v = V_MAX * v.signum();
        }
        // The widths came back with the areas above; `DRY` floors the
        // depth those asked at, so a dry end still reads its own width.
        let fr = froude(v, a_mid, if y_mid >= DRY { w_mid } else { sec.width(DRY) });
        // §6.6: the kinematic-limit criterion reads the *upstream* end.
        let fr_up = froude(
            q_last / a1.max(DRY),
            a1.max(DRY),
            if y1 >= DRY { w1 } else { sec.width(DRY) },
        );

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
            c.friction.pressurised_dq(v, r_mid, dt).unwrap_or_else(|| {
                dt * GRAVITY * c.n * c.n / super::section::four_thirds(r_wtd) * v.abs()
            })
        } else {
            dt * GRAVITY * c.n * c.n / super::section::four_thirds(r_wtd) * v.abs()
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
        let mut evap_part = 0.0;
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
                let f = cap / loss_rate;
                evap *= f;
                loss_rate = cap;
            }
            evap_part = evap;
        }
        let dq_seep = 2.5 * v * loss_rate * dt / c.length;

        let denom = 1.0 + dq_friction + dq_losses;
        let mut q_new = (q_old - dq_pressure + dq_in1 + dq_in2 + dq_seep) / denom;

        // Culvert inlet control caps positive, non-full flow (§7.6);
        // otherwise the normal-flow limit applies (§6.6).
        if q_new > 0.0 && !is_full && c.culvert > 0 && c.culvert < tables::CULVERT_PARAMS.len() {
            q_new = culvert_inlet_cap(c, q_new, y1);
        } else if q_new > 0.0 && y1 < y_full && matches!(class, FlowClass::Subcritical) {
            q_new = self.normal_flow_limit(ci, q_new, y1, y2, a1, r1, fr_up);
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
        // §2.6: an outfall's gate blocks flow the receiving water would
        // push back into the network, whichever end it connects at.
        if (q_new < 0.0 && self.verts[c.to].gated) || (q_new > 0.0 && self.verts[c.from].gated) {
            q_new = 0.0;
        }
        // No flow out of a dry vertex (§6.6).
        if q_new > Q_DRY && y_vert[c.from] <= DRY {
            q_new = Q_DRY;
        }
        if q_new < -Q_DRY && y_vert[c.to] <= DRY {
            q_new = -Q_DRY;
        }

        (
            q_new * c.barrels,
            a_mid,
            s1,
            s2,
            loss_rate * c.barrels,
            evap_part * c.barrels,
        )
    }

    /// §6.6's characteristic depths for this channel and flow, computed
    /// at most once per §6.3 update. `flow_class` and `assemble_surface`
    /// both want them for the same `(ci, q)`, and each normal/critical
    /// pair is two bracketed inversions — much too heavy to solve twice
    /// for one answer.
    fn char_depths_memo(&self, ci: usize, q: f64, memo: &mut Option<(f64, f64)>) -> (f64, f64) {
        *memo.get_or_insert_with(|| self.char_depths(ci, q))
    }

    #[allow(clippy::too_many_arguments)] // the §6.6 classification's natural inputs
    fn flow_class(
        &self,
        ci: usize,
        q: f64,
        h1: f64,
        h2: f64,
        y1: f64,
        y2: f64,
        cd: &mut Option<(f64, f64)>,
    ) -> FlowClass {
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
        // Both characteristic depths are capped at the crown - normal
        // depth at the section factor's peak, which for a closed section
        // is below it, and critical depth at full depth outright - so an
        // end already at the crown cannot be under either. Testing that
        // first is worth doing because §5.7's inversions are the most
        // expensive thing in a step and a surcharged network asks for
        // them at every trial of every channel.
        let crown = c.geom.sec.y_full();
        if wet1 && wet2 {
            if q < 0.0 && z1 > 0.0 && y1 < crown {
                let (yn, yc) = self.char_depths_memo(ci, q, cd);
                if y1 < yn.min(yc) {
                    return FlowClass::UpCritical;
                }
            } else if q >= 0.0 && z2 > 0.0 && y2 < crown {
                let (yn, yc) = self.char_depths_memo(ci, q, cd);
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

    /// A channel's §6.3 flow class at the accepted state, with whether
    /// the normal-flow limiter and §7.6 culvert inlet control bind there.
    ///
    /// Recomputed from the accepted state rather than carried out of the
    /// winning trial: a step's class is a property of the state it
    /// settled on, and a trial that was rejected classified a state that
    /// never happened. It costs one classification per channel per
    /// accepted step against the several per trial the solver already
    /// pays.
    fn classify_now(&self, ci: usize) -> (FlowClass, bool, bool) {
        let c = &self.chans[ci];
        let (z1, z2) = (c.z1(&self.verts), c.z2(&self.verts));
        let h1 = (self.verts[c.from].invert + self.y[c.from]).max(z1);
        let h2 = (self.verts[c.to].invert + self.y[c.to]).max(z2);
        let y1 = (h1 - z1).max(DRY);
        let y2 = (h2 - z2).max(DRY);
        let q = self.q[ci] / c.barrels;
        let mut cd = None;
        let class = self.flow_class(ci, q, h1, h2, y1, y2, &mut cd);
        let y_full = c.geom.sec.y_full();
        let is_full = y1 >= y_full && y2 >= y_full;
        // The same either/or the channel update applies (§6.6, §7.6).
        let inlet_control =
            q > 0.0 && !is_full && c.culvert > 0 && c.culvert < tables::CULVERT_PARAMS.len();
        let norm_limited = !inlet_control
            && q > 0.0
            && y1 < y_full
            && matches!(class, FlowClass::Subcritical)
            && {
                let (a1, r1) = c.geom.area_and_radius(y1);
                let fr = froude(q / a1.max(DRY), a1.max(DRY), c.geom.width(y1.max(DRY)));
                self.normal_flow_limit(ci, q, y1, y2, a1, r1, fr) < q
            };
        (class, norm_limited, inlet_control)
    }

    /// Normal and critical depths for a per-barrel flow (§6.6).
    fn char_depths(&self, ci: usize, q: f64) -> (f64, f64) {
        let c = &self.chans[ci];
        let per_barrel = q.abs();
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
        cd: &mut Option<(f64, f64)>,
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
                    let (yn, yc) = self.char_depths_memo(ci, q, cd);
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
                let (yn, yc) = self.char_depths_memo(ci, q, cd);
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
                let (yn, yc) = self.char_depths_memo(ci, q, cd);
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
            let q_norm = c.slope.sqrt() / c.n * a1 * super::section::two_thirds(r1);
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
mod step_floor_tests {
    use super::{step_floor, DT_FLOOR_MIN};

    /// The floor is the one control a modeller has over the trade a floor
    /// step makes, so what it may be set to is worth pinning. The clamps
    /// bound both ends: a floor above the user step makes every step a
    /// floor step, and a floor at zero lets one short channel drive the
    /// step toward zero until the run stops advancing in finite time.

    #[test]
    fn the_default_is_half_a_second() {
        assert_eq!(0.5, step_floor(0.5, 30.0));
    }

    #[test]
    fn a_model_may_lower_it() {
        assert_eq!(0.05, step_floor(0.05, 30.0));
    }

    #[test]
    fn a_model_may_raise_it() {
        assert_eq!(5.0, step_floor(5.0, 30.0));
    }

    #[test]
    fn it_never_exceeds_the_user_step() {
        assert_eq!(10.0, step_floor(60.0, 10.0));
    }

    #[test]
    fn zero_becomes_the_absolute_minimum() {
        assert_eq!(DT_FLOOR_MIN, step_floor(0.0, 30.0));
    }

    #[test]
    fn the_absolute_minimum_outranks_the_user_step() {
        // The predecessor applies the two clamps in this order, so a
        // routing step under a millisecond ends with a floor above it.
        // Any other reading of that model asks for a step of zero.
        assert_eq!(DT_FLOOR_MIN, step_floor(0.5, 1e-6));
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

    // ── §6.4 criterion-2 allowance ────────────────────────────────────────
    //
    // The decision, tested by name: what summed residual counts as
    // conserving mass. The defect this guards against was an allowance
    // that collapsed with the flow while the residual's noise floor did
    // not, so a dry-weather trickle rejected every trial and pinned a
    // 36-hour run at the step floor — 134k steps of 0.5 s, 39 s of wall
    // clock, and 76k degraded-accuracy warnings on a healthy network.
    //
    // The behavioural reproduction needs hydrology-driven forcing and
    // lives at the session level: tests/session.rs,
    // a_runoff_recession_tail_converges_at_full_steps. Constant-inflow
    // miniatures settle to machine noise and pass even the broken gate.

    /// Criterion 1 accepts iterates whose heads still move by ε_H, so the
    /// mass gate must always grant at least the flow that motion
    /// represents — whatever the network's flow happens to be, including
    /// none at all.
    #[test]
    fn the_allowance_never_demands_closure_finer_than_the_head_gate() {
        let (head_tol, dt, area_sum) = (1.524e-3, 1.0, 26.0);
        let floor = head_tol / dt * area_sum;
        for flow_scale in [0.0, 1e-6, 1e-3, 1.0, 1e3] {
            assert!(
                continuity_allowance(1e-3, flow_scale, head_tol, dt, area_sum) >= floor,
                "allowance under the head-gate floor at flow {flow_scale}"
            );
        }
    }

    /// At real flow the relative term is the gate — the ε_H term must not
    /// have loosened the criterion where it used to bind.
    #[test]
    fn high_flow_is_governed_by_the_relative_term() {
        // 10 m³/s through the network, a 10 s step, modest storage.
        let allowance = continuity_allowance(1e-3, 10.0, 1.524e-3, 10.0, 30.0);
        let relative = 1e-3 * 10.0;
        assert!(allowance < 1.5 * relative, "{allowance} vs {relative}");
        assert!(allowance >= relative);
    }

    /// The observed failure, in its own numbers: a 1 L/s trickle at a 1 s
    /// step with ~26 m² of storage carried ~1.4e-5 of settled-noise
    /// residual against a 1.2e-6 allowance. The fixed allowance admits it.
    #[test]
    fn the_dry_weather_trickle_case_is_admitted() {
        let allowance = continuity_allowance(1e-3, 1.2e-3, 1.524e-3, 1.0, 26.0);
        assert!(
            allowance > 1.4e-5,
            "allowance {allowance} still rejects the trickle"
        );
    }

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

    /// An outfall opens at the depth its boundary imposes (§6.7), and the
    /// channel reaching it opens holding that water.
    ///
    /// The defect this guards: seeding skipped outfall vertices entirely,
    /// so a channel running to a staged outfall measured its opening
    /// volume with one end dry — then filled to the boundary on the very
    /// first step. The water arrived from nowhere, and because opening
    /// storage is a single snapshot the flow ledger carried the difference
    /// for the whole run rather than settling. extran8a opened 0.47 of
    /// 2.46 acre-feet short, a 19% continuity error where the predecessor
    /// managed 1%.
    #[test]
    fn a_staged_outfall_opens_at_its_boundary_depth() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.0  4

[OUTFALLS]
O1  98.0  FIXED  100.0

[CONDUITS]
C1  J1  O1  500  0.015  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2.0  0  0  0
";
        let (net, r) = build(inp);
        let o = net
            .vertices
            .iter()
            .position(|v| v.id == "O1")
            .expect("the outfall");
        // Stage 100.0 over invert 98.0 is 2 m standing against the outlet.
        assert!(
            (r.y[o] - 2.0).abs() < 1e-9,
            "outfall opened dry: {}",
            r.y[o]
        );

        // So the opening volume is the channel at the mid-depth its two
        // ends imply — 2 m against the outlet, dry at the junction, half
        // full between — and not the sliver a dry outfall left behind.
        let half_full = r.chans[0].geom.area(1.0) * 500.0;
        let dry_end = r.chans[0].geom.area(0.5) * 500.0;
        let in_channel = r.report.initial_storage
            - (0..r.verts.len())
                .map(|v| r.vertex_volume_now(v))
                .sum::<f64>();
        assert!(
            (in_channel - half_full).abs() < 1e-6 * half_full,
            "channel opened with {in_channel} m³, expected {half_full}",
        );
        assert!(
            half_full > 1.5 * dry_end,
            "fixture too weak to tell the two seedings apart",
        );
    }

    /// The surplus is the *inflow* surplus (§6.6), not the surplus of the
    /// under-relaxed iterate.
    ///
    /// The defect this guards: the overflow was measured after §6.4's
    /// relaxation had pulled the new depth back toward the old one. The
    /// vertex was pinned at its rim either way, so the difference — a
    /// factor of ω of the overflow — left the model with no ledger entry
    /// anywhere. extran6 lost 4.6 of 33.5 acre-feet that way, a 13.7%
    /// continuity error against the predecessor's 0.05%.
    ///
    /// It needs a rim at or below the highest connecting crown, because
    /// §6.4 exempts a vertex *above* its crown from relaxation — which is
    /// why `flooding_pins_the_rim_and_reports_the_surplus`, whose junction
    /// floods 0.6 m up a 0.3 m pipe, never reached the faulty branch.
    #[test]
    fn a_rim_at_the_crown_still_reports_its_whole_overflow() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
ROUTING_STEP  5

[JUNCTIONS]
J1  100.0  0.3

[OUTFALLS]
O1  99.0  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.3  0  0  0
";
        let (_, mut r) = build(inp);
        // The rim and the pipe crown coincide at 0.3 m, so the vertex is
        // never above its crown and relaxation is in force throughout.
        assert!((r.verts[0].crown - 0.3).abs() < 1e-12);
        assert!((r.verts[0].y_max - 0.3).abs() < 1e-12);

        r.advance(1800.0, &inflow_at(0, 0.2));
        assert!((r.y[0] - 0.3).abs() < 1e-9, "not pinned: y = {}", r.y[0]);
        assert!(r.report.flooding > 0.0, "nothing flooded");

        // Nothing is stored — the vertex ends where it was pinned — so the
        // inflow is wholly accounted for by what left through the pipe and
        // what left over the rim. Under the defect roughly half the
        // overflow was missing from both.
        let led = &r.report;
        let gap = led.inflow - led.outflow - led.flooding;
        assert!(
            gap.abs() < 0.01 * led.inflow,
            "unaccounted {gap:.4} m³ of in {} out {} flood {}",
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

    /// A pump declared `OFF` is off because its *setting* is zero — not
    /// because of a second on/off state that control cannot reach.
    ///
    /// The defect this guards: the initial status seeded a separate latch
    /// flag, and only the §7.1 startup and shutoff depths ever wrote it. A
    /// pump given neither depth — extran10's five, and every pump switched
    /// purely by rule — could be commanded on, would report itself open,
    /// and would still pass no water for the whole run. The node it drained
    /// filled to its maximum and flooded, for a 47% continuity error.
    ///
    /// Same fixture as `inline_pump_finds_its_operating_point`, declared
    /// off and then started, so it must reach the identical operating
    /// point: control has to be able to undo the initial status exactly.
    #[test]
    fn a_pump_declared_off_can_be_started_by_control() {
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
P1  SU1  J2  PC  OFF  0  0

[CONDUITS]
C1  J2  O1  50  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  0.5  0  0  0

[CURVES]
PC  PUMP4  0  0  2  0.2
";
        let (net, mut r) = build(inp);
        let p = net
            .links
            .iter()
            .position(|l| l.id == "P1")
            .expect("the pump");
        let q_in = 0.05;

        assert_eq!(r.is_open(p), Some(false), "declared OFF");
        r.advance(600.0, &inflow_at(0, q_in));
        assert_eq!(r.sq[0], 0.0, "an off pump passes no flow");

        assert_eq!(r.set_setting(p, 1.0), Some(true), "the setting changed");
        assert_eq!(r.is_open(p), Some(true), "control started it");
        r.advance(7800.0, &inflow_at(0, q_in));
        assert!((r.y[0] - 0.5).abs() < 0.02, "well depth {}", r.y[0]);
        assert!((r.sq[0] - q_in).abs() < 0.02 * q_in, "pump {}", r.sq[0]);
    }

    /// The startup and shutoff depths write the same setting control does,
    /// which is what makes them latch — and what stops the two from
    /// disagreeing about whether the pump is running.
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
            if r.sett[0] > 0.0 {
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

// ── Checkpointing (§12.3) ────────────────────────────────────────────────────

impl Router {
    /// Write every piece of accepted routing state (§12.3).
    ///
    /// The destructure is exhaustive on purpose. Fields bound to `_` are
    /// parameters, rebuilt from the model when the checkpoint is loaded;
    /// everything else is state and is written. A field added to `Router`
    /// fails to compile here until it has been put in one group or the
    /// other, which is the only check that does not depend on a test
    /// happening to exercise it.
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_b, put_f, put_fs, put_u};
        let Router {
            // Parameters: the model builds these.
            chans: _,
            verts: _,
            structs: _,
            ids: _,
            dt_user: _,
            dt_floor: _,
            courant_factor: _,
            max_trials: _,
            head_tol: _,
            min_surface_area: _,
            continuity_tol: _,
            err_tol: _,
            allow_ponding: _,
            normal_flow: _,
            stor_evap_frac: _,
            stor_seep_ksat: _,
            // State.
            t,
            y,
            q,
            sq,
            sett,
            sett_cur,
            chan_open,
            struct_flip_t,
            chan_flip_t,
            a_mid,
            net_flow,
            flood_now,
            chan_evap_now,
            chan_seep_now,
            stor_ga,
            stor_evap_now,
            stor_seep_now,
            hist,
            dt_prev,
            quiet_streak,
            evap_rate,
            report,
            vertex_stats,
            pump_prev_off,
            link_stats,
            worst_counts,
            stats_start,
        } = self;
        put_f(w, *t)?;
        for vs in [y, q, sq, sett, sett_cur, struct_flip_t, chan_flip_t] {
            put_fs(w, vs)?;
        }
        for vs in [a_mid, net_flow, flood_now, chan_evap_now, chan_seep_now] {
            put_fs(w, vs)?;
        }
        for vs in [stor_evap_now, stor_seep_now] {
            put_fs(w, vs)?;
        }
        for flags in [chan_open, pump_prev_off] {
            put_u(w, flags.len() as u64)?;
            for f in flags {
                put_b(w, *f)?;
            }
        }
        put_u(w, stor_ga.len() as u64)?;
        for slot in stor_ga {
            put_b(w, slot.is_some())?;
            if let Some(state) = slot {
                state.checkpoint_put(w)?;
            }
        }
        put_u(w, hist.len() as u64)?;
        for (ht, heads) in hist {
            put_f(w, *ht)?;
            put_fs(w, heads)?;
        }
        put_f(w, *dt_prev)?;
        put_u(w, u64::from(*quiet_streak))?;
        put_f(w, *evap_rate)?;
        put_f(w, *stats_start)?;
        put_u(w, worst_counts.len() as u64)?;
        for c in worst_counts {
            put_u(w, *c)?;
        }
        report.checkpoint_put(w)?;
        put_u(w, vertex_stats.len() as u64)?;
        for s in vertex_stats {
            s.checkpoint_put(w)?;
        }
        put_u(w, link_stats.len() as u64)?;
        for s in link_stats {
            s.checkpoint_put(w)?;
        }
        Ok(())
    }

    /// Restore what `checkpoint_put` wrote, over a router the model has
    /// already built. Parameters are left as the model made them.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        self.t = r.f()?;
        for slot in [
            &mut self.y,
            &mut self.q,
            &mut self.sq,
            &mut self.sett,
            &mut self.sett_cur,
            &mut self.struct_flip_t,
            &mut self.chan_flip_t,
            &mut self.a_mid,
            &mut self.net_flow,
            &mut self.flood_now,
            &mut self.chan_evap_now,
            &mut self.chan_seep_now,
            &mut self.stor_evap_now,
            &mut self.stor_seep_now,
        ] {
            let want = slot.len();
            let got = r.fs()?;
            if got.len() != want {
                return Err(format!(
                    "checkpoint holds {} values where this model has {want}",
                    got.len()
                ));
            }
            *slot = got;
        }
        for flags in [&mut self.chan_open, &mut self.pump_prev_off] {
            let n = r.u()? as usize;
            if n != flags.len() {
                return Err(format!(
                    "checkpoint holds {n} states where this model has {}",
                    flags.len()
                ));
            }
            for flag in flags.iter_mut() {
                *flag = r.b()?;
            }
        }
        let n = r.u()? as usize;
        if n != self.stor_ga.len() {
            return Err(format!(
                "checkpoint holds {n} storage infiltration states where this \
                 model has {}",
                self.stor_ga.len()
            ));
        }
        for i in 0..n {
            if r.b()? {
                match &mut self.stor_ga[i] {
                    Some(state) => state.checkpoint_get(r)?,
                    // The model decides which vertices infiltrate, so a
                    // checkpoint carrying a state the model has no slot
                    // for is a checkpoint of another model.
                    None => {
                        return Err("checkpoint infiltrates a vertex this model does not".into())
                    }
                }
            } else if self.stor_ga[i].is_some() {
                return Err("this model infiltrates a vertex the checkpoint does not".into());
            }
        }
        let n = r.u()? as usize;
        self.hist = Vec::with_capacity(n);
        for _ in 0..n {
            let ht = r.f()?;
            self.hist.push((ht, r.fs()?));
        }
        self.dt_prev = r.f()?;
        self.quiet_streak = u32::try_from(r.u()?).map_err(|_| "implausible step counter")?;
        self.evap_rate = r.f()?;
        self.stats_start = r.f()?;
        let n = r.u()? as usize;
        self.worst_counts = (0..n).map(|_| r.u()).collect::<Result<_, _>>()?;
        self.report.checkpoint_get(r)?;
        let n = r.u()? as usize;
        if n != self.vertex_stats.len() {
            return Err(format!(
                "checkpoint holds statistics for {n} vertices where this model \
                 has {}",
                self.vertex_stats.len()
            ));
        }
        for s in &mut self.vertex_stats {
            s.checkpoint_get(r)?;
        }
        let n = r.u()? as usize;
        if n != self.link_stats.len() {
            return Err(format!(
                "checkpoint holds statistics for {n} channels where this model \
                 has {}",
                self.link_stats.len()
            ));
        }
        for s in &mut self.link_stats {
            s.checkpoint_get(r)?;
        }
        Ok(())
    }
}

impl RoutingReport {
    /// Write this record (§12.3). Exhaustive by design: a field added
    /// here fails to compile until it is written.
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_f, put_u};
        let RoutingReport {
            accepted,
            rejected,
            degraded,
            inflow,
            outflow,
            flooding,
            negative_out,
            initial_storage,
            losses,
            evaporation,
            dt_min,
            dt_max,
            iterations,
            nonconverged,
            elapsed,
            dt_bands,
        } = self;
        put_u(w, *accepted)?;
        put_u(w, *rejected)?;
        put_u(w, degraded.len() as u64)?;
        for (at, who) in degraded {
            put_f(w, *at)?;
            put_u(w, who.len() as u64)?;
            w.write_all(who.as_bytes())?;
        }
        put_f(w, *inflow)?;
        put_f(w, *outflow)?;
        put_f(w, *flooding)?;
        put_f(w, *negative_out)?;
        put_f(w, *initial_storage)?;
        put_f(w, *losses)?;
        put_f(w, *evaporation)?;
        put_f(w, *dt_min)?;
        put_f(w, *dt_max)?;
        put_u(w, *iterations)?;
        put_u(w, *nonconverged)?;
        put_f(w, *elapsed)?;
        for v in dt_bands {
            put_u(w, *v)?;
        }
        Ok(())
    }

    /// Read back what `checkpoint_put` wrote.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        self.accepted = r.u()?;
        self.rejected = r.u()?;
        let n = r.u()? as usize;
        self.degraded = Vec::with_capacity(n);
        for _ in 0..n {
            let at = r.f()?;
            self.degraded.push((at, r.text()?));
        }
        self.inflow = r.f()?;
        self.outflow = r.f()?;
        self.flooding = r.f()?;
        self.negative_out = r.f()?;
        self.initial_storage = r.f()?;
        self.losses = r.f()?;
        self.evaporation = r.f()?;
        self.dt_min = r.f()?;
        self.dt_max = r.f()?;
        self.iterations = r.u()?;
        self.nonconverged = r.u()?;
        self.elapsed = r.f()?;
        for i in 0..5 {
            self.dt_bands[i] = r.u()?;
        }
        Ok(())
    }
}

impl VertexStats {
    /// Write this record (§12.3). Exhaustive by design: a field added
    /// here fails to compile until it is written.
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_f, put_u};
        let VertexStats {
            max_depth,
            t_max_depth,
            depth_sum,
            reported_max_depth,
            max_flood,
            t_max_flood,
            flood_time,
            flood_volume,
            max_ponded_volume,
            surcharge_time,
            max_crown_height,
            min_rim_depth,
            max_lat_inflow,
            max_total_inflow,
            t_max_total_inflow,
            lat_inflow_volume,
            total_inflow_volume,
            outflow_volume,
            initial_volume,
            final_volume,
            volume_sum,
            max_volume,
            t_max_volume,
            evap_loss_volume,
            exfil_loss_volume,
            max_outflow,
            full_volume,
            out_volume,
            out_peak,
            out_time,
            steps,
            obs_time,
        } = self;
        put_f(w, *max_depth)?;
        put_f(w, *t_max_depth)?;
        put_f(w, *depth_sum)?;
        put_f(w, *reported_max_depth)?;
        put_f(w, *max_flood)?;
        put_f(w, *t_max_flood)?;
        put_f(w, *flood_time)?;
        put_f(w, *flood_volume)?;
        put_f(w, *max_ponded_volume)?;
        put_f(w, *surcharge_time)?;
        put_f(w, *max_crown_height)?;
        put_f(w, *min_rim_depth)?;
        put_f(w, *max_lat_inflow)?;
        put_f(w, *max_total_inflow)?;
        put_f(w, *t_max_total_inflow)?;
        put_f(w, *lat_inflow_volume)?;
        put_f(w, *total_inflow_volume)?;
        put_f(w, *outflow_volume)?;
        put_f(w, *initial_volume)?;
        put_f(w, *final_volume)?;
        put_f(w, *volume_sum)?;
        put_f(w, *max_volume)?;
        put_f(w, *t_max_volume)?;
        put_f(w, *evap_loss_volume)?;
        put_f(w, *exfil_loss_volume)?;
        put_f(w, *max_outflow)?;
        put_f(w, *full_volume)?;
        put_f(w, *out_volume)?;
        put_f(w, *out_peak)?;
        put_f(w, *out_time)?;
        put_u(w, *steps)?;
        put_f(w, *obs_time)?;
        Ok(())
    }

    /// Read back what `checkpoint_put` wrote.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        self.max_depth = r.f()?;
        self.t_max_depth = r.f()?;
        self.depth_sum = r.f()?;
        self.reported_max_depth = r.f()?;
        self.max_flood = r.f()?;
        self.t_max_flood = r.f()?;
        self.flood_time = r.f()?;
        self.flood_volume = r.f()?;
        self.max_ponded_volume = r.f()?;
        self.surcharge_time = r.f()?;
        self.max_crown_height = r.f()?;
        self.min_rim_depth = r.f()?;
        self.max_lat_inflow = r.f()?;
        self.max_total_inflow = r.f()?;
        self.t_max_total_inflow = r.f()?;
        self.lat_inflow_volume = r.f()?;
        self.total_inflow_volume = r.f()?;
        self.outflow_volume = r.f()?;
        self.initial_volume = r.f()?;
        self.final_volume = r.f()?;
        self.volume_sum = r.f()?;
        self.max_volume = r.f()?;
        self.t_max_volume = r.f()?;
        self.evap_loss_volume = r.f()?;
        self.exfil_loss_volume = r.f()?;
        self.max_outflow = r.f()?;
        self.full_volume = r.f()?;
        self.out_volume = r.f()?;
        self.out_peak = r.f()?;
        self.out_time = r.f()?;
        self.steps = r.u()?;
        self.obs_time = r.f()?;
        Ok(())
    }
}

impl LinkStats {
    /// Write this record (§12.3). Exhaustive by design: a field added
    /// here fails to compile until it is written.
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_f, put_u};
        let LinkStats {
            max_flow,
            t_max_flow,
            max_velocity,
            max_depth,
            full_time,
            obs_time,
            class_time,
            norm_limited_time,
            inlet_control_time,
            full_both_time,
            full_up_time,
            full_down_time,
            above_normal_time,
            capacity_limited_time,
            full_depth,
            full_flow,
            instability_count,
            steps,
            prev_flow,
            prev_delta,
            on_time,
            startups,
            min_flow,
            max_pump_flow,
            volume,
            energy_kwh,
            off_low_time,
            off_high_time,
        } = self;
        put_f(w, *max_flow)?;
        put_f(w, *t_max_flow)?;
        put_f(w, *max_velocity)?;
        put_f(w, *max_depth)?;
        put_f(w, *full_time)?;
        put_f(w, *obs_time)?;
        for v in class_time {
            put_f(w, *v)?;
        }
        put_f(w, *norm_limited_time)?;
        put_f(w, *inlet_control_time)?;
        put_f(w, *full_both_time)?;
        put_f(w, *full_up_time)?;
        put_f(w, *full_down_time)?;
        put_f(w, *above_normal_time)?;
        put_f(w, *capacity_limited_time)?;
        put_f(w, *full_depth)?;
        put_f(w, *full_flow)?;
        put_u(w, *instability_count)?;
        put_u(w, *steps)?;
        put_f(w, *prev_flow)?;
        put_f(w, *prev_delta)?;
        put_f(w, *on_time)?;
        put_u(w, u64::from(*startups))?;
        put_f(w, *min_flow)?;
        put_f(w, *max_pump_flow)?;
        put_f(w, *volume)?;
        put_f(w, *energy_kwh)?;
        put_f(w, *off_low_time)?;
        put_f(w, *off_high_time)?;
        Ok(())
    }

    /// Read back what `checkpoint_put` wrote.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        self.max_flow = r.f()?;
        self.t_max_flow = r.f()?;
        self.max_velocity = r.f()?;
        self.max_depth = r.f()?;
        self.full_time = r.f()?;
        self.obs_time = r.f()?;
        for i in 0..7 {
            self.class_time[i] = r.f()?;
        }
        self.norm_limited_time = r.f()?;
        self.inlet_control_time = r.f()?;
        self.full_both_time = r.f()?;
        self.full_up_time = r.f()?;
        self.full_down_time = r.f()?;
        self.above_normal_time = r.f()?;
        self.capacity_limited_time = r.f()?;
        self.full_depth = r.f()?;
        self.full_flow = r.f()?;
        self.instability_count = r.u()?;
        self.steps = r.u()?;
        self.prev_flow = r.f()?;
        self.prev_delta = r.f()?;
        self.on_time = r.f()?;
        self.startups = u32::try_from(r.u()?).map_err(|_| "implausible count")?;
        self.min_flow = r.f()?;
        self.max_pump_flow = r.f()?;
        self.volume = r.f()?;
        self.energy_kwh = r.f()?;
        self.off_low_time = r.f()?;
        self.off_high_time = r.f()?;
        Ok(())
    }
}

// `just mutants crates/engine-uds/src/hydraulics/routing.rs` reports two
// mutants in the geometry and friction relations that no test catches, and
// both are equivalent rather than uncovered:
//
//   `re <= 2000.0` read as `<`, which decides between the laminar law and
//   the transitional blend at exactly 2000. The blend begins at 0.032 and
//   64/2000 is 0.032, so the two regimes meet there and both readings
//   return the same number;
//
//   `yy <= 0.0` read as `<` when a storage table opens below the invert.
//   A point at exactly zero depth contributes no volume either way — the
//   running lower bound is already zero, so the trapezoid it would add has
//   no width — and the area it carries is picked up by both paths;
//
//   `top_width(m) > w_slot` read as `>=` inside the slot's bisection,
//   which decides a branch only when the width lands exactly on the slot
//   width, and the bisection converges to the same crossing either way;
//
//   `w_slot == 0.0 || y <= y_x` read with `&&` in `area_and_radius`. Both
//   routes out of that test return the section's own area and radius
//   below the crossing depth — one through the shared pass and one
//   through the two functions separately — and a test asserts they agree,
//   which is what makes the mutation unobservable;
//
//   `x <= points[0].0` read as `<` in the linear table lookup: on the
//   first abscissa the interpolation below returns a fraction of zero,
//   which is the first point's own value again;
//
//   `*init_flow > 0.0` read as `>=` when seeding §6.7 initial state. A
//   zero flow gives a zero section factor, whose normal depth is zero, so
//   the end depth is unchanged and the dry-link guard below skips the
//   link either way;
//
//   the `.abs()` on a channel's elevation drop. Validation reverses an
//   adverse channel before the router is built (§14.7), comparing end
//   elevations with their offsets already added and swapping the
//   endpoints and the offsets together, so every channel reaching here
//   already falls. The call is defensive and cannot be observed; the
//   reversal it stands behind is tested at session level instead.
//
// Everything else the tool suggests about these two is caught. When this
// file changes, run it again rather than trusting this note.

#[cfg(test)]
mod storage_geometry_tests {
    use super::*;

    /// The three storage geometries at a shape each, chosen so no two
    /// agree anywhere but the origin.
    fn geometries() -> Vec<(&'static str, StoreArea)> {
        vec![
            (
                "functional",
                StoreArea::Functional {
                    coeff: 30.0,
                    exponent: 0.5,
                    constant: 12.0,
                },
            ),
            (
                "shape",
                StoreArea::Shape {
                    a0: 8.0,
                    a1: 5.0,
                    a2: 2.0,
                },
            ),
            (
                "table",
                StoreArea::Table(vec![(0.0, 10.0), (1.0, 25.0), (2.5, 40.0), (4.0, 55.0)]),
            ),
        ]
    }

    /// Volume is the integral of area over depth.
    ///
    /// The two are written separately, in closed form for the first two
    /// geometries and as a running trapezoid for the third, so nothing
    /// makes them agree except being right. Integrating one numerically
    /// and comparing it against the other is the claim that binds them,
    /// and it holds a storage node's mass balance together.
    #[test]
    fn the_volume_is_the_integral_of_the_area() {
        for (what, g) in geometries() {
            for &depth in &[0.5, 1.0, 2.0, 3.3, 4.0] {
                let n = 20_000;
                let h = depth / n as f64;
                // Midpoint rule: exact for the linear pieces, and second
                // order on the curved ones.
                let mut integral = 0.0;
                for i in 0..n {
                    integral += g.area((i as f64 + 0.5) * h) * h;
                }
                let got = g.volume(depth);
                assert!(
                    (got - integral).abs() < 1e-6 * integral.max(1.0),
                    "{what} at {depth} m: volume {got} against the integral {integral}"
                );
            }
        }
    }

    /// A depth below the invert is no depth: neither function extrapolates
    /// backwards into a negative store.
    #[test]
    fn a_negative_depth_is_an_empty_store() {
        for (what, g) in geometries() {
            assert_eq!(g.volume(-1.0), g.volume(0.0), "{what} volume");
            assert_eq!(g.area(-1.0), g.area(0.0), "{what} area");
            assert_eq!(0.0, g.volume(0.0), "{what} holds nothing at the invert");
        }
    }

    /// The functional form is $A = A_0 + a y^b$ and its integral, both
    /// asserted against the relation rather than against each other.
    #[test]
    fn the_functional_form_is_its_relation_and_its_integral() {
        let g = StoreArea::Functional {
            coeff: 30.0,
            exponent: 0.5,
            constant: 12.0,
        };
        let y = 4.0_f64;
        assert!((g.area(y) - (12.0 + 30.0 * y.powf(0.5))).abs() < 1e-12);
        assert!(
            (g.volume(y) - (12.0 * y + 30.0 * y.powf(1.5) / 1.5)).abs() < 1e-12,
            "{}",
            g.volume(y)
        );
        // A constant-area store is a prism, which the exponent's +1 is
        // what makes it: coefficient zero leaves the constant alone.
        let flat = StoreArea::Functional {
            coeff: 0.0,
            exponent: 0.5,
            constant: 12.0,
        };
        assert!((flat.volume(3.0) - 36.0).abs() < 1e-12);
    }

    /// The polynomial form is $A = a_0 + a_1 y + a_2 y^2$ and its
    /// integral $a_0 y + a_1 y^2/2 + a_2 y^3/3$.
    #[test]
    fn the_polynomial_form_is_its_relation_and_its_integral() {
        let g = StoreArea::Shape {
            a0: 8.0,
            a1: 5.0,
            a2: 2.0,
        };
        let y = 3.0;
        assert!((g.area(y) - (8.0 + 5.0 * y + 2.0 * y * y)).abs() < 1e-12);
        let expected = 8.0 * y + 5.0 * y * y / 2.0 + 2.0 * y * y * y / 3.0;
        assert!((g.volume(y) - expected).abs() < 1e-12, "{}", g.volume(y));
    }

    /// A tabulated store interpolates between its points and extends flat
    /// beyond both ends: a depth past the last point keeps the last area
    /// rather than running off the table or back to zero.
    #[test]
    fn a_tabulated_store_interpolates_and_extends_flat() {
        let g = StoreArea::Table(vec![(0.0, 10.0), (1.0, 25.0), (2.5, 40.0)]);

        assert!((g.area(0.0) - 10.0).abs() < 1e-12, "on the first point");
        assert!((g.area(1.0) - 25.0).abs() < 1e-12, "on a middle point");
        assert!((g.area(0.5) - 17.5).abs() < 1e-12, "halfway to it");
        // Two thirds of the way from 1.0 to 2.5.
        assert!((g.area(2.0) - 35.0).abs() < 1e-12, "{}", g.area(2.0));
        assert!(
            (g.area(9.0) - 40.0).abs() < 1e-12,
            "flat past the last point"
        );

        // And the volume follows: past the table the store gains the last
        // area per metre and nothing else.
        let at_top = g.volume(2.5);
        assert!(
            (g.volume(3.5) - (at_top + 40.0)).abs() < 1e-12,
            "{} against {}",
            g.volume(3.5),
            at_top + 40.0
        );
    }

    /// A table that opens below the invert contributes nothing from the
    /// part below it: the store starts at zero depth whatever the curve
    /// says underneath.
    #[test]
    fn a_table_reaching_below_the_invert_starts_at_the_invert() {
        let g = StoreArea::Table(vec![(-1.0, 5.0), (0.0, 10.0), (2.0, 30.0)]);
        assert!((g.area(0.0) - 10.0).abs() < 1e-12, "the invert's own area");
        assert!((g.volume(0.0)).abs() < 1e-12, "and no volume at it");
        // Trapezoid from 10 to 30 over two metres.
        assert!((g.volume(2.0) - 40.0).abs() < 1e-12, "{}", g.volume(2.0));
    }

    /// An empty table is an empty store rather than a panic.
    #[test]
    fn an_empty_table_is_an_empty_store() {
        let g = StoreArea::Table(Vec::new());
        assert_eq!(0.0, g.area(1.0));
        assert_eq!(0.0, g.volume(1.0));
    }
}

#[cfg(test)]
mod friction_factor_tests {
    use super::*;

    /// §7.7: laminar below a Reynolds number of 2000, $f = 64/Re$.
    #[test]
    fn the_laminar_regime_is_sixty_four_over_reynolds() {
        for re in [100.0, 1000.0, 1999.0] {
            let f = swamee_jain(1.0e-3, 0.25, re);
            assert!((f - 64.0 / re).abs() < 1e-15, "at {re}: {f}");
        }
        // The regime is inclusive of its own end, and the friction factor
        // is continuous across it: the blend above starts at 0.032, which
        // is exactly 64/2000, so both readings of the boundary give the
        // same number. That continuity is the property worth having; it
        // also makes the comparison itself untestable, which the note at
        // the head of this module records.
        assert!((swamee_jain(1.0e-3, 0.25, 2000.0) - 64.0 / 2000.0).abs() < 1e-15);
        assert!(
            (64.0_f64 / 2000.0 - 0.032).abs() < 1e-15,
            "the two regimes meet"
        );
    }

    /// §7.7: a linear blend from 0.032 to the turbulent value between
    /// 2000 and 4000.
    #[test]
    fn the_transitional_regime_blends_linearly_to_the_turbulent_one() {
        let (e, hrad) = (1.0e-3, 0.25);
        let at_4000 = swamee_jain(e, hrad, 4000.0);
        let blend = |re: f64| 0.032 + (at_4000 - 0.032) * (re - 2000.0) / 2000.0;

        for re in [2001.0, 2500.0, 3000.0, 3999.0] {
            let f = swamee_jain(e, hrad, re);
            assert!(
                (f - blend(re)).abs() < 1e-15,
                "at {re}: {f} not {}",
                blend(re)
            );
        }
        // Halfway across is halfway between the two ends.
        let mid = swamee_jain(e, hrad, 3000.0);
        assert!((mid - (0.032 + at_4000) / 2.0).abs() < 1e-15, "{mid}");
    }

    /// §7.7: the Swamee-Jain form itself above 4000, which approximates
    /// Colebrook-White in closed form.
    #[test]
    fn the_turbulent_regime_is_the_swamee_jain_form() {
        let (e, hrad, re) = (1.5e-3, 0.3_f64, 1.0e5_f64);
        let x = e / 3.7 / (4.0 * hrad) + 5.74 / re.powf(0.9);
        let expected = 0.25 / (x.log10() * x.log10());
        let f = swamee_jain(e, hrad, re);
        assert!((f - expected).abs() < 1e-15, "{f} not {expected}");

        // Rougher pipe, more friction; larger pipe, less.
        assert!(swamee_jain(3.0e-3, hrad, re) > f, "roughness raises it");
        assert!(swamee_jain(e, 0.6, re) < f, "a larger radius lowers it");
    }

    /// §7.7: at extreme Reynolds numbers the fully rough form applies,
    /// the Reynolds term having stopped mattering.
    #[test]
    fn the_fully_rough_form_drops_the_reynolds_term() {
        let (e, hrad) = (1.5e-3, 0.3_f64);
        let x = e / 3.7 / (4.0 * hrad);
        let expected = 0.25 / (x.log10() * x.log10());
        let f = swamee_jain(e, hrad, 1.0e10);
        assert!((f - expected).abs() < 1e-15, "{f} not {expected}");

        // Just below the threshold the term is still there, and it makes
        // a difference the comparison can see.
        assert!(swamee_jain(e, hrad, 9.9e9) > f, "the term is still carried");
    }
}

#[cfg(test)]
mod table_helper_tests {
    use super::*;

    /// §7.1: a stepwise table holds each value until the next abscissa is
    /// passed, so the value returned is the first point *strictly beyond*
    /// the argument. On a breakpoint the earlier interval has not ended.
    #[test]
    fn a_stepwise_table_holds_each_value_to_its_own_breakpoint() {
        let t = [(0.0, 10.0), (1.0, 20.0), (2.0, 30.0)];
        assert_eq!(20.0, interval_lookup(&t, 0.0), "on the first breakpoint");
        assert_eq!(20.0, interval_lookup(&t, 0.5), "inside the first interval");
        assert_eq!(30.0, interval_lookup(&t, 1.0), "on the second");
        assert_eq!(30.0, interval_lookup(&t, 1.9), "inside the second");
        assert_eq!(10.0, interval_lookup(&t, -1.0), "before the table");
        // Past the end the last value stands rather than falling to zero.
        assert_eq!(30.0, interval_lookup(&t, 99.0), "beyond the table");
        assert_eq!(0.0, interval_lookup(&[], 1.0), "an empty table is zero");
    }

    /// A linear table interpolates between its points and clamps flat at
    /// both ends rather than extrapolating.
    #[test]
    fn a_linear_table_interpolates_and_clamps_at_its_ends() {
        // The table deliberately does not open at the origin, and no two
        // of its intervals share a value: a table starting at zero makes
        // an interpolation reading `x + x0` indistinguishable from
        // `x - x0`, and a flat interval hides the slope entirely.
        let t = [(1.0, 10.0), (3.0, 30.0), (4.0, 34.0)];
        assert_eq!(10.0, linear_lookup(&t, -5.0), "before the table");
        assert_eq!(10.0, linear_lookup(&t, 1.0), "on the first point");
        assert!((linear_lookup(&t, 2.0) - 20.0).abs() < 1e-12, "halfway");
        assert!(
            (linear_lookup(&t, 1.5) - 15.0).abs() < 1e-12,
            "a quarter in"
        );
        assert_eq!(30.0, linear_lookup(&t, 3.0), "on an interior point");
        assert!(
            (linear_lookup(&t, 3.5) - 32.0).abs() < 1e-12,
            "the next interval"
        );
        assert_eq!(34.0, linear_lookup(&t, 99.0), "beyond the table");
        assert_eq!(0.0, linear_lookup(&[], 1.0), "an empty table is zero");
    }

    /// §11.2: six edges spanning the routing step down to the floor,
    /// largest first and spaced logarithmically, so the five intervals
    /// the report prints have equal ratios.
    #[test]
    fn the_step_bands_span_the_range_logarithmically() {
        let (top, floor) = (30.0, 0.3);
        let e = step_bands(top, floor);
        assert!((e[0] - top).abs() < 1e-12, "the first edge is the step");
        assert!(
            (e[5] - floor).abs() < 1e-12,
            "the last is the floor: {}",
            e[5]
        );
        // Five equal ratios, which is what "logarithmically" means here.
        let r = (floor / top).powf(0.2);
        for k in 1..6 {
            assert!((e[k] - e[k - 1] * r).abs() < 1e-12, "edge {k}");
        }
        assert!(e.windows(2).all(|w| w[0] > w[1]), "largest first");
    }

    /// A floor above the step leaves no range to divide: the bands
    /// collapse rather than inverting.
    #[test]
    fn a_floor_above_the_step_collapses_the_bands() {
        let e = step_bands(1.0, 5.0);
        assert!(e.iter().all(|&x| (x - 5.0).abs() < 1e-12), "{e:?}");
    }

    /// A step falls in the first band whose lower edge it clears, and
    /// anything at or below the floor falls in the last.
    #[test]
    fn a_step_falls_in_the_band_that_holds_it() {
        let (top, floor) = (30.0, 0.3);
        let e = step_bands(top, floor);
        assert_eq!(0, step_band(top, top, floor), "the whole step");
        assert_eq!(
            0,
            step_band(e[1] * 1.01, top, floor),
            "just inside the first"
        );
        assert_eq!(1, step_band(e[1], top, floor), "on the first lower edge");
        assert_eq!(4, step_band(floor, top, floor), "the floor itself");
        assert_eq!(4, step_band(floor / 10.0, top, floor), "and below it");
        // Every band is reachable, so none of the five is dead.
        let bands: Vec<usize> = (0..5)
            .map(|k| step_band(0.5 * (e[k] + e[k + 1]), top, floor))
            .collect();
        assert_eq!(vec![0, 1, 2, 3, 4], bands, "each interval's midpoint");
    }
}

#[cfg(test)]
mod slot_geometry_tests {
    use super::*;
    use crate::hydraulics::section::build_section;
    use crate::model::XsectShape;
    use std::f64::consts::PI;

    fn section(shape: XsectShape, geom: [f64; 4]) -> Section {
        build_section(shape, geom, 1.0, None)
            .expect("a section")
            .section
    }

    fn circle(diameter: f64) -> Section {
        section(XsectShape::Circular, [diameter, 0.0, 0.0, 0.0])
    }

    /// §6.2: the slot width is derived from a stated celerity rather than
    /// posited, $w_{slot} = gA_{full}/c^2$. The specification works the
    /// example through: a one-metre circular channel at the default
    /// 50 m/s gets a slot 3.081 mm wide.
    #[test]
    fn the_slot_width_comes_from_the_celerity() {
        let g = SlotGeom::build(circle(1.0), 50.0);
        let expected = GRAVITY * (PI / 4.0) / (50.0 * 50.0);
        assert!((g.w_slot - expected).abs() < 1e-15, "{}", g.w_slot);
        assert!(
            (g.w_slot - 3.081e-3).abs() < 1e-6,
            "the specification's worked example: {} m",
            g.w_slot
        );

        // Faster waves mean a narrower slot, and the relation is inverse
        // square rather than merely decreasing.
        let quick = SlotGeom::build(circle(1.0), 100.0);
        assert!(
            (quick.w_slot - g.w_slot / 4.0).abs() < 1e-18,
            "{}",
            quick.w_slot
        );
    }

    /// An open section carries no slot, and its geometry is the section's
    /// own untouched.
    #[test]
    fn an_open_section_carries_no_slot() {
        let open = section(XsectShape::RectOpen, [2.0, 3.0, 0.0, 0.0]);
        let g = SlotGeom::build(open.clone(), 50.0);
        assert_eq!(0.0, g.w_slot);
        for y in [0.1, 1.0, 1.9, 5.0] {
            assert_eq!(open.top_width(y), g.width(y), "width at {y}");
            assert_eq!(open.area(y.min(open.y_full())), g.area(y), "area at {y}");
        }
    }

    /// §6.2, and the consistency §5.1 asks for: the slot-modified area
    /// integrates the floored width, so $\\tilde W = d\\tilde A/dy$ holds
    /// *through the crown band*, which is the part the slot invents and
    /// the part nothing else checks.
    #[test]
    fn the_width_is_the_derivative_of_the_area_everywhere() {
        for sec in [
            circle(1.0),
            section(XsectShape::RectClosed, [1.5, 2.0, 0.0, 0.0]),
            section(XsectShape::Circular, [0.3, 0.0, 0.0, 0.0]),
        ] {
            let y_full = sec.y_full();
            let g = SlotGeom::build(sec, 50.0);
            let h = 1.0e-7;
            // Swept from the engine's own dry threshold upward, and
            // deliberately over the crossing depth and the crown, where
            // the three arms meet.
            //
            // Below `DRY` the two disagree, and knowingly: the floored
            // width applies from zero up, while the area takes the slot
            // correction only above the crossing depth, so a closed
            // section whose true width has not yet reached the slot width
            // integrates to less than the floor. For a one-metre circular
            // channel that region ends at 2.4 micrometres, against a dry
            // threshold of 305. It is two orders of magnitude inside the
            // depth at which the vertex is dry and no update runs.
            let mut y = DRY;
            while y < 1.4 * y_full {
                let slope = (g.area(y + h) - g.area(y - h)) / (2.0 * h);
                let w = g.width(y);
                assert!(
                    (slope - w).abs() < 1.0e-4 * w.max(1.0e-4),
                    "at {y} of {y_full}: dA/dy is {slope}, width is {w}"
                );
                y += y_full / 97.0;
            }
        }
    }

    /// The area has no step in it: the three arms meet at the crossing
    /// depth and at the crown, which is what makes the vertex update
    /// well-posed at all.
    #[test]
    fn the_area_is_continuous_across_both_joins() {
        let g = SlotGeom::build(circle(1.0), 50.0);
        let y_full = g.sec.y_full();
        assert!(g.y_x < y_full, "the crossing depth is below the crown");
        let h = 1.0e-9;
        for (join, what) in [(g.y_x, "the crossing depth"), (y_full, "the crown")] {
            let below = g.area(join - h);
            let above = g.area(join + h);
            assert!(
                (above - below).abs() < 1.0e-6,
                "{what}: {below} jumps to {above}"
            );
        }
    }

    /// §6.2: above the crown the slot alone carries the water, and the
    /// hydraulic radius holds at its full value because the slot is
    /// storage rather than conveyance.
    #[test]
    fn above_the_crown_the_slot_is_the_whole_geometry() {
        let g = SlotGeom::build(circle(1.0), 50.0);
        let y_full = g.sec.y_full();
        let full_area = g.area(y_full);

        for surcharge in [0.001, 0.5, 1.0, 10.0] {
            let y = y_full + surcharge;
            assert!(
                (g.width(y) - g.w_slot).abs() < 1e-18,
                "width at {y} is {}",
                g.width(y)
            );
            assert!(
                (g.area(y) - (full_area + g.w_slot * surcharge)).abs() < 1e-12,
                "area at {y} is {}",
                g.area(y)
            );
            let (a, r) = g.area_and_radius(y);
            assert!((a - g.area(y)).abs() < 1e-18, "the pair agrees on area");
            assert!((r - g.sec.r_full()).abs() < 1e-18, "radius holds full");
        }

        // A metre of surcharge on a one-metre pipe stores well under a
        // percent of the full area: the storage artefact the celerity
        // bounds (§6.2).
        assert!(
            g.w_slot / (PI / 4.0) < 0.005,
            "slot storage is {} of the full area per metre",
            g.w_slot / (PI / 4.0)
        );
    }

    /// The pair that shares a pass must agree with the two functions it
    /// stands in for, at every depth and in every arm.
    #[test]
    fn the_shared_pass_agrees_with_asking_separately() {
        let sec = circle(1.0);
        let y_full = sec.y_full();
        let g = SlotGeom::build(sec.clone(), 50.0);
        let mut y = 0.01;
        while y < 1.3 * y_full {
            let (a, r) = g.area_and_radius(y);
            assert!((a - g.area(y)).abs() < 1e-15, "area at {y}");
            let expected_r = if y >= y_full {
                sec.r_full()
            } else {
                sec.hyd_radius(y)
            };
            assert!((r - expected_r).abs() < 1e-15, "radius at {y}: {r}");
            y += y_full / 53.0;
        }
    }

    /// A section whose width never falls to the slot width has no crown
    /// band to correct for, and must not go looking for a crossing that
    /// is not there.
    #[test]
    fn a_lid_wider_than_the_slot_has_no_crown_band() {
        // A rectangular closed section keeps its full width to the crown.
        let g = SlotGeom::build(section(XsectShape::RectClosed, [1.0, 2.0, 0.0, 0.0]), 50.0);
        assert_eq!(g.y_x, g.sec.y_full(), "the crossing is the crown itself");
        assert_eq!(0.0, g.band_full, "and there is no band to correct");
        // Its area is the section's right up to the crown.
        let y_full = g.sec.y_full();
        assert!((g.area(0.5 * y_full) - g.sec.area(0.5 * y_full)).abs() < 1e-15);
        assert!((g.area(y_full) - g.sec.a_full()).abs() < 1e-15);
    }
}
