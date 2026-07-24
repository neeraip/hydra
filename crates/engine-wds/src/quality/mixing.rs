use super::reactions::bulk_rate;
use super::shared::{
    push_segment_merge, qual_flow_dir, MassBalance, PipeQuality, Segment, TankQuality, C_MAX,
};
use crate::{LinkState, Network, NodeState, QualityMode};

/// Advances a single tank's quality state for one sub-step (§6.7).
/// Returns the outflow concentration.
#[allow(clippy::too_many_arguments)]
pub(super) fn update_tank_mix(
    tq_opt: &mut Option<TankQuality>,
    network: &Network,
    tank: &crate::Tank,
    c_in: f64,
    v_in: f64,
    v_out: f64,
    v_net: f64,
    kb: f64,
    order: f64,
    conc_limit: f64,
    dt: f64,
    reactive: bool,
    mb: &mut MassBalance,
    accumulate_rates: bool,
) -> f64 {
    let tq = match tq_opt {
        Some(t) => t,
        None => return 0.0,
    };

    match tq {
        // ── §6.7.1 CSTR ──────────────────────────────────────────────────────
        // Match EPANET's tankmix1: volume-weighted mixing of current tank
        // contents with inflow.  Bulk reactions are handled separately in
        // react_tanks() — do NOT add a reaction term here.
        TankQuality::Cstr { volume, conc } => {
            let v = *volume;
            let vnew = v + v_in;
            let c_new = if vnew > 0.0 {
                (*conc * v + v_in * c_in) / vnew
            } else {
                c_in
            };
            let v_new = (v + v_net).max(0.0);
            *conc = c_new;
            *volume = v_new;
            c_new
        }

        // ── §6.7.2 Two-compartment ───────────────────────────────────────────
        TankQuality::TwoComp {
            mix_vol,
            mix_conc,
            stag_vol,
            stag_conc,
        } => {
            let v_max = tank.volume_from_level(tank.max_level, &network.curves);
            let v_mz = tank.mix_fraction * v_max;
            let v_sz = v_max - v_mz;

            if v_net >= 0.0 {
                // Filling or no-net-flow.
                // Step 1: mix inflow into mixing zone (concentration only).
                let w_in = v_in * c_in;
                if *mix_vol + v_in > 0.0 {
                    *mix_conc = (*mix_conc * *mix_vol + w_in) / (*mix_vol + v_in);
                }

                // Step 2: compute overflow from mixing zone.
                let v_t = (*mix_vol + v_net - v_mz).max(0.0);

                if v_t > 0.0 {
                    // Step 3: transfer overflow to stagnant zone.
                    *stag_conc = (*stag_conc * *stag_vol + *mix_conc * v_t) / (*stag_vol + v_t);
                    *mix_vol = v_mz;
                    *stag_vol += v_t;
                    if *stag_vol > v_sz {
                        *stag_vol = v_sz; // surplus exits; volume discarded
                    }
                } else {
                    // Step 4: no overflow.
                    *mix_vol = (*mix_vol + v_net).clamp(0.0, v_mz);
                    if *mix_vol < v_mz {
                        *stag_vol = 0.0; // clear stagnant zone (§6.7.2)
                    }
                }
            } else {
                // Emptying.
                let v_t = stag_vol.min(v_net.abs());
                let w_in = v_in * c_in;
                // Step 2: mix inflow and transferred stagnant into mixing zone.
                let denom = *mix_vol + v_in + v_t;
                if denom > 0.0 {
                    *mix_conc = (*mix_conc * *mix_vol + w_in + *stag_conc * v_t) / denom;
                }
                *stag_vol = (*stag_vol - v_t).max(0.0);
                *mix_vol = (v_mz + v_t + v_net).max(0.0);
            }

            // §6.7.2: apply bulk reactions to each zone after mixing.
            if reactive {
                let c0m = *mix_conc;
                let c0s = *stag_conc;
                let dcm = bulk_rate(kb, order, c0m, conc_limit) * dt;
                let dcs = bulk_rate(kb, order, c0s, conc_limit) * dt;
                *mix_conc = (c0m + dcm).clamp(0.0, C_MAX);
                *stag_conc = (c0s + dcs).clamp(0.0, C_MAX);
                mb.reacted += -(*mix_conc - c0m) * *mix_vol;
                mb.reacted += -(*stag_conc - c0s) * *stag_vol;
                if accumulate_rates {
                    mb.reacted_tank += dcm.abs() * *mix_vol;
                    mb.reacted_tank += dcs.abs() * *stag_vol;
                }
            }

            *mix_conc
        }

        // ── §6.7.3 FIFO plug flow ────────────────────────────────────────────
        TankQuality::Fifo { segments } => {
            // Inflow: push new segment at the back (newest/inlet end).
            if v_in > 0.0 {
                push_segment_merge(
                    segments,
                    Segment {
                        volume: v_in,
                        concentration: c_in,
                    },
                    network.options.quality_tolerance,
                );
            }
            // Outflow: consume from front (oldest/outlet end).
            // Track withdrawn mass/volume for reporting quality (EPANET tankmix3).
            let mut vol_out = v_out;
            let mut vsum = 0.0_f64;
            let mut wsum = 0.0_f64;
            while vol_out > 0.0 {
                if segments.is_empty() {
                    break;
                }
                let is_last = segments.len() == 1;
                let seg = segments.front_mut().unwrap();
                // EPANET: if seg == LastSeg, vseg = vout (last seg absorbs all)
                let vseg = if is_last {
                    vol_out
                } else {
                    vol_out.min(seg.volume)
                };
                vsum += vseg;
                wsum += seg.concentration * vseg;
                vol_out -= vseg;
                if vseg >= seg.volume {
                    segments.pop_front();
                } else {
                    seg.volume -= vseg;
                }
            }
            // EPANET: tank->C = wsum/vsum (withdrawn avg), or first seg, or 0
            if vsum > 0.0 {
                wsum / vsum
            } else if segments.is_empty() {
                0.0
            } else {
                segments.front().unwrap().concentration
            }
        }

        // ── §6.7.4 LIFO stacked layers ───────────────────────────────────────
        TankQuality::Lifo { segments } => {
            // Outflow: consume from top (newest end) first.
            let mut vol_out = v_out;
            while vol_out > 0.0 {
                match segments.last_mut() {
                    None => break,
                    Some(seg) => {
                        let rem = vol_out.min(seg.volume);
                        vol_out -= rem;
                        seg.volume -= rem;
                        if seg.volume <= 0.0 {
                            segments.pop();
                        }
                    }
                }
            }
            // Inflow: push new segment at the top.
            if v_in > 0.0 {
                if let Some(top) = segments.last_mut() {
                    let tol = network.options.quality_tolerance;
                    if tol > 0.0 && (top.concentration - c_in).abs() <= tol {
                        top.volume += v_in; // merge
                    } else {
                        segments.push(Segment {
                            volume: v_in,
                            concentration: c_in,
                        });
                    }
                } else {
                    segments.push(Segment {
                        volume: v_in,
                        concentration: c_in,
                    });
                }
            }
            segments.last().map_or(0.0, |s| s.concentration)
        }
    }
}

