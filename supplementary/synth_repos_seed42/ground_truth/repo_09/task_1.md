# Feature: minute and hour timeouts

Operators want to write timeouts as `5m` or `2h`.

Extend `timeout_from_str` in `src/config.rs` so that the suffix `m` means minutes and
`h` means hours (`5m` = 300000 ms, `2h` = 7200000 ms). Existing suffixes
keep working.
