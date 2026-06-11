use std::collections::BTreeSet;

/// Initialise degree doubly-linked lists.
fn mmdint(
    neqns: usize,
    xadj: &[i32],
    dhead: &mut [i32],
    dforw: &mut [i32],
    dbakw: &mut [i32],
    qsize: &mut [i32],
    marker: &mut [i32],
    llist: &mut [i32],
) {
    for node in 1..=neqns {
        dhead[node] = 0;
        qsize[node] = 1;
        marker[node] = 0;
        llist[node] = 0;
    }
    for node in 1..=neqns {
        let ndeg = (xadj[node + 1] - xadj[node] + 1) as usize;
        let fnode = dhead[ndeg];
        dforw[node] = fnode;
        dhead[ndeg] = node as i32;
        if fnode > 0 {
            dbakw[fnode as usize] = node as i32;
        }
        dbakw[node] = -(ndeg as i32);
    }
}

/// Eliminate `mdnode` and perform quotient graph transformation.
fn mmdelm(
    mdnode: usize,
    xadj: &[i32],
    adjncy: &mut [i32],
    dhead: &mut [i32],
    dforw: &mut [i32],
    dbakw: &mut [i32],
    qsize: &mut [i32],
    llist: &mut [i32],
    marker: &mut [i32],
    maxint: i32,
    tag: &mut i32,
) {
    marker[mdnode] = *tag;
    let istrt = xadj[mdnode] as usize;
    let istop = (xadj[mdnode + 1] - 1) as usize;

    let mut elmnt: i32 = 0;
    let mut rloc = istrt;
    let mut rlmt = istop;

    let mut i = istrt;
    while i <= istop {
        let nabor = adjncy[i];
        if nabor == 0 {
            break;
        }
        if marker[nabor as usize] < *tag {
            marker[nabor as usize] = *tag;
            if dforw[nabor as usize] < 0 {
                llist[nabor as usize] = elmnt;
                elmnt = nabor;
            } else {
                adjncy[rloc] = nabor;
                rloc += 1;
            }
        }
        i += 1;
    }

    while elmnt > 0 {
        adjncy[rlmt] = -elmnt;
        let mut link = elmnt as usize;
        'l400: loop {
            let jstrt = xadj[link] as usize;
            let jstop_i32 = xadj[link + 1] - 1;
            if jstop_i32 < jstrt as i32 {
                break 'l400;
            }
            let jstop = jstop_i32 as usize;
            let mut j = jstrt;
            let mut follow = false;
            while j <= jstop {
                let node = adjncy[j];
                if node < 0 {
                    link = (-node) as usize;
                    follow = true;
                    break;
                } else if node == 0 {
                    break;
                } else {
                    if marker[node as usize] < *tag && dforw[node as usize] >= 0 {
                        marker[node as usize] = *tag;
                        while rloc >= rlmt {
                            link = (-adjncy[rlmt]) as usize;
                            rloc = xadj[link] as usize;
                            rlmt = (xadj[link + 1] - 1) as usize;
                        }
                        adjncy[rloc] = node;
                        rloc += 1;
                    }
                }
                j += 1;
            }
            if follow {
                continue 'l400;
            }
            break 'l400;
        }
        elmnt = llist[elmnt as usize];
    }

    if rloc <= rlmt {
        adjncy[rloc] = 0;
    }

    let mut link = mdnode;
    'rloop: loop {
        let istrt2 = xadj[link] as usize;
        let istop2 = (xadj[link + 1] - 1) as usize;
        let mut ii = istrt2;
        while ii <= istop2 {
            let rnode = adjncy[ii];
            if rnode < 0 {
                link = (-rnode) as usize;
                break;
            } else if rnode == 0 {
                break 'rloop;
            }
            let rn = rnode as usize;

            let pvnode = dbakw[rn];
            if pvnode != 0 && pvnode != -maxint {
                let nxnode = dforw[rn];
                if nxnode > 0 {
                    dbakw[nxnode as usize] = pvnode;
                }
                if pvnode > 0 {
                    dforw[pvnode as usize] = nxnode;
                }
                if pvnode < 0 {
                    let npv = (-pvnode) as usize;
                    dhead[npv] = nxnode;
                }
            }

            let jstrt = xadj[rn] as usize;
            let jstop = (xadj[rn + 1] - 1) as usize;
            let mut xqnbr = jstrt;
            let mut jj = jstrt;
            while jj <= jstop {
                let nabor = adjncy[jj];
                if nabor == 0 {
                    break;
                }
                if marker[nabor as usize] < *tag {
                    adjncy[xqnbr] = nabor;
                    xqnbr += 1;
                }
                jj += 1;
            }

            let nqnbrs = xqnbr - jstrt;
            if nqnbrs == 0 {
                qsize[mdnode] += qsize[rn];
                qsize[rn] = 0;
                marker[rn] = maxint;
                dforw[rn] = -(mdnode as i32);
                dbakw[rn] = -maxint;
            } else {
                dforw[rn] = nqnbrs as i32 + 1;
                dbakw[rn] = 0;
                adjncy[xqnbr] = mdnode as i32;
                xqnbr += 1;
                if xqnbr <= jstop {
                    adjncy[xqnbr] = 0;
                }
            }
            ii += 1;
        }
        if ii > istop2 {
            break 'rloop;
        }
    }
}

