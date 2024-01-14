use crate::diff::{Hunk, Line, RichHunk, RichHunks};
use crate::render::{
    default_option, opt_color_def, ColorDef, DisplayData, EmphasizedStyle, RegularStyle, Renderer,
};
use anyhow::Result;
use console::{Color, Style, Term};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::{cmp::max, io::Write};

/// The ascii separator used after the diff title
const TITLE_SEPARATOR: &str = "=";

/// The ascii separator used after the hunk title
const HUNK_TITLE_SEPARATOR: &str = "-";

/// Something similar to the unified diff format.
///
/// NOTE: is a huge misnomer because this isn't really a unified diff.
///
/// The format is 'in-line', where differences from each document are displayed to the terminal in
/// lockstep.
// TODO(afnan): change this name
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Unified {
    pub addition: TextStyle,
    pub deletion: TextStyle,
}

/// Text style options for additions or deleetions.
///
/// This allows users to define text options like foreground, background colors, etc.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct TextStyle {
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

impl Default for Unified {
    fn default() -> Self {
        Unified {
            addition: TextStyle {
                regular_foreground: Color::Green,
                emphasized_foreground: Color::Green,
                highlight: None,
                bold: true,
                underline: false,
                prefix: "+ ".into(),
            },
            deletion: TextStyle {
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

/// The formatting directives to use with different types of text in a diff
struct FormattingDirectives<'a> {
    /// The formatting to use with normal unchanged text in a diff line
    pub regular: RegularStyle,
    /// The formatting to use with emphasized text in a diff line
    pub emphasis: EmphasizedStyle,
    /// The prefix (if any) to use with the line
    pub prefix: &'a dyn AsRef<str>,
}

/// The parameters required to display a diff for a particular document
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentDiffData<'a> {
    /// The filename of the document
    pub filename: &'a str,
    /// The full text of the document
    pub text: &'a str,
}

/// User supplied parameters that are required to display a diff
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayParameters<'a> {
    /// The hunks constituting the diff.
    pub hunks: RichHunks<'a>,
    /// The parameters that correspond to the old document
    pub old: DocumentDiffData<'a>,
    /// The parameters that correspond to the new document
    pub new: DocumentDiffData<'a>,
}

impl<'a> From<&'a TextStyle> for FormattingDirectives<'a> {
    fn from(fmt_opts: &'a TextStyle) -> Self {
        Self {
            regular: fmt_opts.into(),
            emphasis: fmt_opts.into(),
            prefix: &fmt_opts.prefix,
        }
    }
}

impl Renderer for Unified {
    fn render(
        &self,
        writer: &mut dyn Write,
        data: &DisplayData,
        term_info: Option<&Term>,
    ) -> Result<()> {
        let DisplayData { hunks, old, new } = &data;
        let old_fmt = FormattingDirectives::from(&self.deletion);
        let new_fmt = FormattingDirectives::from(&self.addition);

        // We need access to specific line numbers in the text so we can print out text ranges
        // within a line. It's more efficient to break up the text by line up-front so we don't
        // have to redo that when we print out each line/hunk.
        let old_lines: Vec<_> = old.text.lines().collect();
        let new_lines: Vec<_> = new.text.lines().collect();

        self.print_title(
            writer,
            old.filename,
            new.filename,
            &old_fmt,
            &new_fmt,
            term_info,
        )?;

        for hunk_wrapper in &hunks.0 {
            match hunk_wrapper {
                RichHunk::Old(hunk) => {
                    self.print_hunk(writer, &old_lines, hunk, &old_fmt)?;
                }
                RichHunk::New(hunk) => {
                    self.print_hunk(writer, &new_lines, hunk, &new_fmt)?;
                }
            }
        }
        Ok(())
    }
}

impl Unified {
    /// Print the title for the diff
    ///
    /// This will print the two files being compared. This will also attempt to modify the layout
    /// (stacking horizontally or vertically) based on the terminal width.
    fn print_title(
        &self,
        term: &mut dyn Write,
        old_fname: &str,
        new_fname: &str,
        old_fmt: &FormattingDirectives,
        new_fmt: &FormattingDirectives,
        term_info: Option<&Term>,
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
        let title_len = format!("{old_fname}{divider}{new_fname}").len();
        // Set terminal width equal to the title length if there is no terminal info is available, then the title will
        // stack horizontally be default
        let term_width = if let Some(term_info) = term_info {
            if let Some((_height, width)) = term_info.size_checked() {
                width.into()
            } else {
                title_len
            }
        } else {
            title_len
        };
        // We only display the horizontal title format if we know we have enough horizontal space
        // to display it. If we can't determine the terminal width, play it safe and default to
        // vertical stacking.
        let stack_style = if title_len <= term_width {
            TitleStack::Horizontal
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
        writeln!(term, "{styled_title_str}")?;
        writeln!(term, "{title_sep}")?;
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
            let line_index = line.line_index;
            // It's find for this to be fatal in debug builds. We want to avoid crashing in
            // release.
            debug_assert!(line_index < lines.len());
            if line_index >= lines.len() {
                error!(
                    "Received invalid line index {}. Skipping printing this line.",
                    line_index
                );
                continue;
            }
            let text = lines[line_index];
            debug!("Printing line {}", line_index);
            self.print_line(term, text, line, fmt)?;
            debug!("End line {}", line_index);
        }
        debug!(
            "End hunk (lines {} - {})",
            hunk.first_line().unwrap(),
            hunk.last_line().unwrap()
        );
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
            format!("\n{first_line}:")
        } else {
            format!("\n{first_line} - {last_line}:")
        };

        debug!("Title string has length of {}", title_str.len());

        // Note that we need to get rid of whitespace (including newlines) before we can take the
        // length of the string, which is why we call `trim()`
        let separator = HUNK_TITLE_SEPARATOR.repeat(title_str.trim().len());
        writeln!(term, "{}", fmt.regular.0.apply_to(title_str))?;
        writeln!(term, "{separator}")?;
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

impl From<&TextStyle> for RegularStyle {
    fn from(fmt: &TextStyle) -> Self {
        let mut style = Style::default();
        style = style.fg(fmt.regular_foreground);
        RegularStyle(style)
    }
}

impl From<&TextStyle> for EmphasizedStyle {
    fn from(fmt: &TextStyle) -> Self {
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
