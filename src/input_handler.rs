use crossterm::event::{self, Event, KeyCode};

use crate::{app::{App, AppState}, save_ssh_config};

pub fn handle_inputs(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    if let Event::Key(key) = event::read()? {
        match app.state {
            AppState::Normal => {
                handle_input_normal_mode(app, key.code);
                
                // Global keybindings for normal mode
                match key.code {
                    KeyCode::Tab => app.change_selected_group(true),
                    KeyCode::BackTab => app.change_selected_group(false),
                    KeyCode::Left | KeyCode::Char('h') => app.change_selected_group(false),
                    KeyCode::Right | KeyCode::Char('l') => app.change_selected_group(true),
                    KeyCode::Down | KeyCode::Char('j') => app.change_selected_item(true),
                    KeyCode::Up | KeyCode::Char('k') => app.change_selected_item(false),
                    KeyCode::PageDown => app.scroll_config_paragraph(1),
                    KeyCode::PageUp => app.scroll_config_paragraph(-1),
                    KeyCode::Char(' ') => app.select_recents_group(),
                    KeyCode::Enter => {
                        if app.get_selected_item().is_some() {
                            app.should_spawn_ssh = true;
                        }
                    }
                    _ => {}
                }
            },
            AppState::Searching => handle_input_search_mode(app, key.code),
            AppState::Editing => handle_input_edit_mode(app, key.code),
        };
    }
    Ok(())
}

fn handle_input_search_mode(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.searcher.clear_search();
            app.state = AppState::Normal;
        }
        KeyCode::Char(c) => app.searcher.add_char(c),
        KeyCode::Backspace => app.searcher.del_char(),
        _ => {}
    }
}

