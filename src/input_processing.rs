//! Utilities for processing the ASTs provided by `tree_sitter`

use logging_timer::time;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::{cell::RefCell, ops::Index, path::PathBuf};
use tree_sitter::Node as TSNode;
use tree_sitter::Point;
use tree_sitter::Tree as TSTree;
use unicode_segmentation as us;

/// The configuration options for processing tree-sitter output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct TreeSitterProcessor {
    /// Whether we should split the nodes graphemes.
    ///
    /// If this is disabled, then the direct tree-sitter nodes will be used and diffs will be less
    /// granular. This has the advantage of being faster and using less memory.
    pub split_graphemes: bool,
}

impl Default for TreeSitterProcessor {
    fn default() -> Self {
        Self {
            split_graphemes: true,
        }
    }
}

impl TreeSitterProcessor {
    #[time("info", "ast::{}")]
    pub fn process<'a>(&self, tree: &'a TSTree, text: &'a str) -> Vec<Entry<'a>> {
        let ast_vector = from_ts_tree(tree, text);
        let entries = if self.split_graphemes {
            ast_vector
                .leaves
                .iter()
                .flat_map(|leaf| leaf.split_on_graphemes())
                .collect()
        } else {
            ast_vector.leaves.iter().map(|&x| Entry::from(x)).collect()
        };
        entries
    }
}

/// Create a `DiffVector` from a `tree_sitter` tree
///
/// This method calls a helper function that does an in-order traversal of the tree and adds
/// leaf nodes to a vector
#[time("info", "ast::{}")]
fn from_ts_tree<'a>(tree: &'a TSTree, text: &'a str) -> Vector<'a> {
    let leaves = RefCell::new(Vec::new());
    build(&leaves, tree.root_node(), text);
    Vector {
        leaves: leaves.into_inner(),
        source_text: text,
    }
}

/// The leaves of an AST vector
///
/// This is used as an intermediate struct for flattening the tree structure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorLeaf<'a> {
    pub reference: TSNode<'a>,
    pub text: &'a str,
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

    /// The entry's start position in the document.
    pub start_position: Point,

    /// The entry's end position in the document.
    pub end_position: Point,

    /// The cached kind_id from the TSNode reference.
    ///
    /// Caching it here saves some time because it is queried repeatedly later.
    pub kind_id: u16,
}

impl<'a> VectorLeaf<'a> {
    /// Split an entry into a vector of entries per grapheme.
    ///
    /// Each grapheme will get its own [Entry] struct. This method will resolve the
    /// indices/positioning of each grapheme from the `self.text` field.
    fn split_on_graphemes(self) -> Vec<Entry<'a>> {
        let mut entries = Vec::new();
        let indices: Vec<(usize, &str)> =
            us::UnicodeSegmentation::grapheme_indices(self.text, true).collect();
        entries.reserve(indices.len());
        let mut current_line = self.reference.start_position().row;

        for (idx, grapheme) in indices {
            // Every grapheme has to be at least one byte
            debug_assert!(!grapheme.is_empty());

            let original_start_col = self.reference.start_position().column;
            let new_start_pos = Point {
                row: current_line,
                column: original_start_col + idx,
            };
            let new_end_pos = Point {
                row: current_line,
                column: new_start_pos.column + grapheme.len(),
            };

            debug_assert!(new_start_pos.row <= new_end_pos.row);

            // If the end position is on the next row, then the column index can be less than or
            // equal to the the start column. If they are on the same line, then the ending column
            // *must be* greater than the starting column.
            debug_assert!(
                new_start_pos.column < new_end_pos.column || new_start_pos.row < new_end_pos.row
            );

            let entry = Entry {
                reference: self.reference,
                text: &self.text[idx..idx + grapheme.len()],
                start_position: new_start_pos,
                end_position: new_end_pos,
                kind_id: self.reference.kind_id(),
            };
            entries.push(entry);

            // If the last entry was a new line, iterate up for the next entry
            if grapheme == "\n" || grapheme == "\r\n" {
                current_line += 1;
            }
        }
        entries
    }
}

impl<'a> From<VectorLeaf<'a>> for Entry<'a> {
    fn from(leaf: VectorLeaf<'a>) -> Self {
        Self {
            reference: leaf.reference,
            text: leaf.text,
            start_position: leaf.reference.start_position(),
            end_position: leaf.reference.start_position(),
            kind_id: leaf.reference.kind_id(),
        }
    }
}

