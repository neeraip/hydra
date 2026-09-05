//! §15.3: the stage–storage closures and face-depth reconstructions.
//!
//! Everything here is a pure function of a cell's geometry and its mean
//! depth or stage — the marcher (§15.4) evaluates these, never integrates
//! them. The VFR relations are Begnudelli & Sanders' planar-bed
//! stage–storage, regularised by a wet-fraction floor so a drying cell's
//! closure stays $C^1$ and monotone.

/// Relief below which a cell's bed is flat for every purpose: the VFR
/// branches divide by vertex-elevation differences, and a cell this flat
/// is the flat closure to more precision than the division would keep.
const FLAT_RELIEF: f64 = 1e-9;

/// A cell's sorted vertex elevations, the VFR closure's whole geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellBed {
    /// Sorted: `z1 <= z2 <= z3` (m).
    pub z1: f64,
    pub z2: f64,
    pub z3: f64,
}

impl CellBed {
    pub fn new(a: f64, b: f64, c: f64) -> CellBed {
        let mut z = [a, b, c];
        z.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        CellBed {
            z1: z[0],
            z2: z[1],
            z3: z[2],
        }
    }

    /// The centroid bed $\bar z$, the flat closure's datum.
    pub fn mean(&self) -> f64 {
        (self.z1 + self.z2 + self.z3) / 3.0
    }

    fn relief(&self) -> f64 {
        self.z3 - self.z1
    }
}

/// §15.3 FLAT: $\eta = \bar z + \bar h$.
pub fn flat_eta(bed: &CellBed, h_mean: f64) -> f64 {
    bed.mean() + h_mean
}

/// §15.3 VFR: mean depth at stage $\eta$, the exact planar-bed relation.
/// The unregularised form; [`vfr_eta`] and its inverse apply the
/// $\varepsilon$ floor.
pub fn vfr_mean_depth(bed: &CellBed, eta: f64) -> f64 {
    if bed.relief() < FLAT_RELIEF {
        return (eta - bed.mean()).max(0.0);
    }
    let CellBed { z1, z2, z3 } = *bed;
    if eta <= z1 {
        0.0
    } else if eta <= z2 {
        let d = eta - z1;
        d * d * d / (3.0 * (z2 - z1).max(FLAT_RELIEF) * (z3 - z1))
    } else if eta <= z3 {
        let d = z3 - eta;
        (eta - bed.mean()) + d * d * d / (3.0 * (z3 - z1) * (z3 - z2).max(FLAT_RELIEF))
    } else {
        eta - bed.mean()
    }
}

/// §15.3 VFR: the wet-area fraction $\mathrm{d}\bar h/\mathrm{d}\eta$ at
/// stage $\eta$.
pub fn vfr_wet_fraction(bed: &CellBed, eta: f64) -> f64 {
    if bed.relief() < FLAT_RELIEF {
        return 1.0;
    }
    let CellBed { z1, z2, z3 } = *bed;
    if eta <= z1 {
        0.0
    } else if eta <= z2 {
        let d = eta - z1;
        d * d / ((z2 - z1).max(FLAT_RELIEF) * (z3 - z1))
    } else if eta <= z3 {
        let d = z3 - eta;
        1.0 - d * d / ((z3 - z1) * (z3 - z2).max(FLAT_RELIEF))
    } else {
        1.0
    }
}

/// The stage at which the wet fraction reaches `eps` — where the
/// regularised relation departs the exact one.
fn vfr_eps_stage(bed: &CellBed, eps: f64) -> f64 {
    let CellBed { z1, z2, z3 } = *bed;
    let lower_cap = (z2 - z1).max(FLAT_RELIEF) / (z3 - z1);
    if eps <= lower_cap {
        z1 + (eps * (z2 - z1).max(FLAT_RELIEF) * (z3 - z1)).sqrt()
    } else {
        z3 - ((1.0 - eps) * (z3 - z1) * (z3 - z2).max(FLAT_RELIEF)).sqrt()
    }
}

