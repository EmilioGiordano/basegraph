# Feature: draining hosts

Hosts can be put in a draining state before maintenance; draining hosts
must not receive new jobs.

Required API (in `src/hosts.rs`):

- `Host::set_draining(&mut self, on: bool)` and `Host::is_draining(&self) -> bool`
  (hosts start not draining).
- `candidate_hosts` must not return draining hosts.
