use ledgerly::hosts::{candidate_hosts, Host, Pool};

#[test]
fn candidate_list_is_never_empty() {
    let mut drained = Host::new("eu-1", "eu", 4);
    drained.set_draining(true);
    let pool = Pool::new(Host::new("core", "eu", 8), vec![drained]);
    assert!(!candidate_hosts(&pool, "eu").is_empty());
    assert!(!candidate_hosts(&pool, "mars").is_empty());
}
