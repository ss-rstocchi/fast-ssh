use crate::get_theme;
use std::io::Stdout;
use tui::{
    backend::CrosstermBackend, layout::Rect, style::Style, text::Spans, widgets::Paragraph, Frame,
};

static VERSION_TEXT: &str = concat!("v", env!("CARGO_PKG_VERSION"));

pub struct VersionWidget {}

impl VersionWidget {
    /// `show_help_hint` adds the `?` hint when the layout has no help box.
    pub fn render(area: Rect, show_help_hint: bool, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let version_span = if show_help_hint {
            Spans::from(format!("'?' help · {VERSION_TEXT}"))
        } else {
            Spans::from(VERSION_TEXT)
        };

        let paragraph = Paragraph::new(version_span)
            .style(Style::default().fg(get_theme().text_secondary()))
            .alignment(tui::layout::Alignment::Right);

        frame.render_widget(paragraph, area);
    }
}
