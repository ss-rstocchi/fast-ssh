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
use term::{init_terminal, restore_terminal};
use theme::Theme;
use widgets::{
    config_widget::ConfigWidget, groups_widget::GroupsWidget, help_widget::HelpWidget,
    hosts_widget::HostsWidget, shortcuts_widget::ShortcutsWidget,
};

// SSH connection constants
const SSH_CONNECT_TIMEOUT: &str = "ConnectTimeout=10";
const SSH_KEEP_ALIVE_INTERVAL: &str = "ServerAliveInterval=5";

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn get_config() -> &'static Config {
    CONFIG.get_or_init(resolve_config)
}

// Re-export THEME for backwards compatibility
pub static THEME: OnceLock<Theme> = OnceLock::new();

pub fn get_theme() -> &'static Theme {
    THEME.get_or_init(|| get_config().theme)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize configuration and theme
    get_theme();

    let mut app = match App::new().await {
        Ok(app) => app,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let mut terminal = init_terminal()?;

    app.host_state.select(Some(0));

    loop {
        terminal.draw(|frame| {
            let layout = create_layout(&app, frame);

            match app.state {
                AppState::Normal => GroupsWidget::render(&app, layout.groups_area, frame),
                AppState::Searching => app.searcher.render(&app, layout.groups_area, frame),
            };

            HelpWidget::render(&app, layout.help_area, frame);
            HostsWidget::render(&mut app, layout.hosts_area, frame);
            ConfigWidget::render(&app, layout.config_area, frame);

            if let Some(shortcuts_area) = layout.shortcuts_area {
                ShortcutsWidget::render(&app, shortcuts_area, frame);
            }
        })?;

        handle_inputs(&mut app)?;

        if app.should_quit || app.should_spawn_ssh {
            break;
        }
    }

    restore_terminal(&mut terminal)?;

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

    if let Some(cmd) = command {
        // Safely get selected config, exit gracefully if none selected
        let Some(selected_config) = app.get_selected_item() else {
            eprintln!("Error: No host selected");
            return Ok(());
        };
        
        // Assert preconditions
        debug_assert!(!selected_config.full_name.is_empty(), "host name should not be empty");
        debug_assert!(selected_config.connection_count >= 0, "connection count should be non-negative");
        
        let host_name = &selected_config.full_name;

        // Update database with connection info
        app.db.save_host_values(
            host_name,
            selected_config.connection_count + 1,
            chrono::offset::Local::now().timestamp(),
        )?;

        // Extract the first part of the hostname (before any space)
        let host_arg = host_name.split_whitespace().next().unwrap_or(host_name);
        
        // Assert extracted host is valid
        debug_assert!(!host_arg.is_empty(), "extracted host should not be empty");

        // Build and execute the command
        let mut command = Command::new(cmd);
        
        // Add SSH-specific options
        if cmd == "ssh" {
            command
                .arg("-o")
                .arg(SSH_CONNECT_TIMEOUT)
                .arg("-o")
                .arg(SSH_KEEP_ALIVE_INTERVAL);
        }
        
        command.arg(host_arg).spawn()?.wait()?;
    }

    Ok(())
}
