//! Utilities related to displaying/formatting the edits computed as the difference between two
//! ASTs

use crate::diff::{Hunk, Hunks, Line};
use anyhow::Result;
use console::{Color, Style, Term};
use log::{debug, info};
use logging_timer::time;
use serde::{Deserialize, Serialize};
use std::{
    cmp::{max, Ordering},
    io::{BufWriter, Write},
};
use strum_macros::EnumString;

/// The ascii separator used after the diff title
const TITLE_SEPARATOR: &str = "=";

/// The ascii separator used after the hunk title
const HUNK_TITLE_SEPARATOR: &str = "-";

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

/// Formatting directives for text
///
/// This was abstracted out because the exact same settings apply for both additions and deletions
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct TextFormatting {
    /// The highlight/background color to use with emphasized text
    #[serde(with = "opt_color_def", default = "default_option")]
    pub highlight: Option<Color>,
    /// The foreground color to use with un-emphasized text
    #[serde(with = "ColorDef")]
    pub regular_foreground: Color,
    /// The foreground color to use with emphasized text
    #[serde(with = "ColorDef")]
    pub emphasized_foreground: Color,
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

/// The style that applies to regular text in a diff
#[derive(Clone, Debug, PartialEq, Eq)]
struct RegularStyle(Style);

/// The style that applies to emphasized text in a diff
#[derive(Clone, Debug, PartialEq, Eq)]
struct EmphasizedStyle(Style);

impl From<&TextFormatting> for RegularStyle {
    fn from(fmt: &TextFormatting) -> Self {
        let mut style = Style::default();
        style = style.fg(fmt.regular_foreground);
        RegularStyle(style)
    }
}

impl From<&TextFormatting> for EmphasizedStyle {
    fn from(fmt: &TextFormatting) -> Self {
        let mut style = Style::default();
        style = style.fg(fmt.emphasized_foreground);

        if fmt.bold {
            style = style.bold();
        }

        if fmt.underline {
            style = style.underlined();
        }

        if let Some(color) = fmt.highlight {
            style = style.bg(color);
        }
        EmphasizedStyle(style)
    }
}

/// A writer that can render a diff to a terminal
///
/// This struct contains the formatting options for the diff
#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
#[serde(default, rename_all = "kebab-case")]
pub struct DiffWriter {
    /// The formatting options to use with text addition
    pub addition: TextFormatting,
    /// The formatting options to use with text addition
    pub deletion: TextFormatting,
}

impl Default for DiffWriter {
    fn default() -> Self {
        DiffWriter {
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

/// User supplied parameters that are required to display a diff
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayParameters<'a> {
    /// The parameters that correspond to the old document
    pub old: DocumentDiffData<'a>,
    /// The parameters that correspond to the new document
    pub new: DocumentDiffData<'a>,
}

/// The parameters required to display a diff for a particular document
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentDiffData<'a> {
    /// The filename of the document
    pub filename: &'a str,
    /// The edit hunks for the document
    pub hunks: &'a Hunks<'a>,
    /// The full text of the document
    pub text: &'a str,
}

/// The formatting directives to use with different types of text in a diff
struct FormattingDirectives<'a> {
    /// The formatting to use with normal unchanged text in a diff line
    pub regular: RegularStyle,
    /// The formatting to use with emphasized text in a diff line
    pub emphasis: EmphasizedStyle,
    /// The prefix (if any) to use with the line
    pub prefix: &'a dyn AsRef<str>,
}

impl<'a> From<&'a TextFormatting> for FormattingDirectives<'a> {
    fn from(fmt_opts: &'a TextFormatting) -> Self {
        Self {
            regular: fmt_opts.into(),
            emphasis: fmt_opts.into(),
            prefix: &fmt_opts.prefix,
        }
    }
}

