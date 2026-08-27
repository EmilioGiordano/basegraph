use vaultkeeper::arena::{reserve_aligned, Arena};

#[test]
fn empty_aligned_reservations_are_rejected() {
    let outcome = std::panic::catch_unwind(|| {
        let mut arena = Arena::new();
        reserve_aligned(&mut arena, 0, 8)
    });
    assert!(outcome.is_err(), "an empty reservation must be rejected");
}
