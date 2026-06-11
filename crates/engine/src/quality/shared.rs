use std::collections::VecDeque;

/// Physical concentration ceiling (mg/L or equivalent). Used for clamping.
pub(super) const C_MAX: f64 = 1.0e6;

/// Stagnation threshold (m³/s). SI equivalent of EPANET's QZERO = 1.114e-5 ft³/s.
/// Used to decide whether a link flow is stagnant for quality transport purposes (§6.3.1).
pub(super) const Q_STAG: f64 = 3.154e-7;

/// Returns the quality-engine flow direction for a given link flow: +1 positive,
/// −1 negative, 0 stagnant (|q| < Q_STAG). Matches EPANET's FlowDir logic.
pub(super) fn qual_flow_dir(q: f64) -> i8 {
    if q.abs() < Q_STAG {
        0
    } else if q > 0.0 {
        1
    } else {
        -1
    }
}

/// A single Lagrangian parcel of water with uniform constituent concentration (§6.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// Volume of this parcel (m³).
    pub volume: f64,
    /// Constituent concentration (mg/L for CHEMICAL; hours for AGE; % for TRACE).
    pub concentration: f64,
}

/// Quality state for a single pipe (§6.3).
#[derive(Debug, Clone)]
pub struct PipeQuality {
    pub segments: VecDeque<Segment>,
    /// Last-known flow sign: `true` = positive (from_node → to_node).
    pub flow_dir: i8,
}

/// Quality state for a single tank, keyed to its mixing model (§6.7).
#[derive(Debug, Clone)]
pub enum TankQuality {
    Cstr {
        volume: f64,
        conc: f64,
    },
    TwoComp {
        mix_vol: f64,
        mix_conc: f64,
        stag_vol: f64,
        stag_conc: f64,
    },
    Fifo {
        segments: VecDeque<Segment>,
    },
    Lifo {
        segments: Vec<Segment>,
    },
}

/// Errors returned by the quality engine.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum QualityError {
    /// `run_quality` was called when the network's `quality_mode` is `None`.
    ModeNone,
}

pub use crate::io::MassBalance;

pub struct QualityState {
    pub pipe_quality: Vec<Option<PipeQuality>>,
    pub tank_quality: Vec<Option<TankQuality>>,
    pub node_conc: Vec<f64>,
    pub(super) node_links: Vec<Vec<usize>>,
    pub(super) topo_order: Vec<usize>,
    pub(super) adjacency: Vec<Vec<(usize, bool)>>,
    pub(super) flow_dir: Vec<i8>,
    pub(super) needs_topo: bool,
    pub mass_balance: MassBalance,
    pub pipe_rate_coeff: Vec<f64>,
    pub(super) tank_overflows: Vec<bool>,
}

/// Returns the outflow concentration of a tank's quality state.
pub(super) fn tank_outflow_conc(tq: &TankQuality) -> f64 {
    match tq {
        TankQuality::Cstr { conc, .. } => *conc,
        TankQuality::TwoComp { mix_conc, .. } => *mix_conc,
        TankQuality::Fifo { segments } => segments.front().map_or(0.0, |s| s.concentration),
        TankQuality::Lifo { segments } => segments.last().map_or(0.0, |s| s.concentration),
    }
}

/// §6.3.3 Pushes a new segment to the back (upstream end) of a pipe's deque.
pub(super) fn push_segment_merge(segs: &mut VecDeque<Segment>, new: Segment, tol: f64) {
    if tol > 0.0 {
        if let Some(back) = segs.back_mut() {
            if (back.concentration - new.concentration).abs() <= tol {
                back.concentration = (back.concentration * back.volume
                    + new.concentration * new.volume)
                    / (back.volume + new.volume);
                back.volume += new.volume;
                return;
            }
        }
    }
    segs.push_back(new);
}

