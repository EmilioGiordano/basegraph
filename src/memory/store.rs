//! Append-only JSONL event log for memories.
//!
//! Each line is a versioned event envelope; the current set of memories is the
//! fold of the log in order. This log's version is independent of the graph
//! cache version.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::memory::model::{AnchorKey, Memory, MemoryId, Status};

const EVENT_VERSION: u32 = 1;
const MEMORY_LOG_FILE: &str = "codegraph-memory.jsonl";

/// A single mutation to the memory log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Created { memory: Memory },
    Reanchored { id: MemoryId, anchor: AnchorKey },
    StatusChanged { id: MemoryId, status: Status },
    Superseded { id: MemoryId },
}

#[derive(Serialize, Deserialize)]
struct EventEnvelope {
    version: u32,
    event: Event,
}

/// Errors from reading or writing the memory log.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("memory log version {found}, expected {expected}; rebuild the memory log")]
    Incompatible { found: u32, expected: u32 },
}

/// Append-only store of memory events backed by a JSONL file.
pub struct MemoryStore {
    path: PathBuf,
}

impl MemoryStore {
    /// Store backed by `<dir>/codegraph-memory.jsonl`.
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            path: dir.as_ref().join(MEMORY_LOG_FILE),
        }
    }

    /// The log file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one event as a single JSONL line, creating the file if absent.
    pub fn append(&self, event: &Event) -> Result<(), StoreError> {
        let envelope = EventEnvelope {
            version: EVENT_VERSION,
            event: event.clone(),
        };
        let line = serde_json::to_string(&envelope)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        Ok(())
    }

    /// Read every event in log order. A missing log is an empty history.
    pub fn load_events(&self) -> Result<Vec<Event>, StoreError> {
        let data = match std::fs::read_to_string(&self.path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut events = Vec::new();
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let envelope: EventEnvelope = serde_json::from_str(line)?;
            if envelope.version != EVENT_VERSION {
                return Err(StoreError::Incompatible {
                    found: envelope.version,
                    expected: EVENT_VERSION,
                });
            }
            events.push(envelope.event);
        }
        Ok(events)
    }

    /// The current memories: the fold of the event log.
    pub fn materialize(&self) -> Result<Vec<Memory>, StoreError> {
        Ok(fold(self.load_events()?))
    }
}

/// Fold events into the current set of memories, preserving creation order.
/// An id index keeps this O(events); superseded slots are tombstoned and
/// compacted out at the end. Events referencing an unknown id are ignored.
fn fold(events: Vec<Event>) -> Vec<Memory> {
    let mut slots: Vec<Option<Memory>> = Vec::new();
    let mut index: HashMap<MemoryId, usize> = HashMap::new();
    for event in events {
        match event {
            Event::Created { memory } => match index.get(&memory.id) {
                Some(&i) => slots[i] = Some(memory),
                None => {
                    index.insert(memory.id.clone(), slots.len());
                    slots.push(Some(memory));
                }
            },
            Event::Reanchored { id, anchor } => {
                if let Some(&i) = index.get(&id) {
                    if let Some(m) = &mut slots[i] {
                        m.anchor = anchor;
                    }
                }
            }
            Event::StatusChanged { id, status } => {
                if let Some(&i) = index.get(&id) {
                    if let Some(m) = &mut slots[i] {
                        m.status = status;
                    }
                }
            }
            Event::Superseded { id } => {
                if let Some(i) = index.remove(&id) {
                    slots[i] = None;
                }
            }
        }
    }
    slots.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::model::{Kind, Provenance, Scope};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("cg_mem_store_{}_{n}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("create temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn memory(id: &str, fqn: &str, sig_hash: &str) -> Memory {
        Memory {
            id: MemoryId(id.into()),
            content: format!("note about {fqn}"),
            anchor: AnchorKey {
                fqn: fqn.into(),
                sig_hash: sig_hash.into(),
                shape_hash: String::new(),
            },
            scope: Scope::Symbol(fqn.into()),
            kind: Kind::Decision,
            status: Status::Intact,
            provenance: Provenance::default(),
        }
    }

    #[test]
    fn missing_log_materializes_empty() {
        let dir = TempDir::new();
        let store = MemoryStore::new(&dir.0);
        assert!(store.materialize().expect("materialize").is_empty());
    }

    #[test]
    fn appends_fold_into_current_state() {
        let dir = TempDir::new();
        let store = MemoryStore::new(&dir.0);
        store
            .append(&Event::Created {
                memory: memory("m1", "a::f", "h1"),
            })
            .expect("append m1");
        store
            .append(&Event::Created {
                memory: memory("m2", "a::g", "h2"),
            })
            .expect("append m2");

        let mems = store.materialize().expect("materialize");
        assert_eq!(mems.len(), 2);
        assert_eq!(mems[0].id, MemoryId("m1".into()));
        assert_eq!(mems[1].id, MemoryId("m2".into()));
    }

    #[test]
    fn superseded_hides_the_memory() {
        let dir = TempDir::new();
        let store = MemoryStore::new(&dir.0);
        store
            .append(&Event::Created {
                memory: memory("m1", "a::f", "h1"),
            })
            .expect("append");
        store
            .append(&Event::Superseded {
                id: MemoryId("m1".into()),
            })
            .expect("supersede");

        assert!(store.materialize().expect("materialize").is_empty());
    }

    #[test]
    fn reanchored_updates_the_anchor() {
        let dir = TempDir::new();
        let store = MemoryStore::new(&dir.0);
        store
            .append(&Event::Created {
                memory: memory("m1", "a::f", "h1"),
            })
            .expect("append");
        store
            .append(&Event::Reanchored {
                id: MemoryId("m1".into()),
                anchor: AnchorKey {
                    fqn: "b::f".into(),
                    sig_hash: "h1".into(),
                    shape_hash: String::new(),
                },
            })
            .expect("reanchor");

        let mems = store.materialize().expect("materialize");
        assert_eq!(mems.len(), 1);
        assert_eq!(mems[0].anchor.fqn, "b::f");
    }

    #[test]
    fn status_changed_updates_status() {
        let dir = TempDir::new();
        let store = MemoryStore::new(&dir.0);
        store
            .append(&Event::Created {
                memory: memory("m1", "a::f", "h1"),
            })
            .expect("append");
        store
            .append(&Event::StatusChanged {
                id: MemoryId("m1".into()),
                status: Status::Evolved,
            })
            .expect("status change");

        let mems = store.materialize().expect("materialize");
        assert_eq!(mems[0].status, Status::Evolved);
    }

    #[test]
    fn rejects_incompatible_version() {
        let dir = TempDir::new();
        let store = MemoryStore::new(&dir.0);
        std::fs::write(
            store.path(),
            "{\"version\":999,\"event\":{\"Superseded\":{\"id\":\"x\"}}}\n",
        )
        .expect("write");

        let err = store.load_events().expect_err("should reject");
        assert!(matches!(
            err,
            StoreError::Incompatible {
                found: 999,
                expected: 1
            }
        ));
    }
}
