use beaconhub::paths::normalize_path;

#[test]
fn current_dir_segments_are_removed() {
    assert_eq!(normalize_path("a/./b"), "a/b");
    assert_eq!(normalize_path("./a"), "a");
    assert_eq!(normalize_path("a/b"), "a/b");
}
