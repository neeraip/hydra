//! Infiltration (§3.3): five relations on the pervious sub-area, each
//! carrying its dry-weather recovery model so continuous multi-storm
//! simulation is meaningful. All rates are m/s and depths m; the applied
//! rate is capacity or availability including ponded water.

use crate::io::options::InfiltrationModel;
use crate::model::Infiltration;

// `just mutants crates/engine-uds/src/hydrology/infiltration.rs` reports a
// few mutants in the Horton arm that no test catches, and they are
// equivalent rather than uncovered. They are listed here so the next
// reader does not chase them again:
//
//   both terms of the flat-curve arm's `(cp, cp + f_min * dt)`, where the
//   cumulative value cancels in the difference the caller takes and any
//   under-computation is clamped back by the `.max(f_min)` floor below —
//   the arm can only produce the floor, which is what it is for;
//
//   `dt / 2.0` in the Newton starting guess, which changes where the
//   solve begins and not where it lands;
//
//   `fa > 1e-12` read as `>=`, which decides the wet and dry arms for an
//   availability of exactly 1e-12 m/s. That is 4e-9 mm/hr: both arms
//   return the same nothing, and pinning the boundary would assert an
//   epsilon rather than a decision.
//
// Everything else the tool suggests is caught. When this file changes,
// run it again rather than trusting this note.

/// Exact feet-to-metres.
const FT: f64 = 0.3048;

/// Per-step scaling handles: the monthly conductivity pattern and the
/// recovery pattern (§3.1), both default 1.
#[derive(Debug, Clone, Copy)]
pub struct InfilFactors {
    /// Scales `f0`, `f∞`, `Ks` (and the Green–Ampt upper zone by its
    /// square root).
    pub conductivity: f64,
    /// Scales every regeneration coefficient.
    pub recovery: f64,
}

impl Default for InfilFactors {
    fn default() -> Self {
        InfilFactors {
            conductivity: 1.0,
            recovery: 1.0,
        }
    }
}

/// The live infiltration state for one parcel.
#[derive(Debug, Clone)]
pub enum InfilState {
    /// Horton by equivalent time on the capacity curve.
    Horton {
        f0: f64,
        f_min: f64,
        kd: f64,
        /// Regeneration coefficient (1/s): −ln(0.02)/T_dry.
        kr: f64,
        f_max: f64,
        /// Equivalent time on the curve (s).
        tp: f64,
        /// Cumulative infiltration against the volume cap (m).
        fe: f64,
    },
    /// Modified Horton by cumulative excess infiltration.
    ModHorton {
        f0: f64,
        f_min: f64,
        kd: f64,
        kr: f64,
        f_max: f64,
        /// Cumulative excess above `f_min` (m); the §3.3 volume cap
        /// seals the surface when it reaches `f_max`.
        fe: f64,
    },
    /// Green–Ampt (Mein–Larson), plain or modified.
    GreenAmpt {
        ks: f64,
        suction: f64,
        imd_max: f64,
        /// The low-intensity event reset is skipped by the modified form.
        modified: bool,
        /// Upper-zone thickness (m), the converted empirical fit.
        lu: f64,
        /// Current moisture deficit.
        imd: f64,
        /// Cumulative event infiltration (m).
        f: f64,
        /// Upper-zone water volume (m).
        fu: f64,
        /// Saturated-surface flag.
        sat: bool,
        /// Time until the current event expires (s).
        t: f64,
    },
    /// The SCS curve-number relation, differenced incrementally.
    CurveNumber {
        s_max: f64,
        /// Regeneration constant (1/s).
        regen: f64,
        /// Inter-event threshold (s).
        t_max: f64,
        /// Remaining capacity (m).
        s: f64,
        /// Event capacity at event start (m).
        se: f64,
        /// Event cumulative precipitation (m).
        p: f64,
        /// Event cumulative infiltration (m).
        f: f64,
        /// Previous applied rate (m/s), held through rainless gaps.
        f_prev: f64,
        /// Time since rain stopped (s).
        t: f64,
    },
}

impl InfilState {
    /// The §14.8 hotstart state vector, SI, in the predecessor's slot
    /// order for the active model.
    pub fn hotstart_get(&self) -> [f64; 6] {
        match self {
            InfilState::Horton { tp, fe, .. } => [*tp, *fe, 0.0, 0.0, 0.0, 0.0],
            InfilState::ModHorton { fe, .. } => [0.0, *fe, 0.0, 0.0, 0.0, 0.0],
            InfilState::GreenAmpt {
                imd, f, fu, sat, t, ..
            } => [*imd, *f, *fu, f64::from(u8::from(*sat)), *t, 0.0],
            InfilState::CurveNumber {
                s,
                p,
                f,
                t,
                se,
                f_prev,
                ..
            } => [*s, *p, *f, *t, *se, *f_prev],
        }
    }

    /// Restore the §14.8 hotstart state vector, SI.
    pub fn hotstart_set(&mut self, x: [f64; 6]) {
        match self {
            InfilState::Horton { tp, fe, .. } => {
                *tp = x[0];
                *fe = x[1];
            }
            InfilState::ModHorton { fe, .. } => {
                *fe = x[1];
            }
            InfilState::GreenAmpt {
                imd, f, fu, sat, t, ..
            } => {
                *imd = x[0];
                *f = x[1];
                *fu = x[2];
                *sat = x[3] != 0.0;
                *t = x[4];
            }
            InfilState::CurveNumber {
                s,
                p,
                f,
                t,
                se,
                f_prev,
                ..
            } => {
                *s = x[0];
                *p = x[1];
                *f = x[2];
                *t = x[3];
                *se = x[4];
                *f_prev = x[5];
            }
        }
    }

