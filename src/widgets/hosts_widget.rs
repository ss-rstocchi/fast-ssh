use super::block;
use crate::{
    app::{App, AppState},
    get_theme,
    ssh_config_store::SshGroupItem,
};
use chrono::{DateTime, Local};
use std::{
    io::Stdout,
    time::{Duration, UNIX_EPOCH},
};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Cell, Row, Table, TableState},
    Frame,
};

pub struct HostsWidget {}

impl HostsWidget {
    pub fn render(app: &mut App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let theme = get_theme();
        let block = block::new(" Hosts ");
        let header = HostsWidget::create_header();
        // In search mode results span all groups, so show "group/name" to disambiguate
        let use_full_name = matches!(app.state, AppState::Searching);

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
        let rows = HostsWidget::create_rows_from_items(&items, use_full_name);

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

    fn create_rows_from_items(items: &[&SshGroupItem], use_full_name: bool) -> Vec<Row<'static>> {
        let style = Style::default();
        items
            .iter()
            .map(|item| {
                let timestamp_str = HostsWidget::format_last_used_date(item);
                let name = if use_full_name {
                    &item.full_name
                } else {
                    &item.name
                };

                let cells = [
                    Cell::from(name.to_string()).style(style),
                    Cell::from(timestamp_str).style(style),
                    Cell::from(item.connection_count.to_string()).style(style),
                ];

                Row::new(cells).height(1).bottom_margin(1)
            })
            .collect::<Vec<Row<'static>>>()
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
