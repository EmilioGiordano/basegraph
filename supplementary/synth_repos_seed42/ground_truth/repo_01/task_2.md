# Feature: fractional seconds

Allow fractional second timeouts such as `1.5s` (= 1500 ms) and `0.25s`
(= 250 ms) in `parse_timeout` (`src/config.rs`). Whole-second values keep working.
