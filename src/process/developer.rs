use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use super::{GuiClassification, ProcessIdentity, ProcessSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeveloperKind {
    ListeningServer,
    DevelopmentCommand,
}

impl DeveloperKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ListeningServer => "LISTENING SERVER",
            Self::DevelopmentCommand => "DEVELOPMENT COMMAND",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeveloperClassification {
    pub identity: ProcessIdentity,
    pub kind: DeveloperKind,
    pub endpoints: Vec<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ListeningSocket {
    inode: u64,
    endpoint: String,
}

#[must_use]
pub fn classify_developer_processes(
    root: &Path,
    processes: &[ProcessSnapshot],
    graphical: &[GuiClassification],
) -> Vec<DeveloperClassification> {
    let current_uid = rustix::process::getuid().as_raw();
    if current_uid == 0 {
        return Vec::new();
    }
    let pidra_pid = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    let protected_ancestors = ancestor_pids(processes, pidra_pid);
    let graphical_roots: HashSet<_> = graphical
        .iter()
        .map(|classification| classification.identity)
        .collect();
    let listening = listening_sockets(root);

    processes
        .iter()
        .filter(|process| {
            eligible_process(
                process,
                current_uid,
                pidra_pid,
                &protected_ancestors,
                &graphical_roots,
            )
        })
        .filter_map(|process| {
            let endpoints = process_listeners(root, process.identity.pid, &listening);
            let command_evidence = development_command_evidence(process);
            if endpoints.is_empty() && command_evidence.is_none() {
                return None;
            }

            let (kind, mut evidence) = if endpoints.is_empty() {
                (
                    DeveloperKind::DevelopmentCommand,
                    vec![
                        command_evidence
                            .as_ref()
                            .expect("command evidence exists")
                            .clone(),
                    ],
                )
            } else {
                (
                    DeveloperKind::ListeningServer,
                    vec![format!(
                        "owns {} TCP listening socket{}",
                        endpoints.len(),
                        if endpoints.len() == 1 { "" } else { "s" }
                    )],
                )
            };
            if let Some(command_evidence) = command_evidence {
                evidence.push(command_evidence);
            }
            evidence.push("owned by the current user".to_owned());

            Some(DeveloperClassification {
                identity: process.identity,
                kind,
                endpoints,
                evidence,
            })
        })
        .collect()
}

fn eligible_process(
    process: &ProcessSnapshot,
    current_uid: u32,
    pidra_pid: i32,
    protected_ancestors: &HashSet<i32>,
    graphical_roots: &HashSet<ProcessIdentity>,
) -> bool {
    if current_uid == 0
        || process.identity.pid <= 1
        || process.identity.pid == pidra_pid
        || process.uid != current_uid
        || process.executable.is_none()
        || protected_ancestors.contains(&process.identity.pid)
        || graphical_roots.contains(&process.identity)
    {
        return false;
    }

    let name = executable_name(process);
    !is_protected_or_desktop_process(&name)
}

fn ancestor_pids(processes: &[ProcessSnapshot], pid: i32) -> HashSet<i32> {
    let parents: HashMap<_, _> = processes
        .iter()
        .filter_map(|process| {
            process
                .parent_pid
                .map(|parent| (process.identity.pid, parent))
        })
        .collect();
    let mut ancestors = HashSet::new();
    let mut cursor = pid;
    while let Some(parent) = parents.get(&cursor).copied() {
        if parent <= 0 || !ancestors.insert(parent) {
            break;
        }
        cursor = parent;
    }
    ancestors
}

fn executable_name(process: &ProcessSnapshot) -> String {
    process
        .executable
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or(&process.name)
        .to_ascii_lowercase()
}

fn is_protected_or_desktop_process(name: &str) -> bool {
    matches!(
        name,
        "systemd"
            | "systemd-logind"
            | "systemd-udevd"
            | "systemd-journald"
            | "systemd-resolved"
            | "systemd-timesyncd"
            | "systemd-networkd"
            | "networkmanager"
            | "dbus-broker"
            | "dbus-daemon"
            | "pipewire"
            | "pipewire-pulse"
            | "wireplumber"
            | "polkitd"
            | "udisksd"
            | "upowerd"
            | "accounts-daemon"
            | "gnome-shell"
            | "kwin_wayland"
            | "hyprland"
            | "niri"
            | "sway"
            | "xorg"
            | "xwayland"
            | "gdm"
            | "sddm"
            | "lightdm"
            | "sshd"
            | "firefox"
            | "chrome"
            | "chromium"
            | "brave"
            | "spotify"
            | "discord"
            | "electron"
    )
}

fn development_command_evidence(process: &ProcessSnapshot) -> Option<String> {
    let executable = executable_name(process);
    let arguments: Vec<_> = process
        .command
        .iter()
        .map(|argument| argument.to_string_lossy().to_ascii_lowercase())
        .collect();
    let joined = arguments.join(" ");

    let explicit = match executable.as_str() {
        "python" | "python3" | "python3.12" | "python3.13" => {
            joined.contains("-m http.server")
                || joined.contains("-m uvicorn")
                || joined.contains("manage.py runserver")
                || joined.contains("flask run")
        }
        "uvicorn" | "gunicorn" | "flask" | "django-admin" => true,
        "node" | "bun" | "deno" => {
            joined.contains(" vite")
                || joined.contains(" next dev")
                || joined.contains(" nuxt dev")
                || joined.contains(" serve")
                || joined.contains(" dev")
        }
        "npm" | "pnpm" | "yarn" => {
            joined.contains(" run dev")
                || joined.contains(" run serve")
                || joined.ends_with(" dev")
                || joined.ends_with(" serve")
        }
        "rails" => joined.contains(" server") || joined.ends_with(" s"),
        "php" => joined.contains(" -s "),
        _ => false,
    };
    explicit.then(|| format!("explicit developer/server command via {executable}"))
}

fn listening_sockets(root: &Path) -> HashMap<u64, String> {
    [
        (root.join("net/tcp"), "TCP"),
        (root.join("net/tcp6"), "TCP6"),
    ]
    .into_iter()
    .filter_map(|(path, protocol)| fs::read_to_string(path).ok().map(|text| (text, protocol)))
    .flat_map(|(text, protocol)| parse_listening_sockets(&text, protocol))
    .map(|socket| (socket.inode, socket.endpoint))
    .collect()
}

fn parse_listening_sockets(text: &str, protocol: &str) -> Vec<ListeningSocket> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() <= 9 || fields[3] != "0A" {
                return None;
            }
            let port = fields[1]
                .rsplit_once(':')
                .and_then(|(_, port)| u16::from_str_radix(port, 16).ok())?;
            let inode = fields[9].parse().ok()?;
            Some(ListeningSocket {
                inode,
                endpoint: format!("{protocol} port {port}"),
            })
        })
        .collect()
}