    /// Build the state for a parcel's parameters under the session's
    /// model selection.
    pub fn build(params: &Infiltration, model: InfiltrationModel) -> InfilState {
        match params {
            Infiltration::Horton {
                f0,
                f_min,
                decay,
                dry_time,
                f_max,
            } => {
                let kr = if *dry_time > 0.0 {
                    -(1.0_f64 - 0.98).ln() / dry_time
                } else {
                    0.0
                };
                if model == InfiltrationModel::ModifiedHorton {
                    InfilState::ModHorton {
                        f0: *f0,
                        f_min: *f_min,
                        kd: *decay,
                        kr,
                        f_max: *f_max,
                        fe: 0.0,
                    }
                } else {
                    InfilState::Horton {
                        f0: *f0,
                        f_min: *f_min,
                        kd: *decay,
                        kr,
                        f_max: *f_max,
                        tp: 0.0,
                        fe: 0.0,
                    }
                }
            }
            Infiltration::GreenAmpt {
                suction,
                conductivity,
                initial_deficit,
            } => {
                // L_u = 4·√Ks, the empirical fit in inches with Ks in
                // in/hr, converted (§3.3).
                let ks_inhr = conductivity * 3600.0 / 0.0254;
                let lu = 4.0 * ks_inhr.sqrt() * 0.0254;
                InfilState::GreenAmpt {
                    ks: *conductivity,
                    suction: *suction,
                    imd_max: *initial_deficit,
                    modified: model == InfiltrationModel::ModifiedGreenAmpt,
                    lu,
                    imd: *initial_deficit,
                    f: 0.0,
                    fu: 0.0,
                    sat: false,
                    t: 0.0,
                }
            }
            Infiltration::CurveNumber {
                curve_number,
                dry_time,
            } => {
                let cn = curve_number.clamp(10.0, 99.0);
                let s_max = (1000.0 / cn - 10.0) / 12.0 * FT;
                let regen = if *dry_time > 0.0 { 1.0 / dry_time } else { 0.0 };
                let t_max = if regen > 0.0 { 0.06 / regen } else { f64::MAX };
                InfilState::CurveNumber {
                    s_max,
                    regen,
                    t_max,
                    s: s_max,
                    se: s_max,
                    p: 0.0,
                    f: 0.0,
                    f_prev: 0.0,
                    t: 0.0,
                }
            }
        }
    }

