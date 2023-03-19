use super::DisplayData;
use crate::render::Renderer;
use console::Term;
use logging_timer::time;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

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
