use crate::term_ui::{chars, TerminalRenderError, TerminalRenderableBase};
use console::{measure_text_width, pad_str, Alignment};
use std::{cmp::max, io::Write};

/// A box that can be drawn in the terminal
///
/// This won't draw anything if the provided text is empty.
pub(crate) struct TermBox<'text> {
    /// The amount of padding/spacing to add between the box border and text
    ///
    /// The vertical padding will always be one pixel because of how we draw the unicode boxes.
    /// This must be at least 1 because the unicode characters mandate having at least one
    /// character of padding.
    padding: u32,

    /// The text to draw in the box.
    ///
    /// This must have a length greater than 0 (in terms of unicode graphemes) when stripped of any
    /// termiinal escape characters.
    text: &'text str,
}

/// The border characters to use to draw a box.
///
/// This allows us to parametrize between ascii and unicode chars.
struct BoxChars {
    pub top_border: char,
    pub bottom_border: char,
    pub left_border: char,
    pub right_border: char,
}

impl<'text> TermBox<'text> {
    /// Creates a terminal box for the provided text and default padding.
    pub fn new_from_text(text: &'text str) -> anyhow::Result<Self> {
        TermBox::new(text, 1)
    }

    /// Create a termbox instance with the provided text and padding.
    ///
    /// `padding` must be greater than 0.
    pub fn new(text: &'text str, padding: u32) -> anyhow::Result<Self> {
        if padding == 0 {
            anyhow::bail!("Padding must be greater than 0");
        }
        if measure_text_width(text) == 0 {
            anyhow::bail!("Text (as displayed in terminal) must not be empty");
        }
        Ok(Self { text, padding })
    }
}

impl<'text> TermBox<'text> {
    /// Get the line length of the longest line in the provided text.
    ///
    /// Users can supply multiline strings to use in the text box, and we use the width of the
    /// longest line to compute the width of the rendered box.
    fn max_line_length(&self) -> usize {
        self.text
            .lines()
            .fold(0, |acc, x| max(acc, measure_text_width(x)))
    }

    fn calculate_border_width(&self) -> usize {
        (self.padding as usize * 2) + self.max_line_length()
    }

    /// Drawing a box agnostic to unicode or ascii characters.
    fn draw_helper(
        &self,
        writer: &mut dyn Write,
        border_chars: &BoxChars,
    ) -> Result<(), TerminalRenderError> {
        if self.text.is_empty() || self.padding == 0 {
            panic!("Text must may not be empty and padding must be greater than 0");
        }
        let border_width = self.calculate_border_width();
        // With our invariant that the padding must be at least 1 and the text must be at least one
        // char long, the border width should always at least be 3.
        debug_assert!(border_width >= 3);
        writeln!(
            writer,
            " {}",
            border_chars.top_border.to_string().repeat(border_width)
        )?;
        for _ in 0..self.padding {
            writeln!(
                writer,
                "{}{:width$}{}",
                border_chars.left_border,
                " ",
                border_chars.right_border,
                width = border_width
            )?;
        }
        for line in self.text.lines() {
            writeln!(
                writer,
                "{}{}{}",
                border_chars.left_border,
                pad_str(line, border_width, Alignment::Center, None),
                border_chars.right_border,
            )?;
        }
        for _ in 0..self.padding {
            writeln!(
                writer,
                "{}{:width$}{}",
                border_chars.left_border,
                " ",
                border_chars.right_border,
                width = border_width
            )?;
        }
        writeln!(
            writer,
            " {}",
            border_chars.bottom_border.to_string().repeat(border_width)
        )?;
        Ok(())
    }
}

impl<'text> TerminalRenderableBase for TermBox<'text> {
    fn draw_ascii(&self, writer: &mut dyn Write) -> Result<(), TerminalRenderError> {
        self.draw_helper(
            writer,
            &BoxChars {
                top_border: '-',
                bottom_border: '-',
                left_border: '|',
                right_border: '|',
            },
        )
    }

    fn draw_unicode(&self, writer: &mut dyn Write) -> Result<(), TerminalRenderError> {
        self.draw_helper(
            writer,
            &BoxChars {
                top_border: chars::LOWER_BLOCK,
                bottom_border: chars::UPPER_BLOCK,
                left_border: chars::RIGHT_BLOCK,
                right_border: chars::LEFT_BLOCK,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::StringWriter;
    use console::Style;

    #[test]
    fn test_empty_text() {
        // Testing terminal escape characters and unicode zero-width chars since those should be
        // counted as "empty", even though they have multiple chars.
        let style = Style::new().italic().red();
        let inputs = [
            String::from(""),
            style.apply_to("").to_string(),
            String::from("​"),
        ];
        for input in inputs {
            assert!(
                TermBox::new_from_text(&input).is_err(),
                "'{}' was not detected as a zero width string",
                input
            );
        }
    }

    #[test]
    fn test_draw_box_unicode() {
        let term_box = TermBox {
            padding: 1,
            text: "X",
        };
        let actual = {
            let mut writer = StringWriter::new();
            term_box.draw_unicode(&mut writer).unwrap();
            writer.consume()
        };
        let expected = " ▁▁▁
▕   ▏
▕ X ▏
▕   ▏
 ▔▔▔
";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_draw_box_ascii() {
        let term_box = TermBox {
            padding: 1,
            text: "X",
        };
        let actual = {
            let mut writer = StringWriter::new();
            term_box.draw_ascii(&mut writer).unwrap();
            writer.consume()
        };
        let expected = " ---
|   |
| X |
|   |
 ---
";
        assert_eq!(actual, expected);
    }
}
