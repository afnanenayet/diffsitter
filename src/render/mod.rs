//! Utilities and modules related to rendering diff outputs.
//!
//! We have a modular system for displaying diff data to the terminal. Using this system makes it
//! much easier to extend with new formats that people may request.
//!
//! This library defines a fairly minimal interface for renderers: a single trait called
//! `Renderer`. From there implementers are free to do whatever they want with the diff data.
//!
//! This module also defines utilities that may be useful for `Renderer` implementations.

mod unified;

use crate::diff::RichHunks;
use console::Term;
use console::{Color, Style};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::BufWriter;
use strum::{self, Display, EnumIter, EnumString};
use unified::Unified;

/// The parameters required to display a diff for a particular document
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentDiffData<'a> {
    /// The filename of the document
    pub filename: &'a str,
    /// The full text of the document
    pub text: &'a str,
}

/// The parameters a [Renderer] instance receives to render a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayData<'a> {
    /// The hunks constituting the diff.
    pub hunks: RichHunks<'a>,
    /// The parameters that correspond to the old document
    pub old: DocumentDiffData<'a>,
    /// The parameters that correspond to the new document
    pub new: DocumentDiffData<'a>,
}

/// A buffered writer for a [terminal](Term) object.
type TermWriter = BufWriter<Term>;

#[enum_dispatch]
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, Display, EnumIter, EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Renderers {
    Unified,
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
    fn render(&self, writer: &mut TermWriter, data: &DisplayData) -> anyhow::Result<()>;
}

/// A copy of the [Color](console::Color) enum so we can serialize using serde, and get around the
/// orphan rule.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[serde(remote = "Color", rename_all = "snake_case")]
enum ColorDef {
    Color256(u8),
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
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
        }
    }
}

impl Default for ColorDef {
    fn default() -> Self {
        ColorDef::Black
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
pub enum Emphasis {
    /// Don't emphasize anything
    ///
    /// This field exists because the absence of a value implies that the user wants to use the
    /// default emphasis strategy.
    None,
    /// Bold the differences between the two lines for emphasis
    Bold,
    /// Underline the differences between two lines for emphasis
    Underline,
    /// Use a colored highlight for emphasis
    Highlight(HighlightColors),
}

impl Default for Emphasis {
    fn default() -> Self {
        Emphasis::Bold
    }
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

    /// A mapping of tags to custom rendering configurations.
    ///
    /// These names *must* be distinct from the renderer names, otherwise the keys will conflict
    /// with the configs set for each renderer in this config section.
    custom: HashMap<String, Renderers>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        let default_renderer = Renderers::default();
        RenderConfig {
            default: default_renderer.to_string(),
            unified: Unified::default(),
            custom: HashMap::default(),
        }
    }
}

impl RenderConfig {
    /// Verify that the custom user supplied keys don't conflict with built in types.
    fn check_custom_render_keys(&self) -> anyhow::Result<()> {
        let custom_map = &self.custom;
        let render_iter = <Renderers as strum::IntoEnumIterator>::iter();
        let conflicting_keys: Vec<String> = render_iter
            .filter_map(|key| {
                let key_str = key.to_string();
                if custom_map.contains_key(&key_str) {
                    Some(key_str)
                } else {
                    None
                }
            })
            .collect();
        let error_string = conflicting_keys.join(", ");
        anyhow::ensure!(
            conflicting_keys.is_empty(),
            "Received invalid keys {}",
            error_string
        );
        Ok(())
    }

    /// Get the renderer specified by the given tag.
    ///
    /// If the tag is not specified this will fall back to the default renderer. This is a
    /// relatively expensive operation so it should be used once and the result should be saved.
    pub fn get_renderer(self, tag: Option<String>) -> anyhow::Result<Renderers> {
        self.check_custom_render_keys()?;
        let final_tag = if let Some(t) = tag { t } else { self.default };
        let mut render_map = self.custom;

        // TODO(afnan): automate this with a proc macro so we don't have to
        // manually sync each renderer engine by hand.
        render_map.insert("unified".into(), Renderers::from(self.unified));

        if let Some(renderer) = render_map.remove(&final_tag) {
            Ok(renderer)
        } else {
            Err(anyhow::anyhow!(
                "Specified tag {} not found in renderers",
                final_tag
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_render_keys() {
        let cfg = RenderConfig::default();
        assert!(cfg.check_custom_render_keys().is_ok());
    }

    #[test]
    fn test_custom_renderer_tags_collision() {
        let custom_map: HashMap<String, Renderers> = HashMap::from([(
            "unified".to_string(),
            Renderers::Unified(Unified::default()),
        )]);
        let cfg = RenderConfig {
            custom: custom_map,
            ..Default::default()
        };
        assert!(cfg.check_custom_render_keys().is_err());
    }
}
