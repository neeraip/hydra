//! The section-property contract (§5.1) over the analytic families (§5.2)
//! and custom shapes (§5.5), with the inversions of §5.7.
//!
//! Every section supplies area, top width, wetted perimeter, hydraulic
//! radius, and the Manning section factor $\Psi = A R^{2/3}$ as functions
//! of depth, evaluated in closed form wherever one exists — none of the
//! predecessor's normalised tables, fitted seeds, or capped iterations are
//! carried (§5.2). Full-depth constants follow the predecessor's
//! conventions (a flat lid counts as wetted perimeter at exactly full
//! depth); $\Psi_{max}$ and its depth are computed from the geometry, not
//! taken from fitted multipliers.

use super::{tables, GRAVITY};
use crate::model::XsectShape;

/// Exact feet-to-metres, for catalogue rows published in US customary
/// dimensions regardless of the file's unit system (§5.4).
const FT: f64 = 0.3048;

/// Why a cross-section's geometry was refused — the predecessor's
/// `xsect_setParams` accept-set, verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    /// A geometry parameter outside its accepted range.
    BadGeometry(&'static str),
    /// A shape that this entry point does not build. Transect-backed
    /// sections (§5.6) carry no geometry of their own, so they are built
    /// through `build_transect_section` / `build_street_section` instead of
    /// the generic path; reaching that path with one is a caller error, not
    /// a missing capability.
    Unsupported(&'static str),
}

/// A constructed section, plus any §14.7 mutation applied on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionBuild {
    /// The section.
    pub section: Section,
    /// A rectangular-round or basket-handle bottom radius smaller than the
    /// geometric minimum was enlarged to it; the new radius (m).
    pub radius_raised: Option<f64>,
}

/// A cross-section with its full-depth constants precomputed.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    kind: Kind,
    y_full: f64,
    a_full: f64,
    r_full: f64,
    /// Top width at full depth. Stored because a closed section spends
    /// much of a surcharged run at its crown, and asking for it there
    /// costs the same inverse cosine as anywhere else for a value that
    /// never changes.
    w_full: f64,
    w_max: f64,
    y_at_w_max: f64,
    psi_max: f64,
    y_at_psi_max: f64,
    /// The critical-depth relation at full depth, $A^3/W$ evaluated once
    /// at build — a constant of the section that the solve's full check
    /// was re-deriving on every call.
    crit_full: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Dummy,
    /// Diameter.
    Circle {
        d: f64,
    },
    /// A circle with a sediment-filled bottom; depth is measured above the
    /// fill. Derived fill constants are stored.
    FilledCircle {
        d: f64,
        y_bot: f64,
        a_bot: f64,
        p_bot: f64,
        w_bot: f64,
    },
    /// Width; flat lid at full depth.
    RectClosed {
        w: f64,
    },
    /// Width and the number of side walls excluded from wetted perimeter.
    RectOpen {
        w: f64,
        sides_ignored: f64,
    },
    /// Half-slope `s` (run per rise per side).
    Triangle {
        s: f64,
    },
    /// Bottom width and the two side slopes.
    Trapezoid {
        b: f64,
        sl: f64,
        sr: f64,
    },
    /// Half-width coefficient: half-width $x = k\sqrt{y}$.
    Parabola {
        k: f64,
    },
    /// Half-width $x = h\,y^{m}$.
    Power {
        h: f64,
        m: f64,
    },
    /// Triangular bottom of height `y_bot` under rectangular walls of
    /// width `w`; flat lid.
    RectTriangle {
        w: f64,
        y_bot: f64,
        s: f64,
    },
    /// Circular-segment bottom of radius `r` under rectangular walls of
    /// width `w`; flat lid.
    RectRound {
        w: f64,
        r: f64,
        y_bot: f64,
        a_bot: f64,
        theta: f64,
    },
    /// Rectangular walls of floor width `w` under a circular-arc roof of
    /// radius `r` and rise `y_bot`.
    ModBasket {
        w: f64,
        r: f64,
        y_bot: f64,
        a_bot: f64,
        theta: f64,
    },
    /// Semi-axes: `a` horizontal, `b` vertical. Catalogue-anchored coded
    /// sections carry scale factors landing the analytic full-flow values
    /// on the published ones (§5.4); arbitrary axes carry 1.
    Ellipse {
        a: f64,
        b: f64,
        area_scale: f64,
        r_scale: f64,
    },
    /// A tabulated family (§5.3): the tables are the shape, anchored by
    /// the family's full-flow constants or a catalogue row (§5.4).
    Tabulated {
        family: TabFamily,
        a_full: f64,
        r_full: f64,
        w_max: f64,
        yw_max: f64,
    },
    /// Piecewise-linear width against depth (§5.5), already scaled to
    /// metres and closed at the top.
    Custom {
        ys: Vec<f64>,
        ws: Vec<f64>,
    },
    /// A surveyed transect (§5.6), evaluated directly from its geometry.
    Transect(TransectGeom),
}

/// A transect's evaluation-ready geometry (§5.6): the survey polyline
/// with vertical end walls, elevations relative to the invert, compiled
/// to per-segment constants (run, ordered end elevations, slant length,
/// zone roughness, sub-section boundaries) so a sweep re-derives nothing
/// that does not depend on the water line.
#[derive(Debug, Clone, PartialEq)]
pub struct TransectGeom {
    n_channel: f64,
    segs: Vec<TransectSeg>,
}

/// One survey segment, everything about it that is independent of depth.
#[derive(Debug, Clone, PartialEq)]
struct TransectSeg {
    /// Horizontal run |x₁ − x₀| (m).
    dx: f64,
    /// End elevations, ordered.
    lo: f64,
    hi: f64,
    /// Full slant length √(dx² + (hi − lo)²) (m).
    wp_full: f64,
    /// The end station's elevation — the re-emergence check reads the
    /// polyline's own order, not the sorted pair.
    z1: f64,
    /// Zone roughness, by the predecessor's rule.
    n: f64,
    /// A bank-roughness boundary ends the sub-section after this segment.
    flush_bank: bool,
    /// The lowest `lo` from this segment onward — when the water line
    /// sits at or below it, nothing ahead is wet and a walk with empty
    /// accumulators can stop.
    min_lo_ahead: f64,
}

/// One depth's sweep with its depth-derivatives (§5.7): what the
/// characteristic-depth Newton steps read.
struct SweepD {
    area: f64,
    width: f64,
    /// d(width)/dy — from the partly-submerged slices; a submerged
    /// segment's width is constant.
    dw: f64,
    /// Conveyance sum over sub-sections.
    k: f64,
    /// d(conveyance)/dy.
    dk: f64,
}

impl TransectGeom {
    fn new(
        xs: &[f64],
        zs: &[f64],
        x_left: f64,
        x_right: f64,
        n_left: f64,
        n_right: f64,
        n_channel: f64,
    ) -> TransectGeom {
        let n = xs.len();
        let mut segs = Vec::with_capacity(n.saturating_sub(1));
        for k in 1..n {
            let (z0, z1) = (zs[k - 1], zs[k]);
            let (lo, hi) = if z0 <= z1 { (z0, z1) } else { (z1, z0) };
            // Manning n by the predecessor's zone rule.
            let n_seg = if xs[k - 1] < x_left {
                n_left
            } else if xs[k] > x_right {
                n_right
            } else {
                n_channel
            };
            let dx = (xs[k] - xs[k - 1]).abs();
            // A bank-roughness change ends the sub-section (a vertical
            // bank wall stays with its channel side, as the predecessor
            // keeps it).
            let flush_bank = k + 1 < n && {
                let at_left = xs[k] == x_left && n_left != n_channel && xs[k] != xs[k - 1];
                let at_right = xs[k] == x_right && n_right != n_channel && xs[k] != xs[k + 1];
                at_left || at_right
            };
            segs.push(TransectSeg {
                dx,
                lo,
                hi,
                wp_full: (dx * dx + (hi - lo) * (hi - lo)).sqrt(),
                z1,
                n: n_seg,
                flush_bank,
                min_lo_ahead: 0.0,
            });
        }
        let mut min_lo = f64::INFINITY;
        for sg in segs.iter_mut().rev() {
            min_lo = min_lo.min(sg.lo);
            sg.min_lo_ahead = min_lo;
        }
        TransectGeom { n_channel, segs }
    }

    /// Sweep the polyline at depth `y`: total area, top width, geometric
    /// wetted perimeter, and the conveyance sum over sub-sections — a new
    /// sub-section at each bank-roughness change and wherever ground
    /// re-emerges above the water line (§5.6).
    fn sweep(&self, y: f64) -> (f64, f64, f64, f64) {
        let (mut area, mut width, mut perim, mut k_sum) = (0.0, 0.0, 0.0, 0.0);
        // The running sub-section.
        let (mut a_t, mut p_t) = (0.0, 0.0);
        let mut flush = |a_t: &mut f64, p_t: &mut f64, n: f64| {
            if *a_t > 0.0 && *p_t > 0.0 {
                k_sum += (1.0 / n) * *a_t * (*a_t / *p_t).powf(2.0 / 3.0);
            }
            *a_t = 0.0;
            *p_t = 0.0;
        };
        for sg in &self.segs {
            // Nothing ahead is wet and nothing is pending: every further
            // step would add zero and flush nothing.
            if a_t == 0.0 && sg.min_lo_ahead >= y {
                break;
            }
            if sg.lo < y {
                let mut w = sg.dx;
                let mut wp = sg.wp_full;
                let a;
                if y > sg.hi {
                    a = sg.dx * ((y - sg.hi) + (y - sg.lo)) / 2.0;
                } else {
                    // Partly submerged slice.
                    let ratio = (y - sg.lo) / (sg.hi - sg.lo);
                    a = sg.dx * (sg.hi - sg.lo) / 2.0 * ratio * ratio;
                    w *= ratio;
                    wp *= ratio;
                }
                area += a;
                width += w;
                perim += wp;
                a_t += a;
                p_t += wp;
            }
            // Ground at or above the water line ends the sub-section.
            if sg.z1 >= y {
                flush(&mut a_t, &mut p_t, sg.n);
                continue;
            }
            if sg.flush_bank {
                flush(&mut a_t, &mut p_t, sg.n);
            }
        }
        let last_n = self.segs.last().map_or(self.n_channel, |s| s.n);
        flush(&mut a_t, &mut p_t, last_n);
        (area, width, perim, k_sum)
    }

    /// The sweep without its conveyance sum: area, top width, and wetted
    /// perimeter alone. What `area`, `top_width` and `perimeter_open`
    /// actually need — the full sweep spends an $x^{2/3}$ per sub-section
    /// on a conveyance those callers throw away.
    fn sweep_geom(&self, y: f64) -> (f64, f64, f64) {
        let (mut area, mut width, mut perim) = (0.0, 0.0, 0.0);
        for sg in &self.segs {
            if sg.min_lo_ahead >= y {
                break;
            }
            if sg.lo < y {
                let mut w = sg.dx;
                let mut wp = sg.wp_full;
                let a;
                if y > sg.hi {
                    a = sg.dx * ((y - sg.hi) + (y - sg.lo)) / 2.0;
                } else {
                    let ratio = (y - sg.lo) / (sg.hi - sg.lo);
                    a = sg.dx * (sg.hi - sg.lo) / 2.0 * ratio * ratio;
                    w *= ratio;
                    wp *= ratio;
                }
                area += a;
                width += w;
                perim += wp;
            }
        }
        (area, width, perim)
    }

    /// The same walk carrying depth-derivatives (§5.7). Values are the
    /// sweep's own; the derivatives come from the identical slices — a
    /// partly-submerged segment's width and slant grow at `dx/dz` and
    /// `slant/dz`, a submerged one's not at all — so a Newton step costs
    /// one walk, exactly like the evaluation it accelerates.
    fn sweep_d(&self, y: f64) -> SweepD {
        let (mut area, mut width, mut dw, mut k_sum, mut dk_sum) = (0.0, 0.0, 0.0, 0.0, 0.0);
        // The running sub-section: area, perimeter, width, d(perimeter).
        let (mut a_t, mut p_t, mut w_t, mut dp_t) = (0.0, 0.0, 0.0, 0.0);
        let mut flush = |a_t: &mut f64, p_t: &mut f64, w_t: &mut f64, dp_t: &mut f64, n: f64| {
            if *a_t > 0.0 && *p_t > 0.0 {
                let ki = (1.0 / n) * *a_t * (*a_t / *p_t).powf(2.0 / 3.0);
                k_sum += ki;
                // dK/dy = K·((5/3)·A'/A − (2/3)·P'/P), with A' the
                // sub-section's width.
                dk_sum += ki * (5.0 / 3.0 * *w_t / *a_t - 2.0 / 3.0 * *dp_t / *p_t);
            }
            *a_t = 0.0;
            *p_t = 0.0;
            *w_t = 0.0;
            *dp_t = 0.0;
        };
        for sg in &self.segs {
            if a_t == 0.0 && sg.min_lo_ahead >= y {
                break;
            }
            if sg.lo < y {
                let mut w = sg.dx;
                let mut wp = sg.wp_full;
                let a;
                if y > sg.hi {
                    a = sg.dx * ((y - sg.hi) + (y - sg.lo)) / 2.0;
                } else {
                    let dz = sg.hi - sg.lo;
                    let ratio = (y - sg.lo) / dz;
                    a = sg.dx * dz / 2.0 * ratio * ratio;
                    w *= ratio;
                    wp *= ratio;
                    dw += sg.dx / dz;
                    dp_t += sg.wp_full / dz;
                }
                area += a;
                width += w;
                a_t += a;
                p_t += wp;
                w_t += w;
            }
            if sg.z1 >= y {
                flush(&mut a_t, &mut p_t, &mut w_t, &mut dp_t, sg.n);
                continue;
            }
            if sg.flush_bank {
                flush(&mut a_t, &mut p_t, &mut w_t, &mut dp_t, sg.n);
            }
        }
        let last_n = self.segs.last().map_or(self.n_channel, |s| s.n);
        flush(&mut a_t, &mut p_t, &mut w_t, &mut dp_t, last_n);
        SweepD {
            area,
            width,
            dw,
            k: k_sum,
            dk: dk_sum,
        }
    }
}

