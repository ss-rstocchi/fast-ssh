use crate::{
    get_theme,
    ssh_config_store::{SshGroup, SshGroupItem, RECENTS_GROUP},
    widgets::block,
};
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

    pub fn search_string(&self) -> &str {
        &self.search_string
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
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            Span::styled(
                " [↑↓ navigate · Enter connect]",
                Style::default()
                    .fg(get_theme().text_primary())
                    .add_modifier(Modifier::DIM),
            ),
        ]);

        let paragraph = Paragraph::new(spans).block(block);

        frame.render_widget(paragraph, area);
    }
}

/// Returns `(group index, item index)` pairs for every host matching `query`,
/// ranked best-first. Recents are excluded. An empty query matches every host.
pub fn rank_matches(query: &str, groups: &[SshGroup]) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.name != RECENTS_GROUP)
            .flat_map(|(group_idx, group)| {
                (0..group.items.len()).map(move |item_idx| (group_idx, item_idx))
            })
            .collect();
    }

    let mut scored: Vec<(isize, i64, (usize, usize))> = Vec::new();

    for (group_idx, group) in groups.iter().enumerate() {
        if group.name == RECENTS_GROUP {
            continue;
        }

        let group_score = best_match(query, &group.name).map(|m| m.score());

        for (item_idx, item) in group.items.iter().enumerate() {
            if let Some(score) = item_score(query, item).or(group_score) {
                scored.push((score, item.connection_count, (group_idx, item_idx)));
            }
        }
    }

    // Best match first; connection count breaks ties so daily hosts float up
    scored.sort_by_key(|(score, connection_count, _)| (-score, -connection_count));
    scored.into_iter().map(|(_, _, idx)| idx).collect()
}

fn item_score(query: &str, item: &SshGroupItem) -> Option<isize> {
    let name = best_match(query, &item.full_name).map(|m| m.score());

    let hostname = item
        .host_config
        .get(&SshOptionKey::Hostname)
        .and_then(|value| best_match(query, value).map(|m| m.score()));

    let comment = item
        .comment
        .as_ref()
        .and_then(|c| best_match(query, c).map(|m| m.score()));

    [name, hostname, comment].into_iter().flatten().max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(full_name: &str, connection_count: i64) -> SshGroupItem {
        SshGroupItem {
            name: full_name.to_string(),
            full_name: full_name.to_string(),
            connection_count,
            last_used: 0,
            host_config: ssh_cfg::SshHostConfig::default(),
            comment: None,
        }
    }

    fn make_group(name: &str, items: Vec<SshGroupItem>) -> SshGroup {
        SshGroup {
            name: name.to_string(),
            items,
        }
    }

    #[test]
    fn test_searcher_new() {
        let searcher = Searcher::new();
        assert_eq!(searcher.search_string(), "");
    }

    #[test]
    fn test_searcher_default() {
        let searcher = Searcher::default();
        assert_eq!(searcher.search_string(), "");
    }

    #[test]
    fn test_add_char() {
        let mut searcher = Searcher::new();
        searcher.add_char('h');
        searcher.add_char('e');
        searcher.add_char('l');
        searcher.add_char('l');
        searcher.add_char('o');
        assert_eq!(searcher.search_string(), "hello");
    }

    #[test]
    fn test_del_char() {
        let mut searcher = Searcher::new();
        searcher.add_char('h');
        searcher.add_char('i');
        assert_eq!(searcher.search_string(), "hi");

        searcher.del_char();
        assert_eq!(searcher.search_string(), "h");

        searcher.del_char();
        assert_eq!(searcher.search_string(), "");

        // Deleting from empty string should not panic
        searcher.del_char();
        assert_eq!(searcher.search_string(), "");
    }

    #[test]
    fn test_clear_search() {
        let mut searcher = Searcher::new();
        searcher.add_char('t');
        searcher.add_char('e');
        searcher.add_char('s');
        searcher.add_char('t');

        assert_eq!(searcher.search_string(), "test");

        searcher.clear_search();
        assert_eq!(searcher.search_string(), "");
    }

    #[test]
    fn test_item_score_ranks_tight_match_higher() {
        let tight = item_score("prod", &make_item("prod-web-01", 0)).unwrap();
        let scattered = item_score("prod", &make_item("deploy-runner-old", 0)).unwrap();
        assert!(tight > scattered);
        assert!(item_score("prod", &make_item("staging-db", 0)).is_none());
    }

    #[test]
    fn test_rank_matches_excludes_recents() {
        let groups = vec![
            make_group(RECENTS_GROUP, vec![make_item("recent-host", 10)]),
            make_group("Prod", vec![make_item("prod-web", 0)]),
        ];

        assert_eq!(rank_matches("", &groups), vec![(1, 0)]);
        assert_eq!(rank_matches("prod", &groups), vec![(1, 0)]);
        assert!(rank_matches("recent", &groups).is_empty());
    }

    #[test]
    fn test_rank_matches_orders_by_score_then_connections() {
        let groups = vec![make_group(
            "Prod",
            vec![
                make_item("deploy-runner-old", 100),
                make_item("prod-web-01", 0),
            ],
        )];

        // "prod" matches prod-web-01 tightly; deploy-runner-old matches the group name
        assert_eq!(rank_matches("prod", &groups), vec![(0, 1), (0, 0)]);

        // Equal scores fall back to connection count, highest first
        let groups = vec![make_group(
            "Prod",
            vec![make_item("prod-a", 1), make_item("prod-b", 9)],
        )];
        assert_eq!(rank_matches("prod", &groups), vec![(0, 1), (0, 0)]);
    }

    #[test]
    fn test_unicode_support() {
        let mut searcher = Searcher::new();
        searcher.add_char('こ');
        searcher.add_char('ん');
        searcher.add_char('に');
        searcher.add_char('ち');
        searcher.add_char('は');
        assert_eq!(searcher.search_string(), "こんにちは");

        searcher.del_char();
        assert_eq!(searcher.search_string(), "こんにち");
    }

    #[test]
    fn test_emoji_support() {
        let mut searcher = Searcher::new();
        searcher.add_char('🚀');
        searcher.add_char('🎉');
        assert_eq!(searcher.search_string(), "🚀🎉");

        searcher.del_char();
        assert_eq!(searcher.search_string(), "🚀");
    }
}