/// Update degrees after a multiple elimination step.
fn mmdupd(
    ehead: i32,
    neqns: usize,
    xadj: &[i32],
    adjncy: &mut [i32],
    delta: i32,
    mdeg: &mut i32,
    dhead: &mut [i32],
    dforw: &mut [i32],
    dbakw: &mut [i32],
    qsize: &mut [i32],
    llist: &mut [i32],
    marker: &mut [i32],
    maxint: i32,
    tag: &mut i32,
) {
    let mdeg0 = *mdeg + delta;
    let mut elmnt = ehead;

    while elmnt > 0 {
        let mut mtag = *tag + mdeg0;
        if mtag >= maxint {
            *tag = 1;
            for i in 1..=neqns {
                if marker[i] < maxint {
                    marker[i] = 0;
                }
            }
            mtag = *tag + mdeg0;
        }

        let mut q2head: i32 = 0;
        let mut qxhead: i32 = 0;
        let mut deg0: i32 = 0;

        let mut link = elmnt as usize;
        'scan: loop {
            let istrt = xadj[link] as usize;
            let istop = (xadj[link + 1] - 1) as usize;
            let mut i = istrt;
            while i <= istop {
                let enode = adjncy[i];
                if enode < 0 {
                    link = (-enode) as usize;
                    break;
                } else if enode == 0 {
                    break 'scan;
                }
                let en = enode as usize;
                if qsize[en] == 0 {
                    i += 1;
                    continue;
                }
                deg0 += qsize[en];
                marker[en] = mtag;

                if dbakw[en] != 0 {
                    i += 1;
                    continue;
                }
                if dforw[en] == 2 {
                    llist[en] = q2head;
                    q2head = enode;
                } else {
                    llist[en] = qxhead;
                    qxhead = enode;
                }
                i += 1;
            }
            if i > istop {
                break 'scan;
            }
        }

        let mut enode = q2head;
        let mut iq2 = true;

        loop {
            if enode <= 0 {
                if iq2 {
                    enode = qxhead;
                    iq2 = false;
                    if enode <= 0 {
                        break;
                    }
                } else {
                    break;
                }
            }
            let en = enode as usize;
            if dbakw[en] != 0 {
                enode = llist[en];
                continue;
            }
            *tag += 1;
            let mut deg = deg0;

            if iq2 {
                let istrt = xadj[en] as usize;
                let mut nabor = adjncy[istrt];
                if nabor == elmnt {
                    nabor = adjncy[istrt + 1];
                }

                if dforw[nabor as usize] >= 0 {
                    deg += qsize[nabor as usize];
                } else {
                    let mut link2 = nabor as usize;
                    'q2walk: loop {
                        let jstrt = xadj[link2] as usize;
                        let jstop = (xadj[link2 + 1] - 1) as usize;
                        let mut j = jstrt;
                        while j <= jstop {
                            let node = adjncy[j];
                            if node == enode {
                                j += 1;
                                continue;
                            }
                            if node < 0 {
                                link2 = (-node) as usize;
                                break;
                            } else if node == 0 {
                                break 'q2walk;
                            }
                            let nn = node as usize;
                            if qsize[nn] == 0 {
                                j += 1;
                                continue;
                            }
                            if marker[nn] >= *tag {
                                if dbakw[nn] == 0 {
                                    if dforw[nn] == 2 {
                                        qsize[en] += qsize[nn];
                                        qsize[nn] = 0;
                                        marker[nn] = maxint;
                                        dforw[nn] = -(enode);
                                        dbakw[nn] = -maxint;
                                    } else if dbakw[nn] == 0 {
                                        dbakw[nn] = -maxint;
                                    }
                                }
                            } else {
                                marker[nn] = *tag;
                                deg += qsize[nn];
                            }
                            j += 1;
                        }
                        if j > jstop {
                            break;
                        }
                    }
                }
            } else {
                let istrt = xadj[en] as usize;
                let istop = (xadj[en + 1] - 1) as usize;
                let mut i = istrt;
                while i <= istop {
                    let nabor = adjncy[i];
                    if nabor == 0 {
                        break;
                    }
                    if marker[nabor as usize] < *tag {
                        marker[nabor as usize] = *tag;
                        if dforw[nabor as usize] >= 0 {
                            deg += qsize[nabor as usize];
                        } else {
                            let mut link2 = nabor as usize;
                            'qxwalk: loop {
                                let jstrt = xadj[link2] as usize;
                                let jstop = (xadj[link2 + 1] - 1) as usize;
                                let mut j = jstrt;
                                while j <= jstop {
                                    let node = adjncy[j];
                                    if node < 0 {
                                        link2 = (-node) as usize;
                                        break;
                                    } else if node == 0 {
                                        break 'qxwalk;
                                    }
                                    let nn = node as usize;
                                    if marker[nn] < *tag {
                                        marker[nn] = *tag;
                                        deg += qsize[nn];
                                    }
                                    j += 1;
                                }
                                if j > jstop {
                                    break;
                                }
                            }
                        }
                    }
                    i += 1;
                }
            }

            deg = deg - qsize[en] + 1;
            let fnode = dhead[deg as usize];
            dforw[en] = fnode;
            dbakw[en] = -deg;
            if fnode > 0 {
                dbakw[fnode as usize] = enode;
            }
            dhead[deg as usize] = enode;
            if deg < *mdeg {
                *mdeg = deg;
            }

            enode = llist[en];
        }

        *tag = mtag;
        elmnt = llist[elmnt as usize];
    }
}

