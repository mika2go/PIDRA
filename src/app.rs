use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::process::{ProcessIdentity, ProcessSnapshot};

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

#[derive(Debug)]
pub struct App {
    pub processes: Vec<ProcessSnapshot>,
    pub selected: usize,
    pub focus: FocusColumn,
    pub status: String,
    pub should_quit: bool,
}

impl App {
    #[must_use]
    pub fn new() -> Self {
        Self {
            processes: Vec::new(),
            selected: 0,
            focus: FocusColumn::Restart,
            status: "Scanning /proc…".to_owned(),
            should_quit: false,
        }
    }

    #[must_use]
    pub fn fixture() -> Self {
        Self {
            processes: vec![
                ProcessSnapshot::fixture("nira", 18_422, 1_932_735_283),
                ProcessSnapshot::fixture("firefox", 2_204, 3_328_599_654),
                ProcessSnapshot::fixture("qs", 1_198, 432_013_312),
                ProcessSnapshot::fixture("pipewire", 806, 32_505_856),
                ProcessSnapshot::fixture("ffmpeg", 19_102, 0),
            ],
            selected: 0,
            focus: FocusColumn::Restart,
            status: "Phase 0 fixture — no real process actions are enabled".to_owned(),
            should_quit: false,
        }
    }

    pub fn apply_snapshot(&mut self, mut processes: Vec<ProcessSnapshot>) {
        let selected_identity = self
            .processes
            .get(self.selected)
            .map(|process| process.identity);
        let previous_index = self.selected;

        processes.sort_by(|left, right| {
            right
                .rss_bytes
                .cmp(&left.rss_bytes)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.identity.pid.cmp(&right.identity.pid))
        });
        self.processes = processes;
        self.selected = selected_identity
            .and_then(|identity| self.index_of(identity))
            .unwrap_or_else(|| previous_index.min(self.processes.len().saturating_sub(1)));
        self.status = format!("Read {} processes from /proc", self.processes.len());
    }

    pub fn report_scan_error(&mut self, error: &str) {
        self.status = format!("Process scan failed: {error}");
    }

    fn index_of(&self, identity: ProcessIdentity) -> Option<usize> {
        self.processes
            .iter()
            .position(|process| process.identity == identity)
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
            FocusColumn::Restart => format!(
                "Restart unavailable for {} ({}) until Phase 5",
                process.name, process.identity.pid
            ),
            FocusColumn::Stop => {
                format!(
                    "Stop disabled for {} ({}) until safe signalling is implemented",
                    process.name, process.identity.pid
                )
            }
            FocusColumn::Details => {
                format!(
                    "Details for {} ({}) arrive in Phase 3",
                    process.name, process.identity.pid
                )
            }
        };
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn refresh_preserves_selection_by_identity() {
        let mut app = App::fixture();
        app.selected = 2;
        let selected_identity = app.processes[2].identity;
        let mut refreshed = app.processes.clone();
        refreshed[2].rss_bytes = u64::MAX;

        app.apply_snapshot(refreshed);

        assert_eq!(app.processes[app.selected].identity, selected_identity);
    }
}
