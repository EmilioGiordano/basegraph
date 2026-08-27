use metricsmith::config::timeout_from_str;

#[test]
fn minutes_and_hours_are_parsed() {
    assert_eq!(timeout_from_str("5m"), 300_000);
    assert_eq!(timeout_from_str("2h"), 7_200_000);
    assert_eq!(timeout_from_str("30s"), 30_000);
}
