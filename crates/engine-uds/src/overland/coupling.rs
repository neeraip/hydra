//! §15.6: the junction-exchange orifice law and its regularisations.
//!
//! Pure functions of the two heads and the point's geometry. The
//! marcher evaluates them at tier-0 cadence; the network side reads the
//! conductance for its §6.4 damping. Everything is SI (m, m², m³/s).

/// Regularisation head (m) below which the orifice root is replaced by
/// the value-and-slope-matched quadratic (§15.6): the bare root's
/// infinite slope at equal heads is exactly the stiffness that makes
/// near-equal heads oscillate.
pub const ORIFICE_EPS: f64 = 0.02;

/// The band (m) above the rim over which the gate opens and the
/// exchange area ramps (§15.6).
pub const RIM_BAND: f64 = 0.05;

const G: f64 = 9.80665;

/// The C¹ smoothstep on \[0, 1\].
fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// $\varphi(x)$: $\sqrt x$ beyond the regularisation head, the C¹
/// quadratic below it.
pub fn orifice_phi(x: f64) -> f64 {
    if x >= ORIFICE_EPS {
        x.sqrt()
    } else {
        let inv = 1.0 / ORIFICE_EPS.sqrt();
        1.5 * inv * x - 0.5 * inv / ORIFICE_EPS * x * x
    }
}

/// $\varphi'(x)$, for the conductance.
pub fn orifice_phi_prime(x: f64) -> f64 {
    if x >= ORIFICE_EPS {
        0.5 / x.sqrt()
    } else {
        let inv = 1.0 / ORIFICE_EPS.sqrt();
        1.5 * inv - inv / ORIFICE_EPS * x
    }
}

/// The rim gate: opens C¹ over the band above the rim, evaluated at the
/// higher of the two surfaces — exchange exists only when either side
/// reaches the ground (§15.6).
pub fn rim_gate(h_max: f64, rim: f64) -> f64 {
    smoothstep((h_max - rim) / RIM_BAND)
}

/// The effective exchange area: authored below ground, ramped 1× to 2×
/// over the band above it (§15.6).
pub fn effective_area(h_max: f64, rim: f64, area: f64) -> f64 {
    if h_max < rim {
        area
    } else {
        area * (1.0 + ((h_max - rim) / RIM_BAND).min(1.0))
    }
}

/// The §15.4.3 wetting ramp on the source side's depth.
pub fn wet_ramp(depth: f64, dry_depth: f64) -> f64 {
    smoothstep(depth / dry_depth)
}

/// §15.6 junction exchange. Positive drains the surface into the node;
/// negative spills the node onto the surface.
#[allow(clippy::too_many_arguments)]
pub fn exchange_q(
    h_2d: f64,
    h_1d: f64,
    rim: f64,
    cd: f64,
    area: f64,
    depth_2d: f64,
    depth_1d: f64,
    dry_depth: f64,
) -> f64 {
    let dh = h_2d - h_1d;
    if dh.abs() < 1e-12 {
        return 0.0;
    }
    let h_max = h_2d.max(h_1d);
    let a_eff = effective_area(h_max, rim, area);
    let mut q = dh.signum() * cd * a_eff * (2.0 * G).sqrt() * orifice_phi(dh.abs());
    q *= rim_gate(h_max, rim);
    q *= if q > 0.0 {
        wet_ramp(depth_2d, dry_depth)
    } else {
        wet_ramp(depth_1d, dry_depth)
    };
    q
}

