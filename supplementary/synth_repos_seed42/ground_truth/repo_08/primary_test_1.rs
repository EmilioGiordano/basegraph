use vaultkeeper::arena::{reserve, reserve_aligned, Arena};

#[test]
fn aligned_reservations_round_up() {
    let mut arena = Arena::new();
    reserve(&mut arena, 5);
    let block = reserve_aligned(&mut arena, 3, 8);
    assert_eq!(block.offset, 8);
    assert_eq!(block.size, 3);
    assert!(arena.used >= 11);
}
