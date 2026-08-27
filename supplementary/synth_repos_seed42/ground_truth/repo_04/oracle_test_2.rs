use ledgerly::hosts::{candidate_hosts_with_capacity, Host, Pool};

#[test]
fn capacity_candidates_are_never_empty() {
    let pool = Pool::new(
        Host::new("core", "eu", 8),
        vec![Host::new("eu-1", "eu", 1), Host::new("eu-2", "eu", 4)],
    );
    assert!(!candidate_hosts_with_capacity(&pool, "eu", 999).is_empty());
    assert!(!candidate_hosts_with_capacity(&pool, "mars", 1).is_empty());
}
