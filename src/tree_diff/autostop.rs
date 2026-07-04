//! The tau-doubling driver (paper Alg. 6) with the TopDiff+ algorithm switch
//! (Alg. 5). Starts from a size-difference lower bound and doubles tau until
//! the stopping condition holds; each round picks TopDiff or TouzetDepth by
//! the cost estimates in [`super::cost`].

use super::band::BandMatrix;
use super::cost::{cost_topdiff, cost_touzet_depth};
use super::topdiff::topdiff_impl;
use super::touzet::touzet_depth_impl;
use super::tree::LabeledTree;
use super::{TreeDiffError, TreeDiffOptions};

/// A converged bounded-distance computation, retaining what mapping recovery
/// needs.
pub(crate) struct BoundedResult {
    /// The exact tree edit distance.
    pub distance: u32,
    /// The bound the search converged at (`distance <= tau`).
    #[allow(dead_code)] // consumed by mapping recovery (Task 8)
    pub tau: u32,
    /// Subtree distances from the final round.
    #[allow(dead_code)] // consumed by mapping recovery (Task 8)
    pub td: BandMatrix,
}

/// Compute the exact tree edit distance with no prior bound (paper Alg. 6).
///
/// The stopping condition `dtau <= tau` guarantees exactness (Thm. 7.2).
/// Fails with [`TreeDiffError::BoundExceeded`] when tau would exceed
/// `options.max_tau`, which signals inputs too dissimilar to diff usefully.
pub(crate) fn autostop(
    old: &LabeledTree,
    new: &LabeledTree,
    options: &TreeDiffOptions,
) -> Result<BoundedResult, TreeDiffError> {
    debug_assert!(!old.is_empty() && !new.is_empty());
    let size_diff = old.len().abs_diff(new.len());
    let mut tau = u32::try_from(size_diff).unwrap_or(u32::MAX).max(1);
    // TopDiff's cost estimate is independent of tau, so compute it once.
    let topdiff_cost = cost_topdiff(old);
    loop {
        if tau > options.max_tau {
            return Err(TreeDiffError::BoundExceeded {
                tau,
                limit: options.max_tau,
            });
        }
        // TopDiff+ (Alg. 5): pick the algorithm with the smaller estimate.
        // Only TouzetDepth's estimate depends on tau and is recomputed.
        let (distance, td) = if topdiff_cost < cost_touzet_depth(old, tau) {
            topdiff_impl(old, new, tau)
        } else {
            touzet_depth_impl(old, new, tau)
        };
        if distance <= tau {
            return Ok(BoundedResult { distance, tau, td });
        }
        tau = tau.saturating_mul(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::{LabeledTree, TreeDiffOptions};

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
    fn paper_example_needs_no_bound() {
        let (t, t_prime) = paper_trees();
        let result = autostop(&t, &t_prime, &TreeDiffOptions::default()).unwrap();
        assert_eq!(result.distance, 2);
        assert!(result.tau >= 2);
    }

    #[test]
    fn identical_trees() {
        let (t, _) = paper_trees();
        let result = autostop(&t, &t, &TreeDiffOptions::default()).unwrap();
        assert_eq!(result.distance, 0);
        assert_eq!(result.tau, 1); // starts at max(size diff, 1) = 1 and stops
    }

    #[test]
    fn bound_exceeded_for_tiny_limit() {
        let (t, t_prime) = paper_trees();
        let options = TreeDiffOptions { max_tau: 1 };
        assert!(matches!(
            autostop(&t, &t_prime, &options),
            Err(crate::tree_diff::TreeDiffError::BoundExceeded { limit: 1, .. })
        ));
    }
}
