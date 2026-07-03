//! Postorder array representation of a processed syntax tree.

use std::ops::Range;
use tree_sitter::Point;

use super::TreeDiffError;

/// A labeled, ordered tree stored as structure-of-arrays in postorder.
///
/// Index `i` is the node with postorder ID `i` (zero-based); the root is the
/// last node. Labels are interned `u32`s that are only comparable between
/// trees built by the same builder invocation.
///
/// This follows the owned-data / borrowed-view split used elsewhere in the
/// crate ([`crate::input_processing::VectorData`] owns the source text and
/// tree-sitter tree; this struct borrows from it).
#[derive(Debug, Clone)]
pub struct LabeledTree<'a> {
    labels: Vec<u32>,
    parents: Vec<Option<usize>>,
    depths: Vec<usize>,
    sizes: Vec<usize>,
    llds: Vec<usize>,
    kinds: Vec<&'a str>,
    named: Vec<bool>,
    byte_ranges: Vec<Range<usize>>,
    starts: Vec<Point>,
    ends: Vec<Point>,
    source: &'a str,
}

impl LabeledTree<'static> {
    /// Build a tree from postorder labels and parent links.
    ///
    /// Kinds, positions, and source text are filled with placeholders; this
    /// constructor exists for algorithm tests and synthetic benchmarks. It
    /// validates that `parents` encodes a genuine postorder numbering.
    pub fn from_postorder(
        labels: &[u32],
        parents: &[Option<usize>],
    ) -> Result<Self, TreeDiffError> {
        if labels.len() != parents.len() {
            return Err(TreeDiffError::InvalidTree(format!(
                "labels ({}) and parents ({}) length mismatch",
                labels.len(),
                parents.len()
            )));
        }
        let n = labels.len();
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, parent) in parents.iter().enumerate() {
            match parent {
                None if i + 1 != n => {
                    return Err(TreeDiffError::InvalidTree(format!(
                        "non-root node {i} has no parent"
                    )));
                }
                Some(p) if *p <= i || *p >= n => {
                    return Err(TreeDiffError::InvalidTree(format!(
                        "node {i} has invalid parent {p}"
                    )));
                }
                Some(p) => children[*p].push(i),
                None => {}
            }
        }
        if n > 0 {
            // A DFS visiting children in sibling order must yield 0..n
            // exactly; otherwise subtrees are not contiguous postorder runs.
            let mut order = Vec::with_capacity(n);
            postorder_dfs(n - 1, &children, &mut order);
            if order.len() != n || order.iter().enumerate().any(|(seq, &id)| seq != id) {
                return Err(TreeDiffError::InvalidTree(
                    "parent links do not form a postorder numbering".into(),
                ));
            }
        }
        let (depths, sizes, llds) = derive_structure(parents);
        Ok(LabeledTree {
            labels: labels.to_vec(),
            parents: parents.to_vec(),
            depths,
            sizes,
            llds,
            kinds: vec![""; n],
            named: vec![true; n],
            byte_ranges: vec![0..0; n],
            starts: vec![Point::default(); n],
            ends: vec![Point::default(); n],
            source: "",
        })
    }
}

/// Compute `(depths, sizes, llds)` from validated postorder parent links.
pub(super) fn derive_structure(parents: &[Option<usize>]) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
    let n = parents.len();
    let mut depths = vec![0usize; n];
    // Parents always have larger postorder IDs, so a descending sweep sees
    // every parent before its children.
    for i in (0..n).rev() {
        if let Some(p) = parents[i] {
            depths[i] = depths[p] + 1;
        }
    }
    let mut sizes = vec![1usize; n];
    // Ascending sweep sees every child before its parent.
    for i in 0..n {
        if let Some(p) = parents[i] {
            sizes[p] += sizes[i];
        }
    }
    let llds = (0..n).map(|i| i + 1 - sizes[i]).collect();
    (depths, sizes, llds)
}

fn postorder_dfs(id: usize, children: &[Vec<usize>], out: &mut Vec<usize>) {
    for &c in &children[id] {
        postorder_dfs(c, children, out);
    }
    out.push(id);
}

impl<'a> LabeledTree<'a> {
    /// The number of nodes in the tree.
    #[must_use]
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    /// Whether the tree has no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    /// The interned label of node `i`.
    #[must_use]
    pub fn label(&self, i: usize) -> u32 {
        self.labels[i]
    }

    /// Postorder ID of the leftmost leaf descendant of node `i`.
    #[must_use]
    pub fn lld(&self, i: usize) -> usize {
        self.llds[i]
    }

    /// Depth of node `i` (the root has depth 0).
    #[must_use]
    pub fn depth(&self, i: usize) -> usize {
        self.depths[i]
    }

    /// Subtree size of node `i` (including itself).
    #[must_use]
    pub fn size(&self, i: usize) -> usize {
        self.sizes[i]
    }

    /// Parent of node `i`, or `None` for the root.
    #[must_use]
    pub fn parent(&self, i: usize) -> Option<usize> {
        self.parents[i]
    }

