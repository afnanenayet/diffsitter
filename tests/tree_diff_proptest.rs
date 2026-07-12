//! Property tests for the tree diff engine against a naive Zhang–Shasha
//! oracle. The oracle uses full matrices and no pruning — slow but obviously
//! correct — so any disagreement is a bug in the banded/pruned algorithms.

use libdiffsitter::tree_diff::{
    LabeledTree, TreeDiffOptions, edit_mapping, topdiff, touzet_depth, tree_edit_distance,
};
use proptest::prelude::*;

const INF: u32 = u32::MAX / 2;

/// Keyroots: for each distinct leftmost-leaf descendant, the node with the
/// largest postorder ID having that lld (includes the root).
fn keyroots(t: &LabeledTree) -> Vec<usize> {
    let mut best_by_lld = std::collections::HashMap::new();
    for i in 0..t.len() {
        best_by_lld
            .entry(t.lld(i))
            .and_modify(|b: &mut usize| *b = (*b).max(i))
            .or_insert(i);
    }
    let mut roots: Vec<usize> = best_by_lld.into_values().collect();
    roots.sort_unstable();
    roots
}

/// Textbook Zhang–Shasha tree edit distance (unit costs, rename-free when
/// labels match). Reference: Zhang & Shasha 1989, as recapped in the paper's
/// Alg. 1.
fn zs_oracle(a: &LabeledTree, b: &LabeledTree) -> u32 {
    match (a.len(), b.len()) {
        (0, nb) => return nb as u32,
        (na, 0) => return na as u32,
        _ => {}
    }
    let mut td = vec![vec![INF; b.len()]; a.len()];
    for &kx in &keyroots(a) {
        for &ky in &keyroots(b) {
            let (lx, ly) = (a.lld(kx), b.lld(ky));
            let (m, n) = (kx - lx + 1, ky - ly + 1);
            let mut fd = vec![vec![0u32; n + 1]; m + 1];
            for i in 1..=m {
                fd[i][0] = fd[i - 1][0] + 1;
            }
            for j in 1..=n {
                fd[0][j] = fd[0][j - 1] + 1;
            }
            for i in 1..=m {
                let gi = lx + i - 1;
                for j in 1..=n {
                    let gj = ly + j - 1;
                    let del = fd[i - 1][j] + 1;
                    let ins = fd[i][j - 1] + 1;
                    if a.lld(gi) == lx && b.lld(gj) == ly {
                        let rename = u32::from(a.label(gi) != b.label(gj));
                        fd[i][j] = del.min(ins).min(fd[i - 1][j - 1] + rename);
                        td[gi][gj] = fd[i][j];
                    } else {
                        let (li, lj) = (a.lld(gi) - lx, b.lld(gj) - ly);
                        fd[i][j] = del.min(ins).min(fd[li][lj].saturating_add(td[gi][gj]));
                    }
                }
            }
        }
    }
    td[a.len() - 1][b.len() - 1]
}

/// Convert a "random attachment" tree (node 0 is the root; `parent_of[k]` is
/// the parent of node `k + 1`, always a lower ID) into a postorder
/// `LabeledTree`. Children keep ascending-ID sibling order.
fn build_postorder_tree(labels: &[u32], parent_of: &[usize]) -> LabeledTree<'static> {
    let n = labels.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, &p) in parent_of.iter().enumerate() {
        children[p].push(k + 1);
    }
    fn dfs(id: usize, children: &[Vec<usize>], order: &mut Vec<usize>) {
        for &c in &children[id] {
            dfs(c, children, order);
        }
        order.push(id);
    }
    let mut order = Vec::with_capacity(n);
    dfs(0, &children, &mut order);
    let mut post_of_id = vec![0usize; n];
    for (post, &id) in order.iter().enumerate() {
        post_of_id[id] = post;
    }
    let mut post_labels = vec![0u32; n];
    let mut post_parents: Vec<Option<usize>> = vec![None; n];
    for id in 0..n {
        post_labels[post_of_id[id]] = labels[id];
        post_parents[post_of_id[id]] = if id == 0 {
            None
        } else {
            Some(post_of_id[parent_of[id - 1]])
        };
    }
    LabeledTree::from_postorder(&post_labels, &post_parents)
        .expect("generated parent links are a valid tree by construction")
}

/// Random trees with 1..=max_nodes nodes and labels drawn from a small
/// alphabet (small alphabets maximize interesting rename/match interactions).
fn arb_tree(max_nodes: usize, n_labels: u32) -> impl Strategy<Value = LabeledTree<'static>> {
    (1..=max_nodes).prop_flat_map(move |n| {
        let parents: Vec<std::ops::Range<usize>> = (1..n).map(|i| 0..i).collect();
        (proptest::collection::vec(0..n_labels, n), parents)
            .prop_map(|(labels, parent_of)| build_postorder_tree(&labels, &parent_of))
    })
}

