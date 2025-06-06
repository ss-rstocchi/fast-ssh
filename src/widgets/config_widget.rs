use crate::{
    app::{App, ConfigDisplayMode as ConfigMode},
    ssh_config_store::SshGroupItem,
    THEME,
};
use std::io::Stdout;
use tui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};

use super::block;

pub struct ConfigWidget {}

impl ConfigWidget {
    pub fn render(app: &mut App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let block = block::new(" Configuration ");

        let paragraph = match app.config_display_mode {
            ConfigMode::Selected => ConfigWidget::get_paragraph_for_selected_mode(app, block),
            ConfigMode::Global => ConfigWidget::get_paragraph_for_global_mode(app, block),
        };

        frame.render_widget(paragraph.scroll((app.config_paragraph_offset, 0)), area);
    }

    fn get_paragraph_for_global_mode<'a>(app: &'a App, block: Block<'a>) -> Paragraph<'a> {
        let spans: Vec<Spans> = app
            .get_all_items()
            .iter()
            .flat_map(|item| ConfigWidget::ssh_group_item_to_spans(item))
            .collect();

        Paragraph::new(spans)
            .block(block)
            .wrap(Wrap { trim: false })
    }

    fn get_paragraph_for_selected_mode<'a>(app: &'a App, block: Block<'a>) -> Paragraph<'a> {
        let mut spans = vec![Spans::from(Span::styled(
            "No item selected.\n",
            Style::default()
                .fg(THEME.text_secondary())
                .add_modifier(Modifier::BOLD),
        ))];

        let config = &app.get_selected_item();

        if let Some(config) = config {
            spans = ConfigWidget::ssh_group_item_to_spans(config);
        }

        Paragraph::new(spans)
            .block(block)
            .wrap(Wrap { trim: false })
    }

    fn ssh_group_item_to_spans(config: &SshGroupItem) -> Vec<Spans> {
        let mut spans = Vec::new();

        if config.full_name.contains('/') {
            let parts: Vec<&str> = config.full_name.split('/').collect();
            if parts.len() >= 2 {
                spans.push(Spans::from(vec![
                    Span::styled("Host ", Style::default().fg(THEME.text_primary())),
                    Span::styled(
                        parts[1],
                        Style::default().fg(THEME.text_secondary()),
                    ),
                ]));
                spans.push(Spans::from(vec![
                    Span::styled("  Group ", Style::default().fg(THEME.text_primary())),
                    Span::styled(
                        parts[0].replace('_', " "),
                        Style::default().fg(THEME.text_secondary()),
                    ),
                ]));
            }
        } else {
            spans.push(Spans::from(vec![
                Span::styled("Host ", Style::default().fg(THEME.text_primary())),
                Span::styled(
                    &config.full_name,
                    Style::default().fg(THEME.text_secondary()),
                ),
            ]));
        }

        config.host_config.iter().for_each(|(key, value)| {
            spans.push(Spans::from(vec![
                Span::styled("  ", Style::default().fg(THEME.text_primary())),
                Span::styled(key.to_string(), Style::default().fg(THEME.text_primary())),
                Span::styled(" ", Style::default().fg(THEME.text_secondary())),
                Span::styled(value, Style::default().fg(THEME.text_secondary())),
            ]));
        });

        if let Some(comment) = &config.comment {
            spans.push(Spans::from(vec![
                Span::styled("  Notes", Style::default().fg(THEME.text_primary())),
            ]));

            for line in comment.lines() {
                spans.push(Spans::from(vec![
                    Span::styled("    ", Style::default().fg(THEME.text_primary())),
                    Span::styled(line, Style::default().fg(THEME.text_secondary())),
                ]));
            }
        }

        spans.push(Spans::from(vec![Span::styled(
            "\n",
            Style::default().fg(THEME.text_secondary()),
        )]));

        spans
    }
}
