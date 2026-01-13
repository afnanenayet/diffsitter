//! A delta-style diff renderer.
//!
//! This renderer produces output similar to the delta diff tool (https://github.com/dandavison/delta),
//! featuring:
//! - File headers with decorations
//! - Line numbers in the margin
//! - Color-coded additions and deletions with within-line emphasis
//! - Box-drawing characters for visual structure
//! - Optional side-by-side view

use crate::diff::{DocumentType, Hunk, Line, RichHunk};
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
const COLUMN_SEPARATOR: &str = "│";

/// Default line number width for padding.
const DEFAULT_LINE_NUMBER_WIDTH: usize = 4;

/// Default terminal width when we can't detect it.
const DEFAULT_TERM_WIDTH: usize = 80;

/// Minimum column width for side-by-side view.
const MIN_COLUMN_WIDTH: usize = 40;

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
    /// Whether to display diffs in side-by-side view.
    ///
    /// When enabled, deletions appear on the left and additions on the right,
    /// similar to delta's `-s` or `--side-by-side` option.
    pub side_by_side: bool,
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
            side_by_side: false,
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

        // Get terminal width for side-by-side calculations
        let term_width = term_info
            .and_then(|t| t.size_checked())
            .map(|(_, w)| w as usize)
            .unwrap_or(DEFAULT_TERM_WIDTH);

        // Print file header
        if self.show_header {
            self.print_header(writer, old.filename, new.filename, term_info)?;
        }

        if self.side_by_side {
            self.render_side_by_side(
                writer,
                hunks,
                &old_lines,
                &new_lines,
                &old_formatter,
                &new_formatter,
                line_num_width,
                term_width,
            )?;
        } else {
            // Render each hunk sequentially (unified view)
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

    /// Render hunks in side-by-side view.
    ///
    /// Groups related old/new hunks and displays them in two columns:
    /// - Left column: deletions (old file)
    /// - Right column: additions (new file)
    #[allow(clippy::too_many_arguments)]
    fn render_side_by_side(
        &self,
        writer: &mut dyn Write,
        hunks: &crate::diff::RichHunks,
        old_lines: &[&str],
        new_lines: &[&str],
        old_formatter: &DeltaFormatter,
        new_formatter: &DeltaFormatter,
        line_num_width: usize,
        term_width: usize,
    ) -> Result<()> {
        // Calculate column widths
        // Layout: [line_num | prefix content] [sep] [line_num | prefix content]
        // Each side needs: line_num_width + separator(1) + space(1) + prefix(1) + content
        let separator_width = 3; // " │ "
        let line_num_overhead = if self.line_numbers {
            line_num_width + 3 // number + " │ "
        } else {
            0
        };
        let prefix_width = 1;

        // Calculate content width for each column
        // Total line: 2 * (line_num_overhead + prefix + content) + separator
        let total_overhead = (line_num_overhead + prefix_width) * 2 + separator_width;
        let available_width = term_width.saturating_sub(total_overhead);
        // Don't use MIN_COLUMN_WIDTH if it would exceed terminal width
        // This ensures lines never wrap due to being too wide
        let column_content_width = if available_width >= MIN_COLUMN_WIDTH * 2 {
            available_width / 2
        } else {
            // Terminal is narrow - use what space we have
            (available_width / 2).max(1)
        };

        // Group hunks into pairs of (old_hunks, new_hunks) for side-by-side display
        let hunk_groups = self.group_hunks_for_side_by_side(hunks);

        for (old_hunks, new_hunks) in hunk_groups {
            self.render_side_by_side_group(
                writer,
                &old_hunks,
                &new_hunks,
                old_lines,
                new_lines,
                old_formatter,
                new_formatter,
                line_num_width,
                column_content_width,
            )?;
        }

        Ok(())
    }

    /// Group hunks for side-by-side display.
    ///
    /// Returns a vector of (old_hunks, new_hunks) pairs. Consecutive hunks of the
    /// same type are grouped together, and adjacent old/new groups form pairs.
    fn group_hunks_for_side_by_side<'a>(
        &self,
        hunks: &'a crate::diff::RichHunks<'a>,
    ) -> Vec<(Vec<&'a Hunk<'a>>, Vec<&'a Hunk<'a>>)> {
        let mut groups: Vec<(Vec<&'a Hunk<'a>>, Vec<&'a Hunk<'a>>)> = Vec::new();
        let mut current_old: Vec<&'a Hunk<'a>> = Vec::new();
        let mut current_new: Vec<&'a Hunk<'a>> = Vec::new();

        for hunk_wrapper in &hunks.0 {
            match hunk_wrapper {
                DocumentType::Old(hunk) => {
                    // If we have pending new hunks, flush the current group
                    if !current_new.is_empty() {
                        groups.push((
                            std::mem::take(&mut current_old),
                            std::mem::take(&mut current_new),
                        ));
                    }
                    current_old.push(hunk);
                }
                DocumentType::New(hunk) => {
                    current_new.push(hunk);
                    // If we have both old and new, and encounter another old next,
                    // we'll flush then. For now, keep accumulating.
                }
            }
        }

        // Flush any remaining hunks
        if !current_old.is_empty() || !current_new.is_empty() {
            groups.push((current_old, current_new));
        }

        groups
    }

    /// Render a group of old/new hunks side by side.
    #[allow(clippy::too_many_arguments)]
    fn render_side_by_side_group(
        &self,
        writer: &mut dyn Write,
        old_hunks: &[&Hunk],
        new_hunks: &[&Hunk],
        old_lines: &[&str],
        new_lines: &[&str],
        old_formatter: &DeltaFormatter,
        new_formatter: &DeltaFormatter,
        line_num_width: usize,
        column_width: usize,
    ) -> Result<()> {
        // Collect all lines from old hunks
        let old_display_lines: Vec<_> = old_hunks
            .iter()
            .flat_map(|hunk| {
                hunk.0.iter().filter_map(|line| {
                    if line.line_index < old_lines.len() {
                        Some((line, old_lines[line.line_index]))
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Collect all lines from new hunks
        let new_display_lines: Vec<_> = new_hunks
            .iter()
            .flat_map(|hunk| {
                hunk.0.iter().filter_map(|line| {
                    if line.line_index < new_lines.len() {
                        Some((line, new_lines[line.line_index]))
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Print header for the group
        self.print_side_by_side_header(
            writer,
            old_hunks,
            new_hunks,
            old_formatter,
            new_formatter,
            line_num_width,
            column_width,
        )?;

        // Print lines side by side, padding the shorter side
        let max_lines = max(old_display_lines.len(), new_display_lines.len());

        for i in 0..max_lines {
            let old_line_data = old_display_lines.get(i);
            let new_line_data = new_display_lines.get(i);

            self.print_side_by_side_line(
                writer,
                old_line_data.map(|(line, text)| (*line, *text)),
                new_line_data.map(|(line, text)| (*line, *text)),
                old_formatter,
                new_formatter,
                line_num_width,
                column_width,
            )?;
        }

        Ok(())
    }

    /// Print the header for a side-by-side hunk group.
    #[allow(clippy::too_many_arguments)]
    fn print_side_by_side_header(
        &self,
        writer: &mut dyn Write,
        old_hunks: &[&Hunk],
        new_hunks: &[&Hunk],
        old_formatter: &DeltaFormatter,
        new_formatter: &DeltaFormatter,
        line_num_width: usize,
        column_width: usize,
    ) -> Result<()> {
        let header_style = Style::default().fg(Color::Cyan);
        let separator_style = Style::default().fg(Color::Color256(240));

        // Print blank separator line with vertical bars (reuse the line printing logic)
        self.print_side_by_side_line(
            writer,
            None,
            None,
            old_formatter,
            new_formatter,
            line_num_width,
            column_width,
        )?;

        // Build old side header text
        let old_range = if !old_hunks.is_empty() {
            let first = old_hunks.first().and_then(|h| h.first_line()).unwrap_or(0);
            let last = old_hunks.last().and_then(|h| h.last_line()).unwrap_or(0);
            if first == last {
                format!("@@ line {} @@", first + 1)
            } else {
                format!("@@ lines {}-{} @@", first + 1, last + 1)
            }
        } else {
            String::new()
        };

        // Build new side header text
        let new_range = if !new_hunks.is_empty() {
            let first = new_hunks.first().and_then(|h| h.first_line()).unwrap_or(0);
            let last = new_hunks.last().and_then(|h| h.last_line()).unwrap_or(0);
            if first == last {
                format!("@@ line {} @@", first + 1)
            } else {
                format!("@@ lines {}-{} @@", first + 1, last + 1)
            }
        } else {
            String::new()
        };

        // Build header with same structure as data lines:
        // [line_num_padding │ header_content padded to column_width] │ [line_num_padding │ header_content]

        // Left side: line number area + header content
        let left_side = if self.line_numbers {
            let line_num_padding = " ".repeat(line_num_width);
            // Pad the header content to fill the column width (including prefix space)
            let padded_content = format!("{:<width$}", old_range, width = column_width + 1);
            format!(
                "{} {} {}",
                separator_style.apply_to(&line_num_padding),
                separator_style.apply_to(LINE_NUMBER_SEPARATOR),
                header_style.apply_to(padded_content)
            )
        } else {
            let padded_content = format!("{:<width$}", old_range, width = column_width + 1);
            format!("{}", header_style.apply_to(padded_content))
        };

        // Right side: line number area + header content
        let right_side = if self.line_numbers {
            let line_num_padding = " ".repeat(line_num_width);
            format!(
                "{} {} {}",
                separator_style.apply_to(&line_num_padding),
                separator_style.apply_to(LINE_NUMBER_SEPARATOR),
                header_style.apply_to(&new_range)
            )
        } else {
            format!("{}", header_style.apply_to(&new_range))
        };

        writeln!(
            writer,
            "{} {} {}",
            left_side,
            separator_style.apply_to(COLUMN_SEPARATOR),
            right_side
        )?;

        Ok(())
    }

    /// Print a single line in side-by-side view.
    #[allow(clippy::too_many_arguments)]
    fn print_side_by_side_line(
        &self,
        writer: &mut dyn Write,
        old_data: Option<(&Line, &str)>,
        new_data: Option<(&Line, &str)>,
        old_formatter: &DeltaFormatter,
        new_formatter: &DeltaFormatter,
        line_num_width: usize,
        column_width: usize,
    ) -> Result<()> {
        let separator_style = Style::default().fg(Color::Color256(240));

        // Render left (old) side
        let left_content =
            self.format_side_content(old_data, old_formatter, line_num_width, column_width);

        // Render right (new) side
        let right_content =
            self.format_side_content(new_data, new_formatter, line_num_width, column_width);

        writeln!(
            writer,
            "{} {} {}",
            left_content,
            separator_style.apply_to(COLUMN_SEPARATOR),
            right_content
        )?;

        Ok(())
    }

    /// Format content for one side of the side-by-side view.
    ///
    /// Returns a string with the line number, prefix, and content, padded to column_width.
    fn format_side_content(
        &self,
        data: Option<(&Line, &str)>,
        formatter: &DeltaFormatter,
        line_num_width: usize,
        column_width: usize,
    ) -> String {
        let mut result = String::new();

        match data {
            Some((line, text)) => {
                // Line number
                if self.line_numbers {
                    let line_num_str =
                        format!("{:>width$}", line.line_index + 1, width = line_num_width);
                    result.push_str(&format!(
                        "{} {} ",
                        formatter.line_number_style.apply_to(&line_num_str),
                        formatter.line_number_style.apply_to(LINE_NUMBER_SEPARATOR)
                    ));
                }

                // Prefix
                result.push_str(&format!(
                    "{}",
                    formatter.regular_style.apply_to(&formatter.prefix)
                ));

                // Content with emphasis
                let content = self.format_line_content(text, line, formatter, column_width);
                result.push_str(&content);
            }
            None => {
                // Empty placeholder
                if self.line_numbers {
                    let padding = " ".repeat(line_num_width);
                    result.push_str(&format!(
                        "{} {} ",
                        formatter.line_number_style.apply_to(&padding),
                        formatter.line_number_style.apply_to(LINE_NUMBER_SEPARATOR)
                    ));
                }
                // Empty prefix and content
                result.push_str(&" ".repeat(1 + column_width));
            }
        }

        result
    }

    /// Format line content with emphasis, truncating or padding to fit column width.
    fn format_line_content(
        &self,
        text: &str,
        line: &Line,
        formatter: &DeltaFormatter,
        column_width: usize,
    ) -> String {
        let regular = &formatter.regular_style;
        let emphasis = &formatter.emphasis_style;

        let mut result = String::new();
        let mut printed_chars = 0;
        let mut display_len = 0;

        // Build content with emphasis
        for entry in &line.entries {
            let emphasis_range = entry.start_position().column..entry.end_position().column;
            let emphasis_start = emphasis_range.start.min(text.len());
            let emphasis_end = emphasis_range.end.min(text.len());

            // Regular text before this entry
            if printed_chars < emphasis_start {
                let regular_text = &text[printed_chars..emphasis_start];
                let chars_to_add = (column_width - display_len).min(regular_text.len());
                if chars_to_add > 0 {
                    result.push_str(&format!(
                        "{}",
                        regular.apply_to(&regular_text[..chars_to_add])
                    ));
                    display_len += chars_to_add;
                }
            }

            // Emphasized text
            if emphasis_start < emphasis_end && display_len < column_width {
                let emphasized_text = &text[emphasis_start..emphasis_end];
                let chars_to_add = (column_width - display_len).min(emphasized_text.len());
                if chars_to_add > 0 {
                    result.push_str(&format!(
                        "{}",
                        emphasis.apply_to(&emphasized_text[..chars_to_add])
                    ));
                    display_len += chars_to_add;
                }
            }

            printed_chars = emphasis_end;

            if display_len >= column_width {
                break;
            }
        }

        // Remaining text after last entry
        if printed_chars < text.len() && display_len < column_width {
            let remaining_text = &text[printed_chars..];
            let chars_to_add = (column_width - display_len).min(remaining_text.len());
            if chars_to_add > 0 {
                result.push_str(&format!(
                    "{}",
                    regular.apply_to(&remaining_text[..chars_to_add])
                ));
                display_len += chars_to_add;
            }
        }

        // Pad to column width if needed
        if display_len < column_width {
            result.push_str(&" ".repeat(column_width - display_len));
        }

        result
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
        assert!(!delta.side_by_side);
        assert_eq!(delta.addition.prefix, "+");
        assert_eq!(delta.deletion.prefix, "-");
    }

    #[test]
    fn test_side_by_side_config() {
        let mut delta = Delta::default();
        delta.side_by_side = true;
        assert!(delta.side_by_side);
    }

    #[test]
    fn test_formatter_creation() {
        let style = DeltaTextStyle::default();
        let formatter = DeltaFormatter::from_style(&style);
        assert_eq!(formatter.prefix, " ");
    }
}