/// §6.9 Total constituent mass in all pipes and tanks.
pub(super) fn total_mass(state: &QualityState) -> f64 {
    let pipe_mass: f64 = state
        .pipe_quality
        .iter()
        .flatten()
        .flat_map(|pq| pq.segments.iter())
        .map(|s| s.concentration * s.volume)
        .sum();
    let tank_mass: f64 = state
        .tank_quality
        .iter()
        .flatten()
        .map(|tq| match tq {
            TankQuality::Cstr { volume, conc } => conc * volume,
            TankQuality::TwoComp {
                mix_vol,
                mix_conc,
                stag_vol,
                stag_conc,
            } => mix_conc * mix_vol + stag_conc * stag_vol,
            TankQuality::Fifo { segments } => {
                segments.iter().map(|s| s.concentration * s.volume).sum()
            }
            TankQuality::Lifo { segments } => {
                segments.iter().map(|s| s.concentration * s.volume).sum()
            }
        })
        .sum();
    pipe_mass + tank_mass
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── qual_flow_dir ─────────────────────────────────────────────────────────

    #[test]
    fn qual_flow_dir_stagnant_below_threshold() {
        assert_eq!(qual_flow_dir(0.0), 0);
        assert_eq!(qual_flow_dir(Q_STAG * 0.5), 0);
        assert_eq!(qual_flow_dir(-Q_STAG * 0.5), 0);
    }

    #[test]
    fn qual_flow_dir_positive_above_threshold() {
        assert_eq!(qual_flow_dir(Q_STAG * 2.0), 1);
        assert_eq!(qual_flow_dir(1.0), 1);
    }

    #[test]
    fn qual_flow_dir_negative_above_threshold() {
        assert_eq!(qual_flow_dir(-Q_STAG * 2.0), -1);
        assert_eq!(qual_flow_dir(-1.0), -1);
    }

    // ── tank_outflow_conc ─────────────────────────────────────────────────────

    #[test]
    fn tank_outflow_conc_cstr_returns_conc() {
        let tq = TankQuality::Cstr {
            volume: 100.0,
            conc: 0.5,
        };
        assert!((tank_outflow_conc(&tq) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn tank_outflow_conc_two_comp_returns_mix_zone() {
        let tq = TankQuality::TwoComp {
            mix_vol: 50.0,
            mix_conc: 1.2,
            stag_vol: 50.0,
            stag_conc: 0.3,
        };
        assert!((tank_outflow_conc(&tq) - 1.2).abs() < 1e-12);
    }

    #[test]
    fn tank_outflow_conc_fifo_returns_front() {
        let mut segs = VecDeque::new();
        segs.push_back(Segment {
            volume: 10.0,
            concentration: 0.7,
        });
        segs.push_back(Segment {
            volume: 10.0,
            concentration: 0.4,
        });
        let tq = TankQuality::Fifo { segments: segs };
        assert!((tank_outflow_conc(&tq) - 0.7).abs() < 1e-12);
    }

    #[test]
    fn tank_outflow_conc_lifo_returns_last() {
        let tq = TankQuality::Lifo {
            segments: vec![
                Segment {
                    volume: 10.0,
                    concentration: 0.1,
                },
                Segment {
                    volume: 10.0,
                    concentration: 0.9,
                },
            ],
        };
        assert!((tank_outflow_conc(&tq) - 0.9).abs() < 1e-12);
    }

    // ── push_segment_merge ────────────────────────────────────────────────────

    #[test]
    fn push_segment_merge_appends_when_outside_tolerance() {
        let mut segs = VecDeque::new();
        segs.push_back(Segment {
            volume: 1.0,
            concentration: 0.0,
        });
        push_segment_merge(
            &mut segs,
            Segment {
                volume: 2.0,
                concentration: 1.0,
            },
            0.01,
        );
        assert_eq!(segs.len(), 2);
        assert!((segs.back().unwrap().concentration - 1.0).abs() < 1e-12);
    }

    #[test]
    fn push_segment_merge_merges_when_within_tolerance() {
        let mut segs = VecDeque::new();
        segs.push_back(Segment {
            volume: 2.0,
            concentration: 1.0,
        });
        // New segment with concentration 1.005 — within tol=0.01.
        push_segment_merge(
            &mut segs,
            Segment {
                volume: 2.0,
                concentration: 1.005,
            },
            0.01,
        );
        assert_eq!(segs.len(), 1);
        let merged = segs.back().unwrap();
        assert!((merged.volume - 4.0).abs() < 1e-12);
        // Weighted average: (1.0*2 + 1.005*2)/4 = 1.0025.
        assert!((merged.concentration - 1.0025).abs() < 1e-12);
    }

    #[test]
    fn push_segment_merge_zero_tol_always_appends() {
        let mut segs = VecDeque::new();
        segs.push_back(Segment {
            volume: 1.0,
            concentration: 1.0,
        });
        push_segment_merge(
            &mut segs,
            Segment {
                volume: 1.0,
                concentration: 1.0,
            },
            0.0,
        );
        // tol=0 → always append even if identical.
        assert_eq!(segs.len(), 2);
    }
}