/// Adaptive Simpson quadrature on a smooth integrand: exact evaluation of
/// the arc-length integrals no closed form covers, to a stated tolerance —
/// not a fixed-resolution polyline.
fn integrate(f: &dyn Fn(f64) -> f64, a: f64, b: f64) -> f64 {
    fn simpson(a: f64, fa: f64, b: f64, fb: f64, fm: f64) -> f64 {
        (b - a) / 6.0 * (fa + 4.0 * fm + fb)
    }
    #[allow(clippy::too_many_arguments)]
    fn recurse(
        f: &dyn Fn(f64) -> f64,
        a: f64,
        fa: f64,
        b: f64,
        fb: f64,
        fm: f64,
        whole: f64,
        tol: f64,
        depth: u32,
    ) -> f64 {
        let m = 0.5 * (a + b);
        let (lm, rm) = (0.5 * (a + m), 0.5 * (m + b));
        let (flm, frm) = (f(lm), f(rm));
        let left = simpson(a, fa, m, fm, flm);
        let right = simpson(m, fm, b, fb, frm);
        if depth == 0 || (left + right - whole).abs() <= 15.0 * tol {
            left + right + (left + right - whole) / 15.0
        } else {
            recurse(f, a, fa, m, fm, flm, left, 0.5 * tol, depth - 1)
                + recurse(f, m, fm, b, fb, frm, right, 0.5 * tol, depth - 1)
        }
    }
    if b <= a {
        return 0.0;
    }
    let (fa, fb, fm) = (f(a), f(b), f(0.5 * (a + b)));
    let whole = simpson(a, fa, b, fb, fm);
    recurse(f, a, fa, b, fb, fm, whole, 1e-12 * (1.0 + whole.abs()), 48)
}

/// Golden-section maximisation of a unimodal function on `[a, b]`,
/// deterministic to floating-point resolution.
fn maximise(f: &dyn Fn(f64) -> f64, mut a: f64, mut b: f64) -> (f64, f64) {
    const INV_PHI: f64 = 0.618_033_988_749_894_9;
    let mut c = b - INV_PHI * (b - a);
    let mut d = a + INV_PHI * (b - a);
    let (mut fc, mut fd) = (f(c), f(d));
    for _ in 0..200 {
        if fc < fd {
            a = c;
            c = d;
            fc = fd;
            d = a + INV_PHI * (b - a);
            fd = f(d);
        } else {
            b = d;
            d = c;
            fd = fc;
            c = b - INV_PHI * (b - a);
            fc = f(c);
        }
        if (b - a).abs() <= f64::EPSILON * (a.abs() + b.abs()) {
            break;
        }
    }
    let y = 0.5 * (a + b);
    (y, f(y))
}

/// Bisection on a monotone-increasing function: find `y` in `[a, b]` with
/// `f(y) = target`. A bracketed solve on a monotone relation cannot fail
/// (§5.7); 100 halvings resolve the bracket to floating-point exactness.
/// The §5.7 stated tolerance (m): a bracketed solve stops at a bracket
/// this narrow and answers with its midpoint. Three orders below the
/// tightest §6.4 head tolerance, so nothing downstream can tell the
/// answer from the exact root — while sparing the ~30 further halvings
/// f64 exhaustion would run, each a full section evaluation.
const INVERT_TOL: f64 = 1e-6;

/// Bracketed root-finding on a monotone non-decreasing `f` over `[a, b]`,
/// to the §5.7 stated tolerance, answering with the bracket midpoint.
fn invert(f: &dyn Fn(f64) -> f64, mut a: f64, mut b: f64, target: f64) -> f64 {
    for _ in 0..100 {
        let m = 0.5 * (a + b);
        // Stop at the stated tolerance — or at adjacent doubles, where
        // the midpoint is one of the ends and further halving reproduces
        // this state (a bracket can open that narrow when it starts
        // narrow).
        if b - a <= INVERT_TOL || m <= a || m >= b {
            break;
        }
        if f(m) < target {
            a = m;
        } else {
            b = m;
        }
    }
    0.5 * (a + b)
}

/// The tabulated families (§5.3) and the arch (§5.4), keyed to their
/// provision groups: which properties each tabulates and which it derives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)] // the names are the shapes
pub enum TabFamily {
    Egg,
    Horseshoe,
    Gothic,
    Catenary,
    SemiElliptical,
    BasketHandle,
    SemiCircular,
    Arch,
}

impl TabFamily {
    /// Normalised area against depth, where tabulated.
    fn a_table(self) -> Option<&'static [f64]> {
        match self {
            TabFamily::Egg => Some(&tables::A_EGG),
            TabFamily::Horseshoe => Some(&tables::A_HORSESHOE),
            TabFamily::BasketHandle => Some(&tables::A_BASKET_HANDLE),
            TabFamily::Arch => Some(&tables::A_ARCH),
            _ => None,
        }
    }

    /// Normalised hydraulic radius against depth, where tabulated.
    fn r_table(self) -> Option<&'static [f64]> {
        match self {
            TabFamily::Egg => Some(&tables::R_EGG),
            TabFamily::Horseshoe => Some(&tables::R_HORSESHOE),
            TabFamily::BasketHandle => Some(&tables::R_BASKET_HANDLE),
            TabFamily::Arch => Some(&tables::R_ARCH),
            _ => None,
        }
    }

    /// Normalised depth against area, where tabulated.
    fn y_table(self) -> Option<&'static [f64]> {
        match self {
            TabFamily::Egg => Some(&tables::Y_EGG),
            TabFamily::Horseshoe => Some(&tables::Y_HORSESHOE),
            TabFamily::Gothic => Some(&tables::Y_GOTHIC),
            TabFamily::Catenary => Some(&tables::Y_CATENARY),
            TabFamily::SemiElliptical => Some(&tables::Y_SEMI_ELLIPTICAL),
            TabFamily::BasketHandle => Some(&tables::Y_BASKET_HANDLE),
            TabFamily::SemiCircular => Some(&tables::Y_SEMI_CIRCULAR),
            TabFamily::Arch => None,
        }
    }

    /// Normalised section factor against area, where tabulated.
    fn s_table(self) -> Option<&'static [f64]> {
        match self {
            TabFamily::Egg => Some(&tables::S_EGG),
            TabFamily::Horseshoe => Some(&tables::S_HORSESHOE),
            TabFamily::Gothic => Some(&tables::S_GOTHIC),
            TabFamily::Catenary => Some(&tables::S_CATENARY),
            TabFamily::SemiElliptical => Some(&tables::S_SEMI_ELLIPTICAL),
            TabFamily::BasketHandle => Some(&tables::S_BASKET_HANDLE),
            TabFamily::SemiCircular => Some(&tables::S_SEMI_CIRCULAR),
            TabFamily::Arch => None,
        }
    }

    /// Normalised width against depth.
    fn w_table(self) -> &'static [f64] {
        match self {
            TabFamily::Egg => &tables::W_EGG,
            TabFamily::Horseshoe => &tables::W_HORSESHOE,
            TabFamily::Gothic => &tables::W_GOTHIC,
            TabFamily::Catenary => &tables::W_CATENARY,
            TabFamily::SemiElliptical => &tables::W_SEMI_ELLIPTICAL,
            TabFamily::BasketHandle => &tables::W_BASKET_HANDLE,
            TabFamily::SemiCircular => &tables::W_SEMI_CIRCULAR,
            TabFamily::Arch => &tables::W_ARCH,
        }
    }
}

/// The predecessor's table interpolation (§5.3): linear over equally
/// spaced entries, with the quadratic refinement over the two lowest
/// segments and a floor at zero.
fn lookup(x: f64, table: &[f64]) -> f64 {
    let n = table.len();
    let delta = 1.0 / (n as f64 - 1.0);
    let i = (x / delta) as usize;
    if i >= n - 1 {
        return table[n - 1];
    }
    let x0 = i as f64 * delta;
    let x1 = (i as f64 + 1.0) * delta;
    let mut y = table[i] + (x - x0) * (table[i + 1] - table[i]) / delta;
    if i < 2 {
        let y2 = y
            + (x - x0) * (x - x1) / (delta * delta)
                * (table[i] / 2.0 - table[i + 1] + table[i + 2] / 2.0);
        if y2 > 0.0 {
            y = y2;
        }
    }
    y.max(0.0)
}

/// The predecessor's inverse table lookup (§5.3): assumes entries either
/// strictly increase or peak third from the end (the section-factor
/// tables); an ambiguous value above the tail is resolved on the tail.
fn inv_lookup(y: f64, table: &[f64]) -> f64 {
    let n_items = table.len();
    let dx = 1.0 / (n_items as f64 - 1.0);
    let mut n = n_items;
    if table[n - 3] > table[n - 1] {
        n -= 2;
    }
    let i;
    if n < n_items && y > table[n_items - 1] {
        if y >= table[n_items - 3] {
            return (n as f64 - 1.0) * dx;
        }
        if y <= table[n_items - 2] {
            i = n_items - 2;
        } else {
            i = n_items - 3;
        }
    } else {
        i = locate(y, &table[..n]);
        if i >= n - 1 {
            return (n as f64 - 1.0) * dx;
        }
    }
    let x0 = i as f64 * dx;
    let dy = table[i + 1] - table[i];
    let x = if dy == 0.0 {
        x0
    } else {
        x0 + (y - table[i]) * dx / dy
    };
    x.clamp(0.0, 1.0)
}

/// Bisection locate over a monotone table slice: the highest index whose
/// entry does not exceed `y`.
fn locate(y: f64, table: &[f64]) -> usize {
    let last = table.len() - 1;
    if y <= table[0] {
        return 0;
    }
    if y >= table[last] {
        return last;
    }
    let (mut j1, mut j2) = (0, last);
    while j2 - j1 > 1 {
        let j = (j1 + j2) / 2;
        if y >= table[j] {
            j1 = j;
        } else {
            j2 = j;
        }
    }
    j1
}

/// Circle geometry through the filled angle (§5.2), diameter `d`.
fn circle_theta(d: f64, y: f64) -> f64 {
    let c = (1.0 - 2.0 * y / d).clamp(-1.0, 1.0);
    2.0 * c.acos()
}

fn circle_area(d: f64, y: f64) -> f64 {
    // $\sin\theta = 2\sin(\theta/2)\cos(\theta/2)$, and both halves are
    // already to hand without trigonometry: the chord is $d\sin(\theta/2)$
    // and the cosine is the very argument the angle was taken from. So the
    // segment costs one inverse cosine, which is irreducible — a circular
    // segment's area is an inverse-trigonometric quantity — and no sine.
    let c = (1.0 - 2.0 * y / d).clamp(-1.0, 1.0);
    let t = 2.0 * c.acos();
    // Both forms of $\theta - \sin\theta$ cancel as $\theta \to 0$, where
    // the difference falls away to $\theta^3/6$. The identity cancels
    // harder, because the chord reaches it by a rounding of its own
    // rather than from the very angle being subtracted. Against a
    // 50-digit reference over a metre pipe: above a thousandth of full
    // depth the identity holds to 5e-12 relative against the sine's
    // 7e-14, both of them some twelve orders below the millimetre the
    // solver converges heads to. Below that the identity degrades
    // steeply, so there the sine is kept and the worst case over the
    // whole range is exactly what it was — a band the router has already
    // called dry, `DRY` sitting a quarter of the way into it.
    const CANCELS: f64 = 0.13;
    let sin_t = if t < CANCELS {
        t.sin()
    } else {
        2.0 * (circle_width(d, y) / d) * c
    };
    d * d / 8.0 * (t - sin_t)
}

/// The chord of a circle of diameter `d` at depth `y`.
///
/// $W = 2\sqrt{y(d-y)}$, the same quantity as $d\sin(\theta/2)$ and
/// reached without the angle. Two reasons, and the second is the better
/// one. It costs a square root where the trigonometric form costs an
/// inverse cosine and a sine, and a partly full circular pipe is most of
/// a drainage model. And it is the more accurate of the two: the inverse
/// cosine is ill-conditioned as its argument approaches ±1, which is a
/// nearly empty or nearly full pipe, where this form is exact to within
/// a rounding of its own product.
fn circle_width(d: f64, y: f64) -> f64 {
    2.0 * (y * (d - y)).max(0.0).sqrt()
}

fn circle_perimeter(d: f64, y: f64) -> f64 {
    d * circle_theta(d, y) / 2.0
}

