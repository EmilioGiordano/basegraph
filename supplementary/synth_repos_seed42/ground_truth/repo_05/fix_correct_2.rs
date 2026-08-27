//! Asset path canonicalisation used by the cache.

/// Cache key of an asset; the lookup side normalises the path again.
pub fn cache_key(path: &str) -> String {
    format!("asset:{}", normalize_path(path))
}

/// Canonical form of an asset path: single slashes, `..` resolved, no trailing slash.
pub fn normalize_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    let joined = segments.join("/");
    if joined.is_empty() {
        "/".to_string()
    } else if absolute {
        format!("/{joined}")
    } else {
        joined
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
