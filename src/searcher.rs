use crate::{app::App, get_theme, ssh_config_store::{SshGroupItem, RECENTS_GROUP}, widgets::block};
use ssh_cfg::SshOptionKey;
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
        }
    }

    pub fn get_filtered_items<'a>(&self, app: &'a App) -> Vec<&'a SshGroupItem> {
        if self.search_string.is_empty() {
            return app.get_all_items_except_recents();
        }

        let mut scored: Vec<(isize, &SshGroupItem)> = app.scs.groups
            .iter()
            .filter(|g| g.name != RECENTS_GROUP)
            .flat_map(|group| {
                let group_score = best_match(&self.search_string, &group.name).map(|m| m.score());
                group.items.iter().filter_map(move |item| {
                    self.item_score(item).or(group_score).map(|s| (s, item))
                })
            })
            .collect();

        // Best match first; connection count breaks ties so daily hosts float up
        scored.sort_by_key(|(score, item)| (-score, -item.connection_count));
        scored.into_iter().map(|(_, item)| item).collect()
    }

    fn item_score(&self, item: &SshGroupItem) -> Option<isize> {
        let name = best_match(&self.search_string, &item.full_name).map(|m| m.score());

        let hostname = item.host_config.iter()
            .filter(|(key, _)| **key == SshOptionKey::Hostname)
            .filter_map(|(_, value)| best_match(&self.search_string, value).map(|m| m.score()))
            .max();

        let comment = item.comment.as_ref()
            .and_then(|c| best_match(&self.search_string, c).map(|m| m.score()));

        [name, hostname, comment].into_iter().flatten().max()
    }

    pub fn add_char(&mut self, c: char) {
        self.search_string.push(c);
    }

    pub fn del_char(&mut self) {
        self.search_string.pop();
    }

    pub fn clear_search(&mut self) {
        self.search_string.clear();
    }

    pub fn render(&self, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let block = block::new(" Search ");

        let spans = Spans::from(vec![
            Span::styled(" > ", Style::default().fg(get_theme().text_primary())),
            Span::styled(
                &self.search_string,
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "_",
                Style::default().add_modifier(Modifier::SLOW_BLINK),
            ),
            Span::styled(
                " [↑↓ navigate · Enter connect]",
                Style::default().fg(get_theme().text_primary()).add_modifier(Modifier::DIM),
            ),
        ]);

        let paragraph = Paragraph::new(spans).block(block);

        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_searcher_new() {
        let searcher = Searcher::new();
        assert_eq!(searcher.search_string, "");
    }

    #[test]
    fn test_searcher_default() {
        let searcher = Searcher::default();
        assert_eq!(searcher.search_string, "");
    }

    #[test]
    fn test_add_char() {
        let mut searcher = Searcher::new();
        searcher.add_char('h');
        searcher.add_char('e');
        searcher.add_char('l');
        searcher.add_char('l');
        searcher.add_char('o');
        assert_eq!(searcher.search_string, "hello");
    }

    #[test]
    fn test_del_char() {
        let mut searcher = Searcher::new();
        searcher.add_char('h');
        searcher.add_char('i');
        assert_eq!(searcher.search_string, "hi");
        
        searcher.del_char();
        assert_eq!(searcher.search_string, "h");
        
        searcher.del_char();
        assert_eq!(searcher.search_string, "");
        
        // Deleting from empty string should not panic
        searcher.del_char();
        assert_eq!(searcher.search_string, "");
    }

    #[test]
    fn test_clear_search() {
        let mut searcher = Searcher::new();
        searcher.add_char('t');
        searcher.add_char('e');
        searcher.add_char('s');
        searcher.add_char('t');

        assert_eq!(searcher.search_string, "test");

        searcher.clear_search();
        assert_eq!(searcher.search_string, "");
    }

    #[test]
    fn test_item_score_ranks_tight_match_higher() {
        use crate::ssh_config_store::SshGroupItem;
        use ssh_cfg::SshHostConfig;

        let make = |full_name: &str| SshGroupItem {
            name: full_name.to_string(),
            full_name: full_name.to_string(),
            connection_count: 0,
            last_used: 0,
            host_config: SshHostConfig::default(),
            comment: None,
        };

        let mut searcher = Searcher::new();
        for c in "prod".chars() {
            searcher.add_char(c);
        }

        let tight = searcher.item_score(&make("prod-web-01")).unwrap();
        let scattered = searcher.item_score(&make("deploy-runner-old")).unwrap();
        assert!(tight > scattered);
        assert!(searcher.item_score(&make("staging-db")).is_none());
    }

    #[test]
    fn test_unicode_support() {
        let mut searcher = Searcher::new();
        searcher.add_char('こ');
        searcher.add_char('ん');
        searcher.add_char('に');
        searcher.add_char('ち');
        searcher.add_char('は');
        assert_eq!(searcher.search_string, "こんにちは");
        
        searcher.del_char();
        assert_eq!(searcher.search_string, "こんにち");
    }

    #[test]
    fn test_emoji_support() {
        let mut searcher = Searcher::new();
        searcher.add_char('🚀');
        searcher.add_char('🎉');
        assert_eq!(searcher.search_string, "🚀🎉");
        
        searcher.del_char();
        assert_eq!(searcher.search_string, "🚀");
    }
}
