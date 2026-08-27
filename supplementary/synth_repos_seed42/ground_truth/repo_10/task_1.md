# Feature: overnight priority

Add `Priority::Overnight` to `src/transit.rs`. An overnight shipment is
promised one day earlier than an express shipment on the same route.

Example: for a 2000 km route express is 5 days, so overnight is 4 days.
