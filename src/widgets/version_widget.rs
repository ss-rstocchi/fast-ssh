use crate::app::App;
use crate::get_theme;
use std::io::Stdout;
use tui::{
    backend::CrosstermBackend, layout::Rect, style::Style, text::Spans, widgets::Paragraph, Frame,
};

pub struct VersionWidget {}

impl VersionWidget {
    pub fn render(_app: &App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let version = env!("CARGO_PKG_VERSION");
        let version_text = format!("v{}", version);

        let version_span = Spans::from(version_text);

        let paragraph = Paragraph::new(version_span)
            .style(Style::default().fg(get_theme().text_secondary()))
            .alignment(tui::layout::Alignment::Right);

        frame.render_widget(paragraph, area);
    }
}
