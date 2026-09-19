use std::time::Duration;

use zestors::supervision::messages::Health;

use super::*;

#[derive(Default, Debug)]
pub struct ProcessMap {
    pub map: IndexMap<Name, ProcessMapEntry>,
}

impl ProcessMap {
    pub fn merge(&mut self, new_map: IndexMap<Name, (ChildConfig, ActorStatus, Vec<Name>)>) {
        // Mark outdated entries that are no longer present
        for (name, entry) in self.map.iter_mut() {
            if !new_map.contains_key(name) {
                entry.outdated_since.get_or_insert_with(Instant::now);
            } else {
                entry.outdated_since = None;
            }
        }

        // Add or update entries from the new map
        for (name, (cfg, status, children)) in new_map {
            match self.map.get_mut(&name) {
                Some(entry) => {
                    entry.update(cfg, status, children);
                }
                None => {
                    let entry = ProcessMapEntry::new(name.clone(), cfg, status, children);
                    self.map.insert(name.clone(), entry);
                }
            }
        }
    }

    pub fn add_snapshots(&mut self, snapshots: Vec<Option<ChannelSnapshot>>) {
        for snapshot in snapshots {
            if let Some(snapshot) = snapshot {
                if let Some(entry) = self.map.get_mut(&snapshot.name) {
                    entry.snapshot = Some(snapshot);
                }
            }
        }
    }

    pub fn tree(&self) -> Vec<ProcessTree<'_>> {
        let now = Instant::now();

        let is_visible = |entry: &ProcessMapEntry| {
            entry
                .outdated_since
                .is_none_or(|since| now.duration_since(since) <= Duration::from_secs(10))
        };

        let child_names: HashSet<_> = self
            .map
            .values()
            .filter(|entry| is_visible(entry))
            .flat_map(|entry| entry.children.iter().cloned())
            .collect();

        self.map
            .iter()
            .filter(|(_, entry)| is_visible(entry))
            .filter(|(name, _)| !child_names.contains(*name))
            .filter_map(|(name, _)| self.build_tree(name, now))
            .collect()
    }

    fn build_tree(&self, name: &Name, now: Instant) -> Option<ProcessTree<'_>> {
        let process = self.map.get(name)?;

        if process
            .outdated_since
            .is_some_and(|since| now.duration_since(since) > Duration::from_secs(10))
        {
            return None;
        }

        let children = process
            .children
            .iter()
            .filter_map(|child_name| self.build_tree(child_name, now))
            .collect();

        Some(ProcessTree {
            entry: process,
            children,
        })
    }
}
#[derive(Debug)]
pub struct ProcessTree<'a> {
    pub entry: &'a ProcessMapEntry,
    pub children: Vec<ProcessTree<'a>>,
}

#[derive(Clone, Debug)]
pub struct ProcessMapEntry {
    pub name: Name,
    pub cfg: ChildConfig,
    pub status: ActorStatus,
    pub children: Vec<Name>,
    pub snapshot: Option<ChannelSnapshot>,
    pub health: Option<Health>,
    pub outdated_since: Option<Instant>,
}

impl ProcessMapEntry {
    pub fn new(name: Name, cfg: ChildConfig, status: ActorStatus, children: Vec<Name>) -> Self {
        Self {
            name,
            cfg,
            status,
            children,
            snapshot: None,
            health: None,
            outdated_since: None,
        }
    }

    pub fn update(&mut self, cfg: ChildConfig, status: ActorStatus, children: Vec<Name>) {
        self.cfg = cfg;
        self.status = status;
        self.children = children;
        self.outdated_since = None;
    }
}
