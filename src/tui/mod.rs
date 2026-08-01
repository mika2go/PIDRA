mod layout;
mod process_table;
mod theme;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::Paragraph,
};

use crate::app::{App, FocusColumn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableHit {
    pub row: usize,
    pub focus: Option<FocusColumn>,
}

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub ascii: bool,
    pub no_color: bool,
}

pub fn render(frame: &mut Frame<'_>, app: &App, options: RenderOptions) {
    let areas = layout::areas(frame.area());
    let palette = theme::Palette::new(options.no_color);

    let header_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(18),
            Constraint::Min(22),
            Constraint::Length(18),
        ])
        .split(areas.header);
    frame.render_widget(
        Paragraph::new("PIDRA").style(palette.header()),
        header_columns[0],
    );
    let count = if app.search_query.is_empty() {
        format!("{} GUI PROCESSES", app.graphical_total())
    } else {
        format!(
            "{} / {} GUI  /{}",
            app.processes.len(),
            app.graphical_total(),
            app.search_query
        )
    };
    frame.render_widget(
        Paragraph::new(count)
            .alignment(Alignment::Center)
            .style(palette.header()),
        header_columns[1],
    );
    frame.render_widget(
        Paragraph::new("CPU --  MEM --")
            .alignment(Alignment::Right)
            .style(palette.header()),
        header_columns[2],
    );

    process_table::render(frame, areas.table, app, options, &palette);

    frame.render_widget(
        Paragraph::new(app.status.as_str()).style(palette.status()),
        areas.status,
    );
    let footer = if app.searching {
        if options.ascii {
            "SEARCH: TYPE NAME OR PID   BACKSPACE DELETE   ENTER/ESC CLOSE"
        } else {
            "SEARCH: TYPE NAME OR PID   ⌫ DELETE   ENTER/ESC CLOSE"
        }
    } else if options.ascii {
        "UP/DOWN SELECT   LEFT/RIGHT ACTION   ENTER ACTIVATE   / SEARCH   Q QUIT"
    } else {
        "↑↓ SELECT   ←→ ACTION   ENTER ACTIVATE   / SEARCH   Q QUIT"
    };
    frame.render_widget(Paragraph::new(footer).style(palette.footer()), areas.footer);
}

#[must_use]
pub fn table_hit(area: ratatui::layout::Rect, app: &App, x: u16, y: u16) -> Option<TableHit> {
    let areas = layout::areas(area);
    process_table::hit_test(areas.table, app, x, y)
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use crate::app::App;

    use super::{RenderOptions, render};

    #[test]
    fn renders_the_phase_zero_surface() {
        let backend = TestBackend::new(100, 14);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let app = App::fixture();

        terminal
            .draw(|frame| {
                render(
                    frame,
                    &app,
                    RenderOptions {
                        ascii: false,
                        no_color: true,
                    },
                );
            })
            .expect("render fixture");

        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("PIDRA"));
        assert!(rendered.contains("PROCESS NAME"));
        assert!(rendered.contains("firefox"));
        assert!(rendered.contains("ENTER ACTIVATE"));
    }

    #[test]
    fn renders_a_narrow_ascii_no_color_surface() {
        let backend = TestBackend::new(44, 10);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let app = App::fixture();

        terminal
            .draw(|frame| {
                render(
                    frame,
                    &app,
                    RenderOptions {
                        ascii: true,
                        no_color: true,
                    },
                );
            })
            .expect("render compact fixture");

        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("PIDRA"));
        assert!(rendered.contains("PROCESS NAME"));
        assert!(rendered.contains(">nira"));
    }
}