/// Build a section from a cross-section record: shape, the four geometry
/// values in file units, the file length factor to metres, and — for
/// custom shapes — the referenced shape curve's normalised points.
///
/// The accept-set is the predecessor's `xsect_setParams`, including its one
/// mutation: a rectangular-round or basket-handle bottom radius below half
/// the width is enlarged to the geometric minimum (§14.7), reported in the
/// build result.
pub fn build_section(
    shape: XsectShape,
    geom_user: [f64; 4],
    len: f64,
    shape_curve: Option<&[(f64, f64)]>,
) -> Result<SectionBuild, BuildError> {
    // Transect-backed shapes carry no geometry values of their own; route
    // them before the depth gate.
    if matches!(shape, XsectShape::Irregular | XsectShape::Street) {
        return Err(BuildError::Unsupported(
            "transect-backed sections build through their own entry point",
        ));
    }
    let p = geom_user;
    if shape != XsectShape::Dummy && p[0] <= 0.0 {
        return Err(BuildError::BadGeometry("full depth must be positive"));
    }
    let mut radius_raised = None;
    let kind = match shape {
        XsectShape::Dummy => Kind::Dummy,
        XsectShape::Circular => Kind::Circle { d: p[0] * len },
        XsectShape::ForceMain => {
            // Geometrically a circle; the friction coefficient in the
            // second slot belongs to §6 and travels on the link.
            Kind::Circle { d: p[0] * len }
        }
        XsectShape::FilledCircular => {
            if p[1] >= p[0] {
                return Err(BuildError::BadGeometry("fill depth must be below diameter"));
            }
            let d = p[0] * len;
            let y_bot = p[1] * len;
            Kind::FilledCircle {
                d,
                y_bot,
                a_bot: circle_area(d, y_bot),
                p_bot: circle_perimeter(d, y_bot),
                w_bot: circle_width(d, y_bot),
            }
        }
        XsectShape::RectClosed => {
            if p[1] <= 0.0 {
                return Err(BuildError::BadGeometry("width must be positive"));
            }
            Kind::RectClosed { w: p[1] * len }
        }
        XsectShape::RectOpen => {
            if p[1] <= 0.0 {
                return Err(BuildError::BadGeometry("width must be positive"));
            }
            if !(0.0..=2.0).contains(&p[2]) {
                return Err(BuildError::BadGeometry("sides ignored must be 0 to 2"));
            }
            Kind::RectOpen {
                w: p[1] * len,
                sides_ignored: p[2],
            }
        }
        XsectShape::Triangular => {
            if p[1] <= 0.0 {
                return Err(BuildError::BadGeometry("top width must be positive"));
            }
            Kind::Triangle {
                s: (p[1] * len) / (p[0] * len) / 2.0,
            }
        }
        XsectShape::Trapezoidal => {
            if p[1] < 0.0 || p[2] < 0.0 || p[3] < 0.0 {
                return Err(BuildError::BadGeometry("negative trapezoid parameter"));
            }
            if p[1] == 0.0 && p[2] + p[3] == 0.0 {
                return Err(BuildError::BadGeometry(
                    "bottom width and side slopes all zero",
                ));
            }
            Kind::Trapezoid {
                b: p[1] * len,
                sl: p[2],
                sr: p[3],
            }
        }
        XsectShape::Parabolic => {
            if p[1] <= 0.0 {
                return Err(BuildError::BadGeometry("top width must be positive"));
            }
            // Half-width x = k√y through (w/2, y_full).
            Kind::Parabola {
                k: (p[1] * len) / 2.0 / (p[0] * len).sqrt(),
            }
        }
        XsectShape::Power => {
            if p[1] <= 0.0 || p[2] <= 0.0 {
                return Err(BuildError::BadGeometry(
                    "top width and exponent must be positive",
                ));
            }
            // Half-width x = h·y^m through (w/2, y_full), m = 1/exponent.
            let m = 1.0 / p[2];
            Kind::Power {
                h: (p[1] * len) / 2.0 / (p[0] * len).powf(m),
                m,
            }
        }
        XsectShape::RectTriangular => {
            if p[1] <= 0.0 || p[2] <= 0.0 {
                return Err(BuildError::BadGeometry(
                    "width and triangle height must be positive",
                ));
            }
            let w = p[1] * len;
            let y_bot = p[2] * len;
            Kind::RectTriangle {
                w,
                y_bot,
                s: w / y_bot / 2.0,
            }
        }
        XsectShape::RectRound => {
            if p[1] <= 0.0 {
                return Err(BuildError::BadGeometry("width must be positive"));
            }
            let w = p[1] * len;
            let mut r = p[2] * len;
            if r < w / 2.0 {
                r = w / 2.0;
                radius_raised = Some(r);
            }
            let theta = 2.0 * (w / 2.0 / r).asin();
            let y_bot = r * (1.0 - (theta / 2.0).cos());
            if y_bot > p[0] * len {
                return Err(BuildError::BadGeometry("bottom arc taller than section"));
            }
            Kind::RectRound {
                w,
                r,
                y_bot,
                a_bot: r * r / 2.0 * (theta - theta.sin()),
                theta,
            }
        }
        XsectShape::ModBasketHandle => {
            if p[1] <= 0.0 {
                return Err(BuildError::BadGeometry("width must be positive"));
            }
            let w = p[1] * len;
            let mut r = p[2] * len;
            if r < w / 2.0 {
                r = w / 2.0;
                radius_raised = Some(r);
            }
            let theta = 2.0 * (w / 2.0 / r).asin();
            let y_bot = r * (1.0 - (theta / 2.0).cos());
            if y_bot > p[0] * len {
                return Err(BuildError::BadGeometry("roof arc taller than section"));
            }
            Kind::ModBasket {
                w,
                r,
                y_bot,
                a_bot: r * r / 2.0 * (theta - theta.sin()),
                theta,
            }
        }
        XsectShape::HorizEllipse | XsectShape::VertEllipse => {
            // A zero width makes the first value a size code; an explicit
            // third value is a size code directly (§5.4).
            let code = if p[1] == 0.0 { p[0] } else { p[2] };
            if code > 0.0 {
                let Some(i) = catalogue_index(code, tables::ELLIPSE_MINOR_AXIS_IN.len()) else {
                    return Err(BuildError::BadGeometry("unknown ellipse size code"));
                };
                let minor = tables::ELLIPSE_MINOR_AXIS_IN[i] / 12.0 * FT;
                let major = tables::ELLIPSE_MAJOR_AXIS_IN[i] / 12.0 * FT;
                let (h, w) = if shape == XsectShape::HorizEllipse {
                    (minor, major)
                } else {
                    (major, minor)
                };
                return Ok(SectionBuild {
                    section: build_coded_ellipse(
                        h,
                        w,
                        tables::ELLIPSE_A_FULL_FT2[i] * FT * FT,
                        tables::ELLIPSE_R_FULL_FT[i] * FT,
                    ),
                    radius_raised: None,
                });
            }
            if p[1] < 0.0 {
                return Err(BuildError::BadGeometry("negative ellipse axis"));
            }
            // Arbitrary axes: the analytic ellipse at the axes the user
            // wrote — where the predecessor substitutes fixed-proportion
            // constants (§5.4 CORRESPONDENCE).
            Kind::Ellipse {
                a: p[1] * len / 2.0,
                b: p[0] * len / 2.0,
                area_scale: 1.0,
                r_scale: 1.0,
            }
        }
        XsectShape::Arch => {
            let code = if p[1] == 0.0 { p[0] } else { p[2] };
            if code > 0.0 {
                let Some(i) = catalogue_index(code, tables::ARCH_Y_FULL_IN.len()) else {
                    return Err(BuildError::BadGeometry("unknown arch size code"));
                };
                let y = tables::ARCH_Y_FULL_IN[i] / 12.0 * FT;
                let w = tables::ARCH_W_MAX_IN[i] / 12.0 * FT;
                return Ok(SectionBuild {
                    section: Section::assemble(
                        Kind::Tabulated {
                            family: TabFamily::Arch,
                            a_full: tables::ARCH_A_FULL_FT2[i] * FT * FT,
                            r_full: tables::ARCH_R_FULL_FT[i] * FT,
                            w_max: w,
                            yw_max: 0.28 * y,
                        },
                        y,
                    ),
                    radius_raised: None,
                });
            }
            if p[1] < 0.0 {
                return Err(BuildError::BadGeometry("negative arch span"));
            }
            // User dimensions: the predecessor's proportionality constants
            // over the same tables (§5.4).
            let y = p[0] * len;
            let w = p[1] * len;
            Kind::Tabulated {
                family: TabFamily::Arch,
                a_full: 0.7879 * y * w,
                r_full: 0.2991 * y,
                w_max: w,
                yw_max: 0.28 * y,
            }
        }
        XsectShape::Custom => {
            let Some(points) = shape_curve else {
                return Err(BuildError::BadGeometry("custom shape without its curve"));
            };
            build_custom(p[0] * len, points)?
        }
        XsectShape::Egg
        | XsectShape::Horseshoe
        | XsectShape::Gothic
        | XsectShape::Catenary
        | XsectShape::SemiElliptical
        | XsectShape::BasketHandle
        | XsectShape::SemiCircular => {
            // One rise value; the family constants supply the full-flow
            // anchors (§5.3).
            let (family, c_a, c_r, c_w, c_yw) = match shape {
                XsectShape::Egg => (TabFamily::Egg, 0.5105, 0.1931, 2.0 / 3.0, 0.64),
                XsectShape::Horseshoe => (TabFamily::Horseshoe, 0.8293, 0.2538, 1.0, 0.5),
                XsectShape::Gothic => (TabFamily::Gothic, 0.6554, 0.2269, 0.84, 0.45),
                XsectShape::Catenary => (TabFamily::Catenary, 0.70277, 0.23172, 0.9, 0.25),
                XsectShape::SemiElliptical => (TabFamily::SemiElliptical, 0.785, 0.242, 1.0, 0.15),
                XsectShape::BasketHandle => (TabFamily::BasketHandle, 0.7862, 0.2464, 0.944, 0.2),
                _ => (TabFamily::SemiCircular, 1.2697, 0.2946, 1.64, 0.15),
            };
            let y = p[0] * len;
            Kind::Tabulated {
                family,
                a_full: c_a * y * y,
                r_full: c_r * y,
                w_max: c_w * y,
                yw_max: c_yw * y,
            }
        }
        // Routed before the depth gate; kept as an error, not a panic.
        XsectShape::Irregular | XsectShape::Street => {
            return Err(BuildError::Unsupported(
                "transect-backed sections build through their own entry point",
            ));
        }
    };
    Ok(SectionBuild {
        section: Section::assemble(kind, p[0] * len),
        radius_raised,
    })
}

/// A one-based catalogue size code, bounds-checked.
fn catalogue_index(code: f64, n: usize) -> Option<usize> {
    let i = code.floor() as i64 - 1;
    (0..n as i64).contains(&i).then_some(i as usize)
}

/// A catalogue ellipse (§5.4): the analytic shape at the catalogue axes,
/// scaled so the full-flow area and hydraulic radius land on the
/// published values.
fn build_coded_ellipse(height: f64, width: f64, a_cat: f64, r_cat: f64) -> Section {
    let (a, b) = (width / 2.0, height / 2.0);
    let area_scale = a_cat / (std::f64::consts::PI * a * b);
    // Full perimeter of the analytic ellipse, for the radius anchor.
    let f = |t: f64| ((a * t.cos()).powi(2) + (b * t.sin()).powi(2)).sqrt();
    let p_full = 2.0 * integrate(&f, 0.0, std::f64::consts::PI);
    let r_scale = r_cat * p_full / a_cat;
    Section::assemble(
        Kind::Ellipse {
            a,
            b,
            area_scale,
            r_scale,
        },
        height,
    )
}

/// Build a section from a surveyed transect (§5.6). The accept-set is the
/// predecessor's: at least two stations, non-decreasing station distances,
/// a positive channel roughness, ordered bank stations, and a non-flat
/// profile. Omitted overbank coefficients default to the channel's; the
/// meander modifier inflates the channel roughness by its square root.
pub fn build_transect_section(t: &crate::model::Transect) -> Result<SectionBuild, BuildError> {
    if t.stations.len() < 2 {
        return Err(BuildError::BadGeometry(
            "transect needs at least 2 stations",
        ));
    }
    if t.n_channel <= 0.0 {
        return Err(BuildError::BadGeometry(
            "channel roughness must be positive",
        ));
    }
    if t.x_left > t.x_right {
        return Err(BuildError::BadGeometry("bank stations out of order"));
    }
    let mut xs: Vec<f64> = Vec::with_capacity(t.stations.len() + 2);
    let mut zs: Vec<f64> = Vec::with_capacity(t.stations.len() + 2);
    for w in t.stations.windows(2) {
        if w[1].1 < w[0].1 {
            return Err(BuildError::BadGeometry("station distances decrease"));
        }
    }
    let z_min = t.stations.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    let z_max = t
        .stations
        .iter()
        .map(|s| s.0)
        .fold(f64::NEG_INFINITY, f64::max);
    if z_min >= z_max {
        return Err(BuildError::BadGeometry("flat transect"));
    }
    let y_full = z_max - z_min;
    // Vertical end walls to full height on both ends (§5.6).
    xs.push(t.stations[0].1);
    zs.push(y_full);
    for &(z, x) in &t.stations {
        xs.push(x);
        zs.push(z - z_min);
    }
    xs.push(t.stations[t.stations.len() - 1].1);
    zs.push(y_full);
    let n_channel = t.n_channel * t.meander_factor.sqrt();
    let n_left = if t.n_left > 0.0 { t.n_left } else { n_channel };
    let n_right = if t.n_right > 0.0 {
        t.n_right
    } else {
        n_channel
    };
    Ok(SectionBuild {
        section: Section::assemble(
            Kind::Transect(TransectGeom::new(
                &xs, &zs, t.x_left, t.x_right, n_left, n_right, n_channel,
            )),
            y_full,
        ),
        radius_raised: None,
    })
}

