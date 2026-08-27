# Gotchas

- billing::render_invoice — MUST be pure/idempotent; NEVER call issue_number() here. Rendering a draft (number == 0) prints "INVOICE DRAFT", it does not assign a number.
- billing::issue_number — every call irreversibly consumes a number from the global NEXT_INVOICE sequence; only call it from Invoice::issue (write path), never from read/preview/log paths.
- billing::Invoice::number — 0 is the reserved "unissued draft" sentinel; NEXT_INVOICE starts at 1000 so real invoices never get 0. Do not lower the starting value or construct an Invoice with number 0 that is meant to be issued.
- billing::Invoice::issue — the ONLY place a draft becomes numbered; it consumes self so a draft cannot be issued twice.
