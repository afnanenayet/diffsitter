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
use console::{Color, Style, Term, measure_text_width};
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

/// Default tab width for expanding tabs.
const DEFAULT_TAB_WIDTH: usize = 4;

/// Layout parameters for side-by-side rendering.
///
/// This struct encapsulates all the calculated widths needed for consistent
/// side-by-side rendering, ensuring the left and right columns are calculated
/// from a single source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SideBySideLayout {
    /// Content width for the left (old/deletion) column.
    left_content_width: usize,
    /// Content width for the right (new/addition) column.
    right_content_width: usize,
    /// Width of the line number area (including separator and spacing).
    /// This is 0 if line numbers are disabled.
    line_num_area_width: usize,
    /// Width of the line number itself (for formatting).
    line_num_width: usize,
}

impl SideBySideLayout {
    /// Calculate the layout for side-by-side rendering.
    ///
    /// The layout is structured as:
    /// ```text
    /// [line_num │ prefix content] │ [line_num │ prefix content]
    /// ```
    ///
    /// Where:
    /// - `line_num` is right-padded to `line_num_width`
    /// - `│` is the LINE_NUMBER_SEPARATOR (1 display column)
    /// - `prefix` is the +/- character (1 display column)
    /// - `content` fills the remaining space up to `content_width`
    /// - The middle `│` is the COLUMN_SEPARATOR with spaces: ` │ ` (3 display columns)
    fn calculate(
        term_width: usize,
        line_num_width: usize,
        show_line_numbers: bool,
    ) -> SideBySideLayout {
        // Per-side overhead breakdown:
        // - Line number: line_num_width chars (if enabled)
        // - Space after line number: 1 char (if enabled)
        // - LINE_NUMBER_SEPARATOR (│): 1 char (if enabled)
        // - Space after separator: 1 char (if enabled)
        // - Prefix (+/-): 1 char
        //
        // Total per side with line numbers: line_num_width + 4
        // Total per side without line numbers: 1 (just prefix)
        let line_num_area_width = if show_line_numbers {
            line_num_width + 3 // "NNNN │ " = line_num_width + space + separator + space
        } else {
            0
        };
        let prefix_width = 1;
        let per_side_overhead = line_num_area_width + prefix_width;

        // Middle separator: " │ " = 3 display columns
        let middle_separator_width = 3;

        let total_overhead = per_side_overhead * 2 + middle_separator_width;
        let available_for_content = term_width.saturating_sub(total_overhead);

        // Split content space between columns.
        // Give any odd character to the left column.
        let left_content_width = if available_for_content >= MIN_COLUMN_WIDTH * 2 {
            (available_for_content + 1) / 2
        } else {
            // Terminal is narrow - use what space we have
            ((available_for_content + 1) / 2).max(1)
        };

        let right_content_width = if available_for_content >= MIN_COLUMN_WIDTH * 2 {
            available_for_content / 2
        } else {
            (available_for_content / 2).max(1)
        };

        SideBySideLayout {
            left_content_width,
            right_content_width,
            line_num_area_width,
            line_num_width,
        }
    }

    /// Calculate the total expected line width for validation.
    #[cfg(test)]
    fn total_width(&self, show_line_numbers: bool) -> usize {
        let per_side_overhead = if show_line_numbers {
            self.line_num_area_width + 1 // +1 for prefix
        } else {
            1 // just prefix
        };
        let middle_separator = 3; // " │ "

        per_side_overhead * 2
            + self.left_content_width
            + self.right_content_width
            + middle_separator
    }
}

