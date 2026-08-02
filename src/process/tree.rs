use std::collections::{HashMap, HashSet};

use super::{ProcessIdentity, ProcessSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeNode {
    pub identity: ProcessIdentity,
    pub depth: usize,
    pub has_children: bool,
    pub expanded: bool,
}

#[derive(Debug)]
pub struct ProcessTree<'a> {
    processes: HashMap<ProcessIdentity, &'a ProcessSnapshot>,
    children: HashMap<i32, Vec<ProcessIdentity>>,
}

impl<'a> ProcessTree<'a> {
    #[must_use]
    pub fn new(processes: &'a [ProcessSnapshot]) -> Self {
        let process_map: HashMap<_, _> = processes
            .iter()
            .map(|process| (process.identity, process))
            .collect();
        let identity_by_pid: HashMap<_, _> = processes
            .iter()
            .map(|process| (process.identity.pid, process.identity))
            .collect();
        let mut children: HashMap<i32, Vec<ProcessIdentity>> = HashMap::new();
        for process in processes {
            if let Some(parent_pid) = process.parent_pid
                && identity_by_pid.contains_key(&parent_pid)
            {
                children
                    .entry(parent_pid)
                    .or_default()
                    .push(process.identity);
            }
        }
        for identities in children.values_mut() {
            identities.sort_by(|left, right| {
                let left_process = process_map[left];
                let right_process = process_map[right];
                left_process
                    .name
                    .cmp(&right_process.name)
                    .then_with(|| left.pid.cmp(&right.pid))
            });
        }
        Self {
            processes: process_map,
            children,
        }
    }

    #[must_use]
    pub fn process(&self, identity: ProcessIdentity) -> Option<&'a ProcessSnapshot> {
        self.processes.get(&identity).copied()
    }

    #[must_use]
    pub fn direct_children(&self, identity: ProcessIdentity) -> &[ProcessIdentity] {
        self.children.get(&identity.pid).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn descendants(&self, identity: ProcessIdentity) -> Vec<ProcessIdentity> {
        let mut descendants = Vec::new();
        let mut stack = self.direct_children(identity).to_vec();
        let mut visited = HashSet::new();
        while let Some(child) = stack.pop() {
            if !visited.insert(child) {
                continue;
            }
            descendants.push(child);
            stack.extend_from_slice(self.direct_children(child));
        }
        descendants
    }

    #[must_use]
    pub fn visible_nodes(
        &self,
        root: ProcessIdentity,
        expanded: &HashSet<ProcessIdentity>,
    ) -> Vec<TreeNode> {
        let mut nodes = Vec::new();
        let mut visited = HashSet::new();
        self.push_visible(root, 0, expanded, &mut visited, &mut nodes);
        nodes
    }

    fn push_visible(
        &self,
        identity: ProcessIdentity,
        depth: usize,
        expanded: &HashSet<ProcessIdentity>,
        visited: &mut HashSet<ProcessIdentity>,
        nodes: &mut Vec<TreeNode>,
    ) {
        if !self.processes.contains_key(&identity) || !visited.insert(identity) {
            return;
        }
        let children = self.direct_children(identity);
        let is_expanded = expanded.contains(&identity);
        nodes.push(TreeNode {
            identity,
            depth,
            has_children: !children.is_empty(),
            expanded: is_expanded,
        });
        if is_expanded {
            for child in children {
                self.push_visible(*child, depth + 1, expanded, visited, nodes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::ProcessTree;
    use crate::process::ProcessSnapshot;

    #[test]
    fn expands_only_requested_subtrees() {
        let root = ProcessSnapshot::fixture("root", 10, 1);
        let mut child = ProcessSnapshot::fixture("child", 11, 1);
        child.parent_pid = Some(10);
        let mut grandchild = ProcessSnapshot::fixture("grandchild", 12, 1);
        grandchild.parent_pid = Some(11);
        let processes = [root.clone(), child.clone(), grandchild.clone()];
        let tree = ProcessTree::new(&processes);
        let expanded = HashSet::from([root.identity]);

        let visible = tree.visible_nodes(root.identity, &expanded);

        assert_eq!(visible.len(), 2);
        assert_eq!(tree.descendants(root.identity).len(), 2);
        assert!(visible[1].has_children);
        assert!(!visible[1].expanded);
    }
}
