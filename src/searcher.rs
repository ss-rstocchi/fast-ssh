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

/// Maximum connection count that still earns a frecency bonus.
const FRECENCY_COUNT_CAP: i64 = 10;
/// Score added per capped connection.
const FRECENCY_COUNT_WEIGHT: isize = 2;
/// `(max age in days, bonus)`; the first matching bucket wins.
const RECENCY_BONUSES: [(i64, isize); 4] = [(1, 30), (7, 20), (30, 10), (180, 5)];
const SECONDS_PER_DAY: i64 = 86_400;

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

    pub fn render(
        &self,
        area: Rect,
        frame: &mut Frame<CrosstermBackend<Stdout>>,
        result_count: usize,
    ) {
        let block = block::new(" Search ");
        let theme = get_theme();
        let hint_style = Style::default()
            .fg(theme.text_primary())
            .add_modifier(Modifier::DIM);

        let mut spans = vec![
            Span::styled(" > ", Style::default().fg(theme.text_primary())),
            Span::styled(
                &self.search_string,
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
        ];

        if !self.search_string.is_empty() {
            let noun = if result_count == 1 {
                "match"
            } else {
                "matches"
            };
            spans.push(Span::styled(
                format!("  [{} {}]", result_count, noun),
                hint_style,
            ));
        }

        spans.push(Span::styled(" [↑↓ navigate · Enter connect]", hint_style));

        let paragraph = Paragraph::new(Spans::from(spans)).block(block);

        frame.render_widget(paragraph, area);
    }
}

/// Returns `(group index, item index)` pairs for every host matching `query`,
/// ranked best-first. Recents are excluded. An empty query matches every host.
///
/// `query` is split on whitespace and every token must match the item's short
/// name, hostname, user or comment, or the group name, in any order. Matching
/// the short name keeps the `group/` prefix from skewing scores. Hosts in a
/// group whose name matches the query exactly or as a prefix are ranked before
/// every other match, so typing a group name surfaces that group first. `now`
/// is a Unix timestamp used to boost frequently/recently used hosts (frecency).
pub fn rank_matches(query: &str, groups: &[SshGroup], now: i64) -> Vec<(usize, usize)> {
    let tokens: Vec<&str> = query.split_whitespace().collect();

    if tokens.is_empty() {
        return groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.name != RECENTS_GROUP)
            .flat_map(|(group_idx, group)| {
                (0..group.items.len()).map(move |item_idx| (group_idx, item_idx))
            })
            .collect();
    }

    let mut scored: Vec<(u8, isize, i64, (usize, usize))> = Vec::new();

    for (group_idx, group) in groups.iter().enumerate() {
        if group.name == RECENTS_GROUP {
            continue;
        }

        // 0 = the group name matches the query, 1 = it does not
        let priority = u8::from(!group_matches(&tokens, &group.name));

        for (item_idx, item) in group.items.iter().enumerate() {
            let Some(fuzzy_score) = match_tokens(&tokens, item, &group.name) else {
                continue;
            };
            let total = fuzzy_score + frecency_bonus(item, now);
            scored.push((
                priority,
                total,
                item.connection_count,
                (group_idx, item_idx),
            ));
        }
    }

    // Group matches first, then best score; connection count breaks exact ties
    scored.sort_by_key(|(priority, total, connection_count, _)| {
        (*priority, -total, -connection_count)
    });
    scored.into_iter().map(|(_, _, _, idx)| idx).collect()
}

/// Whether any query token is a case-insensitive prefix of the group name.
fn group_matches(tokens: &[&str], group_name: &str) -> bool {
    let group_name = group_name.to_lowercase();
    tokens
        .iter()
        .any(|token| group_name.starts_with(&token.to_lowercase()))
}

/// Sums the best score of every token; `None` if any token matches nothing.
fn match_tokens(tokens: &[&str], item: &SshGroupItem, group_name: &str) -> Option<isize> {
    let mut total = 0;

    for token in tokens {
        total += token_score(token, item, group_name)?;
    }

    Some(total)
}

/// Best score for a single token across the item's fields, falling back to the
/// group name when no field matches.
fn token_score(token: &str, item: &SshGroupItem, group_name: &str) -> Option<isize> {
    item_score(token, item).or_else(|| best_match(token, group_name).map(|m| m.score()))
}

fn item_score(token: &str, item: &SshGroupItem) -> Option<isize> {
    let name = best_match(token, &item.name).map(|m| m.score());

    let hostname = item
        .host_config
        .get(&SshOptionKey::Hostname)
        .and_then(|value| best_match(token, value).map(|m| m.score()));

    let user = item
        .host_config
        .get(&SshOptionKey::User)
        .and_then(|value| best_match(token, value).map(|m| m.score()));

    let comment = item
        .comment
        .as_ref()
        .and_then(|c| best_match(token, c).map(|m| m.score()));

    [name, hostname, user, comment].into_iter().flatten().max()
}

/// Bonus for hosts that are used often and recently, so daily drivers float up.
fn frecency_bonus(item: &SshGroupItem, now: i64) -> isize {
    let count = item.connection_count.clamp(0, FRECENCY_COUNT_CAP) * FRECENCY_COUNT_WEIGHT as i64;
    let mut bonus = count as isize;

    if item.last_used > 0 {
        let age_days = now.saturating_sub(item.last_used).max(0) / SECONDS_PER_DAY;

        for (max_age_days, value) in RECENCY_BONUSES {
            if age_days < max_age_days {
                bonus += value;
                break;
            }
        }
    }

    bonus
}

