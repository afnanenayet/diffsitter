//! Utilities for processing the ASTs provided by `tree_sitter`
//!
//! These methods handle preprocessing the input data so it can be fed into the diff engines to
//! compute diff data.

use logging_timer::time;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::{cell::RefCell, ops::Index, path::PathBuf};
use tree_sitter::Node as TSNode;
use tree_sitter::Point;
use tree_sitter::Tree as TSTree;
use unicode_segmentation as us;

#[cfg(test)]
use mockall::{automock, predicate::str};

/// A wrapper trait that exists so we can mock TS nodes.
#[cfg_attr(test, automock)]
trait TSNodeTrait {
    /// Return the kind string that corresponds to a node.
    fn kind(&self) -> &str;
}

/// The configuration options for processing tree-sitter output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", default)]
pub struct TreeSitterProcessor {
    /// Whether we should split the nodes graphemes.
    ///
    /// If this is disabled, then the direct tree-sitter nodes will be used and diffs will be less
    /// granular. This has the advantage of being faster and using less memory.
    pub split_graphemes: bool,

    /// The kinds of nodes to exclude from processing. This takes precedence over `include_kinds`.
    ///
    /// This is a set of strings that correspond to the tree sitter node types.
    pub exclude_kinds: Option<HashSet<String>>,

    /// The kinds of nodes to explicitly include when processing. The nodes specified here will be overridden by the
    /// nodes specified in `exclude_kinds`.
    ///
    /// This is a set of strings that correspond to the tree sitter node types.
    pub include_kinds: Option<HashSet<String>>,

    /// Whether to strip whitespace when processing node text.
    ///
    /// Whitespace includes whitespace characters and newlines. This can provide much more accurate
    /// diffs that do not account for line breaks. This is useful especially for more text heavy
    /// documents like markdown files.
    pub strip_whitespace: bool,

    /// A mapping of tree sitter languages to node types to treat like leaves.
    ///
    /// This is useful for grammars like markdown, which has an "inline" node type should be diffed
    /// on. Users can set global leaf types using "all" as the language key.
    pub pseudo_leaf_types: HashMap<String, HashSet<String>>,
}

// TODO: if we want to do any string transformations we need to store Cow strings.
// Most strings won't be modified so it's fine to use a pointer. For the few we do
// modify we'll need to store the direct string.
// We should add some abstractions to do input processing.

impl Default for TreeSitterProcessor {
    fn default() -> Self {
        let pseudo_leaf_types: HashMap<String, HashSet<String>> =
            HashMap::from([("markdown".into(), HashSet::from(["inline".into()]))]);
        Self {
            split_graphemes: true,
            exclude_kinds: None,
            include_kinds: None,
            strip_whitespace: true,
            pseudo_leaf_types,
        }
    }
}

#[derive(Debug)]
struct TSNodeWrapper<'a>(TSNode<'a>);

impl<'a> TSNodeTrait for TSNodeWrapper<'a> {
    fn kind(&self) -> &str {
        self.0.kind()
    }
}

impl TreeSitterProcessor {
    #[time("info", "ast::{}")]
    pub fn process<'a>(&self, tree: &'a TSTree, text: &'a str, lang_name: &str) -> Vec<Entry<'a>> {
        let empty_set: HashSet<String> = HashSet::new();
        let pseudo_leaf_types = self.pseudo_leaf_types.get(lang_name).unwrap_or(&empty_set);
        let ast_vector = from_ts_tree(tree, text, pseudo_leaf_types);
        let iter = ast_vector
            .leaves
            .iter()
            .filter(|leaf| self.should_include_node(&TSNodeWrapper(leaf.reference)));
        // Splitting on graphemes generates a vector of entries instead of a direct mapping, which
        // is why we have the branching here
        if self.split_graphemes {
            iter.flat_map(|leaf| leaf.split_on_graphemes(self.strip_whitespace))
                .collect()
        } else {
            iter.map(|&x| self.process_leaf(x)).collect()
        }
    }