/// Compile a street cross-section to a transect (§7.8 → §5.6): backing,
/// curb, depressed gutter, and crown on one or both sides, with the
/// backing roughness as the overbanks'.
pub fn build_street_section(st: &crate::model::Street) -> Result<SectionBuild, BuildError> {
    if st.crown_width <= 0.0 || st.cross_slope <= 0.0 || st.roughness <= 0.0 {
        return Err(BuildError::BadGeometry("street geometry incomplete"));
    }
    let w1 = st.backing_width;
    let w2 = st.gutter_width;
    let w3 = st.crown_width;
    let w4 = w3 - w2;
    let y3 = st.gutter_depression + st.cross_slope * w2;
    let y1 = st.curb_height + st.gutter_depression;
    let y4 = y3 + st.cross_slope * w4;
    let y_max = (st.backing_slope * w1 + y1).max(y4);
    // Backing top, curb top, curb bottom, gutter bottom, gutter top,
    // crown — mirrored for a two-sided street.
    let mut xs = vec![0.0, w1, w1, w1 + w2, w1 + w3];
    let mut zs = vec![y_max, y1, 0.0, y3, y4];
    if st.sides == 1 {
        xs.push(w1 + w3);
        zs.push(y_max);
    } else {
        xs.extend_from_slice(&[w1 + w3 + w4, w1 + w3 + w4 + w2, w1 + w3 + w4 + w2, {
            w1 + w3 + w4 + w2 + w1
        }]);
        zs.extend_from_slice(&[y3, 0.0, y1, y_max]);
    }
    let (n_left, n_right, x_left, x_right);
    let n_channel = st.roughness;
    if w1 == 0.0 {
        n_left = n_channel;
        n_right = n_channel;
        x_left = xs[0];
        x_right = *xs.last().unwrap_or(&0.0);
    } else {
        n_left = st.backing_roughness;
        n_right = n_left;
        x_left = w1;
        x_right = if st.sides == 2 {
            xs[xs.len() - 2]
        } else {
            *xs.last().unwrap_or(&0.0)
        };
    }
    Ok(SectionBuild {
        section: Section::assemble(
            Kind::Transect(TransectGeom::new(
                &xs, &zs, x_left, x_right, n_left, n_right, n_channel,
            )),
            y_max,
        ),
        radius_raised: None,
    })
}

/// Custom-shape semantics (§5.5): anchored at the origin, truncated above
/// unit height, extended at the last width if short, closed at the top.
fn build_custom(y_full: f64, points: &[(f64, f64)]) -> Result<Kind, BuildError> {
    let mut ys = vec![0.0];
    let mut ws = vec![0.0];
    for &(yf, wf) in points {
        if yf < 0.0 || wf < 0.0 {
            return Err(BuildError::BadGeometry("negative shape-curve value"));
        }
        let last = *ys.last().unwrap_or(&0.0);
        if yf <= last && !(ys.len() == 1 && yf == 0.0) {
            return Err(BuildError::BadGeometry(
                "shape-curve depths must be increasing",
            ));
        }
        if ys.len() == 1 && yf == 0.0 {
            // The curve supplies its own bottom width.
            ws[0] = wf;
            continue;
        }
        if yf >= 1.0 {
            // Truncate at unit height.
            let (y0, w0) = (last, *ws.last().unwrap_or(&0.0));
            let w1 = w0 + (wf - w0) * (1.0 - y0) / (yf - y0);
            ys.push(1.0);
            ws.push(w1);
            break;
        }
        ys.push(yf);
        ws.push(wf);
    }
    // Extended at the last width if the curve stops short.
    if *ys.last().unwrap_or(&0.0) < 1.0 {
        let w = *ws.last().unwrap_or(&0.0);
        ys.push(1.0);
        ws.push(w);
    }
    // Scale the unit-height section by the full depth.
    let ys: Vec<f64> = ys.iter().map(|y| y * y_full).collect();
    let ws: Vec<f64> = ws.iter().map(|w| w * y_full).collect();
    if ws.iter().all(|w| *w <= 0.0) {
        return Err(BuildError::BadGeometry("shape curve encloses no area"));
    }
    Ok(Kind::Custom { ys, ws })
}

impl Section {
    fn assemble(kind: Kind, y_full: f64) -> Section {
        if matches!(kind, Kind::Dummy) {
            return Section {
                kind,
                y_full: 0.0,
                a_full: 0.0,
                r_full: 0.0,
                w_full: 0.0,
                w_max: 0.0,
                y_at_w_max: 0.0,
                psi_max: 0.0,
                y_at_psi_max: 0.0,
                crit_full: 0.0,
            };
        }
        // The filled circle measures depth above the fill.
        let y_full = match &kind {
            Kind::FilledCircle { d, y_bot, .. } => d - y_bot,
            _ => y_full,
        };
        let mut s = Section {
            kind,
            y_full,
            a_full: 0.0,
            r_full: 0.0,
            w_full: 0.0,
            w_max: 0.0,
            y_at_w_max: 0.0,
            psi_max: 0.0,
            y_at_psi_max: 0.0,
            crit_full: 0.0,
        };
        s.a_full = s.area(y_full);
        s.w_full = s.top_width(y_full);
        s.crit_full = match &s.kind {
            Kind::Circle { .. } => s.a_full.powi(3) / s.top_width(y_full).max(1e-30),
            Kind::Transect(t) => {
                let (a, w, _, _) = t.sweep(y_full);
                a.powi(3) / w.max(1e-30)
            }
            _ => {
                let (a, w, _) = s.geom(y_full);
                a.powi(3) / w.max(1e-30)
            }
        };
        // Full-depth perimeter includes any flat lid; a catalogue-anchored
        // ellipse lands on its published radius through its scale (§5.4).
        s.r_full = s.a_full / (s.perimeter_open(y_full) + s.lid_width());
        if let Kind::Ellipse { r_scale, .. } = &s.kind {
            s.r_full *= r_scale;
        }
        // Widest point.
        let (yw, ww) = match &s.kind {
            Kind::Circle { .. } | Kind::FilledCircle { .. } | Kind::Ellipse { .. } => {
                let y = match &s.kind {
                    Kind::FilledCircle { d, y_bot, .. } => d / 2.0 - y_bot,
                    _ => y_full / 2.0,
                };
                (y.max(0.0), s.top_width(y.max(0.0)))
            }
            Kind::ModBasket { y_bot, .. } => (y_full - y_bot, s.top_width(y_full - y_bot)),
            Kind::Custom { ys, ws } => {
                let (mut yb, mut wb) = (0.0, 0.0);
                for (y, w) in ys.iter().zip(ws) {
                    if *w > wb {
                        (yb, wb) = (*y, *w);
                    }
                }
                (yb, wb)
            }
            Kind::Tabulated { yw_max, w_max, .. } => (*yw_max, *w_max),
            _ => (y_full, s.top_width(y_full)),
        };
        s.w_max = ww;
        s.y_at_w_max = yw;
        // Ψ peaks below full depth for closed sections (§5.1); open
        // sections are monotone to the brim. A section-factor table's own
        // peak is authoritative for the family it defines (§5.3).
        if let Kind::Tabulated {
            family,
            a_full,
            r_full,
            ..
        } = &s.kind
        {
            if let Some(t) = family.s_table() {
                let (mut i_max, mut v_max) = (0, 0.0);
                for (i, v) in t.iter().enumerate() {
                    if *v > v_max {
                        (i_max, v_max) = (i, *v);
                    }
                }
                let s_full = a_full * r_full.powf(2.0 / 3.0);
                let alpha = i_max as f64 / (t.len() as f64 - 1.0);
                s.psi_max = s_full * v_max;
                s.y_at_psi_max = s.depth_of_area(alpha * a_full);
                return s;
            }
        }
        if s.is_closed() {
            let f = |y: f64| s.psi(y);
            let (y, p) = maximise(&f, 1e-9 * y_full, y_full * (1.0 - 1e-9));
            let p_full = s.a_full * s.r_full.powf(2.0 / 3.0);
            if p > p_full {
                s.psi_max = p;
                s.y_at_psi_max = y;
            } else {
                s.psi_max = p_full;
                s.y_at_psi_max = y_full;
            }
        } else {
            s.psi_max = s.a_full * s.r_full.powf(2.0 / 3.0);
            s.y_at_psi_max = y_full;
        }
        s
    }

    /// Whether the section closes over the flow (a crown or lid exists).
    pub fn is_closed(&self) -> bool {
        !matches!(
            self.kind,
            Kind::RectOpen { .. }
                | Kind::Triangle { .. }
                | Kind::Trapezoid { .. }
                | Kind::Parabola { .. }
                | Kind::Power { .. }
                | Kind::Transect(_)
        )
    }

    /// Width of the flat lid counted as wetted perimeter at exactly full
    /// depth (§5.2's compound closures and the custom top).
    fn lid_width(&self) -> f64 {
        match &self.kind {
            Kind::RectClosed { w } | Kind::RectTriangle { w, .. } | Kind::RectRound { w, .. } => *w,
            Kind::Custom { ws, .. } => *ws.last().unwrap_or(&0.0),
            _ => 0.0,
        }
    }

    /// Full depth (m).
    pub fn y_full(&self) -> f64 {
        self.y_full
    }

    /// Full-flow area (m²).
    pub fn a_full(&self) -> f64 {
        self.a_full
    }

    /// Full-flow hydraulic radius (m), lid included where one exists.
    pub fn r_full(&self) -> f64 {
        self.r_full
    }

    /// The depth where the top width peaks, and that width (m) — in that
    /// order, which is the opposite of how the sentence reads. Callers
    /// want `.1` far more often than `.0`.
    pub fn w_max(&self) -> (f64, f64) {
        (self.y_at_w_max, self.w_max)
    }

    /// The depth where the section factor peaks, and that peak
    /// $\Psi_{max}$ (§5.1) — depth first, as [`Self::w_max`] does.
    pub fn psi_max(&self) -> (f64, f64) {
        (self.y_at_psi_max, self.psi_max)
    }

    /// Flow area (m²) at depth `y`.
    pub fn area(&self, y: f64) -> f64 {
        let y = y.clamp(0.0, self.y_full);
        match &self.kind {
            Kind::Dummy => 0.0,
            Kind::Circle { d } => circle_area(*d, y),
            Kind::FilledCircle {
                d, y_bot, a_bot, ..
            } => circle_area(*d, y + y_bot) - a_bot,
            Kind::RectClosed { w } => w * y,
            Kind::RectOpen { w, .. } => w * y,
            Kind::Triangle { s } => s * y * y,
            Kind::Trapezoid { b, sl, sr } => (b + (sl + sr) / 2.0 * y) * y,
            Kind::Parabola { k } => 4.0 / 3.0 * k * y * y.sqrt(),
            Kind::Power { h, m } => 2.0 * h * y.powf(m + 1.0) / (m + 1.0),
            Kind::RectTriangle { w, y_bot, s } => {
                if y <= *y_bot {
                    s * y * y
                } else {
                    s * y_bot * y_bot + w * (y - y_bot)
                }
            }
            Kind::RectRound {
                w, r, y_bot, a_bot, ..
            } => {
                if y <= *y_bot {
                    segment_area(*r, y)
                } else {
                    a_bot + w * (y - y_bot)
                }
            }
            Kind::ModBasket {
                w,
                r,
                y_bot,
                a_bot,
                theta,
            } => {
                let y_spring = self.y_full - y_bot;
                if y <= y_spring {
                    w * y
                } else {
                    // The filled part of the roof arc: total arc area
                    // minus the empty segment above the water line, whose
                    // height above the arc's centre is hw + r·cos(θ/2).
                    let hw = y - y_spring;
                    let c0 = r * (theta / 2.0).cos();
                    let phi = 2.0 * (((hw + c0) / r).clamp(-1.0, 1.0)).acos();
                    let empty = r * r / 2.0 * (phi - phi.sin());
                    w * y_spring + (a_bot - empty)
                }
            }
            Kind::Ellipse {
                a, b, area_scale, ..
            } => {
                let t = 2.0 * ((1.0 - y / b).clamp(-1.0, 1.0)).acos();
                a * b / 2.0 * (t - t.sin()) * area_scale
            }
            Kind::Tabulated { family, a_full, .. } => {
                let yn = y / self.y_full;
                match family.a_table() {
                    Some(t) => a_full * lookup(yn, t),
                    // No area table: the depth table's inverse (§5.3).
                    None => a_full * inv_lookup(yn, family.y_table().unwrap_or(&[0.0, 1.0])),
                }
            }
            Kind::Transect(t) => t.sweep_geom(y).0,
            Kind::Custom { ys, ws } => {
                let mut area = 0.0;
                for i in 1..ys.len() {
                    if y <= ys[i - 1] {
                        break;
                    }
                    let (y1, w1) = custom_edge(ys, ws, i, y);
                    area += 0.5 * (ws[i - 1] + w1) * (y1 - ys[i - 1]);
                }
                area
            }
        }
    }

