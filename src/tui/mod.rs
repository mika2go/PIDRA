mod confirm;
mod details;
mod layout;
mod process_table;
mod restart_confirm;
mod theme;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::Paragraph,
};

use crate::app::{App, AppView, FocusColumn};

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
    match app.view {
        AppView::Details => {
            details::render(frame, app, options);
            return;
        }
        AppView::Confirm => {
            confirm::render(frame, app, options);
            return;
        }
        AppView::RestartConfirm => {
            restart_confirm::render(frame, app, options);
            return;
        }
        AppView::Table => {}
    }
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
    let system = format!(
        "CPU {}  MEM {}",
        percent(app.system_metrics.cpu_percent),
        percent(app.system_metrics.memory_used_percent)
    );
    frame.render_widget(
        Paragraph::new(system)
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

fn percent(value: Option<f32>) -> String {
    value.map_or_else(|| "--".to_owned(), |value| format!("{value:02.0}"))
}

#[must_use]
pub fn table_hit(area: ratatui::layout::Rect, app: &App, x: u16, y: u16) -> Option<TableHit> {
    if app.view != AppView::Table {
        return None;
    }
    let areas = layout::areas(area);
    process_table::hit_test(areas.table, app, x, y)
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf};

    use ratatui::{Terminal, backend::TestBackend};

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use crate::app::{App, AppView, FocusColumn};

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

    #[test]
    fn details_replace_the_table_and_show_risk_analysis() {
        let backend = TestBackend::new(110, 32);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = App::fixture();
        app.all_processes[0].executable = Some("/usr/bin/nira".into());
        app.focus = FocusColumn::Details;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

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
            .expect("render details");

        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("PROCESS TREE"));
        assert!(rendered.contains("TERMINATION ANALYSIS"));
        assert!(rendered.contains("CLOSE FROM APPLICATION FIRST"));
        assert!(!rendered.contains("PROCESS NAME"));
    }

    #[test]
    fn force_stop_confirmation_names_the_exact_target_and_risk() {
        let backend = TestBackend::new(110, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = App::fixture();
        let identity = app.processes[0].identity;
        let uid = rustix::process::getuid().as_raw();
        app.processes[0].uid = uid;
        app.all_processes[0].uid = uid;
        app.focus = FocusColumn::Details;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT));
        assert_eq!(app.view, AppView::Confirm);

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
            .expect("render confirmation");

        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("CONFIRM FORCE STOP"));
        assert!(rendered.contains(&format!("PID        {}", identity.pid)));
        assert!(rendered.contains("exact PID/start-time identity"));
        assert!(rendered.contains("SIGKILL gives the process no chance"));
        assert!(rendered.contains("ENTER / Y CONFIRM"));
    }

    #[test]
    fn restart_confirmation_explains_direct_exec_limitations() {
        let backend = TestBackend::new(110, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = App::fixture();
        let uid = rustix::process::getuid().as_raw();
        app.processes[0].uid = uid;
        app.processes[0].executable = Some(PathBuf::from("/usr/bin/sleep"));
        app.processes[0].cwd = Some(PathBuf::from("/tmp"));
        app.processes[0].command = vec![OsString::from("sleep"), OsString::from("30")];
        app.all_processes[0] = app.processes[0].clone();
        app.focus = FocusColumn::Restart;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.view, AppView::RestartConfirm);

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
            .expect("render restart confirmation");

        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("CONFIRM RESTART"));
        assert!(rendered.contains("DIRECT EXEC"));
        assert!(rendered.contains("cannot reconstruct the original environment"));
        assert!(rendered.contains("never uses a shell"));
    }
}
