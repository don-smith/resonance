use std::collections::{BTreeMap, BTreeSet};

use super::{
    projector::MaterializedRecord,
    watcher::{RootSnapshot, SnapshotEntry},
    LocalChange,
};

#[derive(Clone, Debug)]
pub(crate) struct FilesystemIngestor {
    acknowledged: RootSnapshot,
    last_seen: RootSnapshot,
}

impl FilesystemIngestor {
    pub(crate) fn new(snapshot: RootSnapshot) -> Self {
        Self {
            acknowledged: snapshot.clone(),
            last_seen: snapshot,
        }
    }

    pub(crate) fn resume(acknowledged: RootSnapshot, observed: RootSnapshot) -> Self {
        Self {
            acknowledged,
            last_seen: observed,
        }
    }

    pub(crate) fn observe(
        &mut self,
        snapshot: RootSnapshot,
        materialized: &mut BTreeMap<String, MaterializedRecord>,
    ) -> Vec<LocalChange> {
        if snapshot != self.last_seen {
            self.last_seen = snapshot;
            return Vec::new();
        }
        if snapshot == self.acknowledged {
            return Vec::new();
        }

        let mut changes = Vec::new();
        let mut moved_from = BTreeSet::new();
        let mut moved_to = BTreeSet::new();
        let removed_files = materialized
            .iter()
            .filter(|(path, record)| {
                !record.directory
                    && !path.contains(".resonance-conflict-")
                    && !snapshot.contains_key(*path)
            })
            .map(|(path, record)| (path.clone(), record.clone()))
            .collect::<Vec<_>>();
        let added_files = snapshot
            .iter()
            .filter(|(path, entry)| {
                !materialized.contains_key(*path) && matches!(entry, SnapshotEntry::File { .. })
            })
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect::<Vec<_>>();

        for (old_path, record) in &removed_files {
            let Some(old_hash) = record.content_hash.as_deref() else {
                continue;
            };
            let matches = added_files
                .iter()
                .filter(|(new_path, entry)| {
                    !moved_to.contains(new_path)
                        && matches!(entry, SnapshotEntry::File { hash, .. } if hash == old_hash)
                })
                .collect::<Vec<_>>();
            let same_hash_removed = removed_files
                .iter()
                .filter(|(_, candidate)| candidate.content_hash.as_deref() == Some(old_hash))
                .count();
            if matches.len() == 1 && same_hash_removed == 1 {
                let new_path = matches[0].0.clone();
                moved_from.insert(old_path.clone());
                moved_to.insert(new_path.clone());
                changes.push(LocalChange::MoveNode {
                    node_id: record.node_id.clone(),
                    new_relative_path: new_path.clone(),
                });
                let mut moved = record.clone();
                moved.relative_path = new_path.clone();
                materialized.remove(old_path);
                materialized.insert(new_path, moved);
            }
        }

        let existing = materialized.clone();
        for (path, record) in existing {
            if record.directory
                || path.contains(".resonance-conflict-")
                || moved_from.contains(&path)
                || moved_to.contains(&path)
            {
                continue;
            }
            match snapshot.get(&path) {
                Some(SnapshotEntry::File { hash, bytes }) => {
                    if record.content_hash.as_deref() != Some(hash) {
                        changes.push(LocalChange::ReplaceFile {
                            node_id: record.node_id.clone(),
                            base_revision_id: record.revision_id.clone().unwrap_or_default(),
                            relative_path: path.clone(),
                            content_hash: hash.clone(),
                            bytes: bytes.to_vec(),
                        });
                        if let Some(current) = materialized.get_mut(&path) {
                            current.content_hash = Some(hash.clone());
                        }
                    }
                }
                Some(SnapshotEntry::Directory) => {}
                None => {
                    changes.push(LocalChange::TombstoneNode {
                        node_id: record.node_id.clone(),
                    });
                    materialized.remove(&path);
                }
            }
        }

        for (path, entry) in &snapshot {
            if materialized.contains_key(path)
                || moved_to.contains(path)
                || self.acknowledged.get(path) == Some(entry)
            {
                continue;
            }
            match entry {
                SnapshotEntry::Directory => changes.push(LocalChange::CreateDirectory {
                    relative_path: path.clone(),
                }),
                SnapshotEntry::File { hash, bytes } => changes.push(LocalChange::CreateFile {
                    relative_path: path.clone(),
                    content_hash: hash.clone(),
                    mime_type: mime_type(path).to_owned(),
                    bytes: bytes.to_vec(),
                }),
            }
        }

        self.acknowledged = snapshot;
        changes.sort_by_key(change_key);
        changes
    }
}

fn mime_type(path: &str) -> &'static str {
    if path.ends_with(".md") {
        "text/markdown"
    } else {
        "application/octet-stream"
    }
}

fn change_key(change: &LocalChange) -> String {
    match change {
        LocalChange::CreateDirectory { relative_path }
        | LocalChange::CreateFile { relative_path, .. }
        | LocalChange::ReplaceFile { relative_path, .. } => relative_path.clone(),
        LocalChange::MoveNode {
            new_relative_path, ..
        } => new_relative_path.clone(),
        LocalChange::TombstoneNode { node_id } => node_id.clone(),
    }
}
