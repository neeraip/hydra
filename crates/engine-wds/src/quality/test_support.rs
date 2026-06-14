use crate::{
    DemandCategory, Junction, Link, LinkBase, LinkKind, LinkState, LinkStatus, Node, NodeBase,
    NodeKind, Pipe,
};

pub(super) fn default_pipe(length: f64, diameter: f64) -> Pipe {
    Pipe {
        length,
        diameter,
        roughness: 100.0,
        minor_loss: 0.0,
        check_valve: false,
        bulk_coeff: None,
        wall_coeff: None,
        leak_coeff_1: 0.0,
        leak_coeff_2: 0.0,
    }
}

pub(super) fn junction_node(index: usize, elev: f64) -> Node {
    Node {
        base: NodeBase {
            id: format!("J{index}"),
            index,
            elevation: elev,
            initial_quality: 0.0,
        },
        kind: NodeKind::Junction(Junction {
            demands: vec![DemandCategory {
                base_demand: 0.0,
                pattern: None,
                name: None,
            }],
            emitter_coeff: 0.0,
            emitter_exp: 0.5,
        }),
        source: None,
    }
}

pub(super) fn link(index: usize, from: usize, to: usize, pipe: Pipe) -> Link {
    Link {
        base: LinkBase {
            id: format!("P{index}"),
            index,
            from_node: from,
            to_node: to,
            initial_status: LinkStatus::Open,
            initial_setting: Some(1.0),
        },
        kind: LinkKind::Pipe(pipe),
    }
}

pub(super) fn link_state_q(flow: f64) -> LinkState {
    LinkState {
        flow,
        status: LinkStatus::Open,
        setting: 1.0,
        quality: 0.0,
        reaction_rate: 0.0,
    }
}
