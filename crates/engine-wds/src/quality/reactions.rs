use super::shared::{PipeQuality, QualityState, TankQuality, C_MAX};
use crate::{
    HeadLossFormula, LinkKind, LinkState, Network, NodeKind, QualityMode, SimulationOptions,
    WallOrder,
};

/// Applies bulk and wall reactions to all pipe segments (§6.5.1–6.5.3). **∥**
pub(super) fn react_pipe_segs(
    state: &mut QualityState,
    network: &Network,
    link_states: &[LinkState],
    dt: f64,
    accumulate_rates: bool,
) {
    const SEC_PER_DAY: f64 = 86400.0;

    if network.options.quality_mode != QualityMode::Chemical {
        return;
    }
    let options = &network.options;

    // Split borrows so pipe_quality and pipe_rate_coeff can be mutated in
    // parallel while mass_balance is accumulated via reduction.
    let pipe_quality = &mut state.pipe_quality;
    let pipe_rate_coeff = &mut state.pipe_rate_coeff;

    // Per-link reaction logic shared by serial and parallel paths.
    let react_link = |pq_opt: &mut Option<PipeQuality>,
                      rate_coeff: &mut f64,
                      link: &crate::Link,
                      ls: &LinkState|
     -> (f64, f64, f64) {
        let pipe = match &link.kind {
            LinkKind::Pipe(p) => p,
            _ => {
                *rate_coeff = 0.0;
                return (0.0, 0.0, 0.0);
            }
        };
        let pq = match pq_opt.as_mut() {
            Some(p) => p,
            None => {
                *rate_coeff = 0.0;
                return (0.0, 0.0, 0.0);
            }
        };
        let flow = ls.flow.abs();
        let kb = pipe.bulk_coeff.unwrap_or(options.bulk_coeff);
        let kw = wall_coeff_for_pipe(pipe, options);

        let (keff_w1, kf_w0, zero_order_wall) = if kw != 0.0 {
            let kf = mass_transfer_coeff(
                flow,
                pipe.diameter,
                pipe.length,
                options.viscosity,
                options.diffusivity,
            );
            match options.wall_order {
                WallOrder::One => {
                    let ke = (4.0 / pipe.diameter) * kw * kf / (kf + kw.abs());
                    (ke, 0.0, false)
                }
                WallOrder::Zero => (0.0, kf, true),
            }
        } else {
            (0.0, 0.0, false)
        };

        let mut rsum = 0.0_f64;
        let mut vsum = 0.0_f64;
        let mut local_reacted = 0.0_f64;
        let mut local_bulk = 0.0_f64;
        let mut local_wall = 0.0_f64;

        for seg in &mut pq.segments {
            let c0 = seg.concentration;
            let dc_b = bulk_rate(kb, options.bulk_order, c0, options.conc_limit) * dt;
            let dc_w = if kw != 0.0 {
                if zero_order_wall {
                    // kw in mg/(m²·s); c0 in mg/L; kf in m/s (spec §6.5.2).
                    // Convert c0 → mg/m³ (×1000) so both sides of min() are
                    // in mg/(m²·s).  Divide result by 1000 to return mg/L/s.
                    let c0_m3 = c0 * 1000.0;
                    let kf_rate = c0_m3 * kf_w0;
                    let mag = kw.abs().min(kf_rate);
                    kw.signum() * mag * (4.0 / pipe.diameter) * dt / 1000.0
                } else {
                    keff_w1 * c0 * dt
                }
            } else {
                0.0
            };
            let c_new = (c0 + dc_b + dc_w).clamp(0.0, C_MAX);
            local_reacted += -(c_new - c0) * seg.volume;
            if accumulate_rates {
                local_bulk += dc_b.abs() * seg.volume;
                local_wall += dc_w.abs() * seg.volume;
            }
            rsum += (c_new - c0).abs() * seg.volume;
            vsum += seg.volume;

            seg.concentration = c_new;
        }

        *rate_coeff = if vsum > 0.0 {
            rsum / vsum / dt * SEC_PER_DAY
        } else {
            0.0
        };

        (local_reacted, local_bulk, local_wall)
    };

    let mut totals = (0.0_f64, 0.0_f64, 0.0_f64);
    for ((pq_opt, rc), (link, ls)) in pipe_quality
        .iter_mut()
        .zip(pipe_rate_coeff.iter_mut())
        .zip(network.links.iter().zip(link_states.iter()))
    {
        let (r, b, w) = react_link(pq_opt, rc, link, ls);
        totals.0 += r;
        totals.1 += b;
        totals.2 += w;
    }
    let (reacted, reacted_bulk, reacted_wall) = totals;

    state.mass_balance.reacted += reacted;
    if accumulate_rates {
        state.mass_balance.reacted_bulk += reacted_bulk;
        state.mass_balance.reacted_wall += reacted_wall;
    }
}

