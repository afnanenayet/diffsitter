//! Utilities and modules related to rendering diff outputs.
//!
//! We have a modular system for displaying diff data to the terminal. Using this system makes it
//! much easier to extend with new formats that people may request.
//!
//! This library defines a fairly minimal interface for renderers: a single trait called
//! `Renderer`. From there implementers are free to do whatever they want with the diff data.
//!
//! This module also defines utilities that may be useful for `Renderer` implementations.

mod delta;
mod json;
mod unified;

use self::delta::Delta;
use self::json::Json;
use crate::diff::RichHunks;
use anyhow::anyhow;
use console::{Color, Style, Term};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use std::io::Write;
use strum::{self, Display, EnumIter, EnumString};
use unified::Unified;

/// The parameters required to display a diff for a particular document
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentDiffData<'a> {
    /// The filename of the document
    pub filename: &'a str,
    /// The full text of the document
    pub text: &'a str,
}

/// The parameters a [Renderer] instance receives to render a diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DisplayData<'a> {
    /// The hunks constituting the diff.
    pub hunks: RichHunks<'a>,
    /// The parameters that correspond to the old document
    pub old: DocumentDiffData<'a>,
    /// The parameters that correspond to the new document
    pub new: DocumentDiffData<'a>,
}

#[enum_dispatch]
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, Display, EnumIter, EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Renderers {
    Unified,
    Json,
    Delta,
}

impl Default for Renderers {
    fn default() -> Self {
        Renderers::Unified(Unified::default())
    }
}

/// An interface that renders given diff data.
#[enum_dispatch(Renderers)]
pub trait Renderer {
    /// Render a diff.
    ///
    /// We use anyhow for errors so errors are free form for implementors, as they are not
    /// recoverable.
    ///
    /// `writer` can be any generic writer - it's not guaranteed that we're writing to a particular sink (could be a
    /// pager, stdout, etc). `data` is the data that the renderer needs to display, this has information about the
    /// document being written out. `term_info` is an optional reference to a term object that can be used by the
    /// renderer to access information about the terminal if the current process is a TTY output.
    fn render(
        &self,
        writer: &mut dyn Write,
        data: &DisplayData,
        term_info: Option<&Term>,
    ) -> anyhow::Result<()>;
}

/// A copy of the [Color](console::Color) enum so we can serialize using serde, and get around the
/// orphan rule.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[serde(remote = "Color", rename_all = "snake_case")]
#[derive(Default)]
enum ColorDef {
    Color256(u8),
    #[default]
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    TrueColor(u8, u8, u8),
}

impl From<ColorDef> for Color {
    fn from(c: ColorDef) -> Self {
        match c {
            ColorDef::Black => Color::Black,
            ColorDef::White => Color::White,
            ColorDef::Red => Color::Red,
            ColorDef::Green => Color::Green,
            ColorDef::Yellow => Color::Yellow,
            ColorDef::Blue => Color::Blue,
            ColorDef::Magenta => Color::Magenta,
            ColorDef::Cyan => Color::Cyan,
            ColorDef::Color256(c) => Color::Color256(c),
            ColorDef::TrueColor(r, g, b) => Color::TrueColor(r, g, b),
        }
    }
}

/// Workaround so we can use the `ColorDef` remote serialization mechanism with optional types
mod opt_color_def {
    use super::{Color, ColorDef};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S>(value: &Option<Color>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Helper<'a>(#[serde(with = "ColorDef")] &'a Color);

        value.as_ref().map(Helper).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Color>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper(#[serde(with = "ColorDef")] Color);

        let helper = Option::deserialize(deserializer)?;
        Ok(helper.map(|Helper(external)| external))
    }
}

/// A helper function for the serde serializer
///
/// Due to the shenanigans we're using to serialize the optional color, we need to supply this
/// method so serde can infer a default value for an option when its key is missing.
fn default_option<T>() -> Option<T> {
    None
}

/// The style that applies to regular text in a diff
#[derive(Clone, Debug, PartialEq, Eq)]
struct RegularStyle(Style);

/// The style that applies to emphasized text in a diff
#[derive(Clone, Debug, PartialEq, Eq)]
struct EmphasizedStyle(Style);

/// The formatting directives to use with emphasized text in the line of a diff
///
/// `Bold` is used as the default emphasis strategy between two lines.
#[derive(Debug, PartialEq, EnumString, Serialize, Deserialize, Eq)]
#[strum(serialize_all = "snake_case")]
#[derive(Default)]
pub enum Emphasis {
    /// Don't emphasize anything
    ///
    /// This field exists because the absence of a value implies that the user wants to use the
    /// default emphasis strategy.
    None,
    /// Bold the differences between the two lines for emphasis
    #[default]
    Bold,
    /// Underline the differences between two lines for emphasis
    Underline,
    /// Use a colored highlight for emphasis
    Highlight(HighlightColors),
}

/// The colors to use when highlighting additions and deletions
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HighlightColors {
    /// The background color to use with an addition
    #[serde(with = "ColorDef")]
    pub addition: Color,
    /// The background color to use with a deletion
    #[serde(with = "ColorDef")]
    pub deletion: Color,
}

impl Default for HighlightColors {
    fn default() -> Self {
        HighlightColors {
            addition: Color::Color256(0),
            deletion: Color::Color256(0),
        }
    }
}

/// Configurations and templates for different configuration aliases
///
/// The user can define settings for each renderer as well as custom tags for different renderer
/// configurations.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case", default)]
pub struct RenderConfig {
    /// The default diff renderer to use.
    ///
    /// This is used if no renderer is specified at the command line.
    default: String,

    unified: unified::Unified,
    json: json::Json,
    delta: delta::Delta,
}

impl Default for RenderConfig {
    fn default() -> Self {
        let default_renderer = Renderers::default();
        RenderConfig {
            default: default_renderer.to_string(),
            unified: Unified::default(),
            json: Json::default(),
            delta: Delta::default(),
        }
    }
}

impl RenderConfig {
    /// Get the renderer specified by the given tag.
    ///
    /// If the tag is not specified this will fall back to the default renderer. This is a
    /// relatively expensive operation so it should be used once and the result should be saved.
    pub fn get_renderer(self, tag: Option<String>) -> anyhow::Result<Renderers> {
        let tag = tag.unwrap_or_else(|| self.default.clone());

        // Match the tag to the configured renderer, using the config values
        match tag.as_str() {
            "unified" => Ok(Renderers::Unified(self.unified)),
            "json" => Ok(Renderers::Json(self.json)),
            "delta" => Ok(Renderers::Delta(self.delta)),
            _ => Err(anyhow!("'{}' is not a valid renderer", &tag)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("unified")]
    #[test_case("json")]
    #[test_case("delta")]
    fn test_get_renderer_custom_tag(tag: &str) {
        let cfg = RenderConfig::default();
        let res = cfg.get_renderer(Some(tag.into()));
        assert!(res.is_ok());
    }

    #[test]
    fn test_render_config_default_tag() {
        let cfg = RenderConfig::default();
        let res = cfg.get_renderer(None);
        assert_eq!(res.unwrap(), Renderers::default());
    }
}
