//! Utilities related to displaying/formatting the edits computed as the difference between two
//! ASTs

use crate::ast::{Edit, Entry};
use anyhow::Result;
use console::{Color, Style, Term};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::VecDeque};
use strum_macros::EnumString;

/// A copy of the [Color](console::Color) enum so we can serialize using serde, and get around the
/// orphan rule.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[serde(remote = "Color", rename_all = "snake_case")]
enum ColorDef {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Color256(u8),
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
            ColorDef::Cyan => Color::White,
            ColorDef::Color256(c) => Color::Color256(c),
        }
    }
}

impl Default for ColorDef {
    fn default() -> Self {
        ColorDef::Black
    }
}

/// Formatting directives for text
///
/// This was abstracted out because the exact same settings apply for both additions and deletions
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct TextFormatting {
    /// The foreground color to use with un-emphasized text
    #[serde(with = "ColorDef")]
    pub regular_foreground: Color,
    /// The foreground color to use with emphasized text
    #[serde(with = "ColorDef")]
    pub emphasized_foreground: Color,
    /// The highlight/background color to use with emphasized text
    #[serde(with = "opt_color_def", default = "default_option")]
    pub highlight: Option<Color>,
    /// Whether to bold emphasized text
    pub bold: bool,
    /// Whether to underline emphasized text
    pub underline: bool,
    /// The prefix to use with the line
    pub prefix: String,
}

/// A helper function for the serde serializer
///
/// Due to the shenanigans we're using to serialize the optional color, we need to supply this
/// method so serde can infer a default value for an option when its key is missing.
fn default_option<T>() -> Option<T> {
    None
}

impl TextFormatting {
    /// Generate a [Style] for regular text
    pub fn to_regular_style(&self) -> Style {
        let mut style = Style::default();
        style = style.fg(self.regular_foreground);
        style
    }

    /// Generate a [Style] for emphasized text
    pub fn to_emphasized_style(&self) -> Style {
        let mut style = Style::default();
        style = style.fg(self.emphasized_foreground);

        if self.bold {
            style = style.bold();
        }

        if self.underline {
            style = style.underlined();
        }

        if let Some(color) = self.highlight {
            style = style.bg(color);
        }
        style
    }
}

/// Formatting options for rendering a diff
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// The formatting options to use with text addition
    pub addition: TextFormatting,
    /// The formatting options to use with text addition
    pub deletion: TextFormatting,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            addition: TextFormatting {
                regular_foreground: Color::Green,
                emphasized_foreground: Color::Green,
                highlight: None,
                bold: true,
                underline: false,
                prefix: "+ ".into(),
            },
            deletion: TextFormatting {
                regular_foreground: Color::Red,
                emphasized_foreground: Color::Red,
                highlight: None,
                bold: true,
                underline: false,
                prefix: "- ".into(),
            },
        }
    }
}

/// The edit information representing a line
#[derive(Debug)]
struct Line<'a, 'b> {
    /// The index of the line in the original document
    pub line_index: usize,
    /// The entries corresponding to the line
    pub entries: Vec<&'a Entry<'b>>,
}

/// User supplied parameters that are required to display a diff
#[derive(Debug)]
pub struct DisplayParameters<'text> {
    /// A vector of the edit nodes
    pub diff: &'text VecDeque<Edit<'text>>,
    /// The full text of the old document
    pub old_text: &'text str,
    /// The full text of the new document
    pub new_text: &'text str,
}

/// Preprocessed information indicating how AST edit nodes create a line
struct LineInfo<'a> {
    /// The nodes, grouped by line, for a document
    pub node_groups: Vec<Line<'a, 'a>>,
    /// The (text) lines in the document
    pub lines: Vec<&'a str>,
}

/// The formatting directives to use with different types of text
///
/// This struct defines the different types of text to use with `print_line`.
struct FormattingDirectives<'a> {
    /// The formatting to use with normal unchanged text in a diff line
    pub regular: &'a Style,
    /// The formatting to use with emphasized text in a diff line
    pub emphasis: &'a Style,
    /// The prefix (if any) to use with the line
    pub prefix: &'a str,
}

impl Options {
    /// A helper function for printing a line-by-line diff
    ///
    /// This will process the "raw" [diff vector](AstVector) and turn extract the differences
    /// between lines.
    // TODO(afnan) make this function private and use `print` to dispatch
    pub fn line_by_line(&self, term: &mut Term, params: &DisplayParameters) -> Result<()> {
        let &DisplayParameters {
            diff,
            old_text,
            new_text,
        } = params;
        let (old_print_record, new_print_record) = generate_lines(diff, old_text, new_text);
        let deletion_fmt = FormattingDirectives {
            regular: &self.deletion.to_regular_style(),
            emphasis: &self.deletion.to_emphasized_style(),
            prefix: &self.deletion.prefix,
        };

        let addition_fmt = FormattingDirectives {
            regular: &self.addition.to_regular_style(),
            emphasis: &self.addition.to_emphasized_style(),
            prefix: &self.addition.prefix,
        };
        // Iterate through the edits on both documents. We know that both of the vectors are
        // sorted, and we can use that property to iterate through the entries in O(n). Basic
        // leetcode woo!
        let mut it_old = 0;
        let mut it_new = 0;
        let new_node_group = &new_print_record.node_groups;
        let old_node_group = &old_print_record.node_groups;

        while it_old < old_node_group.len() && it_new < new_node_group.len() {
            let old_line_num = old_node_group[it_old].line_index;
            let new_line_num = new_node_group[it_new].line_index;

            match old_line_num.cmp(&new_line_num) {
                Ordering::Equal => {
                    self.print_line(term, it_new, &old_print_record, &deletion_fmt)?;
                    self.print_line(term, it_old, &new_print_record, &addition_fmt)?;
                    it_old += 1;
                    it_new += 1;
                }
                Ordering::Less => {
                    self.print_line(term, it_old, &old_print_record, &deletion_fmt)?;
                    it_old += 1;
                }
                Ordering::Greater => {
                    self.print_line(term, it_new, &new_print_record, &addition_fmt)?;
                    it_new += 1;
                }
            };
        }

        while it_old < old_node_group.len() {
            self.print_line(term, it_old, &old_print_record, &deletion_fmt)?;
            it_old += 1;
        }

        while it_new < new_node_group.len() {
            self.print_line(term, it_new, &new_print_record, &addition_fmt)?;
            it_new += 1;
        }
        Ok(())
    }

