# TopDiff Tree-Diff Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in tree-edit-distance diff engine (TopDiff/TopDiff+/AutoStop from Pawlik & Augsten, CIKM '20) with a new structural renderer, alongside the existing Myers engine.

**Architecture:** A new `src/tree_diff/` module builds a postorder-array `LabeledTree` from the tree-sitter CST (reusing `TreeSitterProcessor` filter semantics), computes exact TED with banded dynamic programming (TouzetDepth + TopDiff + cost-model switch + τ-doubling AutoStop), recovers the node mapping by backtrace, and classifies it into a `StructuralDiff` rendered by a new `structural` renderer. Engine selected via `diff-engine` config / `--diff-engine` flag.

**Tech Stack:** Rust edition 2024 (MSRV 1.85.1), tree-sitter, thiserror, serde, strum, console, insta, proptest, test_case, criterion, cargo-fuzz. **No new dependencies.**

**Spec:** `docs/superpowers/specs/2026-07-02-topdiff-tree-diff-design.md`
**Paper:** Pawlik & Augsten, *Minimal Edit-Based Diffs for Large Trees* (algorithms referenced as Alg. 1–6 below).

## Global Constraints

- Edition 2024, MSRV 1.85.1. Use `unsafe extern "C"` only for FFI (none expected here). **No `unsafe` in this feature** — fully safe first version.
- Library errors use `thiserror` (`TreeDiffError`); `anyhow` only in `src/bin/`.
- No `.unwrap()`/`.expect()` in library code unless the invariant is proven by preceding logic and guarded by `debug_assert!`.
- Config structs: `#[serde(rename_all = "kebab-case", default)]`. **Any `Config` change must update `assets/sample_config.json5`** (CI parses it via `test_sample_config`).
- Lints must pass: `cargo clippy --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`.
- Tests run with `cargo nextest run` (doc tests separately via `cargo test --doc --all-features`).
- Grammar-dependent tests gate on `static-grammar-libs` (a default feature).
- **VCS is jujutsu (jj), colocated.** Never use raw git mutations. Commit flow per task: `jj st` (verify changes), `jj desc -m "<conventional commit message>"`, `jj new`. Always `--no-pager` for output commands. Conventional commit format, lowercase description, no trailing period.
- Write code for humans: clear names, doc comments on public items, cite paper sections/algorithms in comments where the code implements them.

## Paper Cheat Sheet (read before Task 3)

All trees are postorder-indexed, zero-based; root is the last node.

