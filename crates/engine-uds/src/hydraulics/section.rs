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

use super::GRAVITY;
use crate::model::XsectShape;

/// Why a cross-section's geometry was refused — the predecessor's
/// `xsect_setParams` accept-set, verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    /// A geometry parameter outside its accepted range.
    BadGeometry(&'static str),
    /// A family this build stage does not construct yet: the tabulated
    /// families (§5.3), size catalogues (§5.4), and transect-backed
    /// sections (§5.6) arrive in later increments.
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
    w_max: f64,
    y_at_w_max: f64,
    psi_max: f64,
    y_at_psi_max: f64,
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
    /// Semi-axes: `a` horizontal, `b` vertical.
    Ellipse {
        a: f64,
        b: f64,
    },
    /// Piecewise-linear width against depth (§5.5), already scaled to
    /// metres and closed at the top.
    Custom {
        ys: Vec<f64>,
        ws: Vec<f64>,
    },
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
fn invert(f: &dyn Fn(f64) -> f64, mut a: f64, mut b: f64, target: f64) -> f64 {
    for _ in 0..100 {
        let m = 0.5 * (a + b);
        if f(m) < target {
            a = m;
        } else {
            b = m;
        }
    }
    0.5 * (a + b)
}

/// Circle geometry through the filled angle (§5.2), diameter `d`.
fn circle_theta(d: f64, y: f64) -> f64 {
    let c = (1.0 - 2.0 * y / d).clamp(-1.0, 1.0);
    2.0 * c.acos()
}

fn circle_area(d: f64, y: f64) -> f64 {
    let t = circle_theta(d, y);
    d * d / 8.0 * (t - t.sin())
}

fn circle_width(d: f64, y: f64) -> f64 {
    d * (circle_theta(d, y) / 2.0).sin()
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
            // A zero width or an explicit third value selects a
            // standard-size code — that path arrives with the §5.4
            // catalogues. Arbitrary axes are the analytic ellipse:
            // height first, width second, for both orientations.
            if p[1] <= 0.0 || p[2] > 0.0 {
                return Err(BuildError::Unsupported(
                    "standard-size ellipse codes await the §5.4 catalogues",
                ));
            }
            Kind::Ellipse {
                a: p[1] * len / 2.0,
                b: p[0] * len / 2.0,
            }
        }
        XsectShape::Custom => {
            let Some(points) = shape_curve else {
                return Err(BuildError::BadGeometry("custom shape without its curve"));
            };
            build_custom(p[0] * len, points)?
        }
        XsectShape::Arch
        | XsectShape::Egg
        | XsectShape::Horseshoe
        | XsectShape::Gothic
        | XsectShape::Catenary
        | XsectShape::SemiElliptical
        | XsectShape::BasketHandle
        | XsectShape::SemiCircular => {
            return Err(BuildError::Unsupported(
                "tabulated families await the §5.3 transcription",
            ));
        }
        XsectShape::Irregular | XsectShape::Street => {
            return Err(BuildError::Unsupported(
                "transect-backed sections await §5.6 evaluation",
            ));
        }
    };
    Ok(SectionBuild {
        section: Section::assemble(kind, p[0] * len),
        radius_raised,
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
                w_max: 0.0,
                y_at_w_max: 0.0,
                psi_max: 0.0,
                y_at_psi_max: 0.0,
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
            w_max: 0.0,
            y_at_w_max: 0.0,
            psi_max: 0.0,
            y_at_psi_max: 0.0,
        };
        s.a_full = s.area(y_full);
        // Full-depth perimeter includes any flat lid.
        s.r_full = s.a_full / (s.perimeter_open(y_full) + s.lid_width());
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
            _ => (y_full, s.top_width(y_full)),
        };
        s.w_max = ww;
        s.y_at_w_max = yw;
        // Ψ peaks below full depth for closed sections (§5.1); open
        // sections are monotone to the brim.
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

    fn is_closed(&self) -> bool {
        !matches!(
            self.kind,
            Kind::RectOpen { .. }
                | Kind::Triangle { .. }
                | Kind::Trapezoid { .. }
                | Kind::Parabola { .. }
                | Kind::Power { .. }
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

    /// Maximum top width (m) and the depth where it occurs.
    pub fn w_max(&self) -> (f64, f64) {
        (self.y_at_w_max, self.w_max)
    }

    /// The section-factor peak $\Psi_{max}$ and its depth (§5.1).
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
            Kind::Ellipse { a, b } => {
                let t = 2.0 * ((1.0 - y / b).clamp(-1.0, 1.0)).acos();
                a * b / 2.0 * (t - t.sin())
            }
            Kind::Custom { ys, ws } => {
                let mut area = 0.0;
                for i in 1..ys.len() {
                    if y <= ys[i - 1] {
                        break;
                    }
                    let y1 = ys[i].min(y);
                    let w1 = interp(ys, ws, y1);
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
            Kind::Ellipse { a, b } => {
                let t = 2.0 * ((1.0 - y / b).clamp(-1.0, 1.0)).acos();
                2.0 * a * (t / 2.0).sin()
            }
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
            Kind::Ellipse { a, b } => {
                // Arc length by quadrature on the angle parameter.
                let t_end = ((1.0 - y / b).clamp(-1.0, 1.0)).acos();
                let f = |t: f64| ((a * t.cos()).powi(2) + (b * t.sin()).powi(2)).sqrt();
                2.0 * integrate(&f, 0.0, t_end)
            }
            Kind::Custom { ys, ws } => {
                // Bottom width plus the two side slants.
                let mut p = ws[0];
                for i in 1..ys.len() {
                    if y <= ys[i - 1] {
                        break;
                    }
                    let y1 = ys[i].min(y);
                    let w1 = interp(ys, ws, y1);
                    let dy = y1 - ys[i - 1];
                    let dx = (w1 - ws[i - 1]) / 2.0;
                    p += 2.0 * (dy * dy + dx * dx).sqrt();
                }
                p
            }
        }
    }

    /// Hydraulic radius (m) at depth `y`.
    pub fn hyd_radius(&self, y: f64) -> f64 {
        let y = y.clamp(0.0, self.y_full);
        if y <= 0.0 {
            return 0.0;
        }
        if y >= self.y_full {
            return self.r_full;
        }
        let p = self.perimeter_open(y);
        if p <= 0.0 {
            0.0
        } else {
            self.area(y) / p
        }
    }

    /// The Manning section factor $\Psi(y) = A R^{2/3}$ (§5.1).
    pub fn psi(&self, y: f64) -> f64 {
        let a = self.area(y);
        a * self.hyd_radius(y).powf(2.0 / 3.0)
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
            _ => {
                let f = |y: f64| {
                    let w = self.top_width(y).max(1e-30);
                    self.area(y).powi(3) / w
                };
                if f(self.y_full) < target {
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
        Some(invert(&|y| self.psi(y), 0.0, self.y_at_psi_max, target_psi))
    }
}

/// Area of a circular segment of radius `r` filled to depth `y` from the
/// circle's lowest point.
fn segment_area(r: f64, y: f64) -> f64 {
    let t = 2.0 * ((1.0 - y / r).clamp(-1.0, 1.0)).acos();
    r * r / 2.0 * (t - t.sin())
}

/// Linear interpolation of `ws` against increasing `ys`.
fn interp(ys: &[f64], ws: &[f64], y: f64) -> f64 {
    if y <= ys[0] {
        return ws[0];
    }
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
                assert!((back - y).abs() < 1e-9, "{back} vs {y}");
            }
        }
    }

    #[test]
    fn normal_depth_reports_full_when_demand_exceeds_peak() {
        let s = build(XsectShape::Circular, [1.0, 0.0, 0.0, 0.0]);
        let (_, psi_max) = s.psi_max();
        assert_eq!(s.normal_depth(psi_max * 1.01), None);
        let y = s.normal_depth(psi_max * 0.5).unwrap();
        assert!((s.psi(y) - psi_max * 0.5).abs() < 1e-9);
    }

    #[test]
    fn infeasible_bottom_radius_is_raised() {
        let b = build_section(XsectShape::RectRound, [3.0, 2.0, 0.5, 0.0], 1.0, None).unwrap();
        assert_eq!(b.radius_raised, Some(1.0));
        // At the minimum radius the bottom is a half-circle.
        let a_bot = PI * 1.0 * 1.0 / 2.0;
        assert!((b.section.area(1.0) - a_bot).abs() < 1e-12);
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
