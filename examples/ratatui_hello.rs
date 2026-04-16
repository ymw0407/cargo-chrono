//! Minimal ratatui "hello world" example.
//!
//! Run with: `cargo run --example ratatui_hello`
//!
//! Displays a centered greeting and exits when the user presses 'q' or Esc.
//! Realtime team: use this on Day 1 to verify ratatui/crossterm setup.

use std::io;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

fn main() -> io::Result<()> {
    // Setup terminal.
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    // Main loop.
    loop {
        terminal.draw(|frame| {
            let area = frame.area();

            let greeting = Paragraph::new("Hello, cargo-chrono! Press 'q' to quit.")
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .title(" ratatui hello ")
                        .borders(Borders::ALL),
                );

            frame.render_widget(greeting, area);
        })?;

        // Handle input.
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        _ => {}
                    }
                }
            }
        }
    }

    // Restore terminal.
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    Ok(())
}
