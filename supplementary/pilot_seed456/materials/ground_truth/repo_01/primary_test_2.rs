use quotaflow::config::parse_timeout;

#[test]
fn fractional_seconds_are_parsed() {
    assert_eq!(parse_timeout("1.5s"), 1500);
    assert_eq!(parse_timeout("2s"), 2000);
}
