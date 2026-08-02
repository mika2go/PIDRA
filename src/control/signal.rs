use std::{fs, io, path::Path};

use rustix::{
    io::Errno,
    process::{Pid, PidfdFlags, Signal, kill_process, pidfd_open, pidfd_send_signal},
};

use crate::process::{ProcessIdentity, procfs::parse_stat};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalAction {
    Stop,
    Freeze,
    Resume,
    ForceStop,
}

impl SignalAction {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Stop => "STOP (SIGTERM)",
            Self::Freeze => "FREEZE (SIGSTOP)",
            Self::Resume => "RESUME (SIGCONT)",
            Self::ForceStop => "FORCE STOP (SIGKILL)",
        }
    }

    fn signal(self) -> Signal {
        match self {
            Self::Stop => Signal::TERM,
            Self::Freeze => Signal::STOP,
            Self::Resume => Signal::CONT,
            Self::ForceStop => Signal::KILL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryMethod {
    Pidfd,
    ValidatedKillFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlOutcome {
    Sent(DeliveryMethod),
    IdentityChanged,
    NotFound,
    PermissionDenied,
    Protected(String),
    Unsupported(String),
    Failed(String),
}

impl ControlOutcome {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Sent(DeliveryMethod::Pidfd) => "signal delivered through pidfd".to_owned(),
            Self::Sent(DeliveryMethod::ValidatedKillFallback) => {
                "signal delivered through validated kill(2) fallback".to_owned()
            }
            Self::IdentityChanged => "PID now belongs to a different process identity".to_owned(),
            Self::NotFound => "process identity no longer exists".to_owned(),
            Self::PermissionDenied => "permission denied by the kernel".to_owned(),
            Self::Protected(reason) => format!("protected target: {reason}"),
            Self::Unsupported(reason) => format!("unsupported: {reason}"),
            Self::Failed(reason) => format!("signal failed: {reason}"),
        }
    }
}

#[must_use]
pub fn send_signal(identity: ProcessIdentity, action: SignalAction) -> ControlOutcome {
    send_signal_at(Path::new("/proc"), identity, action)
}

#[must_use]
pub fn send_signal_at(
    proc_root: &Path,
    identity: ProcessIdentity,
    action: SignalAction,
) -> ControlOutcome {
    if identity.pid == 1 {
        return ControlOutcome::Protected("PID 1 is never a valid target".to_owned());
    }
    if identity.pid == i32::try_from(std::process::id()).unwrap_or(i32::MAX) {
        return ControlOutcome::Protected("PIDRA cannot signal itself".to_owned());
    }
    let Some(pid) = Pid::from_raw(identity.pid) else {
        return ControlOutcome::NotFound;
    };

    if let Err(outcome) = validate_identity(proc_root, identity) {
        return outcome;
    }
    match pidfd_open(pid, PidfdFlags::empty()) {
        Ok(pidfd) => {
            if let Err(outcome) = validate_identity(proc_root, identity) {
                return outcome;
            }
            match pidfd_send_signal(&pidfd, action.signal()) {
                Ok(()) => ControlOutcome::Sent(DeliveryMethod::Pidfd),
                Err(error) => map_signal_error(error),
            }
        }
        Err(error) if pidfd_unavailable(error) => {
            if let Err(outcome) = validate_identity(proc_root, identity) {
                return outcome;
            }
            match kill_process(pid, action.signal()) {
                Ok(()) => ControlOutcome::Sent(DeliveryMethod::ValidatedKillFallback),
                Err(error) => map_signal_error(error),
            }
        }
        Err(error) => map_signal_error(error),
    }
}

pub fn read_identity(proc_root: &Path, pid: i32) -> Result<ProcessIdentity, ControlOutcome> {
    let stat = fs::read(proc_root.join(pid.to_string()).join("stat")).map_err(map_read_error)?;
    let parsed = parse_stat(&stat)
        .map_err(|error| ControlOutcome::Failed(format!("invalid stat data: {error}")))?;
    Ok(ProcessIdentity {
        pid: parsed.pid,
        start_time_ticks: parsed.start_time_ticks,
    })
}

fn validate_identity(proc_root: &Path, expected: ProcessIdentity) -> Result<(), ControlOutcome> {
    let current = read_identity(proc_root, expected.pid)?;
    if current == expected {
        Ok(())
    } else {
        Err(ControlOutcome::IdentityChanged)
    }
}

fn map_read_error(error: io::Error) -> ControlOutcome {
    match error.kind() {
        io::ErrorKind::NotFound => ControlOutcome::NotFound,
        io::ErrorKind::PermissionDenied => ControlOutcome::PermissionDenied,
        _ => ControlOutcome::Failed(error.to_string()),
    }
}

fn pidfd_unavailable(error: Errno) -> bool {
    matches!(
        error,
        Errno::NOSYS | Errno::INVAL | Errno::NODEV | Errno::OPNOTSUPP
    )
}

fn map_signal_error(error: Errno) -> ControlOutcome {
    match error {
        Errno::SRCH => ControlOutcome::NotFound,
        Errno::PERM | Errno::ACCESS => ControlOutcome::PermissionDenied,
        Errno::NOSYS | Errno::OPNOTSUPP => ControlOutcome::Unsupported(error.to_string()),
        _ => ControlOutcome::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{ControlOutcome, SignalAction, send_signal_at};
    use crate::process::ProcessIdentity;

    #[test]
    fn protects_pid_one_before_accessing_procfs() {
        let result = send_signal_at(
            std::path::Path::new("/does/not/exist"),
            ProcessIdentity {
                pid: 1,
                start_time_ticks: 1,
            },
            SignalAction::Stop,
        );

        assert!(matches!(result, ControlOutcome::Protected(_)));
    }

    #[test]
    fn protects_pidra_before_accessing_procfs() {
        let result = send_signal_at(
            std::path::Path::new("/does/not/exist"),
            ProcessIdentity {
                pid: i32::try_from(std::process::id()).expect("PIDRA PID fits i32"),
                start_time_ticks: 1,
            },
            SignalAction::ForceStop,
        );

        assert!(matches!(result, ControlOutcome::Protected(_)));
    }
}
