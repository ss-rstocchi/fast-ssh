use std::io::Stdout;

use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::app::App;

/// Below this width the hosts and config panes stack vertically and the help
/// box moves into the footer, so host names get the full terminal width.
const NARROW_WIDTH: u16 = 100;
/// Below this height rows drop their spacing and outer margins shrink.
const SHORT_HEIGHT: u16 = 30;

pub struct AppLayout {
    pub groups_area: Rect,
    /// `None` when narrow; the footer shows the help hint instead
    pub help_area: Option<Rect>,
    pub hosts_area: Rect,
    /// `None` when stacked and the shortcuts panel takes its place
    pub config_area: Option<Rect>,
    pub shortcuts_area: Option<Rect>,
    pub version_area: Rect,
    /// Render host rows without blank spacing lines
    pub dense: bool,
}

pub fn create_layout(app: &App, frame: &mut Frame<CrosstermBackend<Stdout>>) -> AppLayout {
    let size = frame.size();
    let narrow = size.width < NARROW_WIDTH;
    let short = size.height < SHORT_HEIGHT;

    let base_chunk = Layout::default()
        .direction(Direction::Vertical)
        .vertical_margin(u16::from(!short))
        .horizontal_margin(if narrow { 1 } else { 4 })
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(6),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(size);

    let (groups_area, help_area) = if narrow {
        (base_chunk[0], None)
    } else {
        let chunks_top = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(80),
                    Constraint::Length(2),
                    Constraint::Length(10),
                ]
                .as_ref(),
            )
            .split(base_chunk[0]);
        (chunks_top[0], Some(chunks_top[2]))
    };

    let gap = u16::from(!short);
    let (direction, constraints) = match (narrow, app.show_help) {
        (false, false) => (
            Direction::Horizontal,
            vec![
                Constraint::Percentage(50),
                Constraint::Length(2),
                Constraint::Percentage(50),
            ],
        ),
        (false, true) => (
            Direction::Horizontal,
            vec![
                Constraint::Percentage(40),
                Constraint::Length(2),
                Constraint::Percentage(30),
                Constraint::Length(2),
                Constraint::Percentage(30),
            ],
        ),
        // Stacked: the shortcuts panel replaces the config pane
        (true, _) => (
            Direction::Vertical,
            vec![
                Constraint::Percentage(60),
                Constraint::Length(gap),
                Constraint::Percentage(40),
            ],
        ),
    };

    let chunks_bot = Layout::default()
        .direction(direction)
        .vertical_margin(gap)
        .constraints(constraints.as_slice())
        .split(base_chunk[1]);

    let (config_area, shortcuts_area) = match (narrow, app.show_help) {
        (false, false) => (Some(chunks_bot[2]), None),
        (false, true) => (Some(chunks_bot[2]), Some(chunks_bot[4])),
        (true, false) => (Some(chunks_bot[2]), None),
        (true, true) => (None, Some(chunks_bot[2])),
    };

    AppLayout {
        groups_area,
        help_area,
        hosts_area: chunks_bot[0],
        config_area,
        shortcuts_area,
        version_area: base_chunk[2],
        dense: narrow || short,
    }
}
