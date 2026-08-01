use std::{
    collections::{HashMap, HashSet},
    process::Command,
};

use serde::Deserialize;
use x11rb::{
    connection::Connection,
    protocol::xproto::{AtomEnum, ConnectionExt as _},
};

use super::{ProcessIdentity, ProcessSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GuiConfidence {
    Unclassified,
    Probable,
    Confirmed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiClassification {
    pub identity: ProcessIdentity,
    pub confidence: GuiConfidence,
    pub display_name: Option<String>,
    pub application_scope: Option<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowHint {
    pub pid: i32,
    pub class: Option<String>,
    pub title: Option<String>,
    pub source: &'static str,
}

#[derive(Debug, Deserialize)]
struct HyprlandClient {
    mapped: bool,
    pid: i32,
    class: String,
    title: String,
}

pub fn discover_window_hints() -> Vec<WindowHint> {
    let mut hints = Vec::new();
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        hints.extend(discover_hyprland_windows());
    }
    if std::env::var_os("DISPLAY").is_some() {
        hints.extend(discover_x11_windows());
    }
    hints.sort_by(|left, right| {
        left.pid
            .cmp(&right.pid)
            .then_with(|| left.source.cmp(right.source))
    });
    hints.dedup_by(|left, right| left.pid == right.pid && left.source == right.source);
    hints
}

fn discover_hyprland_windows() -> Vec<WindowHint> {
    let Ok(output) = Command::new("hyprctl").args(["-j", "clients"]).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    serde_json::from_slice::<Vec<HyprlandClient>>(&output.stdout).map_or_else(
        |_| Vec::new(),
        |clients| {
            clients
                .into_iter()
                .filter(|client| client.mapped && client.pid > 0)
                .map(|client| WindowHint {
                    pid: client.pid,
                    class: nonempty(client.class),
                    title: nonempty(client.title),
                    source: "Hyprland mapped client",
                })
                .collect()
        },
    )
}

fn discover_x11_windows() -> Vec<WindowHint> {
    let Ok((connection, screen_index)) = x11rb::connect(None) else {
        return Vec::new();
    };
    let Some(screen) = connection.setup().roots.get(screen_index) else {
        return Vec::new();
    };
    let Ok(client_list_atom) = intern_atom(&connection, b"_NET_CLIENT_LIST") else {
        return Vec::new();
    };
    let Ok(pid_atom) = intern_atom(&connection, b"_NET_WM_PID") else {
        return Vec::new();
    };
    let Ok(client_list_cookie) = connection.get_property(
        false,
        screen.root,
        client_list_atom,
        AtomEnum::WINDOW,
        0,
        u32::MAX,
    ) else {
        return Vec::new();
    };
    let Ok(client_list) = client_list_cookie.reply() else {
        return Vec::new();
    };
    let Some(windows) = client_list.value32() else {
        return Vec::new();
    };

    windows
        .filter_map(|window| {
            let pid_reply = connection
                .get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1)
                .ok()?
                .reply()
                .ok()?;
            let pid = i32::try_from(pid_reply.value32()?.next()?).ok()?;
            let class_reply = connection
                .get_property(
                    false,
                    window,
                    AtomEnum::WM_CLASS,
                    AtomEnum::STRING,
                    0,
                    1_024,
                )
                .ok()
                .and_then(|cookie| cookie.reply().ok());
            let class = class_reply.and_then(|reply| {
                reply
                    .value
                    .split(|byte| *byte == 0)
                    .rfind(|part| !part.is_empty())
                    .map(|part| String::from_utf8_lossy(part).into_owned())
            });
            Some(WindowHint {
                pid,
                class,
                title: None,
                source: "X11 EWMH _NET_WM_PID",
            })
        })
        .collect()
}

fn intern_atom<C: Connection>(connection: &C, name: &[u8]) -> Result<u32, ()> {
    connection
        .intern_atom(false, name)
        .map_err(|_| ())?
        .reply()
        .map(|reply| reply.atom)
        .map_err(|_| ())
}

fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

pub fn classify_gui_processes(
    processes: &[ProcessSnapshot],
    window_hints: &[WindowHint],
) -> Vec<GuiClassification> {
    let current_uid = rustix::process::getuid().as_raw();
    let process_by_pid: HashMap<i32, &ProcessSnapshot> = processes
        .iter()
        .filter(|process| process.uid == current_uid)
        .map(|process| (process.identity.pid, process))
        .collect();
    let mut members_by_scope: HashMap<String, Vec<&ProcessSnapshot>> = HashMap::new();
    for process in process_by_pid.values() {
        if let Some(scope) = application_scope(process) {
            members_by_scope.entry(scope).or_default().push(*process);
        }
    }

    let mut root_by_scope = HashMap::new();
    let mut classifications = HashMap::new();
    for (scope, members) in &members_by_scope {
        let member_pids: HashSet<i32> = members.iter().map(|member| member.identity.pid).collect();
        let root = members
            .iter()
            .copied()
            .filter(|member| {
                member
                    .parent_pid
                    .is_none_or(|parent| !member_pids.contains(&parent))
            })
            .min_by_key(|member| member.identity.start_time_ticks)
            .or_else(|| {
                members
                    .iter()
                    .copied()
                    .min_by_key(|member| member.identity.start_time_ticks)
            });
        let Some(root) = root else {
            continue;
        };
        root_by_scope.insert(scope.clone(), root.identity);
        classifications.insert(
            root.identity,
            GuiClassification {
                identity: root.identity,
                confidence: GuiConfidence::Probable,
                display_name: scope_display_name(scope),
                application_scope: Some(scope.clone()),
                evidence: vec![format!("systemd user application unit {scope}")],
            },
        );
    }

    for hint in window_hints {
        let Some(window_process) = process_by_pid.get(&hint.pid).copied() else {
            continue;
        };
        let scope = application_scope(window_process);
        let root_identity = scope
            .as_ref()
            .and_then(|scope| root_by_scope.get(scope))
            .copied()
            .unwrap_or(window_process.identity);
        let classification =
            classifications
                .entry(root_identity)
                .or_insert_with(|| GuiClassification {
                    identity: root_identity,
                    confidence: GuiConfidence::Unclassified,
                    display_name: None,
                    application_scope: scope.clone(),
                    evidence: Vec::new(),
                });
        classification.confidence = GuiConfidence::Confirmed;
        if classification.display_name.is_none() {
            classification.display_name = hint.class.clone();
        }
        let mut evidence = hint.source.to_owned();
        if let Some(class) = &hint.class {
            evidence.push_str(&format!(" class={class}"));
        }
        if let Some(title) = &hint.title {
            evidence.push_str(&format!(" title={title}"));
        }
        if !classification.evidence.contains(&evidence) {
            classification.evidence.push(evidence);
        }
    }

    let mut classifications: Vec<_> = classifications
        .into_values()
        .filter(|classification| classification.confidence >= GuiConfidence::Probable)
        .collect();
    classifications.sort_by_key(|classification| classification.identity);
    classifications
}

fn application_scope(process: &ProcessSnapshot) -> Option<String> {
    process.cgroups.iter().find_map(|entry| {
        let path = entry
            .rsplit_once(':')
            .map_or(entry.as_str(), |(_, path)| path);
        let mut components = path.split('/').filter(|component| !component.is_empty());
        while let Some(component) = components.next() {
            if component == "app.slice" {
                let unit = components.next()?;
                if unit.starts_with("app-")
                    && (unit.ends_with(".scope") || unit.ends_with(".service"))
                {
                    return Some(unit.to_owned());
                }
            }
        }
        None
    })
}

fn scope_display_name(scope: &str) -> Option<String> {
    let without_prefix = scope.strip_prefix("app-")?;
    let without_suffix = without_prefix
        .strip_suffix(".scope")
        .or_else(|| without_prefix.strip_suffix(".service"))?;
    let name = without_suffix
        .rsplit_once('-')
        .map_or(without_suffix, |(candidate, suffix)| {
            if suffix.chars().all(|character| character.is_ascii_digit()) {
                candidate
            } else {
                without_suffix
            }
        });
    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{GuiConfidence, WindowHint, classify_gui_processes};
    use crate::process::ProcessSnapshot;

    fn process(name: &str, pid: i32, parent_pid: Option<i32>, cgroup: &str) -> ProcessSnapshot {
        let mut process = ProcessSnapshot::fixture(name, pid, 1_024);
        process.parent_pid = parent_pid;
        process.uid = rustix::process::getuid().as_raw();
        process.cgroups = vec![cgroup.to_owned()];
        process
    }

    #[test]
    fn collapses_application_scope_to_one_root() {
        let cgroup =
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-browser-42.scope";
        let root = process("browser", 100, Some(1), cgroup);
        let helper = process("renderer", 101, Some(100), cgroup);

        let classified = classify_gui_processes(&[root, helper], &[]);

        assert_eq!(classified.len(), 1);
        assert_eq!(classified[0].identity.pid, 100);
        assert_eq!(classified[0].confidence, GuiConfidence::Probable);
    }

    #[test]
    fn mapped_window_confirms_the_application_root() {
        let cgroup =
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-browser-42.scope";
        let root = process("browser", 100, Some(1), cgroup);
        let helper = process("renderer", 101, Some(100), cgroup);
        let hint = WindowHint {
            pid: 101,
            class: Some("Browser".to_owned()),
            title: Some("Window".to_owned()),
            source: "test compositor",
        };

        let classified = classify_gui_processes(&[root, helper], &[hint]);

        assert_eq!(classified.len(), 1);
        assert_eq!(classified[0].identity.pid, 100);
        assert_eq!(classified[0].confidence, GuiConfidence::Confirmed);
        assert!(classified[0].evidence[1].contains("class=Browser"));
    }

    #[test]
    fn excludes_unclassified_background_processes() {
        let process = process("daemon", 200, Some(1), "0::/user.slice/background.slice");

        assert!(classify_gui_processes(&[process], &[]).is_empty());
    }
}
