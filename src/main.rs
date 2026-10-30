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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    std::panic::set_hook(Box::new(|info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        eprintln!("fast-ssh panicked: {}", info);
    }));

    // Initialize configuration and theme
    get_theme();

    let mut app = match App::new().await {
        Ok(app) => app,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let mut terminal = TerminalGuard::new()?;

    app.host_state.select(Some(0));

    loop {
        app.clamp_host_selection();

        let mut hosts_area_height: u16 = 0;
        terminal.terminal().draw(|frame| {
            let layout = create_layout(&app, frame);
            hosts_area_height = layout.hosts_area.height;

            match app.state {
                AppState::Normal => GroupsWidget::render(&app, layout.groups_area, frame),
                AppState::Searching => {
                    app.searcher
                        .render(layout.groups_area, frame, app.search_result_count())
                }
            };

            HelpWidget::render(layout.help_area, frame);
            HostsWidget::render(&mut app, layout.hosts_area, frame);
            ConfigWidget::render(&app, layout.config_area, frame);
            VersionWidget::render(layout.version_area, frame);

            if let Some(shortcuts_area) = layout.shortcuts_area {
                ShortcutsWidget::render(shortcuts_area, frame);
            }
        })?;
        app.hosts_area_height = hosts_area_height;

        handle_inputs(&mut app)?;

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
