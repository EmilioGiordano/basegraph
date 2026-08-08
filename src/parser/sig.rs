//! Deterministic hashing of a symbol's normalized interface (name + signature).
//!
//! The hash is persisted, so it must stay stable across runs, platforms and
//! Rust versions. It therefore uses a hand-rolled FNV-1a rather than any std
//! hasher (whose algorithm is not guaranteed stable across versions).

/// Collapse every run of whitespace to a single space and trim, so signatures
/// that differ only in formatting map to the same string. Token boundaries are
/// preserved: distinct tokens keep exactly one separating space.
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// FNV-1a over the bytes of `s`, 64-bit. Uses wrapping arithmetic so the result
/// is identical on every platform and Rust version.
fn fnv1a(s: &str) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for byte in s.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Stable hash of a symbol's interface: its name plus its normalized signature,
/// rendered as 16 lowercase hex chars. The signature the parser stores already
/// excludes bodies and attributes, so neither influences the result.
pub fn sig_hash(name: &str, signature: &str) -> String {
    let normalized = normalize(&format!("{name} {signature}"));
    format!("{:016x}", fnv1a(&normalized))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize("fn  f (x : i32)"), "fn f (x : i32)");
        assert_eq!(normalize("fn\tf\n\t(x : i32)"), "fn f (x : i32)");
        assert_eq!(
            normalize("  leading and trailing  "),
            "leading and trailing"
        );
    }

    #[test]
    fn normalize_preserves_token_boundaries() {
        // Collapsing runs must never merge two distinct tokens into one.
        assert_ne!(normalize("mut x"), normalize("mutx"));
    }

    #[test]
    fn whitespace_only_differences_hash_equal() {
        // Same token boundaries, differing only in whitespace runs / tabs /
        // leading / trailing space. Cross-boundary spacing is the parser's job
        // (quote! canonicalizes it); see the parser tests.
        let a = sig_hash("f", "fn f (x : i32) -> i32");
        let b = sig_hash("f", "  fn\tf (x : i32)   ->  i32  ");
        assert_eq!(a, b);
    }

    #[test]
    fn changed_param_type_hashes_differently() {
        let a = sig_hash("f", "fn f (x : i32) -> i32");
        let b = sig_hash("f", "fn f (x : u64) -> i32");
        assert_ne!(a, b);
    }

    #[test]
    fn changed_return_type_hashes_differently() {
        let a = sig_hash("f", "fn f (x : i32) -> i32");
        let b = sig_hash("f", "fn f (x : i32) -> u64");
        assert_ne!(a, b);
    }

    #[test]
    fn hash_is_sixteen_hex_chars() {
        let h = sig_hash("f", "fn f ()");
        assert_eq!(h.len(), 16);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