    /// Process a vector leaf and turn it into an [Entry].
    ///
    /// This applies input processing according to the user provided options.
    fn process_leaf<'a>(&self, leaf: VectorLeaf<'a>) -> Entry<'a> {
        let new_text = if self.strip_whitespace {
            // This includes newlines
            Cow::from(leaf.text.trim())
        } else {
            Cow::from(leaf.text)
        };

        Entry {
            reference: leaf.reference,
            text: new_text,
            start_position: leaf.reference.start_position(),
            end_position: leaf.reference.start_position(),
            kind_id: leaf.reference.kind_id(),
        }
    }

    /// A helper method to determine whether a node type should be filtered out based on the user's filtering
    /// preferences.
    ///
    /// This method will first check if the node has been specified for exclusion, which takes precedence. Then it will
    /// check if the node kind is explicitly included. If either the exclusion or inclusion sets aren't specified,
    /// then the filter will not be applied.
    fn should_include_node(&self, node: &dyn TSNodeTrait) -> bool {
        let should_exclude = self
            .exclude_kinds
            .as_ref()
            .is_some_and(|x| x.contains(node.kind()))
            || self
                .include_kinds
                .as_ref()
                .is_some_and(|x| !x.contains(node.kind()));
        !should_exclude
    }
}

/// Create a `DiffVector` from a `tree_sitter` tree
///
/// This method calls a helper function that does an in-order traversal of the tree and adds
/// leaf nodes to a vector
#[time("info", "ast::{}")]
fn from_ts_tree<'a>(
    tree: &'a TSTree,
    text: &'a str,
    pseudo_leaf_types: &HashSet<String>,
) -> Vector<'a> {
    let leaves = RefCell::new(Vec::new());
    build(&leaves, tree.root_node(), text, pseudo_leaf_types);
    Vector {
        leaves: leaves.into_inner(),
        source_text: text,
    }
}

/// The leaves of an AST vector
///
/// This is used as an intermediate struct for flattening the tree structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorLeaf<'a> {
    pub reference: TSNode<'a>,
    pub text: &'a str,
}

/// A proxy for (Point)[`tree_sitter::Point`] for [serde].
///
/// This is a copy of an external struct that we use with serde so we can create json objects with
/// serde.
#[derive(Serialize, Deserialize)]
#[serde(remote = "Point")]
struct PointWrapper {
    pub row: usize,
    pub column: usize,
}

/// A mapping between a tree-sitter node and the text it corresponds to
///
/// This is also all of the metadata the diff rendering interface has access to, and also defines
/// the data that will be output by the JSON serializer.
#[derive(Debug, Clone, Serialize)]
pub struct Entry<'node> {
    /// The node an entry in the diff vector refers to
    ///
    /// We keep a reference to the leaf node so that we can easily grab the text and other metadata
    /// surrounding the syntax
    #[serde(skip_serializing)]
    pub reference: TSNode<'node>,

    /// A reference to the text the node refers to
    ///
    /// This is different from the `source_text` that the [AstVector] refers to, as the
    /// entry only holds a reference to the specific range of text that the node covers.
    ///
    /// We use a [Cow] here instead of a direct string reference because we might have to rewrite
    /// the text based on input processing settings, but if we don't have to there's no need to
    /// allocate an extra string.
    pub text: Cow<'node, str>,

    /// The entry's start position in the document.
    #[serde(with = "PointWrapper")]
    pub start_position: Point,

    /// The entry's end position in the document.
    #[serde(with = "PointWrapper")]
    pub end_position: Point,

    /// The cached kind_id from the TSNode reference.
    ///
    /// Caching it here saves some time because it is queried repeatedly later. If we don't store
    /// it inline then we have to cross the FFI boundary which incurs some overhead.
    // PERF: Use cross language LTO to see if LLVM can optimize across the FFI boundary.
    pub kind_id: u16,
}

impl<'a> VectorLeaf<'a> {
    /// Split an entry into a vector of entries per grapheme.
    ///
    /// Each grapheme will get its own [Entry] struct. This method will resolve the
    /// indices/positioning of each grapheme from the `self.text` field.
    ///
    /// This effectively maps out the byte position for each node from the unicode text, accounting
    /// for both newlines and grapheme splits.
    fn split_on_graphemes(self, strip_whitespace: bool) -> Vec<Entry<'a>> {
        let mut entries: Vec<Entry<'a>> = Vec::new();

        // We have to split lines because newline characters might be in the text for a tree sitter
        // node. We try to split up each unicode grapheme and assign them a location in the text
        // with a row and column, so we need to make sure that we are properly resetting the column
        // offset for and offsetting the row for each new line in a tree sitter node's text.
        let lines = self.text.lines();

