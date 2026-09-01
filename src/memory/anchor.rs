//! Classify a memory's stored anchor against the CURRENT index.
//!
//! Comparison is always against the live graph, never a persisted old index.
//! The load-bearing invariant: an uncertain re-anchor is never reported as
//! `Intact`. Only an exact fqn + signature-hash match is confident.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::graph::Graph;
use crate::memory::model::{AnchorKey, Status};
use crate::memory::paths;
use crate::model::Node;
use crate::parser::sig;

/// Snapshot a node's identity as an anchor. `root` is the indexed directory:
/// the node's file is recorded relative to it, never as a machine path.
pub fn anchor_of(node: &Node, root: &Path) -> AnchorKey {
    AnchorKey {
        fqn: node.fqn.clone(),
        sig_hash: node.sig_hash.clone(),
        shape_hash: sig::shape_hash(&node.name, &node.signature),
        file: paths::relative(&node.file, root),
    }
}

/// Why a re-anchor confirmation was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfirmError {
    #[error("anchor `{0}` is intact; nothing to re-anchor")]
    Intact(String),
    #[error(
        "anchor `{0}` is orphaned with no candidates; supersede the memory or record a new one"
    )]
    NoCandidates(String),
    #[error("`{chosen}` is not a proposed candidate for `{fqn}` (proposed: {})", .candidates.join(", "))]
    NotProposed {
        fqn: String,
        chosen: String,
        candidates: Vec<String>,
    },
    #[error("candidate `{0}` is no longer in the index")]
    Missing(String),
}

/// Confirm a re-anchor: `chosen_fqn` must be one of the candidates `classify`
/// proposes for `anchor`, or the anchor's own fqn when its interface merely
/// evolved. Returns the fresh snapshot to store. This is the only way an
/// uncertain proposal becomes a served anchor; it never happens implicitly.
pub fn confirm(
    anchor: &AnchorKey,
    chosen_fqn: &str,
    graph: &Graph,
    root: &Path,
) -> Result<AnchorKey, ConfirmError> {
    let proposed = match classify(anchor, graph) {
        Classification::Intact => return Err(ConfirmError::Intact(anchor.fqn.clone())),
        Classification::Orphaned => return Err(ConfirmError::NoCandidates(anchor.fqn.clone())),
        Classification::Evolved => vec![anchor.fqn.clone()],
        Classification::ReanchorCandidate { candidates, .. } => candidates,
    };
    if !proposed.iter().any(|c| c == chosen_fqn) {
        return Err(ConfirmError::NotProposed {
            fqn: anchor.fqn.clone(),
            chosen: chosen_fqn.to_string(),
            candidates: proposed,
        });
    }
    graph
        .nodes()
        .iter()
        .find(|n| n.fqn == chosen_fqn)
        .map(|n| anchor_of(n, root))
        .ok_or_else(|| ConfirmError::Missing(chosen_fqn.to_string()))
}

/// How a re-anchor candidate was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ReanchorBasis {
    /// A node elsewhere carries the anchor's exact signature hash.
    SigHash,
    /// A node carries the anchor's name-free signature hash (likely a rename).
    ShapeHash,
    /// A node's fqn is textually similar to the anchor's.
    TokenSimilarity,
}

/// The outcome of classifying an anchor against the current index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Classification {
    /// fqn present and its signature hash matches the anchor.
    Intact,
    /// fqn present but the signature hash differs (interface changed).
    Evolved,
    /// fqn absent, but plausible relocation targets exist. Always uncertain:
    /// these are proposals for review, never a confirmed re-anchor.
    ReanchorCandidate {
        candidates: Vec<String>,
        basis: ReanchorBasis,
    },
    /// fqn absent and nothing plausible was found.
    Orphaned,
}

impl Classification {
    /// Coarse status. A re-anchor proposal collapses to `Orphaned`, never
    /// `Intact` — the point of the whole layer.
    pub fn status(&self) -> Status {
        match self {
            Classification::Intact => Status::Intact,
            Classification::Evolved => Status::Evolved,
            Classification::ReanchorCandidate { .. } | Classification::Orphaned => Status::Orphaned,
        }
    }

    /// True when the result is a proposal needing human/agent confirmation.
    pub fn is_uncertain(&self) -> bool {
        matches!(self, Classification::ReanchorCandidate { .. })
    }
}

const SIMILARITY_THRESHOLD: f64 = 0.5;

