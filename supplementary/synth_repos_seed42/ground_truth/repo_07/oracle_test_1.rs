use fleetops::matching::{affinity_score, Profile};

#[test]
fn scores_agree_in_both_directions() {
    let mut a = Profile::new("ana", &["rust", "chess"], "lima");
    let b = Profile::new("bo", &["rust"], "lima");
    a.block("bo");
    assert_eq!(affinity_score(&a, &b), affinity_score(&b, &a));
}
