use quotaflow::config::parse_timeout;

#[test]
fn minutes_and_hours_are_parsed() {
    assert_eq!(parse_timeout("5m"), 300_000);
    assert_eq!(parse_timeout("2h"), 7_200_000);
    assert_eq!(parse_timeout("30s"), 30_000);
}
