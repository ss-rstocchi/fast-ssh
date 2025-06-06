use super::block;
use crate::{app::AppState, App, THEME};
use std::io::Stdout;
use tui::layout::Rect;
use tui::style::{Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::Tabs;
use tui::{backend::CrosstermBackend, Frame};

pub struct GroupsWidget {}

impl GroupsWidget {
    pub fn render(app: &App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let title = match app.state {
            AppState::Editing => " Groups (Edit Mode) ",
            _ => " Groups ",
        };
        let block = block::new(title);

        let filtered_groups = app.get_filtered_groups();

        let mut titles: Vec<Spans> = filtered_groups
            .iter()
            .map(|fg| {
                let is_current = fg.original_index == app.selected_group;
                let group_name = if app.state == AppState::Editing {
                    let non_recents_groups: Vec<&str> = app
                        .scs
                        .groups
                        .iter()
                        .filter(|g| g.name != "Recents")
                        .map(|g| g.name.as_str())
                        .collect();
                    let position = non_recents_groups
                        .iter()
                        .position(|&name| name == fg.name)
                        .unwrap_or(0)
                        + 1;
                    if is_current {
                        format!("« [{}] {} »", position, fg.name)
                    } else {
                        format!("[{}] {}", position, fg.name)
                    }
                } else {
                    fg.name.to_string()
                };

                let style = if is_current {
                    Style::default().fg(THEME.text_secondary())
                } else {
                    Style::default()
                        .fg(THEME.text_secondary())
                        .add_modifier(Modifier::DIM)
                };

                Spans::from(Span::styled(group_name, style))
            })
            .collect();

        let filtered_selected_pos = app.get_visible_group_index(app.selected_group).unwrap_or(0);

        if filtered_selected_pos < titles.len() {
            titles.rotate_left(filtered_selected_pos);
        }

        let tabs = Tabs::new(titles)
            .block(block)
            .select(0)
            .highlight_style(match app.state {
                AppState::Editing => Style::default()
                    .add_modifier(Modifier::BOLD)
                    .bg(THEME.text_primary())
                    .add_modifier(Modifier::REVERSED),
                _ => Style::default()
                    .add_modifier(Modifier::BOLD)
                    .bg(THEME.text_primary()),
            });

        frame.render_widget(tabs, area);
    }
}
