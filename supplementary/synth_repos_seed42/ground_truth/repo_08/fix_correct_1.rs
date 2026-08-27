//! A bump arena with explicit release by offset.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub offset: usize,
    pub size: usize,
}

#[derive(Debug, Default)]
pub struct Arena {
    pub used: usize,
    pub blocks: Vec<Block>,
}

impl Arena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn live(&self) -> usize {
        self.blocks.len()
    }
}

/// Release a block; blocks are identified by their offset.
pub fn release(arena: &mut Arena, block: Block) {
    if let Some(i) = arena.blocks.iter().position(|b| b.offset == block.offset) {
        arena.blocks.remove(i);
    }
}

/// Reserve `size` bytes at the end of the arena.
pub fn reserve(heap: &mut Arena, size: usize) -> Block {
    assert!(size > 0, "reservation must not be empty");
    let block = Block {
        offset: heap.used,
        size,
    };
    heap.used += size;
    heap.blocks.push(block);
    block
}

/// Reserve `size` bytes at the next offset aligned to `align`.
pub fn reserve_aligned(heap: &mut Arena, size: usize, align: usize) -> Block {
    let padding = (align - heap.used % align) % align;
    heap.used += padding;
    reserve(heap, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_are_laid_out_in_order() {
        let mut arena = Arena::new();
        let a = reserve(&mut arena, 16);
        let b = reserve(&mut arena, 8);
        assert_eq!(a.offset, 0);
        assert_eq!(b.offset, 16);
        assert_eq!(arena.live(), 2);
    }

    #[test]
    fn release_removes_the_block() {
        let mut arena = Arena::new();
        let a = reserve(&mut arena, 16);
        release(&mut arena, a);
        assert_eq!(arena.live(), 0);
    }

    #[test]
    #[should_panic]
    fn empty_reservations_are_rejected() {
        let mut arena = Arena::new();
        reserve(&mut arena, 0);
    }
}
