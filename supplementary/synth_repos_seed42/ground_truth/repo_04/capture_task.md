# Bug: scheduler crashes for regions without hosts

Submitting a job for a region that has no hosts crashes the scheduler with
`index out of bounds: the len is 0`.

Expected: the job is placed on the pool's primary host. Please fix and add
a regression test.
