//! Recovering the optimal edit mapping from a converged distance computation.
//!
//! The paper's algorithms compute only the distance; this module adds a
//! standard Zhang-Shasha-style backtrace adapted to the banded matrices. See
//! the plan/spec for the correctness argument; the invariant `cost(mapping)
//! == distance` is enforced at runtime.

use super::autostop::{BoundedResult, autostop};
use super::band::BandMatrix;
use super::ted::forest_distance;
use super::tree::LabeledTree;
use super::{TreeDiffError, TreeDiffOptions};

/// The optimal edit mapping between two trees (paper Def. 3.2).
///
/// Node IDs are postorder indices into the respective trees. `mapped` pairs
/// with equal labels cost nothing; pairs with differing labels are renames.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditMapping {
    /// Mapped node pairs `(old, new)`, ascending by old postorder ID.
    pub mapped: Vec<(usize, usize)>,
    /// Old-tree nodes with no counterpart, ascending.
    pub deleted: Vec<usize>,
    /// New-tree nodes with no counterpart, ascending.
    pub inserted: Vec<usize>,
    /// The tree edit distance (= renames + deletions + insertions).
    pub distance: u32,
}

/// Compute the optimal edit mapping between two labeled trees.
pub fn edit_mapping(
    old: &LabeledTree,
    new: &LabeledTree,
    options: &TreeDiffOptions,
) -> Result<EditMapping, TreeDiffError> {
    match (old.is_empty(), new.is_empty()) {
        (true, true) => return Ok(EditMapping::default()),
        (true, false) => {
            return Ok(EditMapping {
                inserted: (0..new.len()).collect(),
                distance: u32::try_from(new.len()).unwrap_or(u32::MAX),
                ..Default::default()
            });
        }
        (false, true) => {
            return Ok(EditMapping {
                deleted: (0..old.len()).collect(),
                distance: u32::try_from(old.len()).unwrap_or(u32::MAX),
                ..Default::default()
            });
        }
        (false, false) => {}
    }
    let result = autostop(old, new, options)?;
    recover_mapping(old, new, &result)
}

fn recover_mapping(
    old: &LabeledTree,
    new: &LabeledTree,
    result: &BoundedResult,
) -> Result<EditMapping, TreeDiffError> {
    let mut mapping = EditMapping {
        distance: result.distance,
        ..Default::default()
    };
    recover_pair(
        old,
        new,
        old.len() - 1,
        new.len() - 1,
        result.tau,
        &result.td,
        &mut mapping,
    )?;
    mapping.mapped.sort_unstable();
    mapping.deleted.sort_unstable();
    mapping.inserted.sort_unstable();

    // Runtime safety net for the backtrace correctness argument: the emitted
    // operations must cost exactly the computed distance.
    let renames = mapping
        .mapped
        .iter()
        .filter(|&&(i, j)| old.label(i) != new.label(j))
        .count();
    let cost = renames + mapping.deleted.len() + mapping.inserted.len();
    if u32::try_from(cost).unwrap_or(u32::MAX) != result.distance {
        return Err(TreeDiffError::MappingBacktrace);
    }
    Ok(mapping)
}

/// Walk one subtree pair's forest-distance matrix backwards, emitting edits.
///
/// Recurses into nested subtree pairs (recursion depth is bounded by tree
/// depth, same as the CST builder).
fn recover_pair(
    old: &LabeledTree,
    new: &LabeledTree,
    x: usize,
    y: usize,
    tau: u32,
    td: &BandMatrix,
    out: &mut EditMapping,
) -> Result<(), TreeDiffError> {
    // Recompute with the full tau band: a superset of any forward-pass band,
    // reading the same TD, so the optimal path is present (see plan notes).
    let result = forest_distance(old, new, x, y, tau, false, td);
    let fd = &result.fd;
    let (lx, ly) = (old.lld(x), new.lld(y));
    let (mut li, mut lj) = (old.size(x), new.size(y));
    while li > 0 || lj > 0 {
        let current = fd.get(li, lj);
        // Prefer structural transitions so ties keep nodes mapped.
        if li > 0 && lj > 0 {
            let (gi, gj) = (lx + li - 1, ly + lj - 1);
            let lld_i = old.lld(gi) - lx + 1;
            let lld_j = new.lld(gj) - ly + 1;
            if lld_i == 1 && lld_j == 1 {
                let rename = u32::from(old.label(gi) != new.label(gj));
                if fd.get(li - 1, lj - 1).saturating_add(rename) == current {
                    out.mapped.push((gi, gj));
                    li -= 1;
                    lj -= 1;
                    continue;
                }
            } else if fd.get(lld_i - 1, lld_j - 1).saturating_add(td.get(gi, gj)) == current {
                recover_pair(old, new, gi, gj, tau, td, out)?;
                li = lld_i - 1;
                lj = lld_j - 1;
                continue;
            }
        }
        if li > 0 && fd.get(li - 1, lj).saturating_add(1) == current {
            out.deleted.push(lx + li - 1);
            li -= 1;
            continue;
        }
        if lj > 0 && fd.get(li, lj - 1).saturating_add(1) == current {
            out.inserted.push(ly + lj - 1);
            lj -= 1;
            continue;
        }
        // No transition explains the current cell: internal invariant broken.
        return Err(TreeDiffError::MappingBacktrace);
    }
    Ok(())
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
    fn paper_example_mapping() {
        let (t, t_prime) = paper_trees();
        let m = edit_mapping(&t, &t_prime, &TreeDiffOptions::default()).unwrap();
        assert_eq!(m.distance, 2);
        assert_eq!(m.deleted, vec![0]); // x0 deleted
        assert_eq!(m.inserted, vec![5]); // y5 inserted
        assert_eq!(
            m.mapped,
            vec![(1, 0), (2, 1), (3, 2), (4, 3), (5, 4), (6, 6), (7, 7)]
        );
    }

    #[test]
    fn identity_mapping() {
        let (t, _) = paper_trees();
        let m = edit_mapping(&t, &t, &TreeDiffOptions::default()).unwrap();
        assert_eq!(m.distance, 0);
        assert!(m.deleted.is_empty());
        assert!(m.inserted.is_empty());
        assert_eq!(m.mapped, (0..t.len()).map(|i| (i, i)).collect::<Vec<_>>());
    }

    #[test]
    fn empty_tree_mappings() {
        let empty = LabeledTree::from_postorder(&[], &[]).unwrap();
        let one = LabeledTree::from_postorder(&[5], &[None]).unwrap();
        let options = TreeDiffOptions::default();

        let m = edit_mapping(&empty, &one, &options).unwrap();
        assert_eq!(
            (m.distance, m.inserted.clone(), m.deleted.len()),
            (1, vec![0], 0)
        );

        let m = edit_mapping(&one, &empty, &options).unwrap();
        assert_eq!(
            (m.distance, m.deleted.clone(), m.inserted.len()),
            (1, vec![0], 0)
        );

        let m = edit_mapping(&empty, &empty, &options).unwrap();
        assert_eq!(m.distance, 0);
    }
}