/// Final numbering of the permutation/inverse-permutation.
fn mmdnum(neqns: usize, perm: &mut [i32], invp: &mut [i32], qsize: &[i32]) {
    for node in 1..=neqns {
        let nqsize = qsize[node];
        if nqsize <= 0 {
            perm[node] = invp[node];
        } else {
            perm[node] = -invp[node];
        }
    }

    for node in 1..=neqns {
        if perm[node] > 0 {
            continue;
        }
        let mut father = node;
        loop {
            if perm[father] > 0 {
                break;
            }
            father = (-perm[father]) as usize;
        }
        let root = father;
        let num = perm[root] + 1;
        invp[node] = -num;
        perm[root] = num;
        let mut father2 = node;
        loop {
            let nextf = (-perm[father2]) as usize;
            if nextf == 0 || perm[father2] >= 0 {
                break;
            }
            perm[father2] = -(root as i32);
            father2 = nextf;
        }
    }

    for node in 1..=neqns {
        let num = (-invp[node]) as usize;
        invp[node] = num as i32;
        perm[num] = node as i32;
    }
}

/// Multiple Minimum Degree ordering, matching EPANET's `genmmd()`.
///
/// Input: adjacency given as `adj[i]` = set of 0-based junction indices
/// adjacent to junction i.
pub(super) fn genmmd_order(
    n: usize,
    adj: &[BTreeSet<usize>],
) -> Result<(Vec<usize>, Vec<usize>), String> {
    if n == 0 {
        return Ok((vec![], vec![]));
    }

    let mut xadj = vec![0i32; n + 2];
    let mut adjncy_vec: Vec<i32> = Vec::new();

    xadj[1] = 1;
    for k in 0..n {
        let node = k + 1;
        for &nbr in &adj[k] {
            let knode = nbr + 1;
            if knode >= 1 && knode <= n {
                adjncy_vec.push(knode as i32);
            }
        }
        xadj[node + 1] = adjncy_vec.len() as i32 + 1;
    }

    let n_adj = adjncy_vec.len();
    adjncy_vec.resize(n_adj + n + 1, 0);

    let mut adjncy = vec![0i32; adjncy_vec.len() + 1];
    for (i, &v) in adjncy_vec.iter().enumerate() {
        adjncy[i + 1] = v;
    }

    let mut invp = vec![0i32; n + 1];
    let mut perm = vec![0i32; n + 1];
    let mut dhead = vec![0i32; n + 1];
    let mut qsize = vec![0i32; n + 1];
    let mut llist = vec![0i32; n + 1];
    let mut marker = vec![0i32; n + 1];

    let maxint = i32::MAX;
    let delta: i32 = -1;

    mmdint(
        n,
        &xadj,
        &mut dhead,
        &mut invp,
        &mut perm,
        &mut qsize,
        &mut marker,
        &mut llist,
    );

    let mut num: i32 = 1;
    let mut nextmd = dhead[1];
    while nextmd > 0 {
        let mdnode = nextmd as usize;
        nextmd = invp[mdnode];
        marker[mdnode] = maxint;
        invp[mdnode] = -num;
        num += 1;
    }

    if num as usize > n {
        mmdnum(n, &mut perm, &mut invp, &qsize);
    } else {
        let mut tag: i32 = 1;
        dhead[1] = 0;
        let mut mdeg: i32 = 2;
        let mut loop_guard = 0usize;
        let max_loops = 4 * n;

        'main: loop {
            loop_guard += 1;
            if loop_guard > max_loops {
                return Err(format!(
                    "genmmd_order: exceeded {max_loops} iterations — likely infinite loop"
                ));
            }
            while dhead[mdeg as usize] <= 0 {
                mdeg += 1;
                if mdeg as usize > n {
                    break 'main;
                }
            }

            let mdlmt = mdeg + delta;
            let mut ehead: i32 = 0;

            loop {
                let mdnode = dhead[mdeg as usize];
                if mdnode <= 0 {
                    mdeg += 1;
                    if mdeg > mdlmt {
                        break;
                    }
                    continue;
                }

                let nextmd = invp[mdnode as usize];
                dhead[mdeg as usize] = nextmd;
                if nextmd > 0 {
                    perm[nextmd as usize] = -mdeg;
                }
                invp[mdnode as usize] = -num;
                if num + qsize[mdnode as usize] > n as i32 {
                    mmdnum(n, &mut perm, &mut invp, &qsize);
                    let order: Vec<usize> = (1..=n).map(|pos| (perm[pos] - 1) as usize).collect();
                    let row: Vec<usize> = (1..=n).map(|node| (invp[node] - 1) as usize).collect();
                    return Ok((order, row));
                }

                tag += 1;
                if tag >= maxint {
                    tag = 1;
                    for i in 1..=n {
                        if marker[i] < maxint {
                            marker[i] = 0;
                        }
                    }
                }

                mmdelm(
                    mdnode as usize,
                    &xadj,
                    &mut adjncy,
                    &mut dhead,
                    &mut invp,
                    &mut perm,
                    &mut qsize,
                    &mut llist,
                    &mut marker,
                    maxint,
                    &mut tag,
                );
                num += qsize[mdnode as usize];
                llist[mdnode as usize] = ehead;
                ehead = mdnode;

                if delta >= 0 {
                    continue;
                }
                break;
            }

            if num as usize > n {
                break 'main;
            }
            mmdupd(
                ehead,
                n,
                &xadj,
                &mut adjncy,
                delta,
                &mut mdeg,
                &mut dhead,
                &mut invp,
                &mut perm,
                &mut qsize,
                &mut llist,
                &mut marker,
                maxint,
                &mut tag,
            );
        }

        mmdnum(n, &mut perm, &mut invp, &qsize);
    }

    let order: Vec<usize> = (1..=n).map(|pos| (perm[pos] - 1) as usize).collect();
    let row: Vec<usize> = (1..=n).map(|node| (invp[node] - 1) as usize).collect();
    Ok((order, row))
}