impl DiffWriter {
    /// A helper function for printing a line-by-line diff
    ///
    /// This will process the "raw" [diff vector](AstVector) and turn extract the differences
    /// between lines.
    #[time("info", "formatting::{}")]
    pub fn print(&self, term: &mut BufWriter<Term>, params: &DisplayParameters) -> Result<()> {
        let DisplayParameters { old, new } = &params;
        let old_fmt = FormattingDirectives::from(&self.deletion);
        let new_fmt = FormattingDirectives::from(&self.addition);

        // We need access to specific line numbers in the text so we can print out text ranges
        // within a line. It's more efficient to break up the text by line up-front so we don't
        // have to redo that when we print out each line/hunk.
        let old_lines: Vec<_> = old.text.lines().collect();
        let new_lines: Vec<_> = new.text.lines().collect();

        self.print_title(term, old.filename, new.filename, &old_fmt, &new_fmt)?;

        // Iterate through the edits on both documents. We know that both of the vectors are
        // sorted, and we can use that property to iterate through the entries in O(n).
        let mut it_old = 0;
        let mut it_new = 0;

        while it_old < old.hunks.0.len() && it_new < new.hunks.0.len() {
            let old_hunk = &old.hunks.0[it_old];
            let new_hunk = &new.hunks.0[it_new];

            // We can unwrap here because the loop invariant enforces that there is at least one
            // element in the deque, otherwise the loop wouldn't run at all.
            let old_line_num = old_hunk.first_line().unwrap();
            let new_line_num = new_hunk.first_line().unwrap();

            match old_line_num.cmp(&new_line_num) {
                Ordering::Equal => {
                    self.print_hunk(term, &old_lines, old_hunk, &old_fmt)?;
                    self.print_hunk(term, &new_lines, new_hunk, &new_fmt)?;
                    it_old += 1;
                    it_new += 1;
                }
                Ordering::Less => {
                    self.print_hunk(term, &old_lines, old_hunk, &old_fmt)?;
                    it_old += 1;
                }
                Ordering::Greater => {
                    self.print_hunk(term, &new_lines, new_hunk, &new_fmt)?;
                    it_new += 1;
                }
            };
        }

        debug!("Printing remaining old hunks");

        while it_old < old.hunks.0.len() {
            let hunk = &old.hunks.0[it_old];
            self.print_hunk(term, &old_lines, hunk, &old_fmt)?;
            it_old += 1;
        }

        debug!("Printing remaining new hunks");

        while it_new < new.hunks.0.len() {
            let hunk = &new.hunks.0[it_new];
            self.print_hunk(term, &new_lines, hunk, &new_fmt)?;
            it_new += 1;
        }
        Ok(())
    }

    /// Print the title for the diff
    ///
    /// This will print the two files being compared. This will also attempt to modify the layout
    /// (stacking horizontally or vertically) based on the terminal width.
    fn print_title(
        &self,
        term: &mut BufWriter<Term>,
        old_fname: &str,
        new_fname: &str,
        old_fmt: &FormattingDirectives,
        new_fmt: &FormattingDirectives,
    ) -> std::io::Result<()> {
        // The different ways we can stack the title
        #[derive(Debug, Eq, PartialEq, PartialOrd, Ord, strum_macros::Display)]
        #[strum(serialize_all = "snake_case")]
        enum TitleStack {
            Vertical,
            Horizontal,
        }
        let divider = " -> ";

        // We construct the fully horizontal title string. If wider than the terminal, then we
        // format another title string that's vertically stacked
        let title_len = format!("{}{}{}", old_fname, divider, new_fname).len();

        // We only display the horizontal title format if we know we have enough horizontal space
        // to display it. If we can't determine the terminal width, play it safe and default to
        // vertical stacking.
        let stack_style = if let Some((_, term_width)) = term.get_ref().size_checked() {
            info!("Detected terminal width: {} columns", term_width);

            if title_len <= term_width as usize {
                TitleStack::Horizontal
            } else {
                TitleStack::Vertical
            }
        } else {
            TitleStack::Vertical
        };

        info!("Using stack style {} for title", stack_style);

        // Generate a title string and separator based on the stacking style we determined from
        // the terminal width
        let (styled_title_str, title_sep) = match stack_style {
            TitleStack::Horizontal => {
                let title_len = old_fname.len() + divider.len() + new_fname.len();
                let styled_title_str = format!(
                    "{}{}{}",
                    old_fmt.regular.0.apply_to(old_fname),
                    divider,
                    new_fmt.regular.0.apply_to(new_fname)
                );
                let title_sep = TITLE_SEPARATOR.repeat(title_len);
                (styled_title_str, title_sep)
            }
            TitleStack::Vertical => {
                let title_len = max(old_fname.len(), new_fname.len());
                let styled_title_str = format!(
                    "{}\n{}",
                    old_fmt.regular.0.apply_to(old_fname),
                    new_fmt.regular.0.apply_to(new_fname)
                );
                let title_sep = TITLE_SEPARATOR.repeat(title_len);
                (styled_title_str, title_sep)
            }
        };
        writeln!(term, "{}", styled_title_str)?;
        writeln!(term, "{}", title_sep)?;
        Ok(())
    }

