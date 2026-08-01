use std::{io, time::Duration};

use crossterm::event::{self, Event};
use ratatui::{Terminal, backend::Backend};

use crate::{app::App, tui};

pub fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    options: tui::RenderOptions,
    refresh_interval: Duration,
) -> io::Result<()>
where
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let mut dirty = true;
    let mut last_draw = std::time::Instant::now()
        .checked_sub(refresh_interval)
        .unwrap_or_else(std::time::Instant::now);

    while !app.should_quit {
        if dirty || last_draw.elapsed() >= refresh_interval {
            terminal
                .draw(|frame| tui::render(frame, app, options))
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
                Event::FocusGained | Event::FocusLost | Event::Mouse(_) | Event::Paste(_) => {}
            }
        }
    }

    Ok(())
}
