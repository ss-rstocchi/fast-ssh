use crossterm::event::{self, Event, KeyCode, KeyModifiers};

use crate::app::{App, AppState};

pub fn handle_inputs(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    if let Event::Key(key) = event::read()? {
        match app.state {
            AppState::Normal => {
                handle_input_normal_mode(app, key.code, key.modifiers);
            }
            AppState::Searching => {
                handle_input_search_mode(app, key.code, key.modifiers);
            }
        };
    }
    Ok(())
}

/// Handle input in normal mode
fn handle_input_normal_mode(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    // Handle mode-specific commands first
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
        _ => {
            // Handle navigation
            handle_normal_mode_navigation(app, key, modifiers);
        }
    }
}

/// Handle navigation in normal mode
#[inline]
fn handle_normal_mode_navigation(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    match key {
        // Group navigation
        KeyCode::Tab => app.change_selected_group(true),
        KeyCode::BackTab => app.change_selected_group(false),
        KeyCode::Left | KeyCode::Char('h') => app.change_selected_group(false),
        KeyCode::Right | KeyCode::Char('l') => app.change_selected_group(true),
        KeyCode::Char(' ') => app.select_recents_group(),
        
        // Item navigation
        KeyCode::Down | KeyCode::Char('j') => app.change_selected_item(true),
        KeyCode::Up | KeyCode::Char('k') => app.change_selected_item(false),
        
        // Config scrolling
        KeyCode::PageDown => app.scroll_config_paragraph(1),
        KeyCode::PageUp => app.scroll_config_paragraph(-1),
        
        // Enter to connect
        KeyCode::Enter => {
            if app.get_selected_item().is_some() {
                app.should_spawn_ssh = true;
            }
            app.pending_g = false;
        }
        
        _ => {
            // Handle common vim-like navigation
            handle_vim_navigation(app, key, modifiers);
        }
    }
}

/// Handle input in search mode
fn handle_input_search_mode(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    // Exit search mode
    if matches!(key, KeyCode::Esc | KeyCode::Char('q')) {
        app.searcher.clear_search();
        app.state = AppState::Normal;
        return;
    }

    // Handle Enter key for search commit or connection
    if key == KeyCode::Enter {
        if !app.searcher.is_committed() {
            app.searcher.commit_search();
            app.host_state.select(Some(0));
        } else if app.get_selected_item().is_some() {
            app.should_spawn_ssh = true;
        }
        app.pending_g = false;
        return;
    }

    // Only allow typing if search is not committed
    if !app.searcher.is_committed() {
        match key {
            KeyCode::Char(c) if !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) => {
                app.searcher.add_char(c);
            }
            KeyCode::Backspace => {
                app.searcher.del_char();
            }
            _ => {}
        }
    } else {
        // When search is committed, handle navigation
        handle_search_mode_navigation(app, key, modifiers);
    }
}

/// Handle navigation when search is committed
#[inline]
fn handle_search_mode_navigation(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    match key {
        // Arrow keys always work
        KeyCode::Down => app.change_selected_item(true),
        KeyCode::Up => app.change_selected_item(false),
        
        // Alt+j/k for navigation
        KeyCode::Char('j') if modifiers.contains(KeyModifiers::ALT) => {
            app.change_selected_item(true);
        }
        KeyCode::Char('k') if modifiers.contains(KeyModifiers::ALT) => {
            app.change_selected_item(false);
        }
        
        // n/N for next/previous (vim-style)
        KeyCode::Char('n') => app.change_selected_item(true),
        KeyCode::Char('N') => app.change_selected_item(false),
        
        _ => {
            // Handle common vim-like navigation
            handle_vim_navigation(app, key, modifiers);
        }
    }
}

/// Handle vim-like navigation common to both modes
#[inline]
fn handle_vim_navigation(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    match key {
        // gg to go to top
        KeyCode::Char('g') => {
            if app.pending_g {
                app.jump_to_first_item();
                app.pending_g = false;
            } else {
                app.pending_g = true;
            }
        }
        // G to go to bottom
        KeyCode::Char('G') => {
            app.jump_to_last_item();
            app.pending_g = false;
        }
        // Ctrl+d for half-page down
        KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.scroll_half_page(true);
            app.pending_g = false;
        }
        // Ctrl+u for half-page up
        KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.scroll_half_page(false);
            app.pending_g = false;
        }
        _ => {
            // Reset pending_g on any other key
            app.pending_g = false;
        }
    }
}
