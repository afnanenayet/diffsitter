//! An AST-aware diff engine based on tree edit distance (TED).
//!
//! Implements Pawlik & Augsten, "Minimal Edit-Based Diffs for Large Trees"
//! (CIKM '20): the `TouzetDepth` and `TopDiff` bounded-distance algorithms, a
//! cost-model switch between them (TopDiff+), and a tau-doubling driver
//! (AutoStop) so no upper bound on the distance needs to be known in advance.
//!
//! Unlike the Myers engine in [`crate::diff`], which flattens the AST into a
//! leaf sequence, this engine diffs the tree structure itself and reports
//! node-level renames, insertions, and deletions.

mod autostop;
mod band;
mod cost;
mod mapping;
mod ted;
mod topdiff;
mod touzet;
mod tree;

pub use mapping::{EditMapping, edit_mapping};
pub use topdiff::topdiff;
pub use touzet::touzet_depth;
pub use tree::{LabeledTree, build_labeled_trees};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// User-facing options for the tree diff engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct TreeDiffOptions {
    /// Abort the distance search when the doubled bound exceeds this value.
    ///
    /// The search runs in time cubic in the bound, so wildly dissimilar
    /// inputs would otherwise dominate the runtime. When exceeded, diffsitter
    /// suggests falling back to the Myers engine.
    pub max_tau: u32,
}

impl Default for TreeDiffOptions {
    fn default() -> Self {
        TreeDiffOptions { max_tau: 2048 }
    }
}

/// Compute the exact tree edit distance between two labeled trees.
///
/// Runs the AutoStop driver (paper Alg. 6) over the TopDiff+ algorithm
/// selection; no upper bound needs to be known in advance.
pub fn tree_edit_distance(
    old: &LabeledTree,
    new: &LabeledTree,
    options: &TreeDiffOptions,
) -> Result<u32, TreeDiffError> {
    // A one-sided empty tree costs one edit per node in the other tree. The
    // saturating conversion mirrors `autostop`; realistic ASTs never approach
    // `u32::MAX` nodes, but truncation would be a silent correctness bug.
    match (old.is_empty(), new.is_empty()) {
        (true, true) => Ok(0),
        (true, false) => Ok(u32::try_from(new.len()).unwrap_or(u32::MAX)),
        (false, true) => Ok(u32::try_from(old.len()).unwrap_or(u32::MAX)),
        (false, false) => Ok(autostop::autostop(old, new, options)?.distance),
    }
}

/// Errors that can arise when computing a tree diff.
#[derive(Error, Debug)]
pub enum TreeDiffError {
    /// The input arrays do not describe a valid postorder-numbered tree.
    #[error("invalid tree structure: {0}")]
    InvalidTree(String),

    /// The tau-doubling search exceeded the configured limit; the inputs are
    /// too dissimilar for the tree diff engine to be practical.
    #[error(
        "edit distance search bound tau = {tau} exceeded the limit {limit}; \
         the inputs are too dissimilar for the tree diff engine"
    )]
    BoundExceeded { tau: u32, limit: u32 },

    /// Internal invariant violation while recovering the edit mapping.
    /// This is a bug in diffsitter — please report it.
    #[error("internal error during mapping backtrace; this is a bug in diffsitter")]
    MappingBacktrace,

    /// The selected renderer cannot display output from the selected engine.
    #[error("the '{renderer}' renderer cannot render output from the '{engine}' diff engine")]
    RendererMismatch { engine: String, renderer: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_edit_distance_handles_empty_trees() {
        let empty = LabeledTree::from_postorder(&[], &[]).unwrap();
        let one = LabeledTree::from_postorder(&[5], &[None]).unwrap();
        let options = TreeDiffOptions::default();
        assert_eq!(tree_edit_distance(&empty, &empty, &options).unwrap(), 0);
        assert_eq!(tree_edit_distance(&empty, &one, &options).unwrap(), 1);
        assert_eq!(tree_edit_distance(&one, &empty, &options).unwrap(), 1);
        assert_eq!(tree_edit_distance(&one, &one, &options).unwrap(), 0);
    }
}
