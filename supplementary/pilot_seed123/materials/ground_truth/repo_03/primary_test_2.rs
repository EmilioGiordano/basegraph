use beaconhub::matching::{affinity_score, Profile};

#[test]
fn well_described_pairs_get_a_bonus() {
    let a = Profile::new("ana", &["rust", "chess", "jazz"], "lima");
    let b = Profile::new("bo", &["rust", "chess", "jazz"], "oslo");
    assert_eq!(affinity_score(&a, &b), 7);
    let c = Profile::new("cy", &["rust"], "lima");
    assert_eq!(affinity_score(&c, &b), 2);
}