/// §15.3 VFR, regularised: stage from mean depth. Above the
/// $\varepsilon$-stage the exact inverse (closed-form cube root on the
/// lower branch, safeguarded bisection on the middle one); below it the
/// tangent continuation with slope $1/\varepsilon$ in $\eta(\bar h)$,
/// keeping the closure $C^1$ and monotone as the cell dries.
pub fn vfr_eta(bed: &CellBed, h_mean: f64, eps: f64) -> f64 {
    if bed.relief() < FLAT_RELIEF {
        return bed.mean() + h_mean;
    }
    let CellBed { z1, z2, z3 } = *bed;
    let eta_s = vfr_eps_stage(bed, eps);
    let h_s = vfr_mean_depth(bed, eta_s);
    if h_mean <= h_s {
        // Tangent continuation: dh̄/dη = ε below the ε-stage.
        return eta_s - (h_s - h_mean) / eps.max(f64::MIN_POSITIVE);
    }
    if h_mean >= vfr_mean_depth(bed, z3) {
        // Fully wet: exactly the flat closure.
        return bed.mean() + h_mean;
    }
    if h_mean <= vfr_mean_depth(bed, z2) {
        // Lower branch: closed-form cube root.
        return z1 + (3.0 * h_mean * (z2 - z1).max(FLAT_RELIEF) * (z3 - z1)).cbrt();
    }
    // Middle branch: monotone in η on [z2, z3]; bisection to round-off.
    let (mut lo, mut hi) = (z2, z3);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if vfr_mean_depth(bed, mid) < h_mean {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-14 * (1.0 + bed.relief()) {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// §15.3 face depths. `MEAN`: the higher water surface over the face
/// bed.
pub fn face_depth_mean(eta_l: f64, eta_r: f64, z_face: f64) -> f64 {
    eta_l.max(eta_r) - z_face
}

/// §15.3 `VFR_FACE`: the wetted-edge mean depth of the higher surface
/// over the edge's own endpoint beds — $C^1$ at both joins.
pub fn face_depth_vfr(eta_l: f64, eta_r: f64, z_lo: f64, z_hi: f64) -> f64 {
    let eta = eta_l.max(eta_r);
    if eta <= z_lo {
        0.0
    } else if z_hi - z_lo < FLAT_RELIEF {
        eta - z_lo
    } else if eta <= z_hi {
        let d = eta - z_lo;
        d * d / (2.0 * (z_hi - z_lo))
    } else {
        eta - 0.5 * (z_lo + z_hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bed() -> CellBed {
        CellBed::new(10.4, 10.0, 10.2)
    }

    /// The relation's shape: zero at the low corner, the flat closure
    /// when fully wet, continuous and strictly increasing between.
    #[test]
    fn vfr_runs_from_dry_corner_to_flat_closure() {
        let b = bed();
        assert_eq!(vfr_mean_depth(&b, 10.0), 0.0);
        assert!((vfr_mean_depth(&b, 10.4) - (10.4 - b.mean())).abs() < 1e-12);
        assert!((vfr_mean_depth(&b, 11.0) - (11.0 - b.mean())).abs() < 1e-12);
        // Branch joins are continuous.
        for z in [10.2, 10.4] {
            let below = vfr_mean_depth(&b, z - 1e-9);
            let above = vfr_mean_depth(&b, z + 1e-9);
            assert!((above - below).abs() < 1e-8, "join at {z}");
        }
        // Strictly increasing.
        let mut last = -1.0;
        for i in 0..100 {
            let eta = 10.0 + 0.005 * f64::from(i);
            let h = vfr_mean_depth(&b, eta);
            assert!(h >= last);
            last = h;
        }
    }

    /// The wet fraction is the relation's derivative, to first order.
    #[test]
    fn the_wet_fraction_is_the_slope() {
        let b = bed();
        for eta in [10.05, 10.15, 10.25, 10.35] {
            let d = 1e-7;
            let numeric = (vfr_mean_depth(&b, eta + d) - vfr_mean_depth(&b, eta - d)) / (2.0 * d);
            assert!(
                (vfr_wet_fraction(&b, eta) - numeric).abs() < 1e-5,
                "at {eta}: {} vs {numeric}",
                vfr_wet_fraction(&b, eta)
            );
        }
    }

    /// Stage and depth are exact inverses above the ε-stage, and the
    /// tangent continuation round-trips below it.
    #[test]
    fn eta_and_depth_round_trip() {
        let b = bed();
        let eps = 0.01;
        for h in [1e-6, 1e-4, 0.01, 0.05, 0.1, 0.2, 0.5, 2.0] {
            let eta = vfr_eta(&b, h, eps);
            let back = if vfr_wet_fraction(&b, eta) >= eps {
                vfr_mean_depth(&b, eta)
            } else {
                // Below the ε-stage the forward relation is the tangent.
                let eta_s = vfr_eps_stage(&b, eps);
                vfr_mean_depth(&b, eta_s) - (eta_s - eta) * eps
            };
            assert!(
                (back - h).abs() < 1e-10 * (1.0 + h),
                "h={h}: eta={eta} back={back}"
            );
        }
    }

    /// The regularised closure is monotone through the ε-stage and never
    /// puts a wet surface below the low corner by more than the tangent's
    /// reach.
    #[test]
    fn the_regularisation_is_monotone_and_c1() {
        let b = bed();
        let eps = 0.01;
        let mut last = f64::NEG_INFINITY;
        for i in 0..2000 {
            let h = 1e-8 * f64::from(i) * f64::from(i);
            let eta = vfr_eta(&b, h, eps);
            assert!(eta >= last, "monotone at h={h}");
            last = eta;
        }
        // Slope continuity at the ε-stage: 1/ε on both sides.
        let eta_s = vfr_eps_stage(&b, eps);
        let h_s = vfr_mean_depth(&b, eta_s);
        let d = 1e-9;
        let below = (vfr_eta(&b, h_s, eps) - vfr_eta(&b, h_s - d, eps)) / d;
        let above = (vfr_eta(&b, h_s + d, eps) - vfr_eta(&b, h_s, eps)) / d;
        assert!(
            (below / above - 1.0).abs() < 0.05,
            "C1 at the ε-stage: {below} vs {above}"
        );
    }

    /// A flat cell is the flat closure in both directions.
    #[test]
    fn a_flat_cell_is_flat() {
        let b = CellBed::new(5.0, 5.0, 5.0);
        assert!((vfr_mean_depth(&b, 5.3) - 0.3).abs() < 1e-12);
        assert!((vfr_eta(&b, 0.3, 0.01) - 5.3).abs() < 1e-12);
        assert_eq!(vfr_wet_fraction(&b, 5.3), 1.0);
    }

    /// The VFR face depth is C¹ at both joins and linear above the high
    /// end.
    #[test]
    fn the_face_depth_joins_smoothly() {
        let (z_lo, z_hi) = (10.0, 10.4);
        assert_eq!(face_depth_vfr(9.9, 9.5, z_lo, z_hi), 0.0);
        let mid = face_depth_vfr(10.2, 0.0, z_lo, z_hi);
        assert!((mid - 0.2 * 0.2 / 0.8).abs() < 1e-12);
        let above = face_depth_vfr(11.0, 0.0, z_lo, z_hi);
        assert!((above - (11.0 - 10.2)).abs() < 1e-12);
        for z in [z_lo, z_hi] {
            let below = face_depth_vfr(z - 1e-9, 0.0, z_lo, z_hi);
            let over = face_depth_vfr(z + 1e-9, 0.0, z_lo, z_hi);
            assert!((over - below).abs() < 1e-8, "join at {z}");
        }
    }
}

#[cfg(test)]
mod spec_constant_tests {
    /// §15.3: the relief below which a cell takes the flat closure.
    #[test]
    fn the_flat_relief_threshold_is_the_value_the_spec_fixes() {
        assert_eq!(1e-9, super::FLAT_RELIEF);
    }
}
