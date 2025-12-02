use std::path::{Path, PathBuf};

use clap::Parser;
use color_eyre::Result;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event},
    style::Style,
    widgets::{Block, Paragraph, Wrap},
};

/// Inspect a document to see the different node types and kind that diffsitter sees.
#[derive(Debug, clap::Parser)]
#[clap(author, version, about)]
pub struct TsDebugger {
    // Path to the file to inspect.
    file_path: PathBuf,

    /// Set the language manually instead of inferring from the file extension.
    language: Option<String>,
}

impl TsDebugger {
    fn run(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;
            if matches!(event::read()?, Event::Key(_)) {
                break Ok(());
            }
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let text = std::fs::read_to_string(&self.file_path).unwrap();
        let para = Paragraph::new(text).block(Block::bordered().title("Paragraph"));
        frame.render_widget(para, frame.area());
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut args = TsDebugger::parse();
    let terminal = ratatui::init();
    let result = args.run(terminal);
    ratatui::restore();
    result
}
