use relaycore::logistics::{lead_time_days, Priority};

#[test]
fn lead_time_is_always_positive() {
    for km in [0, 50, 100, 399, 400, 800] {
        for priority in [Priority::Standard, Priority::Express, Priority::Overnight] {
            let days = lead_time_days(km, priority);
            assert!(days > 0, "{km} km {priority:?} -> {days}");
        }
    }
}
