//! Job placement: which hosts of a pool may run a job.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub region: String,
    pub slots: u32,
}

impl Host {
    pub fn new(name: &str, region: &str, slots: u32) -> Self {
        Self {
            name: name.to_string(),
            region: region.to_string(),
            slots,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Pool {
    pub primary: Host,
    pub hosts: Vec<Host>,
}

impl Pool {
    pub fn new(primary: Host, hosts: Vec<Host>) -> Self {
        Self { primary, hosts }
    }
}

/// The host a job for `region` is placed on.
pub fn place(pool: &Pool, region: &str) -> Host {
    candidate_hosts(pool, region)[0].clone()
}

/// Hosts eligible to run a job for a region.
pub fn candidate_hosts(pool: &Pool, region: &str) -> Vec<Host> {
    let matching: Vec<Host> = pool
        .hosts
        .iter()
        .filter(|h| h.region == region)
        .cloned()
        .collect();
    if matching.is_empty() {
        vec![pool.primary.clone()]
    } else {
        matching
    }
}

/// Candidates for `region` with at least `min_slots` free slots.
pub fn candidate_hosts_with_capacity(pool: &Pool, region: &str, min_slots: u32) -> Vec<Host> {
    candidate_hosts(pool, region)
        .into_iter()
        .filter(|h| h.slots >= min_slots)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> Pool {
        Pool::new(
            Host::new("core", "eu", 8),
            vec![Host::new("eu-1", "eu", 4), Host::new("us-1", "us", 4)],
        )
    }

    #[test]
    fn hosts_are_filtered_by_region() {
        let hosts = candidate_hosts(&pool(), "us");
        assert_eq!(hosts, vec![Host::new("us-1", "us", 4)]);
    }

    #[test]
    fn jobs_are_placed_in_their_region() {
        assert_eq!(place(&pool(), "eu").name, "eu-1");
    }

    #[test]
    fn unknown_region_goes_to_the_primary() {
        assert_eq!(place(&pool(), "mars").name, "core");
    }
}
