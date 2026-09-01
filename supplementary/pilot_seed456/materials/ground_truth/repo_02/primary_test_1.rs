use relaycore::logistics::{lead_time_days, Priority};

#[test]
fn overnight_is_one_day_faster_than_express() {
    assert_eq!(lead_time_days(2000, Priority::Express), 5);
    assert_eq!(lead_time_days(2000, Priority::Overnight), 4);
    assert_eq!(lead_time_days(800, Priority::Overnight), lead_time_days(800, Priority::Express) - 1);
}
