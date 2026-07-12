//! TopDiff (paper Alg. 3 + 4): computes forest distances only for "top node
//! pairs", guaranteeing that no subproblem is solved more than once — the
//! paper's core contribution over Touzet's algorithm.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::band::BandMatrix;
use super::ted::{forest_distance, neighborhood_distance};
use super::tree::LabeledTree;

/// Compute the top node pairs for the given bound (paper Alg. 3, linear
/// time). Returned in construction order (x-major decreasing postorder);
/// callers that need ascending processing order must sort.
pub(crate) fn compute_top_node_pairs(
    old: &LabeledTree,
    new: &LabeledTree,
    tau: u32,
) -> Vec<(usize, usize)> {
    let mut tn: Vec<(usize, usize)> = Vec::new();
    // An empty tree has no nodes to pair, and the `new.len() - 1` below would
    // underflow. The CST builder yields empty trees for whitespace-only input.
    if old.is_empty() || new.is_empty() {
        return tn;
    }
    // Key: (lld(x), lld(y)) -> index of the pair in `tn`.
    let mut seen: HashMap<(usize, usize), usize> = HashMap::new();
    for x in (0..old.len()).rev() {
        let lo = x.saturating_sub(tau as usize);
        let hi = (x + tau as usize).min(new.len() - 1);
        for y in (lo..=hi).rev() {
            if neighborhood_distance(old, new, x, y) > u64::from(tau) {
                continue;
            }
            match seen.entry((old.lld(x), new.lld(y))) {
                Entry::Occupied(entry) => {
                    // Keep the first (largest) x; adopt a later y only if it
                    // sits higher in the tree (paper Alg. 3, lines 6-7).
                    let idx = *entry.get();
                    if y > tn[idx].1 {
                        tn[idx].1 = y;
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(tn.len());
                    tn.push((x, y));
                }
            }
        }
    }
    tn
}

/// Compute the tau-bounded tree edit distance using top node pairs
/// (paper Alg. 4). Exact when the true distance is <= `tau` (Thm. 7.2).
#[must_use]
pub fn topdiff(old: &LabeledTree, new: &LabeledTree, tau: u32) -> u32 {
    topdiff_impl(old, new, tau).0
}

/// As [`topdiff`], but also returns the subtree-distance matrix for mapping
/// recovery.
pub(crate) fn topdiff_impl(old: &LabeledTree, new: &LabeledTree, tau: u32) -> (u32, BandMatrix) {
    debug_assert!(!old.is_empty() && !new.is_empty());
    let mut td = BandMatrix::new(old.len(), new.len(), tau as usize);
    let mut tn = compute_top_node_pairs(old, new, tau);
    // Ascending postorder guarantees the TD values a pair reads were already
    // produced by smaller pairs (correctness argument of Thm. 5.3).
    tn.sort_unstable();
    for &(x, y) in &tn {
        // Top node pairs pass the neighborhood filter by construction.
        debug_assert!(
            neighborhood_distance(old, new, x, y) <= u64::from(tau),
            "top node pair ({x}, {y}) failed the neighborhood filter"
        );
        // Band the forest distance by the full bound `tau`, NOT the tighter
        // per-pair edits budget `eps(x, y, tau)`. Touzet reprocesses every
        // subtree pair, so its FD only needs a band wide enough for that
        // pair's own root cell -- `eps` suffices. TopDiff instead relies on
        // this FD to also produce, as anchored by-products, the subtree
        // distances of every interior pair `(i, j) <l (x, y)` that a later
        // pair (or the root) reads. An interior pair matters only when it is
        // relevant (`||v_i - v_j|| <= tau`), and a relevant anchored cell sits
        // at local band offset `|d(i) - d(j)| <= ||v_i - v_j|| <= tau`. So a
        // `tau`-wide band is exactly wide enough; the narrower `eps` band can
        // leave an interior distance uncomputed (e.g. `eps = 0` on a chain),
        // which would make TopDiff overestimate and violate Lemma 7.1.
        let result = forest_distance(old, new, x, y, tau, false, &td);
        for &(gi, gj, dist) in &result.anchored {
            // A relevant anchored cell has `|gi - gj| <= tau` and so lands in
            // TD's band; the `tau`-wide FD can also surface irrelevant cells
            // (`|gi - gj|` up to `2*tau`) that no one reads -- drop those.
            if gi.abs_diff(gj) <= td.half_width() {
                td.set(gi, gj, dist);
            }
        }
    }
    (td.get(old.len() - 1, new.len() - 1), td)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::LabeledTree;

    fn paper_trees() -> (LabeledTree<'static>, LabeledTree<'static>) {
        let t = LabeledTree::from_postorder(
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[
                Some(7),
                Some(2),
                Some(5),
                Some(4),
                Some(5),
                Some(7),
                Some(7),
                None,
            ],
        )
        .unwrap();
        let t_prime = LabeledTree::from_postorder(
            &[1, 2, 3, 4, 5, 8, 6, 7],
            &[
                Some(1),
                Some(4),
                Some(3),
                Some(4),
                Some(7),
                Some(6),
                Some(7),
                None,
            ],
        )
        .unwrap();
        (t, t_prime)
    }

    #[test]
    fn top_node_pairs_match_paper_example_5_2() {
        let (t, t_prime) = paper_trees();
        let mut tn = compute_top_node_pairs(&t, &t_prime, 2);
        tn.sort_unstable();
        assert_eq!(tn, vec![(2, 3), (4, 3), (5, 4), (6, 6), (7, 7)]);
    }

    #[test]
    fn paper_example_distance_is_two() {
        let (t, t_prime) = paper_trees();
        assert_eq!(topdiff(&t, &t_prime, 2), 2);
        assert_eq!(topdiff(&t, &t_prime, 8), 2);
    }

    #[test]
    fn identical_trees_have_distance_zero() {
        let (t, _) = paper_trees();
        assert_eq!(topdiff(&t, &t, 1), 0);
    }

    #[test]
    fn insufficient_tau_reports_a_value_above_tau() {
        let (t, t_prime) = paper_trees();
        assert!(topdiff(&t, &t_prime, 1) > 1);
    }

    /// Regression for the edits-budget band width. A chain mapped into a
    /// branchy tree forces a top node pair with `eps = 0` whose forest
    /// distance must nonetheless surface an off-diagonal interior subtree
    /// distance. Banding that FD by `eps` (rather than `tau`) leaves the
    /// interior distance uncomputed and makes TopDiff return 4 instead of 2
    /// at the exact bound, violating Lemma 7.1.
    #[test]
    fn tight_bound_needs_off_diagonal_interior_distance() {
        // a: a left chain, root(0) -> (0) -> leaf(1).
        let a = LabeledTree::from_postorder(&[1, 0, 0], &[Some(1), Some(2), None]).unwrap();
        // b: root(0) with children leaf(0) and an inner (0) whose children are
        // leaf(1) and leaf(0).
        let b = LabeledTree::from_postorder(
            &[0, 1, 0, 0, 0],
            &[Some(4), Some(3), Some(3), Some(4), None],
        )
        .unwrap();
        // True distance is 2 (map the three chain nodes onto matching labels,
        // insert the two extra `b` nodes). tau = 2 is the tightest valid bound.
        assert_eq!(topdiff(&a, &b, 2), 2);
    }

    #[test]
    fn empty_tree_yields_no_top_node_pairs() {
        let empty = LabeledTree::from_postorder(&[], &[]).unwrap();
        let (t, _) = paper_trees();
        assert!(compute_top_node_pairs(&empty, &t, 2).is_empty());
        assert!(compute_top_node_pairs(&t, &empty, 2).is_empty());
        assert!(compute_top_node_pairs(&empty, &empty, 2).is_empty());
    }
}
