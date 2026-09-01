# Gotchas

- logistics::lead_time_days — MUST return >= 1; every caller casts the i64 result to unsigned with `as` and no guard (promise_board::promise_day uses `as u32`, the *_capacity_hint fns use `as usize`), so a 0 promises "today" and a negative wraps to a huge value. The trailing `days.max(1)` clamp enforces this — keep it as the last step of the function, after any priority arithmetic.
- logistics::lead_time_days — Priority::Express subtracts exactly the +2 handling days baked into `base`; if the handling constant shrinks or a faster tier subtracts more, the pre-clamp value goes negative and only `.max(1)` saves the unsigned casts downstream. Change formula and clamp together, never the formula alone.
- promise_board::promise_day — assumes lead_time_days >= 1 so promises always land strictly after `today`; do not "optimize away" the cast-safety by re-deriving days locally.
- audit_log::audit_log_capacity_hint / fleet_log::fleet_log_capacity_hint / heartbeat::heartbeat_capacity_hint — capacity is `lead_time_days(1200, Standard) as usize`; they inherit the >= 1 invariant, a non-positive lead time would wrap to a near-usize::MAX reserve hint.
