//! Turn an invariant memory into an executable Rust test.
//!
//! The generated file imports the anchored free function from its crate, calls
//! it with `Default::default()` inputs and asserts the property inferred from
//! the memory's wording, so drift that breaks the invariant fails a test
//! instead of silently invalidating the memory. Inference is a keyword
//! heuristic, not NLP: unrecognised wording yields a test that panics until a
//! human encodes the assertion.

use std::path::{Component, Path};

use quote::quote;

use crate::graph::Graph;
use crate::memory::anchor::{classify, Classification};
use crate::memory::model::{Kind, Memory};
use crate::model::{Node, NodeKind};
use crate::parser::sig;

/// The property asserted on the function's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assertion {
    Sorted,
    NotNull,
    NonEmpty,
    Positive,
    /// Nothing recognised in the wording: the test panics until encoded by hand.
    Unencoded,
}

impl Assertion {
    pub fn label(self) -> &'static str {
        match self {
            Assertion::Sorted => "sorted",
            Assertion::NotNull => "not-null",
            Assertion::NonEmpty => "non-empty",
            Assertion::Positive => "positive",
            Assertion::Unencoded => "unencoded",
        }
    }

    /// The condition asserted on `result`, or `None` when nothing was inferred.
    pub fn condition(self) -> Option<&'static str> {
        match self {
            Assertion::Sorted => Some("result.windows(2).all(|pair| pair[0] <= pair[1])"),
            Assertion::NotNull => Some("result.is_some()"),
            Assertion::NonEmpty => Some("!result.is_empty()"),
            Assertion::Positive => Some("result > Default::default()"),
            Assertion::Unencoded => None,
        }
    }
}

const SORTED: &[&str] = &["sorted", "ordenad"];
const NOT_NULL: &[&str] = &[
    "not null",
    "no null",
    "non-null",
    "nonnull",
    "never null",
    "not none",
    "never none",
    "is some",
    "always some",
];
const NON_EMPTY: &[&str] = &[
    "non-empty",
    "nonempty",
    "not empty",
    "never empty",
    "no vac",
    "nunca vac",
];
const POSITIVE: &[&str] = &["positiv"];

