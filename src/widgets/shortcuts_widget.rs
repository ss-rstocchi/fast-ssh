use crate::{get_theme, App};
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
            Spans::from("=== General ==="),
            Spans::from("'?': Toggle Shortcuts Panel"),
            Spans::from("'q': Exit Fast-SSH"),
            Spans::from(""),
            Spans::from("=== Navigation ==="),
            Spans::from("'Tab/BackTab': Change Group"),
            Spans::from("'Left/Right Arrow or h/l': Change Group"),
            Spans::from("'Space': Select Recents Group"),
            Spans::from("'Up/Down Arrow or k/j': Navigate Hosts"),
            Spans::from("'gg': Jump to First Host"),
            Spans::from("'G': Jump to Last Host"),
            Spans::from("'Ctrl+d': Scroll Half Page Down"),
            Spans::from("'Ctrl+u': Scroll Half Page Up"),
            Spans::from(""),
            Spans::from("=== Actions ==="),
            Spans::from("'Enter': Open Selected SSH Connection"),
            Spans::from("'K': Copy SSH Key and Exit"),
            Spans::from("'C': Copy Files and Exit"),
            Spans::from("'c': Switch Config Display Mode"),
            Spans::from("'PageUp/Down': Scroll Configuration"),
            Spans::from(""),
            Spans::from("=== Search Mode ==="),
            Spans::from("'s' or '/': Enable Search Mode"),
            Spans::from("'Enter': Commit Search (press again to connect)"),
            Spans::from("'Esc' or 'q': Exit Search Mode"),
            Spans::from("'n/N': Next/Previous Match (after commit)"),
            Spans::from("'Alt+j/k': Navigate Matches (after commit)"),
        ];

        let paragraph = Paragraph::new(text)
            .alignment(tui::layout::Alignment::Left)
            .block(block)
            .style(Style::default().fg(get_theme().text_secondary()))
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }
}
