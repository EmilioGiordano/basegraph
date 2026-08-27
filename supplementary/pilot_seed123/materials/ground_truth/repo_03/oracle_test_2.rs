use beaconhub::matching::{affinity_score, Profile};

#[test]
fn scores_agree_in_both_directions() {
    let a = Profile::new("ana", &["rust", "chess", "jazz"], "lima");
    let b = Profile::new("bo", &["rust"], "lima");
    assert_eq!(affinity_score(&a, &b), affinity_score(&b, &a));
}