/// Greedy minimum-degree ordering (§3.6, Phase 1).
///
/// Returns `(order, row)` both 0-based, where `order[k]` is the original node
/// index eliminated at step k, and `row[i]` is the step at which node i is
/// eliminated.
pub(super) fn greedy_mdo(n: usize, orig_adj: &[BTreeSet<usize>]) -> (Vec<usize>, Vec<usize>) {
    let mut adj: Vec<BTreeSet<usize>> = orig_adj.to_vec();
    let mut eliminated = vec![false; n];
    let mut order = Vec::with_capacity(n);
    let mut row = vec![0usize; n];

    for step in 0..n {
        // Pick active node with minimum degree.
        let min_node = (0..n)
            .filter(|&i| !eliminated[i])
            .min_by_key(|&i| adj[i].len())
            .expect("at least one active node remains");

        order.push(min_node);
        row[min_node] = step;
        eliminated[min_node] = true;

        // Add fill edges between all remaining neighbours.
        let nbrs: Vec<usize> = adj[min_node]
            .iter()
            .filter(|&&j| !eliminated[j])
            .copied()
            .collect();
        for i in 0..nbrs.len() {
            for j in (i + 1)..nbrs.len() {
                let u = nbrs[i];
                let v = nbrs[j];
                if adj[u].insert(v) {
                    adj[v].insert(u);
                }
            }
        }
        for &j in &nbrs {
            adj[j].remove(&min_node);
        }
    }
    (order, row)
}

