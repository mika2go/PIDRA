use std::{
    ffi::OsString,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use pidra::{
    control::{
        SignalAction,
        restart::{RestartOutcome, RestartRequest, RestartSource, execute_restart},
        signal::{read_identity, send_signal},
    },
    process::ProcessIdentity,
};

static UNIT_COUNTER: AtomicU64 = AtomicU64::new(1);

struct TestChild(Child);

impl TestChild {
    fn spawn() -> Self {
        Self(
            Command::new("/usr/bin/sleep")
                .arg("30")
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn controlled restart child"),
        )
    }

    fn identity(&self) -> ProcessIdentity {
        read_identity(
            Path::new("/proc"),
            i32::try_from(self.0.id()).expect("child PID fits i32"),
        )
        .expect("read controlled child identity")
    }
}

impl Drop for TestChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct TestUnit {
    name: String,
}

impl TestUnit {
    fn start() -> Option<Self> {
        let suffix = UNIT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!("pidra-test-{}-{suffix}.service", std::process::id());
        let status = Command::new("/usr/bin/systemd-run")
            .args([
                "--user",
                "--quiet",
                "--collect",
                "--unit",
                name.as_str(),
                "--property=Type=simple",
                "/usr/bin/sleep",
                "30",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()?;
        status.success().then_some(Self { name })
    }

    fn main_pid(&self) -> Option<i32> {
        let output = Command::new("/usr/bin/systemctl")
            .args([
                "--user",
                "show",
                "--property=MainPID",
                "--value",
                self.name.as_str(),
            ])
            .output()
            .ok()?;
        output.status.success().then_some(())?;
        std::str::from_utf8(&output.stdout)
            .ok()?
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|pid| *pid > 0)
    }
}

impl Drop for TestUnit {
    fn drop(&mut self) {
        let _ = Command::new("/usr/bin/systemctl")
            .args(["--user", "stop", self.name.as_str()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn direct_request(identity: ProcessIdentity) -> RestartRequest {
    RestartRequest {
        identity,
        process_name: "controlled sleep".to_owned(),
        source: RestartSource::Direct {
            executable: PathBuf::from("/usr/bin/sleep"),
            arguments: vec![OsString::from("30")],
            working_directory: PathBuf::from("/tmp"),
            application_scope: None,
        },
    }
}

fn cleanup_restarted(identity: ProcessIdentity) {
    let _ = send_signal(identity, SignalAction::ForceStop);
}

#[test]
fn direct_restart_replaces_a_controlled_child_without_a_shell() {
    let mut child = TestChild::spawn();
    let old_identity = child.identity();

    let outcome = execute_restart(&direct_request(old_identity));

    let RestartOutcome::Restarted {
        new_identity,
        method,
    } = outcome
    else {
        panic!("expected restarted outcome, got {outcome:?}");
    };
    assert_ne!(new_identity, old_identity);
    assert!(method.contains("no shell"));
    cleanup_restarted(new_identity);
    let _ = child.0.wait();
}

#[test]
fn direct_restart_aborts_when_sigterm_cannot_finish_the_old_process() {
    let child = TestChild::spawn();
    let identity = child.identity();
    assert!(matches!(
        send_signal(identity, SignalAction::Freeze),
        pidra::control::ControlOutcome::Sent(_)
    ));

    assert_eq!(
        execute_restart(&direct_request(identity)),
        RestartOutcome::StillRunning
    );
}

#[test]
fn systemd_user_service_restarts_through_dbus_with_a_new_pid() {
    let Some(unit) = TestUnit::start() else {
        eprintln!("skipped: no running systemd user manager");
        return;
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    let old_pid = loop {
        if let Some(pid) = unit.main_pid() {
            break pid;
        }
        assert!(
            Instant::now() < deadline,
            "test unit did not expose MainPID"
        );
        thread::sleep(Duration::from_millis(20));
    };
    let old_identity = read_identity(Path::new("/proc"), old_pid).expect("test unit identity");
    let request = RestartRequest {
        identity: old_identity,
        process_name: unit.name.clone(),
        source: RestartSource::SystemdUserUnit {
            unit: unit.name.clone(),
        },
    };

    let outcome = execute_restart(&request);

    let RestartOutcome::Restarted {
        new_identity,
        method,
    } = outcome
    else {
        panic!("expected systemd restart, got {outcome:?}");
    };
    assert_ne!(new_identity, old_identity);
    assert!(method.contains(&unit.name));
}