/// Char indices in `target` matched by any whitespace-separated token of
/// `query`, sorted and deduplicated. Used to highlight search results.
pub fn highlight_indices(query: &str, item: &SshGroupItem) -> Vec<usize> {
    // For grouped hosts the displayed name is `group/short-name`; match the
    // group prefix and the short name separately so the highlighted characters
    // agree with how ranking scored the item.
    let (group, offset) = match item.full_name.split_once('/') {
        Some((group, _)) => (Some(group), group.chars().count() + 1),
        None => (None, 0),
    };

    let mut indices = Vec::new();

    for token in query.split_whitespace() {
        if let Some(group) = group {
            if let Some(group_match) = best_match(token, group) {
                indices.extend(group_match.matched_indices().copied());
            }
        }

        if let Some(name_match) = best_match(token, &item.name) {
            indices.extend(
                name_match
                    .matched_indices()
                    .copied()
                    .map(|idx| idx + offset),
            );
        }
    }

    indices.sort_unstable();
    indices.dedup();
    indices
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

        assert_eq!(rank_matches("", &groups, 0), vec![(1, 0)]);
        assert_eq!(rank_matches("prod", &groups, 0), vec![(1, 0)]);
        assert!(rank_matches("recent", &groups, 0).is_empty());
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

        // "prod" matches prod-web-01 tightly; deploy-runner-old only matches loosely
        assert_eq!(rank_matches("prod", &groups, 0), vec![(0, 1), (0, 0)]);

        // Equal scores fall back to connection count, highest first
        let groups = vec![make_group(
            "Prod",
            vec![make_item("prod-a", 1), make_item("prod-b", 9)],
        )];
        assert_eq!(rank_matches("prod", &groups, 0), vec![(0, 1), (0, 0)]);
    }

    #[test]
    fn test_rank_matches_multi_token_cross_field() {
        // "prod" only matches the group name, "db" only the host name
        let groups = vec![make_group("Prod", vec![make_item("db-01", 0)])];

        assert_eq!(rank_matches("prod db", &groups, 0), vec![(0, 0)]);
        assert_eq!(rank_matches("db prod", &groups, 0), vec![(0, 0)]);
        assert!(rank_matches("prod db web", &groups, 0).is_empty());
    }

    #[test]
    fn test_rank_matches_group_prefix_priority() {
        let now = 1_800_000_000;
        let groups = vec![
            make_group(
                "LLM",
                vec![make_item("worker-0", 0), make_item("worker-1", 0)],
            ),
            make_group(
                "Team",
                vec![make_item("llm-service", 0), make_item("llm-proxy", 0)],
            ),
        ];

        // Typing the group name (or a prefix of it) surfaces that group first,
        // even though the `llm-*` names score higher on their own.
        let expected = vec![(0, 0), (0, 1), (1, 0), (1, 1)];
        assert_eq!(rank_matches("llm", &groups, now), expected);
        assert_eq!(rank_matches("LL", &groups, now), expected);

        // A more specific token is not a group match, so only the name matches
        assert_eq!(rank_matches("llm-service", &groups, now), vec![(1, 0)]);
    }

    #[test]
    fn test_rank_matches_frecency_floats_used_host() {
        let now = 1_000_000_000;
        let groups = vec![make_group(
            "Prod",
            vec![
                make_item("prod-a", 0),
                SshGroupItem {
                    last_used: now - 60,
                    ..make_item("prod-b", 0)
                },
            ],
        )];

        // Same fuzzy score; the recently used host wins
        assert_eq!(rank_matches("prod", &groups, now), vec![(0, 1), (0, 0)]);
    }

    #[test]
    fn test_frecency_bonus_decays() {
        let now = 1_000_000_000;

        let recent = SshGroupItem {
            last_used: now - 60,
            ..make_item("a", 0)
        };
        let week = SshGroupItem {
            last_used: now - 2 * SECONDS_PER_DAY,
            ..make_item("a", 0)
        };
        let month = SshGroupItem {
            last_used: now - 10 * SECONDS_PER_DAY,
            ..make_item("a", 0)
        };
        let old = SshGroupItem {
            last_used: now - 200 * SECONDS_PER_DAY,
            ..make_item("a", 0)
        };

        assert_eq!(frecency_bonus(&recent, now), 30);
        assert_eq!(frecency_bonus(&week, now), 20);
        assert_eq!(frecency_bonus(&month, now), 10);
        assert_eq!(frecency_bonus(&old, now), 0);
        assert_eq!(frecency_bonus(&make_item("a", 0), now), 0);
        assert_eq!(frecency_bonus(&make_item("a", 100), now), 20);
    }

    #[test]
    fn test_highlight_indices_union_of_tokens() {
        let item = make_item("ab-cd", 0);
        assert_eq!(highlight_indices("ab", &item), vec![0, 1]);
        assert_eq!(highlight_indices("ab cd", &item), vec![0, 1, 3, 4]);
        assert!(highlight_indices("", &item).is_empty());
        assert!(highlight_indices("zz", &item).is_empty());
    }

    #[test]
    fn test_highlight_indices_group_prefix() {
        let mut item = make_item("db-01", 0);
        item.full_name = "Prod/db-01".to_string();

        // "prod" matches the group prefix, "db" the short name
        assert_eq!(highlight_indices("prod db", &item), vec![0, 1, 2, 3, 5, 6]);
    }

    #[test]
    fn test_highlight_indices_unicode_char_positions() {
        let item = make_item("こんにちは", 0);
        assert_eq!(highlight_indices("ち", &item), vec![3]);
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
