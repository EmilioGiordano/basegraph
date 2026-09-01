# Gotchas

- config::parse_timeout — MUST stay total (never panic, always return a usable u64): it runs on the service-start path (boot::load_timeout, vault::vault_capacity_hint, outbox/metrics_index capacity hints), so any panic here is a boot failure. Never reintroduce `.unwrap()` on the parse.
- config::parse_timeout — malformed input is silently mapped to DEFAULT_TIMEOUT_MS, not rejected; callers cannot distinguish an explicit "30s" from a typo, so config validation/typo reporting must happen upstream, never by inspecting this function's return value.
- config::DEFAULT_TIMEOUT_MS — doubles as the silent fallback for every malformed timeout string; it must remain a safe, nonzero value, and changing it changes runtime behavior for all mistyped configs, not just missing ones.
- config::parse_timeout — suffix order matters: `"ms"` MUST be checked before `'s'`, because `"500ms"` also ends in `s`; swapping the branches would parse it as 500 000 ms.
- config::parse_timeout — the seconds branch uses `saturating_mul(1000)` on purpose; a plain `* 1000` would panic (debug) or wrap (release) on huge values and break the totality invariant.
- boot::startup_budget_ms — sums parse_timeout over all probe entries; a malformed entry silently contributes DEFAULT_TIMEOUT_MS (30 000 ms) to the budget instead of failing, so an unexpectedly large budget may indicate a config typo.
