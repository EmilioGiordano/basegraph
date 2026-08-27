# Feature: capacity-aware candidates

Large jobs need hosts with enough free slots.

Add to `src/hosts.rs`:

- `pub fn candidate_hosts_with_capacity(pool: &Pool, region: &str, min_slots: u32) -> Vec<Host>`
  returning the candidates for `region` that have at least `min_slots` slots.
