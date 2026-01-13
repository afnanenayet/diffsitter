//! A delta-style diff renderer.
//!
//! This renderer produces output similar to the delta diff tool (https://github.com/dandavison/delta),
//! featuring:
//! - File headers with decorations
//! - Line numbers in the margin
//! - Color-coded additions and deletions with within-line emphasis
//! - Box-drawing characters for visual structure

use crate::diff::{Hunk, Line, RichHunk};
use crate::render::{ColorDef, DisplayData, Renderer, default_option, opt_color_def};
use anyhow::Result;
use console::{Color, Style, Term};
use log::debug;
use serde::{Deserialize, Serialize};
use std::cmp::max;
use std::io::Write;

/// Box-drawing characters for delta-style output.
const LINE_NUMBER_SEPARATOR: &str = "│";
const HORIZONTAL_LINE: char = '─';
const HEADER_LEFT: &str = "───";

/// Default line number width for padding.
const DEFAULT_LINE_NUMBER_WIDTH: usize = 4;

/// A delta-style diff renderer.
///
/// Produces output similar to the popular delta diff tool with syntax-aware
/// highlighting and line numbers.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Delta {
    /// Styling for additions (new file content).
    pub addition: DeltaTextStyle,
    /// Styling for deletions (old file content).
    pub deletion: DeltaTextStyle,
    /// Whether to show line numbers in the output.
    pub line_numbers: bool,
    /// Width to use for line number columns (minimum padding).
    pub line_number_width: usize,
    /// Whether to show file headers with decorations.
    pub show_header: bool,
    /// Whether to use a dark theme (affects default colors).
    pub dark_theme: bool,
}

/// Text style configuration for delta-style output.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct DeltaTextStyle {
    /// Foreground color for regular (non-emphasized) text.
    #[serde(with = "ColorDef")]
    pub foreground: Color,
    /// Background color for the entire line.
    #[serde(with = "opt_color_def", default = "default_option")]
    pub line_background: Option<Color>,
    /// Background color for emphasized (changed) portions.
    #[serde(with = "opt_color_def", default = "default_option")]
    pub emphasis_background: Option<Color>,
    /// Foreground color for emphasized text.
    #[serde(with = "ColorDef")]
    pub emphasis_foreground: Color,
    /// Whether to bold emphasized text.
    pub bold: bool,
    /// The prefix character to use (e.g., "+" for additions, "-" for deletions).
    pub prefix: String,
}

impl Default for Delta {
    fn default() -> Self {
        Delta {
            addition: DeltaTextStyle {
                foreground: Color::Green,
                line_background: Some(Color::Color256(22)), // Dark green background
                emphasis_background: Some(Color::Color256(28)), // Brighter green for emphasis
                emphasis_foreground: Color::White,
                bold: true,
                prefix: "+".into(),
            },
            deletion: DeltaTextStyle {
                foreground: Color::Red,
                line_background: Some(Color::Color256(52)), // Dark red background
                emphasis_background: Some(Color::Color256(88)), // Brighter red for emphasis
                emphasis_foreground: Color::White,
                bold: true,
                prefix: "-".into(),
            },
            line_numbers: true,
            line_number_width: DEFAULT_LINE_NUMBER_WIDTH,
            show_header: true,
            dark_theme: true,
        }
    }
}

impl Default for DeltaTextStyle {
    fn default() -> Self {
        DeltaTextStyle {
            foreground: Color::White,
            line_background: None,
            emphasis_background: None,
            emphasis_foreground: Color::White,
            bold: false,
            prefix: " ".into(),
        }
    }
}

/// Internal formatting state for rendering.
struct DeltaFormatter {
    /// Style for regular text on a line.
    regular_style: Style,
    /// Style for emphasized (changed) portions.
    emphasis_style: Style,
    /// Style for line numbers.
    line_number_style: Style,
    /// The prefix to use.
    prefix: String,
}

