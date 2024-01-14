//! A terminal UI rendering helper library.
//!
//! These are utilities for drawing pretty things in the terminal.

mod chars;
pub mod term_box;

use std::io::{self, Write};
use thiserror::Error;

// TODO:(afnan) We will probably want to create some traits/common interface for drawing these
// sorts of elements

/// Errors that pop up when trying to render something to the terminal.
#[derive(Error, Debug)]
pub enum TerminalRenderError {
    #[error("Error writing to terminal")]
    WriteError(#[from] io::Error),
}

/// Some object that can be rendered in the terminal.
///
/// The base renderable widgets must provide both unicode and ASCII compliant renders.
pub trait TerminalRenderableBase {
    /// Draw the renderable using unicode characters.
    fn draw_unicode(&self, writer: &mut dyn Write) -> Result<(), TerminalRenderError>;

    /// Draw the renderable using ASCII characters.
    fn draw_ascii(&self, writer: &mut dyn Write) -> Result<(), TerminalRenderError>;
}

/// The user facing trait that automatically handles whether the terminal is unicode capable.
pub trait TerminalRenderable: TerminalRenderableBase {
    fn draw(&self, writer: &mut dyn Write) -> Result<(), TerminalRenderError>;
}

impl<T: TerminalRenderableBase> TerminalRenderable for T {
    fn draw(&self, writer: &mut dyn Write) -> Result<(), TerminalRenderError> {
        self.draw_unicode(writer)
    }
}