impl<'a> Entry<'a> {
    /// Get the start position of an entry
    pub fn start_position(&self) -> Point {
        self.start_position
    }

    /// Get the end position of an entry
    pub fn end_position(&self) -> Point {
        self.end_position
    }
}

impl<'a> From<&'a Vector<'a>> for Vec<Entry<'a>> {
    fn from(ast_vector: &'a Vector<'a>) -> Self {
        let mut entries = Vec::new();
        entries.reserve(ast_vector.leaves.len());

        for entry in &ast_vector.leaves {
            entries.extend(entry.split_on_graphemes().iter());
        }
        entries
    }
}

/// A vector that allows for linear traversal through the leafs of an AST.
///
/// This representation of the tree leaves is much more convenient for things like dynamic
/// programming, and provides useful for formatting.
#[derive(Debug)]
pub struct Vector<'a> {
    /// The leaves of the AST, build with an in-order traversal
    pub leaves: Vec<VectorLeaf<'a>>,

    /// The full source text that the AST refers to
    pub source_text: &'a str,
}

impl<'a> Eq for Entry<'a> {}

/// A wrapper struct for AST vector data that owns the data that the AST vector references
///
/// Ideally we would just have the AST vector own the actual string and tree, but it makes things
/// extremely messy with the borrow checker, so we have this wrapper struct that holds the owned
/// data that the vector references. This gets tricky because the tree sitter library uses FFI so
/// the lifetime references get even more mangled.
#[derive(Debug)]
pub struct VectorData {
    /// The text in the file
    pub text: String,

    /// The tree that was parsed using the text
    pub tree: TSTree,

    /// The file path that the text corresponds to
    pub path: PathBuf,
}

impl<'a> Vector<'a> {
    /// Create a `DiffVector` from a `tree_sitter` tree
    ///
    /// This method calls a helper function that does an in-order traversal of the tree and adds
    /// leaf nodes to a vector
    #[time("info", "ast::{}")]
    pub fn from_ts_tree(tree: &'a TSTree, text: &'a str) -> Self {
        let leaves = RefCell::new(Vec::new());
        build(&leaves, tree.root_node(), text);
        Vector {
            leaves: leaves.into_inner(),
            source_text: text,
        }
    }

    /// Return the number of nodes in the diff vector
    pub fn len(&self) -> usize {
        self.leaves.len()
    }
}

impl<'a> Index<usize> for Vector<'a> {
    type Output = VectorLeaf<'a>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.leaves[index]
    }
}

impl<'a> Hash for VectorLeaf<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.reference.kind_id().hash(state);
        self.text.hash(state);
    }
}

impl<'a> PartialEq for Entry<'a> {
    fn eq(&self, other: &Entry) -> bool {
        self.kind_id == other.kind_id && self.text == other.text
    }
}

impl<'a> PartialEq for Vector<'a> {
    fn eq(&self, other: &Vector) -> bool {
        if self.leaves.len() != other.leaves.len() {
            return false;
        }

        for i in 0..self.leaves.len() {
            let leaf = self.leaves[i];
            let other_leaf = other.leaves[i];

            if leaf != other_leaf {
                return false;
            }
        }
        true
    }
}

/// Recursively build a vector from a given node
///
/// This is a helper function that simply walks the tree and collects leaves in an in-order manner.
/// Every time it encounters a leaf node, it stores the metadata and reference to the node in an
/// `Entry` struct.
fn build<'a>(vector: &RefCell<Vec<VectorLeaf<'a>>>, node: tree_sitter::Node<'a>, text: &'a str) {
    // If the node is a leaf, we can stop traversing
    if node.child_count() == 0 {
        // We only push an entry if the referenced text range isn't empty, since there's no point
        // in having an empty text range. This also fixes a bug where the program would panic
        // because it would attempt to access the 0th index in an empty text range.
        if !node.byte_range().is_empty() {
            let node_text: &'a str = &text[node.byte_range()];
            vector.borrow_mut().push(VectorLeaf {
                reference: node,
                text: node_text,
            });
        }
        return;
    }

    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        build(vector, child, text);
    }
}

/// The different types of elements that can be in an edit script
#[derive(Debug, Eq, PartialEq)]
pub enum EditType<T> {
    /// An element that was added in the edit script
    Addition(T),

    /// An element that was deleted in the edit script
    Deletion(T),
}
