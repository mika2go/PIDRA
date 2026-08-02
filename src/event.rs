use std::{io, time::Duration};

use crossterm::event::{self, Event, MouseButton, MouseEventKind};
use ratatui::{Terminal, backend::Backend, layout::Rect};

use crate::{app::App, control::ControlWorker, process::ScanWorker, tui};

pub fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    scanner: &ScanWorker,
    control: &ControlWorker,
    options: tui::RenderOptions,
    refresh_interval: Duration,
) -> io::Result<()>
where
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let mut dirty = true;
    let mut frame_area = Rect::default();
    let mut last_draw = std::time::Instant::now()
        .checked_sub(refresh_interval)
        .unwrap_or_else(std::time::Instant::now);

    while !app.should_quit {
        let requests: Vec<_> = app.take_control_requests().collect();
        for request in requests {
            if let Err(error) = control.request(request) {
                app.report_control_dispatch_error(&error);
            }
        }
        while let Some(result) = control.try_result() {
            app.apply_control_result(result);
            dirty = true;
        }

        if let Some(message) = scanner.try_latest() {
            let _captured_at = message.captured_at;
            match message.result {
                Ok(batch) => app.apply_scan_batch(batch),
                Err(error) => app.report_scan_error(&error),
            }
            dirty = true;
        }

        if dirty || last_draw.elapsed() >= refresh_interval {
            terminal
                .draw(|frame| {
                    frame_area = frame.area();
                    tui::render(frame, app, options);
                })
                .map_err(io::Error::other)?;
            dirty = false;
            last_draw = std::time::Instant::now();
        }

        let until_refresh = refresh_interval.saturating_sub(last_draw.elapsed());
        let poll_timeout = until_refresh.min(Duration::from_millis(100));
        if event::poll(poll_timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    app.handle_key(key);
                    dirty = true;
                }
                Event::Resize(_, _) => dirty = true,
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => app.select_previous(),
                        MouseEventKind::ScrollDown => app.select_next(),
                        MouseEventKind::Down(MouseButton::Left) => {
                            if let Some(hit) =
                                tui::table_hit(frame_area, app, mouse.column, mouse.row)
                            {
                                app.select_from_pointer(hit.row, hit.focus);
                            }
                        }
                        MouseEventKind::Down(MouseButton::Right | MouseButton::Middle)
                        | MouseEventKind::Up(_)
                        | MouseEventKind::Drag(_)
                        | MouseEventKind::Moved
                        | MouseEventKind::ScrollLeft
                        | MouseEventKind::ScrollRight => {}
                    }
                    dirty = true;
                }
                Event::FocusGained | Event::FocusLost | Event::Paste(_) => {}
            }
        }
    }

    Ok(())
}
