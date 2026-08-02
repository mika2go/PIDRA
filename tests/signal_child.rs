use std::{
    io::{BufRead, BufReader, Write},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use pidra::{
    control::{
        ControlOutcome, SignalAction,
        diagnosis::{Diagnosis, DiagnosisContext, diagnose_after_signal},
        signal::{read_identity, send_signal},
    },
    process::{
        ProcessIdentity, ProcessState,
        procfs::{parse_stat, scan_system_procfs},
    },
};

struct TestChild(Child);

impl TestChild {
    fn spawn() -> Self {
        let child = Command::new("/usr/bin/sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn controlled sleep child");
        Self(child)
    }

    fn identity(&self) -> ProcessIdentity {
        read_identity(
            Path::new("/proc"),
            i32::try_from(self.0.id()).expect("child PID fits i32"),
        )
        .expect("read controlled child identity")
    }

    fn state(&self) -> Option<ProcessState> {
        let stat = std::fs::read(format!("/proc/{}/stat", self.0.id())).ok()?;
        parse_stat(&stat).ok().map(|parsed| parsed.state)
    }
}

impl Drop for TestChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct TestProcessTree {
    parent: Child,
    child_identity: Option<ProcessIdentity>,
}

impl Drop for TestProcessTree {
    fn drop(&mut self) {
        let _ = self.parent.kill();
        let _ = self.parent.wait();
        if let Some(identity) = self.child_identity {
            let _ = send_signal(identity, SignalAction::ForceStop);
        }
    }
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    condition()
}

#[test]
fn sigterm_exits_only_the_controlled_child() {
    let mut child = TestChild::spawn();
    let identity = child.identity();

    assert!(matches!(
        send_signal(identity, SignalAction::Stop),
        ControlOutcome::Sent(_)
    ));
    assert!(wait_until(|| child
        .0
        .try_wait()
        .is_ok_and(|status| status.is_some())));
}

#[test]
fn freeze_and_resume_are_observable_on_the_controlled_child() {
    let child = TestChild::spawn();
    let identity = child.identity();

    assert!(matches!(
        send_signal(identity, SignalAction::Freeze),
        ControlOutcome::Sent(_)
    ));
    assert!(wait_until(|| child.state() == Some(ProcessState::Stopped)));

    assert!(matches!(
        send_signal(identity, SignalAction::Resume),
        ControlOutcome::Sent(_)
    ));
    assert!(wait_until(|| {
        child
            .state()
            .is_some_and(|state| state != ProcessState::Stopped)
    }));
}

#[test]
fn changed_start_time_is_rejected_without_signalling_the_child() {
    let mut child = TestChild::spawn();
    let mut wrong_identity = child.identity();
    wrong_identity.start_time_ticks = wrong_identity.start_time_ticks.saturating_add(1);

    assert_eq!(
        send_signal(wrong_identity, SignalAction::Stop),
        ControlOutcome::IdentityChanged
    );
    assert!(child.0.try_wait().expect("query child status").is_none());
}

#[test]
fn helper_process_parent() {
    if std::env::var_os("PIDRA_PARENT_HELPER").is_none() {
        return;
    }
    let mut child = Command::new("/usr/bin/sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn helper child");
    println!("PIDRA_CHILD={}", child.id());
    std::io::stdout().flush().expect("flush helper PID");
    let _ = child.wait();
}

#[test]
fn diagnosis_names_a_test_owned_child_that_survives_its_parent() {
    let executable = std::env::current_exe().expect("current integration test executable");
    let parent = Command::new(executable)
        .args(["--exact", "helper_process_parent", "--nocapture"])
        .env("PIDRA_PARENT_HELPER", "1")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn controlled parent helper");
    let mut tree = TestProcessTree {
        parent,
        child_identity: None,
    };
    let stdout = tree.parent.stdout.take().expect("helper stdout");
    let mut child_pid = None;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(value) = line.strip_prefix("PIDRA_CHILD=") {
            child_pid = value.parse::<i32>().ok();
            break;
        }
    }
    let child_pid = child_pid.expect("helper reported child PID");
    let parent_identity = read_identity(
        Path::new("/proc"),
        i32::try_from(tree.parent.id()).expect("parent PID fits i32"),
    )
    .expect("parent identity");
    let child_identity = read_identity(Path::new("/proc"), child_pid).expect("child identity");
    tree.child_identity = Some(child_identity);
    let before = scan_system_procfs().expect("scan before parent stop");
    let target = before
        .iter()
        .find(|process| process.identity == parent_identity)
        .expect("parent snapshot");
    let context = DiagnosisContext::capture(target, &before);
    assert!(context.descendants.contains(&child_identity));

    assert!(matches!(
        send_signal(parent_identity, SignalAction::Stop),
        ControlOutcome::Sent(_)
    ));
    assert!(wait_until(|| tree
        .parent
        .try_wait()
        .is_ok_and(|status| status.is_some())));
    let after = scan_system_procfs().expect("scan after parent stop");
    let diagnoses =
        diagnose_after_signal(&context, SignalAction::Stop, &after, Duration::from_secs(1))
            .expect("completed diagnosis");

    assert!(diagnoses.contains(&Diagnosis::ChildrenRemain(vec![child_identity])));
}
