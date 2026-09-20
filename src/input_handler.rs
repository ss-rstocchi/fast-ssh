use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::app::{App, AppState};

pub fn handle_inputs(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    match event::read()? {
        // On Windows crossterm reports key releases as well; processing them
        // would apply every keystroke twice.
        Event::Key(key) if key.kind == KeyEventKind::Release => {}
        // Raw mode swallows SIGINT, so Ctrl+C must be handled here or it's
        // dead in search mode and toggles the config pane in normal mode
        Event::Key(key)
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            app.should_quit = true;
        }
        Event::Key(key) => match app.state {
            AppState::Normal => handle_input_normal_mode(app, key.code, key.modifiers),
            AppState::Searching => handle_input_search_mode(app, key.code, key.modifiers),
        },
        Event::Resize(_, _) => {}
        _ => {}
    }
    Ok(())
}

/// Handle input in normal mode
fn handle_input_normal_mode(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    // Any key other than `g` cancels a pending `gg`
    if key != KeyCode::Char('g') {
        app.pending_g = false;
    }

    // Handle mode-specific commands first
    match key {
        KeyCode::Char('c') => app.toggle_config_display_mode(),
        KeyCode::Char('?') => app.show_help = !app.show_help,
        KeyCode::Char('s') | KeyCode::Char('/') => app.enter_search_mode(),
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
        }

        _ => {
            // Handle common vim-like navigation
            handle_vim_navigation(app, key, modifiers);
        }
    }
}

/// Handle input in search mode: typing filters live, arrows move the
/// selection, Enter connects to the highlighted (top-ranked) result
fn handle_input_search_mode(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    match key {
        KeyCode::Esc => app.exit_search_mode(),
        KeyCode::Enter => {
            if app.get_selected_item().is_some() {
                app.should_spawn_ssh = true;
            }
        }
        KeyCode::Down => app.change_selected_item(true),
        KeyCode::Up => app.change_selected_item(false),
        KeyCode::Char('j' | 'n') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.change_selected_item(true)
        }
        KeyCode::Char('k' | 'p') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.change_selected_item(false)
        }
        KeyCode::Char(c) if !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) => {
            app.search_add_char(c)
        }
        KeyCode::Backspace => app.search_del_char(),
        _ => {}
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
        KeyCode::Char('G') => app.jump_to_last_item(),
        // Ctrl+d for half-page down
        KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.scroll_half_page(true);
        }
        // Ctrl+u for half-page up
        KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.scroll_half_page(false);
        }
        _ => {}
    }
}