    /// Top width (m) at depth `y`.
    pub fn top_width(&self, y: f64) -> f64 {
        let y = y.clamp(0.0, self.y_full);
        match &self.kind {
            Kind::Dummy => 0.0,
            Kind::Circle { d } => circle_width(*d, y),
            Kind::FilledCircle { d, y_bot, .. } => circle_width(*d, y + y_bot),
            Kind::RectClosed { w } | Kind::RectOpen { w, .. } => *w,
            Kind::Triangle { s } => 2.0 * s * y,
            Kind::Trapezoid { b, sl, sr } => b + (sl + sr) * y,
            Kind::Parabola { k } => 2.0 * k * y.sqrt(),
            Kind::Power { h, m } => 2.0 * h * y.powf(*m),
            Kind::RectTriangle { w, y_bot, s } => {
                if y <= *y_bot {
                    2.0 * s * y
                } else {
                    *w
                }
            }
            Kind::RectRound { w, r, y_bot, .. } => {
                if y <= *y_bot {
                    2.0 * (y * (2.0 * r - y)).max(0.0).sqrt()
                } else {
                    *w
                }
            }
            Kind::ModBasket {
                w, r, y_bot, theta, ..
            } => {
                let y_spring = self.y_full - y_bot;
                if y <= y_spring {
                    *w
                } else {
                    let hw = y - y_spring;
                    let c0 = r * (theta / 2.0).cos();
                    2.0 * (r * r - (hw + c0) * (hw + c0)).max(0.0).sqrt()
                }
            }
            Kind::Ellipse { a, b, .. } => {
                let t = 2.0 * ((1.0 - y / b).clamp(-1.0, 1.0)).acos();
                2.0 * a * (t / 2.0).sin()
            }
            Kind::Tabulated { family, w_max, .. } => {
                w_max * lookup(y / self.y_full, family.w_table())
            }
            Kind::Transect(t) => t.sweep_geom(y).1,
            Kind::Custom { ys, ws } => interp(ys, ws, y),
        }
    }

    /// Wetted perimeter (m) at depth `y`, excluding any lid below full
    /// depth; at `y = y_full` the lid is included (§5.2).
    pub fn perimeter(&self, y: f64) -> f64 {
        let y = y.clamp(0.0, self.y_full);
        let p = self.perimeter_open(y);
        if y >= self.y_full {
            p + self.lid_width()
        } else {
            p
        }
    }

    fn perimeter_open(&self, y: f64) -> f64 {
        match &self.kind {
            Kind::Dummy => 0.0,
            Kind::Circle { d } => circle_perimeter(*d, y),
            Kind::FilledCircle {
                d,
                y_bot,
                p_bot,
                w_bot,
                ..
            } => circle_perimeter(*d, y + y_bot) - p_bot + w_bot,
            Kind::RectClosed { w } => w + 2.0 * y,
            Kind::RectOpen { w, sides_ignored } => w + (2.0 - sides_ignored) * y,
            Kind::Triangle { s } => 2.0 * y * (1.0 + s * s).sqrt(),
            Kind::Trapezoid { b, sl, sr } => {
                b + y * ((1.0 + sl * sl).sqrt() + (1.0 + sr * sr).sqrt())
            }
            Kind::Parabola { k } => {
                // Closed-form parabola arc length in the slope parameter
                // u = 2√y/k.
                let u = 2.0 * y.sqrt() / k;
                let t = (1.0 + u * u).sqrt();
                0.5 * k * k * (u * t + (u + t).ln())
            }
            Kind::Power { h, m } => {
                // Arc length of x = h·y^m, both sides, by quadrature.
                let f = |t: f64| {
                    let dx = h * *m * t.powf(m - 1.0);
                    (1.0 + dx * dx).sqrt()
                };
                if *m >= 1.0 {
                    2.0 * integrate(&f, 0.0, y)
                } else {
                    // dx/dy is singular at zero here: integrate along x
                    // instead, where dy/dx vanishes at the origin.
                    let x_end = h * y.powf(*m);
                    let g = |x: f64| {
                        if x <= 0.0 {
                            return 1.0;
                        }
                        let dydx = (x / h).powf(1.0 / m) / (m * x);
                        (1.0 + dydx * dydx).sqrt()
                    };
                    2.0 * integrate(&g, 0.0, x_end)
                }
            }
            Kind::RectTriangle { y_bot, s, .. } => {
                let slant = (1.0 + s * s).sqrt();
                if y <= *y_bot {
                    2.0 * y * slant
                } else {
                    2.0 * y_bot * slant + 2.0 * (y - y_bot)
                }
            }
            Kind::RectRound {
                r, y_bot, theta, ..
            } => {
                if y <= *y_bot {
                    let t = 2.0 * ((1.0 - y / r).clamp(-1.0, 1.0)).acos();
                    r * t
                } else {
                    r * theta + 2.0 * (y - y_bot)
                }
            }
            Kind::ModBasket {
                w, r, y_bot, theta, ..
            } => {
                let y_spring = self.y_full - y_bot;
                if y <= y_spring {
                    w + 2.0 * y
                } else {
                    let hw = y - y_spring;
                    let c0 = r * (theta / 2.0).cos();
                    let phi = 2.0 * (((hw + c0) / r).clamp(-1.0, 1.0)).acos();
                    w + 2.0 * y_spring + r * (theta - phi)
                }
            }
            Kind::Ellipse { a, b, .. } => {
                // Arc length by quadrature on the angle parameter.
                let t_end = ((1.0 - y / b).clamp(-1.0, 1.0)).acos();
                let f = |t: f64| ((a * t.cos()).powi(2) + (b * t.sin()).powi(2)).sqrt();
                2.0 * integrate(&f, 0.0, t_end)
            }
            Kind::Tabulated { .. } => {
                // The tables provide R directly (§5.3); perimeter is the
                // derived quantity here.
                let r = self.tabulated_radius(y);
                if r <= 0.0 {
                    0.0
                } else {
                    self.area(y) / r
                }
            }
            Kind::Transect(t) => t.sweep_geom(y).2,
            Kind::Custom { ys, ws } => {
                // Bottom width plus the two side slants.
                let mut p = ws[0];
                for i in 1..ys.len() {
                    if y <= ys[i - 1] {
                        break;
                    }
                    let (y1, w1) = custom_edge(ys, ws, i, y);
                    let dy = y1 - ys[i - 1];
                    let dx = (w1 - ws[i - 1]) / 2.0;
                    p += 2.0 * (dy * dy + dx * dx).sqrt();
                }
                p
            }
        }
    }

    /// Area, top width and open perimeter at depth `y`, in one pass.
    ///
    /// Each is available on its own, and asking for them separately makes
    /// a shape rebuild whatever the three have in common: the circle's
    /// filled angle costs an inverse cosine every time it is asked for,
    /// and a transect re-walks its entire survey. §5.7's inversions and
    /// §6's trials want two or three of them at every iteration, which is
    /// where that repetition lands. The values are the ones `area`,
    /// `top_width` and `perimeter_open` give, to the last bit — this
    /// shares the arithmetic rather than approximating it.
    fn geom(&self, y: f64) -> (f64, f64, f64) {
        let y = y.clamp(0.0, self.y_full);
        match &self.kind {
            Kind::Circle { d } => {
                let t = circle_theta(*d, y);
                (circle_area(*d, y), circle_width(*d, y), d * t / 2.0)
            }
            Kind::FilledCircle {
                d,
                y_bot,
                a_bot,
                p_bot,
                w_bot,
            } => {
                let t = circle_theta(*d, y + y_bot);
                (
                    circle_area(*d, y + y_bot) - a_bot,
                    circle_width(*d, y + y_bot),
                    d * t / 2.0 - p_bot + w_bot,
                )
            }
            Kind::Transect(t) => t.sweep_geom(y),
            _ => (self.area(y), self.top_width(y), self.perimeter_open(y)),
        }
    }

    /// Hydraulic radius (m) at depth `y`.
    pub fn hyd_radius(&self, y: f64) -> f64 {
        self.area_and_radius(y).1
    }

    /// Area and hydraulic radius together, the two halves of the section
    /// factor (§5.1).
    ///
    /// Asked for separately they build the section twice, and §5.7's
    /// normal-depth inversion asks for both at every one of its
    /// iterations — which is where the repetition costs most.
    pub(crate) fn area_and_radius(&self, y: f64) -> (f64, f64) {
        let yc = y.clamp(0.0, self.y_full);
        if yc <= 0.0 {
            return (self.area(y), 0.0);
        }
        if yc >= self.y_full {
            return (self.a_full, self.r_full);
        }
        if matches!(self.kind, Kind::Tabulated { .. }) {
            return (self.area(y), self.tabulated_radius(yc));
        }
        if let Kind::Transect(t) = &self.kind {
            // Composite roughness by conveyance summation (§5.6): the
            // effective radius back-computed through the channel n, the
            // same constant in both directions.
            let (a, _, _, k) = t.sweep(yc);
            if a <= 0.0 {
                return (a, 0.0);
            }
            return (a, (t.n_channel * k / a).powf(1.5));
        }
        let (a, _, p) = self.geom(yc);
        if p <= 0.0 {
            return (a, 0.0);
        }
        let r = a / p;
        (
            a,
            match &self.kind {
                Kind::Ellipse { r_scale, .. } => r * r_scale,
                _ => r,
            },
        )
    }

    /// Area, hydraulic radius and top width at one depth, from one pass.
    ///
    /// [`Self::area_and_radius`] already computes the width — `geom`
    /// returns it — and drops it, and §6.3's momentum update then asks for
    /// it separately at the same depth. On a circular section that second
    /// ask is another inverse cosine for a number the first call held, and
    /// a circular section is most of a drainage model. The values are the
    /// ones the separate accessors give, to the last bit: this shares the
    /// arithmetic rather than approximating it.
    pub(crate) fn area_radius_width(&self, y: f64) -> (f64, f64, f64) {
        let yc = y.clamp(0.0, self.y_full);
        // The kinds `area_and_radius` answers without `geom` compute their
        // width cheaply, so they keep their own path rather than pay for
        // one they would not otherwise take.
        if yc >= self.y_full {
            return (self.a_full, self.r_full, self.w_full);
        }
        if yc <= 0.0 || matches!(self.kind, Kind::Tabulated { .. } | Kind::Transect(_)) {
            let (a, r) = self.area_and_radius(y);
            return (a, r, self.top_width(y));
        }
        let (a, w, p) = self.geom(yc);
        if p <= 0.0 {
            return (a, 0.0, w);
        }
        let r = a / p;
        let r = match &self.kind {
            Kind::Ellipse { r_scale, .. } => r * r_scale,
            _ => r,
        };
        (a, r, w)
    }

    /// A tabulated family's hydraulic radius: from its R table where one
    /// exists, else derived from the section factor as $R = (\Psi/A)^{3/2}$
    /// (§5.3).
    fn tabulated_radius(&self, y: f64) -> f64 {
        let Kind::Tabulated {
            family,
            a_full,
            r_full,
            ..
        } = &self.kind
        else {
            return 0.0;
        };
        if let Some(t) = family.r_table() {
            return r_full * lookup(y / self.y_full, t);
        }
        let a = self.area(y);
        if a <= 0.0 {
            return 0.0;
        }
        let s_full = a_full * r_full.powf(2.0 / 3.0);
        let s = match family.s_table() {
            Some(t) => s_full * lookup(a / a_full, t),
            None => return 0.0,
        };
        (s / a).powf(1.5)
    }

    /// The Manning section factor $\Psi(y) = A R^{2/3}$ (§5.1). Families
    /// with a section-factor table read it directly (§5.3).
    pub fn psi(&self, y: f64) -> f64 {
        if let Kind::Tabulated {
            family,
            a_full,
            r_full,
            ..
        } = &self.kind
        {
            if let Some(t) = family.s_table() {
                let s_full = a_full * two_thirds(*r_full);
                return s_full * lookup(self.area(y) / a_full, t);
            }
        }
        let (a, r) = self.area_and_radius(y);
        a * two_thirds(r)
    }

    /// Depth from area (§5.7): closed form where §5.2 provides one,
    /// bracketed inversion of the monotone relation otherwise.
    pub fn depth_of_area(&self, a: f64) -> f64 {
        if a <= 0.0 || self.y_full <= 0.0 {
            return 0.0;
        }
        if a >= self.a_full {
            return self.y_full;
        }
        match &self.kind {
            Kind::RectClosed { w } | Kind::RectOpen { w, .. } => a / w,
            Kind::Triangle { s } => (a / s).sqrt(),
            Kind::Parabola { k } => (3.0 * a / (4.0 * k)).powf(2.0 / 3.0),
            Kind::Power { h, m } => (a * (m + 1.0) / (2.0 * h)).powf(1.0 / (m + 1.0)),
            Kind::Tabulated { family, a_full, .. } => match family.y_table() {
                Some(t) => self.y_full * lookup(a / a_full, t),
                // No depth table: the area table's inverse (§5.3).
                None => {
                    self.y_full * inv_lookup(a / a_full, family.a_table().unwrap_or(&[0.0, 1.0]))
                }
            },
            _ => invert(&|y| self.area(y), 0.0, self.y_full, a),
        }
    }

