//! TouzetDepth (paper Alg. 2): the tau-bounded baseline with subtree, edits,
//! and depth-based pruning. Quadratic worst case but effective on deep trees,
//! where TopDiff's top-node-pairs approach degrades (Sect. 6).

use super::band::BandMatrix;
use super::ted::{edits_budget, forest_distance};
use super::tree::LabeledTree;

/// Compute the tau-bounded tree edit distance with depth-based pruning.
///
/// If the true distance is <= `tau`, the exact distance is returned
/// (Thm. 7.2). Otherwise the returned value merely exceeds `tau` (it may be
/// an overestimate, up to an internal "infinity"), which is exactly the
/// signal the AutoStop driver needs to double the bound.
#[must_use]
pub fn touzet_depth(old: &LabeledTree, new: &LabeledTree, tau: u32) -> u32 {
    touzet_depth_impl(old, new, tau).0
}

/// As [`touzet_depth`], but also returns the subtree-distance matrix for
/// mapping recovery.
pub(crate) fn touzet_depth_impl(
    old: &LabeledTree,
    new: &LabeledTree,
    tau: u32,
) -> (u32, BandMatrix) {
    debug_assert!(!old.is_empty() && !new.is_empty());
    let (n_old, n_new) = (old.len(), new.len());
    let mut td = BandMatrix::new(n_old, n_new, tau as usize);
    for x in 0..n_old {
        for y in td.row_cols(x) {
            // Subtree pruning: only pairs passing the neighborhood filter.
            let Some(budget) = edits_budget(old, new, x, y, tau) else {
                continue;
            };
            let result = forest_distance(old, new, x, y, budget, true, &td);
            let dist = result.root_distance();
            td.set(x, y, dist);
        }
    }
    (td.get(n_old - 1, n_new - 1), td)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::LabeledTree;

    /// Paper Fig. 1 trees. Labels: l=0 s=1 q=2 u=3 t=4 p=5 v=6 o=7 k=8.
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
    fn paper_example_distance_is_two() {
        let (t, t_prime) = paper_trees();
        // tau = 2 is exactly the true distance; the bound is tight and valid.
        assert_eq!(touzet_depth(&t, &t_prime, 2), 2);
        // Looser bounds must give the same exact answer.
        assert_eq!(touzet_depth(&t, &t_prime, 4), 2);
        assert_eq!(touzet_depth(&t, &t_prime, 16), 2);
    }

    #[test]
    fn identical_trees_have_distance_zero() {
        let (t, _) = paper_trees();
        assert_eq!(touzet_depth(&t, &t, 1), 0);
        assert_eq!(touzet_depth(&t, &t, 8), 0);
    }

    #[test]
    fn insufficient_tau_reports_a_value_above_tau() {
        let (t, t_prime) = paper_trees();
        // tau = 1 < true distance 2: per Lemma 7.1 the result must exceed tau
        // (it need not be the true distance).
        assert!(touzet_depth(&t, &t_prime, 1) > 1);
    }

    #[test]
    fn single_nodes() {
        let a = LabeledTree::from_postorder(&[0], &[None]).unwrap();
        let b = LabeledTree::from_postorder(&[0], &[None]).unwrap();
        let c = LabeledTree::from_postorder(&[9], &[None]).unwrap();
        assert_eq!(touzet_depth(&a, &b, 1), 0);
        assert_eq!(touzet_depth(&a, &c, 1), 1);
    }

    #[test]
    fn completely_relabeled_tree() {
        // Same shape, all four labels changed: distance 4 (four renames).
        let a =
            LabeledTree::from_postorder(&[0, 1, 2, 3], &[Some(3), Some(2), Some(3), None]).unwrap();
        let b =
            LabeledTree::from_postorder(&[4, 5, 6, 7], &[Some(3), Some(2), Some(3), None]).unwrap();
        assert_eq!(touzet_depth(&a, &b, 8), 4);
    }
}