/// §15.6 exchange conductance $G = -\partial Q/\partial h_{1D} \geq 0$.
/// The gate and ramp factors are held constant — their derivatives are
/// deliberately dropped so the sign guarantee, a pure damping term on
/// the node's continuity denominator, cannot be violated.
#[allow(clippy::too_many_arguments)]
pub fn exchange_conductance(
    h_2d: f64,
    h_1d: f64,
    rim: f64,
    cd: f64,
    area: f64,
    depth_2d: f64,
    depth_1d: f64,
    dry_depth: f64,
) -> f64 {
    let h_max = h_2d.max(h_1d);
    let a_eff = effective_area(h_max, rim, area);
    let mut g = cd * a_eff * (2.0 * G).sqrt() * orifice_phi_prime((h_2d - h_1d).abs());
    g *= rim_gate(h_max, rim);
    g *= wet_ramp(depth_2d.max(depth_1d), dry_depth);
    g.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// φ meets the bare root with matched value and slope, and holds a
    /// finite slope at zero.
    #[test]
    fn the_regularised_root_is_c1() {
        let e = ORIFICE_EPS;
        assert!((orifice_phi(e) - e.sqrt()).abs() < 1e-14);
        let d = 1e-9;
        let below = (orifice_phi(e) - orifice_phi(e - d)) / d;
        let above = (orifice_phi(e + d) - orifice_phi(e)) / d;
        assert!((below / above - 1.0).abs() < 1e-4, "{below} vs {above}");
        assert_eq!(orifice_phi(0.0), 0.0);
        assert!(orifice_phi_prime(0.0).is_finite());
        // φ′ is φ's derivative on both branches.
        for x in [0.001, 0.01, 0.05, 0.5] {
            let n = (orifice_phi(x + d) - orifice_phi(x - d)) / (2.0 * d);
            assert!((orifice_phi_prime(x) - n).abs() < 1e-4, "at {x}");
        }
    }

    /// Exchange is signed by the head difference, zero at equal heads,
    /// and shut while both surfaces stand below the rim.
    #[test]
    fn the_exchange_is_signed_and_rim_gated() {
        let q = |h2, h1| exchange_q(h2, h1, 10.0, 0.65, 1.0, 1.0, 1.0, 0.001);
        assert_eq!(q(11.0, 11.0), 0.0);
        assert!(q(11.0, 10.5) > 0.0, "surface above node drains");
        assert!(q(10.5, 11.0) < 0.0, "node above surface spills");
        assert_eq!(q(9.5, 9.0), 0.0, "both below the rim: no exchange");
        // The gate is C¹: half a band above the rim it is partial.
        let half = q(10.0 + RIM_BAND / 2.0, 9.0);
        let open = q(10.0 + 2.0 * RIM_BAND, 9.0);
        assert!(half > 0.0 && half < open);
    }

    /// The exchange area ramps from the authored value to twice it over
    /// the band above the rim.
    #[test]
    fn the_area_ramps_to_double_above_ground() {
        assert_eq!(effective_area(9.0, 10.0, 0.7), 0.7);
        assert_eq!(effective_area(10.0 + RIM_BAND, 10.0, 0.7), 1.4);
        assert_eq!(effective_area(12.0, 10.0, 0.7), 1.4);
        let mid = effective_area(10.0 + RIM_BAND / 2.0, 10.0, 0.7);
        assert!((mid - 1.05).abs() < 1e-12);
    }

    /// The conductance is the head sensitivity where the gate and ramps
    /// are fully open, and never negative anywhere.
    #[test]
    fn the_conductance_is_the_drain_slope() {
        let (rim, cd, area, dd) = (10.0, 0.65, 1.0, 0.001);
        let (h2, h1) = (11.0, 10.6);
        let d = 1e-7;
        let numeric = -(exchange_q(h2, h1 + d, rim, cd, area, 1.0, 0.6, dd)
            - exchange_q(h2, h1 - d, rim, cd, area, 1.0, 0.6, dd))
            / (2.0 * d);
        let g = exchange_conductance(h2, h1, rim, cd, area, 1.0, 0.6, dd);
        assert!((g / numeric - 1.0).abs() < 1e-4, "{g} vs {numeric}");
        for (a, b) in [(9.0, 8.0), (10.0, 10.0), (10.02, 10.0), (8.0, 12.0)] {
            assert!(exchange_conductance(a, b, rim, cd, area, 0.5, 0.5, dd) >= 0.0);
        }
    }
}
