use super::block;
use crate::{get_theme, App};
use std::io::Stdout;
use std::ops::Range;
use tui::layout::Rect;
use tui::style::{Modifier, Style};
use tui::text::{Span, Spans};
use tui::widgets::Paragraph;
use tui::{backend::CrosstermBackend, Frame};
use unicode_width::UnicodeWidthStr;

const DIVIDER: &str = "│";
const MORE_LEFT: &str = "‹";
const MORE_RIGHT: &str = "›";

pub struct GroupsWidget {}

impl GroupsWidget {
    /// Group tabs in config order. When they do not all fit, a window that
    /// contains the selected group is shown, with `‹`/`›` marking hidden
    /// groups on either side.
    pub fn render(app: &App, area: Rect, frame: &mut Frame<CrosstermBackend<Stdout>>) {
        let theme = get_theme();
        let names: Vec<&str> = app.scs.groups.iter().map(|g| g.name.as_str()).collect();
        let title = format!(" Groups {}/{} ", app.selected_group + 1, names.len());
        let block = block::new(&title);

        let widths: Vec<usize> = names.iter().map(|name| name.width()).collect();
        let visible = visible_window(
            &widths,
            app.selected_group,
            area.width.saturating_sub(2) as usize,
        );

        let plain = Style::default().fg(theme.text_secondary());
        let selected = Style::default()
            .add_modifier(Modifier::BOLD)
            .bg(theme.text_primary());

        let mut spans = Vec::new();
        if visible.start > 0 {
            spans.push(Span::styled(MORE_LEFT, plain));
        }
        for idx in visible.clone() {
            if idx > visible.start {
                spans.push(Span::styled(DIVIDER, plain));
            }
            let style = if idx == app.selected_group {
                selected
            } else {
                plain
            };
            spans.push(Span::styled(format!(" {} ", names[idx]), style));
        }
        if visible.end < names.len() {
            spans.push(Span::styled(MORE_RIGHT, plain));
        }

        let paragraph = Paragraph::new(Spans::from(spans)).block(block);
        frame.render_widget(paragraph, area);
    }
}

/// Range of tabs, given their title `widths`, that fits in `available`
/// columns and contains `selected`: starts at the first tab when possible,
/// otherwise ends at `selected`. Each tab is padded by a space on both sides,
/// tabs are separated by a divider, and a column is kept for each `‹`/`›`
/// marker of hidden tabs.
fn visible_window(widths: &[usize], selected: usize, available: usize) -> Range<usize> {
    if widths.is_empty() {
        return 0..0;
    }
    let selected = selected.min(widths.len() - 1);

    let width = |range: &Range<usize>| {
        usize::from(range.start > 0)
            + range.clone().map(|idx| widths[idx] + 2).sum::<usize>()
            + range.len().saturating_sub(1)
            + usize::from(range.end < widths.len())
    };
    // Largest end such that `start..end` fits, at least one tab
    let fits_from = |start: usize| {
        (start + 1..=widths.len())
            .filter(|&end| width(&(start..end)) <= available)
            .max()
            .unwrap_or(start + 1)
    };

    let end = fits_from(0);
    if selected < end {
        return 0..end;
    }

    // Walk back from the selected tab while the window still reaches it
    let mut start = selected;
    while start > 0 && fits_from(start - 1) > selected {
        start -= 1;
    }
    start..fits_from(start).max(selected + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visible_window_everything_fits() {
        // " a │ bb │ ccc " = 3 + 1 + 4 + 1 + 5
        assert_eq!(visible_window(&[1, 2, 3], 2, 14), 0..3);
    }

    #[test]
    fn test_visible_window_scrolls_to_selected() {
        let widths = [6; 10]; // 8 columns per tab

        // From the start: 3 tabs + dividers + "›" = 8 * 3 + 2 + 1
        assert_eq!(visible_window(&widths, 0, 27), 0..3);
        assert_eq!(visible_window(&widths, 2, 27), 0..3);

        // Selected past the first window: it becomes the last visible tab,
        // with room for "‹" and "›"
        assert_eq!(visible_window(&widths, 5, 28), 3..6);
        assert_eq!(visible_window(&widths, 5, 27), 4..6);
        // No "›" needed at the end
        assert_eq!(visible_window(&widths, 9, 27), 7..10);
    }

    #[test]
    fn test_visible_window_always_shows_selected() {
        // Even when a single tab is wider than the bar
        assert_eq!(visible_window(&[30, 30], 1, 10), 1..2);
        assert_eq!(visible_window(&[], 0, 10), 0..0);
    }
}