/// Expand tabs in text to spaces.
///
/// Tabs are expanded to align to `tab_width` boundaries, which matches
/// how terminals typically render them.
fn expand_tabs(text: &str, tab_width: usize) -> String {
    if !text.contains('\t') {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len());
    let mut column = 0;

    for c in text.chars() {
        if c == '\t' {
            // Calculate spaces needed to reach next tab stop
            let spaces_needed = tab_width - (column % tab_width);
            result.extend(std::iter::repeat(' ').take(spaces_needed));
            column += spaces_needed;
        } else {
            result.push(c);
            // Use measure_text_width for accurate column counting
            // For single chars, this handles wide characters correctly
            column += measure_text_width(&c.to_string());
        }
    }

    result
}

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
                line_background: Some(Color::Color256(22)),
                emphasis_background: Some(Color::Color256(28)),
                emphasis_foreground: Color::White,
                bold: true,
                prefix: "".into(),
            },
            deletion: DeltaTextStyle {
                foreground: Color::Red,
                line_background: Some(Color::Color256(52)),
                emphasis_background: Some(Color::Color256(88)),
                emphasis_foreground: Color::White,
                bold: true,
                prefix: "".into(),
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
            // Calculate layout using the unified SideBySideLayout struct
            let layout = SideBySideLayout::calculate(term_width, line_num_width, self.line_numbers);

            self.render_side_by_side(
                writer,
                hunks,
                &old_lines,
                &new_lines,
                &old_formatter,
                &new_formatter,
                &layout,
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
    fn render_side_by_side(
        &self,
        writer: &mut dyn Write,
        hunks: &crate::diff::RichHunks,
        old_lines: &[&str],
        new_lines: &[&str],
        old_formatter: &DeltaFormatter,
        new_formatter: &DeltaFormatter,
        layout: &SideBySideLayout,
    ) -> Result<()> {
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
                layout,
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
    fn render_side_by_side_group(
        &self,
        writer: &mut dyn Write,
        old_hunks: &[&Hunk],
        new_hunks: &[&Hunk],
        old_lines: &[&str],
        new_lines: &[&str],
        old_formatter: &DeltaFormatter,
        new_formatter: &DeltaFormatter,
        layout: &SideBySideLayout,
    ) -> Result<()> {
        // Collect all lines from old hunks, expanding tabs
        let old_display_lines: Vec<_> = old_hunks
            .iter()
            .flat_map(|hunk| {
                hunk.0.iter().filter_map(|line| {
                    if line.line_index < old_lines.len() {
                        let expanded = expand_tabs(old_lines[line.line_index], DEFAULT_TAB_WIDTH);
                        Some((line.clone(), expanded))
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Collect all lines from new hunks, expanding tabs
        let new_display_lines: Vec<_> = new_hunks
            .iter()
            .flat_map(|hunk| {
                hunk.0.iter().filter_map(|line| {
                    if line.line_index < new_lines.len() {
                        let expanded = expand_tabs(new_lines[line.line_index], DEFAULT_TAB_WIDTH);
                        Some((line.clone(), expanded))
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
            layout,
        )?;

        // Print lines side by side, padding the shorter side
        let max_lines = max(old_display_lines.len(), new_display_lines.len());

        for i in 0..max_lines {
            let old_line_data = old_display_lines.get(i);
            let new_line_data = new_display_lines.get(i);

            self.print_side_by_side_line(
                writer,
                old_line_data.map(|(line, text)| (line, text.as_str())),
                new_line_data.map(|(line, text)| (line, text.as_str())),
                old_formatter,
                new_formatter,
                layout,
            )?;
        }

        Ok(())
    }

    /// Print the header for a side-by-side hunk group.
    fn print_side_by_side_header(
        &self,
        writer: &mut dyn Write,
        old_hunks: &[&Hunk],
        new_hunks: &[&Hunk],
        old_formatter: &DeltaFormatter,
        new_formatter: &DeltaFormatter,
        layout: &SideBySideLayout,
    ) -> Result<()> {
        let header_style = Style::default().fg(Color::Cyan);
        let separator_style = Style::default().fg(Color::Color256(240));

        // Print blank separator line with vertical bars (reuse the line printing logic)
        self.print_side_by_side_line(writer, None, None, old_formatter, new_formatter, layout)?;

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
        // The header content includes the prefix space (1 char) + content width

        // Left side: line number area + header content
        let left_side = if self.line_numbers {
            let line_num_padding = " ".repeat(layout.line_num_width);
            // Pad the header content to fill the column width plus prefix (1 char)
            let padded_content = format!(
                "{:<width$}",
                old_range,
                width = layout.left_content_width + 1
            );
            format!(
                "{} {} {}",
                separator_style.apply_to(&line_num_padding),
                separator_style.apply_to(LINE_NUMBER_SEPARATOR),
                header_style.apply_to(padded_content)
            )
        } else {
            let padded_content = format!(
                "{:<width$}",
                old_range,
                width = layout.left_content_width + 1
            );
            format!("{}", header_style.apply_to(padded_content))
        };

        // Right side: line number area + header content
        let right_side = if self.line_numbers {
            let line_num_padding = " ".repeat(layout.line_num_width);
            // Pad the header content to fill the column width plus prefix (1 char)
            let padded_content = format!(
                "{:<width$}",
                new_range,
                width = layout.right_content_width + 1
            );
            format!(
                "{} {} {}",
                separator_style.apply_to(&line_num_padding),
                separator_style.apply_to(LINE_NUMBER_SEPARATOR),
                header_style.apply_to(padded_content)
            )
        } else {
            let padded_content = format!(
                "{:<width$}",
                new_range,
                width = layout.right_content_width + 1
            );
            format!("{}", header_style.apply_to(padded_content))
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
    fn print_side_by_side_line(
        &self,
        writer: &mut dyn Write,
        old_data: Option<(&Line, &str)>,
        new_data: Option<(&Line, &str)>,
        old_formatter: &DeltaFormatter,
        new_formatter: &DeltaFormatter,
        layout: &SideBySideLayout,
    ) -> Result<()> {
        let separator_style = Style::default().fg(Color::Color256(240));

        // Render left (old) side with left column width
        let left_content =
            self.format_side_content(old_data, old_formatter, layout, layout.left_content_width);

        // Render right (new) side with right column width
        let right_content =
            self.format_side_content(new_data, new_formatter, layout, layout.right_content_width);

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
        layout: &SideBySideLayout,
        column_width: usize,
    ) -> String {
        let mut result = String::new();

        match data {
            Some((line, text)) => {
                // Line number
                if self.line_numbers {
                    let line_num_str = format!(
                        "{:>width$}",
                        line.line_index + 1,
                        width = layout.line_num_width
                    );
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
                    let padding = " ".repeat(layout.line_num_width);
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
    ///
    /// Uses `measure_text_width` for accurate display width calculation that handles
    /// Unicode characters correctly (including wide characters and combining marks).
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
        let mut byte_pos = 0;
        let mut display_width = 0;

        // Build content with emphasis, using Unicode-aware width calculation
        for entry in &line.entries {
            let emphasis_range = entry.start_position().column..entry.end_position().column;
            let emphasis_start = emphasis_range.start.min(text.len());
            let emphasis_end = emphasis_range.end.min(text.len());

            // Regular text before this entry
            if byte_pos < emphasis_start && display_width < column_width {
                let regular_text = &text[byte_pos..emphasis_start];
                let (truncated, width) =
                    truncate_to_display_width(regular_text, column_width - display_width);
                if !truncated.is_empty() {
                    result.push_str(&format!("{}", regular.apply_to(truncated)));
                    display_width += width;
                }
            }

            // Emphasized text
            if emphasis_start < emphasis_end && display_width < column_width {
                let emphasized_text = &text[emphasis_start..emphasis_end];
                let (truncated, width) =
                    truncate_to_display_width(emphasized_text, column_width - display_width);
                if !truncated.is_empty() {
                    result.push_str(&format!("{}", emphasis.apply_to(truncated)));
                    display_width += width;
                }
            }

            byte_pos = emphasis_end;

            if display_width >= column_width {
                break;
            }
        }

        // Remaining text after last entry
        if byte_pos < text.len() && display_width < column_width {
            let remaining_text = &text[byte_pos..];
            let (truncated, width) =
                truncate_to_display_width(remaining_text, column_width - display_width);
            if !truncated.is_empty() {
                result.push_str(&format!("{}", regular.apply_to(truncated)));
                display_width += width;
            }
        }

        // Pad to column width if needed
        if display_width < column_width {
            result.push_str(&" ".repeat(column_width - display_width));
        }

        result
    }
}

/// Truncate a string to fit within a maximum display width.
///
/// Returns the truncated string slice and its actual display width.
/// Uses `measure_text_width` to correctly handle Unicode characters.
fn truncate_to_display_width(text: &str, max_width: usize) -> (&str, usize) {
    if max_width == 0 {
        return ("", 0);
    }

    let text_width = measure_text_width(text);
    if text_width <= max_width {
        return (text, text_width);
    }

    // Need to truncate - find the byte position where we exceed max_width
    let mut current_width = 0;
    let mut last_valid_byte_pos = 0;

    for (byte_pos, ch) in text.char_indices() {
        let char_width = measure_text_width(&ch.to_string());
        if current_width + char_width > max_width {
            break;
        }
        current_width += char_width;
        last_valid_byte_pos = byte_pos + ch.len_utf8();
    }

    (&text[..last_valid_byte_pos], current_width)
}

#[cfg(test)]
mod tests {
    use std::hint::assert_unchecked;

    use super::*;

    #[test]
    fn test_default_config() {
        let delta = Delta::default();
        assert!(delta.line_numbers);
        assert!(delta.show_header);
        assert!(delta.side_by_side);
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

    // ============================================================
    // SideBySideLayout tests
    // ============================================================

    #[test]
    fn test_layout_calculation_with_line_numbers() {
        // Test with 80-column terminal, 4-digit line numbers
        let layout = SideBySideLayout::calculate(80, 4, true);

        // Per-side overhead: line_num(4) + space(1) + sep(1) + space(1) + prefix(1) = 8
        // Middle separator: 3
        // Total overhead: 8 * 2 + 3 = 19
        // Available: 80 - 19 = 61
        // Left gets (61 + 1) / 2 = 31, Right gets 61 / 2 = 30

        assert_eq!(layout.line_num_width, 4);
        assert_eq!(layout.line_num_area_width, 7); // 4 + 3 = "NNNN │ "
        assert_eq!(layout.left_content_width, 31);
        assert_eq!(layout.right_content_width, 30);

        // Verify total width matches terminal width
        assert_eq!(layout.total_width(true), 80);
    }

    #[test]
    fn test_layout_calculation_without_line_numbers() {
        // Test with 80-column terminal, no line numbers
        let layout = SideBySideLayout::calculate(80, 4, false);

        // Per-side overhead: prefix(1) = 1
        // Middle separator: 3
        // Total overhead: 1 * 2 + 3 = 5
        // Available: 80 - 5 = 75
        // Left gets (75 + 1) / 2 = 38, Right gets 75 / 2 = 37

        assert_eq!(layout.line_num_area_width, 0);
        assert_eq!(layout.left_content_width, 38);
        assert_eq!(layout.right_content_width, 37);

        // Verify total width matches terminal width
        assert_eq!(layout.total_width(false), 80);
    }

    #[test]
    fn test_layout_calculation_even_available_width() {
        // Use a terminal width that results in even available content width
        // With line numbers (overhead 19), 99 - 19 = 80 (even)
        let layout = SideBySideLayout::calculate(99, 4, true);

        // Available: 99 - 19 = 80
        // Left gets (80 + 1) / 2 = 40, Right gets 80 / 2 = 40
        // Both columns get the same width when available is even
        assert_eq!(layout.left_content_width, 40);
        assert_eq!(layout.right_content_width, 40);
        assert_eq!(layout.total_width(true), 99);
    }

    #[test]
    fn test_layout_calculation_odd_available_width() {
        // Use a terminal width that results in odd available content width
        // With line numbers (overhead 19), 100 - 19 = 81 (odd)
        let layout = SideBySideLayout::calculate(100, 4, true);

        // Available: 100 - 19 = 81
        // Left gets (81 + 1) / 2 = 41, Right gets 81 / 2 = 40
        assert_eq!(layout.left_content_width, 41);
        assert_eq!(layout.right_content_width, 40);
        assert_eq!(layout.total_width(true), 100);
    }

    #[test]
    fn test_layout_calculation_narrow_terminal() {
        // Test with very narrow terminal (below MIN_COLUMN_WIDTH * 2)
        let layout = SideBySideLayout::calculate(50, 4, true);

        // Overhead: 19
        // Available: 50 - 19 = 31 (less than MIN_COLUMN_WIDTH * 2 = 80)
        // Should use what space we have: left = 16, right = 15
        assert!(layout.left_content_width >= 1);
        assert!(layout.right_content_width >= 1);
        assert_eq!(layout.total_width(true), 50);
    }

    #[test]
    fn test_layout_calculation_very_narrow_terminal() {
        // Test with extremely narrow terminal
        let layout = SideBySideLayout::calculate(25, 4, true);

        // Overhead: 19
        // Available: 25 - 19 = 6
        // Left = 4, Right = 3 (or similar small values)
        assert!(layout.left_content_width >= 1);
        assert!(layout.right_content_width >= 1);
        assert_eq!(layout.total_width(true), 25);
    }

    #[test]
    fn test_layout_calculation_larger_line_numbers() {
        // Test with larger line number width (e.g., for files with 10000+ lines)
        let layout = SideBySideLayout::calculate(120, 6, true);

        // Per-side overhead: line_num(6) + space(1) + sep(1) + space(1) + prefix(1) = 10
        // Middle separator: 3
        // Total overhead: 10 * 2 + 3 = 23
        // Available: 120 - 23 = 97

        assert_eq!(layout.line_num_width, 6);
        assert_eq!(layout.line_num_area_width, 9); // 6 + 3
        assert_eq!(layout.total_width(true), 120);
    }

    #[test]
    fn test_layout_total_width_consistency() {
        // Test multiple terminal widths to ensure total_width always matches
        for term_width in [40, 60, 80, 100, 120, 150, 200] {
            for line_num_width in [2, 4, 6, 8] {
                for show_line_numbers in [true, false] {
                    let layout =
                        SideBySideLayout::calculate(term_width, line_num_width, show_line_numbers);
                    assert_eq!(
                        layout.total_width(show_line_numbers),
                        term_width,
                        "Mismatch for term_width={}, line_num_width={}, line_numbers={}",
                        term_width,
                        line_num_width,
                        show_line_numbers
                    );
                }
            }
        }
    }

    // ============================================================
    // Tab expansion tests
    // ============================================================

    #[test]
    fn test_expand_tabs_no_tabs() {
        let text = "hello world";
        assert_eq!(expand_tabs(text, 4), "hello world");
    }

    #[test]
    fn test_expand_tabs_single_tab_at_start() {
        let text = "\thello";
        // Tab at position 0, expands to 4 spaces (next tab stop at 4)
        assert_eq!(expand_tabs(text, 4), "    hello");
    }

    #[test]
    fn test_expand_tabs_tab_after_text() {
        let text = "ab\tcd";
        // "ab" takes 2 columns, tab at position 2 expands to 2 spaces (next tab stop at 4)
        assert_eq!(expand_tabs(text, 4), "ab  cd");
    }

    #[test]
    fn test_expand_tabs_multiple_tabs() {
        let text = "\t\t";
        // First tab at 0 -> 4 spaces, second tab at 4 -> 4 spaces
        assert_eq!(expand_tabs(text, 4), "        ");
    }

    #[test]
    fn test_expand_tabs_tab_at_tab_stop() {
        let text = "1234\t5";
        // "1234" takes 4 columns (at tab stop), tab expands to 4 spaces
        assert_eq!(expand_tabs(text, 4), "1234    5");
    }

    #[test]
    fn test_expand_tabs_custom_width() {
        let text = "\thello";
        // Tab at position 0, expands to 8 spaces with tab_width=8
        assert_eq!(expand_tabs(text, 8), "        hello");
    }

    // ============================================================
    // Truncation tests
    // ============================================================

    #[test]
    fn test_truncate_empty_string() {
        let (result, width) = truncate_to_display_width("", 10);
        assert_eq!(result, "");
        assert_eq!(width, 0);
    }

    #[test]
    fn test_truncate_zero_width() {
        let (result, width) = truncate_to_display_width("hello", 0);
        assert_eq!(result, "");
        assert_eq!(width, 0);
    }

    #[test]
    fn test_truncate_fits_exactly() {
        let (result, width) = truncate_to_display_width("hello", 5);
        assert_eq!(result, "hello");
        assert_eq!(width, 5);
    }

    #[test]
    fn test_truncate_fits_with_room() {
        let (result, width) = truncate_to_display_width("hello", 10);
        assert_eq!(result, "hello");
        assert_eq!(width, 5);
    }

    #[test]
    fn test_truncate_needs_truncation() {
        let (result, width) = truncate_to_display_width("hello world", 5);
        assert_eq!(result, "hello");
        assert_eq!(width, 5);
    }

    #[test]
    fn test_truncate_unicode_basic() {
        // Test with accented characters (1 display column each)
        let text = "héllo";
        let (result, width) = truncate_to_display_width(text, 3);
        assert_eq!(result, "hél");
        assert_eq!(width, 3);
    }

    #[test]
    fn test_truncate_preserves_utf8_boundaries() {
        // Ensure we don't split in the middle of a multi-byte character
        let text = "日本語"; // Each character is typically 2 columns wide
        let text_width = measure_text_width(text);

        // If the terminal supports wide characters, this should be 6 columns
        // Truncate to 4 should give us 2 characters
        if text_width == 6 {
            let (result, width) = truncate_to_display_width(text, 4);
            assert_eq!(result, "日本");
            assert_eq!(width, 4);
        }
    }

    // ============================================================
    // Integration tests for width consistency
    // ============================================================

    #[test]
    fn test_side_by_side_output_width_consistency() {
        // This test verifies that the layout calculation matches
        // what would actually be output
        let layout = SideBySideLayout::calculate(80, 4, true);

        // Simulate the output structure:
        // Left side: line_num_area (7) + prefix (1) + content (left_content_width)
        // Middle: " │ " (3)
        // Right side: line_num_area (7) + prefix (1) + content (right_content_width)

        let left_side_width = layout.line_num_area_width + 1 + layout.left_content_width;
        let middle_width = 3;
        let right_side_width = layout.line_num_area_width + 1 + layout.right_content_width;

        let total = left_side_width + middle_width + right_side_width;
        assert_eq!(total, 80);
    }

    #[test]
    fn test_layout_symmetry_check() {
        // For even available width, both columns should differ by at most 1
        let layout = SideBySideLayout::calculate(100, 4, true);
        let diff = layout
            .left_content_width
            .abs_diff(layout.right_content_width);
        assert!(
            diff <= 1,
            "Column widths should differ by at most 1, got {} vs {}",
            layout.left_content_width,
            layout.right_content_width
        );
    }
}
