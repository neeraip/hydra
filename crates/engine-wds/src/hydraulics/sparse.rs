use std::{
    collections::{BTreeSet, HashMap},
    time::Instant,
};

use super::diagnostics::{solve_timing_enabled, SparsePhaseTimings};
use super::sparse_order::{build_csc, genmmd_order, greedy_mdo, symbolic_fill};

/// Sparse symmetric positive-definite solver (§3.6).
///
/// Implements the three-phase algorithm described in §3.6:
/// 1. MMD reordering (done once in `new`)
/// 2. Symbolic factorisation (done once in `new`)
/// 3. Numerical factorisation + solve (`factorize_solve`, called every Newton step)
///
/// Matrix values are assembled externally: call `clear` to zero all entries,
/// set `aii[row[ji]]` and `aij[link_aij_pos[k]]` for each junction and link,
/// then call `factorize_solve` which overwrites `aii`/`aij` with L and stores
/// the solution in `f`.
pub struct SparseSolver {
    /// Number of junctions (system size).
    pub(crate) n: usize,
    /// Total below-diagonal non-zeros in L.
    #[allow(dead_code)]
    pub(crate) n_coeff: usize,
    /// Inverse permutation: `row[i]` = elimination step for original junction i.
    pub(crate) row: Vec<usize>,
    /// Column pointer: `xlnz[k]` = start of column k in `nzsub` (0-based).
    pub(crate) xlnz: Vec<usize>,
    /// Row indices of non-zeros (permuted ordering, sorted per column).
    pub(crate) nzsub: Vec<usize>,
    /// Off-diagonal values, size `n_coeff`. Overwritten in-place during factorisation.
    pub(crate) aij: Vec<f64>,
    /// Diagonal values, size `n`. Overwritten with L[j,j] during factorisation.
    pub(crate) aii: Vec<f64>,
    /// RHS on input, solution (permuted H) on output.
    pub(crate) f: Vec<f64>,
    // Working arrays for the factorisation kernel.
    pub(super) link_chain: Vec<usize>,
    pub(super) first_ptr: Vec<usize>,
    pub(super) temp: Vec<f64>,
    pub(super) last_timings: SparsePhaseTimings,
}

impl SparseSolver {
    /// Constructs the solver for the given junction adjacency graph.
    ///
    /// `adj[i]` = set of 0-based junction indices adjacent to junction i.
    pub fn new(n: usize, adj: &[BTreeSet<usize>]) -> Self {
        if n == 0 {
            return SparseSolver {
                n: 0,
                n_coeff: 0,
                row: vec![],
                xlnz: vec![0],
                nzsub: vec![],
                aij: vec![],
                aii: vec![],
                f: vec![],
                link_chain: vec![],
                first_ptr: vec![],
                temp: vec![],
                last_timings: SparsePhaseTimings::default(),
            };
        }
        let (order, row) = match genmmd_order(n, adj) {
            Ok(result) => result,
            Err(_) => greedy_mdo(n, adj),
        };
        let mut filled_adj = adj.to_vec();
        symbolic_fill(n, &order, &mut filled_adj);
        let (xlnz, nzsub, n_coeff) = build_csc(n, &order, &row, &filled_adj);

        SparseSolver {
            n,
            n_coeff,
            row,
            xlnz,
            nzsub,
            aij: vec![0.0; n_coeff],
            aii: vec![0.0; n],
            f: vec![0.0; n],
            link_chain: vec![n; n],
            first_ptr: vec![0; n],
            temp: vec![0.0; n],
            last_timings: SparsePhaseTimings::default(),
        }
    }

    /// Resets all matrix values to zero before assembly.
    pub fn clear(&mut self) {
        self.aij.fill(0.0);
        self.aii.fill(0.0);
        self.f.fill(0.0);
    }

    /// Builds a lookup map from `(permuted_col, permuted_row)` → position in `aij`.
    ///
    /// Used at construction time to compute `link_aij_pos` in `SolverContext`.
    #[allow(dead_code)]
    pub fn pos_map(&self) -> HashMap<(usize, usize), usize> {
        let mut map = HashMap::with_capacity(self.n_coeff);
        for col in 0..self.n {
            for pos in self.xlnz[col]..self.xlnz[col + 1] {
                let r = self.nzsub[pos];
                map.insert((col, r), pos);
            }
        }
        map
    }