/// §6.4.2 Returns the stagnant-node concentration (average of nearest segments).
pub(super) fn stagnant_conc(
    node_0: usize,
    node_links: &[usize],
    network: &Network,
    link_states: &[LinkState],
    pipe_quality: &[Option<PipeQuality>],
) -> f64 {
    let mut sum = 0.0_f64;
    let mut count = 0usize;
    for &k in node_links {
        let link = &network.links[k];
        let pq = match &pipe_quality[k] {
            Some(p) => p,
            None => continue,
        };
        // Use quality flow direction: stagnant (|q| < Q_STAG) treated as
        // positive, matching EPANET's semantics where dir >= 0 means N2 is
        // the downstream end.
        let dir = qual_flow_dir(link_states[k].flow);
        let is_inflow_to_node = (dir >= 0 && link.base.to_idx() == node_0)
            || (dir < 0 && link.base.from_idx() == node_0);
        let c_near = if is_inflow_to_node {
            pq.segments.front()
        } else {
            pq.segments.back()
        };
        if let Some(seg) = c_near {
            sum += seg.concentration;
            count += 1;
        }
    }
    if count > 0 {
        sum / count as f64
    } else {
        0.0
    }
}

/// §6.4.3 Returns the outflow concentration of a reservoir node at time `t`.
pub(super) fn reservoir_source_conc(
    node_0: usize,
    network: &Network,
    node_states: &[NodeState],
    mode: QualityMode,
    t: f64,
) -> f64 {
    match mode {
        QualityMode::Age => 0.0, // reservoirs reset age to 0
        QualityMode::Trace => {
            // 100 % if this is the trace node, 0 % otherwise.
            let node_id = &network.nodes[node_0].base.id;
            if network.options.trace_node.as_deref() == Some(node_id) {
                100.0
            } else {
                0.0
            }
        }
        _ => {
            // CHEMICAL: a Concentration-type source defines the outflow
            // concentration directly (§6.6 full override), evaluated at the
            // current time so patterned sources track their pattern. Other
            // source types (Mass/Setpoint/FlowPaced) adjust the reservoir's
            // baseline — its initial quality — during source injection
            // (§6.6), so they contribute nothing here.
            match &network.nodes[node_0].source {
                Some(src) if matches!(src.kind, crate::SourceType::Concentration) => src
                    .effective_value(
                        t,
                        &network.options,
                        &network.patterns,
                        &network.pattern_index,
                    ),
                Some(_) => network.nodes[node_0].base.initial_quality,
                None => node_states[node_0].quality,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MassBalance;
    use crate::{SimulationOptions, Tank};

    #[test]
    fn reservoir_source_conc_tracks_pattern_and_source_type() {
        use crate::test_support::TestNetworkBuilder;
        use crate::{QualitySource, SourceType};

        let (mut net, ns, _ls) = TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 10.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
            .pattern("SRC", &[1.0, 2.0])
            .node_quality("R1", 0.5)
            .build();

        // A patterned Concentration source must be evaluated at the current
        // time, not frozen at t = 0.
        net.nodes[0].source = Some(QualitySource {
            node: 1,
            kind: SourceType::Concentration,
            base_value: 10.0,
            pattern: Some("SRC".to_string()),
        });
        let t1 = net.options.pattern_step; // second pattern period, factor 2.0
        let c0 = reservoir_source_conc(0, &net, &ns, QualityMode::Chemical, 0.0);
        let c1 = reservoir_source_conc(0, &net, &ns, QualityMode::Chemical, t1);
        assert!((c0 - 10.0).abs() < 1e-12, "expected 10.0, got {c0}");
        assert!((c1 - 20.0).abs() < 1e-12, "expected 20.0, got {c1}");

        // A Mass-type rate (mg/min) must not be used directly as the outflow
        // concentration — the baseline is the reservoir's initial quality and
        // the mass injection is applied later in source injection (§6.6).
        net.nodes[0].source = Some(QualitySource {
            node: 1,
            kind: SourceType::Mass,
            base_value: 600.0,
            pattern: None,
        });
        let c = reservoir_source_conc(0, &net, &ns, QualityMode::Chemical, 0.0);
        assert!((c - 0.5).abs() < 1e-12, "expected baseline 0.5, got {c}");
    }

    #[test]
    fn cstr_tank_mixing_dilution() {
        let tank = Tank {
            min_level: 0.0,
            max_level: 100.0,
            initial_level: 10.0,
            diameter: 11.285,
            min_volume: 0.0,
            volume_curve: None,
            mix_model: crate::MixModel::Cstr,
            mix_fraction: 1.0,
            bulk_coeff: 0.0,
            overflow: false,
        };
        let net = Network {
            title: vec![],
            options: SimulationOptions {
                quality_mode: crate::QualityMode::Chemical,
                bulk_coeff: 0.0,
                ..SimulationOptions::default()
            },
            patterns: vec![],
            curves: vec![],
            nodes: vec![],
            links: vec![],
            controls: vec![],
            rules: vec![],
            pattern_index: std::collections::HashMap::new(),
            report: crate::ReportOptions::default(),
            coordinates: std::collections::HashMap::new(),
            vertices: std::collections::HashMap::new(),
            node_tags: std::collections::HashMap::new(),
            link_tags: std::collections::HashMap::new(),
        };
        let mut tq = Some(TankQuality::Cstr {
            volume: 1000.0,
            conc: 5.0,
        });
        let mut mb = MassBalance::default();
        let c_out = update_tank_mix(
            &mut tq, &net, &tank, 0.0, 10.0, 10.0, 0.0, 0.0, 1.0, 0.0, 1.0, false, &mut mb, false,
        );
        approx::assert_abs_diff_eq!(c_out, 5000.0 / 1010.0, epsilon = 1e-12);
    }
}
