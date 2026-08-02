use std::{
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use pidra::{
    control::{
        ControlOutcome, SignalAction,
        signal::{read_identity, send_signal},
    },
    process::{ProcessIdentity, ProcessState, procfs::parse_stat},
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
