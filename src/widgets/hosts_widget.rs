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
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Width of a "Last Used" value, e.g. `10/29/26 22:17`
const DATE_WIDTH: u16 = 14;
/// Name width needed to keep the full `# of Conn` header
const MIN_NAME_WIDTH_FULL: u16 = 24;
/// Name width needed to keep the other optional columns
const MIN_NAME_WIDTH: u16 = 20;
/// Narrowest useful group column; below it the group goes back into the name
const MIN_GROUP_WIDTH: u16 = 6;
const HIGHLIGHT_SYMBOL: &str = ">> ";
/// Gap between columns when the pane has room, so neighbouring values
/// (a time and a count) do not run together
const WIDE_SPACING: u16 = 3;
/// Gap in narrow panes, where every column counts
const NARROW_SPACING: u16 = 1;

/// Columns that fit the hosts pane; the host name takes what is left.
#[derive(Debug, PartialEq, Eq)]
struct Columns {
    name_width: u16,
    /// Width of the group column, 0 when it is not shown
    group_width: u16,
    show_date: bool,
    /// `true` for the short `#` count header
    short_count: bool,
    /// Gap between columns
    spacing: u16,
}

impl Columns {
    /// `group_width` is the widest group name to show in its own column, 0
    /// for none. As the pane narrows, drops the long count header, then the
    /// date, then narrows the gaps and tries again, then shrinks the group
    /// column; the name keeps at least `MIN_NAME_WIDTH` while anything else
    /// is shown.
    fn fit(area: Rect, group_width: u16) -> Columns {
        let inner = area.width.saturating_sub(2 + HIGHLIGHT_SYMBOL.len() as u16);
        let columns = |group_width: u16, show_date: bool, short_count: bool, spacing: u16| {
            let mut fixed = Columns::count_width(short_count) + spacing;
            if show_date {
                fixed += DATE_WIDTH + spacing;
            }
            if group_width > 0 {
                fixed += group_width + spacing;
            }
            Columns {
                name_width: inner.saturating_sub(fixed),
                group_width,
                show_date,
                short_count,
                spacing,
            }
        };

        let candidates = [
            (true, false, MIN_NAME_WIDTH_FULL),
            (true, true, MIN_NAME_WIDTH),
            (false, true, MIN_NAME_WIDTH),
        ];
        let fitting = [WIDE_SPACING, NARROW_SPACING]
            .into_iter()
            .flat_map(|spacing| candidates.map(|candidate| (candidate, spacing)))
            .find_map(|((show_date, short_count, min), spacing)| {
                let candidate = columns(group_width, show_date, short_count, spacing);
                (candidate.name_width >= min).then_some(candidate)
            });
        if let Some(fitting) = fitting {
            return fitting;
        }

        // Shrink the group column, or drop it when too narrow to be useful
        let without_group = columns(0, false, true, NARROW_SPACING);
        let room = without_group
            .name_width
            .saturating_sub(MIN_NAME_WIDTH + NARROW_SPACING);
        if group_width > 0 && room >= MIN_GROUP_WIDTH {
            columns(room.min(group_width), false, true, NARROW_SPACING)
        } else {
            without_group
        }
    }

    fn count_width(short: bool) -> u16 {
        if short {
            4
        } else {
            9
        }
    }

    /// `text` right-aligned in the count column, so counts line up by digit
    fn align_count(&self, text: &str) -> String {
        let width = Columns::count_width(self.short_count) as usize;
        format!("{text:>width$}")
    }

    fn widths(&self) -> Vec<Constraint> {
        let mut widths = vec![Constraint::Length(self.name_width)];
        if self.group_width > 0 {
            widths.push(Constraint::Length(self.group_width));
        }
        if self.show_date {
            widths.push(Constraint::Length(DATE_WIDTH));
        }
        widths.push(Constraint::Length(Columns::count_width(self.short_count)));
        widths
    }
}

pub struct HostsWidget {}