    /// The applied infiltration rate (m/s) over a step: `irate` is the
    /// net surface input (rain + melt + run-on − evaporation) and `depth`
    /// the ponded water. State advances, wetting or recovering.
    pub fn step(&mut self, dt: f64, irate: f64, depth: f64, fac: InfilFactors) -> f64 {
        match self {
            InfilState::Horton {
                f0,
                f_min,
                kd,
                kr,
                f_max,
                tp,
                fe,
            } => {
                let f0 = *f0 * fac.conductivity;
                let f_min = *f_min * fac.conductivity;
                let kr = *kr * fac.recovery;
                let df = f0 - f_min;
                // Degenerate parameters mean constant capacity; f0 below
                // f∞ yields zero for the run (§3.3 file semantics).
                if df < 0.0 || *kd < 0.0 || kr < 0.0 {
                    return 0.0;
                }
                if df == 0.0 || *kd == 0.0 {
                    return (irate + depth / dt).min(f0).max(0.0);
                }
                let fa = irate + depth / dt;
                let mut fp = 0.0;
                if fa > 1e-12 {
                    // Step average of the cumulative curve, floored at f∞.
                    let t1 = *tp + dt;
                    let tlim = 16.0 / *kd;
                    let (cum_p, cum_1) = if *tp >= tlim {
                        let cp = f_min * *tp + df / *kd;
                        (cp, cp + f_min * dt)
                    } else {
                        (
                            f_min * *tp + df / *kd * (1.0 - (-*kd * *tp).exp()),
                            f_min * t1 + df / *kd * (1.0 - (-*kd * t1).exp()),
                        )
                    };
                    fp = ((cum_1 - cum_p) / dt).max(f_min).min(fa);
                    if t1 > tlim || fp < fa {
                        // On the flat portion, or capacity-limited: the
                        // soil took everything it could, so it wetted a
                        // whole step's worth. Advance directly.
                        *tp = t1;
                    } else {
                        // Rain-limited: less went in than the curve
                        // allowed, so the soil is not a whole step
                        // further along. Recover the equivalent time from
                        // what actually infiltrated.
                        let target = cum_p + fp * dt;
                        let mut t = *tp + dt / 2.0;
                        for _ in 0..20 {
                            let kt = (*kd * t).min(60.0);
                            let ex = (-kt).exp();
                            let cum = f_min * t + df / *kd * (1.0 - ex) - target;
                            let deriv = f_min + df * ex;
                            let r = cum / deriv;
                            t -= r;
                            if r.abs() <= 0.001 * dt {
                                break;
                            }
                        }
                        *tp = t;
                    }
                    if *f_max > 0.0 {
                        if *fe + fp * dt > *f_max {
                            fp = ((*f_max - *fe) / dt).max(0.0);
                        }
                        *fe += fp * dt;
                    }
                } else if kr > 0.0 {
                    // Dry recovery through the closed-form wetting/drying
                    // map.
                    let r = (-kr * dt).exp();
                    let x = 1.0 - (-*kd * *tp).exp();
                    *tp = -(1.0 - r * x).ln() / *kd;
                    if *f_max > 0.0 {
                        *fe = f_min * *tp + (df / *kd) * (1.0 - (-*kd * *tp).exp());
                    }
                }
                fp
            }
            InfilState::ModHorton {
                f0,
                f_min,
                kd,
                kr,
                f_max,
                fe,
            } => {
                let f0 = *f0 * fac.conductivity;
                let f_min = *f_min * fac.conductivity;
                let kr = *kr * fac.recovery;
                let df = f0 - f_min;
                if df < 0.0 || *kd < 0.0 || kr < 0.0 {
                    return 0.0;
                }
                let fa = irate + depth / dt;
                if fa <= 1e-12 {
                    // Dry-weather decay of the cumulative excess reopens
                    // a sealed surface (§3.3).
                    *fe *= (-kr * dt).exp();
                    return 0.0;
                }
                // §3.3: the volume cap is a finite store above the steady
                // f∞ drainage — the surface seals when the cumulative
                // *excess* fills it, never on the steady share.
                if *f_max > 0.0 && *fe >= *f_max {
                    return 0.0;
                }
                let mut fp = if df == 0.0 || *kd == 0.0 {
                    f0
                } else {
                    (f0 - *kd * *fe).max(f_min)
                };
                fp = fp.min(fa).max(0.0);
                // Excess above the equilibrium rate accumulates, capped
                // at the seal.
                *fe += (fp - f_min).max(0.0) * dt;
                if *f_max > 0.0 {
                    *fe = fe.min(*f_max);
                }
                fp
            }
            InfilState::GreenAmpt {
                ks,
                suction,
                imd_max,
                modified,
                lu,
                imd,
                f,
                fu,
                sat,
                t,
            } => {
                let ks_f = *ks * fac.conductivity;
                let lu_f = *lu * fac.conductivity.sqrt();
                let fu_max = *imd_max * lu_f;
                *t -= dt;
                let ia = (irate + depth / dt).max(0.0);
                let renew_t = 5400.0 / (lu_f / FT) / fac.recovery;

                if !*sat {
                    if ia == 0.0 {
                        // Recover upper-zone moisture.
                        if *fu <= 0.0 {
                            return 0.0;
                        }
                        let kr = (lu_f / FT) / 90_000.0 * fac.recovery;
                        let d_f = kr * fu_max * dt;
                        *f -= d_f;
                        *fu -= d_f;
                        if *fu <= 0.0 {
                            *fu = 0.0;
                            *f = 0.0;
                            *imd = *imd_max;
                            return 0.0;
                        }
                        if *t <= 0.0 {
                            *imd = (fu_max - *fu) / lu_f;
                            *f = 0.0;
                        }
                        return 0.0;
                    }
                    if ia <= ks_f {
                        // Light rain infiltrates whole; the plain form
                        // resets the event, the modified form does not
                        // (§3.3).
                        let d_f = ia * dt;
                        *f += d_f;
                        *fu = (*fu + d_f).min(fu_max);
                        if !*modified && *t <= 0.0 {
                            *imd = (fu_max - *fu) / lu_f;
                            *f = 0.0;
                        }
                        return ia;
                    }
                    *t = renew_t;
                    let fs = ks_f * (*suction + depth) * *imd / (ia - ks_f);
                    if *f > fs {
                        *sat = true;
                        // Fall through to the saturated branch below.
                    } else if *f + ia * dt < fs {
                        let d_f = ia * dt;
                        *f += d_f;
                        *fu = (*fu + d_f).min(fu_max);
                        return ia;
                    } else {
                        // Saturation arrives mid-step.
                        let ts = (dt - (fs - *f) / ia).max(0.0);
                        let c1 = (*suction + depth) * *imd;
                        let mut f2 = green_ampt_f2(fs, c1, ks_f, ts);
                        f2 = f2.min(fs + ia * ts);
                        let d_f = f2 - *f;
                        *f = f2;
                        *fu = (*fu + d_f).min(fu_max);
                        *sat = true;
                        return d_f / dt;
                    }
                }
                // Saturated surface.
                if ia < 1e-12 {
                    return 0.0;
                }
                *t = renew_t;
                let c1 = (*suction + depth) * *imd;
                let f2 = green_ampt_f2(*f, c1, ks_f, dt);
                let mut d_f = f2 - *f;
                if d_f > ia * dt {
                    d_f = ia * dt;
                    *sat = false;
                }
                *f += d_f;
                *fu = (*fu + d_f).min(fu_max);
                d_f / dt
            }
            InfilState::CurveNumber {
                s_max,
                regen,
                t_max,
                s,
                se,
                p,
                f,
                f_prev,
                t,
            } => {
                let fa = irate + depth / dt;
                let mut f1 = 0.0;
                if irate > 1e-12 {
                    if *t >= *t_max {
                        *p = 0.0;
                        *f = 0.0;
                        *f_prev = 0.0;
                        *se = *s;
                    }
                    *t = 0.0;
                    *p += irate * dt;
                    let cum = *p * (1.0 - *p / (*p + *se));
                    f1 = (cum - *f) / dt;
                    if f1 < 0.0 || *s <= 0.0 {
                        f1 = 0.0;
                    }
                } else if depth > 1e-6 && *s > 0.0 {
                    // Ponded water keeps infiltrating at the held rate.
                    f1 = f_prev.min(*s / dt);
                } else {
                    *t += dt;
                }
                if f1 > 0.0 {
                    f1 = f1.min(fa).max(0.0);
                    *f += f1 * dt;
                    if *regen > 0.0 {
                        *s = (*s - f1 * dt).max(0.0);
                    }
                } else {
                    *s = (*s + *regen * *s_max * dt * fac.recovery).min(*s_max);
                }
                *f_prev = f1;
                f1
            }
        }
    }
}

/// The integrated Green–Ampt equation for the new cumulative volume over
/// a step: the direct form for short established steps, Newton–Raphson on
/// the integral otherwise, floored at `f1 + Ks·Δt` (§3.3).
fn green_ampt_f2(f1: f64, c1: f64, ks: f64, ts: f64) -> f64 {
    let f2_min = f1 + ks * ts;
    if c1 == 0.0 {
        return f2_min;
    }
    if ts < 10.0 && f1 > 0.01 * c1 {
        return (f1 + ks * (1.0 + c1 / f1) * ts).max(f2_min);
    }
    let c2 = c1 * (f1 + c1).ln() - ks * ts;
    let mut f2 = f1;
    for _ in 0..20 {
        let d = (f2 - f1 - c1 * (f2 + c1).ln() + c2) / (1.0 - c1 / (f2 + c1));
        if d.abs() < 3.048e-6 {
            return f2.max(f2_min);
        }
        f2 -= d;
    }
    f2_min
}

// ── Checkpointing (§12.3) ────────────────────────────────────────────────────

