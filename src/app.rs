use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusColumn {
    Restart,
    Stop,
    Details,
}

impl FocusColumn {
    fn previous(self) -> Self {
        match self {
            Self::Restart => Self::Restart,
            Self::Stop => Self::Restart,
            Self::Details => Self::Stop,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Restart => Self::Stop,
            Self::Stop => Self::Details,
            Self::Details => Self::Details,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureProcess {
    pub name: &'static str,
    pub pid: i32,
    pub rss_bytes: u64,
    pub restart_available: bool,
}

#[derive(Debug)]
pub struct App {
    pub processes: Vec<FixtureProcess>,
    pub selected: usize,
    pub focus: FocusColumn,
    pub status: String,
    pub should_quit: bool,
}

impl App {
    #[must_use]
    pub fn fixture() -> Self {
        Self {
            processes: vec![
                FixtureProcess {
                    name: "nira",
                    pid: 18_422,
                    rss_bytes: 1_932_735_283,
                    restart_available: true,
                },
                FixtureProcess {
                    name: "firefox",
                    pid: 2_204,
                    rss_bytes: 3_328_599_654,
                    restart_available: true,
                },
                FixtureProcess {
                    name: "qs",
                    pid: 1_198,
                    rss_bytes: 432_013_312,
                    restart_available: true,
                },
                FixtureProcess {
                    name: "pipewire",
                    pid: 806,
                    rss_bytes: 32_505_856,
                    restart_available: true,
                },
                FixtureProcess {
                    name: "ffmpeg",
                    pid: 19_102,
                    rss_bytes: 0,
                    restart_available: false,
                },
            ],
            selected: 0,
            focus: FocusColumn::Restart,
            status: "Phase 0 fixture — no real process actions are enabled".to_owned(),
            should_quit: false,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c' | 'C'))
        {
            self.should_quit = true;
            return;
        }

        match key.code {
            KeyCode::Up => self.select_previous(),
            KeyCode::Down => self.select_next(),
            KeyCode::Left => self.focus = self.focus.previous(),
            KeyCode::Right => self.focus = self.focus.next(),
            KeyCode::Char('r' | 'R') => self.focus = FocusColumn::Restart,
            KeyCode::Char('s' | 'S') => self.focus = FocusColumn::Stop,
            KeyCode::Char('d' | 'D') => self.focus = FocusColumn::Details,
            KeyCode::Enter => self.activate_fixture_action(),
            KeyCode::Char('q' | 'Q') => self.should_quit = true,
            _ => {}
        }
    }

    fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn select_next(&mut self) {
        if self.selected + 1 < self.processes.len() {
            self.selected += 1;
        }
    }

    fn activate_fixture_action(&mut self) {
        let Some(process) = self.processes.get(self.selected) else {
            return;
        };

        self.status = match self.focus {
            FocusColumn::Restart if process.restart_available => {
                format!("Fixture only: restart {} ({})", process.name, process.pid)
            }
            FocusColumn::Restart => {
                format!("Restart unavailable for {} ({})", process.name, process.pid)
            }
            FocusColumn::Stop => {
                format!("Fixture only: stop {} ({})", process.name, process.pid)
            }
            FocusColumn::Details => {
                format!("Fixture only: details {} ({})", process.name, process.pid)
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{App, FocusColumn};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn moves_rows_without_leaving_bounds() {
        let mut app = App::fixture();

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, 0);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, 1);

        for _ in 0..20 {
            app.handle_key(key(KeyCode::Down));
        }
        assert_eq!(app.selected, app.processes.len() - 1);
    }

    #[test]
    fn changes_action_focus() {
        let mut app = App::fixture();

        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.focus, FocusColumn::Stop);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.focus, FocusColumn::Details);
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.focus, FocusColumn::Stop);
    }

    #[test]
    fn letter_shortcuts_focus_without_activating() {
        let mut app = App::fixture();
        let original_status = app.status.clone();

        app.handle_key(key(KeyCode::Char('d')));

        assert_eq!(app.focus, FocusColumn::Details);
        assert_eq!(app.status, original_status);
    }

    #[test]
    fn quit_key_requests_exit() {
        let mut app = App::fixture();

        app.handle_key(key(KeyCode::Char('q')));

        assert!(app.should_quit);
    }
}
