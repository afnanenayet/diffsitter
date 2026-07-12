//! Human- and machine-consumable classification of an edit mapping.

use serde::Serialize;
use tree_sitter::Point;

use super::mapping::EditMapping;
use super::tree::LabeledTree;

/// A location in a source document (row and column are zero-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Position {
    pub row: usize,
    pub column: usize,
}

impl From<Point> for Position {
    fn from(p: Point) -> Self {
        Position {
            row: p.row,
            column: p.column,
        }
    }
}

/// A displayable summary of one AST node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeSummary {
    /// The tree-sitter kind name.
    pub kind: String,
    /// First line of the node's source text, truncated to 60 chars.
    pub snippet: String,
    pub start: Position,
    pub end: Position,
}

/// One structural edit operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StructuralEditKind {
    /// A mapped node pair whose labels differ.
    Rename { old: NodeSummary, new: NodeSummary },
    /// An old-tree node with no counterpart.
    Delete { node: NodeSummary },
    /// A new-tree node with no counterpart.
    Insert { node: NodeSummary },
}

/// A structural edit with its enclosing context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructuralEdit {
    #[serde(flatten)]
    pub kind: StructuralEditKind,
    /// The nearest named ancestor of the edited node, when one exists.
    pub context: Option<NodeSummary>,
}

/// The full structural diff between two documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructuralDiff {
    /// Edits in document order.
    pub edits: Vec<StructuralEdit>,
    /// The tree edit distance (= number of edits).
    pub distance: u32,
}

/// First line of `text`, truncated to 60 characters (with `…` if truncated).
pub(super) fn snippet(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or("");
    let mut out: String = first_line.chars().take(60).collect();
    if first_line.chars().count() > 60 {
        out.push('…');
    }
    out
}

fn summarize(tree: &LabeledTree, node: usize) -> NodeSummary {
    NodeSummary {
        kind: tree.kind(node).to_string(),
        snippet: snippet(&tree.source()[tree.byte_range(node)]),
        start: tree.start_position(node).into(),
        end: tree.end_position(node).into(),
    }
}

/// The nearest named proper ancestor of `node`, if any.
fn context_of(tree: &LabeledTree, node: usize) -> Option<NodeSummary> {
    let mut current = node;
    while let Some(p) = tree.parent(current) {
        if tree.is_named(p) {
            return Some(summarize(tree, p));
        }
        current = p;
    }
    None
}

/// Classify an edit mapping into renames, deletions, and insertions.
pub fn classify(old: &LabeledTree, new: &LabeledTree, mapping: &EditMapping) -> StructuralDiff {
    let mut edits = Vec::new();
    for &(i, j) in &mapping.mapped {
        if old.label(i) != new.label(j) {
            edits.push(StructuralEdit {
                kind: StructuralEditKind::Rename {
                    old: summarize(old, i),
                    new: summarize(new, j),
                },
                context: context_of(old, i),
            });
        }
    }
    for &i in &mapping.deleted {
        edits.push(StructuralEdit {
            kind: StructuralEditKind::Delete {
                node: summarize(old, i),
            },
            context: context_of(old, i),
        });
    }
    for &j in &mapping.inserted {
        edits.push(StructuralEdit {
            kind: StructuralEditKind::Insert {
                node: summarize(new, j),
            },
            context: context_of(new, j),
        });
    }
    // Document order, deterministic tiebreaks. `sort_by_key` is stable, so
    // edits sharing an anchor and rank keep their push order, which is
    // ascending node ID within each kind (the mapping vectors are sorted).
    edits.sort_by_key(|e| {
        let (anchor, rank) = match &e.kind {
            StructuralEditKind::Rename { old, .. } => (old.start, 0u8),
            StructuralEditKind::Delete { node } => (node.start, 1),
            StructuralEditKind::Insert { node } => (node.start, 2),
        };
        (anchor.row, anchor.column, rank)
    });
    StructuralDiff {
        edits,
        distance: mapping.distance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::{LabeledTree, TreeDiffOptions, edit_mapping};

    #[test]
    fn classify_paper_example() {
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
        let mapping = edit_mapping(&t, &t_prime, &TreeDiffOptions::default()).unwrap();
        let diff = classify(&t, &t_prime, &mapping);
        assert_eq!(diff.distance, 2);
        assert_eq!(diff.edits.len(), 2);
        assert!(
            diff.edits
                .iter()
                .any(|e| matches!(e.kind, StructuralEditKind::Delete { .. }))
        );
        assert!(
            diff.edits
                .iter()
                .any(|e| matches!(e.kind, StructuralEditKind::Insert { .. }))
        );
    }

    #[test]
    fn identical_trees_classify_to_no_edits() {
        let t = LabeledTree::from_postorder(&[1, 2], &[Some(1), None]).unwrap();
        let mapping = edit_mapping(&t, &t, &TreeDiffOptions::default()).unwrap();
        let diff = classify(&t, &t, &mapping);
        assert_eq!(diff.distance, 0);
        assert!(diff.edits.is_empty());
    }

    #[test]
    fn snippet_truncates_long_first_line() {
        let long = "x".repeat(100);
        assert_eq!(snippet(&long), format!("{}…", "x".repeat(60)));
        assert_eq!(snippet("short"), "short");
        assert_eq!(snippet("first\nsecond"), "first");
        assert_eq!(snippet(""), "");
    }

    #[test]
    fn snippet_does_not_truncate_at_exactly_sixty_chars() {
        // The boundary case: exactly 60 chars must not gain a `…`.
        let sixty = "y".repeat(60);
        assert_eq!(snippet(&sixty), sixty);
        assert_eq!(snippet(&"z".repeat(61)), format!("{}…", "z".repeat(60)));
    }

    #[test]
    fn context_is_none_at_the_root() {
        // A two-node tree: node 0's only proper ancestor is the root (node 1),
        // and the root itself has no ancestor -> renaming the root yields no
        // context.
        let t = LabeledTree::from_postorder(&[1, 2], &[Some(1), None]).unwrap();
        let t_prime = LabeledTree::from_postorder(&[1, 9], &[Some(1), None]).unwrap();
        let mapping = edit_mapping(&t, &t_prime, &TreeDiffOptions::default()).unwrap();
        let diff = classify(&t, &t_prime, &mapping);
        let root_edit = diff
            .edits
            .iter()
            .find(
                |e| matches!(&e.kind, StructuralEditKind::Rename { old, .. } if old.start.row == 0),
            )
            .expect("the differing root should produce a rename");
        assert!(root_edit.context.is_none());
    }
}
