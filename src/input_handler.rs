use crossterm::event::{self, Event, KeyCode, KeyModifiers};

use crate::app::{App, AppState};

pub fn handle_inputs(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    if let Event::Key(key) = event::read()? {
        match app.state {
            AppState::Normal => handle_input_normal_mode(app, key.code),
            AppState::Searching => handle_input_search_mode(app, key.code, key.modifiers),
        };

        // Process navigation keys in normal mode
        if matches!(app.state, AppState::Normal) {
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
            };
        } else if matches!(app.state, AppState::Searching) {
            // Handle navigation in search mode
            match key.code {
                // Use Alt+j/k for navigation in search mode
                KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::ALT) => {
                    app.change_selected_item(true);
                }
                KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::ALT) => {
                    app.change_selected_item(false);
                }
                KeyCode::Enter => {
                    if app.get_selected_item().is_some() {
                        app.should_spawn_ssh = true;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn handle_input_search_mode(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    match key {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.searcher.clear_search();
            app.state = AppState::Normal;
        }
        // Only add character to search if not using Alt modifier
        // This allows Alt+j/k to be used for navigation in search mode
        KeyCode::Char(c) if !modifiers.contains(KeyModifiers::ALT) => app.searcher.add_char(c),
        KeyCode::Backspace => app.searcher.del_char(),
        _ => {}
    }
}

fn handle_input_normal_mode(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('c') => app.toggle_config_display_mode(),
        KeyCode::Char('?') => app.show_help = !app.show_help,
        KeyCode::Char('s') | KeyCode::Char('/') => app.state = AppState::Searching,
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
