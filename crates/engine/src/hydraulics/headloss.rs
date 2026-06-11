use crate::HeadLossFormula;

/// Gravitational acceleration (m/s²) used in the Darcy-Weisbach resistance
/// formula. EPANET uses 32.2 ft/s²; the SI equivalent rounded to match the
/// same relative precision is 9.81 m/s².
pub const G_DW: f64 = 9.81;

// Hazen-Williams (§3.2.1)
const ALPHA_HW: f64 = 10.67;
pub(super) const HW_EXP: f64 = 1.852;

// Chezy-Manning (§3.2.3)
const K_CM: f64 = 1.0;

// Darcy-Weisbach constants (§3.2.2) — matched to EPANET hydcoeffs.c.
const DW_A1: f64 = 3.141_592_653_589_793e3;
const DW_A2: f64 = 1.570_796_326_794_897e3;
const DW_A8: f64 = 4.618_413_198_590_667;
const DW_A9: f64 = -8.685_889_638_065_037e-1;
const DW_AB: f64 = 3.288_954_763_453_991e-3;
const DW_AC: f64 = -5.142_149_657_990_939e-3;

/// Hazen-Williams resistance R_HW (§3.2.1, SI).
pub(super) fn hw_resistance(length: f64, roughness: f64, diameter: f64) -> f64 {
    ALPHA_HW * length / (roughness.powf(HW_EXP) * diameter.powf(4.871))
}

/// Chezy-Manning resistance R_CM (§3.2.3, SI).
pub(super) fn cm_resistance(length: f64, roughness: f64, diameter: f64) -> f64 {
    let area = std::f64::consts::PI * diameter * diameter / 4.0;
    let r_h = diameter / 4.0;
    roughness * roughness * length / (K_CM * K_CM * r_h.powf(4.0 / 3.0) * area * area)
}

/// Darcy-Weisbach base resistance R = L/(2g*D*A^2) = 8L/(pi^2*g*D^5) (§3.2.2).
pub(super) fn dw_resistance(length: f64, diameter: f64) -> f64 {
    let area = std::f64::consts::PI * diameter * diameter / 4.0;
    length / (2.0 * G_DW * diameter * area * area)
}

/// Pre-computed per-pipe static resistance coefficient (formula-dependent).
pub(super) fn pipe_resistance(
    length: f64,
    diameter: f64,
    roughness: f64,
    formula: HeadLossFormula,
) -> f64 {
    match formula {
        HeadLossFormula::HazenWilliams => hw_resistance(length, roughness, diameter),
        HeadLossFormula::ChezyManning => cm_resistance(length, roughness, diameter),
        HeadLossFormula::DarcyWeisbach => dw_resistance(length, diameter),
    }
}

/// Returns `(h_f, g_f)` — signed friction head loss and gradient — for HW (§3.2.1).
pub(super) fn hw_headloss_grad(r: f64, q: f64) -> (f64, f64) {
    let abs_q = q.abs();
    if abs_q == 0.0 {
        return (0.0, 0.0);
    }
    let q_exp = abs_q.powf(HW_EXP - 1.0);
    let h = r * q_exp * abs_q * q.signum();
    let g = r * HW_EXP * q_exp;
    (h, g)
}

/// Returns `(h_f, g_f)` for Chezy-Manning (§3.2.3).
pub(super) fn cm_headloss_grad(r: f64, q: f64) -> (f64, f64) {
    let abs_q = q.abs();
    let h = r * q * abs_q;
    let g = 2.0 * r * abs_q;
    (h, g)
}

/// Returns `(h_f, g_f)` for Darcy-Weisbach (§3.2.2).
pub(super) fn dw_headloss_grad(
    r: f64,
    q: f64,
    roughness: f64,
    diameter: f64,
    s: f64,
) -> (f64, f64) {
    let abs_q = q.abs();
    if abs_q <= DW_A2 * s {
        let r_lam = 16.0 * std::f64::consts::PI * s * r;
        return (r_lam * q, r_lam);
    }
    let e = roughness / diameter;
    let mut dfdq = 0.0f64;
    let f = friction_factor(abs_q, e, s, &mut dfdq);
    let r1 = f * r;
    let h = r1 * q * abs_q;
    let g = 2.0 * r1 * abs_q + dfdq * r * abs_q * abs_q;
    (h, g)
}

