use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::app::{App, AppState};

/// Waits for the next terminal event and applies it. Returns whether the
/// screen needs to be redrawn.
pub fn handle_inputs(app: &mut App) -> Result<bool> {
    let redraw = match event::read()? {
        // On Windows crossterm reports key releases as well; processing them
        // would apply every keystroke twice.
        Event::Key(key) if key.kind == KeyEventKind::Release => false,
        // Raw mode swallows SIGINT, so Ctrl+C must be handled here or it's
        // dead in search mode and toggles the config pane in normal mode
        Event::Key(key)
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            app.should_quit = true;
            false
        }
        Event::Key(key) => {
            match app.state {
                AppState::Normal => handle_input_normal_mode(app, key.code, key.modifiers),
                AppState::Searching if app.search_browsing => {
                    handle_input_search_results(app, key.code, key.modifiers)
                }
                AppState::Searching => handle_input_search_mode(app, key.code, key.modifiers),
            }
            true
        }
        Event::Resize(_, _) => true,
        // Mouse, focus and paste events change nothing
        _ => false,
    };
    Ok(redraw)
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
/// selection, Enter connects to the highlighted (top-ranked) result and Esc
/// moves focus to the results for vim navigation
fn handle_input_search_mode(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    match key {
        KeyCode::Esc => app.search_browsing = true,
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
        KeyCode::Char('w') if modifiers.contains(KeyModifiers::CONTROL) => app.search_del_word(),
        KeyCode::Backspace if modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) => {
            app.search_del_word()
        }
        KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => app.search_clear(),
        KeyCode::Char(c) if !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) => {
            app.search_add_char(c)
        }
        KeyCode::Backspace => app.search_del_char(),
        _ => {}
    }
}

/// Handle input on the search results, after Esc left the query: vim keys
/// move the selection, `i`, `a` or `/` edit the query again and Esc leaves
/// search with the selected host selected in its group
fn handle_input_search_results(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    if key != KeyCode::Char('g') {
        app.pending_g = false;
    }

    match key {
        KeyCode::Esc => app.exit_search_mode(),
        KeyCode::Char('i' | 'a' | '/') if modifiers.is_empty() => app.search_browsing = false,
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Enter => {
            if app.get_selected_item().is_some() {
                app.should_spawn_ssh = true;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => app.change_selected_item(true),
        KeyCode::Up | KeyCode::Char('k') => app.change_selected_item(false),
        // Ctrl+j/k reach the arms above, as modifiers are not checked there
        KeyCode::Char('n') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.change_selected_item(true)
        }
        KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.change_selected_item(false)
        }
        _ => handle_vim_navigation(app, key, modifiers),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{groups, test_app};

    fn press(app: &mut App, keys: &str) {
        for c in keys.chars() {
            let key = KeyCode::Char(c);
            match app.state {
                AppState::Normal => handle_input_normal_mode(app, key, KeyModifiers::NONE),
                AppState::Searching if app.search_browsing => {
                    handle_input_search_results(app, key, KeyModifiers::NONE)
                }
                AppState::Searching => handle_input_search_mode(app, key, KeyModifiers::NONE),
            }
        }
    }

    fn esc(app: &mut App) {
        match app.search_browsing {
            true => handle_input_search_results(app, KeyCode::Esc, KeyModifiers::NONE),
            false => handle_input_search_mode(app, KeyCode::Esc, KeyModifiers::NONE),
        }
    }

    #[test]
    fn test_search_vim_keys_after_esc() {
        let mut app = test_app(groups());
        press(&mut app, "/");
        // j and k are typed into the query
        press(&mut app, "jk");
        assert_eq!(app.searcher.search_string(), "jk");
        handle_input_search_mode(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);

        // Esc focuses the results: j/k/G/gg navigate, the query is unchanged
        press(&mut app, "d");
        esc(&mut app);
        assert!(app.search_browsing);
        assert_eq!(app.search_result_count(), 4);
        press(&mut app, "jj");
        assert_eq!(app.host_state.selected(), Some(2));
        press(&mut app, "k");
        assert_eq!(app.host_state.selected(), Some(1));
        press(&mut app, "G");
        assert_eq!(app.host_state.selected(), Some(3));
        press(&mut app, "gg");
        assert_eq!(app.host_state.selected(), Some(0));
        assert_eq!(app.searcher.search_string(), "d");

        // i goes back to typing
        press(&mut app, "ib");
        assert!(!app.search_browsing);
        assert_eq!(app.searcher.search_string(), "db");

        // A second Esc leaves search with the host selected in its group
        esc(&mut app);
        esc(&mut app);
        assert!(matches!(app.state, AppState::Normal));
        assert!(!app.search_browsing);
        assert_eq!(app.get_selected_item().unwrap().name, "db");
    }

    #[test]
    fn test_entering_search_starts_typing() {
        let mut app = test_app(groups());
        press(&mut app, "/");
        esc(&mut app);
        press(&mut app, "q");
        assert!(app.should_quit);

        let mut app = test_app(groups());
        press(&mut app, "/");
        esc(&mut app);
        esc(&mut app);
        press(&mut app, "/");
        assert!(!app.search_browsing);
    }
}
