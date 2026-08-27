use vaultkeeper::arena::{regrow, reserve, Arena};

#[test]
fn regrowing_to_zero_is_rejected() {
    let outcome = std::panic::catch_unwind(|| {
        let mut arena = Arena::new();
        let a = reserve(&mut arena, 8);
        regrow(&mut arena, a, 0)
    });
    assert!(outcome.is_err(), "an empty reservation must be rejected");
}