impl DeltaFormatter {
    fn from_style(style: &DeltaTextStyle) -> Self {
        let mut regular_style = Style::default().fg(style.foreground);
        if let Some(bg) = style.line_background {
            regular_style = regular_style.bg(bg);
        }

        let mut emphasis_style = Style::default().fg(style.emphasis_foreground);
        if let Some(bg) = style.emphasis_background {
            emphasis_style = emphasis_style.bg(bg);
        } else if let Some(bg) = style.line_background {
            // Fall back to line background if no emphasis background
            emphasis_style = emphasis_style.bg(bg);
        }
        if style.bold {
            emphasis_style = emphasis_style.bold();
        }

        let line_number_style = Style::default().fg(Color::Color256(240)); // Gray

        DeltaFormatter {
            regular_style,
            emphasis_style,
            line_number_style,
            prefix: style.prefix.clone(),
        }
    }
}

impl Renderer for Delta {
    fn render(
        &self,
        writer: &mut dyn Write,
        data: &DisplayData,
        term_info: Option<&Term>,
    ) -> Result<()> {
        let DisplayData { hunks, old, new } = &data;

        let old_formatter = DeltaFormatter::from_style(&self.deletion);
        let new_formatter = DeltaFormatter::from_style(&self.addition);

        // Pre-split lines for efficient access
        let old_lines: Vec<_> = old.text.lines().collect();
        let new_lines: Vec<_> = new.text.lines().collect();

        // Calculate line number width based on file sizes
        let max_line_num = max(old_lines.len(), new_lines.len());
        let line_num_width = max(self.line_number_width, max_line_num.to_string().len());

        // Print file header
        if self.show_header {
            self.print_header(writer, old.filename, new.filename, term_info)?;
        }

        // Render each hunk
        for hunk_wrapper in &hunks.0 {
            match hunk_wrapper {
                RichHunk::Old(hunk) => {
                    self.print_hunk(writer, &old_lines, hunk, &old_formatter, line_num_width)?;
                }
                RichHunk::New(hunk) => {
                    self.print_hunk(writer, &new_lines, hunk, &new_formatter, line_num_width)?;
                }
            }
        }

        Ok(())
    }
}

impl Delta {
    /// Print the file header with delta-style decorations.
    fn print_header(
        &self,
        writer: &mut dyn Write,
        old_filename: &str,
        new_filename: &str,
        term_info: Option<&Term>,
    ) -> std::io::Result<()> {
        let term_width = term_info
            .and_then(|t| t.size_checked())
            .map(|(_, w)| w as usize)
            .unwrap_or(80);

        let header_style = Style::default().fg(Color::Blue).bold();
        let decoration_style = Style::default().fg(Color::Blue);

        // Top decoration line
        let top_line: String = HORIZONTAL_LINE.to_string().repeat(term_width);
        writeln!(writer, "{}", decoration_style.apply_to(&top_line))?;

        // File names
        if old_filename == new_filename {
            writeln!(
                writer,
                "{} {}",
                decoration_style.apply_to(HEADER_LEFT),
                header_style.apply_to(old_filename)
            )?;
        } else {
            writeln!(
                writer,
                "{} {} → {}",
                decoration_style.apply_to(HEADER_LEFT),
                header_style.apply_to(old_filename),
                header_style.apply_to(new_filename)
            )?;
        }

        // Bottom decoration line
        writeln!(writer, "{}", decoration_style.apply_to(&top_line))?;

        Ok(())
    }

    /// Print a hunk separator showing line range.
    fn print_hunk_header(
        &self,
        writer: &mut dyn Write,
        hunk: &Hunk,
        _formatter: &DeltaFormatter,
        line_num_width: usize,
    ) -> Result<()> {
        let first_line = hunk.first_line().unwrap_or(0);
        let last_line = hunk.last_line().unwrap_or(0);

        let header_style = Style::default().fg(Color::Cyan);

        // Add a blank line before hunks for visual separation
        writeln!(writer)?;

        if self.line_numbers {
            // Padding for line number column
            let padding = " ".repeat(line_num_width);
            if first_line == last_line {
                writeln!(
                    writer,
                    "{} {} @@ line {} @@",
                    padding,
                    LINE_NUMBER_SEPARATOR,
                    header_style.apply_to(first_line + 1) // 1-indexed for display
                )?;
            } else {
                writeln!(
                    writer,
                    "{} {} @@ lines {}-{} @@",
                    padding,
                    LINE_NUMBER_SEPARATOR,
                    header_style.apply_to(first_line + 1),
                    header_style.apply_to(last_line + 1)
                )?;
            }
        } else if first_line == last_line {
            writeln!(
                writer,
                "@@ line {} @@",
                header_style.apply_to(first_line + 1)
            )?;
        } else {
            writeln!(
                writer,
                "@@ lines {}-{} @@",
                header_style.apply_to(first_line + 1),
                header_style.apply_to(last_line + 1)
            )?;
        }

        Ok(())
    }

