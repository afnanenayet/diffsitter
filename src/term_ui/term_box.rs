use crate::term_ui::{chars, TerminalRenderError, TerminalRenderableBase};
use console::{measure_text_width, pad_str, Alignment};
use std::{cmp::max, io::Write};

/// A box that can be drawn in the terminal
pub struct TermBox<'text> {
    /// The amount of padding/spacing to add between the box border and text
    pub padding: u32,

    /// The text to draw in the box.
    pub text: &'text str,
}

impl<'text> TermBox<'text> {
    fn calculate_border_width(&self) -> usize {
        // If there are multiple strings they will be split by newline, we want to take the maximum
        // length of any of the lines.
        let max_len = self.text.lines().fold(0, |acc, x| max(acc, x.len()));
        return max_len + (2 * self.padding as usize);
    }

    fn calculate_border_height(&self) -> usize {
        return 1 + (2 * self.padding as usize);
    }

    /// Helper method to write a horizontal line for top/bottom borders
    fn write_horizontal_line(&self, writer: &mut dyn Write) {}
}

impl<'text> TerminalRenderableBase for TermBox<'text> {
    fn draw_ascii(&self, writer: &mut dyn Write) -> Result<(), TerminalRenderError> {
        todo!();
    }

    fn draw_unicode(&self, writer: &mut dyn Write) -> Result<(), TerminalRenderError> {
        let border_width = self.calculate_border_width();
        let top_border = format!(
            " {} \n",
            chars::LOWER_BLOCK.to_string().repeat(border_width),
        );
        let bottom_border = format!(
            " {} \n",
            chars::UPPER_BLOCK.to_string().repeat(border_width),
        );
        writer.write_all(top_border.as_bytes()).unwrap();
        for _ in 0..self.padding {
            writeln!(
                writer,
                "{}{:width$} {}",
                chars::RIGHT_BLOCK,
                " ",
                chars::LEFT_BLOCK,
                width = border_width - 2
            )?;
        }
        for line in self.text.lines() {
            writeln!(
                writer,
                "{}{} {}",
                chars::RIGHT_BLOCK,
                pad_str(line, border_width - 2, Alignment::Center, None),
                chars::LEFT_BLOCK,
            )?;
        }
        for _ in 0..self.padding {
            writeln!(
                writer,
                "{}{:width$} {}",
                chars::RIGHT_BLOCK,
                " ",
                chars::LEFT_BLOCK,
                width = border_width - 2
            )?;
        }
        writer.write_all(bottom_border.as_bytes())?;
        Ok(())
    }
}
