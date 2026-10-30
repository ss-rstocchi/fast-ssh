use crate::{
    get_theme,
    ssh_config_store::{SshGroup, SshGroupItem, OTHERS_GROUP, RECENTS_GROUP},
    widgets::block,
};
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

/// How a query token matched a field, best first. Ranking compares tiers
/// before fuzzy scores, so an exact or prefix match always beats a scattered
/// fuzzy one regardless of how `sublime_fuzzy` scores them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Exact,
    Prefix,
    /// Contiguous match starting after a separator, e.g. `web` in `api-web-01`
    WordStart,
    Substring,
    Fuzzy,
}

/// Field of a host a token can match, in tie-break preference order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    HostName,
    User,
    Comment,
    Group,
}

#[derive(Debug)]
struct FieldMatch {
    tier: Tier,
    score: isize,
    /// Matched char indices within the field
    indices: Vec<usize>,
}

impl FieldMatch {
    /// Whether `self` is a strictly better match than `other`.
    fn beats(&self, other: &FieldMatch) -> bool {
        (self.tier, -self.score) < (other.tier, -other.score)
    }
}

/// Matches a single (case-insensitive) token against one field value.
fn match_field(token: &[char], value: &str) -> Option<FieldMatch> {
    // One lowercase char per original char keeps indices aligned with `value`
    let field: Vec<char> = value.chars().map(lowercase_char).collect();
    let fuzzy = best_match(&token.iter().collect::<String>(), value)?;
    let score = fuzzy.score();

    let contiguous = |tier: Tier, start: usize| FieldMatch {
        tier,
        score,
        indices: (start..start + token.len()).collect(),
    };

    if field == token {
        return Some(contiguous(Tier::Exact, 0));
    }

    let mut first_occurrence = None;
    for start in 0..field.len().saturating_sub(token.len() - 1) {
        if field[start..start + token.len()] != *token {
            continue;
        }
        if start == 0 {
            return Some(contiguous(Tier::Prefix, 0));
        }
        if !field[start - 1].is_alphanumeric() {
            return Some(contiguous(Tier::WordStart, start));
        }
        first_occurrence.get_or_insert(start);
    }

    Some(match first_occurrence {
        Some(start) => contiguous(Tier::Substring, start),
        None => FieldMatch {
            tier: Tier::Fuzzy,
            score,
            indices: fuzzy.matched_indices().copied().collect(),
        },
    })
}

/// Best field match for one lowercased token; earlier fields win ties.
fn best_field(
    token: &[char],
    item: &SshGroupItem,
    group_name: Option<&str>,
) -> Option<(Field, FieldMatch)> {
    let fields = [
        (Field::Name, Some(item.name.as_str())),
        (Field::HostName, item.host_config.get("HostName")),
        (Field::User, item.host_config.get("User")),
        (Field::Comment, item.comment.as_deref()),
        (Field::Group, group_name),
    ];

    let mut best: Option<(Field, FieldMatch)> = None;
    for (field, value) in fields {
        let Some(found) = value.and_then(|value| match_field(token, value)) else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|(_, current)| found.beats(current))
        {
            best = Some((field, found));
        }
    }
    best
}

