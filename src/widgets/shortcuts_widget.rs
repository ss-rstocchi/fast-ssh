use crate::{App, THEME};
use std::io::Stdout;
use tui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::Style,
    text::Spans,
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::block;

pub struct ShortcutsWidget {}

impl ShortcutsWidget {
    pub fn render(_app: &App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let block = block::new(" Help ");

        let text = vec![
            Spans::from("'?': Display Shortcuts Panel"),
            Spans::from("'Enter': Open Selected SSH Connection"),
            Spans::from("'Space': Select Recents Group"),
            Spans::from("'Tab/BackTab': Change Group"),
            Spans::from("'Left/Right Arrow or h/l': Change Group"),
            Spans::from("'Up/Down Arrow or k/j': Navigate Hosts"),
            Spans::from("'c': Switch Config Display Mode"),
            Spans::from("'PageUp/Down': Scroll Configuration"),
            Spans::from("'e': Enable Edit Mode"),
            Spans::from("'s' or '/': Enable Search Mode"),
            Spans::from("'Esc' or 'q': Exit Search/Edit Mode"),
            Spans::from("'j/k' or '↑/↓': Move connection up/down in group (in Edit Mode)"),
            Spans::from("'h/l' or '←/→': Move connection between groups (in Edit Mode)"),
            Spans::from("'H/L': Move group left/right (in Edit Mode)"),
            Spans::from("'Enter': Save changes (in Edit Mode)"),
            Spans::from("'K': Copy SSH Key and Exit"),
            Spans::from("'C': Copy Files and Exit"),
            Spans::from("'q': Exit Fast-SSH"),
        ];

        let paragraph = Paragraph::new(text)
            .alignment(tui::layout::Alignment::Left)
            .block(block)
            .style(Style::default().fg(THEME.text_secondary()))
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }
}
