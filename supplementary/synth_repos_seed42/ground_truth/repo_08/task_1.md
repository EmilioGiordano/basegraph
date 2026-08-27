# Feature: aligned reservations

SIMD buffers need aligned offsets.

Add to `src/arena.rs`:

- `pub fn reserve_aligned(heap: &mut Arena, size: usize, align: usize) -> Block`
  which reserves `size` bytes at the next offset that is a multiple of
  `align` (a power of two), skipping the padding.

Example: after a 5-byte reservation, `reserve_aligned(&mut arena, 3, 8)`
returns a block at offset 8.
