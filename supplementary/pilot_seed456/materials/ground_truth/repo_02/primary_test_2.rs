use relaycore::logistics::{lead_time_days_with_credit, Priority};

#[test]
fn credit_days_shorten_the_promise() {
    assert_eq!(lead_time_days_with_credit(2000, Priority::Standard, 2), 5);
    assert_eq!(lead_time_days_with_credit(2000, Priority::Standard, 0), 7);
}