/// First char of the lowercase form, so char indices stay aligned.
fn lowercase_char(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn lowercase_tokens(query: &str) -> Vec<Vec<char>> {
    query
        .split_whitespace()
        .map(|token| token.chars().map(lowercase_char).collect())
        .collect()
}

/// Returns `(group index, item index)` pairs for every host matching `query`,
/// ranked best-first. Recents are excluded. An empty query matches every host,
/// most frequently/recently used first, so search then Enter reconnects to the
/// usual host.
///
/// `query` is split on whitespace and every token must match the item's short
/// name, hostname, user or comment, or its group name, in any order. Each token
/// is ranked by its best field: exact, then prefix, then word-start, then
/// substring, then fuzzy. Hosts are ordered by the sum of their tokens' tiers,
/// then by fuzzy score plus a frecency bonus, then by connection count. Typing
/// a group name therefore surfaces that group as an exact match. The synthetic
/// "Others" group of ungrouped hosts never matches by name. `now` is a Unix
/// timestamp used for the frecency bonus.
pub fn rank_matches(query: &str, groups: &[SshGroup], now: i64) -> Vec<(usize, usize)> {
    let tokens = lowercase_tokens(query);

    let mut scored: Vec<(usize, isize, i64, (usize, usize))> = Vec::new();

    for (group_idx, group) in groups.iter().enumerate() {
        if group.name == RECENTS_GROUP {
            continue;
        }

        let group_name = (group.name != OTHERS_GROUP).then_some(group.name.as_str());

        'items: for (item_idx, item) in group.items.iter().enumerate() {
            let mut tiers = 0;
            let mut score = frecency_bonus(item, now);

            for token in &tokens {
                let Some((_, found)) = best_field(token, item, group_name) else {
                    continue 'items;
                };
                tiers += found.tier as usize;
                score += found.score;
            }

            scored.push((tiers, score, item.connection_count, (group_idx, item_idx)));
        }
    }

    // Stable sort, so equal hosts keep config order
    scored.sort_by_key(|(tiers, score, connection_count, _)| (*tiers, -score, -connection_count));
    scored.into_iter().map(|(_, _, _, idx)| idx).collect()
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

/// Char indices in the item's `full_name` matched by the whitespace-separated
/// tokens of `query`, sorted and deduplicated. Used to highlight search
/// results. A token is highlighted only in the field it ranked by: the short
/// name or the group prefix, and not at all for hostname/user/comment.
pub fn highlight_indices(query: &str, item: &SshGroupItem) -> Vec<usize> {
    // For grouped hosts the displayed name is `group/short-name`; match the
    // group prefix and the short name separately so the highlighted characters
    // agree with how ranking scored the item.
    let (group, offset) = match item.full_name.split_once('/') {
        Some((group, _)) => (Some(group), group.chars().count() + 1),
        None => (None, 0),
    };

    let mut indices = Vec::new();

    for token in lowercase_tokens(query) {
        match best_field(&token, item, group) {
            Some((Field::Name, found)) => {
                indices.extend(found.indices.iter().map(|idx| idx + offset))
            }
            Some((Field::Group, found)) => indices.extend(found.indices),
            _ => {}
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
            host_config: Default::default(),
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

    fn tier(token: &str, value: &str) -> Option<Tier> {
        let token: Vec<char> = token.chars().map(lowercase_char).collect();
        match_field(&token, value).map(|found| found.tier)
    }

    #[test]
    fn test_match_field_tiers() {
        assert_eq!(tier("db", "DB"), Some(Tier::Exact));
        assert_eq!(tier("db", "db-01"), Some(Tier::Prefix));
        assert_eq!(tier("web", "api-web-01"), Some(Tier::WordStart));
        assert_eq!(tier("eb", "web"), Some(Tier::Substring));
        assert_eq!(tier("db", "web-dev-backup"), Some(Tier::Fuzzy));
        assert_eq!(tier("prod", "staging-db"), None);
        assert_eq!(tier("toolong", "short"), None);
    }

    #[test]
    fn test_match_field_contiguous_indices() {
        let token: Vec<char> = "web".chars().collect();
        let found = match_field(&token, "my-web").unwrap();
        assert_eq!(found.indices, vec![3, 4, 5]);
    }

    #[test]
    fn test_rank_matches_tiers_beat_fuzzy_score() {
        // sublime_fuzzy scores the scattered match higher than the exact one
        let groups = vec![make_group(
            "Hosts",
            vec![
                make_item("web-dev-backup", 0),
                make_item("api-db", 0),
                make_item("db-01", 0),
                make_item("db", 0),
            ],
        )];
        assert_eq!(
            rank_matches("db", &groups, 0),
            vec![(0, 3), (0, 2), (0, 1), (0, 0)]
        );

        // Frecency only breaks ties inside a tier
        let now = 1_000_000_000;
        let groups = vec![make_group(
            "Hosts",
            vec![
                SshGroupItem {
                    last_used: now - 60,
                    ..make_item("w-e-b-x", 50)
                },
                make_item("web", 0),
            ],
        )];
        assert_eq!(rank_matches("web", &groups, now), vec![(0, 1), (0, 0)]);
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
            "Hosts",
            vec![
                make_item("deploy-runner-old", 100),
                make_item("prod-web-01", 0),
            ],
        )];

        // "prod" is a prefix of prod-web-01; deploy-runner-old only matches fuzzily
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
    fn test_rank_matches_exact_group_name_first() {
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

        // Typing the group name exactly surfaces that group first, even though
        // the `llm-*` names score higher on their own.
        let expected = vec![(0, 0), (0, 1), (1, 0), (1, 1)];
        assert_eq!(rank_matches("llm", &groups, now), expected);

        // A group-name prefix ranks like any other prefix match
        let mut prefix = rank_matches("LL", &groups, now);
        prefix.sort();
        assert_eq!(prefix, expected);

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
    fn test_rank_matches_empty_query_orders_by_frecency() {
        let now = 1_000_000_000;
        let groups = vec![
            make_group("A", vec![make_item("unused", 0), make_item("often", 5)]),
            make_group(
                "B",
                vec![
                    make_item("also-unused", 0),
                    SshGroupItem {
                        last_used: now - 60,
                        ..make_item("just-now", 1)
                    },
                ],
            ),
        ];

        // Used hosts first; unused ones keep config order
        assert_eq!(
            rank_matches("", &groups, now),
            vec![(1, 1), (0, 1), (0, 0), (1, 0)]
        );
        assert_eq!(
            rank_matches("  ", &groups, now),
            rank_matches("", &groups, now)
        );
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
    fn test_highlight_indices_skips_tokens_ranked_by_other_fields() {
        let mut item = make_item("db-01", 0);
        item.host_config.insert("HostName", "db01.lan");

        // "db01" only fuzzy-matches the name but is a hostname prefix, so the
        // name stays plain
        assert!(highlight_indices("db01", &item).is_empty());
        assert_eq!(highlight_indices("db", &item), vec![0, 1]);
    }

    #[test]
    fn test_rank_matches_others_group_name_never_matches() {
        let groups = vec![
            make_group(OTHERS_GROUP, vec![make_item("db-01", 0)]),
            make_group("Prod", vec![make_item("web-other", 0)]),
        ];

        // "other" matches only a real host name, not the synthetic group
        assert_eq!(rank_matches("other", &groups, 0), vec![(1, 0)]);
        // A prefix of "Others" neither matches nor prioritizes ungrouped hosts
        assert_eq!(rank_matches("o", &groups, 0), vec![(1, 0)]);
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
