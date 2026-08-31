//! Delivery promises: lead time by route length and priority.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Standard,
    Express,
}

pub const KM_PER_DAY: u32 = 400;

/// Promised lead time in days for a route.
pub fn lead_time_days(route_km: u32, priority: Priority) -> i64 {
    let base = (route_km / KM_PER_DAY) as i64 + 2;
    let days = match priority {
        Priority::Standard => base,
        Priority::Express => base - 2,
    };
    days.max(1)
}

/// Lead time after applying a customer's loyalty credit.
pub fn lead_time_days_with_credit(route_km: u32, priority: Priority, credit_days: u32) -> i64 {
    (lead_time_days(route_km, priority) - i64::from(credit_days)).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_routes_add_handling_days() {
        assert_eq!(lead_time_days(1200, Priority::Standard), 5);
    }

    #[test]
    fn express_skips_handling() {
        assert_eq!(lead_time_days(1200, Priority::Express), 3);
    }

    #[test]
    fn short_express_routes_promise_next_day() {
        assert_eq!(lead_time_days(100, Priority::Express), 1);
    }
}