    /// Numerically factorises A = LLᵀ and solves Lz = F, Lᵀx = z in place (§3.6).
    ///
    /// On entry: `aii` = diagonal of A, `aij` = off-diagonal entries (assembled as
    /// negative values, matching EPANET's sign convention), `f` = RHS.
    /// On success: `f` holds the solution (permuted head vector).
    /// Returns `Ok(())` on success, or `Err(step)` with the zero-based
    /// elimination step at which a non-positive pivot was found (the matrix is
    /// ill-conditioned). Map `step` through the inverse permutation (`row`) to
    /// recover the original junction index.
    pub fn factorize_solve(&mut self) -> Result<(), usize> {
        if solve_timing_enabled() {
            return self.factorize_solve_timed();
        }

        self.factorize_solve_fast()
    }

    fn factorize_solve_fast(&mut self) -> Result<(), usize> {
        self.factorize_solve_scalar()
    }

    /// Scalar left-looking Cholesky factorisation + solve (§3.6 EPANET kernel).
    fn factorize_solve_scalar(&mut self) -> Result<(), usize> {
        let n = self.n;
        self.link_chain.fill(n);
        self.first_ptr.fill(0);
        // `temp` is zeroed entry-by-entry during the column scatter loop below
        // (each written entry is reset to 0.0 before moving to the next column).
        // By the time this function returns, `temp` is all-zeros again, so the
        // upfront fill is redundant on every call after the first.
        debug_assert!(
            self.temp.iter().all(|&v| v == 0.0),
            "factorize_solve_scalar: temp buffer not clean on entry"
        );

        let xlnz = &self.xlnz;
        let nzsub = &self.nzsub;
        let aij = &mut self.aij;
        let aii = &mut self.aii;
        let f = &mut self.f;
        let temp = &mut self.temp;
        let link_chain = &mut self.link_chain;
        let first_ptr = &mut self.first_ptr;

        debug_assert!(self.nzsub.iter().all(|&s| s < n));
        debug_assert!(*xlnz.last().unwrap_or(&0) <= aij.len());

        for j in 0..n {
            let mut diagj = 0.0f64;
            let mut k = link_chain[j];
            while k != n {
                let newk = link_chain[k];
                let kfirst = first_ptr[k];
                let ljk = aij[kfirst];
                diagj += ljk * ljk;
                let istrt = kfirst + 1;
                let istop = xlnz[k + 1];
                if istrt < istop {
                    first_ptr[k] = istrt;
                    let isub = nzsub[istrt];
                    link_chain[k] = link_chain[isub];
                    link_chain[isub] = k;
                    // SAFETY: `istrt..istop` are within `aij` and `nzsub` (both
                    // length `n_coeff`, bounded by `xlnz[k+1]` ≤ n_coeff).
                    // `nzsub` entries are all < n, so `temp.get_unchecked_mut(isub)`
                    // is in bounds. The debug_assert! above validates both invariants.
                    unsafe {
                        let mut row_ptr = nzsub.as_ptr().add(istrt);
                        let mut val_ptr = aij.as_ptr().add(istrt);
                        let row_end = nzsub.as_ptr().add(istop);
                        while row_ptr != row_end {
                            let isub = *row_ptr;
                            *temp.get_unchecked_mut(isub) += *val_ptr * ljk;
                            row_ptr = row_ptr.add(1);
                            val_ptr = val_ptr.add(1);
                        }
                    }
                }
                k = newk;
            }

            diagj = aii[j] - diagj;
            if diagj <= 0.0 {
                // The column scatter above may have left partial sums in
                // `temp`; restore the all-zeros invariant so the caller can
                // safely retry factorisation after adjusting the matrix.
                temp.fill(0.0);
                return Err(j);
            }
            diagj = diagj.sqrt();
            aii[j] = diagj;

            let istrt = xlnz[j];
            let istop = xlnz[j + 1];
            if istrt < istop {
                first_ptr[j] = istrt;
                let isub = nzsub[istrt];
                link_chain[j] = link_chain[isub];
                link_chain[isub] = j;
                // SAFETY: same invariants as the first unsafe block above:
                // `istrt..istop` ⊆ `0..n_coeff`, all `nzsub` entries < n,
                // so all `temp` / `aij` / `f` accesses stay in bounds.
                unsafe {
                    let mut row_ptr = nzsub.as_ptr().add(istrt);
                    let mut val_ptr = aij.as_mut_ptr().add(istrt);
                    let row_end = nzsub.as_ptr().add(istop);
                    while row_ptr != row_end {
                        let isub = *row_ptr;
                        let bj = (*val_ptr - *temp.get_unchecked(isub)) / diagj;
                        *val_ptr = bj;
                        *temp.get_unchecked_mut(isub) = 0.0;
                        row_ptr = row_ptr.add(1);
                        val_ptr = val_ptr.add(1);
                    }
                }
            }
        }

        for j in 0..n {
            let bj = f[j] / aii[j];
            f[j] = bj;
            // SAFETY: `i` ranges over `xlnz[j]..xlnz[j+1]` ⊆ `0..n_coeff`;
            // `nzsub[i] < n` (debug_assert! above) so `f.get_unchecked_mut` is valid.
            unsafe {
                let mut row_ptr = nzsub.as_ptr().add(xlnz[j]);
                let mut val_ptr = aij.as_ptr().add(xlnz[j]);
                let row_end = nzsub.as_ptr().add(xlnz[j + 1]);
                while row_ptr != row_end {
                    let isub = *row_ptr;
                    *f.get_unchecked_mut(isub) -= *val_ptr * bj;
                    row_ptr = row_ptr.add(1);
                    val_ptr = val_ptr.add(1);
                }
            }
        }

        for j in (0..n).rev() {
            let mut bj = f[j];
            // SAFETY: same range invariants as the forward solve above;
            // `f.get_unchecked(isub)` is valid because all `nzsub` entries < n.
            unsafe {
                let mut row_ptr = nzsub.as_ptr().add(xlnz[j]);
                let mut val_ptr = aij.as_ptr().add(xlnz[j]);
                let row_end = nzsub.as_ptr().add(xlnz[j + 1]);
                while row_ptr != row_end {
                    let isub = *row_ptr;
                    bj -= *val_ptr * *f.get_unchecked(isub);
                    row_ptr = row_ptr.add(1);
                    val_ptr = val_ptr.add(1);
                }
            }
            f[j] = bj / aii[j];
        }

        Ok(())
    }

