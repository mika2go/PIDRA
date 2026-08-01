#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessIdentity {
    pub pid: i32,
    pub start_time_ticks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Sleeping,
    DiskSleep,
    Stopped,
    Zombie,
    Dead,
    Unknown(char),
}

impl ProcessState {
    #[must_use]
    pub fn from_procfs(value: char) -> Self {
        match value {
            'R' => Self::Running,
            'S' | 'I' => Self::Sleeping,
            'D' => Self::DiskSleep,
            'T' | 't' => Self::Stopped,
            'Z' => Self::Zombie,
            'X' | 'x' => Self::Dead,
            other => Self::Unknown(other),
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Sleeping => "SLEEPING",
            Self::DiskSleep => "DISK SLEEP",
            Self::Stopped => "FROZEN",
            Self::Zombie => "ZOMBIE",
            Self::Dead => "DEAD",
            Self::Unknown(_) => "UNKNOWN",
        }
    }
}
