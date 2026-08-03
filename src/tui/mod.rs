mod confirm;
mod details;
mod help;
mod history;
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
        AppView::History => {
            history::render(frame, app, options);
            return;
        }
        AppView::Help => {
            help::render(frame, options);
            return;
        }
        AppView::Table | AppView::Developer => {}
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
    let count = if app.developer_layer_active() {
        if app.search_query.is_empty() {
            format!("DEV / SERVER {}  [V] GUI", app.developer_total())
        } else {
            format!(
                "{} / {} DEV  /{}",
                app.processes.len(),
                app.developer_total(),
                app.search_query
            )
        }
    } else if app.search_query.is_empty() {
        format!(
            "{} GUI  [V] {} DEV",
            app.graphical_total(),
            app.developer_total()
        )
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
        if app.developer_layer_active() {
            "V/ESC GUI  UP/DOWN ROW  LEFT/RIGHT ACTION  ENTER USE  O SORT  / SEARCH  H HISTORY  ? HELP  Q QUIT"
        } else {
            "V DEV  UP/DOWN ROW  LEFT/RIGHT ACTION  ENTER USE  O SORT  / SEARCH  H HISTORY  ? HELP  Q QUIT"
        }
    } else {
        if app.developer_layer_active() {
            "V/ESC GUI  ↑↓ ROW  ←→ ACTION  ENTER USE  O SORT  / SEARCH  H HISTORY  ? HELP  Q QUIT"
        } else {
            "V DEV  ↑↓ ROW  ←→ ACTION  ENTER USE  O SORT  / SEARCH  H HISTORY  ? HELP  Q QUIT"
        }
    };
    frame.render_widget(Paragraph::new(footer).style(palette.footer()), areas.footer);
}

fn percent(value: Option<f32>) -> String {
    value.map_or_else(|| "--".to_owned(), |value| format!("{value:02.0}"))
}

#[must_use]
pub fn table_hit(area: ratatui::layout::Rect, app: &App, x: u16, y: u16) -> Option<TableHit> {
    if !matches!(app.view, AppView::Table | AppView::Developer) {
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

    use crate::{
        app::{App, AppView, FocusColumn},
        process::{
            DeveloperClassification, DeveloperKind, ProcessSnapshot, ScanBatch, cpu::SystemMetrics,
        },
    };

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
        assert!(rendered.contains("ENTER USE"));
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
    fn developer_layer_and_details_show_the_classification_evidence() {
        let backend = TestBackend::new(110, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut server = ProcessSnapshot::fixture("vite", 5173, 32_000_000);
        server.uid = rustix::process::getuid().as_raw();
        server.executable = Some(PathBuf::from("/usr/bin/node"));
        let developer = vec![DeveloperClassification {
            identity: server.identity,
            kind: DeveloperKind::ListeningServer,
            endpoints: vec!["TCP port 5173".to_owned()],
            evidence: vec!["owns 1 TCP listening socket".to_owned()],
        }];
        let mut app = App::new();
        app.apply_scan_batch(ScanBatch {
            processes: vec![server],
            graphical: Vec::new(),
            developer,
            system: SystemMetrics::default(),
        });
        app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));

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
            .expect("render developer layer");
        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("DEV / SERVER 1"));
        assert!(rendered.contains("vite"));
        assert!(rendered.contains("protected targets are excluded"));

        app.focus = FocusColumn::Details;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
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
            .expect("render developer details");
        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("DEVELOPER / SERVER EVIDENCE"));
        assert!(rendered.contains("TCP port 5173"));
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
        assert!(rendered.contains("APP TREE"));
        assert!(rendered.contains("TREND"));
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

    #[test]
    fn action_history_replaces_the_table_and_shows_identity_and_result() {
        let backend = TestBackend::new(100, 18);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = App::fixture();
        let identity = app.processes[0].identity;
        app.history.record(
            "nira".to_owned(),
            identity,
            "STOP (SIGTERM)",
            "EXITED; CHILDREN REMAIN — 18423",
        );
        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));

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
            .expect("render action history");

        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("PIDRA ACTION HISTORY"));
        assert!(rendered.contains("STOP (SIGTERM)"));
        assert!(rendered.contains("CHILDREN REMAIN"));
        assert!(rendered.contains(&format!(
            "PID {} / {}",
            identity.pid, identity.start_time_ticks
        )));
        assert!(!rendered.contains("PROCESS NAME"));
    }

    #[test]
    fn help_replaces_the_table_and_states_the_safety_contract() {
        let backend = TestBackend::new(100, 22);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = App::fixture();
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT));

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
            .expect("render help");

        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("PIDRA HELP"));
        assert!(rendered.contains("PID plus process start time"));
        assert!(rendered.contains("never escalate automatically"));
        assert!(!rendered.contains("PROCESS NAME"));
    }

    #[test]
    fn ascii_no_color_details_expose_frozen_as_text() {
        let backend = TestBackend::new(100, 26);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = App::fixture();
        app.processes[0].state = crate::process::ProcessState::Stopped;
        app.all_processes[0].state = crate::process::ProcessState::Stopped;
        app.focus = FocusColumn::Details;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

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
            .expect("render frozen details");

        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("FROZEN"));
        assert!(rendered.contains("F RESUME"));
    }

    #[test]
    fn tiny_viewports_render_without_panicking() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).expect("tiny test terminal");
        let mut app = App::fixture();

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
            .expect("render tiny table");
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT));
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
            .expect("render tiny help");
    }
}
