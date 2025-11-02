use super::block;
use crate::{get_theme, App};
use std::io::Stdout;
use tui::layout::Rect;
use tui::style::{Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::Tabs;
use tui::{backend::CrosstermBackend, Frame};

pub struct GroupsWidget {}

impl GroupsWidget {
    pub fn render(app: &App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let theme = get_theme();
        let block = block::new(" Groups ");

        let mut titles: Vec<Spans> = app
            .scs
            .groups
            .iter()
            .map(|t| {
                Spans::from(Span::styled(
                    t.name.to_string(),
                    Style::default().fg(theme.text_secondary()),
                ))
            })
            .collect();

        // Rotate to show selected group first (safe: bounds checked above)
        if !titles.is_empty() && app.selected_group < titles.len() {
            titles.rotate_left(app.selected_group);
        }

        let tabs = Tabs::new(titles)
            .block(block)
            .select(0)
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .bg(theme.text_primary()),
            );

        frame.render_widget(tabs, area);
    }
}
