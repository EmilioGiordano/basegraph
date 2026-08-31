//! Service configuration values and their parsing.

pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Parse a timeout such as `30s`, `500ms`, `5m` or `2h` into milliseconds.
pub fn parse_timeout(raw: &str) -> u64 {
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
