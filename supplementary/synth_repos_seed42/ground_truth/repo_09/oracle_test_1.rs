use metricsmith::config::timeout_from_str;

#[test]
fn parsing_never_panics() {
    for input in ["", "m", "h", "s", "é", "10é", "５m", "-5m", " ", "ms", "1e3s"] {
        let outcome = std::panic::catch_unwind(|| timeout_from_str(input));
        assert!(outcome.is_ok(), "panicked on {input:?}");
    }
}