    /// Print a line with edits
    ///
    /// This is a generic helper method for additions and deletions, since the logic is very
    /// similar, they just use different styles.
    fn print_line(
        &self,
        term: &mut Term,
        group_idx: usize,
        line_info: &LineInfo,
        fmt: &FormattingDirectives,
    ) -> Result<()> {
        // First, we print the prefix to stdout
        term.write_str(&fmt.regular.apply_to(fmt.prefix).to_string())?;

        let line = &line_info.node_groups[group_idx];
        let text_line_idx = line.line_index;
        let text = line_info.lines[text_line_idx];

        // The number of characters that have been printed out to stdout already. These aren't
        // *actually* chars because UTF-8, but you get the gist.
        let mut printed_chars = 0;

        // We keep printing ranges until we've covered the entire line
        for entry in &line.entries {
            // The range of text to emphasize
            // TODO(afnan) deal with ranges spanning multiple rows
            let emphasis_range =
                entry.reference.start_position().column..entry.reference.end_position().column;

            // First we need to see if there's any regular text to cover. If the range has a len of
            // zero this is a no-op
            let regular_range = printed_chars..emphasis_range.start;
            let regular_text: String = text[regular_range].into();
            term.write_str(&fmt.regular.apply_to(regular_text).to_string())?;

            // Need to set the printed_chars marker here because emphasized_text moves the range
            printed_chars = emphasis_range.end;
            let emphasized_text: String = text[emphasis_range].into();
            term.write_str(&fmt.emphasis.apply_to(emphasized_text).to_string())?;
        }
        // Finally, print any normal text after the last entry
        let remaining_range = printed_chars..text.len();
        let remaining_text: String = text[remaining_range].into();
        term.write_str(&fmt.regular.apply_to(remaining_text).to_string())?;
        term.write_str("\n")?;
        Ok(())
    }
}

/// A helper method to return the latest line in the vector or create one if necessary
///
/// Given a reference to the current latest line, and the current entry, this method will check if
/// the latest line in the diff has the same line index as the current entry, and if not, create a
/// new [Line].
fn upsert_latest_line<'vec, 'entry_ptr, 'text>(
    lines: &'vec mut Vec<Line<'entry_ptr, 'text>>,
    entry: &Entry,
) -> &'vec mut Line<'entry_ptr, 'text> {
    let line_index = entry.reference.start_position().row;

    if lines.is_empty() {
        lines.push(Line {
            line_index,
            entries: Vec::new(),
        });
    } else {
        // We already know that lines isn't empty
        let last_entry = lines.last().unwrap();

        if last_entry.line_index != line_index {
            lines.push(Line {
                line_index,
                entries: Vec::new(),
            });
        }
    }
    lines.last_mut().unwrap()
}

/// The method to use when emphasizing diffs within a line
///
/// `Bold` is used as the default emphasis strategy between two lines.
#[derive(Debug, PartialEq, EnumString, Serialize, Deserialize)]
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

/// Specify the colors to use when highlighting differences
// TODO(afnan) implement the proper defaults for this struct
#[derive(Debug, PartialEq, Serialize, Deserialize)]
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

fn generate_lines<'a>(
    diff: &'a VecDeque<Edit<'a>>,
    old_text: &'a str,
    new_text: &'a str,
) -> (LineInfo<'a>, LineInfo<'a>) {
    // The nodes that correspond to a line in the old AST
    let mut lines_old: Vec<Line> = Vec::new();
    // The nodes that correspond to a line in the new AST
    let mut lines_new: Vec<Line> = Vec::new();
    // Get the lines from the original text of each document
    let old_text_lines: Vec<&str> = old_text.lines().collect();
    let new_text_lines: Vec<&str> = new_text.lines().collect();

    // Iterate through the entries in the `diff` vector so we can construct the lines in old
    // and new
    for edit in diff {
        match edit {
            Edit::Addition(entry) => {
                let current_line = upsert_latest_line(&mut lines_new, &entry);
                current_line.entries.push(&entry);
            }
            Edit::Deletion(entry) => {
                let current_line = upsert_latest_line(&mut lines_old, &entry);
                current_line.entries.push(&entry);
            }
            _ => (),
        }
    }
    let old_print_record = LineInfo {
        node_groups: lines_old,
        lines: old_text_lines,
    };
    let new_print_record = LineInfo {
        node_groups: lines_new,
        lines: new_text_lines,
    };
    (old_print_record, new_print_record)
}

// Workaround so we can use the `ColorDef` remote serialization mechanism with optional types
mod opt_color_def {
    use super::{Color, ColorDef};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
