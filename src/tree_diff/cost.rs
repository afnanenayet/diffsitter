//! Structure-aware cost estimates for choosing between TopDiff and
//! TouzetDepth (paper Sect. 6). Both estimate the number of subproblems the
//! respective algorithm would compute, in O(n) over the left-hand tree.

use std::collections::HashMap;

use super::tree::LabeledTree;

/// Estimated cost of TouzetDepth: depth pruning limits each node's
/// contribution to `min(tau, depth(x))`.
pub(crate) fn cost_touzet_depth(t: &LabeledTree, tau: u32) -> u64 {
    (0..t.len())
        .map(|i| (t.depth(i) as u64).min(u64::from(tau)))
        .sum()
}

/// Estimated cost of TopDiff: at most one top node pair exists per leaf
/// pair, so we sum, for every leaf `l`, the size of the largest subtree
/// whose leftmost leaf descendant is `l`.
pub(crate) fn cost_topdiff(t: &LabeledTree) -> u64 {
    let mut largest_by_lld: HashMap<usize, usize> = HashMap::new();
    for i in 0..t.len() {
        largest_by_lld
            .entry(t.lld(i))
            .and_modify(|s| *s = (*s).max(t.size(i)))
            .or_insert(t.size(i));
    }
    largest_by_lld.values().map(|&s| s as u64).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::LabeledTree;

    fn paper_tree_t() -> LabeledTree<'static> {
        LabeledTree::from_postorder(
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
        .unwrap()
    }

    #[test]
    fn touzet_cost_sums_clamped_depths() {
        let t = paper_tree_t();
        // Depths: x0=1 x1=3 x2=2 x3=3 x4=2 x5=1 x6=1 x7=0.
        assert_eq!(cost_touzet_depth(&t, 100), 13); // sum of depths
        assert_eq!(cost_touzet_depth(&t, 2), 1 + 2 + 2 + 2 + 2 + 1 + 1); // clamped at 2
        assert_eq!(cost_touzet_depth(&t, 0), 0);
    }

    #[test]
    fn topdiff_cost_sums_largest_subtree_per_lld() {
        let t = paper_tree_t();
        // Leaves: x0, x1, x3, x6. Largest subtree with lld=0 is the root
        // (size 8); lld=1 is x5 (size 5); lld=3 is x4 (size 2); lld=6 is x6
        // (size 1). Total = 16.
        assert_eq!(cost_topdiff(&t), 16);
    }
}
