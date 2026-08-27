//! Service configuration values and their parsing.

pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Timeout to use at boot, from the raw config value if present.
pub fn load_timeout(raw: Option<&str>) -> u64 {
    raw.map(timeout_from_str).unwrap_or(DEFAULT_TIMEOUT_MS)
}

/// Parse a timeout such as `30s`, `500ms`, `5m` or `2h` into milliseconds.
pub fn timeout_from_str(raw: &str) -> u64 {
    let value = raw.trim();
    if let Some(ms) = value.strip_suffix("ms") {
        return ms.trim().parse::<u64>().unwrap_or(DEFAULT_TIMEOUT_MS);
    }
    let (digits, unit) = value.split_at(value.len() - 1);
    let scale = match unit {
        "s" => 1000,
        "m" => 60_000,
        "h" => 3_600_000,
        _ => return value.parse::<u64>().unwrap_or(DEFAULT_TIMEOUT_MS),
    };
    digits
        .trim()
        .parse::<u64>()
        .map(|n| n.saturating_mul(scale))
        .unwrap_or(DEFAULT_TIMEOUT_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_and_milliseconds_are_parsed() {
        assert_eq!(timeout_from_str("30s"), 30_000);
        assert_eq!(timeout_from_str("500ms"), 500);
        assert_eq!(timeout_from_str("750"), 750);
    }

    #[test]
    fn missing_value_uses_the_default() {
        assert_eq!(load_timeout(None), DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn malformed_values_use_the_default() {
        assert_eq!(timeout_from_str("3O s"), DEFAULT_TIMEOUT_MS);
        assert_eq!(load_timeout(Some("soon")), DEFAULT_TIMEOUT_MS);
    }
}