- **Neighborhood vector** of node `x`: `v_x = (l, d, a, r)` = (#nodes to the left, #descendants, #ancestors, #nodes to the right). Derived O(1): `l = post(x) − |Tx| + 1`, `d = |Tx| − 1`, `a = depth(x)` (root depth 0), `r = |T| − post(x) − depth(x) − 1`.
- **Neighborhood distance**: L1 distance `‖v_x − v_y‖₁`. If a pair `(x,y)` is in a mapping of cost ≤ τ, then `‖v_x − v_y‖₁ ≤ τ` (Lemma 4.2). Also `|post(x) − post(y)| ≤ τ`.
- **Edits budget** for subtree pair `(Tx, Ty)`: `ε(x,y,τ) = τ − |Δl| − |Δa| − |Δr|`. When the neighborhood filter passes, `ε ≥ |Δd| ≥ 0`.
- **FD matrix** (forest distances inside one subtree pair): indexed by *local* postorder positions `1..=|Tx|` × `1..=|Ty|` plus an empty-prefix row/col 0. **Edits pruning**: only cells with local index difference ≤ ε are computed (a band of width 2ε+1 around the local diagonal — see paper Fig. 4, where ε=0 keeps only the diagonal).
- **TD matrix** (subtree distances): global `|T| × |T′|`, banded to `|post(x) − post(y)| ≤ τ`.
- **Anchored cell**: FD cell `(i,j)` where `lld(i) = lld(x)` and `lld(j) = lld(y)` (both prefixes are whole subtrees sharing the pair's leftmost leaves). Anchored cell values ARE subtree distances and are stored into TD.
- **Depth pruning** (TouzetDepth only): a node `i ∈ Tx` with `depth(i) − depth(x) − 1 ≥ ε` cannot be mapped inside this subproblem → its FD row is forced deletions.
- **Top node pairs** (Def. 5.1, Alg. 3): the maximal pairs per `(lld(x), lld(y))` key; computing FD only for these avoids all redundant subproblems (TopDiff, Alg. 4).
- **AutoStop** (Alg. 6): τ starts at `max(||T|−|T′||, 1)`, doubles while `δτ > τ`. Stopping condition (Thm. 7.2): `δτ ≤ τ ⇒ δτ = δ`.

### Verified worked example (paper Fig. 1) — use in tests

Labels interned as: l=0, s=1, q=2, u=3, t=4, p=5, v=6, o=7, k=8.

- Tree `T` postorder labels `[0,1,2,3,4,5,6,7]`, parents `[Some(7), Some(2), Some(5), Some(4), Some(5), Some(7), Some(7), None]`.
- Tree `T′` postorder labels `[1,2,3,4,5,8,6,7]`, parents `[Some(1), Some(4), Some(3), Some(4), Some(7), Some(6), Some(7), None]`.
- `δ(T, T′) = 2` (delete x0, insert y5).
- Top node pairs for τ=2 (as produced by Alg. 3 with x-major decreasing iteration, before sorting): `[(7,7), (6,6), (5,4), (4,3), (2,3)]` (Example 5.2).
- Neighborhood vectors (hand-verified): `v_x4 = (3,1,2,1)`, `v_y3 = (2,1,2,2)`, `‖v_x4 − v_y3‖₁ = 2`, `ε(x4,y3,2) = 0`; `v_x5 = (1,4,1,1)`, `v_y4 = (0,4,1,2)`, `‖v_x5 − v_y4‖₁ = 2`, `ε(x5,y4,2) = 0`.
- Optimal mapping: x1→y0, x2→y1, x3→y2, x4→y3, x5→y4, x6→y6, x7→y7 (all zero-cost renames); x0 deleted, y5 inserted.

## File Structure

```
src/tree_diff/
    mod.rs        public API surface, TreeDiffError, TreeDiffOptions, tree_diff()
    tree.rs       LabeledTree (postorder SoA), from_postorder, CST builder
    band.rs       BandMatrix (banded u32 matrix, INF sentinel)
    ted.rs        forest_distance DP core, neighborhood_distance, edits_budget
    touzet.rs     TouzetDepth (Alg. 2)
    topdiff.rs    compute_top_node_pairs (Alg. 3) + TopDiff (Alg. 4)
    cost.rs       cost estimates + TopDiff+ switch (Sect. 6, Alg. 5)
    autostop.rs   τ-doubling driver (Alg. 6)
    mapping.rs    EditMapping backtrace recovery
    output.rs     StructuralDiff / StructuralEdit / NodeSummary + classification
src/render/structural.rs   new renderer
src/render/mod.rs          DiffPayload enum, Renderers::Structural
src/input_processing.rs    should_include_kind made pub(crate)
src/config.rs              DiffEngine enum, diff_engine + tree_diff fields
src/cli.rs                 --diff-engine flag
src/bin/diffsitter.rs      engine dispatch
tests/tree_diff_test.rs    integration + snapshot tests
tests/tree_diff_proptest.rs  ZS oracle + property tests
benches/tree_diff_bench.rs
fuzz/fuzz_targets/fuzz_tree_diff.rs
test_data/tree_diff/       fixture pairs
```

---

### Task 1: LabeledTree postorder representation

**Files:**
- Create: `src/tree_diff/mod.rs`, `src/tree_diff/tree.rs`
- Modify: `src/lib.rs` (add `pub mod tree_diff;` after `pub mod render;`, keeping the module list alphabetical)

**Interfaces (produced):**
- `pub struct LabeledTree<'a>` — postorder structure-of-arrays tree
- `pub fn LabeledTree::from_postorder(labels: &[u32], parents: &[Option<usize>]) -> Result<LabeledTree<'static>, TreeDiffError>` — synthetic constructor for tests/benches; validates postorder numbering
- Accessors: `len() -> usize`, `is_empty() -> bool`, `label(i) -> u32`, `lld(i) -> usize`, `depth(i) -> usize`, `size(i) -> usize`, `parent(i) -> Option<usize>`, `kind(i) -> &'a str`, `is_named(i) -> bool`, `byte_range(i) -> Range<usize>`, `start_position(i) -> Point`, `end_position(i) -> Point`, `source() -> &'a str`
- `pub(crate) fn neighborhood(&self, i: usize) -> (i64, i64, i64, i64)` — `(l, d, a, r)` per paper Def. 4.1
- `pub enum TreeDiffError` with variants: `InvalidTree(String)`, `BoundExceeded { tau: u32, limit: u32 }`, `MappingBacktrace`, `RendererMismatch { engine: String, renderer: String }` (later tasks use the last three; public enum variants don't trigger dead-code warnings)

- [ ] **Step 1: Write `src/tree_diff/mod.rs`**

```rust
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

pub use tree::LabeledTree;

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
```

- [ ] **Step 2: Write the failing tests in `src/tree_diff/tree.rs`**

Create the file with a stub struct and the test module (tests first — they will fail to compile until Step 4, which is the TDD failure signal for this task):

```rust
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
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo check`
Expected: FAIL — `LabeledTree` not defined / module missing.

- [ ] **Step 4: Implement `LabeledTree` in `src/tree_diff/tree.rs`**

```rust
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
pub(super) fn derive_structure(
    parents: &[Option<usize>],
) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
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
    pub(crate) fn neighborhood(&self, i: usize) -> (i64, i64, i64, i64) {
        let left = (i + 1 - self.sizes[i]) as i64;
        let descendants = (self.sizes[i] - 1) as i64;
        let ancestors = self.depths[i] as i64;
        let right = (self.len() - 1 - i - self.depths[i]) as i64;
        (left, descendants, ancestors, right)
    }
}
```

Note: Task 2 adds more fields/constructors; keep field names exactly as above. If clippy flags unused fields (`kinds`, `named`, etc. are only read from Task 2 onward), silence at the field level is NOT allowed — instead the accessors above already read every field, which satisfies the lint.

- [ ] **Step 5: Register the module in `src/lib.rs`**

Add after `pub mod render;` (keep the list alphabetical):

```rust
pub mod tree_diff;
```

- [ ] **Step 6: Run tests**

Run: `cargo nextest run tree_diff`
Expected: PASS (8 tests).

- [ ] **Step 7: Lint and format**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all`
Expected: clean.

- [ ] **Step 8: Commit**

```sh
jj st
jj desc -m "feat: add labeled tree representation for tree diffs

First step of the TopDiff tree-diff engine (see
docs/superpowers/specs/2026-07-02-topdiff-tree-diff-design.md):
a postorder structure-of-arrays tree with derived subtree sizes,
depths, and leftmost-leaf descendants, plus a validated synthetic
constructor for algorithm tests.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 2: Build LabeledTrees from tree-sitter parses

**Files:**
- Modify: `src/tree_diff/tree.rs` (builder), `src/tree_diff/mod.rs` (re-export), `src/input_processing.rs` (expose kind filter)

**Interfaces:**
- Consumes: `LabeledTree` fields + `derive_structure` from Task 1; `TreeSitterProcessor { strip_whitespace, pseudo_leaf_types, exclude_kinds, include_kinds }` and `VectorData { text, tree, resolved_language }` from `input_processing`
- Produces: `pub fn build_labeled_trees<'a>(processor: &TreeSitterProcessor, old: &'a VectorData, new: &'a VectorData) -> (LabeledTree<'a>, LabeledTree<'a>)` (re-exported from `tree_diff::mod`); `pub(crate) fn TreeSitterProcessor::should_include_kind(&self, kind: &str) -> bool`

**Semantics (mirror the Myers path in `input_processing.rs` exactly):**
1. A CST node is a *leaf* if `child_count() == 0` or its kind is in `pseudo_leaf_types[resolved_language]` (mirrors `build()`).
2. Drop a leaf if: byte range empty; or its text minus `\r\n`/`\n`/`\r` is empty (the Go-newline workaround in `build()`); or `should_include_kind(kind)` is false; or `strip_whitespace` is on and the trimmed text is empty.
3. Leaf label = `(kind_id, trimmed text)` when `strip_whitespace`, else `(kind_id, raw text)`. Internal label = `(kind_id, None)`.
4. Drop internal nodes whose children all got dropped (recursively). If the root is dropped, the tree is empty.
5. One `Interner` spans both trees so labels are cross-comparable. `split_graphemes` is irrelevant at node granularity and is ignored.

- [ ] **Step 1: Expose the kind filter in `src/input_processing.rs`**

Replace the body of `should_include_node` with a delegation and add the new method directly above it:

```rust
    /// Whether a node kind passes the user's include/exclude filters.
    ///
    /// Exclusion takes precedence over inclusion; missing sets apply no
    /// filter. Shared by the Myers leaf pipeline and the tree diff engine so
    /// both honor the same configuration.
    pub(crate) fn should_include_kind(&self, kind: &str) -> bool {
        let should_exclude = self
            .exclude_kinds
            .as_ref()
            .is_some_and(|x| x.contains(kind))
            || self
                .include_kinds
                .as_ref()
                .is_some_and(|x| !x.contains(kind));
        !should_exclude
    }

    /// A helper method to determine whether a node type should be filtered out based on the user's filtering
    /// preferences.
    ///
    /// This method will first check if the node has been specified for exclusion, which takes precedence. Then it will
    /// check if the node kind is explicitly included. If either the exclusion or inclusion sets aren't specified,
    /// then the filter will not be applied.
    fn should_include_node(&self, node: &dyn TSNodeTrait) -> bool {
        self.should_include_kind(node.kind())
    }
```

Run: `cargo nextest run input_processing` — existing filter tests must still pass.

- [ ] **Step 2: Write failing builder tests in `src/tree_diff/tree.rs`**

Append inside the existing `mod tests`, in a nested feature-gated module (parsing needs compiled grammars):

```rust
    #[cfg(feature = "static-grammar-libs")]
    mod builder {
        use super::super::*;
        use crate::input_processing::{TreeSitterProcessor, VectorData};
        use crate::parse::{GrammarConfig, generate_language};
        use std::collections::HashSet;
        use std::path::PathBuf;

        fn parse(text: &str, lang: &str, file_name: &str) -> VectorData {
            let language = generate_language(lang, &GrammarConfig::default()).unwrap();
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&language).unwrap();
            let tree = parser.parse(text, None).unwrap();
            VectorData {
                text: text.into(),
                tree,
                path: PathBuf::from(file_name),
                resolved_language: lang.into(),
            }
        }

        #[test]
        fn identical_sources_produce_identical_labels() {
            let a = parse("fn a() -> i32 { 1 }\n", "rust", "a.rs");
            let b = parse("fn a() -> i32 { 1 }\n", "rust", "b.rs");
            let (ta, tb) = build_labeled_trees(&TreeSitterProcessor::default(), &a, &b);
            assert!(!ta.is_empty());
            assert_eq!(ta.len(), tb.len());
            for i in 0..ta.len() {
                assert_eq!(ta.label(i), tb.label(i), "label mismatch at node {i}");
                assert_eq!(ta.kind(i), tb.kind(i));
            }
        }

        #[test]
        fn renamed_identifier_changes_exactly_one_label() {
            let a = parse("fn a() -> i32 { 1 }\n", "rust", "a.rs");
            let b = parse("fn b() -> i32 { 1 }\n", "rust", "b.rs");
            let (ta, tb) = build_labeled_trees(&TreeSitterProcessor::default(), &a, &b);
            assert_eq!(ta.len(), tb.len());
            let diffs: Vec<usize> = (0..ta.len()).filter(|&i| ta.label(i) != tb.label(i)).collect();
            assert_eq!(diffs.len(), 1, "expected exactly one differing label");
            assert_eq!(ta.kind(diffs[0]), "identifier");
        }

        #[test]
        fn postorder_invariants_hold() {
            let a = parse(
                "fn a() -> i32 { 1 }\nfn main() { println!(\"{}\", a()); }\n",
                "rust",
                "a.rs",
            );
            let (t, _) = build_labeled_trees(&TreeSitterProcessor::default(), &a, &a);
            let n = t.len();
            assert!(n > 0);
            assert_eq!(t.parent(n - 1), None, "root must be the last node");
            for i in 0..n {
                if let Some(p) = t.parent(i) {
                    assert!(p > i, "parent {p} must come after child {i} in postorder");
                }
                assert_eq!(t.lld(i), i + 1 - t.size(i));
                // Subtree byte ranges nest within the parent's range.
                if let Some(p) = t.parent(i) {
                    assert!(t.byte_range(i).start >= t.byte_range(p).start);
                    assert!(t.byte_range(i).end <= t.byte_range(p).end);
                }
            }
        }

        #[test]
        fn excluded_kinds_are_dropped() {
            let a = parse("fn a() -> i32 { 1 }\n", "rust", "a.rs");
            let processor = TreeSitterProcessor {
                exclude_kinds: Some(HashSet::from(["identifier".into()])),
                ..Default::default()
            };
            let (t, _) = build_labeled_trees(&processor, &a, &a);
            for i in 0..t.len() {
                assert_ne!(t.kind(i), "identifier");
            }
        }

        #[test]
        fn whitespace_only_source_yields_empty_tree() {
            let a = parse("   \n\n", "rust", "a.rs");
            let (t, _) = build_labeled_trees(&TreeSitterProcessor::default(), &a, &a);
            assert!(t.is_empty());
        }

        #[test]
        fn pseudo_leaf_types_stop_descent() {
            // The default processor treats markdown "inline" nodes as leaves.
            let a = parse("# Title\n\nSome *emphasized* text.\n", "markdown", "a.md");
            let (t, _) = build_labeled_trees(&TreeSitterProcessor::default(), &a, &a);
            let inline_nodes: Vec<usize> =
                (0..t.len()).filter(|&i| t.kind(i) == "inline").collect();
            assert!(!inline_nodes.is_empty(), "expected at least one inline node");
            for i in inline_nodes {
                assert_eq!(t.size(i), 1, "pseudo-leaf must have no children");
            }
        }
    }
```

Note: if `generate_language` / `set_language` signatures differ, mirror the exact call pattern used in `fuzz/fuzz_targets/fuzz_parse_and_navigate.rs` — that file compiles against the same APIs.

- [ ] **Step 3: Run to verify failure**

Run: `cargo check --tests`
Expected: FAIL — `build_labeled_trees` not defined.

- [ ] **Step 4: Implement the builder in `src/tree_diff/tree.rs`**

```rust
use crate::input_processing::{TreeSitterProcessor, VectorData};
use std::collections::{HashMap, HashSet};
use tree_sitter::Node as TSNode;

/// Interns `(kind_id, leaf text)` pairs to dense `u32` labels.
///
/// A single interner spans both input trees so that equal labels mean equal
/// content across the pair. Internal nodes intern `(kind_id, None)`.
#[derive(Default)]
struct Interner<'a> {
    map: HashMap<(u16, Option<&'a str>), u32>,
}

impl<'a> Interner<'a> {
    fn intern(&mut self, kind_id: u16, text: Option<&'a str>) -> u32 {
        let next = self.map.len() as u32;
        *self.map.entry((kind_id, text)).or_insert(next)
    }
}

/// A node surviving the processor filters, before postorder flattening.
struct PendingNode<'a> {
    kind_id: u16,
    kind: &'a str,
    named: bool,
    byte_range: Range<usize>,
    start: Point,
    end: Point,
    /// `Some(text)` for leaves (the label text), `None` for internal nodes.
    leaf_text: Option<&'a str>,
    children: Vec<PendingNode<'a>>,
}

/// Build comparable [`LabeledTree`]s for a pair of parsed documents.
///
/// Applies the same filter semantics as the Myers leaf pipeline
/// ([`TreeSitterProcessor::process`]): pseudo-leaf kinds stop descent,
/// excluded kinds and whitespace-only leaves are dropped, and leaf label text
/// is whitespace-trimmed when `strip_whitespace` is on. Internal nodes whose
/// children are all dropped are themselves dropped. `split_graphemes` is
/// irrelevant at node granularity and is ignored.
pub fn build_labeled_trees<'a>(
    processor: &TreeSitterProcessor,
    old: &'a VectorData,
    new: &'a VectorData,
) -> (LabeledTree<'a>, LabeledTree<'a>) {
    let mut interner = Interner::default();
    let old_tree = build_one(processor, old, &mut interner);
    let new_tree = build_one(processor, new, &mut interner);
    (old_tree, new_tree)
}

fn build_one<'a>(
    processor: &TreeSitterProcessor,
    data: &'a VectorData,
    interner: &mut Interner<'a>,
) -> LabeledTree<'a> {
    let empty_set = HashSet::new();
    let pseudo_leaf_types = processor
        .pseudo_leaf_types
        .get(&data.resolved_language)
        .unwrap_or(&empty_set);
    let pending = convert(data.tree.root_node(), &data.text, processor, pseudo_leaf_types);
    match pending {
        Some(root) => flatten(root, &data.text, interner),
        None => LabeledTree {
            labels: Vec::new(),
            parents: Vec::new(),
            depths: Vec::new(),
            sizes: Vec::new(),
            llds: Vec::new(),
            kinds: Vec::new(),
            named: Vec::new(),
            byte_ranges: Vec::new(),
            starts: Vec::new(),
            ends: Vec::new(),
            source: &data.text,
        },
    }
}

/// Recursively convert a CST node, applying the processor's filters.
/// Returns `None` when the node (and its whole subtree) is dropped.
fn convert<'a>(
    node: TSNode<'a>,
    text: &'a str,
    processor: &TreeSitterProcessor,
    pseudo_leaf_types: &HashSet<String>,
) -> Option<PendingNode<'a>> {
    let is_leaf = node.child_count() == 0 || pseudo_leaf_types.contains(node.kind());
    if is_leaf {
        if node.byte_range().is_empty() {
            return None;
        }
        let node_text: &'a str = &text[node.byte_range()];
        // Mirror the Go-parser newline workaround in `input_processing::build`.
        if node_text
            .replace("\r\n", "")
            .replace(['\n', '\r'], "")
            .is_empty()
        {
            return None;
        }
        if !processor.should_include_kind(node.kind()) {
            return None;
        }
        let label_text = if processor.strip_whitespace {
            node_text.trim()
        } else {
            node_text
        };
        if processor.strip_whitespace && label_text.is_empty() {
            return None;
        }
        return Some(PendingNode {
            kind_id: node.kind_id(),
            kind: node.kind(),
            named: node.is_named(),
            byte_range: node.byte_range(),
            start: node.start_position(),
            end: node.end_position(),
            leaf_text: Some(label_text),
            children: Vec::new(),
        });
    }
    let mut cursor = node.walk();
    let children: Vec<PendingNode<'a>> = node
        .children(&mut cursor)
        .filter_map(|child| convert(child, text, processor, pseudo_leaf_types))
        .collect();
    if children.is_empty() {
        // Every descendant was filtered out; drop the now-empty internal node.
        return None;
    }
    Some(PendingNode {
        kind_id: node.kind_id(),
        kind: node.kind(),
        named: node.is_named(),
        byte_range: node.byte_range(),
        start: node.start_position(),
        end: node.end_position(),
        leaf_text: None,
        children,
    })
}

/// Flatten a pending tree into postorder arrays.
fn flatten<'a>(
    root: PendingNode<'a>,
    source: &'a str,
    interner: &mut Interner<'a>,
) -> LabeledTree<'a> {
    struct Arrays<'a> {
        labels: Vec<u32>,
        parents: Vec<Option<usize>>,
        kinds: Vec<&'a str>,
        named: Vec<bool>,
        byte_ranges: Vec<Range<usize>>,
        starts: Vec<Point>,
        ends: Vec<Point>,
    }

    fn push_subtree<'a>(
        node: PendingNode<'a>,
        out: &mut Arrays<'a>,
        interner: &mut Interner<'a>,
    ) -> usize {
        let child_ids: Vec<usize> = node
            .children
            .into_iter()
            .map(|child| push_subtree(child, out, interner))
            .collect();
        let id = out.labels.len();
        out.labels.push(interner.intern(node.kind_id, node.leaf_text));
        out.parents.push(None);
        out.kinds.push(node.kind);
        out.named.push(node.named);
        out.byte_ranges.push(node.byte_range);
        out.starts.push(node.start);
        out.ends.push(node.end);
        for c in child_ids {
            out.parents[c] = Some(id);
        }
        id
    }

    let mut arrays = Arrays {
        labels: Vec::new(),
        parents: Vec::new(),
        kinds: Vec::new(),
        named: Vec::new(),
        byte_ranges: Vec::new(),
        starts: Vec::new(),
        ends: Vec::new(),
    };
    push_subtree(root, &mut arrays, interner);
    let (depths, sizes, llds) = derive_structure(&arrays.parents);
    LabeledTree {
        labels: arrays.labels,
        parents: arrays.parents,
        depths,
        sizes,
        llds,
        kinds: arrays.kinds,
        named: arrays.named,
        byte_ranges: arrays.byte_ranges,
        starts: arrays.starts,
        ends: arrays.ends,
        source,
    }
}
```

Add the re-export in `src/tree_diff/mod.rs`:

```rust
pub use tree::{LabeledTree, build_labeled_trees};
```

- [ ] **Step 5: Run tests**

Run: `cargo nextest run tree_diff`
Expected: PASS (14 tests). CST recursion depth note: `convert`/`push_subtree` recurse to tree depth; source ASTs are at most a few hundred deep, matching the recursion already used in `input_processing::build`.

- [ ] **Step 6: Lint, format, commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all`

```sh
jj st
jj desc -m "feat: build labeled trees from tree-sitter parses

Convert the CST into the postorder tree-diff representation, honoring
the same TreeSitterProcessor semantics as the Myers pipeline
(pseudo-leaves, kind filters, whitespace stripping, the Go newline
workaround). A shared interner makes labels comparable across the
input pair.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 3: BandMatrix

**Files:**
- Create: `src/tree_diff/band.rs`
- Modify: `src/tree_diff/mod.rs` (add `mod band;`)

**Interfaces:**
- Produces: `pub(crate) const INF: u32 = u32::MAX / 2;`
- `pub(crate) struct BandMatrix` with:
  - `new(rows: usize, cols: usize, half_width: usize) -> Self` — all cells start at `INF`; storage is `rows × (2·h+1)` where `h = half_width.min(rows.max(cols))` (clamped so huge τ on small trees doesn't over-allocate)
  - `get(&self, r: usize, c: usize) -> u32` — `INF` if `(r, c)` is outside the band (`|c − r| > h`) or out of range
  - `set(&mut self, r: usize, c: usize, v: u32)` — `debug_assert!` in-band and in-range
  - `row_cols(&self, r: usize) -> RangeInclusive<usize>` — the in-band, in-range column indices for row `r` (empty-ish range when none)
  - `half_width(&self) -> usize`

The band is always centered on the main diagonal (`diag_offset = 0`): the TD matrix bands on global postorder difference `|post(x) − post(y)| ≤ τ`, and FD matrices band on **local** index difference `≤ ε` (see cheat sheet — paper Fig. 4 confirms the FD band is local-diagonal).

- [ ] **Step 1: Write failing tests in `src/tree_diff/band.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_start_at_inf() {
        let m = BandMatrix::new(4, 4, 1);
        assert_eq!(m.get(0, 0), INF);
        assert_eq!(m.get(3, 3), INF);
    }

    #[test]
    fn set_get_roundtrip_within_band() {
        let mut m = BandMatrix::new(4, 4, 1);
        m.set(0, 0, 7);
        m.set(1, 2, 9);
        m.set(2, 1, 3);
        assert_eq!(m.get(0, 0), 7);
        assert_eq!(m.get(1, 2), 9);
        assert_eq!(m.get(2, 1), 3);
    }

    #[test]
    fn out_of_band_reads_are_inf() {
        let mut m = BandMatrix::new(5, 5, 1);
        m.set(2, 2, 1);
        assert_eq!(m.get(0, 4), INF);
        assert_eq!(m.get(4, 0), INF);
        // Out of range entirely.
        assert_eq!(m.get(9, 0), INF);
        assert_eq!(m.get(0, 9), INF);
    }

    #[test]
    fn row_cols_clips_to_band_and_range() {
        let m = BandMatrix::new(5, 5, 1);
        assert_eq!(m.row_cols(0), 0..=1);
        assert_eq!(m.row_cols(2), 1..=3);
        assert_eq!(m.row_cols(4), 3..=4);
    }

    #[test]
    fn row_cols_can_be_empty() {
        // 1 row, many cols, tiny band: row 0 still sees cols 0..=h.
        let m = BandMatrix::new(1, 10, 2);
        assert_eq!(m.row_cols(0), 0..=2);
        // Row beyond all cols yields an empty range.
        let m = BandMatrix::new(10, 1, 2);
        assert!(m.row_cols(9).is_empty());
    }

    #[test]
    fn zero_width_band_is_the_diagonal() {
        let mut m = BandMatrix::new(3, 3, 0);
        m.set(1, 1, 5);
        assert_eq!(m.get(1, 1), 5);
        assert_eq!(m.get(1, 0), INF);
        assert_eq!(m.get(1, 2), INF);
        assert_eq!(m.row_cols(1), 1..=1);
    }

    #[test]
    fn half_width_clamps_to_dimensions() {
        // Storage must not scale with an absurd tau on a tiny matrix.
        let m = BandMatrix::new(3, 3, 1_000_000);
        assert_eq!(m.half_width(), 3);
        assert_eq!(m.row_cols(0), 0..=2);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo check --tests` — FAIL: `BandMatrix` undefined.

- [ ] **Step 3: Implement**

```rust
//! A dense band matrix for bounded tree edit distance computations.
//!
//! Both matrices in the paper are diagonal bands: the subtree-distance matrix
//! `TD` is banded by the postorder difference `|post(x) − post(y)| ≤ τ`
//! (Alg. 2, line 2), and each forest-distance matrix `FD` is banded by the
//! *local* index difference `≤ ε(x, y, τ)` (edits pruning, Fig. 4). Cells
//! outside the band are never stored and read back as [`INF`], which the DP
//! treats as "unreachable within the budget".

/// Effectively-infinite distance; large enough that `INF + 1` cannot wrap.
pub(crate) const INF: u32 = u32::MAX / 2;

/// A `rows × cols` matrix that only stores cells with `|col − row| ≤ h`.
#[derive(Debug, Clone)]
pub(crate) struct BandMatrix {
    rows: usize,
    cols: usize,
    half_width: usize,
    data: Vec<u32>,
}

impl BandMatrix {
    /// Create a band matrix with every stored cell initialized to [`INF`].
    ///
    /// `half_width` is clamped to `rows.max(cols)`: a wider band cannot hold
    /// any additional valid cells, and clamping keeps memory proportional to
    /// the matrix instead of the requested bound.
    pub(crate) fn new(rows: usize, cols: usize, half_width: usize) -> Self {
        let half_width = half_width.min(rows.max(cols));
        let width = 2 * half_width + 1;
        BandMatrix {
            rows,
            cols,
            half_width,
            data: vec![INF; rows * width],
        }
    }

    pub(crate) fn half_width(&self) -> usize {
        self.half_width
    }

    /// Storage slot for `(r, c)`, or `None` when outside the band or matrix.
    fn index(&self, r: usize, c: usize) -> Option<usize> {
        if r >= self.rows || c >= self.cols {
            return None;
        }
        let k = c as i64 - r as i64;
        if k.abs() > self.half_width as i64 {
            return None;
        }
        let width = 2 * self.half_width + 1;
        Some(r * width + (k + self.half_width as i64) as usize)
    }

    /// Read a cell; out-of-band or out-of-range cells are [`INF`].
    pub(crate) fn get(&self, r: usize, c: usize) -> u32 {
        self.index(r, c).map_or(INF, |i| self.data[i])
    }

    /// Write a cell. Callers must stay inside the band (loop bounds come from
    /// [`Self::row_cols`]), which `debug_assert` verifies.
    pub(crate) fn set(&mut self, r: usize, c: usize, v: u32) {
        let idx = self.index(r, c);
        debug_assert!(idx.is_some(), "BandMatrix::set out of band: ({r}, {c})");
        if let Some(i) = idx {
            self.data[i] = v;
        }
    }

    /// The in-band, in-range column indices of row `r` (may be empty).
    pub(crate) fn row_cols(&self, r: usize) -> std::ops::RangeInclusive<usize> {
        let lo = r.saturating_sub(self.half_width);
        let hi = (r + self.half_width).min(self.cols.saturating_sub(1));
        lo..=hi
    }
}
```

- [ ] **Step 4: Add `mod band;` to `src/tree_diff/mod.rs`, run tests**

Run: `cargo nextest run tree_diff::band`
Expected: PASS (7 tests).

- [ ] **Step 5: Lint, format, commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all`

```sh
jj st
jj desc -m "feat: add banded matrix for bounded tree edit distance

Both TED matrices are diagonal bands (paper Sect. 4): TD bands on
global postorder difference <= tau, FD bands on local index
difference <= the edits budget. Out-of-band cells read as INF.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 4: Forest-distance DP core + TouzetDepth

**Files:**
- Create: `src/tree_diff/ted.rs`, `src/tree_diff/touzet.rs`
- Modify: `src/tree_diff/mod.rs` (add `mod ted; mod touzet;` and `pub use touzet::touzet_depth;`)

**Interfaces:**
- Consumes: `LabeledTree` (Task 1), `BandMatrix`/`INF` (Task 3)
- Produces (in `ted.rs`):
  - `pub(crate) fn neighborhood_distance(old: &LabeledTree, new: &LabeledTree, x: usize, y: usize) -> u64`
  - `pub(crate) fn edits_budget(old: &LabeledTree, new: &LabeledTree, x: usize, y: usize, tau: u32) -> Option<u32>` — `None` when the neighborhood filter rejects the pair
  - `pub(crate) struct ForestDistance { pub fd: BandMatrix, pub anchored: Vec<(usize, usize, u32)>, pub rows: usize, pub cols: usize }` with `pub(crate) fn root_distance(&self) -> u32` (= `fd.get(rows, cols)`)
  - `pub(crate) fn forest_distance(old: &LabeledTree, new: &LabeledTree, x: usize, y: usize, budget: u32, depth_pruning: bool, td: &BandMatrix) -> ForestDistance`
- Produces (in `touzet.rs`):
  - `pub fn touzet_depth(old: &LabeledTree, new: &LabeledTree, tau: u32) -> u32` — public τ-bounded distance (returns a value > τ, possibly `INF`, when the true distance exceeds τ); thin facade over
  - `pub(crate) fn touzet_depth_impl(old, new, tau) -> (u32, BandMatrix)` — also returns the TD matrix for later mapping recovery
  - Both `debug_assert!` non-empty inputs (empty trees are handled by the caller in Task 7/8).

**Algorithm notes for the implementer (derived from Alg. 1/2 and Sect. 5, verified against the paper's figures):**
- FD for pair `(x, y)`: local indices `li ∈ 0..=m`, `lj ∈ 0..=n` where `m = size(x)`, `n = size(y)`; index 0 is the empty prefix. Global node for `li` is `gi = lld(x) + li − 1`.
- Recurrence: `del = fd[li−1][lj] + 1`, `ins = fd[li][lj−1] + 1`. If both prefixes are **anchored** (`lld(gi) == lld(x)` and `lld(gj) == lld(y)`): `fd[li][lj] = min(del, ins, fd[li−1][lj−1] + rename_cost)` and the value is a subtree distance (record it). Otherwise `fd[li][lj] = min(del, ins, fd[lld_i−1][lld_j−1] + TD[gi][gj])` where `lld_i` is the local index of `lld(gi)`.
- All additions use `saturating_add` so `INF` never wraps.
- Depth pruning (TouzetDepth only): rows with `depth(gi) − depth(x) − 1 ≥ budget` are forced deletions (`fd[li][lj] = fd[li−1][lj] + 1` across the band) — the node cannot be mapped inside this subproblem, and old-tree nodes are either mapped or deleted.
- TouzetDepth outer loop: `x` ascending; `y` in the global band `[x−τ, x+τ]` ascending; skip pairs failing the neighborhood filter; each computed pair stores only its root distance into TD. Ascending order guarantees every TD value an FD computation reads (always for strictly-smaller subtree pairs, or same `x` with smaller `y`) was already stored or is legitimately `INF` (pruned = irrelevant, per Thm. 5.3's argument).

- [ ] **Step 1: Write failing tests**

In `src/tree_diff/touzet.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::LabeledTree;

    /// Paper Fig. 1 trees. Labels: l=0 s=1 q=2 u=3 t=4 p=5 v=6 o=7 k=8.
    fn paper_trees() -> (LabeledTree<'static>, LabeledTree<'static>) {
        let t = LabeledTree::from_postorder(
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[Some(7), Some(2), Some(5), Some(4), Some(5), Some(7), Some(7), None],
        )
        .unwrap();
        let t_prime = LabeledTree::from_postorder(
            &[1, 2, 3, 4, 5, 8, 6, 7],
            &[Some(1), Some(4), Some(3), Some(4), Some(7), Some(6), Some(7), None],
        )
        .unwrap();
        (t, t_prime)
    }

    #[test]
    fn paper_example_distance_is_two() {
        let (t, t_prime) = paper_trees();
        // tau = 2 is exactly the true distance; the bound is tight and valid.
        assert_eq!(touzet_depth(&t, &t_prime, 2), 2);
        // Looser bounds must give the same exact answer.
        assert_eq!(touzet_depth(&t, &t_prime, 4), 2);
        assert_eq!(touzet_depth(&t, &t_prime, 16), 2);
    }

    #[test]
    fn identical_trees_have_distance_zero() {
        let (t, _) = paper_trees();
        assert_eq!(touzet_depth(&t, &t, 1), 0);
        assert_eq!(touzet_depth(&t, &t, 8), 0);
    }

    #[test]
    fn insufficient_tau_reports_a_value_above_tau() {
        let (t, t_prime) = paper_trees();
        // tau = 1 < true distance 2: per Lemma 7.1 the result must exceed tau
        // (it need not be the true distance).
        assert!(touzet_depth(&t, &t_prime, 1) > 1);
    }

    #[test]
    fn single_nodes() {
        let a = LabeledTree::from_postorder(&[0], &[None]).unwrap();
        let b = LabeledTree::from_postorder(&[0], &[None]).unwrap();
        let c = LabeledTree::from_postorder(&[9], &[None]).unwrap();
        assert_eq!(touzet_depth(&a, &b, 1), 0);
        assert_eq!(touzet_depth(&a, &c, 1), 1);
    }

    #[test]
    fn completely_relabeled_tree() {
        // Same shape, all four labels changed: distance 4 (four renames).
        let a = LabeledTree::from_postorder(
            &[0, 1, 2, 3],
            &[Some(3), Some(2), Some(3), None],
        )
        .unwrap();
        let b = LabeledTree::from_postorder(
            &[4, 5, 6, 7],
            &[Some(3), Some(2), Some(3), None],
        )
        .unwrap();
        assert_eq!(touzet_depth(&a, &b, 8), 4);
    }
}
```

In `src/tree_diff/ted.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::LabeledTree;

    fn paper_trees() -> (LabeledTree<'static>, LabeledTree<'static>) {
        let t = LabeledTree::from_postorder(
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[Some(7), Some(2), Some(5), Some(4), Some(5), Some(7), Some(7), None],
        )
        .unwrap();
        let t_prime = LabeledTree::from_postorder(
            &[1, 2, 3, 4, 5, 8, 6, 7],
            &[Some(1), Some(4), Some(3), Some(4), Some(7), Some(6), Some(7), None],
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
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo check --tests` — FAIL: functions undefined.

- [ ] **Step 3: Implement `src/tree_diff/ted.rs`**

```rust
//! The shared forest-distance dynamic programming core (Zhang–Shasha style),
//! banded and budgeted per Touzet / Pawlik & Augsten.

use super::band::{BandMatrix, INF};
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
/// never computed read as [`INF`], which is correct because such pairs are
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
        // Depth-based pruning: a node deeper than the budget allows cannot be
        // mapped within this subproblem, so it must be deleted.
        let forced_delete = depth_pruning
            && (old.depth(gi) as i64 - old.depth(x) as i64 - 1) >= i64::from(budget);
        let lld_i = old.lld(gi) - lx + 1;
        for lj in fd.row_cols(li) {
            let del = fd.get(li - 1, lj).saturating_add(1);
            if lj == 0 || forced_delete {
                fd.set(li, lj, del);
                continue;
            }
            let gj = ly + lj - 1;
            let ins = fd.get(li, lj - 1).saturating_add(1);
            let lld_j = new.lld(gj) - ly + 1;
            let best = if lld_i == 1 && lld_j == 1 {
                // Both prefixes are whole subtrees sharing the pair's
                // leftmost leaves: this cell is itself a subtree distance.
                let rename = u32::from(old.label(gi) != new.label(gj));
                let dist = del
                    .min(ins)
                    .min(fd.get(li - 1, lj - 1).saturating_add(rename));
                anchored.push((gi, gj, dist));
                dist
            } else {
                let sub = fd
                    .get(lld_i - 1, lld_j - 1)
                    .saturating_add(td.get(gi, gj));
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
```

- [ ] **Step 4: Implement `src/tree_diff/touzet.rs`**

```rust
//! TouzetDepth (paper Alg. 2): the tau-bounded baseline with subtree, edits,
//! and depth-based pruning. Quadratic worst case but effective on deep trees,
//! where TopDiff's top-node-pairs approach degrades (Sect. 6).

use super::band::BandMatrix;
use super::ted::{edits_budget, forest_distance};
use super::tree::LabeledTree;

/// Compute the tau-bounded tree edit distance with depth-based pruning.
///
/// If the true distance is <= `tau`, the exact distance is returned
/// (Thm. 7.2). Otherwise the returned value merely exceeds `tau` (it may be
/// an overestimate, up to an internal "infinity"), which is exactly the
/// signal the AutoStop driver needs to double the bound.
#[must_use]
pub fn touzet_depth(old: &LabeledTree, new: &LabeledTree, tau: u32) -> u32 {
    touzet_depth_impl(old, new, tau).0
}

/// As [`touzet_depth`], but also returns the subtree-distance matrix for
/// mapping recovery.
pub(crate) fn touzet_depth_impl(
    old: &LabeledTree,
    new: &LabeledTree,
    tau: u32,
) -> (u32, BandMatrix) {
    debug_assert!(!old.is_empty() && !new.is_empty());
    let (n_old, n_new) = (old.len(), new.len());
    let mut td = BandMatrix::new(n_old, n_new, tau as usize);
    for x in 0..n_old {
        for y in td.row_cols(x) {
            // Subtree pruning: only pairs passing the neighborhood filter.
            let Some(budget) = edits_budget(old, new, x, y, tau) else {
                continue;
            };
            let result = forest_distance(old, new, x, y, budget, true, &td);
            let dist = result.root_distance();
            td.set(x, y, dist);
        }
    }
    (td.get(n_old - 1, n_new - 1), td)
}
```

Wire up `src/tree_diff/mod.rs`:

```rust
mod band;
mod ted;
mod touzet;
mod tree;

pub use touzet::touzet_depth;
pub use tree::{LabeledTree, build_labeled_trees};
```

- [ ] **Step 5: Run tests**

Run: `cargo nextest run tree_diff`
Expected: PASS. If `paper_example_distance_is_two` fails, debug against the hand-verified values in the cheat sheet (neighborhood vectors and budgets are all listed) before touching the recurrence.

- [ ] **Step 6: Lint, format, commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all`

```sh
jj st
jj desc -m "feat: add TouzetDepth bounded tree edit distance

Banded Zhang-Shasha forest-distance core with subtree pruning
(neighborhood filter), edits pruning (local band), and depth-based
pruning, per Touzet as revisited in Pawlik & Augsten Alg. 2. Verified
against the paper's worked example.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 5: Zhang–Shasha oracle + property tests

**Files:**
- Create: `tests/tree_diff_proptest.rs`

**Interfaces:**
- Consumes: `LabeledTree::from_postorder`, `label(i)`, `lld(i)`, `len()`, `touzet_depth` (all public, Tasks 1 & 4)
- Produces (inside the test file, reused by later tasks extending this file):
  - `fn zs_oracle(a: &LabeledTree, b: &LabeledTree) -> u32` — textbook unbounded Zhang–Shasha TED, full matrices, no pruning
  - `fn arb_tree(max_nodes: usize, n_labels: u32) -> impl Strategy<Value = LabeledTree<'static>>`

The oracle is deliberately naive (full `|T|×|T′|` matrices, keyroot pairs) — its only job is to be obviously correct.

- [ ] **Step 1: Write the oracle, generators, and property tests**

```rust
//! Property tests for the tree diff engine against a naive Zhang–Shasha
//! oracle. The oracle uses full matrices and no pruning — slow but obviously
//! correct — so any disagreement is a bug in the banded/pruned algorithms.

use libdiffsitter::tree_diff::{LabeledTree, touzet_depth};
use proptest::prelude::*;

const INF: u32 = u32::MAX / 2;

/// Keyroots: for each distinct leftmost-leaf descendant, the node with the
/// largest postorder ID having that lld (includes the root).
fn keyroots(t: &LabeledTree) -> Vec<usize> {
    let mut best_by_lld = std::collections::HashMap::new();
    for i in 0..t.len() {
        best_by_lld
            .entry(t.lld(i))
            .and_modify(|b: &mut usize| *b = (*b).max(i))
            .or_insert(i);
    }
    let mut roots: Vec<usize> = best_by_lld.into_values().collect();
    roots.sort_unstable();
    roots
}

/// Textbook Zhang–Shasha tree edit distance (unit costs, rename-free when
/// labels match). Reference: Zhang & Shasha 1989, as recapped in the paper's
/// Alg. 1.
fn zs_oracle(a: &LabeledTree, b: &LabeledTree) -> u32 {
    match (a.len(), b.len()) {
        (0, nb) => return nb as u32,
        (na, 0) => return na as u32,
        _ => {}
    }
    let mut td = vec![vec![INF; b.len()]; a.len()];
    for &kx in &keyroots(a) {
        for &ky in &keyroots(b) {
            let (lx, ly) = (a.lld(kx), b.lld(ky));
            let (m, n) = (kx - lx + 1, ky - ly + 1);
            let mut fd = vec![vec![0u32; n + 1]; m + 1];
            for i in 1..=m {
                fd[i][0] = fd[i - 1][0] + 1;
            }
            for j in 1..=n {
                fd[0][j] = fd[0][j - 1] + 1;
            }
            for i in 1..=m {
                let gi = lx + i - 1;
                for j in 1..=n {
                    let gj = ly + j - 1;
                    let del = fd[i - 1][j] + 1;
                    let ins = fd[i][j - 1] + 1;
                    if a.lld(gi) == lx && b.lld(gj) == ly {
                        let rename = u32::from(a.label(gi) != b.label(gj));
                        fd[i][j] = del.min(ins).min(fd[i - 1][j - 1] + rename);
                        td[gi][gj] = fd[i][j];
                    } else {
                        let (li, lj) = (a.lld(gi) - lx, b.lld(gj) - ly);
                        fd[i][j] = del
                            .min(ins)
                            .min(fd[li][lj].saturating_add(td[gi][gj]));
                    }
                }
            }
        }
    }
    td[a.len() - 1][b.len() - 1]
}

/// Convert a "random attachment" tree (node 0 is the root; `parent_of[k]` is
/// the parent of node `k + 1`, always a lower ID) into a postorder
/// `LabeledTree`. Children keep ascending-ID sibling order.
fn build_postorder_tree(labels: &[u32], parent_of: &[usize]) -> LabeledTree<'static> {
    let n = labels.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, &p) in parent_of.iter().enumerate() {
        children[p].push(k + 1);
    }
    fn dfs(id: usize, children: &[Vec<usize>], order: &mut Vec<usize>) {
        for &c in &children[id] {
            dfs(c, children, order);
        }
        order.push(id);
    }
    let mut order = Vec::with_capacity(n);
    dfs(0, &children, &mut order);
    let mut post_of_id = vec![0usize; n];
    for (post, &id) in order.iter().enumerate() {
        post_of_id[id] = post;
    }
    let mut post_labels = vec![0u32; n];
    let mut post_parents: Vec<Option<usize>> = vec![None; n];
    for id in 0..n {
        post_labels[post_of_id[id]] = labels[id];
        post_parents[post_of_id[id]] = if id == 0 {
            None
        } else {
            Some(post_of_id[parent_of[id - 1]])
        };
    }
    LabeledTree::from_postorder(&post_labels, &post_parents)
        .expect("generated parent links are a valid tree by construction")
}

/// Random trees with 1..=max_nodes nodes and labels drawn from a small
/// alphabet (small alphabets maximize interesting rename/match interactions).
fn arb_tree(max_nodes: usize, n_labels: u32) -> impl Strategy<Value = LabeledTree<'static>> {
    (1..=max_nodes).prop_flat_map(move |n| {
        let parents: Vec<std::ops::Range<usize>> = (1..n).map(|i| 0..i).collect();
        (proptest::collection::vec(0..n_labels, n), parents)
            .prop_map(|(labels, parent_of)| build_postorder_tree(&labels, &parent_of))
    })
}

#[test]
fn oracle_sanity_paper_example() {
    let t = LabeledTree::from_postorder(
        &[0, 1, 2, 3, 4, 5, 6, 7],
        &[Some(7), Some(2), Some(5), Some(4), Some(5), Some(7), Some(7), None],
    )
    .unwrap();
    let t_prime = LabeledTree::from_postorder(
        &[1, 2, 3, 4, 5, 8, 6, 7],
        &[Some(1), Some(4), Some(3), Some(4), Some(7), Some(6), Some(7), None],
    )
    .unwrap();
    assert_eq!(zs_oracle(&t, &t_prime), 2);
    assert_eq!(zs_oracle(&t, &t), 0);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn touzet_matches_oracle_with_loose_bound(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
    ) {
        let expected = zs_oracle(&a, &b);
        let tau = (a.len() + b.len()) as u32; // always a valid upper bound
        prop_assert_eq!(touzet_depth(&a, &b, tau), expected);
    }

    #[test]
    fn touzet_matches_oracle_with_tight_bound(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
    ) {
        // The tightest valid bound exercises the pruning paths hardest.
        let expected = zs_oracle(&a, &b);
        let tau = expected.max(1);
        prop_assert_eq!(touzet_depth(&a, &b, tau), expected);
    }

    #[test]
    fn touzet_is_symmetric(a in arb_tree(15, 3), b in arb_tree(15, 3)) {
        let tau = (a.len() + b.len()) as u32;
        prop_assert_eq!(touzet_depth(&a, &b, tau), touzet_depth(&b, &a, tau));
    }

    #[test]
    fn identity_distance_is_zero(a in arb_tree(20, 4)) {
        prop_assert_eq!(touzet_depth(&a, &a, 1), 0);
    }
}
```

- [ ] **Step 2: Run the property tests**

Run: `cargo nextest run --test tree_diff_proptest`
Expected: PASS (5 tests; the proptest group in `.config/nextest.toml` picks this binary up automatically via the `binary(~proptest)` filter). If `touzet_matches_oracle_with_tight_bound` fails, minimize with proptest's shrinker output and compare FD matrices by hand against the oracle's — the likely culprits are the anchored-cell condition or the depth-pruning row (a too-aggressive prune shows up only with tight budgets).

- [ ] **Step 3: Lint, format, commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all`

```sh
jj st
jj desc -m "test: add Zhang-Shasha oracle proptests for tree edit distance

Random small trees checked against a naive full-matrix Zhang-Shasha
implementation, with both loose and tight tau bounds, plus symmetry
and identity properties.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 6: Top node pairs + TopDiff

**Files:**
- Create: `src/tree_diff/topdiff.rs`
- Modify: `src/tree_diff/mod.rs` (add `mod topdiff;` and `pub use topdiff::topdiff;`), `tests/tree_diff_proptest.rs` (extend)

**Interfaces:**
- Consumes: `neighborhood_distance`, `edits_budget`, `forest_distance`, `BandMatrix`, `LabeledTree`
- Produces:
  - `pub(crate) fn compute_top_node_pairs(old: &LabeledTree, new: &LabeledTree, tau: u32) -> Vec<(usize, usize)>` — paper Alg. 3, returned in construction order (x-major decreasing)
  - `pub fn topdiff(old: &LabeledTree, new: &LabeledTree, tau: u32) -> u32` — paper Alg. 4 facade
  - `pub(crate) fn topdiff_impl(old, new, tau) -> (u32, BandMatrix)`

**Algorithm notes:**
- Alg. 3 iterates `x` in **decreasing** postorder; for each `x`, `y` in decreasing postorder over the global band `[x−τ, x+τ]`, keeping only pairs with neighborhood distance ≤ τ. First pair seen for a `(lld(x), lld(y))` key is appended to `TN`; subsequent hits update only the stored `y` if the new `y` has a *larger* postorder (paper lines 6–7 — a larger `y` can appear later because the neighborhood filter is not monotonic).
- Alg. 4 processes `TN` in **ascending** postorder — sort the pairs lexicographically before the DP loop — and stores **all anchored-cell distances** into TD (the by-products `(i,j) <l (x,y)`; storing the pair itself matches Alg. 1/2's `TD[x,y] ← FD[x,y]` line, which Alg. 4 elides as shorthand). Anchored cells are always inside TD's τ-band: their global postorder difference is ≤ ε + |Δl| ≤ τ.
- No depth pruning in TopDiff (that is TouzetDepth's job).

- [ ] **Step 1: Write failing tests in `src/tree_diff/topdiff.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::LabeledTree;

    fn paper_trees() -> (LabeledTree<'static>, LabeledTree<'static>) {
        let t = LabeledTree::from_postorder(
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[Some(7), Some(2), Some(5), Some(4), Some(5), Some(7), Some(7), None],
        )
        .unwrap();
        let t_prime = LabeledTree::from_postorder(
            &[1, 2, 3, 4, 5, 8, 6, 7],
            &[Some(1), Some(4), Some(3), Some(4), Some(7), Some(6), Some(7), None],
        )
        .unwrap();
        (t, t_prime)
    }

    #[test]
    fn top_node_pairs_match_paper_example_5_2() {
        let (t, t_prime) = paper_trees();
        let mut tn = compute_top_node_pairs(&t, &t_prime, 2);
        tn.sort_unstable();
        assert_eq!(tn, vec![(2, 3), (4, 3), (5, 4), (6, 6), (7, 7)]);
    }

    #[test]
    fn paper_example_distance_is_two() {
        let (t, t_prime) = paper_trees();
        assert_eq!(topdiff(&t, &t_prime, 2), 2);
        assert_eq!(topdiff(&t, &t_prime, 8), 2);
    }

    #[test]
    fn identical_trees_have_distance_zero() {
        let (t, _) = paper_trees();
        assert_eq!(topdiff(&t, &t, 1), 0);
    }

    #[test]
    fn insufficient_tau_reports_a_value_above_tau() {
        let (t, t_prime) = paper_trees();
        assert!(topdiff(&t, &t_prime, 1) > 1);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo check --tests` — FAIL: `topdiff` undefined.

- [ ] **Step 3: Implement `src/tree_diff/topdiff.rs`**

```rust
//! TopDiff (paper Alg. 3 + 4): computes forest distances only for "top node
//! pairs", guaranteeing that no subproblem is solved more than once — the
//! paper's core contribution over Touzet's algorithm.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::band::BandMatrix;
use super::ted::{edits_budget, forest_distance, neighborhood_distance};
use super::tree::LabeledTree;

/// Compute the top node pairs for the given bound (paper Alg. 3, linear
/// time). Returned in construction order (x-major decreasing postorder);
/// callers that need ascending processing order must sort.
pub(crate) fn compute_top_node_pairs(
    old: &LabeledTree,
    new: &LabeledTree,
    tau: u32,
) -> Vec<(usize, usize)> {
    let mut tn: Vec<(usize, usize)> = Vec::new();
    // Key: (lld(x), lld(y)) -> index of the pair in `tn`.
    let mut seen: HashMap<(usize, usize), usize> = HashMap::new();
    for x in (0..old.len()).rev() {
        let lo = x.saturating_sub(tau as usize);
        let hi = (x + tau as usize).min(new.len() - 1);
        for y in (lo..=hi).rev() {
            if neighborhood_distance(old, new, x, y) > u64::from(tau) {
                continue;
            }
            match seen.entry((old.lld(x), new.lld(y))) {
                Entry::Occupied(entry) => {
                    // Keep the first (largest) x; adopt a later y only if it
                    // sits higher in the tree (paper Alg. 3, lines 6-7).
                    let idx = *entry.get();
                    if y > tn[idx].1 {
                        tn[idx].1 = y;
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(tn.len());
                    tn.push((x, y));
                }
            }
        }
    }
    tn
}

/// Compute the tau-bounded tree edit distance using top node pairs
/// (paper Alg. 4). Exact when the true distance is <= `tau` (Thm. 7.2).
#[must_use]
pub fn topdiff(old: &LabeledTree, new: &LabeledTree, tau: u32) -> u32 {
    topdiff_impl(old, new, tau).0
}

/// As [`topdiff`], but also returns the subtree-distance matrix for mapping
/// recovery.
pub(crate) fn topdiff_impl(
    old: &LabeledTree,
    new: &LabeledTree,
    tau: u32,
) -> (u32, BandMatrix) {
    debug_assert!(!old.is_empty() && !new.is_empty());
    let mut td = BandMatrix::new(old.len(), new.len(), tau as usize);
    let mut tn = compute_top_node_pairs(old, new, tau);
    // Ascending postorder guarantees the TD values a pair reads were already
    // produced by smaller pairs (correctness argument of Thm. 5.3).
    tn.sort_unstable();
    for &(x, y) in &tn {
        // Top node pairs pass the neighborhood filter by construction.
        let Some(budget) = edits_budget(old, new, x, y, tau) else {
            debug_assert!(false, "top node pair ({x}, {y}) failed the neighborhood filter");
            continue;
        };
        let result = forest_distance(old, new, x, y, budget, false, &td);
        for &(gi, gj, dist) in &result.anchored {
            td.set(gi, gj, dist);
        }
    }
    (td.get(old.len() - 1, new.len() - 1), td)
}
```

Wire `mod topdiff;` and `pub use topdiff::topdiff;` into `src/tree_diff/mod.rs`.

- [ ] **Step 4: Run unit tests**

Run: `cargo nextest run tree_diff`
Expected: PASS. The `top_node_pairs_match_paper_example_5_2` expectation was hand-traced through Alg. 3 during planning (see cheat sheet) — if it fails, print the computed neighborhood distances for the offending pairs first.

- [ ] **Step 5: Extend the proptests**

Append to `tests/tree_diff_proptest.rs` (inside the existing `proptest!` block):

```rust
    #[test]
    fn topdiff_matches_oracle_with_loose_bound(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
    ) {
        let expected = zs_oracle(&a, &b);
        let tau = (a.len() + b.len()) as u32;
        prop_assert_eq!(libdiffsitter::tree_diff::topdiff(&a, &b, tau), expected);
    }

    #[test]
    fn topdiff_matches_oracle_with_tight_bound(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
    ) {
        let expected = zs_oracle(&a, &b);
        let tau = expected.max(1);
        prop_assert_eq!(libdiffsitter::tree_diff::topdiff(&a, &b, tau), expected);
    }

    #[test]
    fn topdiff_and_touzet_agree(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
        tau in 1u32..40,
    ) {
        // For ANY tau (even invalid bounds) both algorithms must agree on
        // whether the result is within tau, per Lemma 7.1: dtau <= tau iff
        // the true distance is <= tau.
        let t1 = touzet_depth(&a, &b, tau);
        let t2 = libdiffsitter::tree_diff::topdiff(&a, &b, tau);
        prop_assert_eq!(t1 <= tau, t2 <= tau);
        if t1 <= tau {
            prop_assert_eq!(t1, t2);
        }
    }
```

- [ ] **Step 6: Run all proptests**

Run: `cargo nextest run --test tree_diff_proptest`
Expected: PASS (8 tests).

- [ ] **Step 7: Lint, format, commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all`

```sh
jj st
jj desc -m "feat: add TopDiff top node pairs algorithm

Implements the paper's core contribution (Alg. 3 + 4): forest
distances are computed only for top node pairs, storing anchored-cell
by-products, which avoids all redundant subproblem computations.
Verified against the paper's Example 5.2 and the Zhang-Shasha oracle.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 7: Cost model, TopDiff+, and AutoStop

**Files:**
- Create: `src/tree_diff/cost.rs`, `src/tree_diff/autostop.rs`
- Modify: `src/tree_diff/mod.rs` (add modules, `TreeDiffOptions`, `pub use autostop::tree_edit_distance;`), `tests/tree_diff_proptest.rs` (extend)

**Interfaces:**
- Consumes: `touzet_depth_impl`, `topdiff_impl`, `BandMatrix`, `LabeledTree`, `TreeDiffError::BoundExceeded`
- Produces:
  - `pub struct TreeDiffOptions { pub max_tau: u32 }` in `mod.rs` — serde `kebab-case` + `default`, `Default = { max_tau: 2048 }`
  - `pub(crate) fn cost_topdiff(t: &LabeledTree) -> u64` (Sect. 6: `Σ_{leaves l} |T^l|`, the largest subtree per lld)
  - `pub(crate) fn cost_touzet_depth(t: &LabeledTree, tau: u32) -> u64` (Sect. 6: `Σ_x min{τ, depth(x)}`)
  - `pub(crate) struct BoundedResult { pub distance: u32, pub tau: u32, pub td: BandMatrix }`
  - `pub(crate) fn autostop(old: &LabeledTree, new: &LabeledTree, options: &TreeDiffOptions) -> Result<BoundedResult, TreeDiffError>` (Alg. 5 + 6 combined: per round, pick the cheaper algorithm by cost estimate, double τ until `δτ ≤ τ`)
  - `pub fn tree_edit_distance(old: &LabeledTree, new: &LabeledTree, options: &TreeDiffOptions) -> Result<u32, TreeDiffError>` — public facade, handles empty trees (`(0,0) → 0`, one-sided → other tree's size)

- [ ] **Step 1: Write failing tests**

In `src/tree_diff/cost.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::LabeledTree;

    fn paper_tree_t() -> LabeledTree<'static> {
        LabeledTree::from_postorder(
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[Some(7), Some(2), Some(5), Some(4), Some(5), Some(7), Some(7), None],
        )
        .unwrap()
    }

    #[test]
    fn touzet_cost_sums_clamped_depths() {
        let t = paper_tree_t();
        // Depths: x0=1 x1=3 x2=2 x3=3 x4=2 x5=1 x6=1 x7=0.
        assert_eq!(cost_touzet_depth(&t, 100), 13); // sum of depths
        assert_eq!(cost_touzet_depth(&t, 2), 1 + 2 + 2 + 2 + 2 + 1 + 1); // clamped at 2
        assert_eq!(cost_touzet_depth(&t, 0), 0);
    }

    #[test]
    fn topdiff_cost_sums_largest_subtree_per_lld() {
        let t = paper_tree_t();
        // Leaves: x0, x1, x3, x6. Largest subtree with lld=0 is the root
        // (size 8); lld=1 is x5 (size 5); lld=3 is x4 (size 2); lld=6 is x6
        // (size 1). Total = 16.
        assert_eq!(cost_topdiff(&t), 16);
    }
}
```

In `src/tree_diff/autostop.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::{LabeledTree, TreeDiffOptions};

    fn paper_trees() -> (LabeledTree<'static>, LabeledTree<'static>) {
        let t = LabeledTree::from_postorder(
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[Some(7), Some(2), Some(5), Some(4), Some(5), Some(7), Some(7), None],
        )
        .unwrap();
        let t_prime = LabeledTree::from_postorder(
            &[1, 2, 3, 4, 5, 8, 6, 7],
            &[Some(1), Some(4), Some(3), Some(4), Some(7), Some(6), Some(7), None],
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
```

In `src/tree_diff/mod.rs` tests (create a `#[cfg(test)] mod tests` at the bottom):

```rust
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
```

- [ ] **Step 2: Run to verify failure** — `cargo check --tests` FAILs.

- [ ] **Step 3: Implement `src/tree_diff/cost.rs`**

```rust
//! Structure-aware cost estimates for choosing between TopDiff and
//! TouzetDepth (paper Sect. 6). Both estimate the number of subproblems the
//! respective algorithm would compute, in O(n) over the left-hand tree.

use super::tree::LabeledTree;

/// Estimated cost of TouzetDepth: depth pruning limits each node's
/// contribution to `min(tau, depth(x))`.
pub(crate) fn cost_touzet_depth(t: &LabeledTree, tau: u32) -> u64 {
    (0..t.len())
        .map(|i| (t.depth(i) as u64).min(u64::from(tau)))
        .sum()
}

/// Estimated cost of TopDiff: at most one top node pair exists per leaf
/// pair, so we sum, for every leaf `l`, the size of the largest subtree
/// whose leftmost leaf descendant is `l`.
pub(crate) fn cost_topdiff(t: &LabeledTree) -> u64 {
    let mut largest_by_lld: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    for i in 0..t.len() {
        largest_by_lld
            .entry(t.lld(i))
            .and_modify(|s| *s = (*s).max(t.size(i)))
            .or_insert(t.size(i));
    }
    largest_by_lld.values().map(|&s| s as u64).sum()
}
```

- [ ] **Step 4: Implement `src/tree_diff/autostop.rs`**

```rust
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
    pub tau: u32,
    /// Subtree distances from the final round.
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
    loop {
        if tau > options.max_tau {
            return Err(TreeDiffError::BoundExceeded {
                tau,
                limit: options.max_tau,
            });
        }
        // TopDiff+ (Alg. 5): pick the algorithm with the smaller estimate.
        let (distance, td) = if cost_topdiff(old) < cost_touzet_depth(old, tau) {
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
```

- [ ] **Step 5: Add options and the public facade to `src/tree_diff/mod.rs`**

```rust
mod autostop;
mod band;
mod cost;
mod ted;
mod topdiff;
mod touzet;
mod tree;

pub use topdiff::topdiff;
pub use touzet::touzet_depth;
pub use tree::{LabeledTree, build_labeled_trees};

use serde::{Deserialize, Serialize};

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
    match (old.is_empty(), new.is_empty()) {
        (true, true) => Ok(0),
        (true, false) => Ok(new.len() as u32),
        (false, true) => Ok(old.len() as u32),
        (false, false) => Ok(autostop::autostop(old, new, options)?.distance),
    }
}
```

- [ ] **Step 6: Run tests** — `cargo nextest run tree_diff` — PASS.

- [ ] **Step 7: Extend proptests**

Append to the `proptest!` block in `tests/tree_diff_proptest.rs` (add `tree_edit_distance, TreeDiffOptions` to the existing `libdiffsitter::tree_diff` import):

```rust
    #[test]
    fn autostop_matches_oracle(a in arb_tree(20, 4), b in arb_tree(20, 4)) {
        // The strongest end-to-end check: no bound is provided at all.
        let expected = zs_oracle(&a, &b);
        let got = tree_edit_distance(&a, &b, &TreeDiffOptions::default()).unwrap();
        prop_assert_eq!(got, expected);
    }

    #[test]
    fn autostop_distance_bounded_by_total_size(
        a in arb_tree(20, 4),
        b in arb_tree(20, 4),
    ) {
        let got = tree_edit_distance(&a, &b, &TreeDiffOptions::default()).unwrap();
        prop_assert!(got as usize <= a.len() + b.len());
    }
```

- [ ] **Step 8: Run all proptests** — `cargo nextest run --test tree_diff_proptest` — PASS (10 tests).

- [ ] **Step 9: Lint, format, commit**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all`

```sh
jj st
jj desc -m "feat: add TopDiff+ cost model and AutoStop driver

Structure-aware cost estimates choose between top node pairs and
depth-based pruning per round (Alg. 5); the tau-doubling AutoStop
loop (Alg. 6) computes exact TED with no prior bound, guarded by a
configurable max-tau limit.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 8: Edit mapping recovery

**Files:**
- Create: `src/tree_diff/mapping.rs`
- Modify: `src/tree_diff/mod.rs` (add `mod mapping;`, `pub use mapping::{EditMapping, edit_mapping};`), `tests/tree_diff_proptest.rs` (extend)

**Interfaces:**
- Consumes: `BoundedResult`/`autostop` (Task 7), `forest_distance` (Task 4), `BandMatrix`, `LabeledTree`, `TreeDiffError::MappingBacktrace`
- Produces:
  - `pub struct EditMapping { pub mapped: Vec<(usize, usize)>, pub deleted: Vec<usize>, pub inserted: Vec<usize>, pub distance: u32 }` — postorder-sorted vectors
  - `pub fn edit_mapping(old: &LabeledTree, new: &LabeledTree, options: &TreeDiffOptions) -> Result<EditMapping, TreeDiffError>`

**Correctness argument the implementer should keep in mind (the paper stops at the distance; recovery is our addition):**
- The backtrace recomputes each needed FD matrix with band half-width **τ** (not the pair's forward-pass ε). A τ-band is a superset of any forward band, and reads the same final TD, so recomputed values are ≤ the forward values while every finite value still corresponds to a realizable edit script. The root pair's recomputation is identical to the forward pass (its budget was already τ), so the walk starts from exactly `distance`.
- At each cell we take any transition whose arithmetic is consistent with the recomputed matrix; recursing into a subtree transition consumes `TD[gi][gj]` and recurses on that pair. Since the total is `distance` and any valid mapping costs ≥ `distance` (minimality), every consumed TD value on the walked path must be exact — a final cost check (`renames + deletions + insertions == distance`) enforces this at runtime and returns `MappingBacktrace` on violation instead of producing a wrong diff.
- Transition priority: try mapping/subtree (diagonal) first, then deletion, then insertion — biases ties toward keeping nodes mapped, which produces better-looking diffs and is deterministic.

- [ ] **Step 1: Write failing tests in `src/tree_diff/mapping.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::{LabeledTree, TreeDiffOptions};

    fn paper_trees() -> (LabeledTree<'static>, LabeledTree<'static>) {
        let t = LabeledTree::from_postorder(
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[Some(7), Some(2), Some(5), Some(4), Some(5), Some(7), Some(7), None],
        )
        .unwrap();
        let t_prime = LabeledTree::from_postorder(
            &[1, 2, 3, 4, 5, 8, 6, 7],
            &[Some(1), Some(4), Some(3), Some(4), Some(7), Some(6), Some(7), None],
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
        assert_eq!((m.distance, m.inserted.clone(), m.deleted.len()), (1, vec![0], 0));

        let m = edit_mapping(&one, &empty, &options).unwrap();
        assert_eq!((m.distance, m.deleted.clone(), m.inserted.len()), (1, vec![0], 0));

        let m = edit_mapping(&empty, &empty, &options).unwrap();
        assert_eq!(m.distance, 0);
    }
}
```

- [ ] **Step 2: Run to verify failure** — `cargo check --tests` FAILs.

- [ ] **Step 3: Implement `src/tree_diff/mapping.rs`**

```rust
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
                distance: new.len() as u32,
                ..Default::default()
            });
        }
        (false, true) => {
            return Ok(EditMapping {
                deleted: (0..old.len()).collect(),
                distance: old.len() as u32,
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
    if cost as u32 != result.distance {
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
            } else if fd
                .get(lld_i - 1, lld_j - 1)
                .saturating_add(td.get(gi, gj))
                == current
            {
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
```

Wire `mod mapping;` and `pub use mapping::{EditMapping, edit_mapping};` into `mod.rs`.

- [ ] **Step 4: Run tests** — `cargo nextest run tree_diff` — PASS.

- [ ] **Step 5: Extend proptests with mapping validity**

Append to `tests/tree_diff_proptest.rs` (import `edit_mapping` too). These check Def. 3.2's mapping conditions directly:

```rust
/// True iff `anc` is a proper ancestor of `node` in `t`.
fn is_ancestor(t: &LabeledTree, anc: usize, node: usize) -> bool {
    let mut current = node;
    while let Some(p) = t.parent(current) {
        if p == anc {
            return true;
        }
        current = p;
    }
    false
}
```

and inside the `proptest!` block:

```rust
    #[test]
    fn mapping_cost_equals_distance(a in arb_tree(20, 4), b in arb_tree(20, 4)) {
        let m = edit_mapping(&a, &b, &TreeDiffOptions::default()).unwrap();
        prop_assert_eq!(m.distance, zs_oracle(&a, &b));
        // Every node is accounted for exactly once.
        prop_assert_eq!(m.mapped.len() + m.deleted.len(), a.len());
        prop_assert_eq!(m.mapped.len() + m.inserted.len(), b.len());
    }

    #[test]
    fn mapping_satisfies_edit_mapping_conditions(
        a in arb_tree(15, 3),
        b in arb_tree(15, 3),
    ) {
        let m = edit_mapping(&a, &b, &TreeDiffOptions::default()).unwrap();
        for (idx, &(x, y)) in m.mapped.iter().enumerate() {
            for &(x2, y2) in &m.mapped[idx + 1..] {
                // One-to-one.
                prop_assert!(x != x2 && y != y2);
                // Ancestor condition.
                prop_assert_eq!(is_ancestor(&a, x, x2), is_ancestor(&b, y, y2));
                prop_assert_eq!(is_ancestor(&a, x2, x), is_ancestor(&b, y2, y));
                // Order condition: postorder + ancestor relation determine
                // left-of; with both ancestor checks equal, left-of reduces
                // to postorder comparison.
                if !is_ancestor(&a, x2, x) && !is_ancestor(&a, x, x2) {
                    prop_assert_eq!(x < x2, y < y2);
                }
            }
        }
    }

    #[test]
    fn mapping_is_deterministic(a in arb_tree(15, 3), b in arb_tree(15, 3)) {
        let opts = TreeDiffOptions::default();
        prop_assert_eq!(
            edit_mapping(&a, &b, &opts).unwrap(),
            edit_mapping(&a, &b, &opts).unwrap()
        );
    }
```

- [ ] **Step 6: Run proptests** — `cargo nextest run --test tree_diff_proptest` — PASS (13 tests).

- [ ] **Step 7: Lint, format, commit**

```sh
jj st
jj desc -m "feat: recover edit mappings from tree edit distance

Zhang-Shasha-style backtrace over the banded matrices, recomputing
forest distances with the full tau band during the walk. A runtime
cost check guarantees the emitted mapping realizes the exact
distance; property tests verify Def. 3.2's mapping conditions.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 9: StructuralDiff classification + `tree_diff()` entry point + fixtures

**Files:**
- Create: `src/tree_diff/output.rs`, `test_data/tree_diff/fn_rename/a.rs`, `test_data/tree_diff/fn_rename/b.rs`, `test_data/tree_diff/add_param/a.py`, `test_data/tree_diff/add_param/b.py`, `tests/tree_diff_test.rs`
- Modify: `src/tree_diff/mod.rs`

**Interfaces:**
- Consumes: `EditMapping`, `edit_mapping`, `LabeledTree`, `build_labeled_trees`, `TreeSitterProcessor`, `VectorData`
- Produces (all `Serialize` for JSON snapshots; all owned — no lifetimes — so render layers stay simple):
  - `pub struct Position { pub row: usize, pub column: usize }` (+ `From<tree_sitter::Point>`)
  - `pub struct NodeSummary { pub kind: String, pub snippet: String, pub start: Position, pub end: Position }`
  - `pub enum StructuralEditKind { Rename { old: NodeSummary, new: NodeSummary }, Delete { node: NodeSummary }, Insert { node: NodeSummary } }` (serde `tag = "type", rename_all = "snake_case"`)
  - `pub struct StructuralEdit { pub kind: StructuralEditKind, pub context: Option<NodeSummary> }`
  - `pub struct StructuralDiff { pub edits: Vec<StructuralEdit>, pub distance: u32 }`
  - `pub fn classify(old: &LabeledTree, new: &LabeledTree, mapping: &EditMapping) -> StructuralDiff`
  - In `mod.rs`: `pub fn tree_diff(processor: &TreeSitterProcessor, old: &VectorData, new: &VectorData, options: &TreeDiffOptions) -> Result<StructuralDiff, TreeDiffError>` = build trees → `edit_mapping` → `classify`

**Classification rules:**
- Mapped pair with differing labels → `Rename`; equal labels → no edit.
- `deleted` → `Delete`, `inserted` → `Insert`.
- `context` = the nearest **named** proper ancestor's summary (walk `parent()`; `is_named`), from the old tree for renames/deletes and the new tree for inserts.
- `snippet` = first line of the node's source slice, truncated to 60 chars (`chars().take(60)`, append `…` when truncated). Context snippets likewise.
- Edits sorted by `(row, column)` of their anchor position (old start for rename/delete, new start for insert), tie-broken by edit type order rename < delete < insert, then node ID — fully deterministic.
- `StructuralDiff` derives `Debug, Clone, PartialEq, Eq, Serialize` (needed later by `DisplayData`).

- [ ] **Step 1: Create fixtures**

`test_data/tree_diff/fn_rename/a.rs`:

```rust
fn foo() -> i32 {
    1
}

fn main() {
    println!("{}", foo());
}
```

`test_data/tree_diff/fn_rename/b.rs`: identical but both `foo` occurrences become `bar`.

`test_data/tree_diff/add_param/a.py`:

```python
def greet(name):
    return "hi " + name
```

`test_data/tree_diff/add_param/b.py`:

```python
def greet(name, excited):
    return "hi " + name
```

- [ ] **Step 2: Write failing unit tests in `src/tree_diff/output.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_diff::{LabeledTree, TreeDiffOptions, edit_mapping};

    #[test]
    fn classify_paper_example() {
        let t = LabeledTree::from_postorder(
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[Some(7), Some(2), Some(5), Some(4), Some(5), Some(7), Some(7), None],
        )
        .unwrap();
        let t_prime = LabeledTree::from_postorder(
            &[1, 2, 3, 4, 5, 8, 6, 7],
            &[Some(1), Some(4), Some(3), Some(4), Some(7), Some(6), Some(7), None],
        )
        .unwrap();
        let mapping = edit_mapping(&t, &t_prime, &TreeDiffOptions::default()).unwrap();
        let diff = classify(&t, &t_prime, &mapping);
        assert_eq!(diff.distance, 2);
        assert_eq!(diff.edits.len(), 2);
        assert!(diff
            .edits
            .iter()
            .any(|e| matches!(e.kind, StructuralEditKind::Delete { .. })));
        assert!(diff
            .edits
            .iter()
            .any(|e| matches!(e.kind, StructuralEditKind::Insert { .. })));
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
}
```

- [ ] **Step 3: Run to verify failure** — `cargo check --tests` FAILs.

- [ ] **Step 4: Implement `src/tree_diff/output.rs`**

```rust
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
pub fn classify(
    old: &LabeledTree,
    new: &LabeledTree,
    mapping: &EditMapping,
) -> StructuralDiff {
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
    // Document order, deterministic tiebreaks.
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
```

- [ ] **Step 5: Add the `tree_diff()` entry point to `src/tree_diff/mod.rs`**

```rust
mod output;

pub use mapping::{EditMapping, edit_mapping};
pub use output::{
    NodeSummary, Position, StructuralDiff, StructuralEdit, StructuralEditKind, classify,
};

use crate::input_processing::{TreeSitterProcessor, VectorData};

/// Compute a structural diff between two parsed documents.
///
/// This is the tree diff engine's main entry point, mirroring what
/// [`crate::diff::compute_edit_script`] is for the Myers engine.
pub fn tree_diff(
    processor: &TreeSitterProcessor,
    old: &VectorData,
    new: &VectorData,
    options: &TreeDiffOptions,
) -> Result<StructuralDiff, TreeDiffError> {
    let (old_tree, new_tree) = build_labeled_trees(processor, old, new);
    let mapping = edit_mapping(&old_tree, &new_tree, options)?;
    Ok(classify(&old_tree, &new_tree, &mapping))
}
```

- [ ] **Step 6: Write integration tests in `tests/tree_diff_test.rs`**

```rust
//! Integration tests for the tree diff engine on real parsed files.
#![cfg(feature = "static-grammar-libs")]

use libdiffsitter::generate_ast_vector_data;
use libdiffsitter::input_processing::TreeSitterProcessor;
use libdiffsitter::parse::GrammarConfig;
use libdiffsitter::tree_diff::{StructuralDiff, TreeDiffOptions, tree_diff};
use std::path::PathBuf;

fn diff_fixtures(name: &str, ext: &str) -> StructuralDiff {
    let root = PathBuf::from(format!("./test_data/tree_diff/{name}"));
    let a = generate_ast_vector_data(root.join(format!("a.{ext}")), None, &GrammarConfig::default())
        .unwrap();
    let b = generate_ast_vector_data(root.join(format!("b.{ext}")), None, &GrammarConfig::default())
        .unwrap();
    tree_diff(
        &TreeSitterProcessor::default(),
        &a,
        &b,
        &TreeDiffOptions::default(),
    )
    .unwrap()
}

#[test]
fn fn_rename_rust() {
    let diff = diff_fixtures("fn_rename", "rs");
    // Two identifier renames (definition + call site), nothing else.
    assert_eq!(diff.distance, 2);
    insta::assert_json_snapshot!("fn_rename_rust", diff);
}

#[test]
fn add_param_python() {
    let diff = diff_fixtures("add_param", "py");
    assert!(diff.distance > 0);
    insta::assert_json_snapshot!("add_param_python", diff);
}

#[test]
fn identical_files_have_empty_diff() {
    let root = PathBuf::from("./test_data/tree_diff/fn_rename");
    let a = generate_ast_vector_data(root.join("a.rs"), None, &GrammarConfig::default()).unwrap();
    let a2 = generate_ast_vector_data(root.join("a.rs"), None, &GrammarConfig::default()).unwrap();
    let diff = tree_diff(
        &TreeSitterProcessor::default(),
        &a,
        &a2,
        &TreeDiffOptions::default(),
    )
    .unwrap();
    assert_eq!(diff.distance, 0);
    assert!(diff.edits.is_empty());
}
```

- [ ] **Step 7: Run, review snapshots**

Run: `cargo nextest run --test tree_diff_test tree_diff`
Expected: the two snapshot tests fail on first run (no accepted snapshot). Inspect with `cargo insta review` — verify `fn_rename_rust` shows exactly two `rename` edits of kind `identifier` with `foo`/`bar` snippets and sensible contexts — then accept. Re-run to green.

- [ ] **Step 8: Lint, format, commit**

```sh
jj st
jj desc -m "feat: classify edit mappings into structural diffs

Adds the tree_diff() entry point: build labeled trees, recover the
optimal mapping, and classify it into renames/deletes/inserts with
positions, snippets, and nearest-named-ancestor context. Covered by
insta JSON snapshots over rust and python fixture pairs.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 10: DiffPayload + structural renderer

**Files:**
- Create: `src/render/structural.rs`
- Modify: `src/render/mod.rs`, `src/render/unified.rs`, `src/bin/diffsitter.rs` (compile fix only), `tests/tree_diff_test.rs` (renderer snapshot)

**Interfaces:**
- Consumes: `StructuralDiff`, `StructuralEdit(Kind)`, `TreeDiffError::RendererMismatch`
- Produces (in `render/mod.rs`):
  - `pub enum DiffPayload<'a> { Hunks(RichHunks<'a>), Structural(StructuralDiff) }` with `#[serde(untagged)]` and derives `Debug, Clone, PartialEq, Eq, Serialize`
  - `DisplayData.hunks: RichHunks<'a>` field becomes `pub diff: DiffPayload<'a>` with `#[serde(rename = "hunks")]` — the rename keeps the JSON renderer's output schema byte-identical for the Myers path (untagged enum serializes the `Hunks` variant exactly like the old field)
  - `Renderers::Structural(Structural)` variant; `RenderConfig` gains a `structural: structural::Structural` field (mirroring `unified`/`json`)
- Produces (in `render/structural.rs`): `pub struct Structural {}` (`Default`, serde-round-trippable like `Json`) implementing `Renderer`

**Renderer output format** (locked by snapshot; colors via `console::Style`, which no-ops when colors are globally disabled):

```
old.rs -> new.rs (tree diff, distance 2)
~ identifier 1:3 `foo` -> `bar`  (in function_item `fn foo() -> i32 {`)
- parameter 10:8 `verbose: bool`  (in function_item `fn parse_file(`)
+ match_arm 22:8 `Err(e) => {`  (in function_item `fn run(`)
```

Rows/columns display 1-based. `~` styled yellow, `-` red, `+` green, header bold. Empty `edits` → render nothing (standard difftool behavior).

- [ ] **Step 1: Introduce `DiffPayload` in `src/render/mod.rs`**

```rust
use crate::tree_diff::{StructuralDiff, TreeDiffError};

mod structural;
use structural::Structural;

/// The diff content produced by whichever engine ran.
///
/// Serialization is untagged so the `Hunks` variant keeps the exact JSON
/// shape `RichHunks` had before this enum existed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum DiffPayload<'a> {
    /// Line-oriented hunks from the Myers engine.
    Hunks(RichHunks<'a>),
    /// Node-level edits from the tree diff engine.
    Structural(StructuralDiff),
}
```

Change `DisplayData`:

```rust
pub struct DisplayData<'a> {
    /// The diff payload produced by the configured engine.
    ///
    /// Serialized under the historical name `hunks` so the JSON renderer's
    /// output schema stays stable for the default (Myers) engine.
    #[serde(rename = "hunks")]
    pub diff: DiffPayload<'a>,
    /// The parameters that correspond to the old document
    pub old: DocumentDiffData<'a>,
    /// The parameters that correspond to the new document
    pub new: DocumentDiffData<'a>,
}
```

Add the variant and config field:

```rust
pub enum Renderers {
    Unified,
    Json,
    Structural,
}
```

and in `RenderConfig` (+ its `Default`): `structural: structural::Structural,` initialized with `Structural::default()`.

- [ ] **Step 2: Fix `src/render/unified.rs` and `src/bin/diffsitter.rs` to compile**

`unified.rs` (`render`, line ~104):

```rust
        let DisplayData { diff, old, new } = &data;
        let DiffPayload::Hunks(hunks) = diff else {
            return Err(TreeDiffError::RendererMismatch {
                engine: "topdiff".into(),
                renderer: "unified".into(),
            }
            .into());
        };
```

(add `use crate::render::DiffPayload;` and `use crate::tree_diff::TreeDiffError;`). `json.rs` needs no change — it serializes the whole `DisplayData` and the untagged enum handles both variants.

`src/bin/diffsitter.rs` (`run_diff`): wrap the existing hunks in the payload:

```rust
    let hunks = diff::compute_edit_script(&diff_vec_a, &diff_vec_b)?;
    let params = DisplayData {
        diff: DiffPayload::Hunks(hunks),
        ...
```

(import `DiffPayload` from `libdiffsitter::render`). Run: `cargo check --all-targets` — everything compiles; `cargo nextest run render` — existing renderer tests pass.

- [ ] **Step 3: Write failing renderer tests**

In `src/render/structural.rs` (unit test, no grammars needed):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{DiffPayload, DisplayData, DocumentDiffData, Renderer};
    use crate::tree_diff::{
        NodeSummary, Position, StructuralDiff, StructuralEdit, StructuralEditKind,
    };

    fn summary(kind: &str, snippet: &str, row: usize, column: usize) -> NodeSummary {
        NodeSummary {
            kind: kind.into(),
            snippet: snippet.into(),
            start: Position { row, column },
            end: Position { row, column: column + snippet.len() },
        }
    }

    fn render_to_string(diff: StructuralDiff) -> String {
        let data = DisplayData {
            diff: DiffPayload::Structural(diff),
            old: DocumentDiffData { filename: "a.rs", text: "" },
            new: DocumentDiffData { filename: "b.rs", text: "" },
        };
        let mut buf = Vec::new();
        console::set_colors_enabled(false);
        Structural::default().render(&mut buf, &data, None).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn renders_all_edit_kinds() {
        let diff = StructuralDiff {
            distance: 3,
            edits: vec![
                StructuralEdit {
                    kind: StructuralEditKind::Rename {
                        old: summary("identifier", "foo", 0, 3),
                        new: summary("identifier", "bar", 0, 3),
                    },
                    context: Some(summary("function_item", "fn foo() {", 0, 0)),
                },
                StructuralEdit {
                    kind: StructuralEditKind::Delete {
                        node: summary("parameter", "x: i32", 2, 7),
                    },
                    context: None,
                },
                StructuralEdit {
                    kind: StructuralEditKind::Insert {
                        node: summary("match_arm", "Err(e) =>", 4, 8),
                    },
                    context: Some(summary("function_item", "fn run() {", 3, 0)),
                },
            ],
        };
        let out = render_to_string(diff);
        assert_eq!(
            out,
            "a.rs -> b.rs (tree diff, distance 3)\n\
             ~ identifier 1:4 `foo` -> `bar`  (in function_item `fn foo() {`)\n\
             - parameter 3:8 `x: i32`\n\
             + match_arm 5:9 `Err(e) =>`  (in function_item `fn run() {`)\n"
        );
    }

    #[test]
    fn empty_diff_renders_nothing() {
        let out = render_to_string(StructuralDiff { edits: vec![], distance: 0 });
        assert!(out.is_empty());
    }

    #[test]
    fn rejects_hunks_payload() {
        let data = DisplayData {
            diff: DiffPayload::Hunks(crate::diff::RichHunks(Vec::new())),
            old: DocumentDiffData { filename: "a.rs", text: "" },
            new: DocumentDiffData { filename: "b.rs", text: "" },
        };
        let mut buf = Vec::new();
        assert!(Structural::default().render(&mut buf, &data, None).is_err());
    }
}
```

Also extend the existing tag test in `src/render/mod.rs`: add `#[test_case("structural")]` to `test_get_renderer_custom_tag`.

- [ ] **Step 4: Run to verify failure** — `cargo check --tests` FAILs (no `Structural`).

- [ ] **Step 5: Implement `src/render/structural.rs`**

```rust
//! A renderer for structural (tree diff) output: one annotated line per
//! node-level edit, with positions and enclosing context.

use console::{Style, Term};
use serde::{Deserialize, Serialize};
use std::io::Write;

use super::{DiffPayload, DisplayData, Renderer};
use crate::tree_diff::{NodeSummary, StructuralEditKind, TreeDiffError};

/// Renders the structural edits produced by the tree diff engine.
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug, Default)]
pub struct Structural {}

impl Renderer for Structural {
    fn render(
        &self,
        writer: &mut dyn Write,
        data: &DisplayData,
        _term_info: Option<&Term>,
    ) -> anyhow::Result<()> {
        let DiffPayload::Structural(diff) = &data.diff else {
            return Err(TreeDiffError::RendererMismatch {
                engine: "myers".into(),
                renderer: "structural".into(),
            }
            .into());
        };
        if diff.edits.is_empty() {
            return Ok(());
        }
        let header = format!(
            "{} -> {} (tree diff, distance {})",
            data.old.filename, data.new.filename, diff.distance
        );
        writeln!(writer, "{}", Style::new().bold().apply_to(header))?;
        for edit in &diff.edits {
            let line = match &edit.kind {
                StructuralEditKind::Rename { old, new } => Style::new().yellow().apply_to(
                    format!("~ {} {} `{}` -> `{}`", old.kind, position(old), old.snippet, new.snippet),
                ),
                StructuralEditKind::Delete { node } => Style::new()
                    .red()
                    .apply_to(format!("- {} {} `{}`", node.kind, position(node), node.snippet)),
                StructuralEditKind::Insert { node } => Style::new()
                    .green()
                    .apply_to(format!("+ {} {} `{}`", node.kind, position(node), node.snippet)),
            };
            match &edit.context {
                Some(ctx) => writeln!(writer, "{line}  (in {} `{}`)", ctx.kind, ctx.snippet)?,
                None => writeln!(writer, "{line}")?,
            }
        }
        Ok(())
    }
}

/// Display position as 1-based `row:column`.
fn position(node: &NodeSummary) -> String {
    format!("{}:{}", node.start.row + 1, node.start.column + 1)
}
```

- [ ] **Step 6: Run all render + tree_diff tests**

Run: `cargo nextest run render tree_diff && cargo nextest run --test regression_test`
Expected: PASS, including the untouched regression snapshots (proving the Myers path's output is unchanged).

- [ ] **Step 7: Add an end-to-end renderer snapshot to `tests/tree_diff_test.rs`**

```rust
use libdiffsitter::render::{DiffPayload, DisplayData, DocumentDiffData, RenderConfig, Renderer};

#[test]
fn structural_renderer_snapshot() {
    let diff = diff_fixtures("fn_rename", "rs");
    let data = DisplayData {
        diff: DiffPayload::Structural(diff),
        old: DocumentDiffData { filename: "a.rs", text: "" },
        new: DocumentDiffData { filename: "b.rs", text: "" },
    };
    // Disable colors so the snapshot has no ANSI escapes.
    console::set_colors_enabled(false);
    let renderer = RenderConfig::default()
        .get_renderer(Some("structural".into()))
        .unwrap();
    let mut buf = Vec::new();
    renderer.render(&mut buf, &data, None).unwrap();
    insta::assert_snapshot!("structural_render_fn_rename", String::from_utf8(buf).unwrap());
}
```

`get_renderer` already resolves the `"structural"` tag via the strum iterator, so no new API is needed. Add `console` to `[dev-dependencies]` in `Cargo.toml` (it is already a main dependency, so this adds no new crate) so the integration test can call `console::set_colors_enabled(false)`.

Run: `cargo nextest run --test tree_diff_test`, review + accept the snapshot with `cargo insta review`.

- [ ] **Step 8: Lint, format, commit**

```sh
jj st
jj desc -m "feat: add structural diff renderer

DisplayData now carries a DiffPayload enum (serialized under the
historical 'hunks' key so JSON output for the Myers engine is
unchanged). The new 'structural' renderer prints one annotated,
colored line per node-level edit; unified rejects structural
payloads with a typed RendererMismatch error.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 11: Config, CLI, and engine dispatch

**Files:**
- Modify: `src/config.rs`, `src/cli.rs`, `src/bin/diffsitter.rs`, `assets/sample_config.json5`
- Create: `resources/test_configs/tree_diff.json5`

**Interfaces:**
- Consumes: `tree_diff`, `TreeDiffOptions`, `TreeDiffError`, `DiffPayload`, `Renderers`
- Produces (in `config.rs`):
  - `pub enum DiffEngine { Myers (default), Topdiff }` — derives `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Display, EnumString`; `#[serde(rename_all = "kebab-case")]`, `#[strum(serialize_all = "snake_case")]` (string forms: `myers`, `topdiff`)
  - `Config` gains `pub diff_engine: DiffEngine` and `pub tree_diff: TreeDiffOptions` (both covered by the struct-level `#[serde(default)]`)
- Produces (in `cli.rs`): `pub diff_engine: Option<String>` with `#[clap(long)]`

- [ ] **Step 1: Add config fields and the engine enum**

In `src/config.rs` (imports: `use crate::tree_diff::TreeDiffOptions;` and `use strum::{Display, EnumString};` — match the import style used in `render/mod.rs`):

```rust
/// Which diff algorithm drives the comparison.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Display, EnumString,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "snake_case")]
pub enum DiffEngine {
    /// Myers diff over the flattened leaf sequence (the classic behavior).
    #[default]
    Myers,
    /// Tree edit distance over the AST (reports structural edits; requires
    /// the "structural" renderer).
    Topdiff,
}
```

and in `Config` (after `input_processing`):

```rust
    /// Which diff engine to use.
    ///
    /// "myers" (the default) diffs the flattened token sequence and renders
    /// hunks. "topdiff" computes a tree edit distance over the AST and
    /// reports node-level structural edits.
    pub diff_engine: DiffEngine,

    /// Options for the tree diff engine (used when `diff-engine` is
    /// "topdiff").
    pub tree_diff: TreeDiffOptions,
```

- [ ] **Step 2: Update `assets/sample_config.json5`** (before the `"input-processing"` key):

```json5
    // Which diff engine to use: "myers" (token-sequence diff, the default)
    // or "topdiff" (tree edit distance; requires the "structural" renderer).
    "diff-engine": "myers",
    // Options for the "topdiff" tree diff engine.
    "tree-diff": {
        // Abort when the distance search bound exceeds this value; wildly
        // dissimilar inputs are better served by the myers engine.
        "max-tau": 2048,
    },
```

- [ ] **Step 3: Add a file-driven config test fixture**

Create `resources/test_configs/tree_diff.json5` (picked up automatically by the `rstest` `#[files(...)]` test in `config.rs`):

```json5
{
    "diff-engine": "topdiff",
    "tree-diff": {
        "max-tau": 512,
    },
}
```

Run: `cargo nextest run config` — `test_sample_config` and the rstest file tests pass.

- [ ] **Step 4: Add the CLI flag in `src/cli.rs`** (after `renderer`):

```rust
    /// Specify which diff engine to use. Valid values are: "myers", "topdiff".
    ///
    /// This overrides the "diff-engine" config key. The "topdiff" engine
    /// computes a tree edit distance and requires the "structural" renderer.
    #[clap(long)]
    pub diff_engine: Option<String>,
```

- [ ] **Step 5: Dispatch in `src/bin/diffsitter.rs` `run_diff`**

Imports to add: `use libdiffsitter::config::DiffEngine;`, `use libdiffsitter::render::{DiffPayload, Renderers};`, `use libdiffsitter::tree_diff::{self, TreeDiffError};`, `use std::str::FromStr;`, `use anyhow::anyhow;`.

Replace the middle of `run_diff` (keep renderer resolution at the top):

```rust
    let engine = match args.diff_engine.as_deref() {
        Some(s) => DiffEngine::from_str(s)
            .map_err(|_| anyhow!("'{s}' is not a valid diff engine (expected one of: myers, topdiff)"))?,
        None => config.diff_engine,
    };

    let file_type = args.file_type.as_deref();
    let path_a = args.old.as_ref().unwrap();
    let path_b = args.new.as_ref().unwrap();
    let ast_data_a = generate_ast_vector_data(path_a.clone(), file_type, &config.grammar)?;
    let ast_data_b = generate_ast_vector_data(path_b.clone(), file_type, &config.grammar)?;

    // The Myers hunks borrow from the processed vectors, so those must
    // outlive `params`; they are only initialized on the Myers path.
    let diff_vec_a;
    let diff_vec_b;
    let diff_payload = match engine {
        DiffEngine::Myers => {
            diff_vec_a = config.input_processing.process_vec_data(&ast_data_a);
            diff_vec_b = config.input_processing.process_vec_data(&ast_data_b);
            DiffPayload::Hunks(diff::compute_edit_script(&diff_vec_a, &diff_vec_b)?)
        }
        DiffEngine::Topdiff => {
            if !matches!(renderer, Renderers::Structural(_)) {
                return Err(TreeDiffError::RendererMismatch {
                    engine: engine.to_string(),
                    renderer: renderer.to_string(),
                }
                .into());
            }
            let structural_diff = tree_diff::tree_diff(
                &config.input_processing,
                &ast_data_a,
                &ast_data_b,
                &config.tree_diff,
            )
            .map_err(|e| match e {
                TreeDiffError::BoundExceeded { .. } => anyhow!(e).context(
                    "the inputs are too dissimilar for the tree diff engine; \
                     rerun with --diff-engine myers",
                ),
                other => anyhow!(other),
            })?;
            DiffPayload::Structural(structural_diff)
        }
    };
    let params = DisplayData {
        diff: diff_payload,
        old: DocumentDiffData { ... unchanged ... },
        new: DocumentDiffData { ... unchanged ... },
    };
```

- [ ] **Step 6: Verify end-to-end by hand**

```sh
cargo build
./target/debug/diffsitter --diff-engine topdiff --renderer structural \
    test_data/tree_diff/fn_rename/a.rs test_data/tree_diff/fn_rename/b.rs
```

Expected: header + two yellow `~ identifier ... \`foo\` -> \`bar\`` lines.

```sh
./target/debug/diffsitter --diff-engine topdiff \
    test_data/tree_diff/fn_rename/a.rs test_data/tree_diff/fn_rename/b.rs
```

Expected: exits non-zero with the `RendererMismatch` message naming both engine and renderer.

```sh
./target/debug/diffsitter test_data/tree_diff/fn_rename/a.rs test_data/tree_diff/fn_rename/b.rs
```

Expected: unchanged Myers/unified output.

- [ ] **Step 7: Full test suite, lint, format**

Run: `cargo nextest run --all-features && cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all`
Expected: everything green (this catches any `mcp-server`-feature interaction).

- [ ] **Step 8: Commit**

```sh
jj st
jj desc -m "feat: wire topdiff engine into config and CLI

Adds the diff-engine config key and --diff-engine flag (myers |
topdiff), tree-diff engine options (max-tau), engine dispatch in the
binary with a typed error for renderer mismatches, and sample-config
coverage.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 12: Benchmarks

**Files:**
- Create: `benches/tree_diff_bench.rs`
- Modify: `Cargo.toml` (register the bench)

**Interfaces:**
- Consumes: `tree_diff`, `TreeDiffOptions`, `TreeSitterProcessor`, `VectorData`, `compute_edit_script`, `generate_language`

- [ ] **Step 1: Register in `Cargo.toml`** (next to the existing `[[bench]]`):

```toml
[[bench]]
name = "tree_diff_bench"
harness = false
required-features = ["static-grammar-libs"]
```

- [ ] **Step 2: Write `benches/tree_diff_bench.rs`**

```rust
//! Benchmarks comparing the tree diff engine against the Myers engine on
//! synthetic Rust sources with small and large edits.

use criterion::{Criterion, criterion_group, criterion_main};
use libdiffsitter::diff::compute_edit_script;
use libdiffsitter::input_processing::{TreeSitterProcessor, VectorData};
use libdiffsitter::parse::{GrammarConfig, generate_language};
use libdiffsitter::tree_diff::{TreeDiffOptions, tree_diff};
use std::hint::black_box;
use std::path::PathBuf;

fn parse_rust(text: &str) -> VectorData {
    let language = generate_language("rust", &GrammarConfig::default()).unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let tree = parser.parse(text, None).unwrap();
    VectorData {
        text: text.to_string(),
        tree,
        path: PathBuf::from("bench.rs"),
        resolved_language: "rust".into(),
    }
}

/// Generate a source file with `n_fns` small functions; functions whose index
/// is in `renamed` get a different name (a "small edit" between versions).
fn make_source(n_fns: usize, renamed: &[usize]) -> String {
    let mut out = String::new();
    for i in 0..n_fns {
        let name = if renamed.contains(&i) {
            format!("renamed_{i}")
        } else {
            format!("original_{i}")
        };
        out.push_str(&format!("fn {name}(x: i64) -> i64 {{\n    x + {i}\n}}\n\n"));
    }
    out
}

fn bench_engines(c: &mut Criterion) {
    let processor = TreeSitterProcessor::default();
    let options = TreeDiffOptions::default();

    // Small edit: 200 functions, one renamed.
    let old_small = parse_rust(&make_source(200, &[]));
    let new_small = parse_rust(&make_source(200, &[100]));
    // Larger edit: 200 functions, 25 renamed.
    let old_large = parse_rust(&make_source(200, &[]));
    let renamed: Vec<usize> = (0..200).step_by(8).collect();
    let new_large = parse_rust(&make_source(200, &renamed));

    let mut group = c.benchmark_group("tree_diff_vs_myers");
    group.bench_function("topdiff_small_edit", |b| {
        b.iter(|| black_box(tree_diff(&processor, &old_small, &new_small, &options).unwrap()));
    });
    group.bench_function("topdiff_larger_edit", |b| {
        b.iter(|| black_box(tree_diff(&processor, &old_large, &new_large, &options).unwrap()));
    });
    group.bench_function("myers_small_edit", |b| {
        b.iter(|| {
            let a = processor.process_vec_data(&old_small);
            let bv = processor.process_vec_data(&new_small);
            black_box(compute_edit_script(&a, &bv).unwrap())
        });
    });
    group.bench_function("myers_larger_edit", |b| {
        b.iter(|| {
            let a = processor.process_vec_data(&old_large);
            let bv = processor.process_vec_data(&new_large);
            black_box(compute_edit_script(&a, &bv).unwrap())
        });
    });
    group.finish();
}

criterion_group!(benches, bench_engines);
criterion_main!(benches);
```

If import paths or `criterion` idioms differ from the existing suite, mirror `benches/ast_navigation_bench.rs` — it is the authoritative in-repo example.

- [ ] **Step 3: Run**

Run: `cargo bench --bench tree_diff_bench -- --quick`
Expected: four benchmarks complete; note the topdiff-vs-myers ratios in the commit message body if notable. Sanity-expect topdiff to be slower than Myers (it computes a richer result) but well under a second per iteration on the small edit.

- [ ] **Step 4: Lint, format, commit**

```sh
jj st
jj desc -m "bench: add tree diff benchmarks

Criterion group comparing the topdiff engine against myers on
synthetic rust sources with small and larger edit counts.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

### Task 13: Fuzz target + docs

**Files:**
- Create: `fuzz/fuzz_targets/fuzz_tree_diff.rs`
- Modify: `fuzz/Cargo.toml`, `CLAUDE.md`

**Interfaces:**
- Consumes: `tree_diff`, `TreeDiffOptions`, `TreeDiffError`, `VectorData`, `generate_language`

- [ ] **Step 1: Register the target in `fuzz/Cargo.toml`** (after the last `[[bin]]`):

```toml
[[bin]]
name = "fuzz_tree_diff"
path = "fuzz_targets/fuzz_tree_diff.rs"
test = false
doc = false
```

- [ ] **Step 2: Write `fuzz/fuzz_targets/fuzz_tree_diff.rs`**

```rust
#![no_main]

use libdiffsitter::input_processing::{TreeSitterProcessor, VectorData};
use libdiffsitter::parse::{GrammarConfig, generate_language};
use libdiffsitter::tree_diff::{TreeDiffError, TreeDiffOptions, tree_diff};
use libfuzzer_sys::fuzz_target;
use std::path::PathBuf;

fuzz_target!(|data: &[u8]| {
    // Layout: [split byte][old source bytes][new source bytes].
    if data.len() < 3 {
        return;
    }
    let body = &data[1..];
    let split = (data[0] as usize) % (body.len() + 1);
    let (old_bytes, new_bytes) = body.split_at(split);
    let (Ok(old_text), Ok(new_text)) = (
        std::str::from_utf8(old_bytes),
        std::str::from_utf8(new_bytes),
    ) else {
        return;
    };

    let Ok(language) = generate_language("rust", &GrammarConfig::default()) else {
        return;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return;
    }
    let (Some(old_tree), Some(new_tree)) =
        (parser.parse(old_text, None), parser.parse(new_text, None))
    else {
        return;
    };
    let old = VectorData {
        text: old_text.to_string(),
        tree: old_tree,
        path: PathBuf::from("a.rs"),
        resolved_language: "rust".into(),
    };
    let new = VectorData {
        text: new_text.to_string(),
        tree: new_tree,
        path: PathBuf::from("b.rs"),
        resolved_language: "rust".into(),
    };

    // Small bound keeps each fuzz iteration fast; BoundExceeded is a valid
    // outcome, any other error or panic is a bug.
    let processor = TreeSitterProcessor::default();
    let options = TreeDiffOptions { max_tau: 64 };
    match tree_diff(&processor, &old, &new, &options) {
        Ok(diff) => {
            // Unit costs: every edit costs exactly 1, so counts must agree.
            assert_eq!(diff.edits.len() as u32, diff.distance);
            // Determinism.
            let again = tree_diff(&processor, &old, &new, &options).unwrap();
            assert_eq!(diff, again);
        }
        Err(TreeDiffError::BoundExceeded { .. }) => {}
        Err(other) => panic!("unexpected tree diff error: {other}"),
    }
});
```

(One caveat: `parser.parse` timeouts/`None` are skipped; mirror `fuzz_parse_and_navigate.rs` if its language-loading pattern differs.)

- [ ] **Step 3: Smoke-run the fuzzer**

Run: `cargo +nightly fuzz run fuzz_tree_diff -- -max_total_time=60`
Expected: no crashes in 60 seconds. If a crash reproduces, minimize with `cargo +nightly fuzz tmin` and fix before committing (the most likely findings are backtrace invariant violations — they surface as `MappingBacktrace` panics via the `other =>` arm).

- [ ] **Step 4: Update `CLAUDE.md`**

- Architecture section, after the pipeline list, add one bullet:
  `6. Tree diff engine (\`src/tree_diff/\`) — opt-in TED-based engine (\`--diff-engine topdiff\` + \`--renderer structural\`) implementing Pawlik & Augsten's TopDiff/TopDiff+/AutoStop; produces node-level structural edits instead of hunks`
- Test categories: bump fuzz target count to 4 and add `fuzz_tree_diff`; mention `tests/tree_diff_test.rs` and `tests/tree_diff_proptest.rs` in the test-category list with one line each.

- [ ] **Step 5: Final full verification**

Run: `cargo nextest run --all-features && cargo test --doc --all-features && cargo clippy --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: all green.

- [ ] **Step 6: Commit**

```sh
jj st
jj desc -m "test: add tree diff fuzz target

Fuzzes rust source pairs through the full tree diff pipeline,
asserting the edit-count/distance invariant, determinism, and that
the only permitted failure is BoundExceeded. Also documents the new
engine in CLAUDE.md.

Co-Authored-By: Claude <noreply@anthropic.com>"
jj new
```

---

## Post-plan verification checklist (run after Task 13)

1. `cargo nextest run --all-features` — full suite green.
2. `cargo bench --bench tree_diff_bench -- --quick` — benchmarks run.
3. Manual smoke: the three CLI invocations from Task 11 Step 6.
4. `jj --no-pager log` — one commit per task, conventional messages.
