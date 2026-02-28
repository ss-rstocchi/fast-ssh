use crate::get_theme;
use tui::{
    style::Style,
    text::Span,
    widgets::{Block, Borders},
};

pub fn new(title: &str) -> Block<'_> {
    let theme = get_theme();
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_color()))
        .title_alignment(tui::layout::Alignment::Center)
        .border_type(tui::widgets::BorderType::Rounded)
        .title(Span::styled(
            title,
            Style::default().fg(theme.text_secondary()),
        ))
}
