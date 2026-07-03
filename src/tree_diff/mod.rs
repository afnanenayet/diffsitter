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

mod tree;

pub use tree::{LabeledTree, build_labeled_trees};

use thiserror::Error;

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
