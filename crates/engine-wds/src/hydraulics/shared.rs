/// Large conductance constant C∞ (§3.3, §3.5). Same as EPANET's CBIG.
pub(super) const C_INF: f64 = 1.0e8;

/// Minimum head-loss gradient g_min (§3.3). EPANET: CSMALL = 1e-6.
pub(super) const G_MIN: f64 = 1.0e-6;

/// Placeholder flow for closed links at initialisation (§3.10).
pub(super) const Q_CLOSED: f64 = 1.0e-6;

/// Specific weight of water, ρg (N/m³). Used for constant-HP pump.
pub(super) const GAMMA_WATER: f64 = 9810.0;

/// Power-function pump head-curve coefficients (§3.2.5).
///
/// For a pump at relative speed ω the head gain is:
///   ΔH = ω² H₀ − r ω^(2−N) Q^N
#[derive(Debug, Clone)]
pub struct PumpCoeffs {
    /// Shutoff head H₀ (m).
    pub h0: f64,
    /// Resistance coefficient r.
    pub r: f64,
    /// Flow exponent N.
    pub n: f64,
}

impl PumpCoeffs {
    /// Fits coefficients from three head-curve points `(0, h_shutoff)`,
    /// `(q1, h1)`, `(q2, h2)` using the three-point formula (§3.2.5).
    ///
    /// Returns `None` when the constraints `h_shutoff > h1 > h2 > 0`,
    /// `q2 > q1 > 0`, and `0 < N ≤ 20` are not all satisfied.
    pub fn from_three_points(h_shutoff: f64, q1: f64, h1: f64, q2: f64, h2: f64) -> Option<Self> {
        if h_shutoff <= h1 || h1 <= h2 || h2 < 0.0 {
            return None;
        }
        if q1 <= 0.0 || q2 <= q1 {
            return None;
        }
        let n = ((h_shutoff - h2) / (h_shutoff - h1)).ln() / (q2 / q1).ln();
        if n <= 0.0 || n > 20.0 {
            return None;
        }
        let r = (h_shutoff - h1) / q1.powf(n);
        if r <= 0.0 {
            return None;
        }
        Some(PumpCoeffs {
            h0: h_shutoff,
            r,
            n,
        })
    }
}

/// Lower barrier: adds (Δh, Δg) to enforce q ≥ q₀ when called with δq = q − q₀.
pub(super) fn lower_barrier(dq: f64) -> (f64, f64) {
    let a = 1.0e9 * dq;
    let b = (a * a + 1.0e-6_f64).sqrt();
    ((a - b) / 2.0, 5.0e8 * (1.0 - a / b))
}

/// Upper barrier: adds (Δh, Δg) to enforce q ≤ q₁ when called with δq = q − q₁.
pub(super) fn upper_barrier(dq: f64) -> (f64, f64) {
    let a = 1.0e9 * dq;
    let b = (a * a + 1.0e-6_f64).sqrt();
    ((a + b) / 2.0, 5.0e8 * (1.0 + a / b))
}

/// Per-link P/Y coefficient pair (§3.3).
pub(super) struct Py {
    pub(super) p: f64,
    pub(super) y: f64,
}

impl Py {
    pub(super) fn closed(q: f64) -> Self {
        Py {
            p: 1.0 / C_INF,
            y: q,
        }
    }

    pub(super) fn from_hg(h: f64, g: f64, n_f: f64, q: f64) -> Self {
        if g < G_MIN {
            let g2 = G_MIN / n_f;
            let h2 = g2 * q;
            Py {
                p: 1.0 / g2,
                y: h2 / g2,
            }
        } else {
            Py {
                p: 1.0 / g,
                y: h / g,
            }
        }
    }
}

/// Outcome of a completed hydraulic solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveResult {
    /// All convergence criteria satisfied.
    Converged,
    /// `max_iter` reached; extra iterations run with frozen status. Results are
    /// valid but potentially unbalanced.
    Unbalanced,
}

/// Hydraulic solver error.
#[derive(Debug, Clone)]
pub enum HydraulicError {
    /// Newton loop did not converge and `extra_iter = -1` (halt-on-failure mode).
    NotConverged,
    /// Sparse matrix numerically singular at the given permuted junction step.
    SingularMatrix {
        /// Zero-based permuted-junction index at which the singular pivot was detected.
        junction_step: usize,
    },
    /// A pump that requires power-function coefficients has none fitted.
    NoPumpCoeffs {
        /// ID of the pump whose power-function coefficients could not be computed.
        pump_id: String,
    },
}

impl std::fmt::Display for HydraulicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConverged => write!(f, "hydraulic solve did not converge"),
            Self::SingularMatrix { junction_step } => {
                write!(
                    f,
                    "sparse matrix singular at permuted junction step {junction_step}"
                )
            }
            Self::NoPumpCoeffs { pump_id } => {
                write!(
                    f,
                    "pump '{pump_id}' requires power-function coefficients but none were fitted"
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{lower_barrier, upper_barrier, PumpCoeffs};

    #[test]
    fn pump_coeffs_three_point_fit() {
        let c = PumpCoeffs::from_three_points(100.0, 1.0, 90.0, 2.0, 60.0).unwrap();
        assert!((c.n - 2.0).abs() < 1.0e-10, "N={}", c.n);
        assert!((c.r - 10.0).abs() < 1.0e-10, "r={}", c.r);
        assert!((c.h0 - 100.0).abs() < 1.0e-10);
    }

    #[test]
    fn pump_coeffs_rejects_bad_points() {
        assert!(PumpCoeffs::from_three_points(90.0, 1.0, 90.0, 2.0, 60.0).is_none());
        assert!(PumpCoeffs::from_three_points(100.0, 0.0, 90.0, 2.0, 60.0).is_none());
        assert!(PumpCoeffs::from_three_points(100.0, 1.0, 99.99999, 2.0, 0.001).is_none());
    }

    #[test]
    fn lower_barrier_feasible_near_zero() {
        let (dh, _) = lower_barrier(1.0);
        assert!(dh.abs() < 1.0e-3, "Δh={dh}");
    }

    #[test]
    fn lower_barrier_violation_strongly_negative() {
        let (dh, _) = lower_barrier(-1.0);
        assert!(dh < -1.0e3, "Δh={dh}");
    }

    #[test]
    fn upper_barrier_feasible_near_zero() {
        let (dh, _) = upper_barrier(-1.0);
        assert!(dh.abs() < 1.0e-3, "Δh={dh}");
    }
}
