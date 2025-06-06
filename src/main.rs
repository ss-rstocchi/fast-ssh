use layout::create_layout;
use lazy_static::lazy_static;
use std::{fs, io::Write, path::PathBuf, process::Command};

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

use app::*;
use config::*;
use input_handler::*;
use term::*;
use theme::*;
use widgets::{
    config_widget::ConfigWidget, groups_widget::GroupsWidget, help_widget::HelpWidget,
    hosts_widget::HostsWidget, shortcuts_widget::ShortcutsWidget,
};

lazy_static! {
    pub static ref CONFIG: Config = resolve_config();
    pub static ref THEME: &'static Theme = &CONFIG.theme;
}

// Function to save the SSH config file based on the current state of the app
pub fn save_ssh_config(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let config_path = PathBuf::from(home).join(".ssh").join("config");
    
    // First, read the existing file to preserve comments and structure
    let existing_content = fs::read_to_string(&config_path)?;
    let lines: Vec<String> = existing_content.lines().map(String::from).collect();
    
    // Create a mapping of hosts and their positions in the file
    let mut host_positions: Vec<(usize, String)> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if line.trim().starts_with("Host ") && !line.contains("*") {
            let host = line.trim()["Host ".len()..].trim().to_string();
            host_positions.push((idx, host));
        }
    }
    
    // Create a backup of the original file
    let backup_path = config_path.with_extension("bak");
    fs::copy(&config_path, &backup_path)?;
    
    // Get the ordering information from the app, preserving group structure and order
    // Store groups in the order they appear in app.scs.groups
    let mut groups_and_hosts: Vec<(String, Vec<String>)> = Vec::new();
    
    // Skip the "Recents" group as it's dynamically generated
    for group in &app.scs.groups {
        if group.name == "Recents" {
            continue;
        }
        
        let group_hosts = group.items.iter()
            .map(|item| item.full_name.clone())
            .collect::<Vec<String>>();
            
        groups_and_hosts.push((group.name.clone(), group_hosts));
    }
    
    // Create a new file with the updated order
    let mut new_content = String::new();
    let mut processed_hosts = std::collections::HashSet::new();
    
    // First, find all Host * entries and add them at the beginning
    for line in &lines {
        if line.trim().starts_with("Host ") && line.contains("*") {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }
    
    // Process each group in the order we collected them (which is the order in app.scs.groups)
    for (group_name, group_hosts) in groups_and_hosts {
        // Add a comment to identify the group (helps organize the file visually)
        new_content.push_str(&format!("# Group: {}\n", group_name));
        
        // Process all hosts in this group in their new order
        for group_host in group_hosts {
            if processed_hosts.contains(&group_host) {
                continue;
            }
            
            processed_hosts.insert(group_host.clone());
            
            // Find the position of this host in the original file
            if let Some(pos) = host_positions.iter().find(|(_, h)| h == &group_host) {
                let start_idx = pos.0;
                
                // Find the end of this host block (next Host entry or end of file)
                let mut end_idx = start_idx + 1;
                while end_idx < lines.len() {
                    if lines[end_idx].trim().starts_with("Host ") {
                        break;
                    }
                    end_idx += 1;
                }
                
                // Add all lines for this host
                for i in start_idx..end_idx {
                    new_content.push_str(&lines[i]);
                    new_content.push('\n');
                }
                
                // Add an extra newline for readability
                new_content.push('\n');
            }
        }
        
        // Add an extra newline between groups for readability
        new_content.push('\n');
    }
    
    // Add any hosts that weren't processed yet (should not happen normally)
    for (idx, host) in &host_positions {
        if !processed_hosts.contains(host) && !host.contains("*") {
            let start_idx = *idx;
            
            // Find the end of this host block
            let mut end_idx = start_idx + 1;
            while end_idx < lines.len() {
                if lines[end_idx].trim().starts_with("Host ") {
                    break;
                }
                end_idx += 1;
            }
            
            // Add all lines for this host
            for i in start_idx..end_idx {
                new_content.push_str(&lines[i]);
                new_content.push('\n');
            }
            
            new_content.push('\n');
            processed_hosts.insert(host.to_string());
        }
    }
    
    // Write the new content to the file
    let mut file = fs::File::create(&config_path)?;
    file.write_all(new_content.as_bytes())?;
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
                AppState::Normal => GroupsWidget::render(&app, layout.chunks_top[0], frame),
                AppState::Searching => app.searcher.render(&app, layout.chunks_top[0], frame),
                AppState::Editing => GroupsWidget::render(&app, layout.chunks_top[0], frame),
            };

            HelpWidget::render(&mut app, layout.chunks_top[2], frame);
            HostsWidget::render(&mut app, layout.chunks_bot[0], frame);
            ConfigWidget::render(&mut app, layout.chunks_bot[2], frame);

            if app.show_help {
                ShortcutsWidget::render(&app, layout.chunks_bot[4], frame);
            }
        })?;

        handle_inputs(&mut app)?;

        if app.should_quit || app.should_spawn_ssh {
            break;
        }
        
        // Save config if needed without exiting
        if app.should_save_config {
            save_ssh_config(&app)?;
            app.should_save_config = false;
        }
    }

    restore_terminal(&mut terminal)?;

    if app.should_spawn_ssh {
        let selected_config = app.get_selected_item().unwrap();
        let host_name = &selected_config.full_name;

        app.db.save_host_values(
            host_name,
            selected_config.connection_count + 1,
            chrono::offset::Local::now().timestamp(),
        )?;

        Command::new("ssh")
            .arg(host_name.split(' ').take(1).collect::<Vec<&str>>().join(""))
            .spawn()?
            .wait()?;
    }

    if app.should_copy_ssh_key {
        let selected_config = app.get_selected_item().unwrap();
        let host_name = &selected_config.full_name;

        Command::new("ssh-copy-id")
            .arg(host_name.split(' ').take(1).collect::<Vec<&str>>().join(""))
            .spawn()?
            .wait()?;
    }

    if app.should_copy_files {
        let selected_config = app.get_selected_item().unwrap();
        let host_name = &selected_config.full_name;

        Command::new("sftp")
            .arg(host_name.split(' ').take(1).collect::<Vec<&str>>().join(""))
            .spawn()?
            .wait()?;
    }

    Ok(())
}
