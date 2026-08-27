# Bug: schedule contains overlapping windows

Maintenance requests submitted out of order produce a schedule with
overlapping windows.

Repro: merging `[(5,7), (1,2), (2,6)]` returns `[(5,7), (1,6)]`;
expected a single window `[(1,7)]`.

Please fix and add a regression test.
