//! Utilities for processing the ASTs provided by `tree_sitter`

use crate::diff::Hunks;
use anyhow::Result;
use logging_timer::time;
use std::{cell::RefCell, collections::HashMap, ops::Index};
use tree_sitter::Node as TSNode;
use tree_sitter::Tree as TSTree;

/// Get the minium of an arbitrary number of elements
macro_rules! min {
    ($x: expr) => ($x);
    ($x: expr, $($z: expr),+) => (::std::cmp::min($x, min!($($z),*)));
}

/// The internal variant of an edit
///
/// This is the edit enum that's used for the minimum edit distance algorithm. It features a
/// variant, `Substitution`, that isn't exposed externally. When recreating the edit path,
/// [Substitution](Edit::Substitution) variant turns into an
/// [Addition](Edit::Addition) and [Deletion](Internal::Deletion).
#[derive(Debug, Copy, Clone, PartialEq)]
enum Edit<'a> {
    /// A no-op
    ///
    /// There is no edit
    Noop,

    /// Some text was added
    ///
    /// An addition refers to the text from a node that was added from b
    Addition(Entry<'a>),

    /// Some text was deleted
    ///
    /// An addition refers to text from a node that was deleted from source a
    Deletion(Entry<'a>),

    /// Some text was replaced
    ///
    /// A substitution refers to text from a node that was replaced, holding a reference to the old
    /// AST node and the AST node that replaced it
    Substitution {
        /// The old text
        old: Entry<'a>,

        /// The new text that took its palce
        new: Entry<'a>,
    },
}

/// A mapping between a tree-sitter node and the text it corresponds to
#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    /// The node an entry in the diff vector refers to
    ///
    /// We keep a reference to the leaf node so that we can easily grab the text and other metadata
    /// surrounding the syntax
    pub reference: TSNode<'a>,

    /// A reference to the text the node refers to
    ///
    /// This is different from the `source_text` that the [AstVector](AstVector) refers to, as the
    /// entry only holds a reference to the specific range of text that the node covers.
    pub text: &'a str,
}

/// A vector that allows for linear traversal through the leafs of an AST.
///
/// This representation of the tree leaves is much more convenient for things like dynamic
/// programming, and provides useful for formatting.
pub struct AstVector<'a> {
    /// The leaves of the AST, build with an in-order traversal
    pub leaves: Vec<Entry<'a>>,

    /// The full source text that the AST refers to
    pub source_text: &'a str,
}

impl<'a> AstVector<'a> {
    /// Create a `DiffVector` from a `tree_sitter` tree
    ///
    /// This method calls a helper function that does an in-order traversal of the tree and adds
    /// leaf nodes to a vector
    #[time("info")]
    pub fn from_ts_tree(tree: &'a TSTree, text: &'a str) -> Self {
        let leaves = RefCell::new(Vec::new());
        build(&leaves, tree.root_node(), text);
        AstVector {
            leaves: leaves.into_inner(),
            source_text: text,
        }
    }

    /// Return the number of nodes in the diff vector
    pub fn len(&self) -> usize {
        self.leaves.len()
    }
}

impl<'a> Index<usize> for AstVector<'a> {
    type Output = Entry<'a>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.leaves[index]
    }
}

impl<'a> PartialEq for Entry<'a> {
    fn eq(&self, other: &Entry) -> bool {
        self.text == other.text
    }
}

impl<'a> PartialEq for AstVector<'a> {
    fn eq(&self, other: &AstVector) -> bool {
        if self.leaves.len() != other.leaves.len() {
            return false;
        }

        // Zip through each entry to determine whether the elements are equal. We start with a
        // `false` value for not equal and accumulate any inequalities along the way.
        let not_equal = self
            .leaves
            .iter()
            .zip(other.leaves.iter())
            .fold(false, |not_equal, (entry_a, entry_b)| {
                not_equal | (entry_a != entry_b)
            });
        !not_equal
    }
}

/// Recursively build a vector from a given node
///
/// This is a helper function that simply walks the tree and collects leaves in an in-order manner.
/// Every time it encounters a leaf node, it stores the metadata and reference to the node in an
/// `Entry` struct.
fn build<'a>(vector: &RefCell<Vec<Entry<'a>>>, node: tree_sitter::Node<'a>, text: &'a str) {
    // If the node is a leaf, we can stop traversing
    if node.child_count() == 0 {
        let node_text: &'a str = &text[node.byte_range()];
        vector.borrow_mut().push(Entry {
            reference: node,
            text: node_text,
        });
        return;
    }

    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        build(vector, child, text);
    }
}

/// Recreate the paths for additions and deletions given a [PredecessorMap]
///
/// This will generate the hunks for both documents in one shot as we reconstruct the path.
#[time("info", "ast::{}")]
fn recreate_path(last_idx: (usize, usize), preds: PredecessorMap) -> Result<(Hunks, Hunks)> {
    // The hunks for the old document. Deletions correspond to this.
    let mut hunks_old = Hunks::new();
    // The hunks for the new document. Additions correspond to this.
    let mut hunks_new = Hunks::new();
    let mut curr_idx = last_idx;

    while let Some(&entry) = preds.get(&curr_idx) {
        match entry.edit {
            Edit::Noop => (),
            Edit::Addition(x) => hunks_new.push_front(x)?,
            Edit::Deletion(x) => hunks_old.push_front(x)?,
            Edit::Substitution { old, new } => {
                hunks_new.push_front(new)?;
                hunks_old.push_front(old)?;
            }
        }
        curr_idx = entry.previous_idx;
    }
    Ok((hunks_old, hunks_new))
}