/// Applies bulk reactions to all tank compartments (§6.5).
pub(super) fn react_tanks(
    state: &mut QualityState,
    network: &Network,
    reactive: bool,
    dt: f64,
    accumulate_rates: bool,
) {
    if !reactive {
        return;
    }
    let options = &network.options;
    for (i, node) in network.nodes.iter().enumerate() {
        if let NodeKind::Tank(tank) = &node.kind {
            let kb = if tank.bulk_coeff != 0.0 {
                tank.bulk_coeff
            } else {
                options.bulk_coeff
            };
            if let Some(tq) = &mut state.tank_quality[i] {
                let mut dr = 0.0_f64;
                let mut dr_abs = 0.0_f64;
                match tq {
                    TankQuality::Cstr { volume, conc } => {
                        let dc = bulk_rate(kb, options.tank_order, *conc, options.conc_limit) * dt;
                        let c_new = (*conc + dc).clamp(0.0, C_MAX);
                        dr = -(c_new - *conc) * *volume;
                        dr_abs = dc.abs() * *volume;
                        *conc = c_new;
                    }
                    TankQuality::TwoComp {
                        mix_vol,
                        mix_conc,
                        stag_vol,
                        stag_conc,
                    } => {
                        // Two-Comp reactions applied after mixing (§6.7.2), so skip here.
                        let _ = (mix_vol, mix_conc, stag_vol, stag_conc);
                    }
                    TankQuality::Fifo { segments } => {
                        for seg in segments.iter_mut() {
                            let c0 = seg.concentration;
                            let dc = bulk_rate(kb, options.tank_order, c0, options.conc_limit) * dt;
                            let c_new = (c0 + dc).clamp(0.0, C_MAX);
                            dr += -(c_new - c0) * seg.volume;
                            dr_abs += dc.abs() * seg.volume;
                            seg.concentration = c_new;
                        }
                    }
                    TankQuality::Lifo { segments } => {
                        for seg in segments.iter_mut() {
                            let c0 = seg.concentration;
                            let dc = bulk_rate(kb, options.tank_order, c0, options.conc_limit) * dt;
                            let c_new = (c0 + dc).clamp(0.0, C_MAX);
                            dr += -(c_new - c0) * seg.volume;
                            dr_abs += dc.abs() * seg.volume;
                            seg.concentration = c_new;
                        }
                    }
                }
                state.mass_balance.reacted += dr;
                if accumulate_rates {
                    state.mass_balance.reacted_tank += dr_abs;
                }
            }
        }
    }
}

pub(super) fn age_inc_tank(tq: &mut TankQuality, inc: f64) {
    match tq {
        TankQuality::Cstr { conc, .. } => *conc = (*conc + inc).min(C_MAX),
        TankQuality::TwoComp {
            mix_conc,
            stag_conc,
            ..
        } => {
            *mix_conc = (*mix_conc + inc).min(C_MAX);
            *stag_conc = (*stag_conc + inc).min(C_MAX);
        }
        TankQuality::Fifo { segments } => {
            for seg in segments {
                seg.concentration = (seg.concentration + inc).min(C_MAX);
            }
        }
        TankQuality::Lifo { segments } => {
            for seg in segments {
                seg.concentration = (seg.concentration + inc).min(C_MAX);
            }
        }
    }
}

