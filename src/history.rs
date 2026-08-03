use std::{
    collections::VecDeque,
    env, fs,
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::process::ProcessIdentity;

const HISTORY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionHistoryEntry {
    pub sequence: u64,
    pub timestamp_unix_seconds: u64,
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
    persistence_path: Option<PathBuf>,
    persistence_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DiskEntry {
    schema_version: u32,
    sequence: u64,
    timestamp_unix_seconds: u64,
    process_name: String,
    pid: i32,
    start_time_ticks: u64,
    action: String,
    result: String,
}

impl ActionHistory {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            next_sequence: 1,
            capacity: capacity.max(1),
            persistence_path: None,
            persistence_error: None,
        }
    }

    pub fn persistent(path: PathBuf, capacity: usize) -> io::Result<Self> {
        let capacity = capacity.max(1);
        let mut history = Self {
            entries: load_entries(&path, capacity)?,
            next_sequence: 1,
            capacity,
            persistence_path: Some(path),
            persistence_error: None,
        };
        history.next_sequence = history
            .entries
            .back()
            .map_or(1, |entry| entry.sequence.saturating_add(1));
        Ok(history)
    }

    pub fn persistent_default(capacity: usize) -> io::Result<Self> {
        Self::persistent(default_history_path()?, capacity)
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
            timestamp_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            process_name,
            identity,
            action: action.into(),
            result: result.into(),
        });
        self.next_sequence = self.next_sequence.saturating_add(1);
        if let Some(path) = &self.persistence_path
            && let Err(error) = write_entries(path, &self.entries)
        {
            let message = format!("cannot persist action history {}: {error}", path.display());
            tracing::warn!(error = %error, path = %path.display(), "action history persistence failed");
            self.persistence_error = Some(message);
        }
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

    #[must_use]
    pub fn is_persistent(&self) -> bool {
        self.persistence_path.is_some()
    }

    #[must_use]
    pub fn persistence_path(&self) -> Option<&Path> {
        self.persistence_path.as_deref()
    }

    #[must_use]
    pub fn persistence_error(&self) -> Option<&str> {
        self.persistence_error.as_deref()
    }
}

impl Default for ActionHistory {
    fn default() -> Self {
        Self::new(100)
    }
}

fn default_history_path() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("pidra/history.jsonl"));
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".local/state/pidra/history.jsonl"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cannot determine state directory"))
}

fn load_entries(path: &Path, capacity: usize) -> io::Result<VecDeque<ActionHistoryEntry>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(VecDeque::new()),
        Err(error) => return Err(error),
    };
    let mut entries: Vec<_> = contents
        .lines()
        .filter_map(|line| serde_json::from_str::<DiskEntry>(line).ok())
        .filter(|entry| entry.schema_version == HISTORY_SCHEMA_VERSION)
        .map(|entry| ActionHistoryEntry {
            sequence: entry.sequence,
            timestamp_unix_seconds: entry.timestamp_unix_seconds,
            process_name: entry.process_name,
            identity: ProcessIdentity {
                pid: entry.pid,
                start_time_ticks: entry.start_time_ticks,
            },
            action: entry.action,
            result: entry.result,
        })
        .collect();
    entries.sort_by_key(|entry| entry.sequence);
    Ok(entries
        .into_iter()
        .rev()
        .take(capacity)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect())
}

fn write_entries(path: &Path, entries: &VecDeque<ActionHistoryEntry>) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "history path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("jsonl.tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        for entry in entries {
            let disk_entry = DiskEntry {
                schema_version: HISTORY_SCHEMA_VERSION,
                sequence: entry.sequence,
                timestamp_unix_seconds: entry.timestamp_unix_seconds,
                process_name: entry.process_name.clone(),
                pid: entry.identity.pid,
                start_time_ticks: entry.identity.start_time_ticks,
                action: entry.action.clone(),
                result: entry.result.clone(),
            };
            serde_json::to_writer(&mut file, &disk_entry)?;
            file.write_all(b"\n")?;
        }
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::ActionHistory;
    use crate::process::ProcessIdentity;

    fn record(history: &mut ActionHistory, pid: i32) {
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

    #[test]
    fn keeps_a_bounded_newest_first_session_history() {
        let mut history = ActionHistory::new(2);
        for pid in 1..=3 {
            record(&mut history, pid);
        }

        let pids: Vec<_> = history
            .newest_first()
            .map(|entry| entry.identity.pid)
            .collect();
        assert_eq!(pids, vec![3, 2]);
        assert!(!history.is_persistent());
    }

    #[test]
    fn persistent_history_reloads_bounded_entries_and_skips_bad_lines() {
        let directory = std::env::temp_dir().join(format!(
            "pidra-history-test-{}-{}",
            std::process::id(),
            SystemTimeSeed::next()
        ));
        fs::create_dir_all(&directory).expect("test directory");
        let path = directory.join("history.jsonl");
        let mut history = ActionHistory::persistent(path.clone(), 2).expect("history");
        for pid in 1..=3 {
            record(&mut history, pid);
        }
        drop(history);
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"not json\n"))
            .expect("append malformed line");

        let mut reloaded = ActionHistory::persistent(path.clone(), 2).expect("reload");
        assert_eq!(
            reloaded
                .newest_first()
                .map(|entry| entry.identity.pid)
                .collect::<Vec<_>>(),
            vec![3, 2]
        );
        record(&mut reloaded, 4);
        assert_eq!(reloaded.newest_first().next().expect("latest").sequence, 4);
        fs::remove_dir_all(directory).expect("cleanup test history");
    }

    struct SystemTimeSeed;

    impl SystemTimeSeed {
        fn next() -> u128 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        }
    }
}