/// Keyword heuristic over the memory's wording (English or Spanish stems).
pub fn infer_assertion(content: &str) -> Assertion {
    let text = content.to_lowercase();
    let mentions = |keywords: &[&str]| keywords.iter().any(|k| text.contains(k));
    if mentions(SORTED) {
        Assertion::Sorted
    } else if mentions(NOT_NULL) {
        Assertion::NotNull
    } else if mentions(NON_EMPTY) {
        Assertion::NonEmpty
    } else if mentions(POSITIVE) {
        Assertion::Positive
    } else {
        Assertion::Unencoded
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedTest {
    pub test_name: String,
    pub file_name: String,
    /// The `use` path the test imports the symbol from.
    pub import_path: String,
    pub assertion: Assertion,
    pub source: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TestGenError {
    #[error("memory `{id}` is a {kind:?}, not an invariant")]
    NotInvariant { id: String, kind: Kind },
    #[error("anchor `{fqn}` is {status}; confirm it with `reanchor` (or `supersede` the memory) before generating a test")]
    NotIntact { fqn: String, status: &'static str },
    #[error("`{fqn}` cannot be exercised: {reason}")]
    Unsupported { fqn: String, reason: String },
    #[error("`{fqn}` was not found as indexed in {file}; rebuild the index")]
    Stale { fqn: String, file: String },
    #[error("no Cargo.toml above {file}")]
    NoManifest { file: String },
    #[error("{manifest} declares no package name")]
    NoPackageName { manifest: String },
    #[error("{file} is not a library module (only files under src/, excluding main.rs and bin/, are importable)")]
    NotImportable { file: String },
    #[error("reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
}

/// Generate the test for `memory`, whose anchor must be intact in `graph`. The
/// symbol's source is read live to check it is a `pub` free function whose
/// inputs can be synthesised, and to locate the crate it must be imported from.
pub fn generate(memory: &Memory, graph: &Graph) -> Result<GeneratedTest, TestGenError> {
    if !memory.kind.is_invariant() {
        return Err(TestGenError::NotInvariant {
            id: memory.id.0.clone(),
            kind: memory.kind,
        });
    }
    let fqn = &memory.anchor.fqn;
    let status = match classify(&memory.anchor, graph) {
        Classification::Intact => None,
        Classification::Evolved => Some("evolved"),
        Classification::ReanchorCandidate { .. } => Some("orphaned with re-anchor candidates"),
        Classification::Orphaned => Some("orphaned"),
    };
    if let Some(status) = status {
        return Err(TestGenError::NotIntact {
            fqn: fqn.clone(),
            status,
        });
    }
    let node = graph
        .nodes()
        .iter()
        .find(|n| &n.fqn == fqn)
        .ok_or_else(|| stale(fqn, ""))?;
    if node.kind != NodeKind::Function {
        return Err(unsupported(
            fqn,
            format!(
                "only free functions are supported, and it is a {:?}",
                node.kind
            ),
        ));
    }

    let file = Path::new(&node.file);
    let target = locate_crate(file)?;
    let item = find_fn(&read(file)?, node)?;
    if !matches!(item.vis, syn::Visibility::Public(_)) {
        return Err(unsupported(
            fqn,
            "it is not `pub`, so an integration test cannot call it".to_string(),
        ));
    }
    let assertion = infer_assertion(&memory.content);
    let call = call_expression(node, &item.sig, assertion)?;

    let import_path = std::iter::once(target.lib_name.as_str())
        .chain(target.module_path.iter().map(String::as_str))
        .chain(std::iter::once(node.name.as_str()))
        .collect::<Vec<_>>()
        .join("::");
    let test_name = format!("invariant_{}", identifier(&memory.id.0));
    let source = render(memory, node, &import_path, &test_name, &call, assertion);
    Ok(GeneratedTest {
        file_name: format!("{test_name}.rs"),
        test_name,
        import_path,
        assertion,
        source,
    })
}

/// The crate a source file belongs to, as seen from an integration test.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CrateTarget {
    lib_name: String,
    module_path: Vec<String>,
}

/// Walk up from `file` to the nearest Cargo.toml and derive the import path.
fn locate_crate(file: &Path) -> Result<CrateTarget, TestGenError> {
    let display = || file.display().to_string();
    let dir = file.parent().unwrap_or_else(|| Path::new(""));
    let root = dir
        .ancestors()
        .find(|a| a.join("Cargo.toml").is_file())
        .ok_or_else(|| TestGenError::NoManifest { file: display() })?;
    let manifest_path = root.join("Cargo.toml");
    let lib_name =
        lib_name_of(&read(&manifest_path)?).ok_or_else(|| TestGenError::NoPackageName {
            manifest: manifest_path.display().to_string(),
        })?;
    let module_path =
        module_path(root, file).ok_or_else(|| TestGenError::NotImportable { file: display() })?;
    Ok(CrateTarget {
        lib_name,
        module_path,
    })
}

/// The library target name: `[lib] name` if set, else the package name with
/// `-` mapped to `_`. A line scan, deliberately not a TOML parser.
fn lib_name_of(manifest: &str) -> Option<String> {
    let mut section = "";
    let mut package = None;
    let mut lib = None;
    for raw in manifest.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            section = line;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "name" {
            continue;
        }
        let value = value
            .split('#')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches(['"', '\'']);
        if value.is_empty() {
            continue;
        }
        match section {
            "[package]" => package = Some(value.to_string()),
            "[lib]" => lib = Some(value.to_string()),
            _ => {}
        }
    }
    lib.or_else(|| package.map(|name| name.replace('-', "_")))
}

/// Module path of `file` inside the crate rooted at `root`, or `None` when the
/// file is not part of the library target.
fn module_path(root: &Path, file: &Path) -> Option<Vec<String>> {
    let relative = file.strip_prefix(root.join("src")).ok()?;
    let mut parts: Vec<String> = relative
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let leaf = parts.pop()?;
    let stem = leaf.strip_suffix(".rs")?;
    if parts.first().map(String::as_str) == Some("bin") {
        return None;
    }
    match stem {
        "main" if parts.is_empty() => None,
        "lib" if parts.is_empty() => Some(parts),
        "mod" => Some(parts),
        _ => {
            parts.push(stem.to_string());
            Some(parts)
        }
    }
}

/// The top-level fn named like `node`, verified against the indexed signature
/// hash so a test is never generated from a stale index.
fn find_fn(source: &str, node: &Node) -> Result<syn::ItemFn, TestGenError> {
    let ast = syn::parse_file(source).map_err(|_| stale(&node.fqn, &node.file))?;
    ast.items
        .into_iter()
        .find_map(|item| match item {
            syn::Item::Fn(f) if f.sig.ident == node.name => {
                let sig = &f.sig;
                let signature = quote! { #sig }.to_string();
                (sig::sig_hash(&node.name, &signature) == node.sig_hash).then_some(f)
            }
            _ => None,
        })
        .ok_or_else(|| stale(&node.fqn, &node.file))
}

/// `name(Default::default(), ...)`, refusing signatures whose inputs cannot be
/// synthesised or whose result cannot carry the assertion.
fn call_expression(
    node: &Node,
    sig: &syn::Signature,
    assertion: Assertion,
) -> Result<String, TestGenError> {
    let refuse = |reason: &str| Err(unsupported(&node.fqn, reason.to_string()));
    if sig.asyncness.is_some() {
        return refuse("it is async");
    }
    if sig.unsafety.is_some() {
        return refuse("it is unsafe");
    }
    if sig.variadic.is_some() {
        return refuse("it is variadic");
    }
    if sig
        .generics
        .params
        .iter()
        .any(|p| !matches!(p, syn::GenericParam::Lifetime(_)))
    {
        return refuse("it has generic parameters, so inputs cannot be synthesised");
    }
    let mut inputs = Vec::new();
    for arg in &sig.inputs {
        match arg {
            syn::FnArg::Receiver(_) => return refuse("it takes `self`"),
            syn::FnArg::Typed(typed) if matches!(*typed.ty, syn::Type::ImplTrait(_)) => {
                return refuse("it takes an `impl Trait` argument");
            }
            syn::FnArg::Typed(_) => inputs.push("Default::default()"),
        }
    }
    if assertion != Assertion::Unencoded && matches!(sig.output, syn::ReturnType::Default) {
        return refuse("it returns nothing to assert on");
    }
    Ok(format!("{}({})", node.name, inputs.join(", ")))
}

fn render(
    memory: &Memory,
    node: &Node,
    import_path: &str,
    test_name: &str,
    call: &str,
    assertion: Assertion,
) -> String {
    let content = memory.content.lines().collect::<Vec<_>>().join("\n//!   ");
    let mut out = format!(
        "//! Generated by codegraph from memory `{}` (invariant anchored to `{}`).\n\
         //! Regenerating with the `generate_test` tool overwrites this file.\n\
         //!\n\
         //! Invariant: {content}\n",
        memory.id.0, node.fqn
    );
    match assertion.condition() {
        Some(condition) => out.push_str(&format!(
            "//! Assertion: {} — `{condition}`\n",
            assertion.label()
        )),
        None => out.push_str(
            "//! Assertion: none inferred from the wording; the test panics until encoded by hand.\n",
        ),
    }
    out.push_str(&format!(
        "//! Inputs: every argument is `Default::default()`.\n\
         //! Anchor: {} @ {}\n\
         \n\
         use {import_path};\n\
         \n\
         const SYMBOL: &str = {:?};\n\
         const INVARIANT: &str = {:?};\n\
         \n\
         #[test]\n\
         fn {test_name}() {{\n",
        node.fqn, node.sig_hash, node.fqn, memory.content
    ));
    match assertion.condition() {
        Some(condition) => out.push_str(&format!(
            "    let result = {call};\n\
             \x20   assert!(\n\
             \x20       {condition},\n\
             \x20       \"invariant violated for `{{}}`: {{}} (got {{:?}})\",\n\
             \x20       SYMBOL,\n\
             \x20       INVARIANT,\n\
             \x20       result\n\
             \x20   );\n"
        )),
        None => out.push_str(&format!(
            "    let _result = {call};\n\
             \x20   panic!(\"invariant not encoded for `{{}}`: {{}}\", SYMBOL, INVARIANT);\n"
        )),
    }
    out.push_str("}\n");
    out
}

/// A memory id as a snake_case identifier fragment.
fn identifier(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn read(path: &Path) -> Result<String, TestGenError> {
    std::fs::read_to_string(path).map_err(|source| TestGenError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn stale(fqn: &str, file: &str) -> TestGenError {
    TestGenError::Stale {
        fqn: fqn.to_string(),
        file: file.to_string(),
    }
}

fn unsupported(fqn: &str, reason: String) -> TestGenError {
    TestGenError::Unsupported {
        fqn: fqn.to_string(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::build_graph;
    use crate::memory::anchor::anchor_of;
    use crate::memory::model::{AnchorKey, MemoryId, Provenance, Scope};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    struct TempCrate(PathBuf);

    impl TempCrate {
        fn new(lib: &str) -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("cg_testgen_{}_{n}", std::process::id()));
            std::fs::create_dir_all(dir.join("src")).expect("create crate dir");
            std::fs::write(
                dir.join("Cargo.toml"),
                "[package]\nname = \"demo-crate\"\nversion = \"0.1.0\"\n",
            )
            .expect("write manifest");
            std::fs::write(dir.join("src").join("lib.rs"), lib).expect("write lib");
            Self(dir)
        }

        fn graph(&self) -> Graph {
            build_graph(&self.0).expect("build graph")
        }
    }

    impl Drop for TempCrate {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn memory_for(graph: &Graph, fqn: &str, kind: Kind, content: &str) -> Memory {
        let node = graph
            .nodes()
            .iter()
            .find(|n| n.fqn == fqn)
            .expect("symbol in index");
        Memory {
            id: MemoryId("mem-1".into()),
            content: content.into(),
            anchor: anchor_of(node),
            scope: Scope::Symbol(fqn.into()),
            kind,
            provenance: Provenance::default(),
        }
    }

    #[test]
    fn infers_assertions_from_keywords() {
        assert_eq!(
            infer_assertion("callers assume this is sorted"),
            Assertion::Sorted
        );
        assert_eq!(
            infer_assertion("Devuelve la lista ORDENADA"),
            Assertion::Sorted
        );
        assert_eq!(
            infer_assertion("never returns None / not null"),
            Assertion::NotNull
        );
        assert_eq!(
            infer_assertion("the result is always some"),
            Assertion::NotNull
        );
        assert_eq!(infer_assertion("result is non-empty"), Assertion::NonEmpty);
        assert_eq!(infer_assertion("nunca vacío"), Assertion::NonEmpty);
        assert_eq!(
            infer_assertion("return value is always positive"),
            Assertion::Positive
        );
        assert_eq!(infer_assertion("siempre positivo"), Assertion::Positive);
        assert_eq!(infer_assertion("must be idempotent"), Assertion::Unencoded);
        assert_eq!(infer_assertion(""), Assertion::Unencoded);
    }

    #[test]
    fn first_matching_family_wins() {
        assert_eq!(infer_assertion("sorted and non-empty"), Assertion::Sorted);
    }

    #[test]
    fn lib_name_maps_hyphens_and_honours_lib_override() {
        assert_eq!(
            lib_name_of("[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\n"),
            Some("my_crate".into())
        );
        assert_eq!(
            lib_name_of("[package]\nname = 'my-crate'\n\n[lib]\nname = \"custom\" # renamed\n"),
            Some("custom".into())
        );
        assert_eq!(
            lib_name_of("[package]\nname = \"pkg\"\n[dependencies]\nname = \"not-a-crate\"\n"),
            Some("pkg".into())
        );
        assert_eq!(lib_name_of("[workspace]\nmembers = [\"a\"]\n"), None);
        assert_eq!(lib_name_of(""), None);
    }

    #[test]
    fn module_paths_follow_cargo_layout() {
        let root = Path::new("repo");
        let path = |p: &str| module_path(root, &root.join(p));
        assert_eq!(path("src/lib.rs"), Some(vec![]));
        assert_eq!(path("src/foo.rs"), Some(vec!["foo".into()]));
        assert_eq!(path("src/foo/mod.rs"), Some(vec!["foo".into()]));
        assert_eq!(
            path("src/foo/bar.rs"),
            Some(vec!["foo".into(), "bar".into()])
        );
        assert_eq!(path("src/main.rs"), None);
        assert_eq!(path("src/bin/tool.rs"), None);
        assert_eq!(path("tests/it.rs"), None);
        assert_eq!(path("src/notes.txt"), None);
    }

    #[test]
    fn identifiers_are_snake_case() {
        assert_eq!(identifier("mem-0"), "mem_0");
        assert_eq!(identifier("Test Inv!"), "test_inv_");
        assert_eq!(identifier(""), "");
    }

    #[test]
    fn generates_a_positive_test_for_a_pub_free_fn() {
        let repo = TempCrate::new("pub fn compute(x: i32) -> i32 { x + 1 }\n");
        let graph = repo.graph();
        let memory = memory_for(&graph, "compute", Kind::Invariant, "always positive");
        let generated = generate(&memory, &graph).expect("generate");
        assert_eq!(generated.assertion, Assertion::Positive);
        assert_eq!(generated.test_name, "invariant_mem_1");
        assert_eq!(generated.file_name, "invariant_mem_1.rs");
        assert_eq!(generated.import_path, "demo_crate::compute");
        let src = &generated.source;
        assert!(src.contains("use demo_crate::compute;"), "{src}");
        assert!(src.contains("fn invariant_mem_1()"), "{src}");
        assert!(
            src.contains("let result = compute(Default::default());"),
            "{src}"
        );
        assert!(src.contains("result > Default::default()"), "{src}");
        assert!(
            src.contains("const INVARIANT: &str = \"always positive\";"),
            "{src}"
        );
        assert!(
            src.contains(&format!("//! Anchor: compute @ {}", memory.anchor.sig_hash)),
            "{src}"
        );
    }

    #[test]
    fn generated_source_parses_as_rust() {
        let repo = TempCrate::new("pub fn items() -> Vec<u8> { vec![] }\n");
        let graph = repo.graph();
        for content in ["sorted", "not null", "non-empty", "positive", "who knows"] {
            let memory = memory_for(&graph, "items", Kind::Invariant, content);
            let generated = generate(&memory, &graph).expect("generate");
            assert!(
                syn::parse_file(&generated.source).is_ok(),
                "{}",
                generated.source
            );
        }
    }

    #[test]
    fn content_is_escaped_into_the_literal() {
        let repo = TempCrate::new("pub fn items() -> Vec<u8> { vec![] }\n");
        let graph = repo.graph();
        let content = "sorted \"by key\" {always}\nsecond line \\ end";
        let memory = memory_for(&graph, "items", Kind::Invariant, content);
        let generated = generate(&memory, &graph).expect("generate");
        assert!(generated
            .source
            .contains("//! Invariant: sorted \"by key\" {always}\n//!   second line"));
        let ast = syn::parse_file(&generated.source).expect("valid rust");
        let literal = ast
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Const(c) if c.ident == "INVARIANT" => match &*c.expr {
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) => Some(s.value()),
                    _ => None,
                },
                _ => None,
            })
            .expect("INVARIANT const");
        assert_eq!(literal, content);
    }

    #[test]
    fn unencoded_wording_yields_a_panicking_scaffold() {
        let repo = TempCrate::new("pub fn touch() {}\n");
        let graph = repo.graph();
        let memory = memory_for(&graph, "touch", Kind::Invariant, "must be idempotent");
        let generated = generate(&memory, &graph).expect("generate");
        assert_eq!(generated.assertion, Assertion::Unencoded);
        assert!(generated.source.contains("let _result = touch();"));
        assert!(generated.source.contains("panic!(\"invariant not encoded"));
    }

    #[test]
    fn zero_arg_call_has_no_inputs() {
        let repo = TempCrate::new("pub fn seed() -> u32 { 1 }\n");
        let graph = repo.graph();
        let memory = memory_for(&graph, "seed", Kind::Invariant, "positive");
        let generated = generate(&memory, &graph).expect("generate");
        assert!(generated.source.contains("let result = seed();"));
    }

    #[test]
    fn module_files_are_imported_through_their_module() {
        let repo = TempCrate::new("pub mod util;\n");
        std::fs::create_dir_all(repo.0.join("src").join("util")).expect("mkdir");
        std::fs::write(
            repo.0.join("src").join("util").join("math.rs"),
            "pub fn twice(x: i32) -> i32 { x * 2 }\n",
        )
        .expect("write module");
        let graph = repo.graph();
        let memory = memory_for(&graph, "twice", Kind::Invariant, "positive");
        let generated = generate(&memory, &graph).expect("generate");
        assert_eq!(generated.import_path, "demo_crate::util::math::twice");
    }

    #[test]
    fn rejects_non_invariants() {
        let repo = TempCrate::new("pub fn compute(x: i32) -> i32 { x }\n");
        let graph = repo.graph();
        let memory = memory_for(&graph, "compute", Kind::Gotcha, "positive");
        let err = generate(&memory, &graph).expect_err("not an invariant");
        assert!(
            matches!(
                err,
                TestGenError::NotInvariant {
                    kind: Kind::Gotcha,
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn rejects_anchors_that_are_not_intact() {
        let repo = TempCrate::new("pub fn compute(x: i32) -> i32 { x }\n");
        let graph = repo.graph();
        let mut memory = memory_for(&graph, "compute", Kind::Invariant, "positive");
        memory.anchor.sig_hash = "0000000000000000".into();
        let err = generate(&memory, &graph).expect_err("evolved");
        assert!(
            matches!(
                err,
                TestGenError::NotIntact {
                    status: "evolved",
                    ..
                }
            ),
            "{err}"
        );

        memory.anchor = AnchorKey {
            fqn: "vanished".into(),
            sig_hash: "0000000000000000".into(),
            shape_hash: String::new(),
            file: String::new(),
        };
        let err = generate(&memory, &graph).expect_err("orphaned");
        assert!(
            matches!(
                err,
                TestGenError::NotIntact {
                    status: "orphaned",
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn rejects_private_generic_method_async_and_unit_symbols() {
        let repo = TempCrate::new(
            "fn hidden(x: i32) -> i32 { x }\n\
             pub fn generic<T: Default>(x: T) -> T { x }\n\
             pub fn opaque(x: impl Into<i32>) -> i32 { x.into() }\n\
             pub async fn later() -> i32 { 1 }\n\
             pub unsafe fn danger() -> i32 { 1 }\n\
             pub fn nothing(x: i32) {}\n\
             pub struct S;\n\
             impl S { pub fn make() -> i32 { 1 } }\n",
        );
        let graph = repo.graph();
        for (fqn, fragment) in [
            ("hidden", "not `pub`"),
            ("generic", "generic"),
            ("opaque", "impl Trait"),
            ("later", "async"),
            ("danger", "unsafe"),
            ("nothing", "returns nothing"),
            ("S::make", "free functions"),
        ] {
            let memory = memory_for(&graph, fqn, Kind::Invariant, "positive");
            let err = generate(&memory, &graph).expect_err(fqn);
            assert!(
                matches!(err, TestGenError::Unsupported { .. }),
                "{fqn}: {err}"
            );
            assert!(err.to_string().contains(fragment), "{fqn}: {err}");
        }
    }

    #[test]
    fn unit_return_is_fine_for_an_unencoded_scaffold() {
        let repo = TempCrate::new("pub fn nothing(x: i32) {}\n");
        let graph = repo.graph();
        let memory = memory_for(&graph, "nothing", Kind::Invariant, "idempotent");
        assert!(generate(&memory, &graph).is_ok());
    }

    #[test]
    fn rejects_a_stale_index() {
        let repo = TempCrate::new("pub fn compute(x: i32) -> i32 { x }\n");
        let graph = repo.graph();
        let memory = memory_for(&graph, "compute", Kind::Invariant, "positive");
        // The file changed after indexing; the stored hash no longer matches.
        std::fs::write(
            repo.0.join("src").join("lib.rs"),
            "pub fn compute(x: i64) -> i64 { x }\n",
        )
        .expect("rewrite");
        let err = generate(&memory, &graph).expect_err("stale");
        assert!(matches!(err, TestGenError::Stale { .. }), "{err}");
    }

    #[test]
    fn rejects_files_outside_a_crate() {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("cg_testgen_nocrate_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("a.rs"), "pub fn compute(x: i32) -> i32 { x }\n").expect("write");
        let graph = build_graph(&dir).expect("build graph");
        let memory = memory_for(&graph, "compute", Kind::Invariant, "positive");
        let err = generate(&memory, &graph).expect_err("no manifest");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(matches!(err, TestGenError::NoManifest { .. }), "{err}");
    }

    #[test]
    fn rejects_binary_targets() {
        let repo = TempCrate::new("");
        std::fs::write(
            repo.0.join("src").join("main.rs"),
            "pub fn compute(x: i32) -> i32 { x }\nfn main() {}\n",
        )
        .expect("write main");
        let graph = repo.graph();
        let memory = memory_for(&graph, "compute", Kind::Invariant, "positive");
        let err = generate(&memory, &graph).expect_err("binary");
        assert!(matches!(err, TestGenError::NotImportable { .. }), "{err}");
    }
}
