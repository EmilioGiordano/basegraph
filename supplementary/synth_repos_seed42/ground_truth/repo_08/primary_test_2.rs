use vaultkeeper::arena::{regrow, reserve, Arena};

#[test]
fn regrow_moves_the_block_to_the_end() {
    let mut arena = Arena::new();
    let a = reserve(&mut arena, 8);
    let b = reserve(&mut arena, 8);
    let bigger = regrow(&mut arena, a, 32);
    assert_eq!(bigger.size, 32);
    assert_eq!(bigger.offset, 16);
    assert!(!arena.blocks.contains(&a));
    assert!(arena.blocks.contains(&b));
}
