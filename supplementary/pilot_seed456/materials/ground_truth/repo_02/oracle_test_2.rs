use relaycore::logistics::{lead_time_days_with_credit, Priority};

#[test]
fn lead_time_with_credit_is_always_positive() {
    for km in [0, 100, 400, 2000] {
        for credit in [0, 1, 3, 10] {
            let days = lead_time_days_with_credit(km, Priority::Express, credit);
            assert!(days > 0, "{km} km, {credit} credit -> {days}");
        }
    }
}
