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
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::block;

pub struct ConfigWidget {}

impl ConfigWidget {
    pub fn render(app: &App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let block = block::new(" Configuration ");

        let lines = match app.config_display_mode {
            ConfigMode::Selected => ConfigWidget::lines_for_selected_mode(app),
            ConfigMode::Global => ConfigWidget::lines_for_global_mode(app),
        };

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((app.config_paragraph_offset, 0));
        frame.render_widget(paragraph, area);
    }

    fn lines_for_global_mode(app: &App) -> Vec<Spans<'_>> {
        // The global view is static for the whole session, so build it once
        // instead of re-formatting every host on every frame. Frames borrow
        // the cached text rather than copying it.
        app.global_config_spans
            .get_or_init(|| {
                app.get_all_items_except_recents()
                    .into_iter()
                    .flat_map(ConfigWidget::ssh_group_item_to_owned_spans)
                    .collect()
            })
            .iter()
            .map(|spans| {
                Spans::from(
                    spans
                        .0
                        .iter()
                        .map(|span| Span::styled(span.content.as_ref(), span.style))
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    }

    fn lines_for_selected_mode(app: &App) -> Vec<Spans<'_>> {
        match app.get_selected_item() {
            Some(item) => ConfigWidget::ssh_group_item_to_spans(item),
            None => vec![Spans::from(Span::styled(
                "No item selected.\n",
                Style::default()
                    .fg(get_theme().text_secondary())
                    .add_modifier(Modifier::BOLD),
            ))],
        }
    }

    fn ssh_group_item_to_owned_spans(config: &SshGroupItem) -> Vec<Spans<'static>> {
        ConfigWidget::ssh_group_item_to_spans(config)
            .into_iter()
            .map(|spans| {
                Spans::from(
                    spans
                        .0
                        .into_iter()
                        .map(|span| Span::styled(span.content.into_owned(), span.style))
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    }

    fn ssh_group_item_to_spans(config: &SshGroupItem) -> Vec<Spans<'_>> {
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
                Span::styled(key, Style::default().fg(theme.text_primary())),
                Span::styled(" ", Style::default().fg(theme.text_secondary())),
                Span::styled(value, Style::default().fg(theme.text_secondary())),
            ]));
        });

        if let Some(comment) = &config.comment {
            spans.push(Spans::from(vec![Span::styled(
                "  Notes",
                Style::default().fg(theme.text_primary()),
            )]));

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
