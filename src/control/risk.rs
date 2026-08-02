use crate::process::{ProcessSnapshot, ProcessState, tree::ProcessTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskRating {
    LikelySafe,
    CloseFromApplicationFirst,
    Caution,
    Protected,
    Unknown,
}

impl RiskRating {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::LikelySafe => "LIKELY SAFE TO TERMINATE",
            Self::CloseFromApplicationFirst => "CLOSE FROM APPLICATION FIRST",
            Self::Caution => "CAUTION",
            Self::Protected => "PROTECTED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskConfidence {
    High,
    Medium,
    Low,
}

impl RiskConfidence {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::High => "HIGH",
            Self::Medium => "MEDIUM",
            Self::Low => "LOW",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskAssessment {
    pub rating: RiskRating,
    pub confidence: RiskConfidence,
    pub evidence: Vec<String>,
    pub warning: String,
}

#[must_use]
pub fn assess_termination(
    process: &ProcessSnapshot,
    all_processes: &[ProcessSnapshot],
    pidra_pid: i32,
) -> RiskAssessment {
    let current_uid = rustix::process::getuid().as_raw();
    let tree = ProcessTree::new(all_processes);
    let descendants = tree.descendants(process.identity);
    let executable_name = process
        .executable
        .as_deref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or(&process.name)
        .to_ascii_lowercase();

    if process.identity.pid == 1 || process.identity.pid == pidra_pid {
        return protected("PID 1 and PIDRA itself are never valid targets");
    }
    if process.uid != current_uid {
        return protected("process is not owned by the current user");
    }
    if is_essential_session_process(&executable_name) {
        return protected("process is essential to the active graphical session");
    }
    if process.state == ProcessState::Zombie {
        return RiskAssessment {
            rating: RiskRating::Unknown,
            confidence: RiskConfidence::High,
            evidence: vec!["process is already a zombie and cannot be terminated again".to_owned()],
            warning:
                "Its parent must collect the exit status; signalling the zombie does not repair it."
                    .to_owned(),
        };
    }
    if process.state == ProcessState::DiskSleep {
        return RiskAssessment {
            rating: RiskRating::Caution,
            confidence: RiskConfidence::High,
            evidence: vec!["process is blocked in uninterruptible kernel state D".to_owned()],
            warning:
                "Even SIGKILL cannot finish it until the kernel wait returns; repeated force-kill attempts do not help."
                    .to_owned(),
        };
    }

    let mut evidence = Vec::new();
    if !descendants.is_empty() {
        evidence.push(format!(
            "{} descendant process{} may outlive the selected process",
            descendants.len(),
            if descendants.len() == 1 { "" } else { "es" }
        ));
    }
    if process.write_rate_bytes.is_some_and(|rate| rate > 0.0) {
        evidence.push(format!(
            "recent write activity is {:.0} bytes/s",
            process.write_rate_bytes.unwrap_or_default()
        ));
    }
    evidence.push("process is owned by the current user".to_owned());

    let (rating, confidence) =
        if process.write_rate_bytes.is_some_and(|rate| rate > 0.0) || !descendants.is_empty() {
            (RiskRating::CloseFromApplicationFirst, RiskConfidence::High)
        } else if process.executable.is_some() {
            (
                RiskRating::CloseFromApplicationFirst,
                RiskConfidence::Medium,
            )
        } else {
            evidence.push("executable path is unavailable".to_owned());
            (RiskRating::Caution, RiskConfidence::Low)
        };

    RiskAssessment {
        rating,
        confidence,
        evidence,
        warning: "No safety guarantee. Prefer normal application shutdown; SIGKILL can damage unsaved user data, configuration, or databases."
            .to_owned(),
    }
}

fn protected(reason: &str) -> RiskAssessment {
    RiskAssessment {
        rating: RiskRating::Protected,
        confidence: RiskConfidence::High,
        evidence: vec![reason.to_owned()],
        warning: "PIDRA will not signal this target under the current safety policy.".to_owned(),
    }
}

fn is_essential_session_process(name: &str) -> bool {
    matches!(
        name,
        "systemd"
            | "hyprland"
            | "niri"
            | "sway"
            | "kwin_wayland"
            | "gnome-shell"
            | "xorg"
            | "xwayland"
            | "dbus-broker"
            | "dbus-daemon"
    )
}

#[cfg(test)]
mod tests {
    use super::{RiskRating, assess_termination};
    use crate::process::{ProcessSnapshot, ProcessState};

    #[test]
    fn protects_pidra_and_session_infrastructure() {
        let mut pidra = ProcessSnapshot::fixture("pidra", 50, 1);
        pidra.uid = rustix::process::getuid().as_raw();
        assert_eq!(
            assess_termination(&pidra, &[pidra.clone()], 50).rating,
            RiskRating::Protected
        );

        let mut compositor = ProcessSnapshot::fixture("Hyprland", 60, 1);
        compositor.uid = rustix::process::getuid().as_raw();
        assert_eq!(
            assess_termination(&compositor, &[compositor.clone()], 50).rating,
            RiskRating::Protected
        );
    }

    #[test]
    fn explains_kernel_and_write_risks() {
        let mut blocked = ProcessSnapshot::fixture("blocked", 70, 1);
        blocked.uid = rustix::process::getuid().as_raw();
        blocked.state = ProcessState::DiskSleep;
        assert_eq!(
            assess_termination(&blocked, &[blocked.clone()], 50).rating,
            RiskRating::Caution
        );

        let mut writer = ProcessSnapshot::fixture("writer", 80, 1);
        writer.uid = rustix::process::getuid().as_raw();
        writer.write_rate_bytes = Some(4_096.0);
        assert_eq!(
            assess_termination(&writer, &[writer.clone()], 50).rating,
            RiskRating::CloseFromApplicationFirst
        );
    }
}
