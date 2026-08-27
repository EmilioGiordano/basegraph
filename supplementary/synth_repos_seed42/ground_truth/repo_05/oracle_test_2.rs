use beaconhub::paths::normalize_path;

#[test]
fn normalising_twice_changes_nothing() {
    for input in ["a/b/../../c", "a/b/c/../../d", "x/../y/../z", "a/../../b"] {
        let once = normalize_path(input);
        let twice = normalize_path(&once);
        assert_eq!(once, twice, "not idempotent for {input:?}");
    }
}
