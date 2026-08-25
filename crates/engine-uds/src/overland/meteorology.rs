//! §15.7: rainfall interpolation to cell centroids.
//!
//! `NATURAL_NEIGHBOUR` builds Laplace (non-Sibsonian) natural-neighbour
//! weights over the located gauges' Delaunay triangulation, falling
//! back per cell to inverse-distance-squared outside the convex hull.
//! Degenerate gauge sets degrade exactly as the spec's ladder says: one
//! gauge serves everywhere, collinear or paired gauges serve by inverse
//! distance, and no located gauge leaves the table unready — the caller
//! then applies the `SYSTEM` mean. Weights are precomputed once.

#[derive(Debug, Clone, Copy)]
struct Pt {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Copy)]
struct Tri {
    a: usize,
    b: usize,
    c: usize,
}

fn orient2d(a: Pt, b: Pt, c: Pt) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn dist2(p: Pt, q: Pt) -> f64 {
    let (dx, dy) = (p.x - q.x, p.y - q.y);
    dx * dx + dy * dy
}

/// Circumcentre; the caller guards the collinear case.
fn circumcentre(a: Pt, b: Pt, c: Pt) -> Pt {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    let a2 = a.x * a.x + a.y * a.y;
    let b2 = b.x * b.x + b.y * b.y;
    let c2 = c.x * c.x + c.y * c.y;
    Pt {
        x: (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        y: (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    }
}

/// Strictly inside the circumcircle of CCW (a, b, c).
fn in_circle(a: Pt, b: Pt, c: Pt, p: Pt) -> bool {
    let (ax, ay) = (a.x - p.x, a.y - p.y);
    let (bx, by) = (b.x - p.x, b.y - p.y);
    let (cx, cy) = (c.x - p.x, c.y - p.y);
    (ax * ax + ay * ay) * (bx * cy - cx * by) - (bx * bx + by * by) * (ax * cy - cx * ay)
        + (cx * cx + cy * cy) * (ax * by - bx * ay)
        > 0.0
}

/// Boundary directed edges of a cavity: an edge is on the boundary when
/// its reverse is not also a cavity edge. Cavities are tiny; O(n²) is
/// fine.
fn cavity_boundary(bad: &[[usize; 2]]) -> Vec<[usize; 2]> {
    bad.iter()
        .filter(|e| !bad.iter().any(|f| f[0] == e[1] && f[1] == e[0]))
        .copied()
        .collect()
}

/// Bowyer–Watson Delaunay triangulation, or empty when the sites are
/// degenerate (coincident or collinear) — the caller falls back to IDW.
fn delaunay(site: &[Pt]) -> Vec<Tri> {
    let m = site.len();
    if m < 3 {
        return Vec::new();
    }
    let (mut minx, mut maxx, mut miny, mut maxy) = (site[0].x, site[0].x, site[0].y, site[0].y);
    for s in site {
        minx = minx.min(s.x);
        maxx = maxx.max(s.x);
        miny = miny.min(s.y);
        maxy = maxy.max(s.y);
    }
    let dmax = (maxx - minx).max(maxy - miny);
    if dmax <= 0.0 {
        return Vec::new();
    }
    let mut pts: Vec<Pt> = site.to_vec();
    let (midx, midy) = (0.5 * (minx + maxx), 0.5 * (miny + maxy));
    let r = 20.0 * dmax;
    pts.push(Pt {
        x: midx - 2.0 * r,
        y: midy - r,
    });
    pts.push(Pt {
        x: midx + 2.0 * r,
        y: midy - r,
    });
    pts.push(Pt {
        x: midx,
        y: midy + 2.0 * r,
    });
    let ccw = |a: usize, b: usize, c: usize, pts: &[Pt]| {
        if orient2d(pts[a], pts[b], pts[c]) < 0.0 {
            Tri { a, b: c, c: b }
        } else {
            Tri { a, b, c }
        }
    };
    let mut tris = vec![ccw(m, m + 1, m + 2, &pts)];
    for i in 0..m {
        let p = pts[i];
        let mut bad: Vec<[usize; 2]> = Vec::new();
        let mut good: Vec<Tri> = Vec::with_capacity(tris.len());
        for t in &tris {
            if in_circle(pts[t.a], pts[t.b], pts[t.c], p) {
                bad.push([t.a, t.b]);
                bad.push([t.b, t.c]);
                bad.push([t.c, t.a]);
            } else {
                good.push(*t);
            }
        }
        tris = good;
        for e in cavity_boundary(&bad) {
            tris.push(ccw(e[0], e[1], i, &pts));
        }
    }
    tris.retain(|t| t.a < m && t.b < m && t.c < m);
    tris
}

/// Inverse-distance-squared weights over every located site.
fn idw_all(p: Pt, site: &[Pt]) -> Vec<(usize, f64)> {
    for (j, s) in site.iter().enumerate() {
        if dist2(p, *s) < 1e-18 {
            return vec![(j, 1.0)];
        }
    }
    let mut w: Vec<(usize, f64)> = site
        .iter()
        .enumerate()
        .map(|(j, s)| (j, 1.0 / dist2(p, *s)))
        .collect();
    let sum: f64 = w.iter().map(|e| e.1).sum();
    for e in &mut w {
        e.1 /= sum;
    }
    w
}

/// Laplace weights at `p`, or empty when `p` lies outside the convex
/// hull or the construction degenerates — the caller falls back to IDW.
/// Inserting `p` carves a cavity in the triangulation; the Voronoi
/// facet between `p` and a cavity-boundary site is bounded by the
/// circumcentres of the two fan triangles incident to that site, and
/// the Laplace weight is facet length over site distance.
fn laplace_weights(p: Pt, site: &[Pt], tris: &[Tri]) -> Vec<(usize, f64)> {
    for (j, s) in site.iter().enumerate() {
        if dist2(p, *s) < 1e-18 {
            return vec![(j, 1.0)];
        }
    }
    let inside = tris.iter().any(|t| {
        let d0 = orient2d(site[t.a], site[t.b], p);
        let d1 = orient2d(site[t.b], site[t.c], p);
        let d2 = orient2d(site[t.c], site[t.a], p);
        !((d0 < 0.0 || d1 < 0.0 || d2 < 0.0) && (d0 > 0.0 || d1 > 0.0 || d2 > 0.0))
    });
    if !inside {
        return Vec::new();
    }
    let mut bad: Vec<[usize; 2]> = Vec::new();
    for t in tris {
        if in_circle(site[t.a], site[t.b], site[t.c], p) {
            bad.push([t.a, t.b]);
            bad.push([t.b, t.c]);
            bad.push([t.c, t.a]);
        }
    }
    let boundary = cavity_boundary(&bad);
    if boundary.is_empty() {
        return Vec::new();
    }
    // Circumcentre of each fan triangle (p, u, v), indexed by tail and
    // head site.
    let mut by_tail: Vec<Option<Pt>> = vec![None; site.len()];
    let mut by_head: Vec<Option<Pt>> = vec![None; site.len()];
    for e in &boundary {
        let (u, v) = (site[e[0]], site[e[1]]);
        if orient2d(p, u, v).abs() < 1e-20 {
            return Vec::new();
        }
        let cc = circumcentre(p, u, v);
        by_tail[e[0]] = Some(cc);
        by_head[e[1]] = Some(cc);
    }
    let mut w: Vec<(usize, f64)> = Vec::with_capacity(boundary.len());
    let mut sum = 0.0;
    for e in &boundary {
        let s = e[1];
        let (Some(cin), Some(cout)) = (by_head[s], by_tail[s]) else {
            return Vec::new();
        };
        let facet = dist2(cin, cout).sqrt();
        let wj = facet / dist2(p, site[s]).sqrt();
        w.push((s, wj));
        sum += wj;
    }
    if sum <= 0.0 {
        return Vec::new();
    }
    for e in &mut w {
        e.1 /= sum;
    }
    w
}

/// Precomputed per-cell gauge weights (CSR), gauge indices as the model
/// numbers them.
#[derive(Debug, Clone, Default)]
pub struct RainWeights {
    ptr: Vec<u32>,
    gage: Vec<u16>,
    val: Vec<f64>,
    ready: bool,
}

impl RainWeights {
    /// Build the table from cell centroids and the model's gauge
    /// positions (`None` = unlocated, excluded). Duplicate positions
    /// keep only the first — natural neighbour is ill-defined for two
    /// gauges at one point. With no located gauge the table stays
    /// unready and the caller applies the `SYSTEM` mean.
    pub fn build(cx: &[f64], cy: &[f64], gauges: &[Option<(f64, f64)>]) -> RainWeights {
        let mut site: Vec<Pt> = Vec::new();
        let mut gid: Vec<usize> = Vec::new();
        for (g, pos) in gauges.iter().enumerate() {
            let Some((x, y)) = *pos else { continue };
            let s = Pt { x, y };
            if site.iter().any(|e| dist2(*e, s) < 1e-12) {
                continue;
            }
            site.push(s);
            gid.push(g);
        }
        let nc = cx.len();
        if site.is_empty() || nc == 0 {
            return RainWeights::default();
        }
        let rows: Vec<Vec<(usize, f64)>> = if site.len() == 1 {
            vec![vec![(0, 1.0)]; nc]
        } else {
            let tris = delaunay(&site);
            (0..nc)
                .map(|i| {
                    let p = Pt { x: cx[i], y: cy[i] };
                    if tris.is_empty() {
                        idw_all(p, &site)
                    } else {
                        let w = laplace_weights(p, &site, &tris);
                        if w.is_empty() {
                            idw_all(p, &site)
                        } else {
                            w
                        }
                    }
                })
                .collect()
        };
        let mut t = RainWeights {
            ptr: Vec::with_capacity(nc + 1),
            gage: Vec::new(),
            val: Vec::new(),
            ready: true,
        };
        for row in rows {
            t.ptr.push(t.gage.len() as u32);
            for (j, w) in row {
                t.gage.push(gid[j] as u16);
                t.val.push(w);
            }
        }
        t.ptr.push(t.gage.len() as u32);
        t
    }

    /// Whether the table can serve; unready means no gauge is located.
    pub fn ready(&self) -> bool {
        self.ready
    }

    /// Interpolate per-gauge intensities to per-cell rates.
    pub fn apply(&self, gauge_rain: &[f64], out: &mut [f64]) {
        for (i, o) in out.iter_mut().enumerate() {
            let (lo, hi) = (self.ptr[i] as usize, self.ptr[i + 1] as usize);
            *o = (lo..hi)
                .map(|k| self.val[k] * gauge_rain[self.gage[k] as usize])
                .sum();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_at(t: &RainWeights, rain: &[f64], n: usize) -> Vec<f64> {
        let mut out = vec![0.0; n];
        t.apply(rain, &mut out);
        out
    }

    /// Laplace natural-neighbour weights reproduce a linear field
    /// exactly inside the gauge hull.
    #[test]
    fn linear_fields_are_exact_inside_the_hull() {
        let gauges = [
            Some((0.0, 0.0)),
            Some((10.0, 0.5)),
            Some((9.5, 9.0)),
            Some((-0.5, 10.0)),
            Some((5.0, 4.0)),
        ];
        let f = |x: f64, y: f64| 3.0 + 0.7 * x - 0.4 * y;
        let rain: Vec<f64> = gauges
            .iter()
            .map(|g| g.map(|(x, y)| f(x, y)).unwrap_or(0.0))
            .collect();
        let cx = [2.0, 5.0, 7.0, 3.3, 6.1];
        let cy = [3.0, 5.0, 2.0, 6.7, 4.9];
        let t = RainWeights::build(&cx, &cy, &gauges);
        assert!(t.ready());
        let out = apply_at(&t, &rain, cx.len());
        for i in 0..cx.len() {
            let exact = f(cx[i], cy[i]);
            assert!(
                (out[i] - exact).abs() < 1e-9,
                "cell {i}: {} vs {exact}",
                out[i]
            );
        }
    }

    /// The degradation ladder: one gauge serves everywhere; two serve
    /// by inverse distance; collinear sets serve by inverse distance;
    /// no located gauge leaves the table unready.
    #[test]
    fn degenerate_gauge_sets_degrade_as_specified() {
        let cx = [0.0, 3.0];
        let cy = [0.0, 4.0];
        let one = RainWeights::build(&cx, &cy, &[None, Some((50.0, 50.0))]);
        assert!(one.ready());
        assert_eq!(apply_at(&one, &[0.0, 7.0], 2), vec![7.0, 7.0]);

        let two = RainWeights::build(&cx, &cy, &[Some((0.0, 0.0)), Some((0.0, 8.0))]);
        // Cell 0 sits on the first gauge: exact hit. Cell 1 is 5 m from
        // both: even split.
        let out = apply_at(&two, &[2.0, 6.0], 2);
        assert!((out[0] - 2.0).abs() < 1e-12);
        assert!((out[1] - 4.0).abs() < 1e-12);

        let collinear = RainWeights::build(
            &[1.0],
            &[1.0],
            &[Some((0.0, 0.0)), Some((5.0, 0.0)), Some((10.0, 0.0))],
        );
        assert!(collinear.ready());
        let out = apply_at(&collinear, &[1.0, 1.0, 1.0], 1);
        assert!((out[0] - 1.0).abs() < 1e-12, "IDW of a uniform field");

        let none = RainWeights::build(&cx, &cy, &[None, None]);
        assert!(!none.ready());
    }

    /// Outside the hull the weights fall back to inverse distance
    /// squared: positive, normalised, nearest-dominated.
    #[test]
    fn outside_the_hull_is_inverse_distance() {
        let gauges = [Some((0.0, 0.0)), Some((10.0, 0.0)), Some((5.0, 8.0))];
        let t = RainWeights::build(&[5.0, -10.0], &[3.0, 0.0], &gauges);
        // Interior cell: natural-neighbour, exact for the constant.
        // Exterior cell at (-10, 0): closest to gauge 0.
        let out = apply_at(&t, &[1.0, 1.0, 1.0], 2);
        assert!((out[0] - 1.0).abs() < 1e-12);
        assert!((out[1] - 1.0).abs() < 1e-12, "weights normalise");
        let spike = apply_at(&t, &[1.0, 0.0, 0.0], 2);
        assert!(spike[1] > 0.5, "the nearest gauge dominates outside");
    }
}
