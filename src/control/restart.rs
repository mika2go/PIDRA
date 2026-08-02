use std::{ffi::OsString, path::PathBuf};

use crate::process::{ProcessSnapshot, ProcessState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartSource {
    SystemdUserUnit {
        unit: String,
    },
    Direct {
        executable: PathBuf,
        arguments: Vec<OsString>,
        working_directory: PathBuf,
    },
    Unavailable {
        reason: String,
    },
}

impl RestartSource {
    #[must_use]
    pub fn is_available(&self) -> bool {
        !matches!(self, Self::Unavailable { .. })
    }

    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::SystemdUserUnit { unit } => format!("systemd user unit {unit}"),
            Self::Direct {
                executable,
                arguments,
                working_directory,
            } => format!(
                "direct {} ({} arguments, cwd {})",
                executable.display(),
                arguments.len(),
                working_directory.display()
            ),
            Self::Unavailable { reason } => format!("unavailable — {reason}"),
        }
    }
}

#[must_use]
pub fn resolve_restart_source(process: &ProcessSnapshot) -> RestartSource {
    if process.uid != rustix::process::getuid().as_raw() {
        return unavailable("process is not owned by the current user");
    }
    if matches!(
        process.state,
        ProcessState::Zombie | ProcessState::Dead | ProcessState::DiskSleep
    ) {
        return unavailable("process state does not permit a controlled restart");
    }
    if let Some(unit) = systemd_service_unit(&process.cgroups) {
        return RestartSource::SystemdUserUnit { unit };
    }
    let Some(executable) = process.executable.clone() else {
        return unavailable("executable path is unreadable or this is a kernel thread");
    };
    if !executable.is_absolute() {
        return unavailable("executable path is not absolute");
    }
    let Some(working_directory) = process.cwd.clone() else {
        return unavailable("working directory is unreadable");
    };
    if !working_directory.is_absolute() {
        return unavailable("working directory is not absolute");
    }

    RestartSource::Direct {
        executable,
        arguments: process.command.iter().skip(1).cloned().collect(),
        working_directory,
    }
}

fn unavailable(reason: &str) -> RestartSource {
    RestartSource::Unavailable {
        reason: reason.to_owned(),
    }
}

fn systemd_service_unit(cgroups: &[String]) -> Option<String> {
    cgroups.iter().find_map(|entry| {
        let path = entry
            .split_once("::")
            .map_or(entry.as_str(), |(_, path)| path);
        path.rsplit('/').find_map(|component| {
            let is_service = component.ends_with(".service");
            let is_user_manager = component.starts_with("user@");
            let is_session_core = matches!(
                component,
                "dbus.service" | "pipewire.service" | "wireplumber.service"
            );
            (is_service && !is_user_manager && !is_session_core).then(|| component.to_owned())
        })
    })
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf};

    use super::{RestartSource, resolve_restart_source};
    use crate::process::{ProcessSnapshot, ProcessState};

    fn owned_process() -> ProcessSnapshot {
        let mut process = ProcessSnapshot::fixture("demo", 200, 1);
        process.uid = rustix::process::getuid().as_raw();
        process.executable = Some(PathBuf::from("/usr/bin/demo"));
        process.cwd = Some(PathBuf::from("/tmp"));
        process.command = vec![OsString::from("demo"), OsString::from("--safe")];
        process
    }

    #[test]
    fn systemd_service_has_priority_over_direct_metadata() {
        let mut process = owned_process();
        process.cgroups = vec![
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/demo.service".to_owned(),
        ];

        assert_eq!(
            resolve_restart_source(&process),
            RestartSource::SystemdUserUnit {
                unit: "demo.service".to_owned()
            }
        );
    }

    #[test]
    fn direct_source_keeps_argument_boundaries() {
        let process = owned_process();

        assert_eq!(
            resolve_restart_source(&process),
            RestartSource::Direct {
                executable: PathBuf::from("/usr/bin/demo"),
                arguments: vec![OsString::from("--safe")],
                working_directory: PathBuf::from("/tmp"),
            }
        );
    }

    #[test]
    fn rejects_unreadable_and_unsafe_process_states() {
        let mut process = owned_process();
        process.executable = None;
        assert!(!resolve_restart_source(&process).is_available());

        process.executable = Some(PathBuf::from("/usr/bin/demo"));
        process.state = ProcessState::Zombie;
        assert!(!resolve_restart_source(&process).is_available());
    }
}
