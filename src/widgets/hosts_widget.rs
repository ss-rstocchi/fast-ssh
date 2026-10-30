use super::block;
use crate::{
    app::{App, AppState},
    get_theme,
    searcher::{highlight_indices, match_hints},
    ssh_config_store::{SshGroupItem, RECENTS_GROUP},
};
use chrono::{DateTime, Local};
use std::{
    io::Stdout,
    time::{Duration, UNIX_EPOCH},
};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::{Cell, Row, Table, TableState},
    Frame,
};

pub struct HostsWidget {}

impl HostsWidget {
    pub fn render(app: &mut App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let theme = get_theme();
        let block = block::new(" Hosts ");
        let header = HostsWidget::create_header();
        let search_query = match app.state {
            AppState::Searching => Some(app.searcher.search_string()),
            AppState::Normal => None,
        };
        // Search results and Recents span all groups, so show "group/name"
        // to disambiguate
        let show_full_name = search_query.is_some()
            || app
                .get_selected_group()
                .is_some_and(|group| group.name == RECENTS_GROUP);

        let total = app.items_len();
        let capacity = HostsWidget::visible_capacity(area);
        let selected = app
            .host_state
            .selected()
            .unwrap_or(0)
            .min(total.saturating_sub(1));

        // Keep the selected row inside the viewport without rebuilding the
        // whole table: only rows in `[start, end)` are materialized.
        let mut start = app.hosts_offset.min(total.saturating_sub(1));
        if selected < start {
            start = selected;
        } else if selected >= start + capacity {
            start = selected + 1 - capacity;
        }
        app.hosts_offset = start;

        let end = (start + capacity).min(total);
        let items = app.get_items_range(start, end);
        let rows = HostsWidget::create_rows_from_items(&items, search_query, show_full_name);

        // Rows are pre-sliced, so the table renders them from offset 0.
        let mut state = TableState::default();
        state.select(if total == 0 {
            None
        } else {
            Some(selected - start)
        });

        let t = Table::new(rows)
            .header(header)
            .block(block)
            .highlight_style(Style::default().fg(theme.text_primary()))
            .style(Style::default().fg(theme.text_secondary()))
            .highlight_symbol(">> ")
            .widths(&[
                Constraint::Percentage(50),
                Constraint::Percentage(30),
                Constraint::Percentage(20),
            ]);

        frame.render_stateful_widget(t, area, &mut state);
    }

    /// Number of host rows that fit in `area`. Each row renders on two lines
    /// (one content line plus a bottom margin) below a two-line header inside
    /// the two-line block border.
    fn visible_capacity(area: Rect) -> usize {
        let rows_height = area.height.saturating_sub(4);
        (rows_height as usize).div_ceil(2).max(1)
    }

    fn create_header() -> Row<'static> {
        let theme = get_theme();
        const HEADERS: [&str; 3] = ["Host", "Last Used", "# of Conn"];
        let header_cells = HEADERS
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(theme.text_secondary())));

        Row::new(header_cells)
            .style(Style::default())
            .height(1)
            .bottom_margin(1)
    }

    fn create_rows_from_items(
        items: &[&SshGroupItem],
        search_query: Option<&str>,
        show_full_name: bool,
    ) -> Vec<Row<'static>> {
        let style = Style::default();
        items
            .iter()
            .map(|item| {
                let timestamp_str = HostsWidget::format_last_used_date(item);

                let cells = [
                    HostsWidget::host_name_cell(item, search_query, show_full_name),
                    Cell::from(timestamp_str).style(style),
                    Cell::from(item.connection_count.to_string()).style(style),
                ];

                Row::new(cells).height(1).bottom_margin(1)
            })
            .collect::<Vec<Row<'static>>>()
    }

    /// Host cell with the characters matched by the search query highlighted,
    /// followed by dim hints for matches on hostname, user or comment.
    fn host_name_cell(
        item: &SshGroupItem,
        search_query: Option<&str>,
        show_full_name: bool,
    ) -> Cell<'static> {
        let displayed = if show_full_name {
            &item.full_name
        } else {
            &item.name
        };

        let Some(query) = search_query.filter(|query| !query.trim().is_empty()) else {
            return Cell::from(displayed.clone());
        };

        let mut spans = HostsWidget::highlighted_spans(
            displayed,
            &highlight_indices(query, item),
            Style::default(),
        );

        let hint_style = Style::default().add_modifier(Modifier::DIM);
        for hint in match_hints(query, item) {
            spans.push(Span::styled(" · ", hint_style));
            spans.extend(HostsWidget::highlighted_spans(
                hint.value,
                &hint.indices,
                hint_style,
            ));
        }

        Cell::from(Spans::from(spans))
    }

    /// Splits `text` into runs, styling the chars at `indices` (sorted char
    /// positions) as matches on top of `base_style`.
    fn highlighted_spans(text: &str, indices: &[usize], base_style: Style) -> Vec<Span<'static>> {
        let theme = get_theme();
        let highlight_style = base_style
            .fg(theme.text_primary())
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);

        let mut spans = Vec::new();
        let mut run = String::new();
        let mut run_highlighted = false;

        for (idx, ch) in text.chars().enumerate() {
            let is_highlighted = indices.binary_search(&idx).is_ok();
            if !run.is_empty() && is_highlighted != run_highlighted {
                let style = if run_highlighted {
                    highlight_style
                } else {
                    base_style
                };
                spans.push(Span::styled(std::mem::take(&mut run), style));
            }
            run_highlighted = is_highlighted;
            run.push(ch);
        }

        if !run.is_empty() {
            let style = if run_highlighted {
                highlight_style
            } else {
                base_style
            };
            spans.push(Span::styled(run, style));
        }

        spans
    }

    fn format_last_used_date(item: &SshGroupItem) -> String {
        if item.last_used <= 0 {
            return "Never".to_string();
        }

        let d = UNIX_EPOCH + Duration::from_secs(item.last_used as u64);
        let dt = DateTime::<Local>::from(d);
        dt.format("%D %R").to_string()
    }
}