/// An entry in the precedecessor table
///
/// This entry contains information about the type of edit, and which index to backtrack to
#[derive(Debug, Clone, Copy)]
struct PredEntry<'a> {
    /// The edit in question
    pub edit: Edit<'a>,

    /// The index the edit came from
    pub previous_idx: (usize, usize),
}

/// A type alias for an index in a two dimensional vector
type Idx2D = (usize, usize);

/// A type alias for the precedessor map used to backtrack the edit path
type PredecessorMap<'a> = HashMap<Idx2D, PredEntry<'a>>;

/// Helper function to use the minimum edit distance algorithm on two [AstVectors](AstVector)
#[time("info", "ast::{}")]
fn min_edit<'a>(a: &'a AstVector, b: &'a AstVector) -> PredecessorMap<'a> {
    // The optimal move that led to the edit distance at an index. We use this map to backtrack
    // and build the edit path once we find the optimal edit distance
    let mut predecessors: PredecessorMap<'a> = HashMap::new();

    // Initialize the dynamic programming array
    // dp[i][j] is the edit distance between a[:i] and b[:j]
    let mut dp: Vec<Vec<u32>> = (0..a.len() + 1)
        .map(|_| (0..b.len() + 1).map(|_| 0).collect())
        .collect();

    // Sanity check that the dimensions of the DP table are correct
    debug_assert!(dp.len() == a.len() + 1);
    debug_assert!(dp[0].len() == b.len() + 1);

    for i in 0..=a.len() {
        for j in 0..=b.len() {
            // If either string is empty, the minimum edit is just to add strings
            if i == 0 {
                dp[i][j] = j as u32;

                if j > 0 {
                    let pred_entry = PredEntry {
                        edit: Edit::Addition(b[j - 1]),
                        previous_idx: (i, j - 1),
                    };
                    predecessors.insert((i, j), pred_entry);
                }
            } else if j == 0 {
                dp[i][j] = i as u32;

                if i > 0 {
                    let pred_entry = PredEntry {
                        edit: Edit::Deletion(b[i - 1]),
                        previous_idx: (i - 1, j),
                    };
                    predecessors.insert((i, j), pred_entry);
                }
            }
            // If the current letter for each string matches, there is no change
            else if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
                let pred_entry = PredEntry {
                    edit: Edit::Noop,
                    previous_idx: (i - 1, j - 1),
                };
                predecessors.insert((i, j), pred_entry);
            }
            // Otherwise, there is either a substitution, a deletion, or an addition
            else {
                let min = min!(dp[i - 1][j - 1], dp[i - 1][j], dp[i][j - 1]);

                // Store the current minimum edit in the precedecessor map based on which path has
                // the lowest edit distance
                let pred_entry = if min == dp[i - 1][j] {
                    PredEntry {
                        edit: Edit::Deletion(a[i - 1]),
                        previous_idx: (i - 1, j),
                    }
                } else if min == dp[i][j - 1] {
                    PredEntry {
                        edit: Edit::Addition(b[j - 1]),
                        previous_idx: (i, j - 1),
                    }
                } else {
                    PredEntry {
                        edit: Edit::Substitution {
                            old: a[i - 1],
                            new: b[j - 1],
                        },
                        previous_idx: (i - 1, j - 1),
                    }
                };
                // Store the precedecessor so we can backtrack and recreate the path that led to
                // the minimum edit path
                predecessors.insert((i, j), pred_entry);

                // Store the current minimum edit distance for a[:i] <-> b[:j]. An addition,
                // deletion, and substitution all have an edit cost of 1, which is why we're adding
                // one to the minimum.
                dp[i][j] = 1 + min;
            }
        }
    }
    predecessors
}

/// Compute the hunks corresponding to the minimum edit path between two documents
///
/// This method computes the minimum edit distance between two `DiffVector`s, which are the leaf
/// nodes of an AST, using the standard DP approach to the longest common subsequence problem, the
/// only twist is that here, instead of operating on raw text, we're operating on the leaves of an
/// AST.
///
/// This has O(mn) space complexity and uses O(mn) space to compute the minimum edit path, and then
/// has O(mn) space complexity and uses O(mn) space to backtrack and recreate the path.
///
/// This will return two groups of [hunks](diff::Hunks) in a tuple of the form
/// `(old_hunks, new_hunks)`.
pub fn edit_hunks<'a>(a: &'a AstVector, b: &'a AstVector) -> Result<(Hunks<'a>, Hunks<'a>)> {
    let predecessors = min_edit(a, b);
    recreate_path((a.len(), b.len()), predecessors)
}