fn handle_input_edit_mode(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.state = AppState::Normal;
            if app.should_save_config {
                if let Err(e) = save_ssh_config(app) {
                    eprintln!("Error saving SSH config: {}", e);
                }
                app.should_save_config = false;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(selected) = app.host_state.selected() {
                if selected > 0 {
                    let group_name = app.get_selected_group().name.clone();
                    if let Some(group_idx) = app.scs.groups.iter().position(|g| g.name == group_name) {
                        if group_name != "Recents" {
                            app.scs.groups[group_idx].items.swap(selected, selected - 1);
                            app.host_state.select(Some(selected - 1));
                            app.should_save_config = true;
                        }
                    }
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(selected) = app.host_state.selected() {
                let items_count = app.get_items_based_on_mode().len();
                if selected < items_count - 1 {
                    let group_name = app.get_selected_group().name.clone();
                    if let Some(group_idx) = app.scs.groups.iter().position(|g| g.name == group_name) {
                        if group_name != "Recents" {
                            app.scs.groups[group_idx].items.swap(selected, selected + 1);
                            app.host_state.select(Some(selected + 1));
                            app.should_save_config = true;
                        }
                    }
                }
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            move_host_between_groups(app, false);
        },
        KeyCode::Right | KeyCode::Char('l') => {
            move_host_between_groups(app, true);
        },
        KeyCode::Char('H') => {
            move_group(app, false);
        },
        KeyCode::Char('L') => {
            move_group(app, true);
        },
        KeyCode::Tab | KeyCode::BackTab => {},
        KeyCode::Enter => {
            if app.should_save_config {
                if let Err(e) = save_ssh_config(app) {
                    eprintln!("Error saving SSH config: {}", e);
                }
                app.should_save_config = false;
            }
        }
        _ => {}
    }
}

// Function to move a host from one group to another
fn move_host_between_groups(app: &mut App, move_right: bool) {
    if let Some(selected) = app.host_state.selected() {
        let current_group_name = app.get_selected_group().name.clone();
        if current_group_name == "Recents" {
            return;
        }
        if let Some(current_group_idx) = app.scs.groups.iter().position(|g| g.name == current_group_name) {
            let groups_len = app.scs.groups.len();
            let skip_recents = app.scs.groups.get(0).map_or(false, |g| g.name == "Recents");
            let effective_idx = if skip_recents && current_group_idx > 0 {
                current_group_idx - 1
            } else {
                current_group_idx
            };
            let target_group_idx = if move_right {
                let next_idx = (effective_idx + 1) % (groups_len - (if skip_recents { 1 } else { 0 }));
                if skip_recents { next_idx + 1 } else { next_idx }
            } else {
                let effective_groups_len = groups_len - (if skip_recents { 1 } else { 0 });
                let prev_idx = (effective_idx + effective_groups_len - 1) % effective_groups_len;
                if skip_recents { prev_idx + 1 } else { prev_idx }
            };
            if app.scs.groups[target_group_idx].name == "Recents" {
                return;
            }
            if let Some(item) = app.scs.groups[current_group_idx].items.get(selected).cloned() {
                let mut new_item = item.clone();
                if item.full_name.contains('/') {
                    let name_part = item.full_name.split('/').skip(1).collect::<Vec<&str>>().join("");
                    new_item.full_name = format!("{}/{}", app.scs.groups[target_group_idx].name, name_part);
                    new_item.name = name_part;
                } else {
                    new_item.full_name = format!("{}/{}", app.scs.groups[target_group_idx].name, item.name);
                }
                app.scs.groups[current_group_idx].items.remove(selected);
                let new_index = app.scs.groups[target_group_idx].items.len();
                app.scs.groups[target_group_idx].items.push(new_item);
                app.selected_group = target_group_idx;
                app.host_state.select(Some(new_index));
                app.should_save_config = true;
            }
        }
    }
}

// Function to move a group left or right in the order
fn move_group(app: &mut App, move_right: bool) {
    // Get the current group
    let current_group_name = app.get_selected_group().name.clone();

    // Don't allow moving the Recents group
    if current_group_name == "Recents" {
        return;
    }

    // Find current group index
    if let Some(current_group_idx) = app.scs.groups.iter().position(|g| g.name == current_group_name) {
        // Create a list of non-Recents groups' indices for proper wrapping
        let non_recents_indices: Vec<usize> = app.scs.groups.iter()
            .enumerate()
            .filter(|(_, g)| g.name != "Recents")
            .map(|(idx, _)| idx)
            .collect();

        // Can't proceed if there are fewer than 2 non-Recents groups
        if non_recents_indices.len() < 2 {
            return;
        }

        // Find position of current group in the non-Recents list
        if let Some(current_pos) = non_recents_indices.iter().position(|&idx| idx == current_group_idx) {
            // Remember the current group
            let current_group = app.scs.groups[current_group_idx].clone();

            // Remove the current group
            app.scs.groups.remove(current_group_idx);

            // Calculate new position with wrap-around
            let new_pos = if move_right {
                // Move right and wrap around to start if at end
                (current_pos + 1) % non_recents_indices.len()
            } else {
                // Move left and wrap around to end if at start
                (current_pos + non_recents_indices.len() - 1) % non_recents_indices.len()
            };

            // After removal, recalculate filtered indices (non-Recents)
            let filtered_indices: Vec<usize> = app.scs.groups.iter()
                .enumerate()
                .filter(|(_, g)| g.name != "Recents")
                .map(|(idx, _)| idx)
                .collect();

            // Insert the group at the new filtered position
            let insert_at = if filtered_indices.is_empty() {
                0
            } else if new_pos >= filtered_indices.len() {
                // Should not happen, but fallback to end
                *filtered_indices.last().unwrap() + 1
            } else {
                filtered_indices[new_pos]
            };

            app.scs.groups.insert(insert_at, current_group);

            // Update selected_group to point to the new position
            app.selected_group = insert_at;

            // Mark config as needing save
            app.should_save_config = true;
        }
    }
}

fn handle_input_normal_mode(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('c') => app.toggle_config_display_mode(),
        KeyCode::Char('?') => app.show_help = !app.show_help,
        KeyCode::Char('s') | KeyCode::Char('/') => app.state = AppState::Searching,
        KeyCode::Char('e') => {
            app.state = AppState::Editing;
        },
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('K') => {
            if app.get_selected_item().is_some() {
                app.should_copy_ssh_key = true;
                app.should_quit = true;
            }
        }
        KeyCode::Char('C') => {
            if app.get_selected_item().is_some() {
                app.should_copy_files = true;
                app.should_quit = true;
            }
        }
        _ => {}
    }
}
