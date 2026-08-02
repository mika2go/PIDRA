use ratatui::{Frame, text::Line, widgets::Paragraph};

use crate::tui::{RenderOptions, theme::Palette};

pub fn render(frame: &mut Frame<'_>, options: RenderOptions) {
    let palette = Palette::new(options.no_color);
    let lines = vec![
        Line::styled("PIDRA HELP", palette.header()),
        Line::from("Keyboard-first Linux process control for detected GUI application roots."),
        Line::from(""),
        Line::styled("TABLE", palette.table_header()),
        Line::from("Up/Down row   Left/Right action   Enter use   / search"),
        Line::from("R restart column   S stop column   D details column"),
        Line::from("H session history   ? help   Q quit"),
        Line::from(""),
        Line::styled("DETAILS", palette.table_header()),
        Line::from("Up/Down tree node   Left/Right collapse/expand   Esc back"),
        Line::from("R restart   F freeze/resume   T SIGTERM   Shift+K confirmed SIGKILL"),
        Line::from(""),
        Line::styled("SAFETY", palette.table_header()),
        Line::from("Every signal validates PID plus process start time; pidfd is preferred."),
        Line::from("Stop and restart never escalate automatically to SIGKILL."),
        Line::from("Force Stop is Details-only and always requires explicit confirmation."),
        Line::from("Analysis is advisory: closing software can still lose unsaved user data."),
        Line::from(
            "PID 1, PIDRA, its ancestors and essential graphical-session services are protected.",
        ),
        Line::from(""),
        Line::styled("ESC / ? BACK     Q QUIT", palette.footer()),
    ];
    frame.render_widget(Paragraph::new(lines), frame.area());
}
