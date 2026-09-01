# Bug: service crashes on boot with a mistyped timeout

With `timeout = "3O s"` (a typo) in the config the service dies at startup
with `called Option::unwrap() on a None value`.

Expected: fall back to the default timeout and start. Please fix and add a
regression test.