    /// Critical depth for discharge `q` (§5.7): solves $A^3/W = Q^2/g$,
    /// exactly where the section admits it, by bracketed solve otherwise;
    /// a section that cannot pass the discharge reports full.
    pub fn critical_depth(&self, q: f64) -> f64 {
        if q <= 0.0 || self.y_full <= 0.0 {
            return 0.0;
        }
        let target = q * q / GRAVITY;
        match &self.kind {
            Kind::RectClosed { w } | Kind::RectOpen { w, .. } => {
                ((q / w).powi(2) / GRAVITY).powf(1.0 / 3.0).min(self.y_full)
            }
            Kind::Triangle { s } => (2.0 * target / (s * s)).powf(0.2).min(self.y_full),
            Kind::Parabola { k } => {
                // A³/W = (32/27)k²y⁴.
                (27.0 * target / (32.0 * k * k)).powf(0.25).min(self.y_full)
            }
            Kind::Power { h, m } => {
                // A³/W = 4h²y^(2m+3)/(m+1)³.
                let e = 2.0 * m + 3.0;
                (target * (m + 1.0).powi(3) / (4.0 * h * h))
                    .powf(1.0 / e)
                    .min(self.y_full)
            }
            Kind::Circle { d } => {
                // §5.7: solved on the filled angle. The full check is the
                // generic arm's, precomputed.
                if self.crit_full < target {
                    self.y_full
                } else {
                    // W(θ) is 0 at exactly 2π; stop a hair short so its
                    // logarithm exists.
                    let theta_hi = 2.0 * std::f64::consts::PI - 1e-9;
                    circle_char_solve(*d, 3.0, 1.0, true, target.ln(), theta_hi).min(self.y_full)
                }
            }
            Kind::Transect(t) => {
                // §5.7: Newton on depth, the derivatives from the same
                // survey walk as the values.
                if self.crit_full < target {
                    self.y_full
                } else {
                    let target_ln = target.ln();
                    newton_bracketed(
                        &|y| {
                            let s = t.sweep_d(y);
                            let a = s.area.max(1e-300);
                            let w = s.width.max(1e-30);
                            (
                                3.0 * a.ln() - w.ln() - target_ln,
                                3.0 * s.width / a - s.dw / w,
                            )
                        },
                        0.0,
                        self.y_full,
                    )
                }
            }
            _ => {
                let f = |y: f64| {
                    let (a, w, _) = self.geom(y);
                    a.powi(3) / w.max(1e-30)
                };
                if self.crit_full < target {
                    self.y_full
                } else {
                    invert(&f, 0.0, self.y_full, target)
                }
            }
        }
    }

    /// Normal depth for the section-factor demand $\Psi = nQ/\sqrt{S_0}$
    /// (§5.7), inverted on the monotone branch below the peak; a demand
    /// beyond $\Psi_{max}$ has no normal depth and reports `None`.
    pub fn normal_depth(&self, target_psi: f64) -> Option<f64> {
        if target_psi <= 0.0 || self.y_full <= 0.0 {
            return Some(0.0);
        }
        if target_psi > self.psi_max {
            return None;
        }
        if let Kind::Circle { d } = &self.kind {
            // §5.7: solved on the filled angle, over the monotone branch
            // below the section-factor peak.
            let theta_hi = circle_theta(*d, self.y_at_psi_max);
            return Some(
                circle_char_solve(*d, 5.0 / 3.0, 2.0 / 3.0, false, target_psi.ln(), theta_hi)
                    .min(self.y_at_psi_max),
            );
        }
        if let Kind::Transect(t) = &self.kind {
            // §5.7: for the transect Ψ = A·R^{2/3} collapses to n_C·K —
            // the §5.6 effective radius is defined through K — so the
            // solve is on the conveyance sum, derivative from the same
            // walk.
            let target_ln = target_psi.ln();
            return Some(
                newton_bracketed(
                    &|y| {
                        let s = t.sweep_d(y);
                        let k = (t.n_channel * s.k).max(1e-300);
                        (k.ln() - target_ln, s.dk / s.k.max(1e-300))
                    },
                    0.0,
                    self.y_at_psi_max,
                )
                .min(self.y_at_psi_max),
            );
        }
        Some(invert(&|y| self.psi(y), 0.0, self.y_at_psi_max, target_psi))
    }
}

/// Safeguarded Newton on depth (§5.7): `eval` answers the residual of a
/// monotone non-decreasing relation and its depth-derivative. Steps are
/// confined to a maintained bracket — one leaving it becomes the
/// midpoint, so termination is as sure as bisection — and the solve stops
/// when successive iterates agree within the stated tolerance.
fn newton_bracketed(eval: &dyn Fn(f64) -> (f64, f64), mut lo: f64, mut hi: f64) -> f64 {
    let mut y = 0.5 * (lo + hi);
    let mut y_prev = f64::NAN;
    for _ in 0..60 {
        let (g, dg) = eval(y);
        if g < 0.0 {
            lo = y;
        } else {
            hi = y;
        }
        if (y - y_prev).abs() <= 0.5 * INVERT_TOL {
            return y;
        }
        y_prev = y;
        let mut y_next = if dg.is_finite() && dg > 0.0 {
            y - g / dg
        } else {
            f64::NAN
        };
        if !(y_next > lo && y_next < hi) {
            y_next = 0.5 * (lo + hi);
        }
        if hi - lo <= INVERT_TOL {
            break;
        }
        y = y_next;
    }
    0.5 * (lo + hi)
}

/// The circle's two characteristic-depth solves (§5.7), by Newton on
/// $u = \ln\theta$ with a maintained bracket.
///
/// Both relations have the form $k_A \ln A - k_D \ln D = \ln t$ with $D$
/// the top width (critical depth) or wetted perimeter (normal depth), and
/// both are asymptotically affine in $u$ at the dry end — which gives a
/// closed-form seed and makes the iteration converge in a handful of
/// steps, none of which needs an inverse trigonometric call. A Newton
/// step leaving the bracket is replaced by its midpoint, so the solve
/// terminates as surely as bisection; it stops when successive iterates
/// agree within the stated depth tolerance.
/// `theta_hi` is the top of the monotone range: a hair under $2\pi$ for
/// critical depth, the section-factor peak's angle for normal depth.
fn circle_char_solve(
    d: f64,
    ka: f64,
    kd: f64,
    den_is_width: bool,
    target_ln: f64,
    theta_hi: f64,
) -> f64 {
    let y_of = |theta: f64| d / 2.0 * (1.0 - (theta / 2.0).cos());
    // θ ≈ 8×10⁻⁷ is a depth of ~10⁻¹³·d — dry beyond anything downstream
    // reads; a root below the bracket answers as that trickle.
    let mut u_lo = -14.0_f64;
    let mut u_hi = theta_hi.ln();
    // Dry-end asymptote: A → d²θ³/48 and both denominators → dθ/2, so
    // G(u) ≈ (3k_A − k_D)·u + c₀ − ln t. Solving that seeds the iteration
    // within about one unit of the root for any physical depth.
    let ln_d_half = (d / 2.0).ln();
    let c0 = ka * (d * d / 48.0).ln() - kd * ln_d_half;
    let mut u = ((target_ln - c0) / (3.0 * ka - kd)).clamp(u_lo, u_hi);
    let mut y_prev = f64::NAN;
    for _ in 0..60 {
        let theta = u.exp();
        // One half-angle sine and cosine carry the whole iteration.
        // $\sin\theta = 2\sin(\theta/2)\cos(\theta/2)$ and
        // $1 - \cos\theta = 2\sin^2(\theta/2)$, and the second of those
        // is the better form as well as the cheaper one: subtracting a
        // cosine from one cancels where this does not. Below a
        // thirteenth of a radian $\theta - \sin\theta$ falls away to
        // $\theta^3/6$, and its series is more accurate there than
        // either difference.
        let (sh, ch) = (theta / 2.0).sin_cos();
        let one_minus_c = 2.0 * sh * sh;
        const SMALL: f64 = 0.13;
        let a_t = if theta < SMALL {
            let t2 = theta * theta;
            theta * t2 / 6.0 * (1.0 - t2 / 20.0 * (1.0 - t2 / 42.0))
        } else {
            theta - 2.0 * sh * ch
        };
        // $\ln(d\theta/2) = \ln(d/2) + u$, since $\theta = e^u$: the
        // perimeter's logarithm is free.
        let ln_den = if den_is_width {
            (d * sh).ln()
        } else {
            ln_d_half + u
        };
        let g = ka * (d * d / 8.0 * a_t).ln() - kd * ln_den - target_ln;
        if g < 0.0 {
            u_lo = u;
        } else {
            u_hi = u;
        }
        let y = d / 2.0 * (1.0 - ch);
        if (y - y_prev).abs() <= 0.5 * INVERT_TOL {
            return y;
        }
        y_prev = y;
        let dg_dtheta = ka * one_minus_c / a_t
            - kd * if den_is_width {
                ch / (2.0 * sh)
            } else {
                1.0 / theta
            };
        let dg_du = theta * dg_dtheta;
        let mut u_next = if dg_du.is_finite() && dg_du > 0.0 {
            u - g / dg_du
        } else {
            f64::NAN
        };
        if !(u_next > u_lo && u_next < u_hi) {
            u_next = 0.5 * (u_lo + u_hi);
        }
        if u_hi - u_lo < 1e-12 {
            break;
        }
        u = u_next;
    }
    y_of((0.5 * (u_lo + u_hi)).exp())
}

/// Area of a circular segment of radius `r` filled to depth `y` from the
/// circle's lowest point.
fn segment_area(r: f64, y: f64) -> f64 {
    let t = 2.0 * ((1.0 - y / r).clamp(-1.0, 1.0)).acos();
    r * r / 2.0 * (t - t.sin())
}

/// The top edge of custom-shape segment `i` when the water stands at `y`:
/// its depth, and the section's width there.
///
/// A segment the water covers ends at its own top, where the width is the
/// curve point `ws[i]` — reached through the interpolation expression
/// rather than the point itself, because that is the arithmetic a walk up
/// the section does and the two differ in the last bit. Only the topmost,
/// partly filled segment needs a search at all, and the walk visits it
/// last; searching at every segment instead made the walk quadratic in the
/// curve's length, which a hundred-point shape curve pays for whenever the
/// water stands high in the section.
fn custom_edge(ys: &[f64], ws: &[f64], i: usize, y: f64) -> (f64, f64) {
    let y1 = ys[i].min(y);
    if y1 == ys[i] {
        (y1, ws[i - 1] + (ws[i] - ws[i - 1]))
    } else {
        (y1, interp(ys, ws, y1))
    }
}

/// $x^{2/3}$, Manning's exponent.
///
/// Reached through a cube root of the square rather than a general power.
/// `powf` would be handed `2.0 / 3.0`, which in binary is not two thirds,
/// and costs several times a cube root to raise to; over the range a
/// hydraulic radius takes, the square cannot overflow.
pub(crate) fn two_thirds(x: f64) -> f64 {
    (x * x).cbrt()
}