    /// Print a single hunk.
    fn print_hunk(
        &self,
        writer: &mut dyn Write,
        lines: &[&str],
        hunk: &Hunk,
        formatter: &DeltaFormatter,
        line_num_width: usize,
    ) -> Result<()> {
        debug!(
            "Printing hunk (lines {} - {})",
            hunk.first_line().unwrap_or(0),
            hunk.last_line().unwrap_or(0)
        );

        self.print_hunk_header(writer, hunk, formatter, line_num_width)?;

        for line in &hunk.0 {
            let line_index = line.line_index;

            // Safety check - skip invalid line indices
            if line_index >= lines.len() {
                debug!("Skipping invalid line index: {}", line_index);
                continue;
            }

            let text = lines[line_index];
            self.print_line(writer, text, line, formatter, line_num_width, line_index)?;
        }

        Ok(())
    }

    /// Print a single line with delta-style formatting.
    ///
    /// This handles:
    /// - Line numbers in the margin
    /// - Prefix character (+/-)
    /// - Regular text with line background
    /// - Emphasized portions with highlight background
    fn print_line(
        &self,
        writer: &mut dyn Write,
        text: &str,
        line: &Line,
        formatter: &DeltaFormatter,
        line_num_width: usize,
        line_index: usize,
    ) -> Result<()> {
        let regular = &formatter.regular_style;
        let emphasis = &formatter.emphasis_style;

        // Print line number if enabled
        if self.line_numbers {
            let line_num_str = format!("{:>width$}", line_index + 1, width = line_num_width);
            write!(
                writer,
                "{} {} ",
                formatter.line_number_style.apply_to(&line_num_str),
                formatter.line_number_style.apply_to(LINE_NUMBER_SEPARATOR)
            )?;
        }

        // Print prefix
        write!(writer, "{}", regular.apply_to(&formatter.prefix))?;

        // Track how many characters we've printed
        let mut printed_chars = 0;

        // Print text with emphasis on changed portions
        for entry in &line.entries {
            let emphasis_range = entry.start_position().column..entry.end_position().column;

            // Clamp range to text bounds to handle edge cases
            let emphasis_start = emphasis_range.start.min(text.len());
            let emphasis_end = emphasis_range.end.min(text.len());

            // Print regular text before this entry
            if printed_chars < emphasis_start {
                let regular_text = &text[printed_chars..emphasis_start];
                write!(writer, "{}", regular.apply_to(regular_text))?;
            }

            // Print emphasized text
            if emphasis_start < emphasis_end {
                let emphasized_text = &text[emphasis_start..emphasis_end];
                write!(writer, "{}", emphasis.apply_to(emphasized_text))?;
            }

            printed_chars = emphasis_end;
        }

        // Print any remaining text after the last entry
        if printed_chars < text.len() {
            let remaining_text = &text[printed_chars..];
            write!(writer, "{}", regular.apply_to(remaining_text))?;
        }

        writeln!(writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let delta = Delta::default();
        assert!(delta.line_numbers);
        assert!(delta.show_header);
        assert_eq!(delta.addition.prefix, "+");
        assert_eq!(delta.deletion.prefix, "-");
    }

    #[test]
    fn test_formatter_creation() {
        let style = DeltaTextStyle::default();
        let formatter = DeltaFormatter::from_style(&style);
        assert_eq!(formatter.prefix, " ");
    }
}
