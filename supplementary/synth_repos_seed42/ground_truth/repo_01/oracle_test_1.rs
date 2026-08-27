use quotaflow::config::parse_timeout;

#[test]
fn parsing_never_panics() {
    for input in ["", "m", "h", "s", "é", "10é", "５m", "-5m", " ", "ms", "1e3s"] {
        let outcome = std::panic::catch_unwind(|| parse_timeout(input));
        assert!(outcome.is_ok(), "panicked on {input:?}");
    }
}
