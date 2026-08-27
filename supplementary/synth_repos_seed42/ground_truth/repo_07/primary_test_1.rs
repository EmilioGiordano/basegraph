use fleetops::matching::{affinity_score, Profile};

#[test]
fn blocking_zeroes_the_score() {
    let mut a = Profile::new("ana", &["rust"], "lima");
    let b = Profile::new("bo", &["rust"], "lima");
    assert!(affinity_score(&a, &b) > 0);
    a.block("bo");
    assert_eq!(affinity_score(&a, &b), 0);
}
