use anyhow::Result;
use layout::create_layout;
use std::process::Command;
use std::sync::OnceLock;

mod app;
mod config;
mod database;
mod input_handler;
mod layout;
mod searcher;
mod ssh_config_store;
mod term;
mod theme;
mod widgets;

use app::{App, AppState};
use config::{resolve_config, Config};
use input_handler::handle_inputs;
use term::TerminalGuard;
use theme::Theme;
use widgets::{
    config_widget::ConfigWidget, groups_widget::GroupsWidget, help_widget::HelpWidget,
    hosts_widget::HostsWidget, shortcuts_widget::ShortcutsWidget, version_widget::VersionWidget,
};

// SSH connection constants
const SSH_CONNECT_TIMEOUT: &str = "ConnectTimeout=10";
const SSH_KEEP_ALIVE_INTERVAL: &str = "ServerAliveInterval=5";

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn get_config() -> &'static Config {
    CONFIG.get_or_init(resolve_config)
}

static THEME: OnceLock<Theme> = OnceLock::new();

pub fn get_theme() -> &'static Theme {
    THEME.get_or_init(|| get_config().theme)
}

fn main() -> Result<()> {
    std::panic::set_hook(Box::new(|info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        eprintln!("fast-ssh panicked: {}", info);
    }));

    // Initialize configuration and theme
    get_theme();

    let mut app = match App::new() {
        Ok(app) => app,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let mut terminal = TerminalGuard::new()?;

    app.host_state.select(Some(0));

    let mut needs_redraw = true;
    loop {
        if needs_redraw {
            draw(&mut terminal, &mut app)?;
        }

        needs_redraw = handle_inputs(&mut app)?;

        if app.should_quit
            || app.should_spawn_ssh
            || app.should_copy_ssh_key
            || app.should_copy_files
        {
            break;
        }
    }

    terminal.restore()?;

    // Execute the command based on the app state
    let command = if app.should_spawn_ssh {
        Some("ssh")
    } else if app.should_copy_ssh_key {
        Some("ssh-copy-id")
    } else if app.should_copy_files {
        Some("sftp")
    } else {
        None
    };

    let Some(cmd) = command else {
        return Ok(());
    };

    // Safely get selected config, exit gracefully if none selected
    let Some(selected_config) = app.get_selected_item() else {
        eprintln!("Error: No host selected");
        return Ok(());
    };

    let host_name = &selected_config.full_name;

    // A leading '-' would be parsed as an ssh/sftp option, not a hostname
    if host_name.starts_with('-') {
        eprintln!(
            "Error: refusing to connect to host starting with '-': {}",
            host_name
        );
        return Ok(());
    }

    // Losing the usage stats must not prevent connecting
    if let Err(e) = app.db.save_host_values(
        host_name,
        selected_config.connection_count + 1,
        chrono::offset::Local::now().timestamp(),
    ) {
        eprintln!("Warning: could not save connection history: {}", e);
    }

    let mut command = Command::new(cmd);

    // Add SSH-specific options
    if cmd == "ssh" {
        command
            .arg("-o")
            .arg(SSH_CONNECT_TIMEOUT)
            .arg("-o")
            .arg(SSH_KEEP_ALIVE_INTERVAL);
    }

    command.arg(host_name);
    run_command(command)
}

/// Replaces this process with `command`, so signals and the exit status go
/// straight to the user's shell.
#[cfg(unix)]
fn run_command(mut command: Command) -> Result<()> {
    use std::os::unix::process::CommandExt;

    let error = command.exec();
    Err(anyhow::Error::new(error).context(format!(
        "Could not run {}",
        command.get_program().to_string_lossy()
    )))
}

/// Runs `command` and exits with its status.
#[cfg(not(unix))]
fn run_command(mut command: Command) -> Result<()> {
    let status = command.spawn()?.wait()?;
    std::process::exit(status.code().unwrap_or(1));
}

fn draw(terminal: &mut TerminalGuard, app: &mut App) -> Result<()> {
    app.clamp_host_selection();

    terminal.terminal().draw(|frame| {
        let layout = create_layout(app, frame);

        match app.state {
            AppState::Normal => GroupsWidget::render(app, layout.groups_area, frame),
            AppState::Searching => {
                app.searcher
                    .render(layout.groups_area, frame, app.search_result_count())
            }
        };

        if let Some(help_area) = layout.help_area {
            HelpWidget::render(help_area, frame);
        }
        HostsWidget::render(app, layout.hosts_area, layout.dense, frame);
        if let Some(config_area) = layout.config_area {
            ConfigWidget::render(app, config_area, frame);
        }
        VersionWidget::render(layout.version_area, layout.help_area.is_none(), frame);

        if let Some(shortcuts_area) = layout.shortcuts_area {
            ShortcutsWidget::render(shortcuts_area, frame);
        }
    })?;

    Ok(())
}
