//! Service configuration values and their parsing.

pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Timeout to use at boot, from the raw config value if present.
pub fn load_timeout(raw: Option<&str>) -> u64 {
    raw.map(parse_timeout).unwrap_or(DEFAULT_TIMEOUT_MS)
}

/// Parse a timeout such as `30s`, `1.5s` or `500ms` into milliseconds.
pub fn parse_timeout(text: &str) -> u64 {
    let value = text.trim();
    let parsed = if let Some(ms) = value.strip_suffix("ms") {
        ms.trim().parse::<u64>().ok()
    } else if let Some(s) = value.strip_suffix('s') {
        s.trim()
            .parse::<f64>()
            .ok()
            .filter(|secs| secs.is_finite() && *secs >= 0.0)
            .map(|secs| (secs * 1000.0) as u64)
    } else {
        value.parse::<u64>().ok()
    };
    parsed.unwrap_or(DEFAULT_TIMEOUT_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_and_milliseconds_are_parsed() {
        assert_eq!(parse_timeout("30s"), 30_000);
        assert_eq!(parse_timeout("500ms"), 500);
        assert_eq!(parse_timeout("750"), 750);
    }

    #[test]
    fn missing_value_uses_the_default() {
        assert_eq!(load_timeout(None), DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn malformed_values_use_the_default() {
        assert_eq!(parse_timeout("3O s"), DEFAULT_TIMEOUT_MS);
        assert_eq!(load_timeout(Some("soon")), DEFAULT_TIMEOUT_MS);
    }
}
