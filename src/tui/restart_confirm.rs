use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    text::Line,
    widgets::Paragraph,
};

use crate::{
    app::App,
    control::{restart::RestartSource, risk::assess_termination},
    tui::{RenderOptions, theme::Palette},
};

pub fn render(frame: &mut Frame<'_>, app: &App, options: RenderOptions) {
    let palette = Palette::new(options.no_color);
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(15),
            Constraint::Fill(1),
        ])
        .split(area);
    let Some(confirmation) = app.restart_confirmation.as_ref() else {
        frame.render_widget(Paragraph::new("Restart target unavailable"), rows[1]);
        return;
    };
    let assessment = app
        .process_by_identity(confirmation.identity)
        .map(|process| {
            assess_termination(
                process,
                &app.all_processes,
                i32::try_from(std::process::id()).unwrap_or(i32::MAX),
            )
        });
    let (source, detail, warning) = match &confirmation.source {
        RestartSource::SystemdUserUnit { unit } => (
            "SYSTEMD USER UNIT".to_owned(),
            unit.clone(),
            "systemd will restart the exact validated service unit".to_owned(),
        ),
        RestartSource::Direct {
            executable,
            arguments,
            working_directory,
        } => (
            "DIRECT EXEC".to_owned(),
            format!(
                "{}  |  {} arguments  |  cwd {}",
                executable.display(),
                arguments.len(),
                working_directory.display()
            ),
            "PIDRA cannot reconstruct the original environment completely".to_owned(),
        ),
        RestartSource::Unavailable { reason } => (
            "UNAVAILABLE".to_owned(),
            reason.clone(),
            "restart cannot continue".to_owned(),
        ),
    };
    let lines = vec![
        Line::styled("CONFIRM RESTART", palette.header()),
        Line::from(""),
        Line::from(format!("PROCESS    {}", confirmation.process_name)),
        Line::from(format!("PID        {}", confirmation.identity.pid)),
        Line::from(format!(
            "START      {} ticks",
            confirmation.identity.start_time_ticks
        )),
        Line::from(format!("SOURCE     {source}")),
        Line::from(format!("DETAIL     {detail}")),
        Line::from(format!(
            "ASSESSMENT {}",
            assessment
                .as_ref()
                .map_or("UNKNOWN", |value| value.rating.label())
        )),
        Line::from(""),
        Line::from(warning),
        Line::from("The old PID/start-time identity is revalidated before any action."),
        Line::from("Direct restart sends SIGTERM and aborts if the old process stays alive."),
        Line::from("PIDRA never uses a shell and never escalates restart to SIGKILL."),
        Line::from(""),
        Line::styled("ENTER / Y CONFIRM     ESC / N CANCEL", palette.header()),
    ];
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Left), rows[1]);
}
