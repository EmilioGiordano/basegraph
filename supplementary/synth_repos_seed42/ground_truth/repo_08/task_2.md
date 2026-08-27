# Feature: regrow a block

Buffers that outgrow their block need to be moved to a bigger one.

Add to `src/arena.rs`:

- `pub fn regrow(heap: &mut Arena, block: Block, new_size: usize) -> Block`
  which releases `block` and reserves `new_size` bytes at the end of the
  arena, returning the new block.
