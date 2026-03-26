---
name: debug-diff
description: "Diagnose unexpected diff output by tracing the pipeline from parsing through AST processing to hunk generation. Use when diffsitter produces wrong or surprising results."
allowed-tools: Read, Grep, Glob, Bash
user-invocable: true
argument-hint: "[description] What unexpected behavior are you seeing?"
---

# Debugging Unexpected Diff Output in diffsitter

When diffsitter produces wrong or surprising results, trace the issue through the pipeline. The diff pipeline has 7 stages, and problems can originate at any of them.

Ask the user for the following information if not already provided:
- The two files being compared (or representative snippets)
- The language/file extension
- Any custom config being used
- What output they expected vs what they got

## Pipeline Overview

The full pipeline (see `src/bin/diffsitter.rs`, function `run_diff`):

```
1. Language detection
2. Tree-sitter parse (file -> AST Tree)
3. AST leaf extraction (Tree -> Vec<VectorLeaf>)
4. Node filtering (exclude_kinds / include_kinds)
5. Grapheme splitting + whitespace stripping (VectorLeaf -> Vec<Entry>)
6. Myers diff (Vec<Entry> x 2 -> Vec<EditType>)
7. Hunk assembly (Vec<EditType> -> RichHunks)
8. Rendering (RichHunks -> terminal output)
```

## Stage-by-Stage Debugging

### Stage 1: Language Detection

**File**: `src/parse.rs`, function `resolve_language_str`

The language is resolved from the file extension via the `FILE_EXTS` phf_map, with optional user overrides from `GrammarConfig.file_associations`.

**Common issues:**
- Extension not mapped (e.g., `.jsx` maps to `"tsx"`, `.h` maps to `"c"` not `"cpp"`)
- User override in config shadowing the default

**Debug**: Check what language is resolved:
```sh
# Enable debug logging to see language resolution
RUST_LOG=debug diffsitter old_file new_file 2>&1 | grep -i "deduced language"
```

Or read `FILE_EXTS` in `src/parse.rs` to verify the extension mapping.

### Stage 2: Tree-sitter Parse

**File**: `src/parse.rs`, function `parse_file`

Creates a `Parser`, sets the language, reads the file to a string, and calls `parser.parse(&text, None)`.

**Common issues:**
- Grammar ABI version mismatch (`AbiOutOfRange` error) -- the grammar was compiled against an incompatible tree-sitter version.
- Parse failure returns `None` from `parser.parse()`, resulting in a `TSParseFailure` error.
- File encoding issues (tree-sitter expects UTF-8).

**Debug**: If parsing succeeds but the tree looks wrong, the grammar itself may have a bug for that language construct. Check tree-sitter's own playground or CLI to inspect the AST.

### Stage 3: AST Leaf Extraction

**File**: `src/input_processing.rs`, function `build`

Recursively walks the tree via `node.children(&mut cursor)`. Collects leaf nodes (nodes with `child_count() == 0`) OR nodes whose `kind()` matches a `pseudo_leaf_types` entry.

**Key behaviors:**
- Empty byte ranges are skipped (`node.byte_range().is_empty()`)
- Nodes that are pure newlines (after removing `\n`, `\r`, `\r\n`) are skipped -- this is a workaround for the Go parser
- Pseudo-leaf types (configured in `input_processing.pseudo_leaf_types`) treat certain non-leaf nodes as leaves. Default: `{"markdown": {"inline"}}`. This is critical for text-heavy documents.

**Common issues:**
- Missing diffs in markdown/prose: Check if `pseudo_leaf_types` includes the right node types for that language. Without `"inline"` for markdown, large text blocks are treated as single atoms.
- Unexpected nodes included: The grammar may expose more leaf nodes than expected (e.g., punctuation, delimiters).

### Stage 4: Node Filtering

**File**: `src/input_processing.rs`, method `TreeSitterProcessor::should_include_node`

Filters nodes based on `exclude_kinds` and `include_kinds` from the config:
- `exclude_kinds` takes precedence: if a node's `kind()` is in this set, it is excluded.
- `include_kinds`: if set, only nodes whose `kind()` is in this set are included (unless also excluded).
- If neither is set, all nodes pass through.

**Common issues:**
- User config has `exclude_kinds` or `include_kinds` that filters out relevant nodes.
- The node `kind()` string doesn't match what the user expects (tree-sitter kind names are grammar-specific).

**Debug**: Check what node kinds exist for a language using tree-sitter's node types. The `kind()` strings come from the grammar definition.

### Stage 5: Grapheme Splitting and Whitespace Stripping

**File**: `src/input_processing.rs`, method `VectorLeaf::split_on_graphemes`

