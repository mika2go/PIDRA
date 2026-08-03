use std::collections::HashMap;

use super::{ProcessIdentity, ProcessSnapshot, tree::ProcessTree};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ApplicationResources {
    pub process_count: usize,
    pub cpu_percent: f32,
    pub rss_bytes: u64,
    pub pss_bytes: u64,
    pub pss_process_count: usize,
    pub read_rate_bytes: f64,
    pub write_rate_bytes: f64,
}

impl ApplicationResources {
    #[must_use]
    pub fn has_complete_pss(self) -> bool {
        self.process_count > 0 && self.pss_process_count == self.process_count
    }

    #[must_use]
    pub fn preferred_memory_bytes(self) -> u64 {
        if self.has_complete_pss() {
            self.pss_bytes
        } else {
            self.rss_bytes
        }
    }

    #[must_use]
    pub fn memory_kind(self) -> &'static str {
        if self.has_complete_pss() {
            "PSS"
        } else {
            "RSS"
        }
    }
}

#[must_use]
pub fn aggregate_application_resources(
    processes: &[ProcessSnapshot],
    roots: impl IntoIterator<Item = ProcessIdentity>,
) -> HashMap<ProcessIdentity, ApplicationResources> {
    let tree = ProcessTree::new(processes);
    roots
        .into_iter()
        .filter_map(|root| {
            let root_process = tree.process(root)?;
            let members = std::iter::once(root)
                .chain(tree.descendants(root))
                .filter_map(|identity| tree.process(identity));
            let aggregate = members.fold(ApplicationResources::default(), |mut total, process| {
                total.process_count = total.process_count.saturating_add(1);
                total.cpu_percent += process.cpu_percent;
                total.rss_bytes = total.rss_bytes.saturating_add(process.rss_bytes);
                if let Some(pss_bytes) = process.pss_bytes {
                    total.pss_bytes = total.pss_bytes.saturating_add(pss_bytes);
                    total.pss_process_count = total.pss_process_count.saturating_add(1);
                }
                total.read_rate_bytes += process.read_rate_bytes.unwrap_or(0.0);
                total.write_rate_bytes += process.write_rate_bytes.unwrap_or(0.0);
                total
            });
            let _ = root_process;
            Some((root, aggregate))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::aggregate_application_resources;
    use crate::process::ProcessSnapshot;

    #[test]
    fn totals_each_descendant_once_and_ignores_unrelated_processes() {
        let root = ProcessSnapshot::fixture("root", 10, 100);
        let mut child = ProcessSnapshot::fixture("child", 11, 40);
        child.parent_pid = Some(10);
        child.cpu_percent = 2.5;
        child.pss_bytes = Some(25);
        let mut unrelated = ProcessSnapshot::fixture("other", 12, 1_000);
        unrelated.pss_bytes = Some(900);

        let totals =
            aggregate_application_resources(&[root.clone(), child, unrelated], [root.identity]);
        let total = totals[&root.identity];

        assert_eq!(total.process_count, 2);
        assert_eq!(total.rss_bytes, 140);
        assert_eq!(total.pss_bytes, 25);
        assert_eq!(total.pss_process_count, 1);
        assert!(!total.has_complete_pss());
        assert_eq!(total.preferred_memory_bytes(), 140);
    }

    #[test]
    fn complete_pss_becomes_the_preferred_memory_measure() {
        let mut root = ProcessSnapshot::fixture("root", 20, 100);
        root.pss_bytes = Some(70);
        let totals = aggregate_application_resources(&[root.clone()], [root.identity]);
        let total = totals[&root.identity];

        assert!(total.has_complete_pss());
        assert_eq!(total.preferred_memory_bytes(), 70);
        assert_eq!(total.memory_kind(), "PSS");
    }

    #[test]
    fn malformed_parent_cycles_do_not_count_the_root_twice() {
        let mut root = ProcessSnapshot::fixture("root", 30, 100);
        root.parent_pid = Some(31);
        let mut child = ProcessSnapshot::fixture("child", 31, 50);
        child.parent_pid = Some(30);

        let totals = aggregate_application_resources(&[root.clone(), child], [root.identity]);
        let total = totals[&root.identity];
        assert_eq!(total.process_count, 2);
        assert_eq!(total.rss_bytes, 150);
    }
}
