use metricsmith::config::timeout_from_str;

#[test]
fn fractional_seconds_are_parsed() {
    assert_eq!(timeout_from_str("1.5s"), 1500);
    assert_eq!(timeout_from_str("2s"), 2000);
}