If `split_graphemes` is enabled (default: true), each leaf's text is split into individual Unicode graphemes, each becoming its own `Entry` with precise row/column positions. If `strip_whitespace` is enabled (default: true), whitespace-only graphemes are skipped.

**Common issues:**
- **Whitespace-only diffs not showing**: `strip_whitespace: true` (the default) means pure whitespace/indentation changes are invisible. This is by design for AST-based diffing. If the user wants to see whitespace changes, they need `"strip-whitespace": false` in their config.
- **Performance with large files**: `split_graphemes: true` generates many entries for large text nodes. Setting `"split-graphemes": false` trades granularity for speed.
- **Line position bugs**: The grapheme splitter tracks row/column positions. If `line_offset == 0`, it offsets from the node's `start_position().column`. Otherwise it resets the column to `idx` (the byte offset within the line). Bugs here manifest as incorrect column highlighting in the rendered output.

### Stage 6: Myers Diff

**File**: `src/diff.rs`, struct `Myers`, method `diff`

Implements the classic Myers "An O(ND) Difference Algorithm" with divide-and-conquer via middle snake. Takes two `&[Entry]` slices, produces `Vec<EditType<&Entry>>` where `EditType` is either `Addition` or `Deletion`.

**Key optimizations:**
- Common prefix/suffix are skipped before running the main algorithm (via `common_prefix_len` / `common_suffix_len` which use `get_unchecked` for speed).
- `Entry` equality is based on `kind_id` AND `text` (see the `PartialEq` impl) -- two nodes are equal only if they have the same tree-sitter kind AND identical text content.

**Common issues:**
- **Too many diffs reported**: If entries that should be equal are not, check if `kind_id` differs between them. Two nodes with the same text but different grammar kinds (e.g., `identifier` vs `type_identifier` in Rust) are considered different.
- **No diffs reported when expected**: If entries that should differ are comparing as equal, verify the text content after whitespace stripping.

### Stage 7: Hunk Assembly

**File**: `src/diff.rs`, struct `RichHunksBuilder`

Converts the flat edit script into grouped hunks. Each `EditType::Addition` becomes `DocumentType::New`, each `EditType::Deletion` becomes `DocumentType::Old`. Consecutive edits on adjacent lines are grouped into the same `Hunk`. Non-adjacent edits start a new hunk.

**Common issues:**
- **`PriorLine` or `PriorColumn` errors**: These indicate the edit script produced entries in non-ascending order, which is a bug in the diff or input processing stage.
- **Hunks splitting unexpectedly**: If edits on adjacent lines end up in separate hunks, there may be a gap in line numbers caused by filtered-out nodes.

### Stage 8: Rendering

**File**: `src/render/unified.rs` (for unified renderer), `src/render/json.rs` (for JSON)

**Common issues with unified renderer:**
- **Line index out of bounds**: The `print_hunk` method accesses `lines[line_index]`. If `line_index >= lines.len()`, it logs an error and skips the line (in release) or panics via `debug_assert!` (in debug).
- **Column range panics**: `print_line` indexes into the line text using byte column ranges from entries. If the entry positions don't align with the actual text bytes, this can panic with a slice bounds error.
- **Missing terminal colors**: If output is piped, `term_info` may be `None`. The unified renderer still works but won't have terminal width info for title formatting.

## Quick Diagnostic Commands

```sh
# Run with full debug logging
RUST_LOG=trace diffsitter old_file new_file 2>debug.log

# Output as JSON to inspect raw diff data
diffsitter --renderer json old_file new_file | jq .

# Force a specific language
diffsitter --file-type python old_file new_file

# Run without config to eliminate config issues
diffsitter --no-config old_file new_file
```

## Common Failure Modes Summary

| Symptom | Likely Stage | Check |
|---------|-------------|-------|
| "Unsupported extension" error | 1 (Language detection) | Extension in `FILE_EXTS`? |
| Parse error / empty AST | 2 (Tree-sitter parse) | Grammar ABI compatible? File is valid UTF-8? |
| No diff when content clearly changed | 5 (Whitespace stripping) | `strip_whitespace` removing the changes? |
| Missing diffs in markdown/prose | 3 (Leaf extraction) | `pseudo_leaf_types` configured for language? |
| Diffs include too many trivial nodes | 4 (Node filtering) | Set `exclude_kinds` for noise (e.g., `"comment"`, `"string"`) |
| Wrong columns highlighted | 5 (Grapheme splitting) | Grapheme position calculation bug |
| Panic on "index out of bounds" | 7/8 (Hunk assembly / Rendering) | Line index mismatch between entry positions and actual text |
| Extremely slow on large files | 5/6 (Graphemes / Myers) | Try `"split-graphemes": false` or check if diff is O(ND) worst-case |
