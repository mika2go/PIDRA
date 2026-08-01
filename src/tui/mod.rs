mod layout;
mod process_table;
mod theme;

use ratatui::{Frame, text::Line, widgets::Paragraph};

use crate::app::App;

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub ascii: bool,
    pub no_color: bool,
}

pub fn render(frame: &mut Frame<'_>, app: &App, options: RenderOptions) {
    let areas = layout::areas(frame.area());
    let palette = theme::Palette::new(options.no_color);

    let title = Line::from(vec![
        "PIDRA".into(),
        format!("{:>42} GUI PROCESSES", app.processes.len()).into(),
        "       CPU 07  MEM 24".into(),
    ])
    .style(palette.header());
    frame.render_widget(Paragraph::new(title), areas.header);

    process_table::render(frame, areas.table, app, options, &palette);

    frame.render_widget(
        Paragraph::new(app.status.as_str()).style(palette.status()),
        areas.status,
    );
    let footer = if options.ascii {
        "UP/DOWN SELECT   LEFT/RIGHT ACTION   ENTER ACTIVATE   / SEARCH   Q QUIT"
    } else {
        "↑↓ SELECT   ←→ ACTION   ENTER ACTIVATE   / SEARCH   Q QUIT"
    };
    frame.render_widget(Paragraph::new(footer).style(palette.footer()), areas.footer);
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
}