    /// The tree-sitter kind name of node `i`.
    #[must_use]
    pub fn kind(&self, i: usize) -> &'a str {
        self.kinds[i]
    }

    /// Whether node `i` is a named tree-sitter node.
    #[must_use]
    pub fn is_named(&self, i: usize) -> bool {
        self.named[i]
    }

    /// Source byte range covered by node `i`.
    #[must_use]
    pub fn byte_range(&self, i: usize) -> Range<usize> {
        self.byte_ranges[i].clone()
    }

    /// Start position of node `i` in the source document.
    #[must_use]
    pub fn start_position(&self, i: usize) -> Point {
        self.starts[i]
    }

    /// End position of node `i` in the source document.
    #[must_use]
    pub fn end_position(&self, i: usize) -> Point {
        self.ends[i]
    }

    /// The source text the tree was built from.
    #[must_use]
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Neighborhood vector `(left, descendants, ancestors, right)` of node
    /// `i` (paper Def. 4.1), derived in O(1) from postorder ID, subtree
    /// size, and depth.
    #[allow(dead_code)]
    pub(crate) fn neighborhood(&self, i: usize) -> (i64, i64, i64, i64) {
        let left = (i + 1 - self.sizes[i]) as i64;
        let descendants = (self.sizes[i] - 1) as i64;
        let ancestors = self.depths[i] as i64;
        let right = (self.len() - 1 - i - self.depths[i]) as i64;
        (left, descendants, ancestors, right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paper running example, tree T (Fig. 1). Labels: l=0 s=1 q=2 u=3 t=4 p=5 v=6 o=7.
    fn paper_tree_t() -> LabeledTree<'static> {
        LabeledTree::from_postorder(
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
        .unwrap()
    }

    #[test]
    fn paper_tree_structure() {
        let t = paper_tree_t();
        assert_eq!(t.len(), 8);
        // Sizes: x2 = {x1,x2}, x4 = {x3,x4}, x5 = {x1..x5}, root = all.
        assert_eq!(t.size(2), 2);
        assert_eq!(t.size(4), 2);
        assert_eq!(t.size(5), 5);
        assert_eq!(t.size(7), 8);
        // Depths (root = 0).
        assert_eq!(t.depth(7), 0);
        assert_eq!(t.depth(5), 1);
        assert_eq!(t.depth(4), 2);
        assert_eq!(t.depth(3), 3);
        // Leftmost leaf descendants.
        assert_eq!(t.lld(7), 0);
        assert_eq!(t.lld(5), 1);
        assert_eq!(t.lld(4), 3);
        assert_eq!(t.lld(6), 6);
        // Parents round-trip.
        assert_eq!(t.parent(7), None);
        assert_eq!(t.parent(3), Some(4));
    }

    #[test]
    fn paper_tree_neighborhood_vectors() {
        let t = paper_tree_t();
        // Example 3.1: x4 has 1 descendant, 2 ancestors, 3 to the left, 1 to the right.
        assert_eq!(t.neighborhood(4), (3, 1, 2, 1));
        // x5: v = (1, 4, 1, 1).
        assert_eq!(t.neighborhood(5), (1, 4, 1, 1));
        // Root: everything below it.
        assert_eq!(t.neighborhood(7), (0, 7, 0, 0));
    }

    #[test]
    fn empty_tree_is_valid() {
        let t = LabeledTree::from_postorder(&[], &[]).unwrap();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn rejects_length_mismatch() {
        assert!(matches!(
            LabeledTree::from_postorder(&[0, 1], &[None]),
            Err(TreeDiffError::InvalidTree(_))
        ));
    }

    #[test]
    fn rejects_parent_not_after_child() {
        // Parent must have a larger postorder ID than its child.
        assert!(matches!(
            LabeledTree::from_postorder(&[0, 1], &[None, Some(0)]),
            Err(TreeDiffError::InvalidTree(_))
        ));
    }

    #[test]
    fn rejects_non_postorder_numbering() {
        // parents = [2, 3, 3, root]: subtree of node 2 would be {0, 2} with
        // node 1 interleaved, which is not a contiguous postorder range.
        assert!(matches!(
            LabeledTree::from_postorder(&[0, 0, 0, 0], &[Some(2), Some(3), Some(3), None]),
            Err(TreeDiffError::InvalidTree(_))
        ));
    }

    #[test]
    fn rejects_missing_parent_on_non_root() {
        assert!(matches!(
            LabeledTree::from_postorder(&[0, 0, 0], &[None, Some(2), None]),
            Err(TreeDiffError::InvalidTree(_))
        ));
    }

    #[test]
    fn single_node_tree() {
        let t = LabeledTree::from_postorder(&[42], &[None]).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t.label(0), 42);
        assert_eq!(t.lld(0), 0);
        assert_eq!(t.size(0), 1);
        assert_eq!(t.neighborhood(0), (0, 0, 0, 0));
    }
}
