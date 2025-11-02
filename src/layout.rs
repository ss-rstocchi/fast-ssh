use std::io::Stdout;

use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::app::App;

pub struct AppLayout {
    pub groups_area: Rect,
    pub help_area: Rect,
    pub hosts_area: Rect,
    pub config_area: Rect,
    pub shortcuts_area: Option<Rect>,
    pub version_area: Rect,
}

pub fn create_layout(app: &App, frame: &mut Frame<CrosstermBackend<Stdout>>) -> AppLayout {
    let base_chunk = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .horizontal_margin(4)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(1),
        ].as_ref())
        .split(frame.size());

    let chunks_top = Layout::default()
        .direction(Direction::Horizontal)
        .margin(0)
        .constraints(
            [
                Constraint::Percentage(80),
                Constraint::Length(2),
                Constraint::Length(10),
            ]
            .as_ref(),
        )
        .split(base_chunk[0]);

    let constraints = match app.show_help {
        false => {
            vec![
                Constraint::Percentage(50),
                Constraint::Length(2),
                Constraint::Percentage(50),
            ]
        }
        true => {
            vec![
                Constraint::Percentage(40),
                Constraint::Length(2),
                Constraint::Percentage(30),
                Constraint::Length(2),
                Constraint::Percentage(30),
            ]
        }
    };

    let chunks_bot = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .horizontal_margin(0)
        .constraints(constraints.as_slice())
        .split(base_chunk[1]);

    AppLayout {
        groups_area: chunks_top[0],
        help_area: chunks_top[2],
        hosts_area: chunks_bot[0],
        config_area: chunks_bot[2],
        shortcuts_area: if app.show_help {
            Some(chunks_bot[4])
        } else {
            None
        },
        version_area: base_chunk[2],
    }
}