/// Darcy friction factor f and df/d|Q| (§3.2.2).
pub(super) fn friction_factor(q: f64, e: f64, s: f64, dfdq: &mut f64) -> f64 {
    let w = q / s;
    if w >= DW_A1 {
        let y1 = DW_A8 / w.powf(0.9);
        let y2 = e / 3.7 + y1;
        let y3 = DW_A9 * y2.ln();
        let f = 1.0 / (y3 * y3);
        *dfdq = 1.8 * f * y1 * DW_A9 / (y2 * y3 * q);
        f
    } else {
        let y2 = e / 3.7 + DW_AB;
        let y3 = DW_A9 * y2.ln();
        let fa = 1.0 / (y3 * y3);
        let fb = (2.0 + DW_AC / (y2 * y3)) * fa;
        let r = w / DW_A2;
        let x2 = 0.128 - 17.0 * fa + 2.5 * fb;
        let x3 = -0.128 + 13.0 * fa - 2.0 * fb;
        let x4 = 0.032 - 3.0 * fa + 0.5 * fb;
        let x1 = 7.0 * fa - fb;
        let f = x1 + r * (x2 + r * (x3 + r * x4));
        *dfdq = (x2 + r * (2.0 * x3 + r * 3.0 * x4)) / (s * DW_A2);
        f
    }
}

/// Computes (h, g) for total pipe head loss including minor losses (§3.2.1–3.2.4).
pub(super) fn pipe_total_hg(
    r: f64,
    q: f64,
    roughness: f64,
    diameter: f64,
    minor_loss: f64,
    formula: HeadLossFormula,
    viscosity: f64,
) -> (f64, f64) {
    let (h_f, g_f) = match formula {
        HeadLossFormula::HazenWilliams => hw_headloss_grad(r, q),
        HeadLossFormula::ChezyManning => cm_headloss_grad(r, q),
        HeadLossFormula::DarcyWeisbach => {
            let s = viscosity * diameter;
            dw_headloss_grad(r, q, roughness, diameter, s)
        }
    };
    let abs_q = q.abs();
    let h_m = minor_loss * q * abs_q;
    let g_m = 2.0 * minor_loss * abs_q;
    (h_f + h_m, g_f + g_m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_resistance_dispatches_by_formula() {
        let length = 1000.0;
        let diameter = 1.0;
        let roughness = 100.0;
        assert_eq!(
            pipe_resistance(length, diameter, roughness, HeadLossFormula::HazenWilliams),
            hw_resistance(length, roughness, diameter)
        );
        assert_eq!(
            pipe_resistance(length, diameter, roughness, HeadLossFormula::ChezyManning),
            cm_resistance(length, roughness, diameter)
        );
        assert_eq!(
            pipe_resistance(length, diameter, roughness, HeadLossFormula::DarcyWeisbach),
            dw_resistance(length, diameter)
        );
    }

    #[test]
    fn friction_factor_transitional_regime_is_positive() {
        let mut dfdq = 0.0;
        let f = friction_factor(1.2, 0.0001, 0.0004, &mut dfdq);
        assert!(f.is_finite() && f > 0.0);
        assert!(dfdq.is_finite());
    }

    #[test]
    fn pipe_total_hg_adds_minor_losses() {
        let r = hw_resistance(500.0, 100.0, 1.0);
        let q = 2.0;
        let (h0, g0) = pipe_total_hg(r, q, 100.0, 1.0, 0.0, HeadLossFormula::HazenWilliams, 0.0);
        let (h1, g1) = pipe_total_hg(r, q, 100.0, 1.0, 0.5, HeadLossFormula::HazenWilliams, 0.0);
        assert!(h1 > h0);
        assert!(g1 > g0);
    }
}