impl HostsWidget {
    pub fn render(
        app: &mut App,
        area: Rect,
        dense: bool,
        frame: &mut Frame<CrosstermBackend<Stdout>>,
    ) {
        let theme = get_theme();
        let block = block::new(" Hosts ");
        let search_query = match app.state {
            AppState::Searching => Some(app.searcher.search_string()),
            AppState::Normal => None,
        };
        // Search results and Recents span all groups, so show each host's
        // group to disambiguate
        let show_groups = search_query.is_some()
            || app
                .get_selected_group()
                .is_some_and(|group| group.name == RECENTS_GROUP);

        let total = app.items_len();
        let capacity = HostsWidget::visible_capacity(area, dense);
        app.hosts_visible_rows = capacity;
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

        let group_width = if show_groups {
            items
                .iter()
                .map(|item| split_group(item).0.width())
                .max()
                // Never narrower than the "Group" header
                .map_or(0, |width| {
                    if width == 0 {
                        0
                    } else {
                        width.max(GROUP_HEADER.width())
                    }
                }) as u16
        } else {
            0
        };
        let columns = Columns::fit(area, group_width);
        let header = HostsWidget::create_header(&columns, dense);
        // Too narrow for a group column: fall back to "group/name"
        let inline_group = show_groups && columns.group_width == 0;
        let rows = HostsWidget::create_rows_from_items(
            &items,
            search_query,
            inline_group,
            &columns,
            dense,
        );

        // Rows are pre-sliced, so the table renders them from offset 0.
        let mut state = TableState::default();
        state.select(if total == 0 {
            None
        } else {
            Some(selected - start)
        });

        let widths = columns.widths();
        let t = Table::new(rows)
            .header(header)
            .block(block)
            .highlight_style(Style::default().fg(theme.text_primary()))
            .style(Style::default().fg(theme.text_secondary()))
            .highlight_symbol(HIGHLIGHT_SYMBOL)
            .column_spacing(columns.spacing)
            .widths(&widths);

        frame.render_stateful_widget(t, area, &mut state);
    }

    /// Number of host rows that fit in `area` inside the two-line block
    /// border. Normally the header and every row are followed by a blank
    /// spacing line; dense rows have none.
    fn visible_capacity(area: Rect, dense: bool) -> usize {
        let inner = area.height.saturating_sub(2) as usize;
        if dense {
            inner.saturating_sub(1).max(1)
        } else {
            inner.saturating_sub(2).div_ceil(2).max(1)
        }
    }

    fn create_header(columns: &Columns, dense: bool) -> Row<'static> {
        let theme = get_theme();
        let mut headers = vec!["Host"];
        if columns.group_width > 0 {
            headers.push(GROUP_HEADER);
        }
        if columns.show_date {
            headers.push("Last Used");
        }
        let count_header = if columns.short_count {
            "#"
        } else {
            "# of Conn"
        };

        // The count column is right-aligned, like its numbers
        let header_cells = headers
            .into_iter()
            .map(str::to_string)
            .chain([columns.align_count(count_header)])
            .map(|h| Cell::from(h).style(Style::default().fg(theme.text_secondary())));