    fn factorize_solve_timed(&mut self) -> Result<(), usize> {
        let mut timings = SparsePhaseTimings::default();
        let n = self.n;

        let phase_started = Instant::now();
        self.link_chain.fill(n);
        self.first_ptr.fill(0);
        self.temp.fill(0.0);
        timings.reset += phase_started.elapsed();

        debug_assert!(self.nzsub.iter().all(|&s| s < n));
        debug_assert!(*self.xlnz.last().unwrap_or(&0) <= self.aij.len());

        let phase_started = Instant::now();
        for j in 0..n {
            let mut diagj = 0.0f64;
            let mut k = self.link_chain[j];
            while k != n {
                let newk = self.link_chain[k];
                let kfirst = self.first_ptr[k];
                let ljk = self.aij[kfirst];
                diagj += ljk * ljk;
                let istrt = kfirst + 1;
                let istop = self.xlnz[k + 1];
                if istrt < istop {
                    self.first_ptr[k] = istrt;
                    let isub = self.nzsub[istrt];
                    self.link_chain[k] = self.link_chain[isub];
                    self.link_chain[isub] = k;
                    // SAFETY: same as the factorisation inner loop above:
                    // all indices bounded by CSC structure invariants, nzsub entries < n.                    // SAFETY: `istrt..istop` ⊆ `0..n_coeff`; `nzsub` entries < n,
                    // so `temp.get_unchecked_mut(isub)` / `aij.get_unchecked(i)` are valid.
                    unsafe {
                        for i in istrt..istop {
                            let isub = *self.nzsub.get_unchecked(i);
                            *self.temp.get_unchecked_mut(isub) += *self.aij.get_unchecked(i) * ljk;
                        }
                    }
                }
                k = newk;
            }

            diagj = self.aii[j] - diagj;
            if diagj <= 0.0 {
                // Restore the all-zeros `temp` invariant (see the scalar path).
                self.temp.fill(0.0);
                return Err(j);
            }
            diagj = diagj.sqrt();
            self.aii[j] = diagj;

            let istrt = self.xlnz[j];
            let istop = self.xlnz[j + 1];
            if istrt < istop {
                self.first_ptr[j] = istrt;
                let isub = self.nzsub[istrt];
                self.link_chain[j] = self.link_chain[isub];
                self.link_chain[isub] = j;
                unsafe {
                    for i in istrt..istop {
                        let isub = *self.nzsub.get_unchecked(i);
                        let bj =
                            (*self.aij.get_unchecked(i) - *self.temp.get_unchecked(isub)) / diagj;
                        *self.aij.get_unchecked_mut(i) = bj;
                        *self.temp.get_unchecked_mut(isub) = 0.0;
                    }
                }
            }
        }
        timings.factor += phase_started.elapsed();

        let phase_started = Instant::now();
        for j in 0..n {
            let bj = self.f[j] / self.aii[j];
            self.f[j] = bj;
            // SAFETY: `i` ∈ `xlnz[j]..xlnz[j+1]` ⊆ `0..n_coeff`;
            // `nzsub[i] < n` (debug_assert! above) so f/nzsub/aij accesses are valid.
            unsafe {
                for i in self.xlnz[j]..self.xlnz[j + 1] {
                    let isub = *self.nzsub.get_unchecked(i);
                    *self.f.get_unchecked_mut(isub) -= *self.aij.get_unchecked(i) * bj;
                }
            }
        }
        timings.forward += phase_started.elapsed();

        let phase_started = Instant::now();
        for j in (0..n).rev() {
            let mut bj = self.f[j];
            // SAFETY: same invariants as forward solve above.
            unsafe {
                for i in self.xlnz[j]..self.xlnz[j + 1] {
                    let isub = *self.nzsub.get_unchecked(i);
                    bj -= *self.aij.get_unchecked(i) * *self.f.get_unchecked(isub);
                }
            }
            self.f[j] = bj / self.aii[j];
        }

        self.last_timings = timings;
        self.last_timings.backward += phase_started.elapsed();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_solver_new_zero_system_has_empty_storage() {
        let solver = SparseSolver::new(0, &[]);
        assert_eq!(solver.n, 0);
        assert_eq!(solver.n_coeff, 0);
        assert_eq!(solver.xlnz, vec![0]);
        assert!(solver.aij.is_empty());
        assert!(solver.aii.is_empty());
    }

    #[test]
    fn sparse_solver_clear_resets_matrix_and_rhs() {
        let adj = vec![BTreeSet::from([1usize]), BTreeSet::from([0usize])];
        let mut solver = SparseSolver::new(2, &adj);
        solver.aij.fill(3.0);
        solver.aii.fill(4.0);
        solver.f.fill(5.0);
        solver.clear();
        assert!(solver.aij.iter().all(|v| *v == 0.0));
        assert!(solver.aii.iter().all(|v| *v == 0.0));
        assert!(solver.f.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn factorize_failure_reports_step_and_leaves_temp_clean() {
        // Triangle graph 0-1-2 so that the first eliminated column has two
        // below-diagonal entries: the failing column then has partial sums
        // scattered into `temp` before the non-positive pivot is detected.
        let adj = vec![
            BTreeSet::from([1usize, 2]),
            BTreeSet::from([0usize, 2]),
            BTreeSet::from([0usize, 1]),
        ];
        let mut solver = SparseSolver::new(3, &adj);

        // A = [[1,-2,-2],[-2,1,-2],[-2,-2,1]] is not positive-definite:
        // column 0 factorises (pivot 1) but column 1's pivot is 1 - 4 = -3.
        solver.clear();
        solver.aii.fill(1.0);
        solver.aij.fill(-2.0);
        solver.f.fill(1.0);
        assert_eq!(solver.factorize_solve(), Err(1));
        assert!(
            solver.temp.iter().all(|&v| v == 0.0),
            "temp buffer must be restored to all-zeros on factorisation failure"
        );

        // A subsequent factorisation with a well-conditioned matrix must
        // succeed and produce the correct solution (buffer not corrupted).
        solver.clear();
        solver.aii.fill(4.0);
        solver.aij.fill(-1.0);
        solver.f.fill(2.0);
        assert_eq!(solver.factorize_solve(), Ok(()));
        // A = [[4,-1,-1],[-1,4,-1],[-1,-1,4]], b = [2,2,2] → x = [1,1,1].
        for &x in &solver.f {
            assert!((x - 1.0).abs() < 1e-12, "expected 1.0, got {x}");
        }
    }

    #[test]
    fn sparse_solver_pos_map_contains_lower_triangle_edge() {
        let adj = vec![BTreeSet::from([1usize]), BTreeSet::from([0usize])];
        let solver = SparseSolver::new(2, &adj);
        let map = solver.pos_map();
        assert_eq!(map.len(), 1);
        assert!(map.contains_key(&(0, 1)) || map.contains_key(&(1, 0)));
    }
}
