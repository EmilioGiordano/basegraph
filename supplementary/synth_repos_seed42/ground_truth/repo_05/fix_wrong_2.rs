//! Asset path canonicalisation used by the cache.

/// Cache key of an asset; the lookup side normalises the path again.
pub fn cache_key(path: &str) -> String {
    format!("asset:{}", normalize_path(path))
}

/// Canonical form of an asset path: single slashes, `..` resolved, no trailing slash.
pub fn normalize_path(path: &str) -> String {
    let mut collapsed = path.to_string();
    while collapsed.contains("//") {
        collapsed = collapsed.replace("//", "/");
    }
    let mut segments: Vec<&str> = collapsed.split('/').collect();
    if let Some(i) = segments.iter().position(|s| *s == "..") {
        segments.remove(i);
        if i > 0 {
            segments.remove(i - 1);
        }
    }
    let joined = segments.join("/");
    let trimmed = joined.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_slashes_collapse() {
        assert_eq!(normalize_path("img//logo.png"), "img/logo.png");
    }

    #[test]
    fn trailing_slash_is_dropped() {
        assert_eq!(normalize_path("img/"), "img");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(cache_key("css/"), "asset:css");
    }

    #[test]
    fn runs_of_slashes_collapse() {
        assert_eq!(normalize_path("img///logo.png"), "img/logo.png");
        assert_eq!(normalize_path("a////b"), "a/b");
    }
}