        Row::new(header_cells)
            .style(Style::default())
            .height(1)
            .bottom_margin(u16::from(!dense))
    }

    fn create_rows_from_items(
        items: &[&SshGroupItem],
        search_query: Option<&str>,
        inline_group: bool,
        columns: &Columns,
        dense: bool,
    ) -> Vec<Row<'static>> {
        let style = Style::default();
        let query = search_query.filter(|query| !query.trim().is_empty());
        items
            .iter()
            .map(|item| {
                // Matched chars index into `full_name`, i.e. "group/name"
                let indices = query.map_or_else(Vec::new, |query| highlight_indices(query, item));
                let (group, name_offset) = split_group(item);

                let name_indices: Vec<usize> = indices
                    .iter()
                    .filter_map(|idx| idx.checked_sub(name_offset))
                    .collect();
                let group_indices: Vec<usize> = indices
                    .iter()
                    .copied()
                    .filter(|&idx| idx < group.chars().count())
                    .collect();
                let group_spans = HostsWidget::highlighted_spans(
                    group,
                    &group_indices,
                    Style::default().add_modifier(Modifier::DIM),
                );

                // Without a group column the group prefixes the name, and is
                // what gets shortened so the name stays readable
                let prefix = if inline_group && !group.is_empty() {
                    let width = columns.name_width as usize;
                    let budget = width.saturating_sub(item.name.width() + 1);
                    if budget >= 2 {
                        let mut prefix = truncate_spans(group_spans.clone(), budget);
                        prefix.push(Span::styled(
                            "/",
                            Style::default().add_modifier(Modifier::DIM),
                        ));
                        prefix
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                let mut cells = vec![HostsWidget::host_name_cell(
                    prefix,
                    &item.name,
                    &name_indices,
                    query,
                    item,
                    columns.name_width,
                )];
                if columns.group_width > 0 {
                    cells.push(Cell::from(Spans::from(truncate_spans(
                        group_spans,
                        columns.group_width as usize,
                    ))));
                }
                if columns.show_date {
                    cells.push(Cell::from(HostsWidget::format_last_used_date(item)).style(style));
                }
                cells.push(
                    Cell::from(columns.align_count(&item.connection_count.to_string()))
                        .style(style),
                );

                Row::new(cells).height(1).bottom_margin(u16::from(!dense))
            })
            .collect::<Vec<Row<'static>>>()
    }

    /// `prefix`, then `displayed` with the characters at `indices` highlighted, followed by
    /// dimmed hints for tokens that matched the hostname, user or comment.
    /// Only the name is cut with `…`; hints that do not fit whole are left
    /// out rather than cut to a meaningless fragment.
    fn host_name_cell(
        prefix: Vec<Span<'static>>,
        displayed: &str,
        indices: &[usize],
        query: Option<&str>,
        item: &SshGroupItem,
        width: u16,
    ) -> Cell<'static> {
        let width = width as usize;
        let prefix_width: usize = prefix.iter().map(|span| span.content.width()).sum();
        let mut spans = prefix;
        spans.extend(truncate_spans(
            HostsWidget::highlighted_spans(displayed, indices, Style::default()),
            width.saturating_sub(prefix_width),
        ));

        if let Some(query) = query {
            let hint_style = Style::default().add_modifier(Modifier::DIM);
            let mut used: usize = spans.iter().map(|span| span.content.width()).sum();
            for hint in match_hints(query, item) {
                let hint_width = SEPARATOR.width() + hint.value.width();
                if used + hint_width > width {
                    break;
                }
                used += hint_width;
                spans.push(Span::styled(SEPARATOR, hint_style));
                spans.extend(HostsWidget::highlighted_spans(
                    hint.value,
                    &hint.indices,
                    hint_style,
                ));
            }
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

const SEPARATOR: &str = " · ";
const GROUP_HEADER: &str = "Group";

/// The group of a grouped host and the char offset of its short name within
/// `full_name`; `("", 0)` for ungrouped hosts.
fn split_group(item: &SshGroupItem) -> (&str, usize) {
    match item.full_name.split_once('/') {
        Some((group, _)) => (group, group.chars().count() + 1),
        None => ("", 0),
    }
}

/// Cuts `spans` to `max_width` terminal columns, ending with `…` when
/// anything was cut, so truncated names are visibly truncated.
fn truncate_spans(spans: Vec<Span<'static>>, max_width: usize) -> Vec<Span<'static>> {
    let total: usize = spans.iter().map(|span| span.content.width()).sum();
    if total <= max_width {
        return spans;
    }

    let mut budget = max_width.saturating_sub(1);
    let mut truncated = Vec::new();
    let mut last_style = Style::default();

    for span in spans {
        if budget == 0 {
            break;
        }
        last_style = span.style;
        let mut content = String::new();
        for ch in span.content.chars() {
            let width = ch.width().unwrap_or(0);
            if width > budget {
                budget = 0;
                break;
            }
            budget -= width;
            content.push(ch);
        }
        truncated.push(Span::styled(content, span.style));
    }

    if max_width > 0 {
        truncated.push(Span::styled("…", last_style));
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(spans: &[Span]) -> String {
        spans.iter().map(|span| span.content.as_ref()).collect()
    }

    #[test]
    fn test_truncate_spans() {
        let spans = || vec![Span::raw("prod-"), Span::raw("postgres")];
        assert_eq!(text(&truncate_spans(spans(), 13)), "prod-postgres");
        assert_eq!(text(&truncate_spans(spans(), 8)), "prod-po…");
        assert_eq!(text(&truncate_spans(spans(), 5)), "prod…");
        assert_eq!(text(&truncate_spans(spans(), 1)), "…");
        assert_eq!(text(&truncate_spans(spans(), 0)), "");

        // Wide chars count as two columns and are never split
        let wide = vec![Span::raw("こんにちは")];
        assert_eq!(text(&truncate_spans(wide, 6)), "こん…");
    }

    #[test]
    fn test_columns_fit() {
        let fit = |width, group_width| {
            let columns = Columns::fit(Rect::new(0, 0, width, 10), group_width);
            (
                columns.name_width,
                columns.group_width,
                columns.show_date,
                columns.short_count,
                columns.spacing,
            )
        };

        // Wide gaps while the name keeps its minimum width
        assert_eq!(fit(80, 0), (46, 0, true, false, 3));
        assert_eq!(fit(52, 0), (23, 0, true, true, 3));
        assert_eq!(fit(44, 0), (32, 0, false, true, 3));
        assert_eq!(fit(40, 0), (28, 0, false, true, 3));

        // The full group column wins over the date
        assert_eq!(fit(100, 18), (45, 18, true, false, 3));
        assert_eq!(fit(60, 18), (27, 18, false, true, 3));
        // Then the gaps narrow, the group column shrinks, then it is dropped
        assert_eq!(fit(52, 18), (23, 18, false, true, 1));
        assert_eq!(fit(45, 18), (20, 14, false, true, 1));
        assert_eq!(fit(35, 18), (25, 0, false, true, 1));
    }
}
