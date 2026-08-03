use std::{path::Path, thread, time::Duration};

use serde::Serialize;
use thiserror::Error;

use crate::{
    control::{restart::resolve_restart_source, risk::assess_termination},
    process::{
        ApplicationResources, DeveloperClassification, GuiClassification, ProcessSnapshot,
        aggregate_application_resources,
        cpu::DeltaTracker,
        developer::classify_developer_processes,
        format::masked_command,
        gui::{classify_gui_processes, discover_window_hints},
        procfs::{self, ProcfsError},
        tree::ProcessTree,
    },
};

#[derive(Debug, Error)]
pub enum InspectError {
    #[error(transparent)]
    Procfs(#[from] ProcfsError),
    #[error("PID {0} does not exist or cannot be read")]
    NotFound(i32),
    #[error("cannot encode inspection report: {0}")]
    Encode(#[from] serde_json::Error),
}

#[derive(Debug, Serialize)]
pub struct InspectionReport {
    pub schema_version: u32,
    pub target: ProcessReport,
    pub application_resources: ResourceReport,
    pub process_tree: Vec<ProcessReport>,
    pub classification: Option<ClassificationReport>,
    pub restart_source: String,
    pub termination_risk: RiskReport,
}

#[derive(Debug, Serialize)]
pub struct ProcessReport {
    pub pid: i32,
    pub start_time_ticks: u64,
    pub parent_pid: Option<i32>,
    pub name: String,
    pub state: String,
    pub uid: u32,
    pub executable: Option<String>,
    pub command: String,
    pub working_directory: Option<String>,
    pub cpu_percent: f32,
    pub rss_bytes: u64,
    pub pss_bytes: Option<u64>,
    pub virtual_bytes: u64,
    pub thread_count: u32,
    pub read_rate_bytes: Option<f64>,
    pub write_rate_bytes: Option<f64>,
    pub cgroups: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ResourceReport {
    pub process_count: usize,
    pub cpu_percent: f32,
    pub rss_bytes: u64,
    pub pss_bytes: Option<u64>,
    pub pss_process_count: usize,
    pub read_rate_bytes: f64,
    pub write_rate_bytes: f64,
}

#[derive(Debug, Serialize)]
pub struct ClassificationReport {
    pub kind: String,
    pub confidence: Option<String>,
    pub display_name: Option<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RiskReport {
    pub rating: String,
    pub confidence: String,
    pub evidence: Vec<String>,
    pub warning: String,
}

pub fn inspect_system(pid: i32) -> Result<InspectionReport, InspectError> {
    inspect(Path::new("/proc"), pid, true)
}

pub fn inspect(
    root: &Path,
    pid: i32,
    detect_windows: bool,
) -> Result<InspectionReport, InspectError> {
    let mut first_sample = procfs::scan_procfs(root)?;
    let target_identity = first_sample
        .iter()
        .find(|process| process.identity.pid == pid)
        .map(|process| process.identity)
        .ok_or(InspectError::NotFound(pid))?;
    let mut delta_tracker = DeltaTracker::default();
    delta_tracker.update(root, &mut first_sample);
    thread::sleep(Duration::from_millis(100));
    let mut processes = procfs::scan_procfs(root)?;
    if !processes
        .iter()
        .any(|process| process.identity == target_identity)
    {
        return Err(InspectError::NotFound(pid));
    }
    delta_tracker.update(root, &mut processes);
    let window_hints = if detect_windows {
        discover_window_hints()
    } else {
        Vec::new()
    };
    let graphical = classify_gui_processes(&processes, &window_hints);
    let developer = classify_developer_processes(root, &processes, &graphical);
    let roots = graphical
        .iter()
        .map(|classification| classification.identity)
        .chain(
            developer
                .iter()
                .map(|classification| classification.identity),
        )
        .chain(std::iter::once(target_identity))
        .collect::<Vec<_>>();
    procfs::enrich_pss(root, &mut processes, roots.iter().copied());

    let target = processes
        .iter()
        .find(|process| process.identity == target_identity)
        .ok_or(InspectError::NotFound(pid))?;
    let tree = ProcessTree::new(&processes);
    let graphical_classification = graphical.iter().find(|classification| {
        classification.identity == target.identity
            || tree
                .descendants(classification.identity)
                .contains(&target.identity)
    });
    let developer_classification = developer.iter().find(|classification| {
        classification.identity == target.identity
            || tree
                .descendants(classification.identity)
                .contains(&target.identity)
    });
    let application_root = graphical_classification
        .map(|classification| classification.identity)
        .or_else(|| developer_classification.map(|classification| classification.identity))
        .unwrap_or(target.identity);
    let tree_reports = std::iter::once(application_root)
        .chain(tree.descendants(application_root))
        .filter_map(|identity| tree.process(identity))
        .map(ProcessReport::from)
        .collect();
    let resources = aggregate_application_resources(&processes, [application_root])
        .remove(&application_root)
        .unwrap_or_default();
    let classification = graphical_classification
        .map(ClassificationReport::from)
        .or_else(|| developer_classification.map(ClassificationReport::from));
    let risk = assess_termination(
        target,
        &processes,
        i32::try_from(std::process::id()).unwrap_or(i32::MAX),
    );

    Ok(InspectionReport {
        schema_version: 1,
        target: ProcessReport::from(target),
        application_resources: ResourceReport::from(resources),
        process_tree: tree_reports,
        classification,
        restart_source: resolve_restart_source(target).summary(),
        termination_risk: RiskReport {
            rating: risk.rating.label().to_owned(),
            confidence: risk.confidence.label().to_owned(),
            evidence: risk.evidence,
            warning: risk.warning,
        },
    })
}

impl InspectionReport {
    pub fn render(&self, json: bool) -> Result<String, serde_json::Error> {
        if json {
            return serde_json::to_string_pretty(self);
        }
        let pss = self.application_resources.pss_bytes.map_or_else(
            || "unavailable or incomplete".to_owned(),
            |bytes| format!("{bytes} bytes"),
        );
        Ok(format!(
            "{}  PID {} / {}  {}\nCOMMAND  {}\nTREE     {} processes, CPU {:.1}%, RSS {} bytes, PSS {}\nRESTART  {}\nRISK     {} ({})\nEVIDENCE {}\nWARNING  {}",
            self.target.name,
            self.target.pid,
            self.target.start_time_ticks,
            self.target.state,
            self.target.command,
            self.application_resources.process_count,
            self.application_resources.cpu_percent,
            self.application_resources.rss_bytes,
            pss,
            self.restart_source,
            self.termination_risk.rating,
            self.termination_risk.confidence,
            self.termination_risk.evidence.join("; "),
            self.termination_risk.warning,
        ))
    }
}

impl From<&ProcessSnapshot> for ProcessReport {
    fn from(process: &ProcessSnapshot) -> Self {
        Self {
            pid: process.identity.pid,
            start_time_ticks: process.identity.start_time_ticks,
            parent_pid: process.parent_pid,
            name: process.name.clone(),
            state: process.state.label().to_owned(),
            uid: process.uid,
            executable: process
                .executable
                .as_deref()
                .map(|path| path.display().to_string()),
            command: masked_command(&process.command),
            working_directory: process
                .cwd
                .as_deref()
                .map(|path| path.display().to_string()),
            cpu_percent: process.cpu_percent,
            rss_bytes: process.rss_bytes,
            pss_bytes: process.pss_bytes,
            virtual_bytes: process.virtual_bytes,
            thread_count: process.thread_count,
            read_rate_bytes: process.read_rate_bytes,
            write_rate_bytes: process.write_rate_bytes,
            cgroups: process.cgroups.clone(),
        }
    }
}

impl From<ApplicationResources> for ResourceReport {
    fn from(resources: ApplicationResources) -> Self {
        Self {
            process_count: resources.process_count,
            cpu_percent: resources.cpu_percent,
            rss_bytes: resources.rss_bytes,
            pss_bytes: resources.has_complete_pss().then_some(resources.pss_bytes),
            pss_process_count: resources.pss_process_count,
            read_rate_bytes: resources.read_rate_bytes,
            write_rate_bytes: resources.write_rate_bytes,
        }
    }
}

impl From<&GuiClassification> for ClassificationReport {
    fn from(classification: &GuiClassification) -> Self {
        Self {
            kind: "graphical".to_owned(),
            confidence: Some(format!("{:?}", classification.confidence)),
            display_name: classification.display_name.clone(),
            evidence: classification.evidence.clone(),
        }
    }
}

impl From<&DeveloperClassification> for ClassificationReport {
    fn from(classification: &DeveloperClassification) -> Self {
        Self {
            kind: classification.kind.label().to_owned(),
            confidence: None,
            display_name: None,
            evidence: classification.evidence.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::inspect;
    use std::path::Path;

    #[test]
    fn fixture_inspection_is_versioned_read_only_and_json_serializable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proc");
        let report = inspect(&root, 100, false).expect("fixture report");
        let json = report.render(true).expect("JSON");

        assert_eq!(report.schema_version, 1);
        assert_eq!(report.target.pid, 100);
        assert_eq!(report.application_resources.process_count, 2);
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"termination_risk\""));
    }
}
