# TopDiff Tree-Diff Engine — Design

**Date:** 2026-07-02
**Paper:** Pawlik & Augsten, *Minimal Edit-Based Diffs for Large Trees* (CIKM '20)
**Status:** Approved

## Goal

Add a structurally-aware diff engine to diffsitter based on tree edit distance
(TED). The existing pipeline flattens the AST into a leaf sequence and runs
Myers diff over it; it cannot express renames, subtree moves, or nesting
changes. TED can, but classic exact algorithms (APTED) run in O(n³) time and
O(n²) space. The paper's TopDiff+/AutoStop stack makes exact TED feasible on
large, similar trees — O(nδ³ log δ) where δ is the diff size — which is what
makes a true tree diff practical here.

The motivation is **diff quality**, not raw speed: leaf-sequence Myers is
already fast. TED is a richer model that surfaces structure.

## Decisions

| Question | Decision |
|---|---|
| Motivation | Structural diff quality |
| Integration | Opt-in second engine; Myers stays the default |
| Rendering | New structural renderer (annotated text diff + structural notes) |
| Algorithm scope | Full stack: TouzetDepth, TopDiff, TopDiff+ cost switch, AutoStop |
| Tree model | Processed tree reusing `TreeSitterProcessor` filter semantics |
| Fallback behavior | No silent fallback; typed errors with a clear message |

## Architecture

### Pipeline

```
                 ┌─ engine = myers (default, unchanged)
parse →          │    flatten leaves → Myers → RichHunks → unified/json
VectorData ──────┤
                 └─ engine = topdiff (new)
                      build LabeledTree ×2 → AutoStop(TopDiff+) → EditMapping
                      → StructuralDiff → structural renderer
```

`src/diff.rs` (Myers/hunk world) is untouched.

### New module: `src/tree_diff/`

- **`tree.rs`** — `LabeledTree<'a>`: postorder structure-of-arrays
  representation. Per node: parent, depth, subtree size, leftmost leaf
  descendant (lld), interned label id, source byte range, and tree-sitter
  `Point`s. Built from the tree-sitter CST applying the same
  `TreeSitterProcessor` semantics as the Myers path (excluded kinds,
  pseudo-leaves, whitespace stripping) so formatting-insensitivity is
  consistent across engines. Labels: `kind_id` for internal nodes,
  `(kind_id, text)` for leaves, interned to `u32` for O(1) comparison.
  Follows the VectorData ownership pattern: borrows `&'a str` from the
  owning `VectorData`. Neighborhood vectors `(l, d, a, r)` are derived in
  O(1) from postorder id, depth, and subtree size.
- **`ted.rs`** — shared Zhang-Shasha-style forest-distance DP core (the
  `FD`/`TD` matrices), stored as bands of width `2τ+1` indexed by
  `(postorder, postorder difference)` per the paper's O(nτ) memory bound. A
  `BandMatrix` type encapsulates the offset arithmetic (analogous to
  `NegIdxVec` for Myers). Pruning predicates are parameters so TouzetDepth
  and TopDiff share the core.
- **`topdiff.rs`** — Algorithm 3 (`ComputeTopNodePairs`, linear time) and
  Algorithm 4 (TopDiff). The outer loop iterates the `TN` list instead of
  all band pairs; by-product distances are stored for all `(i,j) <l (x,y)`.
- **`touzet.rs`** — Algorithm 2 (TouzetDepth): subtree pruning
  (neighborhood distance ≤ τ), edits pruning (postorder difference ≤ edits
  budget ε), and depth-based pruning (`depth(i) − depth(x) − 1 < ε`).
- **`cost.rs`** — the two O(n) cost estimates from §6
  (`cost(TopDiff) = Σ_{leaves} |T^l|`,
  `cost(TouzetDepth, τ) = Σ_x min{τ, depth(x)}`) and the TopDiff+
  break-even switch (Algorithm 5).
- **`autostop.rs`** — Algorithm 6: τ starts at `max(||T| − |T′||, 1)`,
  doubles until the stopping condition `δτ(T,T′) ≤ τ` holds (Theorem 7.2).
  The cost estimate is re-evaluated each round (it is τ-dependent for
  TouzetDepth).
- **`mapping.rs`** — mapping recovery and classification (below).

### Cost model

Exactly the paper's: rename cost 0 iff labels identical, else 1;
insert/delete cost 1 each.

### Mapping recovery (our addition — the paper stops at δ)

Two-phase. The forward pass computes and retains the `TD` band (subtree
distances for all surviving pairs). The backtrace starts at `(root, root)`,
re-runs the FD computation for one subtree pair at a time, and walks the
min-argument chain backwards, emitting mapped pairs, deletions, and
insertions, recursing through `TD` lookups where the DP consumed a subtree
distance. This is standard Zhang-Shasha mapping recovery adapted to banded
matrices; it recomputes at most the subproblems along the optimal path, so
asymptotic bounds are unchanged. The mapping's cost must equal the returned
δ — asserted in tests and via `debug_assert!`.

### StructuralDiff

Classification of the `EditMapping`:

- **Rename**: mapped pair with differing labels
- **Delete**: unmapped node in the old tree
- **Insert**: unmapped node in the new tree

Each edit carries source positions and the nearest enclosing *named*
ancestor node (for annotation context, e.g. "in fn parse_file").

### Engine selection & renderer coupling

- Config: top-level `diff-engine: "myers" | "topdiff"` (kebab-case,
  `#[serde(default)]` → myers). CLI: `--diff-engine` override.
  `assets/sample_config.json5` must be updated (CI parses it).
- New `Structural` variant in the `Renderers` enum via `enum_dispatch`,
  same pattern as `unified`/`json`.
- `DisplayData` gains an enum payload: `Hunks(RichHunks)` /
  `Structural(StructuralDiff)`. A mismatched engine/renderer combination
  (e.g. `topdiff` + `json`) fails up-front with a typed error and a clear
  message. No silent fallback.

### Structural renderer (`src/render/structural.rs`)

Annotated structural notes: a bold header naming both files and the
distance, then one colored line per node-level edit with its 1-based
position, snippet, and nearest named ancestor as context (this is the
format the implementation plan locked and Task 10 shipped, pinned by
snapshot):

```
old.rs -> new.rs (tree diff, distance 2)
~ identifier 1:4 `foo` -> `bar`  (in function_item `fn foo() -> i32 {`)
- parameter 10:8 `verbose: bool`  (in function_item `fn parse_file(`)
+ match_arm 22:8 `Err(e) => {`  (in function_item `fn run(`)
```

Follow-up (not shipped): additionally rendering the changed source
regions line-by-line in `unified.rs`'s style, interleaved with these
notes.

## Guardrails

- AutoStop's τ can grow to 2δ; for dissimilar files δ ≈ n and TED becomes
  expensive. A configurable `max-tau` (default 2048) caps the doubling. If
  exceeded, return `TreeDiffError::BoundExceeded { tau, limit }`; the
  binary suggests `--diff-engine myers`.
- First version is fully safe Rust; band indexing is checked. `unsafe`
  only if profiling later justifies it, with the project's `debug_assert!`
  discipline.

## Error handling

`thiserror` enum `TreeDiffError` in the library: `BoundExceeded`,
`RendererMismatch { engine, renderer }`, and tree-construction failures.
`anyhow` only at the binary boundary, per project rules.

## Testing

- **Oracle proptest** (highest value): a naive, obviously-correct
  Zhang-Shasha TED (test-only, no pruning) vs. TopDiff, TouzetDepth, and
  AutoStop on random small trees (≤ ~25 nodes) — distances must match
  exactly.
- **Unit tests**: the paper's running example (Fig. 1: δ = 2; TN set from
  Example 5.2 verified exactly), neighborhood vectors, lld, top node pair
  computation, `BandMatrix`, stopping condition, mapping-cost = δ.
- **Property tests**: δ(T,T) = 0; δ(T,T′) = δ(T′,T); δ ≤ |T| + |T′|;
  mapping validity (one-to-one, ancestor, order conditions of Def. 3.2).
- **Integration + snapshots**: parameterized across languages on
  `test_data/`, insta JSON snapshots of `StructuralDiff`, and snapshot
  tests of the structural renderer output.
- **Benchmarks**: criterion group comparing topdiff vs. myers on
  small-edit and large-edit pairs.
- **Fuzz target**: `fuzz_tree_diff` — random source pair; assert no panic
  and the mapping-cost invariant.

## Milestones

1. `LabeledTree` + builder + unit tests
2. DP core + TouzetDepth + oracle proptest
3. Top node pairs + TopDiff + paper-example tests
4. Cost model + TopDiff+ + AutoStop
5. Mapping recovery + `StructuralDiff`
6. Config/CLI wiring + structural renderer + snapshots
7. Benchmarks + fuzz target

## Out of scope

- Move detection (TED models delete/insert/rename only; moves appear as
  delete+insert pairs)
- Replacing Myers as the default engine
- Structural JSON renderer (possible follow-up)
- `unsafe` optimizations
