//! File paths as the memory layer stores and compares them.
//!
//! `Node::file` is the indexed root joined with the walked path, so it is an
//! absolute machine path (on Windows, with mixed separators). An anchor must
//! not carry that: it is not portable across machines, not comparable to the
//! path a caller types, and not something to hand back in a tool response.
//! Everything here is pure string and path work — the graph keeps its own
//! format untouched.

use std::ffi::OsStr;
use std::path::{Component, Path};

/// `file` as the memory layer stores it: relative to the index `root`, with `/`
/// separators.
///
/// A path that cannot be placed under `root` is kept as written — that happens
/// only when the graph was indexed under a different spelling of the root than
/// the one being served, and an already-relative path passes through unchanged.
pub fn relative(file: &str, root: &Path) -> String {
    let path = Path::new(file);
    let stripped = path
        .strip_prefix(root)
        .ok()
        .map(|rest| rest.to_string_lossy().into_owned())
        .or_else(|| strip_root_ignoring_case(path, root))
        .unwrap_or_else(|| file.to_string());
    let out = normalize(&stripped);
    if out.is_empty() {
        // The index root is the file itself; keep its name so it stays findable.
        return path
            .file_name()
            .map(|name| normalize(&name.to_string_lossy()))
            .unwrap_or_default();
    }
    out
}

/// A path in comparable form: `/` separators, no leading `./`, no trailing `/`.
pub fn normalize(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_string()
}

/// True when `stored` names the file `query` asks about.
///
/// The query may carry a longer prefix — `crates/demo/src/a.rs` reaches a
/// memory stored as `src/a.rs` — but never a shorter one, or `mod.rs` would
/// reach every module in the tree. Matching is on path boundaries, so
/// `othersrc/a.rs` does not reach `src/a.rs`. An empty side never matches, which
/// is what keeps an anchor written before the field existed out of every
/// file query.
pub fn matches(stored: &str, query: &str) -> bool {
    let stored = normalize(stored);
    let query = normalize(query);
    if stored.is_empty() || query.is_empty() {
        return false;
    }
    stored == query || query.ends_with(&format!("/{stored}"))
}

/// Windows spells one directory several ways (`c:\repo`, `C:/repo`), and the
/// root that produced `Node::file` at build time need not be spelled the way the
/// server was started. Compared component-wise so a separator or case
/// difference does not leave an absolute path in an anchor.
fn strip_root_ignoring_case(path: &Path, root: &Path) -> Option<String> {
    let mut rest = path.components().filter(is_named);
    for want in root.components().filter(is_named) {
        if !same_component(rest.next()?, want) {
            return None;
        }
    }
    let tail: Vec<String> = rest
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(tail.join("/"))
}

fn is_named(c: &Component) -> bool {
    !matches!(c, Component::CurDir)
}

fn same_component(a: Component, b: Component) -> bool {
    let (a, b): (&OsStr, &OsStr) = (a.as_os_str(), b.as_os_str());
    a == b
        || a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn an_absolute_node_path_becomes_root_relative_with_forward_slashes() {
        // Exactly the shape the builder produces: the root as the CLI spelled
        // it, joined with the walked path.
        let root = PathBuf::from("C:/repo/demo");
        assert_eq!(
            relative("C:/repo/demo\\src\\billing.rs", &root),
            "src/billing.rs"
        );
        assert_eq!(
            relative("/home/u/demo/src/a.rs", Path::new("/home/u/demo")),
            "src/a.rs"
        );
    }

    #[test]
    fn a_relative_root_and_an_already_relative_path_both_normalize() {
        assert_eq!(relative(".\\src\\a.rs", Path::new(".")), "src/a.rs");
        assert_eq!(
            relative("./src/a.rs", Path::new("C:/elsewhere")),
            "src/a.rs"
        );
        assert_eq!(relative("src/a.rs", Path::new("C:/repo")), "src/a.rs");
    }

    #[test]
    fn the_root_spelling_may_differ_in_case_or_separator() {
        assert_eq!(
            relative("c:\\Repo\\Demo\\src\\a.rs", Path::new("C:/repo/demo")),
            "src/a.rs"
        );
        assert_eq!(
            relative("C:/repo/demo/src/a.rs", Path::new("C:/repo/demo/")),
            "src/a.rs"
        );
    }

    #[test]
    fn an_unrelatable_path_is_kept_as_written() {
        // Serving a root the index was not built under: nothing to strip, and
        // guessing would be worse than saying what was recorded.
        assert_eq!(
            relative("D:/other/src/a.rs", Path::new("C:/repo")),
            "D:/other/src/a.rs"
        );
    }

    #[test]
    fn indexing_a_single_file_keeps_its_name() {
        assert_eq!(
            relative("C:/repo/lib.rs", Path::new("C:/repo/lib.rs")),
            "lib.rs"
        );
    }

    #[test]
    fn a_query_may_be_longer_but_never_shorter() {
        assert!(matches("src/a.rs", "src/a.rs"));
        assert!(matches("src/a.rs", "crates/demo/src/a.rs"));
        assert!(matches("src/a.rs", "src\\a.rs"));
        assert!(matches("src/a.rs", "./src/a.rs"));
        // A bare file name must not drag in every same-named file.
        assert!(!matches("src/a.rs", "a.rs"));
        assert!(!matches("src/mod.rs", "mod.rs"));
    }

    #[test]
    fn matching_respects_path_boundaries() {
        assert!(!matches("src/a.rs", "othersrc/a.rs"));
        assert!(!matches("a.rs", "banana.rs"));
    }

    #[test]
    fn an_empty_side_never_matches() {
        // An anchor written before the field existed defaults to empty; suffix
        // logic would otherwise make it match every query.
        assert!(!matches("", "src/a.rs"));
        assert!(!matches("src/a.rs", ""));
        assert!(!matches("", ""));
        // A target that normalizes away must fall into the same guard.
        assert!(!matches("src/a.rs", "./"));
        assert!(!matches("src/a.rs", "."));
    }
}
