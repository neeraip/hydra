//! Infiltration (§3.3): five relations on the pervious sub-area, each
//! carrying its dry-weather recovery model so continuous multi-storm
//! simulation is meaningful. All rates are m/s and depths m; the applied
//! rate is capacity or availability including ponded water.

use crate::io::options::InfiltrationModel;
use crate::model::Infiltration;

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
        /// Cumulative excess above `f_min` (m).
        fe: f64,
        /// Total cumulative infiltration (m), for the cap.
        f_total: f64,
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
                        f_total: 0.0,
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
                        // On the flat portion, or rain-limited: advance
                        // directly.
                        *tp = t1;
                    } else {
                        // Capacity-limited: recover the equivalent time
                        // from what actually infiltrated.
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
                f_total,
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
                    // Dry-weather decay of the cumulative excess.
                    *fe *= (-kr * dt).exp();
                    if *f_max > 0.0 {
                        *f_total = (*f_total - kr * dt * *f_max).max(0.0);
                    }
                    return 0.0;
                }
                let mut fp = if df == 0.0 || *kd == 0.0 {
                    f0
                } else {
                    (f0 - *kd * *fe).max(f_min)
                };
                fp = fp.min(fa).max(0.0);
                if *f_max > 0.0 {
                    if *f_total + fp * dt > *f_max {
                        fp = ((*f_max - *f_total) / dt).max(0.0);
                    }
                    *f_total += fp * dt;
                }
                // Excess above the equilibrium rate accumulates.
                *fe += (fp - f_min).max(0.0) * dt;
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