/// Symbolic factorisation via fill-augmentation (§3.6, Phase 2).
///
/// Augments `adj` (original indexing) with all fill edges produced by
/// Cholesky elimination in `order` sequence. After this call, `adj[i]`
/// contains the full below-diagonal sparsity pattern of column i in L.
pub(super) fn symbolic_fill(n: usize, order: &[usize], adj: &mut [BTreeSet<usize>]) {
    let mut eliminated = vec![false; n];
    for &node in order {
        eliminated[node] = true;
        let nbrs: Vec<usize> = adj[node]
            .iter()
            .filter(|&&j| !eliminated[j])
            .copied()
            .collect();
        for i in 0..nbrs.len() {
            for j in (i + 1)..nbrs.len() {
                let u = nbrs[i];
                let v = nbrs[j];
                if adj[u].insert(v) {
                    adj[v].insert(u);
                }
            }
        }
    }
}

/// Builds XLNZ / NZSUB arrays from the filled adjacency (§3.6, Phase 2).
///
/// Returns `(xlnz, nzsub, n_coeff)` where:
/// - `xlnz[k]` = start of column k's entries in `nzsub` (0-based)
/// - `nzsub[pos]` = row index (in permuted ordering)
/// - `n_coeff` = total below-diagonal non-zeros in L
pub(super) fn build_csc(
    n: usize,
    order: &[usize],
    row_perm: &[usize],
    filled_adj: &[BTreeSet<usize>],
) -> (Vec<usize>, Vec<usize>, usize) {
    let mut xlnz = vec![0usize; n + 1];
    let mut nzsub_cols: Vec<Vec<usize>> = vec![Vec::new(); n];

    for col_step in 0..n {
        let orig_col = order[col_step];
        let mut rows: Vec<usize> = filled_adj[orig_col]
            .iter()
            .map(|&orig_row| row_perm[orig_row])
            .filter(|&perm_row| perm_row > col_step)
            .collect();
        rows.sort_unstable();
        xlnz[col_step + 1] = xlnz[col_step] + rows.len();
        nzsub_cols[col_step] = rows;
    }

    let n_coeff = xlnz[n];
    let nzsub: Vec<usize> = nzsub_cols.into_iter().flatten().collect();
    (xlnz, nzsub, n_coeff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_graph(n: usize) -> Vec<BTreeSet<usize>> {
        let mut adj = vec![BTreeSet::new(); n];
        for i in 0..n.saturating_sub(1) {
            adj[i].insert(i + 1);
            adj[i + 1].insert(i);
        }
        adj
    }

    fn cycle_graph(n: usize) -> Vec<BTreeSet<usize>> {
        let mut adj = path_graph(n);
        if n > 2 {
            adj[0].insert(n - 1);
            adj[n - 1].insert(0);
        }
        adj
    }

    fn is_permutation(values: &[usize], n: usize) -> bool {
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        sorted == (0..n).collect::<Vec<_>>()
    }

    #[test]
    fn greedy_mdo_returns_valid_order_and_inverse() {
        let adj = path_graph(4);
        let (order, row) = greedy_mdo(4, &adj);

        assert!(is_permutation(&order, 4));
        assert!(is_permutation(&row, 4));
        for (step, &node) in order.iter().enumerate() {
            assert_eq!(row[node], step);
        }
    }

    #[test]
    fn symbolic_fill_adds_cycle_chord_for_four_cycle() {
        let mut adj = cycle_graph(4);
        let order = vec![0, 1, 2, 3];

        symbolic_fill(4, &order, &mut adj);

        assert!(adj[1].contains(&3));
        assert!(adj[3].contains(&1));
    }

    #[test]
    fn build_csc_tracks_lower_triangle_entries_in_permuted_order() {
        let mut adj = cycle_graph(4);
        let order = vec![0, 1, 2, 3];
        let row_perm = vec![0, 1, 2, 3];
        symbolic_fill(4, &order, &mut adj);

        let (xlnz, nzsub, n_coeff) = build_csc(4, &order, &row_perm, &adj);

        assert_eq!(xlnz, vec![0, 2, 4, 5, 5]);
        assert_eq!(nzsub, vec![1, 3, 2, 3, 3]);
        assert_eq!(n_coeff, nzsub.len());
        assert_eq!(xlnz.last().copied(), Some(n_coeff));
    }

    #[test]
    fn genmmd_order_returns_permutation_and_inverse_for_small_graph() {
        let adj = cycle_graph(5);
        let (order, row) = genmmd_order(5, &adj).expect("ordering should succeed");

        assert!(is_permutation(&order, 5));
        assert!(is_permutation(&row, 5));
        for (step, &node) in order.iter().enumerate() {
            assert_eq!(row[node], step);
        }
    }

    #[test]
    fn genmmd_order_handles_empty_graph() {
        let (order, row) = genmmd_order(0, &[]).expect("empty ordering should succeed");
        assert!(order.is_empty());
        assert!(row.is_empty());
    }
}
