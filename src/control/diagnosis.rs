use std::{collections::HashSet, time::Duration};

use crate::process::{ProcessIdentity, ProcessSnapshot, ProcessState, tree::ProcessTree};

use super::{ControlOutcome, SignalAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementEvidence {
    SameSystemdUnit,
    SameCgroupAndExecutable,
    SameExecutableParentAndRecentStart,
}

impl ReplacementEvidence {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::SameSystemdUnit => "same systemd unit",
            Self::SameCgroupAndExecutable => "same cgroup and executable",
            Self::SameExecutableParentAndRecentStart => {
                "same executable and parent with a recent start time"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnosis {
    Exited,
    Frozen,
    Resumed,
    StillRunning,
    Restarted {
        old_pid: i32,
        new_identity: ProcessIdentity,
        evidence: ReplacementEvidence,
    },
    ChildrenRemain(Vec<ProcessIdentity>),
    Uninterruptible,
    Zombie,
    PermissionDenied,
    IdentityChanged,
    NotFound,
    Unsupported(String),
    Failed(String),
}

impl Diagnosis {
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::Exited => "EXITED".to_owned(),
            Self::Frozen => "FROZEN".to_owned(),
            Self::Resumed => "RESUMED".to_owned(),
            Self::StillRunning => "STILL RUNNING — no automatic SIGKILL escalation".to_owned(),
            Self::Restarted {
                old_pid,
                new_identity,
                evidence,
            } => format!(
                "RESTARTED — old PID {old_pid}, new PID {} ({})",
                new_identity.pid,
                evidence.label()
            ),
            Self::ChildrenRemain(identities) => format!(
                "CHILDREN REMAIN — {}",
                identities
                    .iter()
                    .map(|identity| identity.pid.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Uninterruptible => {
                "UNINTERRUPTIBLE — kernel state D; even SIGKILL must wait for the kernel operation"
                    .to_owned()
            }
            Self::Zombie => {
                "ZOMBIE — already exited; its parent must collect the exit status".to_owned()
            }
            Self::PermissionDenied => {
                "PERMISSION DENIED — PIDRA does not offer privilege escalation".to_owned()
            }
            Self::IdentityChanged => {
                "IDENTITY CHANGED — the PID now identifies a different process".to_owned()
            }
            Self::NotFound => "NOT FOUND — the captured identity no longer exists".to_owned(),
            Self::Unsupported(reason) => format!("UNSUPPORTED — {reason}"),
            Self::Failed(reason) => format!("FAILED — {reason}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiagnosisContext {
    pub target: ProcessSnapshot,
    pub descendants: Vec<ProcessIdentity>,
}

impl DiagnosisContext {
    #[must_use]
    pub fn capture(target: &ProcessSnapshot, all_processes: &[ProcessSnapshot]) -> Self {
        Self {
            target: target.clone(),
            descendants: ProcessTree::new(all_processes).descendants(target.identity),
        }
    }
}

#[must_use]
pub fn diagnose_dispatch_failure(outcome: &ControlOutcome) -> Option<Diagnosis> {
    match outcome {
        ControlOutcome::Sent(_) => None,
        ControlOutcome::IdentityChanged => Some(Diagnosis::IdentityChanged),
        ControlOutcome::NotFound => Some(Diagnosis::NotFound),
        ControlOutcome::PermissionDenied => Some(Diagnosis::PermissionDenied),
        ControlOutcome::Protected(reason) | ControlOutcome::Unsupported(reason) => {
            Some(Diagnosis::Unsupported(reason.clone()))
        }
        ControlOutcome::Failed(reason) => Some(Diagnosis::Failed(reason.clone())),
    }
}

#[must_use]
pub fn diagnose_after_signal(
    context: &DiagnosisContext,
    action: SignalAction,
    after: &[ProcessSnapshot],
    elapsed: Duration,
) -> Option<Vec<Diagnosis>> {
    let current = after
        .iter()
        .find(|process| process.identity == context.target.identity);
    if let Some(process) = current {
        if process.state == ProcessState::DiskSleep {
            return Some(vec![Diagnosis::Uninterruptible]);
        }
        if process.state == ProcessState::Zombie {
            return Some(vec![Diagnosis::Zombie]);
        }
    }

    match action {
        SignalAction::Freeze => match current {
            Some(process) if process.state == ProcessState::Stopped => {
                Some(vec![Diagnosis::Frozen])
            }
            None => Some(vec![Diagnosis::NotFound]),
            Some(_) if elapsed >= Duration::from_secs(2) => Some(vec![Diagnosis::StillRunning]),
            Some(_) => None,
        },
        SignalAction::Resume => match current {
            Some(process) if process.state != ProcessState::Stopped => {
                Some(vec![Diagnosis::Resumed])
            }
            None => Some(vec![Diagnosis::NotFound]),
            Some(_) if elapsed >= Duration::from_secs(2) => Some(vec![Diagnosis::StillRunning]),
            Some(_) => None,
        },
        SignalAction::Stop | SignalAction::ForceStop => {
            if current.is_some() {
                return (elapsed >= Duration::from_secs(2)).then(|| vec![Diagnosis::StillRunning]);
            }
            let replacement = find_replacement(context, after);
            let surviving: Vec<_> = context
                .descendants
                .iter()
                .copied()
                .filter(|identity| after.iter().any(|process| process.identity == *identity))
                .collect();
            if replacement.is_none() && surviving.is_empty() && elapsed < Duration::from_secs(2) {
                return None;
            }
            let mut diagnoses = Vec::new();
            if let Some((replacement, evidence)) = replacement {
                diagnoses.push(Diagnosis::Restarted {
                    old_pid: context.target.identity.pid,
                    new_identity: replacement,
                    evidence,
                });
            } else {
                diagnoses.push(Diagnosis::Exited);
            }
            if !surviving.is_empty() {
                diagnoses.push(Diagnosis::ChildrenRemain(surviving));
            }
            Some(diagnoses)
        }
    }
}

fn find_replacement(
    context: &DiagnosisContext,
    after: &[ProcessSnapshot],
) -> Option<(ProcessIdentity, ReplacementEvidence)> {
    let captured_descendants: HashSet<_> = context.descendants.iter().copied().collect();
    let candidates: Vec<_> = after
        .iter()
        .filter(|candidate| candidate.identity != context.target.identity)
        .filter(|candidate| !captured_descendants.contains(&candidate.identity))
        .filter(|candidate| candidate.uid == context.target.uid)
        .filter(|candidate| {
            candidate.identity.start_time_ticks > context.target.identity.start_time_ticks
        })
        .collect();
    let old_unit = systemd_unit(&context.target.cgroups);
    if let Some(old_unit) = old_unit
        && let Some(candidate) = candidates
            .iter()
            .find(|candidate| systemd_unit(&candidate.cgroups) == Some(old_unit))
    {
        return Some((candidate.identity, ReplacementEvidence::SameSystemdUnit));
    }
    if let Some(candidate) = candidates.iter().find(|candidate| {
        context.target.executable.is_some()
            && candidate.executable == context.target.executable
            && candidate
                .cgroups
                .iter()
                .any(|group| context.target.cgroups.contains(group))
    }) {
        return Some((
            candidate.identity,
            ReplacementEvidence::SameCgroupAndExecutable,
        ));
    }
    let recent_ticks = rustix::param::clock_ticks_per_second().saturating_mul(60);
    candidates
        .iter()
        .find(|candidate| {
            context.target.executable.is_some()
                && candidate.executable == context.target.executable
                && candidate.parent_pid == context.target.parent_pid
                && candidate
                    .identity
                    .start_time_ticks
                    .saturating_sub(context.target.identity.start_time_ticks)
                    <= recent_ticks
        })
        .map(|candidate| {
            (
                candidate.identity,
                ReplacementEvidence::SameExecutableParentAndRecentStart,
            )
        })
}

fn systemd_unit(cgroups: &[String]) -> Option<&str> {
    cgroups.iter().find_map(|entry| {
        let path = entry
            .split_once("::")
            .map_or(entry.as_str(), |(_, path)| path);
        path.rsplit('/')
            .find(|component| component.ends_with(".service") || component.ends_with(".scope"))
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Diagnosis, DiagnosisContext, ReplacementEvidence, diagnose_after_signal};
    use crate::{
        control::SignalAction,
        process::{ProcessSnapshot, ProcessState},
    };

    fn process(name: &str, pid: i32, start: u64) -> ProcessSnapshot {
        let mut process = ProcessSnapshot::fixture(name, pid, 1);
        process.identity.start_time_ticks = start;
        process.uid = rustix::process::getuid().as_raw();
        process
    }

    #[test]
    fn detects_supervisor_restart_from_unit_not_name() {
        let mut old = process("old-name", 100, 1_000);
        old.cgroups = vec!["0::/app.slice/demo.service".to_owned()];
        let context = DiagnosisContext::capture(&old, std::slice::from_ref(&old));
        let mut replacement = process("different-name", 200, 1_100);
        replacement.cgroups = old.cgroups.clone();

        let diagnoses = diagnose_after_signal(
            &context,
            SignalAction::Stop,
            std::slice::from_ref(&replacement),
            std::time::Duration::from_secs(1),
        )
        .expect("complete diagnosis");

        assert!(matches!(
            diagnoses.as_slice(),
            [Diagnosis::Restarted {
                new_identity,
                evidence: ReplacementEvidence::SameSystemdUnit,
                ..
            }] if *new_identity == replacement.identity
        ));
    }

    #[test]
    fn never_matches_a_replacement_from_name_alone() {
        let old = process("same-name", 100, 1_000);
        let context = DiagnosisContext::capture(&old, std::slice::from_ref(&old));
        let replacement = process("same-name", 200, 1_100);

        assert_eq!(
            diagnose_after_signal(
                &context,
                SignalAction::Stop,
                &[replacement],
                std::time::Duration::from_secs(2)
            ),
            Some(vec![Diagnosis::Exited])
        );
    }

    #[test]
    fn waits_for_a_late_supervisor_replacement_before_declaring_exit() {
        let mut old = process("old", 100, 1_000);
        old.cgroups = vec!["0::/app.slice/demo.service".to_owned()];
        let context = DiagnosisContext::capture(&old, std::slice::from_ref(&old));

        assert_eq!(
            diagnose_after_signal(
                &context,
                SignalAction::Stop,
                &[],
                std::time::Duration::from_secs(1)
            ),
            None
        );

        let mut replacement = process("new", 200, 1_100);
        replacement.cgroups = old.cgroups.clone();
        assert!(matches!(
            diagnose_after_signal(
                &context,
                SignalAction::Stop,
                &[replacement],
                std::time::Duration::from_secs(2)
            ),
            Some(diagnoses) if matches!(diagnoses[0], Diagnosis::Restarted { .. })
        ));
    }

    #[test]
    fn reports_surviving_captured_children() {
        let old = process("parent", 100, 1_000);
        let mut child = process("child", 101, 1_001);
        child.parent_pid = Some(100);
        let context = DiagnosisContext::capture(&old, &[old.clone(), child.clone()]);

        let diagnoses = diagnose_after_signal(
            &context,
            SignalAction::Stop,
            &[child.clone()],
            std::time::Duration::from_secs(1),
        )
        .expect("complete diagnosis");

        assert!(diagnoses.contains(&Diagnosis::ChildrenRemain(vec![child.identity])));
    }

    #[test]
    fn explains_d_state_without_promising_immediate_kill() {
        let mut blocked = process("blocked", 100, 1_000);
        blocked.state = ProcessState::DiskSleep;
        blocked.executable = Some(PathBuf::from("/usr/bin/blocked"));
        let context = DiagnosisContext::capture(&blocked, std::slice::from_ref(&blocked));

        let diagnosis = diagnose_after_signal(
            &context,
            SignalAction::ForceStop,
            &[blocked],
            std::time::Duration::ZERO,
        )
        .expect("complete diagnosis");

        assert!(diagnosis[0].summary().contains("even SIGKILL must wait"));
    }
}