impl InfilState {
    /// Write this relation's state (§12.3).
    ///
    /// Every variant and every field is named. The parameters are written
    /// too rather than bound to `_`: they are the *realised* values, which
    /// a monthly pattern moves during a run (§3.1), so restoring them from
    /// the model would restore January's conductivity into a run stopped
    /// in July.
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_b, put_f, put_u};
        match self {
            InfilState::Horton {
                f0,
                f_min,
                kd,
                kr,
                f_max,
                tp,
                fe,
            } => {
                put_u(w, 0)?;
                for v in [f0, f_min, kd, kr, f_max, tp, fe] {
                    put_f(w, *v)?;
                }
            }
            InfilState::ModHorton {
                f0,
                f_min,
                kd,
                kr,
                f_max,
                fe,
            } => {
                put_u(w, 1)?;
                for v in [f0, f_min, kd, kr, f_max, fe] {
                    put_f(w, *v)?;
                }
            }
            InfilState::GreenAmpt {
                ks,
                suction,
                imd_max,
                modified,
                lu,
                imd,
                f,
                fu,
                sat,
                t,
            } => {
                put_u(w, 2)?;
                for v in [ks, suction, imd_max, lu, imd, f, fu, t] {
                    put_f(w, *v)?;
                }
                put_b(w, *modified)?;
                put_b(w, *sat)?;
            }
            InfilState::CurveNumber {
                s_max,
                regen,
                t_max,
                s,
                se,
                p,
                f,
                f_prev,
                t,
            } => {
                put_u(w, 3)?;
                for v in [s_max, regen, t_max, s, se, p, f, f_prev, t] {
                    put_f(w, *v)?;
                }
            }
        }
        Ok(())
    }

    /// Read back what `checkpoint_put` wrote, in place.
    ///
    /// The relation is the model's, not the checkpoint's: a checkpoint of
    /// a different relation is refused rather than replacing the one the
    /// model asked for.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        let tag = r.u()?;
        let want = match self {
            InfilState::Horton { .. } => 0,
            InfilState::ModHorton { .. } => 1,
            InfilState::GreenAmpt { .. } => 2,
            InfilState::CurveNumber { .. } => 3,
        };
        if tag != want {
            return Err(format!(
                "checkpoint holds infiltration relation {tag} where this model \
                 uses {want}"
            ));
        }
        match self {
            InfilState::Horton {
                f0,
                f_min,
                kd,
                kr,
                f_max,
                tp,
                fe,
            } => {
                for v in [f0, f_min, kd, kr, f_max, tp, fe] {
                    *v = r.f()?;
                }
            }
            InfilState::ModHorton {
                f0,
                f_min,
                kd,
                kr,
                f_max,
                fe,
            } => {
                for v in [f0, f_min, kd, kr, f_max, fe] {
                    *v = r.f()?;
                }
            }
            InfilState::GreenAmpt {
                ks,
                suction,
                imd_max,
                modified,
                lu,
                imd,
                f,
                fu,
                sat,
                t,
            } => {
                for v in [ks, suction, imd_max, lu, imd, f, fu, t] {
                    *v = r.f()?;
                }
                *modified = r.b()?;
                *sat = r.b()?;
            }
            InfilState::CurveNumber {
                s_max,
                regen,
                t_max,
                s,
                se,
                p,
                f,
                f_prev,
                t,
            } => {
                for v in [s_max, regen, t_max, s, se, p, f, f_prev, t] {
                    *v = r.f()?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod horton_step_tests {
    use super::*;

    /// Millimetres an hour as m/s, which is how these parameters read in
    /// a model file and how the specification states them.
    fn mmhr(v: f64) -> f64 {
        v * 1.0e-3 / 3600.0
    }

    fn horton(f0: f64, f_min: f64, kd_per_hour: f64, kr: f64, f_max: f64) -> InfilState {
        InfilState::Horton {
            f0: mmhr(f0),
            f_min: mmhr(f_min),
            kd: kd_per_hour / 3600.0,
            kr,
            f_max,
            tp: 0.0,
            fe: 0.0,
        }
    }

    /// Where the parcel has reached on its capacity curve (s).
    fn tp_of(s: &InfilState) -> f64 {
        let InfilState::Horton { tp, .. } = s else {
            panic!("model changed")
        };
        *tp
    }

    /// §3.3: capacity falls from $f_0$ toward $f_\infty$ as
    /// $f = f_\infty + (f_0 - f_\infty)e^{-kt}$, and a step's rate is
    /// that curve's mean across the step, which for a step beginning at
    /// the start of the curve is
    /// $f_\infty + (f_0 - f_\infty)(1 - e^{-k\Delta t})/(k\Delta t)$.
    ///
    /// Stated here as the specification states it, so the test does not
    /// merely repeat the way the code happens to arrange the same terms.
    #[test]
    fn the_rate_is_the_step_average_of_the_capacity_curve() {
        let dt = 900.0;
        let mut s = horton(20.0, 5.0, 4.0, 0.0, 0.0);
        let fp = s.step(dt, mmhr(25.0), 0.0, InfilFactors::default());

        let (f0, f_min, k) = (mmhr(20.0), mmhr(5.0), 4.0 / 3600.0);
        let expected = f_min + (f0 - f_min) * (1.0 - (-k * dt).exp()) / (k * dt);
        assert!(
            (fp - expected).abs() < 1e-12 * expected,
            "{fp} is not the curve's mean {expected}"
        );
        // It is strictly between the two ends: a mean that came out at
        // either would mean the exponential had gone missing.
        assert!(
            fp < f0 && fp > f_min,
            "{fp} is not between the curve's ends"
        );
    }

    /// Availability is what falls *plus what is already standing*: a step
    /// with no rain over ponded water still infiltrates it.
    #[test]
    fn ponded_water_infiltrates_when_no_rain_falls() {
        let dt = 900.0;
        let ponded = 1.0e-3;
        let mut s = horton(20.0, 5.0, 4.0, 0.0, 0.0);
        let fp = s.step(dt, 0.0, ponded, InfilFactors::default());
        // A millimetre standing over a quarter hour is well under the
        // capacity, so all of it goes in.
        assert!((fp - ponded / dt).abs() < 1e-18, "{fp} of {}", ponded / dt);
    }

    /// §3.1: the monthly conductivity pattern scales $f_0$ and $f_\infty$
    /// together, so it scales the whole curve rather than tilting it.
    #[test]
    fn the_monthly_conductivity_pattern_scales_the_whole_curve() {
        let dt = 900.0;
        let rain = mmhr(25.0);
        let mut full = horton(20.0, 5.0, 4.0, 0.0, 0.0);
        let mut half = horton(20.0, 5.0, 4.0, 0.0, 0.0);
        let a = full.step(dt, rain, 0.0, InfilFactors::default());
        let b = half.step(
            dt,
            rain,
            0.0,
            InfilFactors {
                conductivity: 0.5,
                recovery: 1.0,
            },
        );
        assert!(
            (b - a / 2.0).abs() < 1e-12 * a,
            "a pattern of a half gave {b}, not half of {a}"
        );
    }

    /// §3.3: parameters that describe no decay are a constant capacity,
    /// and the rate is still bounded by what is there to infiltrate.
    #[test]
    fn degenerate_parameters_mean_a_constant_capacity() {
        let dt = 900.0;
        let default = InfilFactors::default();

        // f0 equal to f∞: the curve is a horizontal line at that value.
        let mut flat = horton(5.0, 5.0, 4.0, 0.0, 0.0);
        let fp = flat.step(dt, mmhr(25.0), 0.0, default);
        assert!((fp - mmhr(5.0)).abs() < 1e-18, "{fp}");

        // And bounded by availability when that is the smaller.
        let mut flat = horton(5.0, 5.0, 4.0, 0.0, 0.0);
        let fp = flat.step(dt, mmhr(2.0), 0.0, default);
        assert!((fp - mmhr(2.0)).abs() < 1e-18, "{fp}");

        // A zero decay coefficient never leaves f0, and must not divide
        // by itself on the way to saying so.
        let mut undecayed = horton(20.0, 5.0, 0.0, 0.0, 0.0);
        let fp = undecayed.step(dt, mmhr(25.0), 0.0, default);
        assert!((fp - mmhr(20.0)).abs() < 1e-18, "{fp}");

        // Standing water is available on this arm too: the constant
        // capacity is still measured against rain *plus* what is ponded.
        let ponded = 1.0e-3;
        let mut flat = horton(5.0, 5.0, 4.0, 0.0, 0.0);
        let fp = flat.step(dt, 0.0, ponded, default);
        assert!((fp - ponded / dt).abs() < 1e-18, "{fp} of {}", ponded / dt);

        // Evaporation exceeding the rain is not negative infiltration,
        // with or without water standing on the parcel.
        let mut drying = horton(20.0, 5.0, 0.0, 0.0, 0.0);
        assert_eq!(0.0, drying.step(dt, -mmhr(5.0), 0.0, default));
        let mut drying = horton(20.0, 5.0, 0.0, 0.0, 0.0);
        assert_eq!(0.0, drying.step(dt, -mmhr(50.0), ponded, default));
    }

    /// §3.3 file semantics: parameters that cannot describe a decaying
    /// curve yield nothing for the run rather than an invented one. Each
    /// is failed on its own, because any one of them is enough.
    #[test]
    fn parameters_that_cannot_describe_a_curve_infiltrate_nothing() {
        let dt = 900.0;
        let rain = mmhr(25.0);
        let default = InfilFactors::default();

        let mut inverted = horton(2.0, 5.0, 4.0, 0.0, 0.0);
        assert_eq!(
            0.0,
            inverted.step(dt, rain, 0.0, default),
            "an initial capacity below the floor"
        );
        let mut growing = horton(20.0, 5.0, -4.0, 0.0, 0.0);
        assert_eq!(
            0.0,
            growing.step(dt, rain, 0.0, default),
            "a decay that grows the capacity instead"
        );
        let mut backwards = horton(20.0, 5.0, 4.0, -1.0e-6, 0.0);
        assert_eq!(
            0.0,
            backwards.step(dt, rain, 0.0, default),
            "a regeneration that runs the wrong way"
        );
    }

    /// Past $16/k$ the exponential is spent and the curve is its floor
    /// the whole way across, which is a separate arm from the general
    /// one and reachable only from an already-wet parcel.
    #[test]
    fn a_long_wet_curve_flattens_to_the_equilibrium_rate() {
        let dt = 900.0;
        let mut s = InfilState::Horton {
            f0: mmhr(20.0),
            f_min: mmhr(5.0),
            kd: 4.0 / 3600.0,
            kr: 0.0,
            f_max: 0.0,
            // 16/k is 14 400 s for this decay.
            tp: 20_000.0,
            fe: 0.0,
        };
        let fp = s.step(dt, mmhr(25.0), 0.0, InfilFactors::default());
        assert!((fp - mmhr(5.0)).abs() < 1e-18, "{fp} is not the floor");
    }

    /// The curve advances by what actually went in, not by the clock.
    ///
    /// A step that infiltrated everything the soil could take has wetted
    /// a whole step's worth; a step where the rain ran out first has not,
    /// and the equivalent time is recovered from the volume instead. The
    /// two arms were labelled the wrong way round in the source until
    /// this test was written.
    #[test]
    fn the_curve_advances_by_what_actually_infiltrated() {
        let dt = 900.0;
        let default = InfilFactors::default();

        let mut ample = horton(20.0, 5.0, 4.0, 0.0, 0.0);
        ample.step(dt, mmhr(25.0), 0.0, default);
        assert_eq!(dt, tp_of(&ample), "a full step's wetting is a full step");

        let mut trickle = horton(20.0, 5.0, 4.0, 0.0, 0.0);
        let fp = trickle.step(dt, mmhr(6.0), 0.0, default);
        assert!(
            (fp - mmhr(6.0)).abs() < 1e-18,
            "the rain is the limit: {fp}"
        );
        let advanced = tp_of(&trickle);
        assert!(
            advanced > 0.0 && advanced < dt,
            "six of a possible fourteen millimetres advanced the curve \
             {advanced} s of {dt}"
        );
    }

    /// §3.3: a dry step recovers the curve back toward its start, and the
    /// monthly recovery pattern scales how fast.
    #[test]
    fn a_dry_step_walks_the_curve_back_toward_its_start() {
        let dt = 3600.0;
        let wet = |tp| InfilState::Horton {
            f0: mmhr(20.0),
            f_min: mmhr(5.0),
            kd: 4.0 / 3600.0,
            kr: 1.0 / 86_400.0,
            f_max: 0.0,
            tp,
            fe: 0.0,
        };

        let mut s = wet(7_200.0);
        assert_eq!(
            0.0,
            s.step(dt, 0.0, 0.0, InfilFactors::default()),
            "nothing available is nothing infiltrated"
        );
        let recovered = tp_of(&s);
        assert!(recovered < 7_200.0, "the curve recovered to {recovered}");
        assert!(recovered > 0.0, "but a single dry hour is not a dry week");

        let mut faster = wet(7_200.0);
        faster.step(
            dt,
            0.0,
            0.0,
            InfilFactors {
                conductivity: 1.0,
                recovery: 2.0,
            },
        );
        assert!(
            tp_of(&faster) < recovered,
            "a doubled recovery pattern recovers further: {} against {recovered}",
            tp_of(&faster)
        );
    }

    /// §3.3: the optional total-volume cap seals the surface once the
    /// parcel has taken that much.
    #[test]
    fn the_volume_cap_seals_the_surface() {
        let dt = 900.0;
        let cap = 1.0e-3;
        let rain = mmhr(25.0);
        let mut s = InfilState::Horton {
            f0: mmhr(20.0),
            f_min: mmhr(5.0),
            kd: 4.0 / 3600.0,
            kr: 0.0,
            f_max: cap,
            tp: 0.0,
            fe: 0.0,
        };
        // The curve alone would take about three and a half millimetres
        // this step, so the cap is what stops it at one.
        let first = s.step(dt, rain, 0.0, InfilFactors::default());
        assert!(
            (first * dt - cap).abs() < 1e-15,
            "the first step took {} m, not the cap",
            first * dt
        );
        assert_eq!(
            0.0,
            s.step(dt, rain, 0.0, InfilFactors::default()),
            "and the surface is sealed thereafter"
        );
    }
}

#[cfg(test)]
mod build_tests {
    use super::*;

    /// §3.3: the regeneration coefficient is $k_r = 3.912/T_{dry}$, the
    /// 3.912 being $-\ln(0.02)$ — the curve reaching 98% recovery in the
    /// stated drying time. Asserted against the constant the specification
    /// states, not against the expression the code writes it as.
    #[test]
    fn horton_regenerates_over_its_stated_drying_time() {
        let seven_days = 7.0 * 86_400.0;
        let InfilState::Horton { kr, .. } = InfilState::build(
            &Infiltration::Horton {
                f0: 1.0,
                f_min: 2.0,
                decay: 3.0,
                dry_time: seven_days,
                f_max: 4.0,
            },
            InfiltrationModel::Horton,
        ) else {
            panic!("model changed");
        };
        // The specification writes the constant to four figures; the code
        // carries the exact $-\ln(0.02)$ it rounds, so the comparison is
        // to the precision the specification states.
        assert!(
            (kr * seven_days - 3.912).abs() < 1e-3,
            "kr·T_dry is {}, not 3.912",
            kr * seven_days
        );
        // A doubled drying time recovers half as fast, which a coefficient
        // multiplied by the time rather than divided by it would not.
        let InfilState::Horton { kr: slower, .. } = InfilState::build(
            &Infiltration::Horton {
                f0: 1.0,
                f_min: 2.0,
                decay: 3.0,
                dry_time: 2.0 * seven_days,
                f_max: 4.0,
            },
            InfiltrationModel::Horton,
        ) else {
            panic!("model changed");
        };
        assert!(
            (slower - kr / 2.0).abs() < 1e-15,
            "{slower} is not half {kr}"
        );
    }

    /// A drying time of zero is "never recovers", not a division by it.
    #[test]
    fn a_drying_time_of_zero_is_no_recovery_at_all() {
        let InfilState::Horton { kr, .. } = InfilState::build(
            &Infiltration::Horton {
                f0: 1.0,
                f_min: 2.0,
                decay: 3.0,
                dry_time: 0.0,
                f_max: 4.0,
            },
            InfiltrationModel::Horton,
        ) else {
            panic!("model changed");
        };
        assert_eq!(0.0, kr);
    }

    /// The session's model selection picks the relation, and the two
    /// Horton forms are different state machines rather than a flag.
    #[test]
    fn the_selection_chooses_between_the_two_horton_forms() {
        let params = Infiltration::Horton {
            f0: 1.0,
            f_min: 2.0,
            decay: 3.0,
            dry_time: 4.0,
            f_max: 5.0,
        };
        assert!(matches!(
            InfilState::build(&params, InfiltrationModel::Horton),
            InfilState::Horton { .. }
        ));
        assert!(matches!(
            InfilState::build(&params, InfiltrationModel::ModifiedHorton),
            InfilState::ModHorton { .. }
        ));
        // Any other selection reaching Horton parameters is still Horton,
        // not the modified form by default.
        assert!(matches!(
            InfilState::build(&params, InfiltrationModel::GreenAmpt),
            InfilState::Horton { .. }
        ));
    }

    /// §3.3: $L_u = 4\sqrt{K_s}$, an empirical fit in inches with $K_s$
    /// in in/hr. The conversion in and back out is the whole content of
    /// the line, so it is checked at two conductivities: one where the
    /// square root is invisible and one where it is not.
    #[test]
    fn the_green_ampt_upper_zone_is_four_root_ks_in_inches() {
        let inch = 0.0254;
        let lu_of = |ks_in_per_hour: f64| {
            let InfilState::GreenAmpt { lu, .. } = InfilState::build(
                &Infiltration::GreenAmpt {
                    suction: 0.1,
                    conductivity: ks_in_per_hour * inch / 3600.0,
                    initial_deficit: 0.2,
                },
                InfiltrationModel::GreenAmpt,
            ) else {
                panic!("model changed");
            };
            lu
        };
        // One in/hr: four inches of upper zone.
        assert!((lu_of(1.0) - 4.0 * inch).abs() < 1e-12, "{}", lu_of(1.0));
        // Four in/hr: the root doubles it to eight inches, which a fit
        // written without the root would make sixteen.
        assert!((lu_of(4.0) - 8.0 * inch).abs() < 1e-12, "{}", lu_of(4.0));
    }

    /// The modified form is the one that skips the low-intensity event
    /// reset, and nothing else selects it.
    #[test]
    fn the_selection_chooses_between_the_two_green_ampt_forms() {
        let params = Infiltration::GreenAmpt {
            suction: 0.1,
            conductivity: 1.0e-5,
            initial_deficit: 0.2,
        };
        let modified_of = |m| {
            let InfilState::GreenAmpt { modified, .. } = InfilState::build(&params, m) else {
                panic!("model changed");
            };
            modified
        };
        assert!(modified_of(InfiltrationModel::ModifiedGreenAmpt));
        assert!(!modified_of(InfiltrationModel::GreenAmpt));
        assert!(!modified_of(InfiltrationModel::Horton));
    }

    /// The deficit starts at the model's own maximum: a parcel begins as
    /// dry as its parameters say, not empty.
    #[test]
    fn green_ampt_starts_at_its_declared_deficit() {
        let InfilState::GreenAmpt {
            imd,
            imd_max,
            f,
            fu,
            sat,
            t,
            ..
        } = InfilState::build(
            &Infiltration::GreenAmpt {
                suction: 0.1,
                conductivity: 1.0e-5,
                initial_deficit: 0.27,
            },
            InfiltrationModel::GreenAmpt,
        )
        else {
            panic!("model changed");
        };
        assert_eq!((0.27, 0.27), (imd, imd_max));
        assert_eq!((0.0, 0.0, false, 0.0), (f, fu, sat, t));
    }

    /// §3.3: $S_{max} = 1000/CN - 10$ inches, the tabulated relation's own
    /// units, identified and converted. Asserted in inches against the
    /// relation the specification states rather than against the chain of
    /// conversions the code writes it as.
    #[test]
    fn the_curve_number_capacity_is_the_scs_relation_in_inches() {
        let inch = 0.0254;
        let s_max_of = |cn: f64| {
            let InfilState::CurveNumber { s_max, s, se, .. } = InfilState::build(
                &Infiltration::CurveNumber {
                    curve_number: cn,
                    dry_time: 7.0 * 86_400.0,
                },
                InfiltrationModel::CurveNumber,
            ) else {
                panic!("model changed");
            };
            // A parcel begins at its full capacity, in both accounts.
            assert_eq!((s_max, s_max), (s, se));
            s_max
        };
        // CN 80: 1000/80 − 10 = 2.5 inches.
        assert!(
            (s_max_of(80.0) - 2.5 * inch).abs() < 1e-12,
            "{}",
            s_max_of(80.0)
        );
        // CN 50: 1000/50 − 10 = 10 inches, which a relation adding the ten
        // instead of subtracting it would make thirty.
        assert!(
            (s_max_of(50.0) - 10.0 * inch).abs() < 1e-12,
            "{}",
            s_max_of(50.0)
        );
    }

    /// §3.3: $CN$ clamps to $[10, 99]$. Outside that the relation runs
    /// away — CN 0 is an infinite capacity and CN 100 is none at all.
    #[test]
    fn the_curve_number_clamps_to_its_tabulated_range() {
        let s_max_of = |cn: f64| {
            let InfilState::CurveNumber { s_max, .. } = InfilState::build(
                &Infiltration::CurveNumber {
                    curve_number: cn,
                    dry_time: 86_400.0,
                },
                InfiltrationModel::CurveNumber,
            ) else {
                panic!("model changed");
            };
            s_max
        };
        assert!(
            (s_max_of(1.0) - s_max_of(10.0)).abs() < 1e-15,
            "below the range"
        );
        assert!((s_max_of(120.0) - s_max_of(99.0)).abs() < 1e-15, "above it");
        assert!(
            s_max_of(120.0) > 0.0,
            "the upper clamp still leaves capacity"
        );
    }

    /// §3.3: capacity recovers at $k_r$ with $k_r = 1/(24\,T_{dry})$ per
    /// hour, and a new event begins after $0.06/k_r$. In SI both reduce
    /// to the drying time in seconds.
    #[test]
    fn the_curve_number_recovers_over_its_drying_time() {
        let week = 7.0 * 86_400.0;
        let InfilState::CurveNumber { regen, t_max, .. } = InfilState::build(
            &Infiltration::CurveNumber {
                curve_number: 80.0,
                dry_time: week,
            },
            InfiltrationModel::CurveNumber,
        ) else {
            panic!("model changed");
        };
        assert!((regen - 1.0 / week).abs() < 1e-18, "regen {regen}");
        assert!((t_max - 0.06 / regen).abs() < 1e-6, "inter-event {t_max}");
        assert!(
            t_max < week,
            "the inter-event gap is shorter than the drying"
        );
    }

    /// A drying time of zero never recovers, and never starts a second
    /// event either — not a division by zero in either place.
    #[test]
    fn a_curve_number_drying_time_of_zero_never_recovers() {
        let InfilState::CurveNumber { regen, t_max, .. } = InfilState::build(
            &Infiltration::CurveNumber {
                curve_number: 80.0,
                dry_time: 0.0,
            },
            InfiltrationModel::CurveNumber,
        ) else {
            panic!("model changed");
        };
        assert_eq!(0.0, regen);
        assert_eq!(f64::MAX, t_max, "no second event ever begins");
    }
}

#[cfg(test)]
mod hotstart_slot_tests {
    use super::*;

    /// The six-slot vector is the predecessor's, and each model puts its
    /// own state in its own slots.
    ///
    /// Asserted by index, in both directions separately, rather than by a
    /// round trip: a round trip through this pair calls a shared mistake
    /// about the order correct going out and coming back, and the slots
    /// are the whole content of the format. Until these existed, both
    /// halves could be replaced by nothing at all and every test in the
    /// workspace stayed green.
    #[test]
    fn horton_writes_its_curve_time_and_volume_to_the_first_two_slots() {
        let s = InfilState::Horton {
            f0: 1.0,
            f_min: 2.0,
            kd: 3.0,
            kr: 4.0,
            f_max: 5.0,
            tp: 60.0,
            fe: 0.007,
        };
        assert_eq!([60.0, 0.007, 0.0, 0.0, 0.0, 0.0], s.hotstart_get());
    }

    #[test]
    fn horton_reads_them_back_from_the_same_two_slots() {
        let mut s = InfilState::Horton {
            f0: 1.0,
            f_min: 2.0,
            kd: 3.0,
            kr: 4.0,
            f_max: 5.0,
            tp: 0.0,
            fe: 0.0,
        };
        s.hotstart_set([60.0, 0.007, 9.0, 9.0, 9.0, 9.0]);
        let InfilState::Horton { tp, fe, f0, .. } = s else {
            panic!("model changed");
        };
        assert_eq!(60.0, tp, "slot 0 is the equivalent time on the curve");
        assert_eq!(0.007, fe, "slot 1 is the cumulative infiltration");
        assert_eq!(1.0, f0, "the parameters are not state and do not move");
    }

    /// Modified Horton carries no curve time, so its slot 0 stays empty
    /// and its volume sits in the same slot as plain Horton's.
    #[test]
    fn modified_horton_uses_the_volume_slot_alone() {
        let s = InfilState::ModHorton {
            f0: 1.0,
            f_min: 2.0,
            kd: 3.0,
            kr: 4.0,
            f_max: 5.0,
            fe: 0.008,
        };
        assert_eq!([0.0, 0.008, 0.0, 0.0, 0.0, 0.0], s.hotstart_get());

        let mut s = InfilState::ModHorton {
            f0: 1.0,
            f_min: 2.0,
            kd: 3.0,
            kr: 4.0,
            f_max: 5.0,
            fe: 0.0,
        };
        s.hotstart_set([9.0, 0.008, 9.0, 9.0, 9.0, 9.0]);
        let InfilState::ModHorton { fe, .. } = s else {
            panic!("model changed");
        };
        assert_eq!(0.008, fe);
    }

    #[test]
    fn green_ampt_uses_five_slots_and_encodes_its_flag_as_a_number() {
        let s = InfilState::GreenAmpt {
            ks: 1.0,
            suction: 2.0,
            imd_max: 3.0,
            modified: false,
            lu: 4.0,
            imd: 0.11,
            f: 0.22,
            fu: 0.33,
            sat: true,
            t: 44.0,
        };
        assert_eq!([0.11, 0.22, 0.33, 1.0, 44.0, 0.0], s.hotstart_get());

        // And an unsaturated surface is a zero, not merely "not one".
        let dry = InfilState::GreenAmpt {
            ks: 1.0,
            suction: 2.0,
            imd_max: 3.0,
            modified: false,
            lu: 4.0,
            imd: 0.11,
            f: 0.22,
            fu: 0.33,
            sat: false,
            t: 44.0,
        };
        assert_eq!(0.0, dry.hotstart_get()[3]);
    }

    #[test]
    fn green_ampt_reads_its_five_slots_back() {
        let mut s = InfilState::GreenAmpt {
            ks: 1.0,
            suction: 2.0,
            imd_max: 3.0,
            modified: false,
            lu: 4.0,
            imd: 0.0,
            f: 0.0,
            fu: 0.0,
            sat: false,
            t: 0.0,
        };
        s.hotstart_set([0.11, 0.22, 0.33, 1.0, 44.0, 9.0]);
        let InfilState::GreenAmpt {
            imd, f, fu, sat, t, ..
        } = s
        else {
            panic!("model changed");
        };
        assert_eq!((0.11, 0.22, 0.33, true, 44.0), (imd, f, fu, sat, t));

        // Any non-zero is saturated; only zero is not.
        let mut s = InfilState::GreenAmpt {
            ks: 1.0,
            suction: 2.0,
            imd_max: 3.0,
            modified: false,
            lu: 4.0,
            imd: 0.0,
            f: 0.0,
            fu: 0.0,
            sat: true,
            t: 0.0,
        };
        s.hotstart_set([0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let InfilState::GreenAmpt { sat, .. } = s else {
            panic!("model changed");
        };
        assert!(!sat, "slot 3 of zero is an unsaturated surface");
    }

    #[test]
    fn the_curve_number_model_uses_every_slot() {
        let s = InfilState::CurveNumber {
            s_max: 1.0,
            regen: 2.0,
            t_max: 3.0,
            s: 0.11,
            se: 0.55,
            p: 0.22,
            f: 0.33,
            f_prev: 0.66,
            t: 0.44,
        };
        assert_eq!([0.11, 0.22, 0.33, 0.44, 0.55, 0.66], s.hotstart_get());

        let mut s = InfilState::CurveNumber {
            s_max: 1.0,
            regen: 2.0,
            t_max: 3.0,
            s: 0.0,
            se: 0.0,
            p: 0.0,
            f: 0.0,
            f_prev: 0.0,
            t: 0.0,
        };
        s.hotstart_set([0.11, 0.22, 0.33, 0.44, 0.55, 0.66]);
        let InfilState::CurveNumber {
            s,
            p,
            f,
            t,
            se,
            f_prev,
            ..
        } = s
        else {
            panic!("model changed");
        };
        assert_eq!(
            (0.11, 0.22, 0.33, 0.44, 0.55, 0.66),
            (s, p, f, t, se, f_prev)
        );
    }
}
