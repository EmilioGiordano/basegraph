# Gotchas

## Invoice numbering (`src/billing.rs`)

- `billing::NEXT_INVOICE` is a process-wide `AtomicU64` sequence. Every call to
  `billing::issue_number()` permanently consumes a number; there is no way to
  give one back, so a stray call shows up as a gap in the ledger.
- Only `Invoice::issue()` may call `issue_number()`. Do **not** call it from
  rendering, preview, logging, length/capacity calculations, or anything else
  that can run more than once per invoice.
- `Invoice::draft()` yields `number == 0`. `number == 0` means "unissued" —
  treat it as a sentinel, never as a real invoice number.
- `billing::render_invoice()` is read-only. A draft renders as `INVOICE DRAFT`
  (no number); an issued invoice renders as `INVOICE #<n>`. This used to call
  `issue_number()` for drafts, which made previews unstable and burned numbers
  that never reached the ledger (regression test:
  `draft_preview_is_stable_and_does_not_allocate_a_number`).
- `session_index_capacity_hint()`, `vault_cache_capacity_hint()` and
  `vault_queue_capacity_hint()` call `render_invoice(&Invoice::draft(..))`
  purely to measure the rendered length. They rely on rendering having no side
  effects — if `render_invoice` ever mutates global state again, these three
  helpers will silently consume invoice numbers on every call.
- Tests run in parallel and share `NEXT_INVOICE`. Never assert an exact value
  of the sequence (or of an issued number) in a test; assert on the number the
  `Invoice` actually carries, or on draft output, as the existing tests do.
