use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::Stdout;
use tui::{backend::CrosstermBackend, Terminal};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

pub fn init_terminal() -> Result<Tui, Box<dyn std::error::Error>> {
    enable_raw_mode()?;

    let mut stdout = std::io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(error.into());
    }

    match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            let _ = disable_raw_mode();
            let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
            Err(error.into())
        }
    }
}

pub fn restore_terminal(terminal: &mut Tui) -> Result<(), Box<dyn std::error::Error>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Restores the terminal when dropped, so early returns and `?` cannot leave
/// the terminal in raw mode / alternate screen.
pub struct TerminalGuard {
    terminal: Option<Tui>,
}

impl TerminalGuard {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            terminal: Some(init_terminal()?),
        })
    }

    pub fn terminal(&mut self) -> &mut Tui {
        self.terminal.as_mut().expect("terminal already restored")
    }

    pub fn restore(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mut terminal) = self.terminal.take() {
            restore_terminal(&mut terminal)?;
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Some(mut terminal) = self.terminal.take() {
            let _ = restore_terminal(&mut terminal);
        }
    }
}