/// Classify `anchor` against the live `graph`.
pub fn classify(anchor: &AnchorKey, graph: &Graph) -> Classification {
    if let Some(node) = graph.nodes().iter().find(|n| n.fqn == anchor.fqn) {
        return if node.sig_hash == anchor.sig_hash {
            Classification::Intact
        } else {
            Classification::Evolved
        };
    }

    // fqn is gone. Look for a moved/renamed symbol by identical signature hash.
    if !anchor.sig_hash.is_empty() {
        let by_hash: Vec<String> = graph
            .nodes()
            .iter()
            .filter(|n| n.sig_hash == anchor.sig_hash)
            .map(|n| n.fqn.clone())
            .collect();
        if !by_hash.is_empty() {
            return Classification::ReanchorCandidate {
                candidates: by_hash,
                basis: ReanchorBasis::SigHash,
            };
        }
    }

    // Still nothing: look for a pure rename by the name-free shape hash. Never
    // match an empty shape hash (memories written before this field default to
    // empty and must not match each other or shape-less nodes).
    if !anchor.shape_hash.is_empty() {
        let by_shape: Vec<String> = graph
            .nodes()
            .iter()
            .filter(|n| sig::shape_hash(&n.name, &n.signature) == anchor.shape_hash)
            .map(|n| n.fqn.clone())
            .collect();
        if !by_shape.is_empty() {
            return Classification::ReanchorCandidate {
                candidates: by_shape,
                basis: ReanchorBasis::ShapeHash,
            };
        }
    }

    // No hash match. Fall back to fqn token similarity: the anchor stores a
    // hash, not the signature text, so the fqn is the only textual datum left.
    let mut by_tokens: Vec<String> = graph
        .nodes()
        .iter()
        .filter(|n| similarity(&anchor.fqn, &n.fqn) >= SIMILARITY_THRESHOLD)
        .map(|n| n.fqn.clone())
        .collect();
    by_tokens.sort();
    by_tokens.dedup();
    if !by_tokens.is_empty() {
        return Classification::ReanchorCandidate {
            candidates: by_tokens,
            basis: ReanchorBasis::TokenSimilarity,
        };
    }

    Classification::Orphaned
}

/// Jaccard similarity of the identifier tokens in two fully-qualified names.
fn similarity(a: &str, b: &str) -> f64 {
    let ta = tokens(a);
    let tb = tokens(b);
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count();
    let union = ta.union(&tb).count();
    inter as f64 / union as f64
}

