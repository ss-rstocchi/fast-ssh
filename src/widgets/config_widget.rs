use crate::{
    app::{App, ConfigDisplayMode as ConfigMode},
    get_theme,
    ssh_config_store::SshGroupItem,
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
    pub fn render(app: &App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
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
                .fg(get_theme().text_secondary())
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
        let theme = get_theme();
        let mut spans = Vec::new();

        // Use split_once for safer string parsing
        if let Some((group, host)) = config.full_name.split_once('/') {
            spans.push(Spans::from(vec![
                Span::styled("Host ", Style::default().fg(theme.text_primary())),
                Span::styled(host, Style::default().fg(theme.text_secondary())),
            ]));
            spans.push(Spans::from(vec![
                Span::styled("  Group ", Style::default().fg(theme.text_primary())),
                Span::styled(
                    group.replace('_', " "),
                    Style::default().fg(theme.text_secondary()),
                ),
            ]));
        } else {
            spans.push(Spans::from(vec![
                Span::styled("Host ", Style::default().fg(theme.text_primary())),
                Span::styled(
                    &config.full_name,
                    Style::default().fg(theme.text_secondary()),
                ),
            ]));
        }

        config.host_config.iter().for_each(|(key, value)| {
            spans.push(Spans::from(vec![
                Span::styled("  ", Style::default().fg(theme.text_primary())),
                Span::styled(key.to_string(), Style::default().fg(theme.text_primary())),
                Span::styled(" ", Style::default().fg(theme.text_secondary())),
                Span::styled(value, Style::default().fg(theme.text_secondary())),
            ]));
        });

        if let Some(comment) = &config.comment {
            spans.push(Spans::from(vec![
                Span::styled("  Notes", Style::default().fg(theme.text_primary())),
            ]));

            for line in comment.lines() {
                spans.push(Spans::from(vec![
                    Span::styled("    ", Style::default().fg(theme.text_primary())),
                    Span::styled(line, Style::default().fg(theme.text_secondary())),
                ]));
            }
        }

        spans.push(Spans::from(vec![Span::styled(
            "\n",
            Style::default().fg(theme.text_secondary()),
        )]));

        spans
    }
}
