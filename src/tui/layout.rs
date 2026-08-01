use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy)]
pub struct Areas {
    pub header: Rect,
    pub table: Rect,
    pub status: Rect,
    pub footer: Rect,
}

pub fn areas(area: Rect) -> Areas {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    Areas {
        header: rows[0],
        table: rows[2],
        status: rows[3],
        footer: rows[4],
    }
}
