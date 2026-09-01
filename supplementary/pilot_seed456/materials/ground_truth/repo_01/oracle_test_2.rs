use quotaflow::config::parse_timeout;

#[test]
fn parsing_never_panics() {
    for input in ["1.s", ".5s", "1..5s", "1.x", "a.bs", "", "s", "1.5", "99999999999999999999s"] {
        let outcome = std::panic::catch_unwind(|| parse_timeout(input));
        assert!(outcome.is_ok(), "panicked on {input:?}");
    }
}