    /// Print the title of a hunk to stdout
    ///
    /// This will print the line numbers that correspond to the hunk using the color directive for
    /// that file, so the user has some context for the text that's being displayed.
    fn print_hunk_title(
        &self,
        term: &mut dyn Write,
        hunk: &Hunk,
        fmt: &FormattingDirectives,
    ) -> Result<()> {
        let first_line = hunk.first_line().unwrap();
        let last_line = hunk.last_line().unwrap();

        // We don't need to display a range `x - x:` since `x:` is terser and clearer
        let title_str = if last_line - first_line == 0 {
            format!("\n{}:", first_line)
        } else {
            format!("\n{} - {}:", first_line, last_line)
        };

        debug!("Title string has length of {}", title_str.len());

        // Note that we need to get rid of whitespace (including newlines) before we can take the
        // length of the string, which is why we call `trim()`
        let separator = HUNK_TITLE_SEPARATOR.repeat(title_str.trim().len());
        writeln!(term, "{}", fmt.regular.0.apply_to(title_str))?;
        writeln!(term, "{}", separator)?;
        Ok(())
    }

    /// Print a [hunk](Hunk) to `stdout`
    fn print_hunk(
        &self,
        term: &mut dyn Write,
        lines: &[&str],
        hunk: &Hunk,
        fmt: &FormattingDirectives,
    ) -> Result<()> {
        debug!(
            "Printing hunk (lines {} - {})",
            hunk.first_line().unwrap(),
            hunk.last_line().unwrap()
        );
        self.print_hunk_title(term, hunk, fmt)?;

        for line in &hunk.0 {
            let text = lines[line.line_index];
            debug!("Printing line {}", line.line_index);
            self.print_line(term, text, line, fmt)?;
            debug!("End line {}", line.line_index);
        }
        debug!(
            "End hunk (lines {} - {})",
            hunk.first_line().unwrap(),
            hunk.last_line().unwrap()
        );
        Ok(())
    }

    /// Print a line with edits
    ///
    /// This is a generic helper method for additions and deletions, since the logic is very
    /// similar, they just use different styles.
    ///
    /// `text` refers to the text that corresponds line number of the given [line](Line).
    fn print_line(
        &self,
        term: &mut dyn Write,
        text: &str,
        line: &Line,
        fmt: &FormattingDirectives,
    ) -> Result<()> {
        let regular = &fmt.regular.0;
        let emphasis = &fmt.emphasis.0;

        // First, we print the prefix to stdout
        write!(term, "{}", regular.apply_to(fmt.prefix.as_ref()))?;

        // The number of characters that have been printed out to stdout already. All indices are
        // in raw byte offsets, as splitting on graphemes, etc was taken care of when processing
        // the AST nodes.
        let mut printed_chars = 0;

        // We keep printing ranges until we've covered the entire line
        for entry in &line.entries {
            // The range of text to emphasize
            // TODO(afnan) deal with ranges spanning multiple rows
            let emphasis_range = entry.start_position().column..entry.end_position().column;

            // First we need to see if there's any regular text to cover. If the range has a len of
            // zero this is a no-op
            let regular_range = printed_chars..emphasis_range.start;
            let regular_text: String = text[regular_range].into();
            write!(term, "{}", regular.apply_to(&regular_text))?;

            // Need to set the printed_chars marker here because emphasized_text moves the range
            printed_chars = emphasis_range.end;
            let emphasized_text: String = text[emphasis_range].into();
            write!(term, "{}", emphasis.apply_to(emphasized_text))?;
        }
        // Finally, print any normal text after the last entry
        let remaining_range = printed_chars..text.len();
        let remaining_text: String = text[remaining_range].into();
        writeln!(term, "{}", regular.apply_to(remaining_text))?;
        Ok(())
    }
}

/// The formatting directives to use with emphasized text in the line of a diff
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

/// The colors to use when highlighting additions and deletions
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

/// Workaround so we can use the `ColorDef` remote serialization mechanism with optional types
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
