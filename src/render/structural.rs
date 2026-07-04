//! A renderer for structural (tree diff) output: one annotated line per
//! node-level edit, with positions and enclosing context.

use std::io::Write;

use console::{Style, Term};
use serde::{Deserialize, Serialize};

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
            // The engine name is inferred from the payload variant: today
            // only the myers engine produces `Hunks` payloads.
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
                StructuralEditKind::Rename { old, new } => Style::new().yellow().apply_to(format!(
                    "~ {} {} `{}` -> `{}`",
                    old.kind,
                    position(old),
                    old.snippet,
                    new.snippet
                )),
                StructuralEditKind::Delete { node } => Style::new().red().apply_to(format!(
                    "- {} {} `{}`",
                    node.kind,
                    position(node),
                    node.snippet
                )),
                StructuralEditKind::Insert { node } => Style::new().green().apply_to(format!(
                    "+ {} {} `{}`",
                    node.kind,
                    position(node),
                    node.snippet
                )),
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
            end: Position {
                row,
                column: column + snippet.len(),
            },
        }
    }

    fn render_to_string(diff: StructuralDiff) -> String {
        let data = DisplayData {
            diff: DiffPayload::Structural(diff),
            old: DocumentDiffData {
                filename: "a.rs",
                text: "",
            },
            new: DocumentDiffData {
                filename: "b.rs",
                text: "",
            },
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
        let out = render_to_string(StructuralDiff {
            edits: vec![],
            distance: 0,
        });
        assert!(out.is_empty());
    }

    #[test]
    fn rejects_hunks_payload() {
        let data = DisplayData {
            diff: DiffPayload::Hunks(crate::diff::RichHunks(Vec::new())),
            old: DocumentDiffData {
                filename: "a.rs",
                text: "",
            },
            new: DocumentDiffData {
                filename: "b.rs",
                text: "",
            },
        };
        let mut buf = Vec::new();
        assert!(Structural::default().render(&mut buf, &data, None).is_err());
    }
}
