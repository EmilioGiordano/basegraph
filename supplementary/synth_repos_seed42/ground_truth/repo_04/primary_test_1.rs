use ledgerly::hosts::{candidate_hosts, Host, Pool};

#[test]
fn draining_hosts_are_skipped() {
    let mut drained = Host::new("eu-2", "eu", 4);
    drained.set_draining(true);
    assert!(drained.is_draining());
    let pool = Pool::new(
        Host::new("core", "eu", 8),
        vec![Host::new("eu-1", "eu", 4), drained.clone()],
    );
    let hosts = candidate_hosts(&pool, "eu");
    assert!(!hosts.contains(&drained), "{hosts:?}");
    assert_eq!(hosts.len(), 1);
}
