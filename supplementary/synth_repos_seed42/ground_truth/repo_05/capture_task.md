# Bug: cache misses for paths with repeated slashes

Paths like `img///logo.png` produce the cache key `asset:img//logo.png`,
so the lookup (which normalises the path again) misses every time.

Expected: `asset:img/logo.png`. Please fix and add a regression test.
