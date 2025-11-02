use crate::{app::App, get_theme, ssh_config_store::SshGroupItem, widgets::block};
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
    is_committed: bool,
}

impl Default for Searcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Searcher {
    pub fn new() -> Searcher {
        Searcher {
            search_string: String::new(),
            is_committed: false,
        }
    }

    #[inline]
    pub fn is_committed(&self) -> bool {
        self.is_committed
    }

    #[inline]
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

                // Check hostname parameter match - use case-insensitive comparison without allocation
                let has_hostname_match = item.host_config.iter().any(|(key, value)| {
                    key.to_string().eq_ignore_ascii_case("hostname")
                        && best_match(&self.search_string, value).is_some()
                });
                
                if has_hostname_match {
                    return true;
                }

                // Check notes/comments match
                item.comment
                    .as_ref()
                    .map_or(false, |comment| best_match(&self.search_string, comment).is_some())
            })
            .collect()
    }

    pub fn add_char(&mut self, c: char) {
        self.search_string.push(c);
    }

    pub fn del_char(&mut self) {
        self.search_string.pop();
    }

    pub fn clear_search(&mut self) {
        self.search_string.clear();
        self.is_committed = false;
    }

    pub fn render(&self, _app: &App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let block = block::new(" Search ");

        let spans = if self.is_committed {
            // Show navigation hint when committed
            Spans::from(vec![
                Span::styled(" > ", Style::default().fg(get_theme().text_primary())),
                Span::styled(
                    &self.search_string,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " [n/N to navigate]",
                    Style::default().fg(get_theme().text_primary()).add_modifier(Modifier::DIM),
                ),
            ])
        } else {
            // Show typing mode
            Spans::from(vec![
                Span::styled(" > ", Style::default().fg(get_theme().text_primary())),
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
