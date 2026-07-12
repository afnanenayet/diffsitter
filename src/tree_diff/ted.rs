//! The shared forest-distance dynamic programming core (Zhang–Shasha style),
//! banded and budgeted per Touzet / Pawlik & Augsten.

use super::band::BandMatrix;
use super::tree::LabeledTree;

/// L1 distance between the neighborhood vectors of `x` and `y` (Def. 4.1).
pub(crate) fn neighborhood_distance(
    old: &LabeledTree,
    new: &LabeledTree,
    x: usize,
    y: usize,
) -> u64 {
    let (lx, dx, ax, rx) = old.neighborhood(x);
    let (ly, dy, ay, ry) = new.neighborhood(y);
    ((lx - ly).abs() + (dx - dy).abs() + (ax - ay).abs() + (rx - ry).abs()) as u64
}

/// The edits budget `eps(x, y, tau)` (Sect. 3): the maximum number of edits
/// available to transform `Tx` into `Ty` inside a mapping of cost <= tau.
///
/// Returns `None` when the pair fails the neighborhood filter (Lemma 4.2),
/// i.e. it cannot be part of any mapping within `tau`. When it passes, the
/// budget is guaranteed non-negative because `eps >= |dx − dy| >= 0`.
pub(crate) fn edits_budget(
    old: &LabeledTree,
    new: &LabeledTree,
    x: usize,
    y: usize,
    tau: u32,
) -> Option<u32> {
    let (lx, dx, ax, rx) = old.neighborhood(x);
    let (ly, dy, ay, ry) = new.neighborhood(y);
    let (dl, dd, da, dr) = (
        (lx - ly).abs(),
        (dx - dy).abs(),
        (ax - ay).abs(),
        (rx - ry).abs(),
    );
    if dl + dd + da + dr > i64::from(tau) {
        return None;
    }
    let budget = i64::from(tau) - dl - da - dr;
    debug_assert!(budget >= dd);
    Some(budget as u32)
}

/// The result of one forest-distance computation for a subtree pair.
pub(crate) struct ForestDistance {
    /// The banded local matrix, `(m+1) x (n+1)` with index 0 = empty prefix.
    pub fd: BandMatrix,
    /// Anchored-cell subtree distances `(gi, gj, dist)` discovered along the
    /// way, in ascending order; includes the pair `(x, y)` itself last.
    pub anchored: Vec<(usize, usize, u32)>,
    /// `m = |Tx|`.
    pub rows: usize,
    /// `n = |Ty|`.
    pub cols: usize,
}

impl ForestDistance {
    /// The distance between the full subtree pair.
    pub(crate) fn root_distance(&self) -> u32 {
        self.fd.get(self.rows, self.cols)
    }
}

/// Compute the banded forest distances for subtree pair `(x, y)`.
///
/// `budget` is the edits budget (band half-width). `depth_pruning` enables
/// Touzet's depth-based pruning (Sect. 6): rows too deep to be mapped become
/// forced deletions. `td` supplies distances of smaller subtree pairs; cells
/// never computed read as [`super::band::INF`], which is correct because such pairs are
/// irrelevant under the bound (Thm. 5.3).
pub(crate) fn forest_distance(
    old: &LabeledTree,
    new: &LabeledTree,
    x: usize,
    y: usize,
    budget: u32,
    depth_pruning: bool,
    td: &BandMatrix,
) -> ForestDistance {
    let m = old.size(x);
    let n = new.size(y);
    let lx = old.lld(x);
    let ly = new.lld(y);
    let mut fd = BandMatrix::new(m + 1, n + 1, budget as usize);
    let mut anchored = Vec::new();

    fd.set(0, 0, 0);
    for lj in fd.row_cols(0) {
        if lj == 0 {
            continue;
        }
        fd.set(0, lj, fd.get(0, lj - 1).saturating_add(1));
    }
    for li in 1..=m {
        let gi = lx + li - 1;
        // Depth-based pruning: a node this deep cannot be freshly discovered
        // as a *new* anchored subtree match within this subproblem (that
        // would require an expensive nested comparison) -- it must be
        // deleted at the anchor. This does NOT block the "sub" branch below:
        // that branch only performs an O(1) lookup into a `td` entry that
        // was already computed independently (by the ascending-`x` outer
        // loop) using its own, unrelated depth reference, so it stays exact
        // regardless of how deep `gi` is relative to the *current* pair.
        let forced_delete =
            depth_pruning && (old.depth(gi) as i64 - old.depth(x) as i64 - 1) >= i64::from(budget);
        let lld_i = old.lld(gi) - lx + 1;
        for lj in fd.row_cols(li) {
            let del = fd.get(li - 1, lj).saturating_add(1);
            if lj == 0 {
                fd.set(li, lj, del);
                continue;
            }
            let gj = ly + lj - 1;
            let ins = fd.get(li, lj - 1).saturating_add(1);
            let lld_j = new.lld(gj) - ly + 1;
            // Always available: substituting the (possibly already known,
            // independently computed) subtree distance for (gi, gj). This
            // stays correct even when `forced_delete` suppresses a *fresh*
            // rename discovery below, because `td.get(gi, gj)` was computed
            // by an earlier outer-loop iteration using gi/gj's own depth
            // reference, not the current pair's.
            let sub = fd.get(lld_i - 1, lld_j - 1).saturating_add(td.get(gi, gj));
            let best = if lld_i == 1 && lld_j == 1 {
                // Both prefixes are whole subtrees sharing the pair's
                // leftmost leaves: this cell is itself a subtree distance.
                if forced_delete {
                    del.min(ins).min(sub)
                } else {
                    let rename = u32::from(old.label(gi) != new.label(gj));
                    let dist = del
                        .min(ins)
                        .min(fd.get(li - 1, lj - 1).saturating_add(rename))
                        .min(sub);
                    anchored.push((gi, gj, dist));
                    dist
                }
            } else {
                del.min(ins).min(sub)
            };
            fd.set(li, lj, best);
        }
    }
    ForestDistance {
        fd,
        anchored,
        rows: m,
        cols: n,
    }
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
    fn neighborhood_distance_matches_paper_example_4_3() {
        let (t, t_prime) = paper_trees();
        // Example 4.3: ||v_x5 − v_y3|| = 6, ||v_x4 − v_y3|| = 2.
        assert_eq!(neighborhood_distance(&t, &t_prime, 5, 3), 6);
        assert_eq!(neighborhood_distance(&t, &t_prime, 4, 3), 2);
    }

    #[test]
    fn edits_budget_matches_paper_examples() {
        let (t, t_prime) = paper_trees();
        // Example 4.3: eps(x4, y3, 2) = 0. Example 4.5: eps(x5, y4, 2) = 0.
        assert_eq!(edits_budget(&t, &t_prime, 4, 3, 2), Some(0));
        assert_eq!(edits_budget(&t, &t_prime, 5, 4, 2), Some(0));
        // (x5, y3) fails the neighborhood filter at tau = 2.
        assert_eq!(edits_budget(&t, &t_prime, 5, 3, 2), None);
        // The root pair always gets the full budget.
        assert_eq!(edits_budget(&t, &t_prime, 7, 7, 2), Some(2));
    }
}
