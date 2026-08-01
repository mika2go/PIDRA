use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::process::{
    GuiClassification, GuiConfidence, ProcessIdentity, ProcessSnapshot, ScanBatch,
};

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
    pub all_processes: Vec<ProcessSnapshot>,
    pub gui_classifications: HashMap<ProcessIdentity, GuiClassification>,
    pub selected: usize,
    pub focus: FocusColumn,
    pub status: String,
    pub should_quit: bool,
    pub searching: bool,
    pub search_query: String,
}

impl App {
    #[must_use]
    pub fn graphical_total(&self) -> usize {
        self.gui_classifications.len()
    }

    #[must_use]
    pub fn new() -> Self {
        Self {
            processes: Vec::new(),
            all_processes: Vec::new(),
            gui_classifications: HashMap::new(),
            selected: 0,
            focus: FocusColumn::Restart,
            status: "Scanning /proc…".to_owned(),
            should_quit: false,
            searching: false,
            search_query: String::new(),
        }
    }

    #[must_use]
    pub fn fixture() -> Self {
        let processes = vec![
            ProcessSnapshot::fixture("nira", 18_422, 1_932_735_283),
            ProcessSnapshot::fixture("firefox", 2_204, 3_328_599_654),
            ProcessSnapshot::fixture("qs", 1_198, 432_013_312),
            ProcessSnapshot::fixture("pipewire", 806, 32_505_856),
            ProcessSnapshot::fixture("ffmpeg", 19_102, 0),
        ];
        let gui_classifications = processes
            .iter()
            .map(|process| {
                (
                    process.identity,
                    GuiClassification {
                        identity: process.identity,
                        confidence: GuiConfidence::Probable,
                        display_name: None,
                        application_scope: None,
                        evidence: vec!["Phase 0 fixture".to_owned()],
                    },
                )
            })
            .collect();
        Self {
            all_processes: processes.clone(),
            processes,
            gui_classifications,
            selected: 0,
            focus: FocusColumn::Restart,
            status: "Phase 0 fixture — no real process actions are enabled".to_owned(),
            should_quit: false,
            searching: false,
            search_query: String::new(),
        }
    }

    pub fn apply_scan_batch(&mut self, batch: ScanBatch) {
        let selected_identity = self
            .processes
            .get(self.selected)
            .map(|process| process.identity);
        self.all_processes = batch.processes;
        self.gui_classifications = batch
            .graphical
            .into_iter()
            .map(|classification| (classification.identity, classification))
            .collect();
        self.rebuild_visible(selected_identity);
        self.status = format!(
            "Showing {} GUI processes from {} scanned processes",
            self.processes.len(),
            self.all_processes.len()
        );
    }

    fn rebuild_visible(&mut self, selected_identity: Option<ProcessIdentity>) {
        let previous_index = self.selected;
        let query = self.search_query.to_lowercase();
        let mut processes: Vec<_> = self
            .all_processes
            .iter()
            .filter(|process| self.gui_classifications.contains_key(&process.identity))
            .filter(|process| {
                query.is_empty()
                    || process.name.to_lowercase().contains(&query)
                    || process.identity.pid.to_string().contains(&query)
            })
            .cloned()
            .collect();

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

        if self.searching {
            self.handle_search_key(key);
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
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Char('q' | 'Q') => self.should_quit = true,
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        let selected_identity = self
            .processes
            .get(self.selected)
            .map(|process| process.identity);
        match key.code {
            KeyCode::Esc | KeyCode::Enter => self.searching = false,
            KeyCode::Backspace => {
                self.search_query.pop();
                self.rebuild_visible(selected_identity);
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.search_query.push(character);
                self.rebuild_visible(selected_identity);
            }
            _ => {}
        }
        self.status = if self.search_query.is_empty() {
            "Search GUI process name or PID".to_owned()
        } else {
            format!(
                "Search /{} — {} matches",
                self.search_query,
                self.processes.len()
            )
        };
    }

    pub fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn select_next(&mut self) {
        if self.selected + 1 < self.processes.len() {
            self.selected += 1;
        }
    }

    pub fn select_from_pointer(&mut self, row: usize, focus: Option<FocusColumn>) {
        if row >= self.processes.len() {
            return;
        }
        let was_selected = self.selected == row;
        let was_focused = focus.is_some_and(|column| column == self.focus);
        self.selected = row;
        if let Some(column) = focus {
            self.focus = column;
            if was_selected && was_focused {
                self.activate_fixture_action();
            }
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

    use crate::process::{GuiClassification, GuiConfidence, ProcessSnapshot, ScanBatch};

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

        let graphical = app.gui_classifications.values().cloned().collect();
        app.apply_scan_batch(ScanBatch {
            processes: refreshed,
            graphical,
        });

        assert_eq!(app.processes[app.selected].identity, selected_identity);
    }

    #[test]
    fn search_filters_name_and_pid_without_quitting_on_q() {
        let mut app = App::fixture();
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('q')));

        assert!(app.searching);
        assert!(!app.should_quit);
        assert_eq!(app.processes.len(), 1);
        assert_eq!(app.processes[0].name, "qs");

        app.handle_key(key(KeyCode::Backspace));
        for character in "2204".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        assert_eq!(app.processes.len(), 1);
        assert_eq!(app.processes[0].name, "firefox");
    }

    #[test]
    fn ten_thousand_processes_remain_navigable() {
        let processes: Vec<_> = (1..=10_000)
            .map(|pid| ProcessSnapshot::fixture(&format!("app-{pid}"), pid, pid as u64))
            .collect();
        let graphical = processes
            .iter()
            .map(|process| GuiClassification {
                identity: process.identity,
                confidence: GuiConfidence::Probable,
                display_name: None,
                application_scope: None,
                evidence: vec!["load fixture".to_owned()],
            })
            .collect();
        let mut app = App::new();
        app.apply_scan_batch(ScanBatch {
            processes,
            graphical,
        });

        for _ in 0..9_999 {
            app.select_next();
        }

        assert_eq!(app.selected, 9_999);
        assert_eq!(app.processes.len(), 10_000);
    }

    #[test]
    fn second_click_on_selected_action_uses_keyboard_command() {
        let mut app = App::fixture();

        app.select_from_pointer(0, Some(FocusColumn::Details));
        assert!(!app.status.contains("Details for"));
        app.select_from_pointer(0, Some(FocusColumn::Details));

        assert!(app.status.contains("Details for nira"));
    }
}
