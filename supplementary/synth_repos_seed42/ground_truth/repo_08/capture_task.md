# Bug: release frees the wrong block after an empty reservation

After `reserve(&mut arena, 0)` followed by `reserve(&mut arena, 16)`,
releasing the second block removes the first one instead (they share the
offset).

Expected: an empty reservation must never corrupt the arena. Please fix
and add a regression test.
