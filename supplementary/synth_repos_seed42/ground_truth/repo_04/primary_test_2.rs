use ledgerly::hosts::{candidate_hosts_with_capacity, Host, Pool};

#[test]
fn small_hosts_are_filtered_out() {
    let pool = Pool::new(
        Host::new("core", "eu", 8),
        vec![Host::new("eu-1", "eu", 1), Host::new("eu-2", "eu", 4)],
    );
    let hosts = candidate_hosts_with_capacity(&pool, "eu", 2);
    assert_eq!(hosts, vec![Host::new("eu-2", "eu", 4)]);
}