/// True iff `anc` is a proper ancestor of `node` in `t`.
fn is_ancestor(t: &LabeledTree, anc: usize, node: usize) -> bool {
    let mut current = node;
    while let Some(p) = t.parent(current) {
        if p == anc {
            return true;
        }
        current = p;
    }
    false
}

#[test]
fn oracle_sanity_paper_example() {
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
    assert_eq!(zs_oracle(&t, &t_prime), 2);
    assert_eq!(zs_oracle(&t, &t), 0);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn touzet_matches_oracle_with_loose_bound(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
    ) {
        let expected = zs_oracle(&a, &b);
        let tau = (a.len() + b.len()) as u32; // always a valid upper bound
        prop_assert_eq!(touzet_depth(&a, &b, tau), expected);
    }

    #[test]
    fn touzet_matches_oracle_with_tight_bound(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
    ) {
        // The tightest valid bound exercises the pruning paths hardest.
        let expected = zs_oracle(&a, &b);
        let tau = expected.max(1);
        prop_assert_eq!(touzet_depth(&a, &b, tau), expected);
    }

    #[test]
    fn touzet_is_symmetric(a in arb_tree(15, 3), b in arb_tree(15, 3)) {
        let tau = (a.len() + b.len()) as u32;
        prop_assert_eq!(touzet_depth(&a, &b, tau), touzet_depth(&b, &a, tau));
    }

    #[test]
    fn identity_distance_is_zero(a in arb_tree(20, 4)) {
        prop_assert_eq!(touzet_depth(&a, &a, 1), 0);
    }

    #[test]
    fn topdiff_matches_oracle_with_loose_bound(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
    ) {
        let expected = zs_oracle(&a, &b);
        let tau = (a.len() + b.len()) as u32;
        prop_assert_eq!(topdiff(&a, &b, tau), expected);
    }

    #[test]
    fn topdiff_matches_oracle_with_tight_bound(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
    ) {
        let expected = zs_oracle(&a, &b);
        let tau = expected.max(1);
        prop_assert_eq!(topdiff(&a, &b, tau), expected);
    }

    #[test]
    fn topdiff_and_touzet_agree(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
        tau in 1u32..40,
    ) {
        // For ANY tau (even invalid bounds) both algorithms must agree on
        // whether the result is within tau, per Lemma 7.1: dtau <= tau iff
        // the true distance is <= tau.
        let t1 = touzet_depth(&a, &b, tau);
        let t2 = topdiff(&a, &b, tau);
        prop_assert_eq!(t1 <= tau, t2 <= tau);
        if t1 <= tau {
            prop_assert_eq!(t1, t2);
        }
    }

    #[test]
    fn autostop_matches_oracle(a in arb_tree(20, 4), b in arb_tree(20, 4)) {
        // The strongest end-to-end check: no bound is provided at all.
        let expected = zs_oracle(&a, &b);
        let got = tree_edit_distance(&a, &b, &TreeDiffOptions::default()).unwrap();
        prop_assert_eq!(got, expected);
    }

    #[test]
    fn autostop_distance_bounded_by_total_size(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
    ) {
        let got = tree_edit_distance(&a, &b, &TreeDiffOptions::default()).unwrap();
        prop_assert!(got as usize <= a.len() + b.len());
    }

    #[test]
    fn mapping_cost_equals_distance(a in arb_tree(20, 4), b in arb_tree(20, 4)) {
        let m = edit_mapping(&a, &b, &TreeDiffOptions::default()).unwrap();
        prop_assert_eq!(m.distance, zs_oracle(&a, &b));
        // Every node is accounted for exactly once.
        prop_assert_eq!(m.mapped.len() + m.deleted.len(), a.len());
        prop_assert_eq!(m.mapped.len() + m.inserted.len(), b.len());
    }

    #[test]
    fn mapping_satisfies_edit_mapping_conditions(
        a in arb_tree(15, 3),
        b in arb_tree(15, 3),
    ) {
        let m = edit_mapping(&a, &b, &TreeDiffOptions::default()).unwrap();
        for (idx, &(x, y)) in m.mapped.iter().enumerate() {
            for &(x2, y2) in &m.mapped[idx + 1..] {
                // One-to-one.
                prop_assert!(x != x2 && y != y2);
                // Ancestor condition.
                prop_assert_eq!(is_ancestor(&a, x, x2), is_ancestor(&b, y, y2));
                prop_assert_eq!(is_ancestor(&a, x2, x), is_ancestor(&b, y2, y));
                // Order condition: postorder + ancestor relation determine
                // left-of; with both ancestor checks equal, left-of reduces
                // to postorder comparison.
                if !is_ancestor(&a, x2, x) && !is_ancestor(&a, x, x2) {
                    prop_assert_eq!(x < x2, y < y2);
                }
            }
        }
    }

    #[test]
    fn mapping_is_deterministic(a in arb_tree(15, 3), b in arb_tree(15, 3)) {
        let opts = TreeDiffOptions::default();
        prop_assert_eq!(
            edit_mapping(&a, &b, &opts).unwrap(),
            edit_mapping(&a, &b, &opts).unwrap()
        );
    }
}
