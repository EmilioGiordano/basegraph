//! The memory record and its parts.
//!
//! A [`Memory`] carries its own [`AnchorKey`] — an identity snapshot taken when
//! the memory was written — so it can later be classified against the current
//! index without persisting any old index.

use serde::{Deserialize, Serialize};

/// Stable identifier of a memory record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(pub String);

/// The identity a memory is anchored to: a symbol's fully-qualified name and the
/// hash of its normalized signature at the time the memory was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorKey {
    pub fqn: String,
    pub sig_hash: String,
    /// Name-free hash of the signature, enabling rename detection. Defaulted so
    /// memories written before this field still load (they miss rename
    /// detection, and an empty value never matches — see the classifier).
    #[serde(default)]
    pub shape_hash: String,
    /// File the symbol lived in when the memory was written, so an anchor that
    /// becomes unreachable by name is still reachable by file. Same precedent
    /// as `shape_hash`: defaulted for older logs, and an empty value never
    /// matches a query.
    #[serde(default)]
    pub file: String,
}

/// What kind of knowledge a memory captures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Decision,
    Gotcha,
    Invariant,
    BugHistory,
}

impl Kind {
    /// Invariants are the only kind that can become an executable test.
    pub fn is_invariant(self) -> bool {
        matches!(self, Kind::Invariant)
    }
}

/// What a memory is about: a file path or a symbol's fully-qualified name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    File(String),
    Symbol(String),
}

/// How the anchor stands relative to the current index. Computed live by the
/// classifier on read; never stored on a [`Memory`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Intact,
    Evolved,
    Orphaned,
}

/// Free-form origin metadata, always supplied by the caller (never derived by
/// shelling out to git).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub commit: Option<String>,
    pub session: Option<String>,
}

/// A single unit of anchored knowledge about the codebase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub content: String,
    pub anchor: AnchorKey,
    pub scope: Scope,
    pub kind: Kind,
    pub provenance: Provenance,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Memory {
        Memory {
            id: MemoryId("mem-1".into()),
            content: "callers assume this is sorted".into(),
            anchor: AnchorKey {
                fqn: "graph::Graph::neighbors".into(),
                sig_hash: "b4d44c6b030939fa".into(),
                shape_hash: "a1b2c3d4e5f60718".into(),
                file: "src/graph.rs".into(),
            },
            scope: Scope::Symbol("graph::Graph::neighbors".into()),
            kind: Kind::Invariant,
            provenance: Provenance {
                commit: Some("deadbeef".into()),
                session: Some("s-42".into()),
            },
        }
    }

    #[test]
    fn only_invariants_are_invariants() {
        assert!(Kind::Invariant.is_invariant());
        for kind in [Kind::Decision, Kind::Gotcha, Kind::BugHistory] {
            assert!(!kind.is_invariant(), "{kind:?}");
        }
    }

    #[test]
    fn memory_roundtrip() {
        let mem = sample();
        let json = serde_json::to_string(&mem).expect("serialize");
        let back: Memory = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(mem, back);
    }

    #[test]
    fn anchors_written_before_the_added_fields_still_load() {
        let json = r#"{"fqn":"old::sym","sig_hash":"b4d44c6b030939fa"}"#;
        let anchor: AnchorKey = serde_json::from_str(json).expect("deserialize");
        assert_eq!(anchor.fqn, "old::sym");
        assert!(anchor.shape_hash.is_empty());
        assert!(anchor.file.is_empty());
    }

    #[test]
    fn file_scope_and_variants_roundtrip() {
        let mem = Memory {
            scope: Scope::File("src/builder.rs".into()),
            kind: Kind::BugHistory,
            provenance: Provenance::default(),
            ..sample()
        };
        let json = serde_json::to_string(&mem).expect("serialize");
        let back: Memory = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(mem, back);
        assert_eq!(back.scope, Scope::File("src/builder.rs".into()));
        assert_eq!(back.provenance, Provenance::default());
    }
}