/// §6.5.1 Bulk reaction rate at concentration `c`.
///
/// Uses the potential function from the spec table; Michaelis-Menten is
/// activated when `order < 0`.
pub(super) fn bulk_rate(kb: f64, order: f64, c: f64, conc_limit: f64) -> f64 {
    if kb == 0.0 {
        return 0.0;
    }
    // EPANET: for zero-order, rate = kb regardless of concentration.
    // The potential is 1.0 (c is set to 1.0 in EPANET's bulkrate()).
    if order == 0.0 {
        return kb;
    }
    if c <= 0.0 {
        return 0.0;
    }
    let potential = if order < 0.0 {
        // Michaelis-Menten.
        if kb > 0.0 {
            // growth: c / (C_L + c)
            c / (conc_limit + c)
        } else {
            // decay: c / (C_L - c); guard against C_L <= c
            let denom = conc_limit - c;
            if denom > 0.0 {
                c / denom
            } else {
                0.0
            }
        }
    } else if order == 1.0 {
        if conc_limit == 0.0 {
            c
        } else if kb < 0.0 {
            // Decay: approach conc_limit from above.
            (c - conc_limit).max(0.0)
        } else {
            // Growth: approach conc_limit from below.
            (conc_limit - c).max(0.0)
        }
    } else {
        // General order n.
        if kb < 0.0 {
            // decay: c^(n-1) * max(0, c - C_L)
            c.powf(order - 1.0) * (c - conc_limit).max(0.0)
        } else {
            // growth: c^(n-1) * max(0, C_L - c)
            c.powf(order - 1.0) * (conc_limit - c).max(0.0)
        }
    };
    kb * potential
}

/// §6.5.2 Mass transfer coefficient k_f (m/s).
pub(super) fn mass_transfer_coeff(q: f64, d: f64, l: f64, nu: f64, diff: f64) -> f64 {
    if d <= 0.0 || diff <= 0.0 {
        return 0.0;
    }
    let re = 4.0 * q / (std::f64::consts::PI * d * nu);
    let sc = nu / diff;
    let sh = if re < 1.0 {
        2.0
    } else if re < 2300.0 {
        let x = (d / l) * re * sc;
        3.65 + 0.0668 * x / (1.0 + 0.04 * x.powf(2.0 / 3.0))
    } else {
        0.0149 * re.powf(0.88) * sc.powf(1.0 / 3.0)
    };
    sh * diff / d
}

/// §6.5.4 Returns the effective wall reaction coefficient for a pipe.
pub(super) fn wall_coeff_for_pipe(pipe: &crate::Pipe, opts: &SimulationOptions) -> f64 {
    // Explicit per-pipe value takes precedence (§6.5.4).
    if let Some(kw) = pipe.wall_coeff {
        return kw;
    }
    let rf = opts.roughness_reaction_factor;
    if rf != 0.0 {
        // Derive from roughness (§6.5.4).
        let eps = pipe.roughness;
        let d = pipe.diameter;
        match opts.head_loss_formula {
            HeadLossFormula::HazenWilliams => {
                if eps != 0.0 {
                    rf / eps
                } else {
                    0.0
                }
            }
            HeadLossFormula::DarcyWeisbach => {
                let arg = (eps / d).abs();
                if arg > 0.0 && arg != 1.0 {
                    rf / arg.ln().abs()
                } else {
                    0.0
                }
            }
            HeadLossFormula::ChezyManning => rf * eps,
        }
    } else {
        opts.wall_coeff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_rate_zero_order_is_kb() {
        let r = bulk_rate(-0.5, 0.0, 10.0, 0.0);
        approx::assert_abs_diff_eq!(r, -0.5, epsilon = 1e-12);
    }

    #[test]
    fn bulk_rate_first_order_decay() {
        let r = bulk_rate(-0.1, 1.0, 5.0, 0.0);
        approx::assert_abs_diff_eq!(r, -0.5, epsilon = 1e-12);
    }

    #[test]
    fn bulk_rate_zero_concentration_returns_zero() {
        let r = bulk_rate(-0.5, 1.0, 0.0, 0.0);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn mass_transfer_laminar_regime() {
        let kf = mass_transfer_coeff(0.001, 0.5, 1000.0, 1e-5, 1e-8);
        assert!(kf > 0.0, "k_f should be positive");
    }

    #[test]
    fn mass_transfer_very_low_re_returns_sh2() {
        let diff = 1e-8_f64;
        let d = 1.0_f64;
        let kf = mass_transfer_coeff(1e-12, d, 100.0, 1e-5, diff);
        approx::assert_abs_diff_eq!(kf, 2.0 * diff / d, epsilon = 1e-14);
    }
}
