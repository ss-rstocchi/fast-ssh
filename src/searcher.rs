use crate::{app::App, ssh_config_store::SshGroupItem, widgets::block, THEME};
use std::io::Stdout;
use sublime_fuzzy::best_match;
use tui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::Paragraph,
    Frame,
};

pub struct Searcher {
    search_string: String,
    has_user_input: bool,
    is_committed: bool,
}

impl Searcher {
    pub fn new() -> Searcher {
        Searcher {
            search_string: String::new(),
            has_user_input: false,
            is_committed: false,
        }
    }

    pub fn is_committed(&self) -> bool {
        self.is_committed
    }

    pub fn commit_search(&mut self) {
        self.is_committed = true;
    }

    pub fn get_filtered_items<'a>(&self, app: &'a App) -> Vec<&'a SshGroupItem> {
        if self.search_string.is_empty() {
            return app.get_all_items_except_recents();
        }

        app.get_all_items_except_recents()
            .into_iter()
            .filter(|item| {
                // Check host name match
                if best_match(&self.search_string, &item.full_name).is_some() {
                    return true;
                }

                // Check hostname parameter match
                for (key, value) in item.host_config.iter() {
                    if key.to_string().to_lowercase() == "hostname"
                        && best_match(&self.search_string, value).is_some()
                    {
                        return true;
                    }
                }

                // Check notes/comments match
                if let Some(comment) = &item.comment {
                    if best_match(&self.search_string, comment).is_some() {
                        return true;
                    }
                }

                false
            })
            .collect::<Vec<&SshGroupItem>>()
    }

    pub fn add_char(&mut self, c: char) {
        self.search_string.push(c);
        self.has_user_input = true;
    }

    pub fn del_char(&mut self) {
        self.search_string.pop();
    }

    pub fn clear_search(&mut self) {
        self.search_string.clear();
        self.has_user_input = false;
        self.is_committed = false;
    }

    pub fn render(&self, _app: &App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let block = block::new(" Search ");

        let spans = if self.is_committed {
            // Show navigation hint when committed
            Spans::from(vec![
                Span::styled(" > ", Style::default().fg(THEME.text_primary())),
                Span::styled(
                    &self.search_string,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " [n/N to navigate]",
                    Style::default().fg(THEME.text_primary()).add_modifier(Modifier::DIM),
                ),
            ])
        } else {
            // Show typing mode
            Spans::from(vec![
                Span::styled(" > ", Style::default().fg(THEME.text_primary())),
                Span::styled(
                    &self.search_string,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "_",
                    Style::default().add_modifier(Modifier::SLOW_BLINK),
                ),
            ])
        };

        let paragraph = Paragraph::new(spans).block(block);

        frame.render_widget(paragraph, area);
    }
}
