use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::process::{
    GuiClassification, GuiConfidence, ProcessIdentity, ProcessSnapshot, ScanBatch,
    cpu::SystemMetrics,
    tree::{ProcessTree, TreeNode},
};
use crate::{
    control::{
        ControlOutcome, ControlRequest, ControlResult, SignalAction,
        restart::{RestartRequest, RestartResult, RestartSource, resolve_restart_source},
        risk::assess_termination,
    },
    process::ProcessState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    Table,
    Details,
    Confirm,
    RestartConfirm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmation {
    pub identity: ProcessIdentity,
    pub process_name: String,
    pub action: SignalAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartConfirmation {
    pub identity: ProcessIdentity,
    pub process_name: String,
    pub source: RestartSource,
    pub return_to: AppView,
}

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
    pub system_metrics: SystemMetrics,
    pub selected: usize,
    pub focus: FocusColumn,
    pub status: String,
    pub should_quit: bool,
    pub searching: bool,
    pub search_query: String,
    pub view: AppView,
    pub details_root: Option<ProcessIdentity>,
    pub details_selected: usize,
    pub expanded_nodes: HashSet<ProcessIdentity>,
    pub confirmation: Option<Confirmation>,
    pub restart_confirmation: Option<RestartConfirmation>,
    pending_control: VecDeque<ControlRequest>,
    pending_restarts: VecDeque<RestartRequest>,
    pending_observation: HashMap<ProcessIdentity, (SignalAction, Instant)>,
    pub latest_actions: HashMap<ProcessIdentity, String>,
}

impl App {
    #[must_use]
    pub fn graphical_total(&self) -> usize {
        self.gui_classifications.len()
    }

    #[must_use]
    pub fn restart_source_for(&self, identity: ProcessIdentity) -> RestartSource {
        self.process_by_identity(identity).map_or_else(
            || RestartSource::Unavailable {
                reason: "process identity no longer exists".to_owned(),
            },
            resolve_restart_source,
        )
    }

    #[must_use]
    pub fn new() -> Self {
        Self {
            processes: Vec::new(),
            all_processes: Vec::new(),
            gui_classifications: HashMap::new(),
            system_metrics: SystemMetrics::default(),
            selected: 0,
            focus: FocusColumn::Restart,
            status: "Scanning /proc…".to_owned(),
            should_quit: false,
            searching: false,
            search_query: String::new(),
            view: AppView::Table,
            details_root: None,
            details_selected: 0,
            expanded_nodes: HashSet::new(),
            confirmation: None,
            restart_confirmation: None,
            pending_control: VecDeque::new(),
            pending_restarts: VecDeque::new(),
            pending_observation: HashMap::new(),
            latest_actions: HashMap::new(),
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
            system_metrics: SystemMetrics::default(),
            selected: 0,
            focus: FocusColumn::Restart,
            status: "Phase 0 fixture — no real process actions are enabled".to_owned(),
            should_quit: false,
            searching: false,
            search_query: String::new(),
            view: AppView::Table,
            details_root: None,
            details_selected: 0,
            expanded_nodes: HashSet::new(),
            confirmation: None,
            restart_confirmation: None,
            pending_control: VecDeque::new(),
            pending_restarts: VecDeque::new(),
            pending_observation: HashMap::new(),
            latest_actions: HashMap::new(),
        }
    }

    pub fn apply_scan_batch(&mut self, batch: ScanBatch) {
        let selected_identity = self
            .processes
            .get(self.selected)
            .map(|process| process.identity);
        self.all_processes = batch.processes;
        self.system_metrics = batch.system;
        self.gui_classifications = batch
            .graphical
            .into_iter()
            .map(|classification| (classification.identity, classification))
            .collect();
        self.rebuild_visible(selected_identity);
        self.observe_pending_actions();
        let details_missing = self
            .details_root
            .is_some_and(|identity| self.process_by_identity(identity).is_none());
        if details_missing {
            self.status = "The detailed process identity no longer exists".to_owned();
        } else if self.view == AppView::Table {
            self.status = format!(
                "Showing {} GUI processes from {} scanned processes",
                self.processes.len(),
                self.all_processes.len()
            );
        }
        self.clamp_details_selection();
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

        match self.view {
            AppView::Details => {
                self.handle_details_key(key);
                return;
            }
            AppView::Confirm => {
                self.handle_confirmation_key(key);
                return;
            }
            AppView::RestartConfirm => {
                self.handle_restart_confirmation_key(key);
                return;
            }
            AppView::Table => {}
        }

        match key.code {
            KeyCode::Up => self.select_previous(),
            KeyCode::Down => self.select_next(),
            KeyCode::Left => self.focus = self.focus.previous(),
            KeyCode::Right => self.focus = self.focus.next(),
            KeyCode::Char('r' | 'R') => self.focus = FocusColumn::Restart,
            KeyCode::Char('s' | 'S') => self.focus = FocusColumn::Stop,
            KeyCode::Char('d' | 'D') => self.focus = FocusColumn::Details,
            KeyCode::Enter => self.activate_focused_action(),
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Char('q' | 'Q') => self.should_quit = true,
            _ => {}
        }
    }

    fn handle_details_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.view = AppView::Table;
                self.status = "Returned to GUI process table".to_owned();
            }
            KeyCode::Char('q' | 'Q') => self.should_quit = true,
            KeyCode::Up => self.details_selected = self.details_selected.saturating_sub(1),
            KeyCode::Down => {
                let node_count = self.detail_nodes().len();
                if self.details_selected + 1 < node_count {
                    self.details_selected += 1;
                }
            }
            KeyCode::Right | KeyCode::Enter => self.expand_selected_detail_node(),
            KeyCode::Left => self.collapse_or_select_parent(),
            KeyCode::Char('f' | 'F') => {
                let action = self.selected_detail_process().map(|process| {
                    if process.state == ProcessState::Stopped {
                        SignalAction::Resume
                    } else {
                        SignalAction::Freeze
                    }
                });
                if let Some(action) = action {
                    self.queue_selected_detail_action(action);
                }
            }
            KeyCode::Char('t' | 'T') => self.queue_selected_detail_action(SignalAction::Stop),
            KeyCode::Char('r' | 'R') => self.open_selected_restart_confirmation(AppView::Details),
            KeyCode::Char('K') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.open_force_stop_confirmation();
            }
            _ => {}
        }
    }

    fn handle_confirmation_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y' | 'Y') => {
                if let Some(confirmation) = self.confirmation.take() {
                    self.pending_control.push_back(ControlRequest {
                        identity: confirmation.identity,
                        action: confirmation.action,
                    });
                    self.status = format!(
                        "Queued {} for {} ({})",
                        confirmation.action.label(),
                        confirmation.process_name,
                        confirmation.identity.pid
                    );
                }
                self.view = AppView::Details;
            }
            KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                self.confirmation = None;
                self.view = AppView::Details;
                self.status = "Force Stop cancelled".to_owned();
            }
            _ => {}
        }
    }

    fn handle_restart_confirmation_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y' | 'Y') => {
                if let Some(confirmation) = self.restart_confirmation.take() {
                    let return_to = confirmation.return_to;
                    self.status = format!(
                        "Queued RESTART for {} ({})",
                        confirmation.process_name, confirmation.identity.pid
                    );
                    self.pending_restarts.push_back(RestartRequest {
                        identity: confirmation.identity,
                        process_name: confirmation.process_name,
                        source: confirmation.source,
                    });
                    self.view = return_to;
                }
            }
            KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                if let Some(confirmation) = self.restart_confirmation.take() {
                    self.view = confirmation.return_to;
                } else {
                    self.view = AppView::Table;
                }
                self.status = "Restart cancelled".to_owned();
            }
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
                self.activate_focused_action();
            }
        }
    }

    fn activate_focused_action(&mut self) {
        let Some(process) = self.processes.get(self.selected).cloned() else {
            return;
        };

        match self.focus {
            FocusColumn::Restart => {
                self.open_restart_confirmation(&process, AppView::Table);
            }
            FocusColumn::Stop => {
                self.queue_action(&process, SignalAction::Stop);
            }
            FocusColumn::Details => {
                self.view = AppView::Details;
                self.details_root = Some(process.identity);
                self.details_selected = 0;
                self.expanded_nodes.clear();
                self.expanded_nodes.insert(process.identity);
                self.status = format!("Inspecting {} ({})", process.name, process.identity.pid);
            }
        }
    }

    #[must_use]
    pub fn detail_nodes(&self) -> Vec<TreeNode> {
        let Some(root) = self.details_root else {
            return Vec::new();
        };
        ProcessTree::new(&self.all_processes).visible_nodes(root, &self.expanded_nodes)
    }

    #[must_use]
    pub fn selected_detail_process(&self) -> Option<&ProcessSnapshot> {
        let node = self.detail_nodes().get(self.details_selected).copied()?;
        self.process_by_identity(node.identity)
    }

    #[must_use]
    pub fn process_by_identity(&self, identity: ProcessIdentity) -> Option<&ProcessSnapshot> {
        self.all_processes
            .iter()
            .find(|process| process.identity == identity)
    }

    fn expand_selected_detail_node(&mut self) {
        let Some(node) = self.detail_nodes().get(self.details_selected).copied() else {
            return;
        };
        if node.has_children {
            self.expanded_nodes.insert(node.identity);
        } else {
            self.status = "Selected process has no child processes".to_owned();
        }
    }

    fn collapse_or_select_parent(&mut self) {
        let nodes = self.detail_nodes();
        let Some(node) = nodes.get(self.details_selected).copied() else {
            return;
        };
        if node.expanded && node.has_children {
            self.expanded_nodes.remove(&node.identity);
            self.clamp_details_selection();
            return;
        }
        if node.depth == 0 {
            return;
        }
        if let Some(parent_index) = nodes[..self.details_selected]
            .iter()
            .rposition(|candidate| candidate.depth < node.depth)
        {
            self.details_selected = parent_index;
        }
    }

    fn clamp_details_selection(&mut self) {
        self.details_selected = self
            .details_selected
            .min(self.detail_nodes().len().saturating_sub(1));
    }

    fn queue_selected_detail_action(&mut self, action: SignalAction) {
        let Some(process) = self.selected_detail_process().cloned() else {
            self.status = "Selected process identity no longer exists".to_owned();
            return;
        };
        self.queue_action(&process, action);
    }

    fn queue_action(&mut self, process: &ProcessSnapshot, action: SignalAction) {
        let assessment = assess_termination(
            process,
            &self.all_processes,
            i32::try_from(std::process::id()).unwrap_or(i32::MAX),
        );
        if assessment.rating == crate::control::risk::RiskRating::Protected {
            self.status = format!("Action blocked: {}", assessment.evidence.join("; "));
            return;
        }
        self.pending_control.push_back(ControlRequest {
            identity: process.identity,
            action,
        });
        self.status = format!(
            "Queued {} for {} ({})",
            action.label(),
            process.name,
            process.identity.pid
        );
    }

    fn open_force_stop_confirmation(&mut self) {
        let Some(process) = self.selected_detail_process().cloned() else {
            return;
        };
        let assessment = assess_termination(
            &process,
            &self.all_processes,
            i32::try_from(std::process::id()).unwrap_or(i32::MAX),
        );
        if assessment.rating == crate::control::risk::RiskRating::Protected {
            self.status = format!("Force Stop blocked: {}", assessment.evidence.join("; "));
            return;
        }
        self.confirmation = Some(Confirmation {
            identity: process.identity,
            process_name: process.name,
            action: SignalAction::ForceStop,
        });
        self.view = AppView::Confirm;
    }

    fn open_selected_restart_confirmation(&mut self, return_to: AppView) {
        let Some(process) = self.selected_detail_process().cloned() else {
            self.status = "Selected process identity no longer exists".to_owned();
            return;
        };
        self.open_restart_confirmation(&process, return_to);
    }

    fn open_restart_confirmation(&mut self, process: &ProcessSnapshot, return_to: AppView) {
        let assessment = assess_termination(
            process,
            &self.all_processes,
            i32::try_from(std::process::id()).unwrap_or(i32::MAX),
        );
        if assessment.rating == crate::control::risk::RiskRating::Protected {
            self.status = format!("Restart blocked: {}", assessment.evidence.join("; "));
            return;
        }
        let source = self.restart_source_for(process.identity);
        if let RestartSource::Unavailable { reason } = source {
            self.status = format!(
                "Restart unavailable for {} ({}): {reason}",
                process.name, process.identity.pid
            );
            return;
        }
        self.restart_confirmation = Some(RestartConfirmation {
            identity: process.identity,
            process_name: process.name.clone(),
            source,
            return_to,
        });
        self.view = AppView::RestartConfirm;
    }

    pub fn take_control_requests(&mut self) -> impl Iterator<Item = ControlRequest> + '_ {
        self.pending_control.drain(..)
    }

    pub fn take_restart_requests(&mut self) -> impl Iterator<Item = RestartRequest> + '_ {
        self.pending_restarts.drain(..)
    }

    pub fn report_control_dispatch_error(&mut self, error: &str) {
        self.status = format!("Control worker error: {error}");
    }

    pub fn report_restart_dispatch_error(&mut self, error: &str) {
        self.status = format!("Restart worker error: {error}");
    }

    pub fn apply_restart_result(&mut self, result: RestartResult) {
        let message = format!("RESTART: {}", result.outcome.message());
        self.latest_actions
            .insert(result.request.identity, message.clone());
        self.status = message;
    }

    pub fn apply_control_result(&mut self, result: ControlResult) {
        let message = format!(
            "{}: {}",
            result.request.action.label(),
            result.outcome.message()
        );
        self.latest_actions
            .insert(result.request.identity, message.clone());
        self.status = message;
        if matches!(result.outcome, ControlOutcome::Sent(_)) {
            self.pending_observation.insert(
                result.request.identity,
                (result.request.action, Instant::now()),
            );
        }
    }

    fn observe_pending_actions(&mut self) {
        let now = Instant::now();
        let mut finished = Vec::new();
        for (identity, (action, started_at)) in &self.pending_observation {
            let current = self.process_by_identity(*identity);
            let observation = match (action, current) {
                (SignalAction::Stop | SignalAction::ForceStop, None) => Some("EXITED"),
                (SignalAction::Freeze, Some(process)) if process.state == ProcessState::Stopped => {
                    Some("FROZEN")
                }
                (SignalAction::Resume, Some(process)) if process.state != ProcessState::Stopped => {
                    Some("RESUMED")
                }
                (_, None) => Some("NOT FOUND"),
                (SignalAction::Stop, Some(_))
                    if now.saturating_duration_since(*started_at) >= Duration::from_secs(2) =>
                {
                    Some("STILL RUNNING — no automatic SIGKILL escalation")
                }
                _ => None,
            };
            if let Some(observation) = observation {
                self.latest_actions
                    .insert(*identity, format!("{}: {observation}", action.label()));
                finished.push(*identity);
            }
        }
        for identity in finished {
            self.pending_observation.remove(&identity);
        }
    }

    #[must_use]
    pub fn latest_action_for(&self, identity: ProcessIdentity) -> Option<&str> {
        self.latest_actions.get(&identity).map(String::as_str)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf};

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use crate::control::{ControlOutcome, ControlResult, SignalAction};
    use crate::process::{
        GuiClassification, GuiConfidence, ProcessSnapshot, ScanBatch, cpu::SystemMetrics,
    };

    use super::{App, AppView, FocusColumn};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn open_fixture_details(app: &mut App) {
        let identity = app.processes[0].identity;
        let uid = rustix::process::getuid().as_raw();
        app.processes
            .iter_mut()
            .find(|process| process.identity == identity)
            .expect("visible fixture process")
            .uid = uid;
        app.all_processes
            .iter_mut()
            .find(|process| process.identity == identity)
            .expect("fixture process")
            .uid = uid;
        app.focus = FocusColumn::Details;
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.view, AppView::Details);
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
            system: SystemMetrics::default(),
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
            system: SystemMetrics::default(),
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
        assert_eq!(app.view, AppView::Table);
        app.select_from_pointer(0, Some(FocusColumn::Details));

        assert_eq!(app.view, AppView::Details);
        assert!(app.status.contains("Inspecting nira"));
    }

    #[test]
    fn details_expand_children_and_return_to_table() {
        let mut app = App::fixture();
        let mut child = ProcessSnapshot::fixture("renderer", 18_423, 512);
        child.parent_pid = Some(18_422);
        app.all_processes.push(child);
        app.focus = FocusColumn::Details;

        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.view, AppView::Details);
        assert_eq!(app.detail_nodes().len(), 2);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(
            app.selected_detail_process().map(|p| p.name.as_str()),
            Some("renderer")
        );
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.details_selected, 0);
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.view, AppView::Table);
    }

    #[test]
    fn table_stop_queues_sigterm_for_the_exact_identity() {
        let mut app = App::fixture();
        let expected = app.processes[0].identity;
        let uid = rustix::process::getuid().as_raw();
        app.processes[0].uid = uid;
        app.all_processes[0].uid = uid;
        app.focus = FocusColumn::Stop;

        app.handle_key(key(KeyCode::Enter));

        let requests: Vec<_> = app.take_control_requests().collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].identity, expected);
        assert_eq!(requests[0].action, SignalAction::Stop);
    }

    #[test]
    fn force_stop_requires_explicit_confirmation_and_can_be_cancelled() {
        let mut app = App::fixture();
        open_fixture_details(&mut app);
        let expected = app
            .selected_detail_process()
            .expect("detail target")
            .identity;

        app.handle_key(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT));

        assert_eq!(app.view, AppView::Confirm);
        let confirmation = app.confirmation.as_ref().expect("confirmation");
        assert_eq!(confirmation.identity, expected);
        assert_eq!(confirmation.action, SignalAction::ForceStop);
        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.view, AppView::Details);
        assert!(app.confirmation.is_none());
        assert_eq!(app.take_control_requests().count(), 0);
    }

    #[test]
    fn force_stop_confirmation_queues_sigkill_only_after_acceptance() {
        let mut app = App::fixture();
        open_fixture_details(&mut app);
        let expected = app
            .selected_detail_process()
            .expect("detail target")
            .identity;
        app.handle_key(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT));

        app.handle_key(key(KeyCode::Char('y')));

        assert_eq!(app.view, AppView::Details);
        let requests: Vec<_> = app.take_control_requests().collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].identity, expected);
        assert_eq!(requests[0].action, SignalAction::ForceStop);
    }

    #[test]
    fn sent_signal_is_reported_as_pending_kernel_observation() {
        let mut app = App::fixture();
        let identity = app.processes[0].identity;
        app.apply_control_result(ControlResult {
            request: crate::control::ControlRequest {
                identity,
                action: SignalAction::Stop,
            },
            outcome: ControlOutcome::Sent(crate::control::DeliveryMethod::Pidfd),
        });

        assert!(
            app.latest_action_for(identity)
                .is_some_and(|message| message.contains("pidfd"))
        );
    }

    fn prepare_direct_restart(app: &mut App) {
        let identity = app.processes[0].identity;
        let uid = rustix::process::getuid().as_raw();
        for process in app
            .processes
            .iter_mut()
            .chain(app.all_processes.iter_mut())
            .filter(|process| process.identity == identity)
        {
            process.uid = uid;
            process.executable = Some(PathBuf::from("/usr/bin/sleep"));
            process.cwd = Some(PathBuf::from("/tmp"));
            process.command = vec![OsString::from("sleep"), OsString::from("30")];
        }
    }

    #[test]
    fn restart_requires_confirmation_and_cancel_queues_nothing() {
        let mut app = App::fixture();
        prepare_direct_restart(&mut app);
        app.focus = FocusColumn::Restart;

        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.view, AppView::RestartConfirm);
        assert!(app.restart_confirmation.is_some());
        app.handle_key(key(KeyCode::Char('q')));
        assert!(!app.should_quit);
        assert_eq!(app.view, AppView::RestartConfirm);
        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.view, AppView::Table);
        assert_eq!(app.take_restart_requests().count(), 0);
    }

    #[test]
    fn accepted_restart_keeps_the_resolved_source_and_identity() {
        let mut app = App::fixture();
        prepare_direct_restart(&mut app);
        let expected = app.processes[0].identity;
        app.focus = FocusColumn::Restart;
        app.handle_key(key(KeyCode::Enter));

        app.handle_key(key(KeyCode::Char('y')));

        let requests: Vec<_> = app.take_restart_requests().collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].identity, expected);
        assert!(matches!(
            requests[0].source,
            crate::control::restart::RestartSource::Direct { .. }
        ));
    }
}