        for (line_offset, line) in lines.enumerate() {
            let indices: Vec<(usize, &str)> =
                us::UnicodeSegmentation::grapheme_indices(line, true).collect();
            entries.reserve(entries.len() + indices.len());

            for (idx, grapheme) in indices {
                // Every grapheme has to be at least one byte
                debug_assert!(!grapheme.is_empty());

                if strip_whitespace && grapheme.chars().all(char::is_whitespace) {
                    continue;
                }

                // We simply offset from the start position of the node if we are on the first
                // line, which implies no newline offset needs to be applied. If the line_offset is
                // more than 0, we know we've hit a newline so the starting position for the column
                // is 0, shifted over for the grapheme index.
                let start_column = if line_offset == 0 {
                    self.reference.start_position().column + idx
                } else {
                    idx
                };
                let row = self.reference.start_position().row + line_offset;
                let new_start_pos = Point {
                    row,
                    column: start_column,
                };
                let new_end_pos = Point {
                    row,
                    column: new_start_pos.column + grapheme.len(),
                };
                debug_assert!(new_start_pos.row <= new_end_pos.row);
                let entry = Entry {
                    reference: self.reference,
                    text: Cow::from(&line[idx..idx + grapheme.len()]),
                    start_position: new_start_pos,
                    end_position: new_end_pos,
                    kind_id: self.reference.kind_id(),
                };
                // We add the debug assert config here because there's no need to even get a
                // reference to the last element if we're not in debug mode.
                #[cfg(debug_assertions)]
                if let Some(last_entry) = entries.last() {
                    // Our invariant is that one of the following must hold true:
                    // 1. The last entry ended on a previous line (now we don't need to check the
                    //    column offset).
                    // 2. The last entry is on the same line, so the column offset for the entry we
                    //    are about to insert must be greater than or equal to the end column of
                    //    the last entry. It's valid for them to be equal because the end position
                    //    is not inclusive.
                    debug_assert!(
                        last_entry.end_position().row < entry.start_position().row
                            || (last_entry.end_position.row == entry.start_position().row
                                && last_entry.end_position.column <= entry.start_position().column)
                    );
                }
                entries.push(entry);
            }
        }
        entries
    }
}

impl<'a> From<VectorLeaf<'a>> for Entry<'a> {
    fn from(leaf: VectorLeaf<'a>) -> Self {
        Self {
            reference: leaf.reference,
            text: Cow::from(leaf.text),
            start_position: leaf.reference.start_position(),
            end_position: leaf.reference.start_position(),
            kind_id: leaf.reference.kind_id(),
        }
    }
}

impl<'a> Entry<'a> {
    /// Get the start position of an entry
    #[must_use]
    pub fn start_position(&self) -> Point {
        self.start_position
    }

    /// Get the end position of an entry
    #[must_use]
    pub fn end_position(&self) -> Point {
        self.end_position
    }
}

impl<'a> From<&'a Vector<'a>> for Vec<Entry<'a>> {
    fn from(ast_vector: &'a Vector<'a>) -> Self {
        ast_vector
            .leaves
            .iter()
            .flat_map(|entry| entry.split_on_graphemes(true))
            .collect()
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

    /// The identifier for the language that was inferred.
    pub resolved_language: String,
}

impl<'a> Vector<'a> {
    /// Create a `DiffVector` from a `tree_sitter` tree
    ///
    /// This method calls a helper function that does an in-order traversal of the tree and adds
    /// leaf nodes to a vector
    #[time("info", "ast::{}")]
    pub fn from_ts_tree(
        tree: &'a TSTree,
        text: &'a str,
        pseudo_leaf_types: &HashSet<String>,
    ) -> Self {
        let leaves = RefCell::new(Vec::new());
        build(&leaves, tree.root_node(), text, pseudo_leaf_types);
        Vector {
            leaves: leaves.into_inner(),
            source_text: text,
        }
    }