fn process_listeners(root: &Path, pid: i32, listening: &HashMap<u64, String>) -> Vec<String> {
    if listening.is_empty() {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(root.join(pid.to_string()).join("fd")) else {
        return Vec::new();
    };
    let mut endpoints: Vec<_> = entries
        .flatten()
        .filter_map(|entry| fs::read_link(entry.path()).ok())
        .filter_map(|target| {
            let target = target.to_string_lossy();
            target
                .strip_prefix("socket:[")
                .and_then(|inode| inode.strip_suffix(']'))
                .and_then(|inode| inode.parse::<u64>().ok())
        })
        .filter_map(|inode| listening.get(&inode).cloned())
        .collect();
    endpoints.sort();
    endpoints.dedup();
    endpoints
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        fs,
        os::unix::fs::symlink,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{
        DeveloperKind, classify_developer_processes, eligible_process, parse_listening_sockets,
    };
    use crate::process::{GuiClassification, GuiConfidence, ProcessSnapshot};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TempProcRoot(PathBuf);

    impl TempProcRoot {
        fn new() -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "pidra-developer-test-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("temp proc root");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempProcRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn owned_process(name: &str, pid: i32) -> ProcessSnapshot {
        let mut process = ProcessSnapshot::fixture(name, pid, 1);
        process.uid = rustix::process::getuid().as_raw();
        process.executable = Some(PathBuf::from(format!("/usr/bin/{name}")));
        process
    }

    #[test]
    fn parses_only_tcp_listen_rows() {
        let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm retr uid timeout inode\n\
          0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 4242\n\
          1: 0100007F:01BB 0100007F:9999 01 00000000:00000000 00:00000000 00000000 1000 0 5252\n";

        let sockets = parse_listening_sockets(table, "TCP");

        assert_eq!(sockets.len(), 1);
        assert_eq!(sockets[0].inode, 4_242);
        assert_eq!(sockets[0].endpoint, "TCP port 8080");
    }

    #[test]
    fn classifies_the_process_that_owns_a_listener() {
        let root = TempProcRoot::new();
        fs::create_dir_all(root.path().join("net")).expect("net dir");
        fs::write(
            root.path().join("net/tcp"),
            "header\n0: 00000000:0BB8 00000000:0000 0A 0:0 00:0 0 1000 0 777\n",
        )
        .expect("tcp table");
        let process = owned_process("demo-server", 200);
        let fd = root.path().join("200/fd");
        fs::create_dir_all(&fd).expect("fd dir");
        symlink("socket:[777]", fd.join("4")).expect("socket symlink");

        let result = classify_developer_processes(root.path(), &[process], &[]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, DeveloperKind::ListeningServer);
        assert_eq!(result[0].endpoints, ["TCP port 3000"]);
    }

    #[test]
    fn accepts_explicit_dev_commands_but_not_name_only_guesses() {
        let root = TempProcRoot::new();
        let mut server = owned_process("python3", 201);
        server.command = ["python3", "-m", "http.server", "8000"]
            .map(OsString::from)
            .to_vec();
        let idle_node = owned_process("node", 202);

        let result = classify_developer_processes(root.path(), &[server, idle_node], &[]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, DeveloperKind::DevelopmentCommand);
    }

    #[test]
    fn excludes_gui_roots_other_users_and_session_infrastructure() {
        let root = TempProcRoot::new();
        let gui = owned_process("node", 203);
        let mut other_user = owned_process("uvicorn", 204);
        other_user.uid = other_user.uid.saturating_add(1);
        let compositor = owned_process("Hyprland", 205);
        let classifications = vec![GuiClassification {
            identity: gui.identity,
            confidence: GuiConfidence::Confirmed,
            display_name: None,
            application_scope: None,
            evidence: vec!["window".to_owned()],
        }];

        let result = classify_developer_processes(
            root.path(),
            &[gui, other_user, compositor],
            &classifications,
        );

        assert!(result.is_empty());
    }

    #[test]
    fn root_mode_never_exposes_developer_targets() {
        let process = owned_process("demo-server", 206);

        assert!(!eligible_process(
            &process,
            0,
            999,
            &Default::default(),
            &Default::default()
        ));
    }
}
