# Bug: express orders under 400 km show as overdue immediately

Express orders on short routes get a lead time of 0 days, so the dispatch
board promises them for "today" and flags them overdue right away.

Expected: at least next-day delivery. Please fix and add a regression test.
