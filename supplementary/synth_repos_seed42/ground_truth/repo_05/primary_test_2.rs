use beaconhub::paths::normalize_path;

#[test]
fn parent_segments_are_resolved() {
    assert_eq!(normalize_path("a/b/../c"), "a/c");
    assert_eq!(normalize_path("../a"), "a");
    assert_eq!(normalize_path("a/b"), "a/b");
}