/// Linear interpolation of `ws` against increasing `ys`.
///
/// The scan is deliberate and was measured against the alternative. A
/// shape curve is walked at every trial of every step, so bisecting it
/// looks like the same win the gage record's lookup took — but a gage
/// record is hundreds of thousands of intervals and a shape curve is a
/// couple of dozen points. Over that many contiguous doubles the scan's
/// branch is predicted and its reads are one cache line, where bisection
/// pays five unpredictable branches; swapping it in cost 6% of the whole
/// run. Leave it.
fn interp(ys: &[f64], ws: &[f64], y: f64) -> f64 {
    if y <= ys[0] {
        return ws[0];
    }
    // The scan returned at the first `i >= 1` with `y <= ys[i]`; that is
    // the first index whose value is not below `y`.
    for i in 1..ys.len() {
        if y <= ys[i] {
            let f = (y - ys[i - 1]) / (ys[i] - ys[i - 1]);
            return ws[i - 1] + f * (ws[i] - ws[i - 1]);
        }
    }
    *ws.last().unwrap_or(&0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn build(shape: XsectShape, geom: [f64; 4]) -> Section {
        build_section(shape, geom, 1.0, None).unwrap().section
    }

    #[test]
    fn circle_evaluates_in_closed_form() {
        let s = build(XsectShape::Circular, [1.0, 0.0, 0.0, 0.0]);
        assert!((s.area(0.5) - PI / 8.0).abs() < 1e-14);
        assert!((s.top_width(0.5) - 1.0).abs() < 1e-14);
        assert!((s.a_full() - PI / 4.0).abs() < 1e-14);
        assert!((s.r_full() - 0.25).abs() < 1e-14);
        assert!((s.hyd_radius(0.5) - 0.25).abs() < 1e-14);
    }

    #[test]
    fn circle_psi_peaks_below_full_depth() {
        let s = build(XsectShape::Circular, [1.0, 0.0, 0.0, 0.0]);
        let (y, p) = s.psi_max();
        // The true Manning peak sits near 0.94·y_full at ≈ 1.076·Ψ_full —
        // not the predecessor's fitted 1.08 (§5.1).
        assert!((0.93..0.95).contains(&y), "peak depth {y}");
        let ratio = p / (s.a_full() * s.r_full().powf(2.0 / 3.0));
        assert!((1.07..1.08).contains(&ratio), "peak ratio {ratio}");
    }

    /// The circle's Newton-on-θ solves (§5.7) answer the same question
    /// the bracketed inversion answers, to the stated tolerance.
    ///
    /// The reference is an 80-halving bisection on the identical relation,
    /// run right here — so this holds the fast path to the slow one over
    /// the whole monotone range, from a trickle to just under the caps.
    #[test]
    fn circle_characteristic_depths_match_fine_bisection() {
        let s = build(XsectShape::Circular, [1.2, 0.0, 0.0, 0.0]);

        for q in [1e-4, 0.01, 0.1, 0.5, 1.5, 4.0] {
            let yc = s.critical_depth(q);
            let f = |y: f64| s.area(y).powi(3) / s.top_width(y).max(1e-30);
            let target = q * q / GRAVITY;
            if f(s.y_full()) < target {
                assert_eq!(yc, s.y_full(), "q={q} should cap at full");
                continue;
            }
            let (mut a, mut b) = (0.0, s.y_full());
            for _ in 0..80 {
                let m = 0.5 * (a + b);
                if f(m) < target {
                    a = m;
                } else {
                    b = m;
                }
            }
            let reference = 0.5 * (a + b);
            assert!(
                (yc - reference).abs() < 2e-6,
                "critical q={q}: {yc} vs {reference}"
            );
        }

        let (y_peak, psi_max) = s.psi_max();
        for frac in [1e-6, 1e-3, 0.05, 0.3, 0.7, 0.95, 0.999] {
            let t = frac * psi_max;
            let yn = s.normal_depth(t).expect("below the peak");
            let (mut a, mut b) = (0.0, y_peak);
            for _ in 0..80 {
                let m = 0.5 * (a + b);
                if s.psi(m) < t {
                    a = m;
                } else {
                    b = m;
                }
            }
            let reference = 0.5 * (a + b);
            assert!(
                (yn - reference).abs() < 2e-6,
                "normal frac={frac}: {yn} vs {reference}"
            );
        }
    }

    /// The transect's Newton-on-depth solves (§5.7) answer what the
    /// bracketed inversion answers, to the stated tolerance — same
    /// fine-bisection reference as the circle's test, on a surveyed
    /// section with distinct overbank roughness and a re-emerging bank.
    #[test]
    fn transect_characteristic_depths_match_fine_bisection() {
        let s = build_transect_section(&crate::model::Transect {
            id: "t".into(),
            stations: vec![
                (5.0, 0.0),
                (4.5, 55.0),
                (0.0, 60.0),
                (2.0, 95.0),
                (4.0, 115.0),
                (6.0, 160.0),
            ],
            x_left: 55.0,
            x_right: 115.0,
            n_left: 0.06,
            n_right: 0.05,
            n_channel: 0.035,
            meander_factor: 1.0,
        })
        .unwrap()
        .section;

        for q in [1e-3, 0.5, 5.0, 40.0, 200.0] {
            let yc = s.critical_depth(q);
            let f = |y: f64| s.area(y).powi(3) / s.top_width(y).max(1e-30);
            let target = q * q / GRAVITY;
            if f(s.y_full()) < target {
                assert_eq!(yc, s.y_full(), "q={q} should cap at full");
                continue;
            }
            let (mut a, mut b) = (0.0, s.y_full());
            for _ in 0..80 {
                let m = 0.5 * (a + b);
                if f(m) < target {
                    a = m;
                } else {
                    b = m;
                }
            }
            let reference = 0.5 * (a + b);
            assert!(
                (yc - reference).abs() < 2e-6,
                "critical q={q}: {yc} vs {reference}"
            );
        }

        let (y_peak, psi_max) = s.psi_max();
        for frac in [1e-5, 1e-2, 0.2, 0.6, 0.9, 0.999] {
            let t = frac * psi_max;
            let yn = s.normal_depth(t).expect("below the peak");
            let (mut a, mut b) = (0.0, y_peak);
            for _ in 0..80 {
                let m = 0.5 * (a + b);
                if s.psi(m) < t {
                    a = m;
                } else {
                    b = m;
                }
            }
            let reference = 0.5 * (a + b);
            assert!(
                (yn - reference).abs() < 2e-6,
                "normal frac={frac}: {yn} vs {reference}"
            );
        }
    }

    #[test]
    fn spec_example_critical_depth() {
        // §5.7: b = 2 m, Q = 3 m³/s → y_c = (q²/g)^⅓ = 0.6122 m.
        let s = build(XsectShape::RectOpen, [5.0, 2.0, 0.0, 0.0]);
        let yc = s.critical_depth(3.0);
        assert!((yc - (2.25_f64 / GRAVITY).powf(1.0 / 3.0)).abs() < 1e-12);
        assert!((yc - 0.6122).abs() < 5e-5);
    }

    #[test]
    fn trapezoid_full_properties_match_hand_values() {
        // y=2, b=3, slopes 1.5 and 2.5.
        let s = build(XsectShape::Trapezoidal, [2.0, 3.0, 1.5, 2.5]);
        let a = (3.0 + 2.0 * 2.0) * 2.0; // (b + s̄·y)·y = 14
        assert!((s.a_full() - a).abs() < 1e-12);
        let p = 3.0 + 2.0 * ((1.0 + 2.25_f64).sqrt() + (1.0 + 6.25_f64).sqrt());
        assert!((s.r_full() - a / p).abs() < 1e-12);
        assert!((s.top_width(2.0) - 11.0).abs() < 1e-12);
    }

    #[test]
    fn filled_circle_reduces_the_full_values() {
        let d = 2.0;
        let fill = 0.5;
        let s = build(XsectShape::FilledCircular, [d, fill, 0.0, 0.0]);
        assert!((s.y_full() - 1.5).abs() < 1e-14);
        let a_bot = circle_area(d, fill);
        assert!((s.a_full() - (PI - a_bot)).abs() < 1e-12);
        // Perimeter: circle minus the fill arc plus the fill's flat top.
        let p = PI * d - circle_perimeter(d, fill) + circle_width(d, fill);
        assert!((s.r_full() - s.a_full() / p).abs() < 1e-12);
        // Area at depth y above the fill continues the circle.
        assert!((s.area(0.5) - (circle_area(d, 1.0) - a_bot)).abs() < 1e-12);
    }

    #[test]
    fn ellipse_with_equal_axes_is_the_circle() {
        let c = build(XsectShape::Circular, [2.0, 0.0, 0.0, 0.0]);
        let e = build(XsectShape::HorizEllipse, [2.0, 2.0, 0.0, 0.0]);
        for y in [0.2, 0.7, 1.3, 1.9] {
            assert!((c.area(y) - e.area(y)).abs() < 1e-10, "area at {y}");
            assert!((c.top_width(y) - e.top_width(y)).abs() < 1e-10);
            assert!((c.perimeter(y) - e.perimeter(y)).abs() < 1e-9);
        }
    }

    #[test]
    fn power_with_unit_exponent_is_the_triangle() {
        let t = build(XsectShape::Triangular, [2.0, 3.0, 0.0, 0.0]);
        let p = build(XsectShape::Power, [2.0, 3.0, 1.0, 0.0]);
        for y in [0.3, 1.0, 1.8] {
            assert!((t.area(y) - p.area(y)).abs() < 1e-10);
            assert!((t.top_width(y) - p.top_width(y)).abs() < 1e-10);
            assert!((t.perimeter(y) - p.perimeter(y)).abs() < 1e-8, "P at {y}");
        }
        assert!((t.critical_depth(4.0) - p.critical_depth(4.0)).abs() < 1e-9);
    }

    #[test]
    fn parabola_arc_length_matches_quadrature() {
        let s = build(XsectShape::Parabolic, [2.0, 3.0, 0.0, 0.0]);
        // Independent check of the closed form by quadrature on x.
        let k = 1.5 / 2.0_f64.sqrt();
        let y: f64 = 1.7;
        let x_end = k * y.sqrt();
        let f = |x: f64| (1.0 + (2.0 * x / (k * k)).powi(2)).sqrt();
        let expect = 2.0 * integrate(&f, 0.0, x_end);
        assert!((s.perimeter_open(y) - expect).abs() < 1e-9);
    }

    #[test]
    fn depth_of_area_round_trips() {
        let sections = [
            build(XsectShape::Circular, [1.3, 0.0, 0.0, 0.0]),
            build(XsectShape::Trapezoidal, [2.0, 3.0, 1.5, 2.5]),
            build(XsectShape::Parabolic, [2.0, 3.0, 0.0, 0.0]),
            build(XsectShape::RectRound, [3.0, 2.0, 1.5, 0.0]),
            build(XsectShape::ModBasketHandle, [3.0, 2.0, 1.5, 0.0]),
            build(XsectShape::RectTriangular, [3.0, 2.0, 1.0, 0.0]),
        ];
        for s in &sections {
            for f in [0.1, 0.4, 0.8, 0.99] {
                let y = f * s.y_full();
                let back = s.depth_of_area(s.area(y));
                // To the §5.7 stated tolerance: the bracketed inversions
                // answer within half a micron bracket, not to machine
                // precision.
                assert!((back - y).abs() < 1e-6, "{back} vs {y}");
            }
        }
    }

    #[test]
    fn normal_depth_reports_full_when_demand_exceeds_peak() {
        let s = build(XsectShape::Circular, [1.0, 0.0, 0.0, 0.0]);
        let (_, psi_max) = s.psi_max();
        assert_eq!(s.normal_depth(psi_max * 1.01), None);
        let y = s.normal_depth(psi_max * 0.5).unwrap();
        // The depth answers to §5.7's stated micron tolerance, and the
        // section factor is smooth (order-one slope for this circle), so
        // its residual is of the same order.
        assert!((s.psi(y) - psi_max * 0.5).abs() < 1e-6);
    }

    #[test]
    fn infeasible_bottom_radius_is_raised() {
        let b = build_section(XsectShape::RectRound, [3.0, 2.0, 0.5, 0.0], 1.0, None).unwrap();
        assert_eq!(b.radius_raised, Some(1.0));
        // At the minimum radius the bottom is a half-circle.
        let a_bot = PI * 1.0 * 1.0 / 2.0;
        assert!((b.section.area(1.0) - a_bot).abs() < 1e-12);
    }

    /// A submerged segment's width must need no search to find.
    ///
    /// The walk up a custom section skips the interpolation for every
    /// segment the water covers, on the grounds that interpolating at a
    /// segment's own top returns that segment's own width. That identity
    /// is the whole reason the walk is linear rather than quadratic in the
    /// curve's length, and it has to hold to the bit: a section whose area
    /// drifts in the last place is a section whose routing diverges over a
    /// long run, silently and only on the models with long shape curves.
    #[test]
    fn a_submerged_segment_needs_no_search() {
        // Widths chosen so the shortcut is not free: at each of these
        // steps `w_prev + (w - w_prev)` and `w` are different doubles, so
        // reading the curve point directly would answer differently from
        // the interpolation the walk performs. A smooth curve would let
        // that substitution pass unnoticed.
        let widths = [
            1.005_306_656_257_829,
            0.495_005_063_077_280_7,
            1.970_256_695_467_568_7,
            0.263_687_045_669_251_16,
            2.929_952_561_499_114,
            0.187_418_907_822_381_02,
            1.665_846_173_843_195_8,
            0.235_227_476_171_303_3,
            2.941_515_800_103_117_4,
            0.398_294_045_852_138_23,
            1.5,
        ];
        let points: Vec<(f64, f64)> = widths
            .iter()
            .enumerate()
            .map(|(i, w)| (i as f64 / widths.len() as f64, *w))
            .collect();
        // Unit full depth, so the build's scaling leaves the widths exact.
        let s = build_section(XsectShape::Custom, [1.0, 0.0, 0.0, 0.0], 1.0, Some(&points))
            .unwrap()
            .section;
        let Kind::Custom { ys, ws } = &s.kind else {
            panic!("expected a custom section");
        };
        assert!(ys.len() > 10, "curve should be long enough to matter");

        for i in 1..ys.len() {
            // Any water line at or above this segment's top leaves it
            // wholly submerged, whatever the depth above it.
            for y in [ys[i], ys[i] + 1e-9, s.y_full()] {
                let (y1, w1) = custom_edge(ys, ws, i, y);
                assert_eq!(y1, ys[i], "segment {i} should end at its own top");
                assert_eq!(w1, interp(ys, ws, ys[i]), "segment {i} width at {y}");
            }
            // Below its top it is the partly filled one, and the width is
            // the interpolation at the water line itself.
            let mid = (ys[i - 1] + ys[i]) / 2.0;
            let (y1, w1) = custom_edge(ys, ws, i, mid);
            assert_eq!(y1, mid);
            assert_eq!(w1, interp(ys, ws, mid));
        }
    }

    #[test]
    fn custom_shape_truncates_extends_and_closes() {
        // A unit square drawn as a shape curve that stops short of 1.
        let points = [(0.0, 1.0), (0.6, 1.0)];
        let b = build_section(XsectShape::Custom, [2.0, 0.0, 0.0, 0.0], 1.0, Some(&points))
            .unwrap()
            .section;
        // Scaled by y_full = 2: a 2×2 square.
        assert!((b.a_full() - 4.0).abs() < 1e-12);
        assert!((b.top_width(1.0) - 2.0).abs() < 1e-12);
        // Perimeter at full: bottom + two walls + lid.
        assert!((b.perimeter(2.0) - 8.0).abs() < 1e-12);
        let (y, p) = b.psi_max();
        // A closed square's Ψ still peaks below the lid.
        assert!(y < 2.0);
        assert!(p >= b.a_full() * b.r_full().powf(2.0 / 3.0));
    }

    #[test]
    fn width_is_the_area_derivative_everywhere() {
        // The §5.1 contract: W = dA/dy, for every family, checked by
        // integrating the width and comparing against the area.
        let sections = [
            build(XsectShape::Circular, [1.3, 0.0, 0.0, 0.0]),
            build(XsectShape::FilledCircular, [2.0, 0.5, 0.0, 0.0]),
            build(XsectShape::RectClosed, [2.0, 1.5, 0.0, 0.0]),
            build(XsectShape::Triangular, [2.0, 3.0, 0.0, 0.0]),
            build(XsectShape::Trapezoidal, [2.0, 3.0, 1.5, 2.5]),
            build(XsectShape::Parabolic, [2.0, 3.0, 0.0, 0.0]),
            build(XsectShape::Power, [2.0, 3.0, 1.7, 0.0]),
            build(XsectShape::RectTriangular, [3.0, 2.0, 1.0, 0.0]),
            build(XsectShape::RectRound, [3.0, 2.0, 1.5, 0.0]),
            build(XsectShape::ModBasketHandle, [3.0, 2.0, 1.5, 0.0]),
            build(XsectShape::HorizEllipse, [2.0, 3.0, 0.0, 0.0]),
        ];
        for s in &sections {
            for f in [0.25, 0.6, 0.97] {
                let y = f * s.y_full();
                let a = integrate(&|t| s.top_width(t), 0.0, y);
                assert!(
                    (a - s.area(y)).abs() < 1e-8 * (1.0 + s.area(y)),
                    "∫W = {a} vs A = {} at y = {y}",
                    s.area(y)
                );
            }
        }
    }

    #[test]
    fn egg_anchors_and_peak_follow_the_tables() {
        let s = build(XsectShape::Egg, [2.0, 0.0, 0.0, 0.0]);
        // Family constants anchor the full-flow values (§5.3).
        assert!((s.a_full() - 0.5105 * 4.0).abs() < 1e-12);
        assert!((s.r_full() - 0.1931 * 2.0).abs() < 1e-12);
        assert!((s.w_max().1 - 2.0 * 2.0 / 3.0).abs() < 1e-12);
        // The section-factor table's own peak is Ψ_max — near the
        // predecessor's fitted 1.065 constant but read from the table.
        let (y, p) = s.psi_max();
        let ratio = p / (s.a_full() * s.r_full().powf(2.0 / 3.0));
        assert!((1.05..1.08).contains(&ratio), "{ratio}");
        assert!(y < s.y_full());
        // Depth-from-area and area-from-depth are separately tabulated
        // inverses; they round-trip only to the tables' own mutual
        // consistency, worst near the egg's narrow bottom.
        for f in [0.2, 0.5, 0.8] {
            let y = f * s.y_full();
            let back = s.depth_of_area(s.area(y));
            assert!((back - y).abs() < 0.015 * s.y_full(), "{back} vs {y}");
        }
    }

    #[test]
    fn gothic_radius_derives_from_its_section_factor() {
        // No R table: R = (Ψ/A)^{3/2} (§5.3), so Ψ and A·R^{2/3} agree
        // identically.
        let s = build(XsectShape::Gothic, [3.0, 0.0, 0.0, 0.0]);
        for f in [0.2, 0.5, 0.9] {
            let y = f * s.y_full();
            let via_r = s.area(y) * s.hyd_radius(y).powf(2.0 / 3.0);
            assert!((s.psi(y) - via_r).abs() < 1e-9 * (1.0 + via_r));
        }
        assert!((s.a_full() - 0.6554 * 9.0).abs() < 1e-12);
    }

    #[test]
    fn coded_ellipse_lands_on_the_catalogue_row() {
        use crate::hydraulics::tables;
        // Size code 1: 14 in × 23 in.
        let b = build_section(XsectShape::HorizEllipse, [1.0, 0.0, 0.0, 0.0], 1.0, None)
            .unwrap()
            .section;
        let ft = 0.3048;
        assert!((b.y_full() - tables::ELLIPSE_MINOR_AXIS_IN[0] / 12.0 * ft).abs() < 1e-12);
        assert!((b.w_max().1 - tables::ELLIPSE_MAJOR_AXIS_IN[0] / 12.0 * ft).abs() < 1e-12);
        // Full-flow area and radius are the published values, exactly.
        assert!((b.a_full() - tables::ELLIPSE_A_FULL_FT2[0] * ft * ft).abs() < 1e-12);
        assert!((b.r_full() - tables::ELLIPSE_R_FULL_FT[0] * ft).abs() < 1e-12);
        // A vertical ellipse of the same code swaps the axes.
        let v = build_section(XsectShape::VertEllipse, [1.0, 0.0, 0.0, 0.0], 1.0, None)
            .unwrap()
            .section;
        assert!((v.y_full() - tables::ELLIPSE_MAJOR_AXIS_IN[0] / 12.0 * ft).abs() < 1e-12);
        assert!((v.a_full() - b.a_full()).abs() < 1e-12);
        // Catalogue dimensions ignore the file's unit system (§5.4).
        let si = build_section(XsectShape::HorizEllipse, [1.0, 0.0, 0.0, 0.0], 0.3048, None)
            .unwrap()
            .section;
        assert!((si.a_full() - b.a_full()).abs() < 1e-12);
        // The catalogues carry 23 and 102 rows.
        assert_eq!(tables::ELLIPSE_MINOR_AXIS_IN.len(), 23);
        assert_eq!(tables::ARCH_Y_FULL_IN.len(), 102);
        assert!(build_section(XsectShape::HorizEllipse, [24.0, 0.0, 0.0, 0.0], 1.0, None).is_err());
    }

    #[test]
    fn arch_uses_catalogue_or_proportionality_constants() {
        use crate::hydraulics::tables;
        let ft = 0.3048;
        let coded = build_section(XsectShape::Arch, [1.0, 0.0, 0.0, 0.0], 1.0, None)
            .unwrap()
            .section;
        assert!((coded.y_full() - tables::ARCH_Y_FULL_IN[0] / 12.0 * ft).abs() < 1e-12);
        assert!((coded.a_full() - tables::ARCH_A_FULL_FT2[0] * ft * ft).abs() < 1e-12);
        let user = build_section(XsectShape::Arch, [2.0, 3.0, 0.0, 0.0], 1.0, None)
            .unwrap()
            .section;
        assert!((user.a_full() - 0.7879 * 2.0 * 3.0).abs() < 1e-12);
        assert!((user.r_full() - 0.2991 * 2.0).abs() < 1e-12);
        // Depth variation from the transcribed tables: monotone area,
        // consistent inverse.
        for f in [0.3, 0.6, 0.95] {
            let y = f * user.y_full();
            let back = user.depth_of_area(user.area(y));
            assert!((back - y).abs() < 0.01 * user.y_full());
        }
    }

    #[test]
    fn arbitrary_ellipse_uses_the_axes_the_user_wrote() {
        // The predecessor would evaluate 1.2692·y² whatever the width;
        // this engine evaluates the true ellipse (§5.4 CORRESPONDENCE).
        let s = build(XsectShape::HorizEllipse, [2.0, 4.0, 0.0, 0.0]);
        assert!((s.a_full() - PI * 1.0 * 2.0).abs() < 1e-12);
    }

    fn simple_transect() -> crate::model::Transect {
        // A symmetric trapezoidal main channel (2 m deep, bottom 4 m,
        // 1:1 sides) between flat overbanks 5 m wide at 2 m elevation,
        // closed by the implicit end walls at 3 m.
        crate::model::Transect {
            id: "T".into(),
            n_left: 0.05,
            n_right: 0.05,
            n_channel: 0.03,
            x_left: 5.0,
            x_right: 13.0,
            meander_factor: 1.0,
            stations: vec![
                (3.0, 0.0),
                (2.0, 0.0),
                (2.0, 5.0),
                (0.0, 7.0),
                (0.0, 11.0),
                (2.0, 13.0),
                (2.0, 18.0),
                (3.0, 18.0),
            ],
        }
    }

    #[test]
    fn transect_geometry_matches_hand_values_below_the_banks() {
        let s = build_transect_section(&simple_transect()).unwrap().section;
        assert!((s.y_full() - 3.0).abs() < 1e-12);
        // At 1 m depth only the trapezoid is wet: A = (4+1)·1 = 5,
        // W = 6, P = 4 + 2√2.
        assert!((s.area(1.0) - 5.0).abs() < 1e-12);
        assert!((s.top_width(1.0) - 6.0).abs() < 1e-12);
        assert!((s.perimeter(1.0) - (4.0 + 2.0 * 2.0_f64.sqrt())).abs() < 1e-12);
        // One thread, one roughness: R is the plain A/P.
        let r = 5.0 / (4.0 + 2.0 * 2.0_f64.sqrt());
        assert!(
            (s.hyd_radius(1.0) - r).abs() < 1e-12,
            "{}",
            s.hyd_radius(1.0)
        );
    }

    #[test]
    fn composite_radius_follows_conveyance_summation() {
        let s = build_transect_section(&simple_transect()).unwrap().section;
        // At 2.5 m the overbanks carry 0.5 m: three conveyance
        // sub-sections with their own roughness (§5.6).
        let y = 2.5;
        // Trapezoid full (bottom 4, top 8, deep 2) plus the 0.5 m layer
        // over the 8 m channel span.
        let a_ch = (4.0 + 8.0) * 2.0 / 2.0 + 8.0 * 0.5;
        let a_ob = 5.0 * 0.5;
        let p_ob = 5.0 + 0.5; // ground + end wall
        let p_ch = 4.0 + 2.0 * 8.0_f64.sqrt();
        let k = |a: f64, p: f64, n: f64| a / n * (a / p).powf(2.0 / 3.0);
        let k_total = k(a_ch, p_ch, 0.03) + 2.0 * k(a_ob, p_ob, 0.05);
        let a_total = a_ch + 2.0 * a_ob;
        assert!((s.area(y) - a_total).abs() < 1e-12);
        let r_eff = (0.03 * k_total / a_total).powf(1.5);
        assert!(
            (s.hyd_radius(y) - r_eff).abs() < 1e-12,
            "{} vs {r_eff}",
            s.hyd_radius(y)
        );
    }

    #[test]
    fn meander_inflates_only_the_channel_roughness() {
        let mut t = simple_transect();
        t.meander_factor = 2.0;
        let s = build_transect_section(&t).unwrap().section;
        let plain = build_transect_section(&simple_transect()).unwrap().section;
        // Below the banks R_eff = (n_c·K/A)^{3/2} with K ∝ 1/n_c: the
        // inflation cancels, geometry unchanged.
        assert!((s.hyd_radius(1.0) - plain.hyd_radius(1.0)).abs() < 1e-12);
        // Above the banks the overbank terms don't scale, so the
        // composite differs.
        assert!((s.hyd_radius(2.5) - plain.hyd_radius(2.5)).abs() > 1e-6);
    }

    #[test]
    fn transect_accept_set_is_the_predecessors() {
        let mut flat = simple_transect();
        for st in &mut flat.stations {
            st.0 = 1.0;
        }
        assert!(build_transect_section(&flat).is_err());
        let mut backwards = simple_transect();
        backwards.stations[3].1 = 20.0;
        assert!(build_transect_section(&backwards).is_err());
        let mut banks = simple_transect();
        banks.x_left = 14.0;
        assert!(build_transect_section(&banks).is_err());
        let mut no_n = simple_transect();
        no_n.n_channel = 0.0;
        assert!(build_transect_section(&no_n).is_err());
        // Omitted overbank roughness defaults to the channel's: with all
        // zones equal, R at overbank depth is the plain A/P of one thread
        // split only by re-emerging ground — none here, so one thread.
        let mut defaulted = simple_transect();
        defaulted.n_left = 0.0;
        defaulted.n_right = 0.0;
        let s = build_transect_section(&defaulted).unwrap().section;
        let y = 2.5;
        let (a, p) = (s.area(y), s.perimeter(y));
        assert!((s.hyd_radius(y) - a / p).abs() < 1e-12);
    }

    #[test]
    fn street_compiles_to_a_transect() {
        // A one-sided street: 6 m crown width, 0.15 m curb, 2 % cross
        // slope, no gutter depression, 2 m backing at 4 %.
        let st = crate::model::Street {
            id: "ST".into(),
            crown_width: 6.0,
            curb_height: 0.15,
            cross_slope: 0.02,
            roughness: 0.016,
            gutter_depression: 0.0,
            gutter_width: 0.0,
            sides: 1,
            backing_width: 2.0,
            backing_slope: 0.04,
            backing_roughness: 0.02,
        };
        let s = build_street_section(&st).unwrap().section;
        // Crown rise 0.12 m stays below the curb top 0.15; backing tops
        // out at 0.15 + 0.08 = 0.23 = y_full.
        assert!((s.y_full() - 0.23).abs() < 1e-12);
        // At the crown depth the road is exactly full: the backing toe
        // sits at curb-top height and is still dry.
        let y = 0.12;
        assert!((s.top_width(y) - 6.0).abs() < 1e-12);
        // Area at curb-top depth: road triangle full plus backing toe.
        assert!(s.area(0.15) > 0.5 * 6.0 * 0.12);
        // A two-sided street doubles the road geometry.
        let two = crate::model::Street { sides: 2, ..st };
        let s2 = build_street_section(&two).unwrap().section;
        assert!((s2.area(0.12) - 2.0 * s.area(0.12)).abs() < 1e-9);
    }

    #[test]
    fn accept_set_matches_the_predecessor() {
        assert!(build_section(XsectShape::Circular, [0.0; 4], 1.0, None).is_err());
        assert!(build_section(XsectShape::Trapezoidal, [1.0, 0.0, 0.0, 0.0], 1.0, None).is_err());
        assert!(
            build_section(XsectShape::FilledCircular, [1.0, 1.0, 0.0, 0.0], 1.0, None).is_err()
        );
        assert!(build_section(XsectShape::RectOpen, [1.0, 1.0, 3.0, 0.0], 1.0, None).is_err());
        // Dummy is the one shape with no geometry to refuse.
        assert!(build_section(XsectShape::Dummy, [0.0; 4], 1.0, None).is_ok());
    }
}
