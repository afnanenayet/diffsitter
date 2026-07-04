use super::{DiffPayload, DisplayData};
use crate::render::Renderer;
use crate::tree_diff::TreeDiffError;
use console::Term;
use logging_timer::time;
use serde::{Deserialize, Serialize};
use std::io::Write;

/// A renderer that outputs json data about the diff.
///
/// This can be useful if you want to use `jq` or do some programatic analysis on the results.
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug, Default)]
pub struct Json {
    /// Whether to pretty print the output JSON.
    pub pretty_print: bool,
}

impl Renderer for Json {
    fn render(
        &self,
        writer: &mut dyn Write,
        data: &super::DisplayData,
        _term_info: Option<&Term>,
    ) -> anyhow::Result<()> {
        // The serialized `hunks` key describes the Myers schema; emitting a
        // structural payload under it would silently ship a misleading
        // document. A JSON schema for the tree diff engine is deliberate
        // follow-up work.
        if let DiffPayload::Structural(_) = &data.diff {
            return Err(TreeDiffError::RendererMismatch {
                engine: "topdiff".into(),
                renderer: "json".into(),
            }
            .into());
        }
        let json_str = self.generate_json_str(data)?;
        write!(writer, "{}", &json_str)?;
        Ok(())
    }
}

impl Json {
    /// Create a JSON string from the display data.
    ///
    /// This method handles display options that are set in the config.
    #[time("trace")]
    fn generate_json_str(&self, data: &DisplayData) -> Result<String, serde_json::Error> {
        if self.pretty_print {
            return serde_json::to_string_pretty(data);
        }
        serde_json::to_string(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::DocumentDiffData;
    use crate::tree_diff::StructuralDiff;

    #[test]
    fn rejects_structural_payload() {
        let data = DisplayData {
            diff: DiffPayload::Structural(StructuralDiff {
                edits: vec![],
                distance: 0,
            }),
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
        assert!(Json::default().render(&mut buf, &data, None).is_err());
        assert!(buf.is_empty());
    }
}
