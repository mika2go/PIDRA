use ratatui::{Frame, text::Line, widgets::Paragraph};

use crate::{
    app::App,
    tui::{RenderOptions, theme::Palette},
};

pub fn render(frame: &mut Frame<'_>, app: &App, options: RenderOptions) {
    let palette = Palette::new(options.no_color);
    let area = frame.area();
    let mut lines = vec![
        Line::styled("PIDRA ACTION HISTORY", palette.header()),
        Line::from(format!(
            "{} completed action{} {}",
            app.history.len(),
            if app.history.len() == 1 { "" } else { "s" },
            if app.history.is_persistent() {
                "in bounded persistent history"
            } else {
                "in this session"
            }
        )),
        Line::from(""),
    ];
    if let Some(path) = app.history.persistence_path() {
        lines.push(Line::from(format!("STORAGE  {}", path.display())));
    }
    if let Some(error) = app.history.persistence_error() {
        lines.push(Line::from(format!("WARNING  {error}")));
    }
    if app.history.is_empty() {
        lines.push(Line::from("No completed process actions yet."));
    } else {
        let capacity = usize::from(area.height.saturating_sub(5)) / 2;
        for entry in app.history.newest_first().take(capacity) {
            lines.push(Line::styled(
                format!(
                    "#{:03}  {}  {}  PID {} / {}",
                    entry.sequence,
                    entry.action,
                    entry.process_name,
                    entry.identity.pid,
                    entry.identity.start_time_ticks
                ),
                palette.table_header(),
            ));
            lines.push(Line::from(format!("      {}", entry.result)));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "ESC / H BACK     ? HELP     Q QUIT",
        palette.footer(),
    ));
    frame.render_widget(Paragraph::new(lines), area);
}
