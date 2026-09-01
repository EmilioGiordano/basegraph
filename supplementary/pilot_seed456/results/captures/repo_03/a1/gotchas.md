# Gotchas

- billing::Invoice::number — 0 is the reserved "draft/unissued" sentinel; NEVER assign 0 to an issued invoice or the renderer will show it as DRAFT.
- billing::NEXT_INVOICE — must stay >= 1 (starts at 1000, only increments); never reset it to 0 or let it wrap, or issue_number() could hand out the draft sentinel.
- billing::render_invoice — must stay pure/read-only; it must NEVER call issue_number() (previewing a draft would consume a real number from the global sequence each call).
- billing::Invoice::issue — the ONLY sanctioned way to give an invoice a number; constructing an Invoice with a hand-picked number bypasses the 0-means-draft contract (the field is pub, nothing enforces it).
