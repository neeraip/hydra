use crate::{Network, NodeKind};

use super::SparseSolver;

/// Assembles the linear system Ah = F from P/Y coefficients (§3.4 + §3.5).
///
/// Off-diagonal entries in `sparse.aij` are assembled as negative values
/// (EPANET sign convention). Diagonal entries are positive sums of P[k]
/// over all links incident to the junction.
#[allow(clippy::too_many_arguments)]
/// Fused link-level assembly: computes xflow AND matrix coefficients in one pass.
/// Matches EPANET's linkcoeffs which also fuses these operations.
pub(super) fn assemble_links(
    _network: &Network,
    sparse: &mut SparseSolver,
    link_aij_pos: &[Option<usize>],
    node_junc_step_opt: &[Option<usize>],
    p: &[f64],
    y: &[f64],
    flows: &[f64],
    node_heads: &[f64],
    xflow: &mut [f64],
    link_from: &[usize],
    link_to: &[usize],
) {
    xflow.fill(0.0);
    let n_links = p.len();
    for k in 0..n_links {
        let pk = p[k];
        if pk == 0.0 {
            continue;
        }
        let yk = y[k];
        let flow = flows[k];
        let from_node_index = link_from[k];
        let to_node_index = link_to[k];

        xflow[from_node_index] -= flow;
        xflow[to_node_index] += flow;

        if let Some(&pos) = link_aij_pos[k].as_ref() {
            sparse.aij[pos] -= pk;
        }

        match node_junc_step_opt[from_node_index] {
            Some(ji) => {
                let pr = sparse.row[ji];
                sparse.aii[pr] += pk;
                sparse.f[pr] += yk;
            }
            None => {
                if let Some(ji2) = node_junc_step_opt[to_node_index] {
                    let pr2 = sparse.row[ji2];
                    sparse.f[pr2] += pk * node_heads[from_node_index];
                }
            }
        }

        match node_junc_step_opt[to_node_index] {
            Some(ji2) => {
                let pr2 = sparse.row[ji2];
                sparse.aii[pr2] += pk;
                sparse.f[pr2] -= yk;
            }
            None => {
                if let Some(ji) = node_junc_step_opt[from_node_index] {
                    let pr = sparse.row[ji];
                    sparse.f[pr] += pk * node_heads[to_node_index];
                }
            }
        }
    }
}

/// Adds flow-balance residual Δᵢ = Xflow[i] − D_i to each junction's F
/// and subtracts demands from `xflow` in the same pass (§3.4 + §3.5).
///
/// After this call `xflow[i]` for every junction `i` equals the pre-call
/// value minus the demand, which is the value expected by
/// `apply_valve_coefficients` (§3.5) for PRV/PSV active-branch reads.
pub(super) fn assemble_node_residuals(
    network: &Network,
    sparse: &mut SparseSolver,
    node_junc_step_opt: &[Option<usize>],
    demands: &[f64],
    xflow: &mut [f64],
) {
    for (i, node) in network.nodes.iter().enumerate() {
        if let NodeKind::Junction(_) = &node.kind {
            if let Some(ji) = node_junc_step_opt[i] {
                let pr = sparse.row[ji];
                sparse.f[pr] += xflow[i] - demands[i];
                xflow[i] -= demands[i];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::parse;
    use std::collections::BTreeSet;

    fn load_fixture(name: &str) -> crate::Network {
        let path = format!(
            "{}/../../tests/fixtures/{}.inp",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        let input = std::fs::read(path).expect("fixture should be readable");
        parse(&input).expect("fixture should parse")
    }

    #[test]
    fn assemble_links_accumulates_boundary_contribution_for_pipe() {
        let network = load_fixture("single_pipe_hw");
        let mut sparse = SparseSolver::new(1, &[BTreeSet::new()]);
        let mut xflow = vec![0.0; network.nodes.len()];
        let link = &network.links[0];
        let heads = vec![123.0, 99.0];
        let mut node_junc_step_opt = vec![None; network.nodes.len()];
        let junction_node = network
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| matches!(node.kind, NodeKind::Junction(_)).then_some(idx))
            .expect("fixture should contain one junction");
        node_junc_step_opt[junction_node] = Some(0);

        assemble_links(
            &network,
            &mut sparse,
            &[None],
            &node_junc_step_opt,
            &[4.0],
            &[7.5],
            &[2.5],
            &heads,
            &mut xflow,
            &[link.base.from_idx()],
            &[link.base.to_idx()],
        );

        assert_eq!(sparse.aii, vec![4.0]);
        let boundary_node = if link.base.from_idx() == junction_node {
            link.base.to_idx()
        } else {
            link.base.from_idx()
        };
        let expected_y = if link.base.from_idx() == junction_node {
            7.5
        } else {
            -7.5
        };
        assert_eq!(sparse.f, vec![4.0 * heads[boundary_node] + expected_y]);
        assert_eq!(xflow[link.base.from_idx()], -2.5);
        assert_eq!(xflow[link.base.to_idx()], 2.5);
        assert_ne!(link.base.from_idx(), link.base.to_idx());
    }

    #[test]
    fn assemble_node_residuals_only_updates_junction_steps() {
        let network = load_fixture("single_pipe_hw");
        let mut sparse = SparseSolver::new(1, &[BTreeSet::new()]);
        let mut xflow = vec![1.5, -2.0];
        let demands = vec![0.5, -0.5];
        let mut node_junc_step_opt = vec![None; network.nodes.len()];
        let junction_node = network
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| matches!(node.kind, NodeKind::Junction(_)).then_some(idx))
            .expect("fixture should contain one junction");
        node_junc_step_opt[junction_node] = Some(0);
        let xflow_before = xflow[junction_node];

        assemble_node_residuals(&network, &mut sparse, &node_junc_step_opt, &demands, &mut xflow);

        let row = sparse.row[0];
        // RHS gets xflow_before - demand.
        assert_eq!(sparse.f[row], xflow_before - demands[junction_node]);
        // xflow is updated in-place for junctions.
        assert_eq!(xflow[junction_node], xflow_before - demands[junction_node]);
    }
}