fn tokens(fqn: &str) -> BTreeSet<String> {
    fqn.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::build_graph;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("cg_mem_anchor_{}_{n}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("create temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn build(files: &[(&str, &str)]) -> (TempDir, Graph) {
        let dir = TempDir::new();
        for (name, content) in files {
            std::fs::write(dir.0.join(name), content).expect("write source");
        }
        let graph = build_graph(&dir.0).expect("build graph");
        (dir, graph)
    }

    fn anchor_for(dir: &TempDir, graph: &Graph, fqn: &str) -> AnchorKey {
        let node = graph
            .nodes()
            .iter()
            .find(|n| n.fqn == fqn)
            .expect("symbol present in the before-index");
        anchor_of(node, &dir.0)
    }

    #[test]
    fn anchor_of_snapshots_name_bearing_and_name_free_hashes() {
        let (d, graph) = build(&[("a.rs", "pub fn compute(x: i32) -> i32 { x }")]);
        let anchor = anchor_for(&d, &graph, "compute");
        let node = &graph.nodes()[0];
        assert_eq!(anchor.fqn, "compute");
        assert_eq!(anchor.sig_hash, node.sig_hash);
        assert_eq!(
            anchor.shape_hash,
            sig::shape_hash("compute", &node.signature)
        );
        assert_ne!(anchor.shape_hash, anchor.sig_hash);
        // The node carries an absolute machine path; the anchor never does.
        assert_eq!(anchor.file, "a.rs");
        assert!(Path::new(&node.file).is_absolute());
        assert!(!Path::new(&anchor.file).is_absolute());
    }

    #[test]
    fn a_nested_file_is_anchored_by_its_path_under_the_root() {
        let dir = TempDir::new();
        std::fs::create_dir_all(dir.0.join("src")).expect("create src dir");
        std::fs::write(
            dir.0.join("src").join("billing.rs"),
            "pub fn render_invoice(id: u32) -> String { String::new() }",
        )
        .expect("write source");
        let graph = build_graph(&dir.0).expect("build graph");
        let anchor = anchor_for(&dir, &graph, "render_invoice");
        assert_eq!(anchor.file, "src/billing.rs");
    }

    #[test]
    fn confirm_accepts_a_proposed_rename() {
        let (b, before) = build(&[("a.rs", "pub fn compute(x: i32) -> i32 { x }")]);
        let (a, after) = build(&[("a.rs", "pub fn evaluate(x: i32) -> i32 { x }")]);
        let anchor = anchor_for(&b, &before, "compute");
        assert!(classify(&anchor, &after).is_uncertain());

        let confirmed = confirm(&anchor, "evaluate", &after, &a.0).expect("proposed candidate");
        assert_eq!(confirmed, anchor_for(&a, &after, "evaluate"));
        assert_eq!(classify(&confirmed, &after), Classification::Intact);
    }

    #[test]
    fn confirm_accepts_the_evolved_interface_under_the_same_fqn() {
        let (b, before) = build(&[("a.rs", "pub fn total(a: i32) -> i32 { a }")]);
        let (a, after) = build(&[("a.rs", "pub fn total(a: i64) -> i64 { 0 }")]);
        let anchor = anchor_for(&b, &before, "total");
        assert_eq!(classify(&anchor, &after), Classification::Evolved);

        let confirmed = confirm(&anchor, "total", &after, &a.0).expect("same fqn");
        assert_eq!(confirmed.fqn, "total");
        assert_ne!(confirmed.sig_hash, anchor.sig_hash);
        assert_eq!(classify(&confirmed, &after), Classification::Intact);
    }

    #[test]
    fn confirm_rejects_anything_not_proposed() {
        let (b, before) = build(&[("a.rs", "pub fn total(a: i32) -> i32 { a }")]);
        let (a, after) = build(&[(
            "a.rs",
            "pub fn total(a: i64) -> i64 { 0 }\npub fn other(a: i64) -> i64 { 0 }",
        )]);
        let anchor = anchor_for(&b, &before, "total");
        // `other` exists in the index but was never proposed: refused.
        assert_eq!(
            confirm(&anchor, "other", &after, &a.0),
            Err(ConfirmError::NotProposed {
                fqn: "total".into(),
                chosen: "other".into(),
                candidates: vec!["total".into()],
            })
        );
    }

    #[test]
    fn confirm_lists_every_candidate_of_an_ambiguous_rename() {
        let (b, before) = build(&[("a.rs", "pub fn compute(x: i32) -> i32 { x }")]);
        let (a, after) = build(&[(
            "a.rs",
            "pub fn evaluate(x: i32) -> i32 { x }\n\
             pub fn assess(x: i32) -> i32 { x }\n\
             pub fn unrelated(s: String) -> String { s }",
        )]);
        let anchor = anchor_for(&b, &before, "compute");
        match confirm(&anchor, "unrelated", &after, &a.0) {
            Err(ConfirmError::NotProposed { candidates, .. }) => {
                assert_eq!(candidates.len(), 2);
                assert!(candidates.contains(&"evaluate".to_string()));
                assert!(candidates.contains(&"assess".to_string()));
            }
            other => panic!("expected NotProposed, got {other:?}"),
        }
        assert!(confirm(&anchor, "assess", &after, &a.0).is_ok());
    }

    #[test]
    fn confirm_rejects_intact_and_orphaned_anchors() {
        let (b, before) = build(&[(
            "a.rs",
            "pub fn keep(x: i32) -> i32 { x }\npub fn obsolete(token: String) -> Vec<u8> { vec![] }",
        )]);
        let (a, after) = build(&[("a.rs", "pub fn keep(x: i32) -> i32 { x }")]);
        assert_eq!(
            confirm(&anchor_for(&b, &before, "keep"), "keep", &after, &a.0),
            Err(ConfirmError::Intact("keep".into()))
        );
        assert_eq!(
            confirm(&anchor_for(&b, &before, "obsolete"), "keep", &after, &a.0),
            Err(ConfirmError::NoCandidates("obsolete".into()))
        );
    }

    #[test]
    fn unchanged_symbol_is_intact() {
        let (b, before) = build(&[("a.rs", "pub fn keep(x: i32) -> i32 { x }")]);
        let (_a, after) = build(&[("a.rs", "pub fn keep(x: i32) -> i32 { x + 1 }")]);
        let anchor = anchor_for(&b, &before, "keep");
        assert_eq!(classify(&anchor, &after), Classification::Intact);
    }

    #[test]
    fn scenario_rename_is_uncertain_reanchor() {
        // A pure rename changes the name-bearing sig_hash, but the name-free
        // shape hash still matches, so it is an uncertain proposal, not Orphaned.
        let (b, before) = build(&[("a.rs", "pub fn compute(x: i32) -> i32 { x }")]);
        let (_a, after) = build(&[("a.rs", "pub fn evaluate(x: i32) -> i32 { x }")]);
        let anchor = anchor_for(&b, &before, "compute");
        let c = classify(&anchor, &after);
        assert_eq!(
            c,
            Classification::ReanchorCandidate {
                candidates: vec!["evaluate".to_string()],
                basis: ReanchorBasis::ShapeHash,
            }
        );
        assert!(c.is_uncertain());
        assert_ne!(c.status(), Status::Intact);
    }

    #[test]
    fn scenario_rename_with_shared_shape_is_ambiguous() {
        // Two functions share the shape, so a rename yields multiple candidates:
        // uncertain, never a confident Intact.
        let (b, before) = build(&[("a.rs", "pub fn compute(x: i32) -> i32 { x }")]);
        let (_a, after) = build(&[(
            "a.rs",
            "pub fn evaluate(x: i32) -> i32 { x }\npub fn assess(x: i32) -> i32 { x }",
        )]);
        let anchor = anchor_for(&b, &before, "compute");
        let c = classify(&anchor, &after);
        match &c {
            Classification::ReanchorCandidate { candidates, basis } => {
                assert_eq!(*basis, ReanchorBasis::ShapeHash);
                assert_eq!(candidates.len(), 2);
                assert!(candidates.contains(&"evaluate".to_string()));
                assert!(candidates.contains(&"assess".to_string()));
            }
            other => panic!("expected an uncertain re-anchor proposal, got {other:?}"),
        }
        assert!(c.is_uncertain());
        assert_ne!(c.status(), Status::Intact);
    }

    #[test]
    fn scenario_move_free_fn_keeps_fqn_and_is_intact() {
        // A free fn's fqn is its name, so moving files leaves the anchor intact.
        let (b, before) = build(&[("a.rs", "pub fn handler(x: i32) -> i32 { x }")]);
        let (_a, after) = build(&[("b.rs", "pub fn handler(x: i32) -> i32 { x }")]);
        let anchor = anchor_for(&b, &before, "handler");
        assert_eq!(classify(&anchor, &after), Classification::Intact);
    }

    #[test]
    fn scenario_move_method_to_new_type_is_uncertain_reanchor() {
        // fqn changes (A::run -> B::run) but the method signature hash is stable.
        let (b, before) = build(&[("a.rs", "struct A; impl A { pub fn run(&self) {} }")]);
        let (_a, after) = build(&[("a.rs", "struct B; impl B { pub fn run(&self) {} }")]);
        let anchor = anchor_for(&b, &before, "A::run");
        let c = classify(&anchor, &after);
        assert_eq!(
            c,
            Classification::ReanchorCandidate {
                candidates: vec!["B::run".to_string()],
                basis: ReanchorBasis::SigHash,
            }
        );
        assert!(c.is_uncertain());
        assert_ne!(c.status(), Status::Intact);
    }

    #[test]
    fn scenario_signature_change_is_evolved() {
        let (b, before) = build(&[("a.rs", "pub fn total(a: i32, b: i32) -> i32 { a + b }")]);
        let (_a, after) = build(&[("a.rs", "pub fn total(a: i32, b: i64) -> i64 { 0 }")]);
        let anchor = anchor_for(&b, &before, "total");
        let c = classify(&anchor, &after);
        assert_eq!(c, Classification::Evolved);
        assert_eq!(c.status(), Status::Evolved);
    }

    #[test]
    fn scenario_deletion_is_orphaned() {
        // The deleted symbol has a distinctive shape, so nothing shape-matches
        // it (a trivially-shaped `fn ()` would instead surface as a candidate).
        let (b, before) = build(&[(
            "a.rs",
            "pub fn obsolete(token: String) -> Vec<u8> { vec![] }\npub fn other() {}",
        )]);
        let (_a, after) = build(&[("a.rs", "pub fn other() {}")]);
        let anchor = anchor_for(&b, &before, "obsolete");
        assert_eq!(classify(&anchor, &after), Classification::Orphaned);
    }

    #[test]
    fn empty_shape_hash_never_matches() {
        // A memory written before shape_hash existed defaults to empty; it must
        // never shape-match a node.
        let (_a, after) = build(&[("a.rs", "pub fn something() {}")]);
        let anchor = AnchorKey {
            fqn: "vanished".to_string(),
            sig_hash: "nomatch0000000000".to_string(),
            shape_hash: String::new(),
            file: String::new(),
        };
        assert_eq!(classify(&anchor, &after), Classification::Orphaned);
    }

    #[test]
    fn scenario_duplicated_subtree_is_uncertain_never_intact() {
        // The anchored fqn is gone and TWO nodes now share its signature hash:
        // ambiguous, so it must resolve to an uncertain proposal, never Intact.
        let (b, before) = build(&[(
            "a.rs",
            "struct Original; impl Original { pub fn helper(&self) -> i32 { 0 } }",
        )]);
        let (_a, after) = build(&[(
            "a.rs",
            "struct CopyA; struct CopyB;\n\
             impl CopyA { pub fn helper(&self) -> i32 { 0 } }\n\
             impl CopyB { pub fn helper(&self) -> i32 { 0 } }",
        )]);
        let anchor = anchor_for(&b, &before, "Original::helper");
        let c = classify(&anchor, &after);

        match &c {
            Classification::ReanchorCandidate { candidates, basis } => {
                assert_eq!(*basis, ReanchorBasis::SigHash);
                assert_eq!(candidates.len(), 2, "both duplicates are candidates");
                assert!(candidates.contains(&"CopyA::helper".to_string()));
                assert!(candidates.contains(&"CopyB::helper".to_string()));
            }
            other => panic!("expected an uncertain re-anchor proposal, got {other:?}"),
        }
        assert!(c.is_uncertain());
        assert_ne!(c.status(), Status::Intact);
    }

    #[test]
    fn token_similarity_fallback_is_uncertain() {
        // Renamed AND reshaped: neither sig_hash nor shape_hash matches, but the
        // fqn tokens overlap enough to propose a re-anchor — still uncertain.
        let (b, before) = build(&[(
            "a.rs",
            "struct Foo; impl Foo { pub fn compute_total(&self) -> i32 { 0 } }",
        )]);
        let (_a, after) = build(&[(
            "a.rs",
            "struct Foo; impl Foo { pub fn compute_total_sum(&self, extra: i32) -> i64 { 0 } }",
        )]);
        let anchor = anchor_for(&b, &before, "Foo::compute_total");
        let c = classify(&anchor, &after);
        assert!(
            matches!(
                &c,
                Classification::ReanchorCandidate {
                    basis: ReanchorBasis::TokenSimilarity,
                    ..
                }
            ),
            "expected token-similarity proposal, got {c:?}"
        );
        assert!(c.is_uncertain());
        assert_ne!(c.status(), Status::Intact);
    }

    #[test]
    fn invariant_uncertain_is_never_intact() {
        // The load-bearing property, checked across every synthetic scenario.
        let (b1, before1) = build(&[("a.rs", "struct A; impl A { pub fn run(&self) {} }")]);
        let (_a1, after1) = build(&[("a.rs", "struct B; impl B { pub fn run(&self) {} }")]);

        let (b2, before2) = build(&[(
            "a.rs",
            "struct O; impl O { pub fn helper(&self) -> i32 { 0 } }",
        )]);
        let (_a2, after2) = build(&[(
            "a.rs",
            "struct P; struct Q;\n\
             impl P { pub fn helper(&self) -> i32 { 0 } }\n\
             impl Q { pub fn helper(&self) -> i32 { 0 } }",
        )]);

        let (b3, before3) = build(&[(
            "a.rs",
            "struct Foo; impl Foo { pub fn load_user_data(&self) {} }",
        )]);
        let (_a3, after3) = build(&[(
            "a.rs",
            "struct Foo; impl Foo { pub fn load_user_info(&self, id: u64) {} }",
        )]);

        let cases = [
            classify(&anchor_for(&b1, &before1, "A::run"), &after1),
            classify(&anchor_for(&b2, &before2, "O::helper"), &after2),
            classify(&anchor_for(&b3, &before3, "Foo::load_user_data"), &after3),
        ];

        for c in &cases {
            assert!(c.is_uncertain(), "expected an uncertain case, got {c:?}");
            assert_ne!(
                c.status(),
                Status::Intact,
                "an uncertain re-anchor must never be served as Intact: {c:?}"
            );
        }
    }
}
