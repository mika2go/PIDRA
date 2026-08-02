use std::collections::VecDeque;

use crate::process::ProcessIdentity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionHistoryEntry {
    pub sequence: u64,
    pub process_name: String,
    pub identity: ProcessIdentity,
    pub action: String,
    pub result: String,
}

#[derive(Debug)]
pub struct ActionHistory {
    entries: VecDeque<ActionHistoryEntry>,
    next_sequence: u64,
    capacity: usize,
}

impl ActionHistory {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            next_sequence: 1,
            capacity: capacity.max(1),
        }
    }

    pub fn record(
        &mut self,
        process_name: String,
        identity: ProcessIdentity,
        action: impl Into<String>,
        result: impl Into<String>,
    ) {
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(ActionHistoryEntry {
            sequence: self.next_sequence,
            process_name,
            identity,
            action: action.into(),
            result: result.into(),
        });
        self.next_sequence = self.next_sequence.saturating_add(1);
    }

    pub fn newest_first(&self) -> impl Iterator<Item = &ActionHistoryEntry> {
        self.entries.iter().rev()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ActionHistory {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::ActionHistory;
    use crate::process::ProcessIdentity;

    #[test]
    fn keeps_a_bounded_newest_first_session_history() {
        let mut history = ActionHistory::new(2);
        for pid in 1..=3 {
            history.record(
                format!("process-{pid}"),
                ProcessIdentity {
                    pid,
                    start_time_ticks: pid as u64,
                },
                "STOP",
                "EXITED",
            );
        }

        let pids: Vec<_> = history
            .newest_first()
            .map(|entry| entry.identity.pid)
            .collect();
        assert_eq!(pids, vec![3, 2]);
    }
}
