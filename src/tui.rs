use anyhow::Result;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;
use std::io::{Stdout, stdout};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

fn restore() -> Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

/// Owns the terminal's raw/alternate-screen state and restores it on drop,
/// so an early return or a `?` can't leave the user's shell wrecked.
pub struct TerminalGuard(Tui);

impl TerminalGuard {
    pub fn new() -> Result<Self> {
        // Drop alone isn't enough: a panic unwinds through the hook *before*
        // destructors are useful to us, and `panic = "abort"` skips them
        // entirely. Restore in the hook, then delegate to the default one so
        // the backtrace still prints — onto a sane terminal.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = restore();
            prev(info);
        }));

        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen)?;
        Ok(Self(Terminal::new(CrosstermBackend::new(stdout()))?))
    }

    pub fn terminal(&mut self) -> &mut Tui {
        &mut self.0
    }

    /// Hand the terminal back to the shell so a child process (nvim, hx) can
    /// own it. Must be paired with `resume` unless we're quitting.
    pub fn suspend(&mut self) -> Result<()> {
        restore()
    }

    pub fn resume(&mut self) -> Result<()> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen)?;
        self.0.clear()?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore();
    }
}
