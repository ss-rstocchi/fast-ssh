use super::block;
use crate::{app::App, ssh_config_store::SshGroupItem, THEME};
use chrono::{DateTime, Utc};
use std::{
    io::Stdout,
    time::{Duration, UNIX_EPOCH},
};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Cell, Row, Table},
    Frame,
};

pub struct HostsWidget {}

impl HostsWidget {
    pub fn render(app: &mut App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
            let title = match app.state {
                crate::app::AppState::Editing => {
                    if app.get_selected_group().name == "Recents" {
                        " Edit Mode: Cannot edit Recents (Esc to exit) "
                    } else {
                        if app.should_save_config {
                            " Edit Mode: Changes pending (Enter to save, Esc to exit) "
                        } else {
                            // Show a simple edit mode title
                            " Edit Mode (Esc to exit) "
                        }
                    }
                },
                _ => " Hosts ",
            };
            let block = block::new(title);
            let header = HostsWidget::create_header();
            let items = app.get_items_based_on_mode();
            let rows = HostsWidget::create_rows_from_items(&items);

            if app.host_state.selected().unwrap_or(0) >= items.len() {
                app.host_state.select(Some(0));
            }

        let highlight_style = match app.state {
            crate::app::AppState::Editing if app.should_save_config => {
                // Use a different highlight style when changes are pending in Edit mode
                Style::default().fg(THEME.text_primary()).bg(THEME.border_color())
                    .add_modifier(tui::style::Modifier::BOLD)
            },
            crate::app::AppState::Editing => {
                // Use a slightly different highlight style to indicate Edit mode
                Style::default().fg(THEME.text_primary()).add_modifier(tui::style::Modifier::BOLD)
                    .add_modifier(tui::style::Modifier::UNDERLINED)
            },
            _ => Style::default().fg(THEME.text_primary())
        };
        
        let t = Table::new(rows)
            .header(header)
            .block(block)
            .highlight_style(highlight_style)
            .style(Style::default().fg(THEME.text_secondary()))
            .highlight_symbol(">> ")
            .widths(&[
                Constraint::Percentage(50),
                Constraint::Percentage(30),
                Constraint::Percentage(20),
            ]);

        frame.render_stateful_widget(t, area, &mut app.host_state);
    }

    fn create_header() -> Row<'static> {
        let header_cells = ["Host", "Last Used", "# of Conn"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(THEME.text_secondary())));

        Row::new(header_cells)
            .style(Style::default())
            .height(1)
            .bottom_margin(1)
    }

    fn create_rows_from_items(items: &[&SshGroupItem]) -> Vec<Row<'static>> {
        let style = Style::default();
        items
            .iter()
            .map(|item| {
                let timestamp_str = HostsWidget::format_last_used_date(item);

                let cells = [
                    Cell::from(item.name.to_string()).style(style),
                    Cell::from(timestamp_str).style(style),
                    Cell::from(item.connection_count.to_string()).style(style),
                ];

                Row::new(cells).height(1).bottom_margin(1)
            })
            .collect::<Vec<Row<'static>>>()
    }

    fn format_last_used_date(item: &SshGroupItem) -> String {
        let mut timestamp_str = "Never".to_string();
        if item.last_used != 0 {
            let d = UNIX_EPOCH + Duration::from_secs(item.last_used as u64);
            let dt = DateTime::<Utc>::from(d);
            timestamp_str = dt.format("%D %R").to_string();
        }
        timestamp_str
    }
}
