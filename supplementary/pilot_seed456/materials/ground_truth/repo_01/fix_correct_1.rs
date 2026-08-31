//! Service configuration values and their parsing.

pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Parse a timeout such as `30s`, `500ms`, `5m` or `2h` into milliseconds.
pub fn parse_timeout(raw: &str) -> u64 {
    let value = raw.trim();
    let parsed = if let Some(ms) = value.strip_suffix("ms") {
        ms.trim().parse::<u64>().ok()
    } else if let Some(s) = value.strip_suffix('s') {
        s.trim().parse::<u64>().ok().map(|n| n.saturating_mul(1000))
    } else if let Some(m) = value.strip_suffix('m') {
        m.trim().parse::<u64>().ok().map(|n| n.saturating_mul(60_000))
    } else if let Some(h) = value.strip_suffix('h') {
        h.trim().parse::<u64>().ok().map(|n| n.saturating_mul(3_600_000))
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
    fn malformed_values_use_the_default() {
        assert_eq!(parse_timeout("3O s"), DEFAULT_TIMEOUT_MS);
        assert_eq!(parse_timeout("soon"), DEFAULT_TIMEOUT_MS);
    }
}
