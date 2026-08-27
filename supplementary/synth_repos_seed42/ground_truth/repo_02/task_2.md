# Feature: loyalty credit on lead time

Loyal customers earn credit days that shorten their promised lead time.

Add to `src/logistics.rs`:

- `pub fn lead_time_days_with_credit(distance_km: u32, priority: Priority, credit_days: u32) -> i64`
  returning the normal lead time minus the credit.

Example: a 2000 km standard route (7 days) with 2 credit days is promised
in 5 days.