    /// Return the number of nodes in the diff vector
    #[must_use]
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// Return whether there are any leaves in the diff vector.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
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
fn build<'a>(
    vector: &RefCell<Vec<VectorLeaf<'a>>>,
    node: tree_sitter::Node<'a>,
    text: &'a str,
    pseudo_leaf_types: &HashSet<String>,
) {
    // If the node is a leaf, we can stop traversing
    if node.child_count() == 0 || pseudo_leaf_types.contains(node.kind()) {
        // We only push an entry if the referenced text range isn't empty, since there's no point
        // in having an empty text range. This also fixes a bug where the program would panic
        // because it would attempt to access the 0th index in an empty text range.
        if !node.byte_range().is_empty() {
            let node_text: &'a str = &text[node.byte_range()];
            // HACK: this is a workaround that was put in place to work around the Go parser which
            // puts newlines into their own nodes, which later causes errors when trying to print
            // these nodes. We just ignore those nodes.
            if node_text
                .replace("\r\n", "")
                .replace(['\n', '\r'], "")
                .is_empty()
            {
                return;
            }

            vector.borrow_mut().push(VectorLeaf {
                reference: node,
                text: node_text,
            });
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        build(vector, child, text, pseudo_leaf_types);
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

impl<T> AsRef<T> for EditType<T> {
    fn as_ref(&self) -> &T {
        match self {
            Self::Addition(x) | Self::Deletion(x) => x,
        }
    }
}

impl<T> Deref for EditType<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Addition(x) | Self::Deletion(x) => x,
        }
    }
}

impl<T> DerefMut for EditType<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Addition(x) | Self::Deletion(x) => x,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GrammarConfig;
    use tree_sitter::Parser;

    #[cfg(feature = "static-grammar-libs")]
    use crate::parse::generate_language;

    #[test]
    fn test_should_filter_node() {
        let exclude_kinds: HashSet<String> = HashSet::from(["comment".to_string()]);
        let mut mock_node = MockTSNodeTrait::new();
        mock_node.expect_kind().return_const("comment".to_owned());

        // basic scenario - expect that the excluded kind is ignored
        let processor = TreeSitterProcessor {
            split_graphemes: false,
            exclude_kinds: Some(exclude_kinds.clone()),
            include_kinds: None,
            ..Default::default()
        };
        assert!(!processor.should_include_node(&mock_node));

        // expect that it's still excluded if the included list also has an element that was excluded
        let processor = TreeSitterProcessor {
            split_graphemes: false,
            exclude_kinds: Some(exclude_kinds.clone()),
            include_kinds: Some(exclude_kinds),
            ..Default::default()
        };
        assert!(!processor.should_include_node(&mock_node));

        // Don't exclude anything, but only include types that our node is not
        let include_kinds: HashSet<String> = HashSet::from([
            "some_other_type".to_string(),
            "yet another type".to_string(),
        ]);
        let processor = TreeSitterProcessor {
            split_graphemes: false,
            exclude_kinds: None,
            include_kinds: Some(include_kinds),
            ..Default::default()
        };
        assert!(!processor.should_include_node(&mock_node));

        // include our node type
        let include_kinds: HashSet<String> = HashSet::from(["comment".to_string()]);
        let processor = TreeSitterProcessor {
            split_graphemes: false,
            exclude_kinds: None,
            include_kinds: Some(include_kinds),
            ..Default::default()
        };
        assert!(processor.should_include_node(&mock_node));

        // don't filter anything
        let processor = TreeSitterProcessor {
            split_graphemes: false,
            exclude_kinds: None,
            include_kinds: None,
            ..Default::default()
        };
        assert!(processor.should_include_node(&mock_node));
    }

    // NOTE: this has to be gated behind the 'static-grammar-libs' cargo feature, otherwise the
    // crate won't be built with the grammars bundled into the binary which means this won't be
    // able to load the markdown parser. It's possible that the markdown dynamic library is
    // available even if we don't compile the grammars statically but there's no guarantees of
    // which grammars are available dynamically, and we don't enforce that certain grammars have to
    // be available.
    #[cfg(feature = "static-grammar-libs")]
    #[test]
    fn test_strip_whitespace() {
        let md_parser = generate_language("python", &GrammarConfig::default()).unwrap();
        let mut parser = Parser::new();
        parser.set_language(&md_parser).unwrap();
        let text_a = "'''# A heading\nThis has no diff.'''";
        let text_b = "'''# A heading\nThis\nhas\r\nno diff.'''";
        let tree_a = parser.parse(text_a, None).unwrap();
        let tree_b = parser.parse(text_b, None).unwrap();
        {
            let processor = TreeSitterProcessor {
                strip_whitespace: true,
                ..Default::default()
            };
            let entries_a = processor.process(&tree_a, text_a, "python");
            let entries_b = processor.process(&tree_b, text_b, "python");
            assert_eq!(entries_a, entries_b);
        }
        {
            let processor = TreeSitterProcessor {
                strip_whitespace: false,
                ..Default::default()
            };
            let entries_a = processor.process(&tree_a, text_a, "python");
            let entries_b = processor.process(&tree_b, text_b, "python");
            assert_ne!(entries_a, entries_b);
        }
    }
}
