//! The invariant catalog (goal §1): one scenario per invariant type, each a
//! parametrized template of the anchored module across C1 (bug), C2 (fix that
//! leaves the invariant latent), a cosmetic C3 variant, and two tasks with
//! pre-written ground truth (description, primary test, oracle, correct and
//! "obvious wrong" fix).
//!
//! Placeholders: `@FN@` anchored function, `@ARG@` its first parameter,
//! `@MOD@` its module, `@CRATE@` the crate. Fixes replace the whole module.

/// One reference implementation of a task: what the module looks like after
/// the fix. `None` keeps the C3 version of that part.
#[derive(Debug, Clone, Copy)]
pub struct Variant {
    pub types: Option<&'static str>,
    pub anchor_fn: Option<&'static str>,
    pub extras: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct TaskTemplate {
    pub title: &'static str,
    pub description: &'static str,
    pub primary_test: &'static str,
    pub oracle_test: &'static str,
    pub correct: Variant,
    pub wrong: Variant,
}

#[derive(Debug, Clone, Copy)]
pub struct Scenario {
    pub id: &'static str,
    pub invariant_type: &'static str,
    pub invariant_text: &'static str,
    pub module: &'static str,
    pub module_moved: &'static str,
    pub fn_name: &'static str,
    pub fn_renamed: &'static str,
    pub arg: &'static str,
    pub arg_renamed: &'static str,
    /// Items filler modules import from the anchor module besides `@FN@`.
    pub types_import: &'static str,
    /// Module that DEPENDS on the invariant, in a separate file from the
    /// provider so the reason for the rule is never next to the anchored fn.
    pub consumer_module: &'static str,
    /// Full source of the consumer module (placeholders allowed). Its tests
    /// must pass at C1 too (they exercise well-formed inputs, they do not
    /// assert the invariant).
    pub consumer: &'static str,
    /// A `usize` expression that exercises `@FN@` from filler modules.
    pub filler_call: &'static str,
    pub module_doc: &'static str,
    pub types: &'static str,
    pub fn_c1: &'static str,
    pub fn_c2: &'static str,
    pub fn_c2_variant: &'static str,
    pub tests_base: &'static str,
    pub test_regression: &'static str,
    pub commit_c1: &'static str,
    pub commit_c2: &'static str,
    pub commit_c3_drift: &'static str,
    pub commit_c3_body: &'static str,
    pub capture_task: &'static str,
    pub tasks: [TaskTemplate; 2],
}

pub fn all() -> Vec<Scenario> {
    vec![
        sorted_output(),
        return_positivity(),
        non_empty(),
        no_panic(),
        idempotence(),
        precondition(),
        no_side_effect(),
        commutativity(),
    ]
}

pub fn by_id(id: &str) -> Option<Scenario> {
    all().into_iter().find(|s| s.id == id)
}

fn sorted_output() -> Scenario {
    Scenario {
        id: "sorted_output",
        invariant_type: "sorted_output",
        invariant_text: "`@FN@` returns the windows sorted by start (and non-overlapping); `first_gap` and the dispatch loop rely on that order.",
        module: "scheduling",
        module_moved: "windows",
        fn_name: "merge_windows",
        fn_renamed: "coalesce_windows",
        arg: "windows",
        arg_renamed: "requested",
        types_import: "Window",
        consumer_module: "dispatching",
        consumer: r##"//! Dispatch planning over the merged maintenance schedule.

use crate::@MOD@::{@FN@, Window};

/// First free slot between scheduled windows.
pub fn first_gap(schedule: &[Window]) -> Option<u32> {
    schedule
        .windows(2)
        .find(|pair| pair[1].start > pair[0].end + 1)
        .map(|pair| pair[0].end + 1)
}

/// Merge the requested windows and report where the next job can start.
pub fn next_free_slot(requested: &[Window]) -> Option<u32> {
    first_gap(&@FN@(requested))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_are_found_between_windows() {
        let schedule = [Window::new(1, 2), Window::new(4, 6)];
        assert_eq!(first_gap(&schedule), Some(3));
        assert_eq!(first_gap(&[Window::new(1, 2)]), None);
    }

    #[test]
    fn next_free_slot_merges_first() {
        assert_eq!(next_free_slot(&[Window::new(1, 2), Window::new(4, 6)]), Some(3));
    }
}
"##,
        filler_call: "@FN@(&[Window::new(1, 2)]).len()",
        module_doc: "Maintenance windows and the merge step that turns requests into a schedule.",
        types: r##"#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: u32,
    pub end: u32,
}

impl Window {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn overlaps(&self, other: &Window) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}
"##,
        fn_c1: r##"/// Merge overlapping maintenance windows into a compact schedule.
pub fn @FN@(@ARG@: &[Window]) -> Vec<Window> {
    let mut merged: Vec<Window> = Vec::new();
    for w in @ARG@ {
        match merged.last_mut() {
            Some(last) if last.overlaps(w) => {
                last.start = last.start.min(w.start);
                last.end = last.end.max(w.end);
            }
            _ => merged.push(*w),
        }
    }
    merged
}
"##,
        fn_c2: r##"/// Merge overlapping maintenance windows into a compact schedule.
pub fn @FN@(@ARG@: &[Window]) -> Vec<Window> {
    let mut ordered: Vec<Window> = @ARG@.to_vec();
    ordered.sort_by_key(|w| (w.start, w.end));
    let mut merged: Vec<Window> = Vec::new();
    for w in ordered {
        match merged.last_mut() {
            Some(last) if last.overlaps(&w) => {
                last.end = last.end.max(w.end);
            }
            _ => merged.push(w),
        }
    }
    merged
}
"##,
        fn_c2_variant: r##"/// Merge overlapping maintenance windows into a compact schedule.
pub fn @FN@(@ARG@: &[Window]) -> Vec<Window> {
    let mut ordered = @ARG@.to_vec();
    ordered.sort_unstable_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
    ordered.into_iter().fold(Vec::new(), |mut merged: Vec<Window>, w| {
        if let Some(last) = merged.last_mut().filter(|last| last.overlaps(&w)) {
            last.end = last.end.max(w.end);
        } else {
            merged.push(w);
        }
        merged
    })
}
"##,
        tests_base: r##"    #[test]
    fn adjacent_windows_merge() {
        let merged = @FN@(&[Window::new(1, 3), Window::new(2, 5)]);
        assert_eq!(merged, vec![Window::new(1, 5)]);
    }

    #[test]
    fn disjoint_windows_stay_apart() {
        let merged = @FN@(&[Window::new(1, 2), Window::new(4, 6)]);
        assert_eq!(merged.len(), 2);
    }
"##,
        test_regression: r##"    #[test]
    fn unordered_requests_still_merge() {
        let merged = @FN@(&[Window::new(5, 7), Window::new(1, 2), Window::new(2, 6)]);
        assert_eq!(merged, vec![Window::new(1, 7)]);
    }
"##,
        commit_c1: "Initial scheduling service",
        commit_c2: "fix: overlapping windows survive the merge when requests arrive out of order",
        commit_c3_drift: "refactor: tidy the scheduling module",
        commit_c3_body: "refactor: express the window merge as a fold",
        capture_task: r##"# Bug: schedule contains overlapping windows

Maintenance requests submitted out of order produce a schedule with
overlapping windows.

Repro: merging `[(5,7), (1,2), (2,6)]` returns `[(5,7), (1,6)]`;
expected a single window `[(1,7)]`.

Please fix and add a regression test.
"##,
        tasks: [
            TaskTemplate {
                title: "Pinned windows",
                description: r##"# Feature: pinned maintenance windows

Operators need to pin a window so it is scheduled exactly as requested.

Required API (in `src/@MOD@.rs`):

- `Window::pinned(start: u32, end: u32) -> Window` creates a pinned window.
- `Window::is_pinned(&self) -> bool`.
- `@FN@` must return every pinned window exactly as given (never merged
  with a neighbour), while regular windows keep being merged among
  themselves as today. Pinned windows do not absorb regular ones.

Existing behaviour for regular windows must not change.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{@FN@, Window};

#[test]
fn pinned_windows_are_kept_verbatim() {
    let out = @FN@(&[Window::new(1, 4), Window::pinned(2, 3), Window::new(3, 6)]);
    assert!(out.contains(&Window::pinned(2, 3)), "{out:?}");
    assert!(out.contains(&Window::new(1, 6)), "{out:?}");
    assert_eq!(out.len(), 2);
    assert!(Window::pinned(2, 3).is_pinned());
    assert!(!Window::new(2, 3).is_pinned());
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{@FN@, Window};

#[test]
fn schedule_stays_sorted_with_pinned_windows() {
    let out = @FN@(&[
        Window::new(10, 12),
        Window::pinned(2, 3),
        Window::new(14, 15),
        Window::pinned(1, 1),
    ]);
    assert!(
        out.windows(2).all(|p| p[0].start <= p[1].start),
        "schedule is not sorted by start: {out:?}"
    );
}
"##,
                correct: Variant {
                    types: Some(PINNED_TYPES),
                    anchor_fn: Some(r##"/// Merge overlapping maintenance windows into a compact schedule.
pub fn @FN@(@ARG@: &[Window]) -> Vec<Window> {
    let (pinned, regular): (Vec<Window>, Vec<Window>) =
        @ARG@.iter().copied().partition(|w| w.is_pinned());
    let mut ordered = regular;
    ordered.sort_by_key(|w| (w.start, w.end));
    let mut merged: Vec<Window> = Vec::new();
    for w in ordered {
        match merged.last_mut() {
            Some(last) if last.overlaps(&w) => {
                last.end = last.end.max(w.end);
            }
            _ => merged.push(w),
        }
    }
    merged.extend(pinned);
    merged.sort_by_key(|w| (w.start, w.end));
    merged
}
"##),
                    extras: "",
                },
                wrong: Variant {
                    types: Some(PINNED_TYPES),
                    anchor_fn: Some(r##"/// Merge overlapping maintenance windows into a compact schedule.
pub fn @FN@(@ARG@: &[Window]) -> Vec<Window> {
    let (pinned, regular): (Vec<Window>, Vec<Window>) =
        @ARG@.iter().copied().partition(|w| w.is_pinned());
    let mut ordered = regular;
    ordered.sort_by_key(|w| (w.start, w.end));
    let mut merged: Vec<Window> = Vec::new();
    for w in ordered {
        match merged.last_mut() {
            Some(last) if last.overlaps(&w) => {
                last.end = last.end.max(w.end);
            }
            _ => merged.push(w),
        }
    }
    merged.extend(pinned);
    merged
}
"##),
                    extras: "",
                },
            },
            TaskTemplate {
                title: "Inverted windows",
                description: r##"# Bug: inverted windows from the legacy importer

The legacy importer sometimes emits windows with `end < start`. Today they
are passed through as-is and end up as nonsense entries in the schedule.

Required behaviour (in `src/@MOD@.rs`):

- Add `Window::normalised(&self) -> Window` returning the window with its
  bounds swapped when `end < start` (unchanged otherwise).
- `@FN@` must treat an inverted window as its normalised form, i.e.
  `@FN@(&[Window::new(6, 2)])` yields `[Window::new(2, 6)]`, and inverted
  windows merge with the windows they overlap after normalisation.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{@FN@, Window};

#[test]
fn inverted_windows_are_normalised() {
    assert_eq!(Window::new(6, 2).normalised(), Window::new(2, 6));
    assert_eq!(Window::new(2, 6).normalised(), Window::new(2, 6));
    assert_eq!(@FN@(&[Window::new(6, 2)]), vec![Window::new(2, 6)]);
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{@FN@, Window};

#[test]
fn schedule_stays_sorted_with_inverted_windows() {
    let out = @FN@(&[Window::new(3, 4), Window::new(9, 1), Window::new(12, 12)]);
    assert!(
        out.windows(2).all(|p| p[0].start <= p[1].start),
        "schedule is not sorted by start: {out:?}"
    );
}
"##,
                correct: Variant {
                    types: Some(NORMALISED_TYPES),
                    anchor_fn: Some(r##"/// Merge overlapping maintenance windows into a compact schedule.
pub fn @FN@(@ARG@: &[Window]) -> Vec<Window> {
    let mut ordered: Vec<Window> = @ARG@.iter().map(Window::normalised).collect();
    ordered.sort_by_key(|w| (w.start, w.end));
    let mut merged: Vec<Window> = Vec::new();
    for w in ordered {
        match merged.last_mut() {
            Some(last) if last.overlaps(&w) => {
                last.end = last.end.max(w.end);
            }
            _ => merged.push(w),
        }
    }
    merged
}
"##),
                    extras: "",
                },
                wrong: Variant {
                    types: Some(NORMALISED_TYPES),
                    anchor_fn: Some(r##"/// Merge overlapping maintenance windows into a compact schedule.
pub fn @FN@(@ARG@: &[Window]) -> Vec<Window> {
    let mut ordered: Vec<Window> = @ARG@.to_vec();
    ordered.sort_by_key(|w| (w.start, w.end));
    let mut merged: Vec<Window> = Vec::new();
    for w in ordered {
        match merged.last_mut() {
            Some(last) if last.overlaps(&w) => {
                last.end = last.end.max(w.end);
            }
            _ => merged.push(w),
        }
    }
    merged.iter().map(Window::normalised).collect()
}
"##),
                    extras: "",
                },
            },
        ],
    }
}

const PINNED_TYPES: &str = r##"#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: u32,
    pub end: u32,
    pinned: bool,
}

impl Window {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end, pinned: false }
    }

    pub fn pinned(start: u32, end: u32) -> Self {
        Self { start, end, pinned: true }
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    pub fn overlaps(&self, other: &Window) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}
"##;

const NORMALISED_TYPES: &str = r##"#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: u32,
    pub end: u32,
}

impl Window {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn normalised(&self) -> Window {
        if self.end < self.start {
            Window::new(self.end, self.start)
        } else {
            *self
        }
    }

    pub fn overlaps(&self, other: &Window) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}
"##;

fn return_positivity() -> Scenario {
    Scenario {
        id: "return_positivity",
        invariant_type: "return_positivity",
        invariant_text: "`@FN@` always returns at least 1 day (a positive number); `promise_day` adds it to the calendar as an unsigned offset.",
        module: "logistics",
        module_moved: "transit",
        fn_name: "lead_time_days",
        fn_renamed: "delivery_lead_days",
        arg: "distance_km",
        arg_renamed: "route_km",
        types_import: "Priority",
        consumer_module: "promise_board",
        consumer: r##"//! The delivery promise board shown to customers.

use crate::@MOD@::{@FN@, Priority};

/// Calendar day on which a shipment is promised.
pub fn promise_day(today: u32, distance_km: u32, priority: Priority) -> u32 {
    today + @FN@(distance_km, priority) as u32
}

/// The soonest promise among candidate warehouses.
pub fn earliest_promise(today: u32, distances_km: &[u32], priority: Priority) -> Option<u32> {
    distances_km
        .iter()
        .map(|d| promise_day(today, *d, priority))
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promises_land_after_today() {
        assert_eq!(promise_day(10, 1200, Priority::Standard), 15);
    }

    #[test]
    fn the_closest_warehouse_wins() {
        assert_eq!(
            earliest_promise(10, &[2000, 800, 1200], Priority::Standard),
            Some(14)
        );
    }
}
"##,
        filler_call: "@FN@(1200, Priority::Standard) as usize",
        module_doc: "Delivery promises: lead time by route length and priority.",
        types: r##"#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Standard,
    Express,
}

pub const KM_PER_DAY: u32 = 400;
"##,
        fn_c1: r##"/// Promised lead time in days for a route.
pub fn @FN@(@ARG@: u32, priority: Priority) -> i64 {
    let base = (@ARG@ / KM_PER_DAY) as i64 + 2;
    match priority {
        Priority::Standard => base,
        Priority::Express => base - 2,
    }
}
"##,
        fn_c2: r##"/// Promised lead time in days for a route.
pub fn @FN@(@ARG@: u32, priority: Priority) -> i64 {
    let base = (@ARG@ / KM_PER_DAY) as i64 + 2;
    let days = match priority {
        Priority::Standard => base,
        Priority::Express => base - 2,
    };
    days.max(1)
}
"##,
        fn_c2_variant: r##"/// Promised lead time in days for a route.
pub fn @FN@(@ARG@: u32, priority: Priority) -> i64 {
    let transit = i64::from(@ARG@ / KM_PER_DAY);
    let handling = match priority {
        Priority::Standard => 2,
        Priority::Express => 0,
    };
    (transit + handling).max(1)
}
"##,
        tests_base: r##"    #[test]
    fn standard_routes_add_handling_days() {
        assert_eq!(@FN@(1200, Priority::Standard), 5);
    }

    #[test]
    fn express_skips_handling() {
        assert_eq!(@FN@(1200, Priority::Express), 3);
    }
"##,
        test_regression: r##"    #[test]
    fn short_express_routes_promise_next_day() {
        assert_eq!(@FN@(100, Priority::Express), 1);
    }
"##,
        commit_c1: "Initial logistics service",
        commit_c2: "fix: express deliveries on short routes are promised for today",
        commit_c3_drift: "refactor: tidy the logistics module",
        commit_c3_body: "refactor: split transit and handling days",
        capture_task: r##"# Bug: express orders under 400 km show as overdue immediately

Express orders on short routes get a lead time of 0 days, so the dispatch
board promises them for "today" and flags them overdue right away.

Expected: at least next-day delivery. Please fix and add a regression test.
"##,
        tasks: [
            TaskTemplate {
                title: "Overnight priority",
                description: r##"# Feature: overnight priority

Add `Priority::Overnight` to `src/@MOD@.rs`. An overnight shipment is
promised one day earlier than an express shipment on the same route.

Example: for a 2000 km route express is 5 days, so overnight is 4 days.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{@FN@, Priority};

#[test]
fn overnight_is_one_day_faster_than_express() {
    assert_eq!(@FN@(2000, Priority::Express), 5);
    assert_eq!(@FN@(2000, Priority::Overnight), 4);
    assert_eq!(@FN@(800, Priority::Overnight), @FN@(800, Priority::Express) - 1);
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{@FN@, Priority};

#[test]
fn lead_time_is_always_positive() {
    for km in [0, 50, 100, 399, 400, 800] {
        for priority in [Priority::Standard, Priority::Express, Priority::Overnight] {
            let days = @FN@(km, priority);
            assert!(days > 0, "{km} km {priority:?} -> {days}");
        }
    }
}
"##,
                correct: Variant {
                    types: Some(OVERNIGHT_TYPES),
                    anchor_fn: Some(r##"/// Promised lead time in days for a route.
pub fn @FN@(@ARG@: u32, priority: Priority) -> i64 {
    let base = (@ARG@ / KM_PER_DAY) as i64 + 2;
    let days = match priority {
        Priority::Standard => base,
        Priority::Express => base - 2,
        Priority::Overnight => base - 3,
    };
    days.max(1)
}
"##),
                    extras: "",
                },
                wrong: Variant {
                    types: Some(OVERNIGHT_TYPES),
                    anchor_fn: Some(r##"/// Promised lead time in days for a route.
pub fn @FN@(@ARG@: u32, priority: Priority) -> i64 {
    if priority == Priority::Overnight {
        return @FN@(@ARG@, Priority::Express) - 1;
    }
    let base = (@ARG@ / KM_PER_DAY) as i64 + 2;
    let days = match priority {
        Priority::Standard => base,
        Priority::Express => base - 2,
        Priority::Overnight => unreachable!(),
    };
    days.max(1)
}
"##),
                    extras: "",
                },
            },
            TaskTemplate {
                title: "Loyalty credit",
                description: r##"# Feature: loyalty credit on lead time

Loyal customers earn credit days that shorten their promised lead time.

Add to `src/@MOD@.rs`:

- `pub fn @FN@_with_credit(@ARG@: u32, priority: Priority, credit_days: u32) -> i64`
  returning the normal lead time minus the credit.

Example: a 2000 km standard route (7 days) with 2 credit days is promised
in 5 days.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{@FN@_with_credit, Priority};

#[test]
fn credit_days_shorten_the_promise() {
    assert_eq!(@FN@_with_credit(2000, Priority::Standard, 2), 5);
    assert_eq!(@FN@_with_credit(2000, Priority::Standard, 0), 7);
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{@FN@_with_credit, Priority};

#[test]
fn lead_time_with_credit_is_always_positive() {
    for km in [0, 100, 400, 2000] {
        for credit in [0, 1, 3, 10] {
            let days = @FN@_with_credit(km, Priority::Express, credit);
            assert!(days > 0, "{km} km, {credit} credit -> {days}");
        }
    }
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Lead time after applying a customer's loyalty credit.
pub fn @FN@_with_credit(@ARG@: u32, priority: Priority, credit_days: u32) -> i64 {
    (@FN@(@ARG@, priority) - i64::from(credit_days)).max(1)
}
"##,
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Lead time after applying a customer's loyalty credit.
pub fn @FN@_with_credit(@ARG@: u32, priority: Priority, credit_days: u32) -> i64 {
    @FN@(@ARG@, priority) - i64::from(credit_days)
}
"##,
                },
            },
        ],
    }
}

const OVERNIGHT_TYPES: &str = r##"#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Standard,
    Express,
    Overnight,
}

pub const KM_PER_DAY: u32 = 400;
"##;

fn non_empty() -> Scenario {
    Scenario {
        id: "non_empty",
        invariant_type: "non_empty",
        invariant_text: "`@FN@` never returns an empty list (it falls back to the pool's primary host); `place` indexes the first element.",
        module: "placement",
        module_moved: "hosts",
        fn_name: "candidate_hosts",
        fn_renamed: "eligible_hosts",
        arg: "pool",
        arg_renamed: "cluster",
        types_import: "Host, Pool",
        consumer_module: "job_router",
        consumer: r##"//! Routing: pick the concrete host a job lands on.

use crate::@MOD@::{@FN@, Host, Pool};

/// The host a job for `region` is placed on.
pub fn place(pool: &Pool, region: &str) -> Host {
    @FN@(pool, region)[0].clone()
}

/// Place one job per region, in order.
pub fn spread(pool: &Pool, regions: &[&str]) -> Vec<Host> {
    regions.iter().map(|region| place(pool, region)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> Pool {
        Pool::new(
            Host::new("core", "eu", 8),
            vec![Host::new("eu-1", "eu", 4), Host::new("us-1", "us", 4)],
        )
    }

    #[test]
    fn jobs_are_placed_in_their_region() {
        assert_eq!(place(&pool(), "eu").name, "eu-1");
        assert_eq!(place(&pool(), "us").name, "us-1");
    }

    #[test]
    fn spread_visits_every_region() {
        let hosts = spread(&pool(), &["eu", "us"]);
        assert_eq!(hosts.len(), 2);
    }
}
"##,
        filler_call: "@FN@(&Pool::new(Host::new(\"p\", \"eu\", 1), Vec::new()), \"eu\").len()",
        module_doc: "Job placement: which hosts of a pool may run a job.",
        types: r##"#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub region: String,
    pub slots: u32,
}

impl Host {
    pub fn new(name: &str, region: &str, slots: u32) -> Self {
        Self {
            name: name.to_string(),
            region: region.to_string(),
            slots,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Pool {
    pub primary: Host,
    pub hosts: Vec<Host>,
}

impl Pool {
    pub fn new(primary: Host, hosts: Vec<Host>) -> Self {
        Self { primary, hosts }
    }
}
"##,
        fn_c1: r##"/// Hosts eligible to run a job for a region.
pub fn @FN@(@ARG@: &Pool, region: &str) -> Vec<Host> {
    @ARG@
        .hosts
        .iter()
        .filter(|h| h.region == region)
        .cloned()
        .collect()
}
"##,
        fn_c2: r##"/// Hosts eligible to run a job for a region.
pub fn @FN@(@ARG@: &Pool, region: &str) -> Vec<Host> {
    let matching: Vec<Host> = @ARG@
        .hosts
        .iter()
        .filter(|h| h.region == region)
        .cloned()
        .collect();
    if matching.is_empty() {
        vec![@ARG@.primary.clone()]
    } else {
        matching
    }
}
"##,
        fn_c2_variant: r##"/// Hosts eligible to run a job for a region.
pub fn @FN@(@ARG@: &Pool, region: &str) -> Vec<Host> {
    let mut matching: Vec<Host> = Vec::new();
    for host in &@ARG@.hosts {
        if host.region == region {
            matching.push(host.clone());
        }
    }
    if matching.is_empty() {
        matching.push(@ARG@.primary.clone());
    }
    matching
}
"##,
        tests_base: r##"    fn pool() -> Pool {
        Pool::new(
            Host::new("core", "eu", 8),
            vec![Host::new("eu-1", "eu", 4), Host::new("us-1", "us", 4)],
        )
    }

    #[test]
    fn hosts_are_filtered_by_region() {
        let hosts = @FN@(&pool(), "us");
        assert_eq!(hosts, vec![Host::new("us-1", "us", 4)]);
        assert_eq!(@FN@(&pool(), "eu").len(), 1);
    }
"##,
        test_regression: r##"    #[test]
    fn unknown_region_falls_back_to_the_primary() {
        let hosts = @FN@(&pool(), "mars");
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].name, "core");
    }
"##,
        commit_c1: "Initial placement service",
        commit_c2: "fix: placement panics when no host serves the requested region",
        commit_c3_drift: "refactor: tidy the placement module",
        commit_c3_body: "refactor: build the candidate list imperatively",
        capture_task: r##"# Bug: scheduler crashes for regions without hosts

Submitting a job for a region that has no hosts crashes the scheduler with
`index out of bounds: the len is 0`.

Expected: the job is placed on the pool's primary host. Please fix and add
a regression test.
"##,
        tasks: [
            TaskTemplate {
                title: "Draining hosts",
                description: r##"# Feature: draining hosts

Hosts can be put in a draining state before maintenance; draining hosts
must not receive new jobs.

Required API (in `src/@MOD@.rs`):

- `Host::set_draining(&mut self, on: bool)` and `Host::is_draining(&self) -> bool`
  (hosts start not draining).
- `@FN@` must not return draining hosts.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{@FN@, Host, Pool};

#[test]
fn draining_hosts_are_skipped() {
    let mut drained = Host::new("eu-2", "eu", 4);
    drained.set_draining(true);
    assert!(drained.is_draining());
    let pool = Pool::new(
        Host::new("core", "eu", 8),
        vec![Host::new("eu-1", "eu", 4), drained.clone()],
    );
    let hosts = @FN@(&pool, "eu");
    assert!(!hosts.contains(&drained), "{hosts:?}");
    assert_eq!(hosts.len(), 1);
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{@FN@, Host, Pool};

#[test]
fn candidate_list_is_never_empty() {
    let mut drained = Host::new("eu-1", "eu", 4);
    drained.set_draining(true);
    let pool = Pool::new(Host::new("core", "eu", 8), vec![drained]);
    assert!(!@FN@(&pool, "eu").is_empty());
    assert!(!@FN@(&pool, "mars").is_empty());
}
"##,
                correct: Variant {
                    types: Some(DRAINING_TYPES),
                    anchor_fn: Some(r##"/// Hosts eligible to run a job for a region.
pub fn @FN@(@ARG@: &Pool, region: &str) -> Vec<Host> {
    let matching: Vec<Host> = @ARG@
        .hosts
        .iter()
        .filter(|h| h.region == region && !h.is_draining())
        .cloned()
        .collect();
    if matching.is_empty() {
        vec![@ARG@.primary.clone()]
    } else {
        matching
    }
}
"##),
                    extras: "",
                },
                wrong: Variant {
                    types: Some(DRAINING_TYPES),
                    anchor_fn: Some(r##"/// Hosts eligible to run a job for a region.
pub fn @FN@(@ARG@: &Pool, region: &str) -> Vec<Host> {
    let matching: Vec<Host> = @ARG@
        .hosts
        .iter()
        .filter(|h| h.region == region)
        .cloned()
        .collect();
    let mut hosts = if matching.is_empty() {
        vec![@ARG@.primary.clone()]
    } else {
        matching
    };
    hosts.retain(|h| !h.is_draining());
    hosts
}
"##),
                    extras: "",
                },
            },
            TaskTemplate {
                title: "Capacity filter",
                description: r##"# Feature: capacity-aware candidates

Large jobs need hosts with enough free slots.

Add to `src/@MOD@.rs`:

- `pub fn @FN@_with_capacity(@ARG@: &Pool, region: &str, min_slots: u32) -> Vec<Host>`
  returning the candidates for `region` that have at least `min_slots` slots.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{@FN@_with_capacity, Host, Pool};

#[test]
fn small_hosts_are_filtered_out() {
    let pool = Pool::new(
        Host::new("core", "eu", 8),
        vec![Host::new("eu-1", "eu", 1), Host::new("eu-2", "eu", 4)],
    );
    let hosts = @FN@_with_capacity(&pool, "eu", 2);
    assert_eq!(hosts, vec![Host::new("eu-2", "eu", 4)]);
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{@FN@_with_capacity, Host, Pool};

#[test]
fn capacity_candidates_are_never_empty() {
    let pool = Pool::new(
        Host::new("core", "eu", 8),
        vec![Host::new("eu-1", "eu", 1), Host::new("eu-2", "eu", 4)],
    );
    assert!(!@FN@_with_capacity(&pool, "eu", 999).is_empty());
    assert!(!@FN@_with_capacity(&pool, "mars", 1).is_empty());
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Candidates for `region` with at least `min_slots` free slots.
pub fn @FN@_with_capacity(@ARG@: &Pool, region: &str, min_slots: u32) -> Vec<Host> {
    let roomy: Vec<Host> = @FN@(@ARG@, region)
        .into_iter()
        .filter(|h| h.slots >= min_slots)
        .collect();
    if roomy.is_empty() {
        vec![@ARG@.primary.clone()]
    } else {
        roomy
    }
}
"##,
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Candidates for `region` with at least `min_slots` free slots.
pub fn @FN@_with_capacity(@ARG@: &Pool, region: &str, min_slots: u32) -> Vec<Host> {
    @FN@(@ARG@, region)
        .into_iter()
        .filter(|h| h.slots >= min_slots)
        .collect()
}
"##,
                },
            },
        ],
    }
}

const DRAINING_TYPES: &str = r##"#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub region: String,
    pub slots: u32,
    draining: bool,
}

impl Host {
    pub fn new(name: &str, region: &str, slots: u32) -> Self {
        Self {
            name: name.to_string(),
            region: region.to_string(),
            slots,
            draining: false,
        }
    }

    pub fn set_draining(&mut self, on: bool) {
        self.draining = on;
    }

    pub fn is_draining(&self) -> bool {
        self.draining
    }
}

#[derive(Debug, Clone)]
pub struct Pool {
    pub primary: Host,
    pub hosts: Vec<Host>,
}

impl Pool {
    pub fn new(primary: Host, hosts: Vec<Host>) -> Self {
        Self { primary, hosts }
    }
}
"##;

fn no_panic() -> Scenario {
    Scenario {
        id: "no_panic",
        invariant_type: "no_panic",
        invariant_text: "`@FN@` never panics, whatever the input string: it runs on raw config values while the service boots (`load_timeout`).",
        module: "config",
        module_moved: "settings",
        fn_name: "parse_timeout",
        fn_renamed: "timeout_from_str",
        arg: "raw",
        arg_renamed: "text",
        types_import: "",
        consumer_module: "boot",
        consumer: r##"//! Service boot: raw config values become runtime settings here.

use crate::@MOD@::{@FN@, DEFAULT_TIMEOUT_MS};

/// Timeout to use at boot, from the raw config value if present.
pub fn load_timeout(raw: Option<&str>) -> u64 {
    raw.map(@FN@).unwrap_or(DEFAULT_TIMEOUT_MS)
}

/// Total time budget for the startup probes.
pub fn startup_budget_ms(raw_probe_timeouts: &[&str]) -> u64 {
    raw_probe_timeouts.iter().map(|raw| @FN@(raw)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_value_uses_the_default() {
        assert_eq!(load_timeout(None), DEFAULT_TIMEOUT_MS);
        assert_eq!(load_timeout(Some("2s")), 2000);
    }

    #[test]
    fn probe_budget_adds_up() {
        assert_eq!(startup_budget_ms(&["1s", "500ms"]), 1500);
    }
}
"##,
        filler_call: "@FN@(\"10s\") as usize",
        module_doc: "Service configuration values and their parsing.",
        types: r##"pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
"##,
        fn_c1: r##"/// Parse a timeout such as `30s` or `500ms` into milliseconds.
pub fn @FN@(@ARG@: &str) -> u64 {
    let value = @ARG@.trim();
    if let Some(ms) = value.strip_suffix("ms") {
        ms.trim().parse::<u64>().unwrap()
    } else if let Some(s) = value.strip_suffix('s') {
        s.trim().parse::<u64>().unwrap() * 1000
    } else {
        value.parse::<u64>().unwrap()
    }
}
"##,
        fn_c2: r##"/// Parse a timeout such as `30s` or `500ms` into milliseconds.
pub fn @FN@(@ARG@: &str) -> u64 {
    let value = @ARG@.trim();
    let parsed = if let Some(ms) = value.strip_suffix("ms") {
        ms.trim().parse::<u64>().ok()
    } else if let Some(s) = value.strip_suffix('s') {
        s.trim().parse::<u64>().ok().map(|n| n.saturating_mul(1000))
    } else {
        value.parse::<u64>().ok()
    };
    parsed.unwrap_or(DEFAULT_TIMEOUT_MS)
}
"##,
        fn_c2_variant: r##"/// Parse a timeout such as `30s` or `500ms` into milliseconds.
pub fn @FN@(@ARG@: &str) -> u64 {
    let value = @ARG@.trim();
    let (digits, scale) = match value.strip_suffix("ms") {
        Some(ms) => (ms, 1),
        None => match value.strip_suffix('s') {
            Some(s) => (s, 1000),
            None => (value, 1),
        },
    };
    digits
        .trim()
        .parse::<u64>()
        .map(|n| n.saturating_mul(scale))
        .unwrap_or(DEFAULT_TIMEOUT_MS)
}
"##,
        tests_base: r##"    #[test]
    fn seconds_and_milliseconds_are_parsed() {
        assert_eq!(@FN@("30s"), 30_000);
        assert_eq!(@FN@("500ms"), 500);
        assert_eq!(@FN@("750"), 750);
    }
"##,
        test_regression: r##"    #[test]
    fn malformed_values_use_the_default() {
        assert_eq!(@FN@("3O s"), DEFAULT_TIMEOUT_MS);
        assert_eq!(@FN@("soon"), DEFAULT_TIMEOUT_MS);
    }
"##,
        commit_c1: "Initial service configuration",
        commit_c2: "fix: service fails to start when the timeout has a typo",
        commit_c3_drift: "refactor: tidy the config module",
        commit_c3_body: "refactor: parse timeout digits and scale in one place",
        capture_task: r##"# Bug: service crashes on boot with a mistyped timeout

With `timeout = "3O s"` (a typo) in the config the service dies at startup
with `called Option::unwrap() on a None value`.

Expected: fall back to the default timeout and start. Please fix and add a
regression test.
"##,
        tasks: [
            TaskTemplate {
                title: "Minutes and hours",
                description: r##"# Feature: minute and hour timeouts

Operators want to write timeouts as `5m` or `2h`.

Extend `@FN@` in `src/@MOD@.rs` so that the suffix `m` means minutes and
`h` means hours (`5m` = 300000 ms, `2h` = 7200000 ms). Existing suffixes
keep working.
"##,
                primary_test: r##"use @CRATE@::@MOD@::@FN@;

#[test]
fn minutes_and_hours_are_parsed() {
    assert_eq!(@FN@("5m"), 300_000);
    assert_eq!(@FN@("2h"), 7_200_000);
    assert_eq!(@FN@("30s"), 30_000);
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::@FN@;

#[test]
fn parsing_never_panics() {
    for input in ["", "m", "h", "s", "é", "10é", "５m", "-5m", " ", "ms", "1e3s"] {
        let outcome = std::panic::catch_unwind(|| @FN@(input));
        assert!(outcome.is_ok(), "panicked on {input:?}");
    }
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Parse a timeout such as `30s`, `500ms`, `5m` or `2h` into milliseconds.
pub fn @FN@(@ARG@: &str) -> u64 {
    let value = @ARG@.trim();
    let parsed = if let Some(ms) = value.strip_suffix("ms") {
        ms.trim().parse::<u64>().ok()
    } else if let Some(s) = value.strip_suffix('s') {
        s.trim().parse::<u64>().ok().map(|n| n.saturating_mul(1000))
    } else if let Some(m) = value.strip_suffix('m') {
        m.trim().parse::<u64>().ok().map(|n| n.saturating_mul(60_000))
    } else if let Some(h) = value.strip_suffix('h') {
        h.trim().parse::<u64>().ok().map(|n| n.saturating_mul(3_600_000))
    } else {
        value.parse::<u64>().ok()
    };
    parsed.unwrap_or(DEFAULT_TIMEOUT_MS)
}
"##),
                    extras: "",
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Parse a timeout such as `30s`, `500ms`, `5m` or `2h` into milliseconds.
pub fn @FN@(@ARG@: &str) -> u64 {
    let value = @ARG@.trim();
    if let Some(ms) = value.strip_suffix("ms") {
        return ms.trim().parse::<u64>().unwrap_or(DEFAULT_TIMEOUT_MS);
    }
    let (digits, unit) = value.split_at(value.len() - 1);
    let scale = match unit {
        "s" => 1000,
        "m" => 60_000,
        "h" => 3_600_000,
        _ => return value.parse::<u64>().unwrap_or(DEFAULT_TIMEOUT_MS),
    };
    digits
        .trim()
        .parse::<u64>()
        .map(|n| n.saturating_mul(scale))
        .unwrap_or(DEFAULT_TIMEOUT_MS)
}
"##),
                    extras: "",
                },
            },
            TaskTemplate {
                title: "Fractional seconds",
                description: r##"# Feature: fractional seconds

Allow fractional second timeouts such as `1.5s` (= 1500 ms) and `0.25s`
(= 250 ms) in `@FN@` (`src/@MOD@.rs`). Whole-second values keep working.
"##,
                primary_test: r##"use @CRATE@::@MOD@::@FN@;

#[test]
fn fractional_seconds_are_parsed() {
    assert_eq!(@FN@("1.5s"), 1500);
    assert_eq!(@FN@("2s"), 2000);
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::@FN@;

#[test]
fn parsing_never_panics() {
    for input in ["1.s", ".5s", "1..5s", "1.x", "a.bs", "", "s", "1.5", "99999999999999999999s"] {
        let outcome = std::panic::catch_unwind(|| @FN@(input));
        assert!(outcome.is_ok(), "panicked on {input:?}");
    }
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Parse a timeout such as `30s`, `1.5s` or `500ms` into milliseconds.
pub fn @FN@(@ARG@: &str) -> u64 {
    let value = @ARG@.trim();
    let parsed = if let Some(ms) = value.strip_suffix("ms") {
        ms.trim().parse::<u64>().ok()
    } else if let Some(s) = value.strip_suffix('s') {
        s.trim()
            .parse::<f64>()
            .ok()
            .filter(|secs| secs.is_finite() && *secs >= 0.0)
            .map(|secs| (secs * 1000.0) as u64)
    } else {
        value.parse::<u64>().ok()
    };
    parsed.unwrap_or(DEFAULT_TIMEOUT_MS)
}
"##),
                    extras: "",
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Parse a timeout such as `30s`, `1.5s` or `500ms` into milliseconds.
pub fn @FN@(@ARG@: &str) -> u64 {
    let value = @ARG@.trim();
    let parsed = if let Some(ms) = value.strip_suffix("ms") {
        ms.trim().parse::<u64>().ok()
    } else if let Some(s) = value.strip_suffix('s') {
        let s = s.trim();
        if let Some((whole, frac)) = s.split_once('.') {
            let whole: u64 = whole.parse().unwrap();
            let frac: u64 = frac.parse().unwrap();
            Some(whole * 1000 + frac * 100)
        } else {
            s.parse::<u64>().ok().map(|n| n.saturating_mul(1000))
        }
    } else {
        value.parse::<u64>().ok()
    };
    parsed.unwrap_or(DEFAULT_TIMEOUT_MS)
}
"##),
                    extras: "",
                },
            },
        ],
    }
}

fn idempotence() -> Scenario {
    Scenario {
        id: "idempotence",
        invariant_type: "idempotence",
        invariant_text: "`@FN@` is idempotent: normalising an already-normalised path returns it unchanged. Ingest and lookup both normalise, so cache keys must be stable.",
        module: "paths",
        module_moved: "pathutil",
        fn_name: "normalize_path",
        fn_renamed: "canonical_path",
        arg: "path",
        arg_renamed: "raw",
        types_import: "",
        consumer_module: "asset_cache",
        consumer: r##"//! The asset cache: keys are derived at ingest and again at lookup.

use crate::@MOD@::@FN@;

/// Cache key of an asset.
pub fn cache_key(path: &str) -> String {
    format!("asset:{}", @FN@(path))
}

/// Distinct cache keys for a batch of raw paths, sorted.
pub fn key_set(paths: &[&str]) -> Vec<String> {
    let mut keys: Vec<String> = paths.iter().map(|p| cache_key(p)).collect();
    keys.sort();
    keys.dedup();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_prefixed_and_canonical() {
        assert_eq!(cache_key("css/"), "asset:css");
    }

    #[test]
    fn equal_paths_share_a_key() {
        assert_eq!(key_set(&["img/logo.png", "img//logo.png"]).len(), 1);
    }
}
"##,
        filler_call: "@FN@(\"a//b\").len()",
        module_doc: "Asset path canonicalisation.",
        types: "",
        fn_c1: r##"/// Canonical form of an asset path: single slashes, no trailing slash.
pub fn @FN@(@ARG@: &str) -> String {
    let collapsed = @ARG@.replace("//", "/");
    let trimmed = collapsed.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}
"##,
        fn_c2: r##"/// Canonical form of an asset path: single slashes, no trailing slash.
pub fn @FN@(@ARG@: &str) -> String {
    let mut collapsed = @ARG@.to_string();
    while collapsed.contains("//") {
        collapsed = collapsed.replace("//", "/");
    }
    let trimmed = collapsed.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}
"##,
        fn_c2_variant: r##"/// Canonical form of an asset path: single slashes, no trailing slash.
pub fn @FN@(@ARG@: &str) -> String {
    let mut out = String::with_capacity(@ARG@.len());
    for ch in @ARG@.chars() {
        if ch == '/' && out.ends_with('/') {
            continue;
        }
        out.push(ch);
    }
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    if out.is_empty() {
        out.push('/');
    }
    out
}
"##,
        tests_base: r##"    #[test]
    fn double_slashes_collapse() {
        assert_eq!(@FN@("img//logo.png"), "img/logo.png");
    }

    #[test]
    fn trailing_slash_is_dropped() {
        assert_eq!(@FN@("img/"), "img");
        assert_eq!(@FN@("/"), "/");
    }
"##,
        test_regression: r##"    #[test]
    fn runs_of_slashes_collapse() {
        assert_eq!(@FN@("img///logo.png"), "img/logo.png");
        assert_eq!(@FN@("a////b"), "a/b");
    }
"##,
        commit_c1: "Initial asset path helpers",
        commit_c2: "fix: asset paths with repeated slashes are not fully collapsed",
        commit_c3_drift: "refactor: tidy the paths module",
        commit_c3_body: "refactor: collapse slashes in a single pass",
        capture_task: r##"# Bug: cache misses for paths with repeated slashes

Paths like `img///logo.png` produce the cache key `asset:img//logo.png`,
so the lookup (which normalises the path again) misses every time.

Expected: `asset:img/logo.png`. Please fix and add a regression test.
"##,
        tasks: [
            TaskTemplate {
                title: "Current-dir segments",
                description: r##"# Feature: drop `./` segments

Asset paths coming from templates contain current-directory segments.

Extend `@FN@` in `src/@MOD@.rs` so that `./` segments are removed:
`a/./b` becomes `a/b` and `./a` becomes `a`. Everything else is unchanged.
"##,
                primary_test: r##"use @CRATE@::@MOD@::@FN@;

#[test]
fn current_dir_segments_are_removed() {
    assert_eq!(@FN@("a/./b"), "a/b");
    assert_eq!(@FN@("./a"), "a");
    assert_eq!(@FN@("a/b"), "a/b");
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::@FN@;

#[test]
fn normalising_twice_changes_nothing() {
    for input in ["a/././b", "./././a", "x/./y/./z", "./a//./b/", "a/./"] {
        let once = @FN@(input);
        let twice = @FN@(&once);
        assert_eq!(once, twice, "not idempotent for {input:?}");
    }
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Canonical form of an asset path: single slashes, no `./`, no trailing slash.
pub fn @FN@(@ARG@: &str) -> String {
    let mut collapsed = @ARG@.to_string();
    loop {
        let next = collapsed.replace("//", "/").replace("/./", "/");
        let next = match next.strip_prefix("./") {
            Some(rest) => rest.to_string(),
            None => next,
        };
        if next == collapsed {
            break;
        }
        collapsed = next;
    }
    let trimmed = collapsed.trim_end_matches('/');
    let trimmed = trimmed.strip_suffix("/.").unwrap_or(trimmed);
    if trimmed.is_empty() || trimmed == "." {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}
"##),
                    extras: "",
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Canonical form of an asset path: single slashes, no `./`, no trailing slash.
pub fn @FN@(@ARG@: &str) -> String {
    let mut collapsed = @ARG@.to_string();
    while collapsed.contains("//") {
        collapsed = collapsed.replace("//", "/");
    }
    let collapsed = collapsed.replace("/./", "/");
    let collapsed = collapsed.strip_prefix("./").unwrap_or(&collapsed);
    let trimmed = collapsed.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}
"##),
                    extras: "",
                },
            },
            TaskTemplate {
                title: "Parent segments",
                description: r##"# Feature: resolve `..` segments

Extend `@FN@` in `src/@MOD@.rs` to resolve parent segments: `a/b/../c`
becomes `a/c`. A `..` with nothing before it is dropped (`../a` becomes
`a`). Everything else is unchanged.
"##,
                primary_test: r##"use @CRATE@::@MOD@::@FN@;

#[test]
fn parent_segments_are_resolved() {
    assert_eq!(@FN@("a/b/../c"), "a/c");
    assert_eq!(@FN@("../a"), "a");
    assert_eq!(@FN@("a/b"), "a/b");
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::@FN@;

#[test]
fn normalising_twice_changes_nothing() {
    for input in ["a/b/../../c", "a/b/c/../../d", "x/../y/../z", "a/../../b"] {
        let once = @FN@(input);
        let twice = @FN@(&once);
        assert_eq!(once, twice, "not idempotent for {input:?}");
    }
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Canonical form of an asset path: single slashes, `..` resolved, no trailing slash.
pub fn @FN@(@ARG@: &str) -> String {
    let absolute = @ARG@.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in @ARG@.split('/') {
        match segment {
            "" => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    let joined = segments.join("/");
    if joined.is_empty() {
        "/".to_string()
    } else if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}
"##),
                    extras: "",
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Canonical form of an asset path: single slashes, `..` resolved, no trailing slash.
pub fn @FN@(@ARG@: &str) -> String {
    let mut collapsed = @ARG@.to_string();
    while collapsed.contains("//") {
        collapsed = collapsed.replace("//", "/");
    }
    let mut segments: Vec<&str> = collapsed.split('/').collect();
    if let Some(i) = segments.iter().position(|s| *s == "..") {
        segments.remove(i);
        if i > 0 {
            segments.remove(i - 1);
        }
    }
    let joined = segments.join("/");
    let trimmed = joined.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}
"##),
                    extras: "",
                },
            },
        ],
    }
}

fn precondition() -> Scenario {
    Scenario {
        id: "precondition",
        invariant_type: "precondition",
        invariant_text: "`@FN@` requires `size > 0` and rejects zero-size reservations (panics). `release` identifies blocks by offset, so zero-size blocks would alias their neighbour.",
        module: "arena",
        module_moved: "blocks",
        fn_name: "reserve",
        fn_renamed: "claim",
        arg: "arena",
        arg_renamed: "heap",
        types_import: "Arena",
        consumer_module: "buffer_pool",
        consumer: r##"//! Buffer bookkeeping on top of the arena. Buffers are tracked by their
//! block offset.

use std::collections::BTreeMap;

use crate::@MOD@::{@FN@, Arena, Block};

/// Reserve one block per requested size, in order.
pub fn checkout(arena: &mut Arena, sizes: &[usize]) -> Vec<Block> {
    sizes.iter().map(|size| @FN@(arena, *size)).collect()
}

/// Label live blocks by offset; the map has one entry per live block.
pub fn directory(blocks: &[Block]) -> BTreeMap<usize, usize> {
    blocks.iter().map(|b| (b.offset, b.size)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_lays_buffers_out_in_order() {
        let mut arena = Arena::new();
        let blocks = checkout(&mut arena, &[16, 8]);
        assert_eq!(blocks[0].offset, 0);
        assert_eq!(blocks[1].offset, 16);
    }

    #[test]
    fn the_directory_has_one_entry_per_buffer() {
        let mut arena = Arena::new();
        let blocks = checkout(&mut arena, &[16, 8, 4]);
        assert_eq!(directory(&blocks).len(), 3);
    }
}
"##,
        filler_call: "@FN@(&mut Arena::new(), 8).size",
        module_doc: "A bump arena with explicit release by offset.",
        types: r##"#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub offset: usize,
    pub size: usize,
}

#[derive(Debug, Default)]
pub struct Arena {
    pub used: usize,
    pub blocks: Vec<Block>,
}

impl Arena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn live(&self) -> usize {
        self.blocks.len()
    }
}

/// Release a block; blocks are identified by their offset.
pub fn release(arena: &mut Arena, block: Block) {
    if let Some(i) = arena.blocks.iter().position(|b| b.offset == block.offset) {
        arena.blocks.remove(i);
    }
}
"##,
        fn_c1: r##"/// Reserve `size` bytes at the end of the arena.
pub fn @FN@(@ARG@: &mut Arena, size: usize) -> Block {
    let block = Block {
        offset: @ARG@.used,
        size,
    };
    @ARG@.used += size;
    @ARG@.blocks.push(block);
    block
}
"##,
        fn_c2: r##"/// Reserve `size` bytes at the end of the arena.
pub fn @FN@(@ARG@: &mut Arena, size: usize) -> Block {
    assert!(size > 0, "reservation must not be empty");
    let block = Block {
        offset: @ARG@.used,
        size,
    };
    @ARG@.used += size;
    @ARG@.blocks.push(block);
    block
}
"##,
        fn_c2_variant: r##"/// Reserve `size` bytes at the end of the arena.
pub fn @FN@(@ARG@: &mut Arena, size: usize) -> Block {
    if size == 0 {
        panic!("reservation must not be empty");
    }
    let offset = @ARG@.used;
    @ARG@.used += size;
    let block = Block { offset, size };
    @ARG@.blocks.push(block);
    block
}
"##,
        tests_base: r##"    #[test]
    fn reservations_are_laid_out_in_order() {
        let mut arena = Arena::new();
        let a = @FN@(&mut arena, 16);
        let b = @FN@(&mut arena, 8);
        assert_eq!(a.offset, 0);
        assert_eq!(b.offset, 16);
        assert_eq!(arena.live(), 2);
    }

    #[test]
    fn release_removes_the_block() {
        let mut arena = Arena::new();
        let a = @FN@(&mut arena, 16);
        release(&mut arena, a);
        assert_eq!(arena.live(), 0);
    }
"##,
        test_regression: r##"    #[test]
    #[should_panic]
    fn empty_reservations_are_rejected() {
        let mut arena = Arena::new();
        @FN@(&mut arena, 0);
    }
"##,
        commit_c1: "Initial arena allocator",
        commit_c2: "fix: releasing a block after an empty reservation frees the wrong block",
        commit_c3_drift: "refactor: tidy the arena module",
        commit_c3_body: "refactor: compute the block offset before bumping",
        capture_task: r##"# Bug: release frees the wrong block after an empty reservation

After `reserve(&mut arena, 0)` followed by `reserve(&mut arena, 16)`,
releasing the second block removes the first one instead (they share the
offset).

Expected: an empty reservation must never corrupt the arena. Please fix
and add a regression test.
"##,
        tasks: [
            TaskTemplate {
                title: "Aligned reservations",
                description: r##"# Feature: aligned reservations

SIMD buffers need aligned offsets.

Add to `src/@MOD@.rs`:

- `pub fn @FN@_aligned(@ARG@: &mut Arena, size: usize, align: usize) -> Block`
  which reserves `size` bytes at the next offset that is a multiple of
  `align` (a power of two), skipping the padding.

Example: after a 5-byte reservation, `@FN@_aligned(&mut arena, 3, 8)`
returns a block at offset 8.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{@FN@, @FN@_aligned, Arena};

#[test]
fn aligned_reservations_round_up() {
    let mut arena = Arena::new();
    @FN@(&mut arena, 5);
    let block = @FN@_aligned(&mut arena, 3, 8);
    assert_eq!(block.offset, 8);
    assert_eq!(block.size, 3);
    assert!(arena.used >= 11);
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{@FN@_aligned, Arena};

#[test]
fn empty_aligned_reservations_are_rejected() {
    let outcome = std::panic::catch_unwind(|| {
        let mut arena = Arena::new();
        @FN@_aligned(&mut arena, 0, 8)
    });
    assert!(outcome.is_err(), "an empty reservation must be rejected");
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Reserve `size` bytes at the next offset aligned to `align`.
pub fn @FN@_aligned(@ARG@: &mut Arena, size: usize, align: usize) -> Block {
    let padding = (align - @ARG@.used % align) % align;
    @ARG@.used += padding;
    @FN@(@ARG@, size)
}
"##,
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Reserve `size` bytes at the next offset aligned to `align`.
pub fn @FN@_aligned(@ARG@: &mut Arena, size: usize, align: usize) -> Block {
    let padding = (align - @ARG@.used % align) % align;
    let block = Block {
        offset: @ARG@.used + padding,
        size,
    };
    @ARG@.used += padding + size;
    @ARG@.blocks.push(block);
    block
}
"##,
                },
            },
            TaskTemplate {
                title: "Regrow a block",
                description: r##"# Feature: regrow a block

Buffers that outgrow their block need to be moved to a bigger one.

Add to `src/@MOD@.rs`:

- `pub fn regrow(@ARG@: &mut Arena, block: Block, new_size: usize) -> Block`
  which releases `block` and reserves `new_size` bytes at the end of the
  arena, returning the new block.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{regrow, @FN@, Arena};

#[test]
fn regrow_moves_the_block_to_the_end() {
    let mut arena = Arena::new();
    let a = @FN@(&mut arena, 8);
    let b = @FN@(&mut arena, 8);
    let bigger = regrow(&mut arena, a, 32);
    assert_eq!(bigger.size, 32);
    assert_eq!(bigger.offset, 16);
    assert!(!arena.blocks.contains(&a));
    assert!(arena.blocks.contains(&b));
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{regrow, @FN@, Arena};

#[test]
fn regrowing_to_zero_is_rejected() {
    let outcome = std::panic::catch_unwind(|| {
        let mut arena = Arena::new();
        let a = @FN@(&mut arena, 8);
        regrow(&mut arena, a, 0)
    });
    assert!(outcome.is_err(), "an empty reservation must be rejected");
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Move `block` to a fresh reservation of `new_size` bytes.
pub fn regrow(@ARG@: &mut Arena, block: Block, new_size: usize) -> Block {
    release(@ARG@, block);
    @FN@(@ARG@, new_size)
}
"##,
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Move `block` to a fresh reservation of `new_size` bytes.
pub fn regrow(@ARG@: &mut Arena, block: Block, new_size: usize) -> Block {
    release(@ARG@, block);
    let moved = Block {
        offset: @ARG@.used,
        size: new_size,
    };
    @ARG@.used += new_size;
    @ARG@.blocks.push(moved);
    moved
}
"##,
                },
            },
        ],
    }
}

fn no_side_effect() -> Scenario {
    Scenario {
        id: "no_side_effect",
        invariant_type: "no_side_effect",
        invariant_text: "`@FN@` is pure: it never advances `NEXT_INVOICE`. Previews must not consume invoice numbers because the sequence is audited for gaps.",
        module: "billing",
        module_moved: "invoices",
        fn_name: "render_invoice",
        fn_renamed: "format_invoice",
        arg: "invoice",
        arg_renamed: "document",
        types_import: "Invoice",
        consumer_module: "statements",
        consumer: r##"//! Customer statements assembled from rendered invoices.

use crate::@MOD@::{@FN@, Invoice};

/// A statement: every invoice rendered, separated by a rule.
pub fn statement(invoices: &[Invoice]) -> String {
    invoices
        .iter()
        .map(@FN@)
        .collect::<Vec<_>>()
        .join("---\n")
}

/// How many printed lines a statement takes.
pub fn statement_lines(invoices: &[Invoice]) -> usize {
    statement(invoices).lines().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::@MOD@::Line;

    #[test]
    fn statements_keep_every_line_item() {
        let paid = Invoice::draft(vec![Line::new("widget", 500)]).issue();
        let text = statement(&[paid]);
        assert!(text.contains("widget 500\n"));
        assert!(text.ends_with("TOTAL 500\n"));
    }

    #[test]
    fn line_counts_add_up() {
        let paid = Invoice::draft(vec![Line::new("widget", 500)]).issue();
        assert_eq!(statement_lines(&[paid]), 3);
    }
}
"##,
        filler_call: "@FN@(&Invoice::draft(Vec::new())).len()",
        module_doc: "Invoices: numbering sequence, drafts and rendering.",
        types: r##"use std::sync::atomic::{AtomicU64, Ordering};

/// Global invoice sequence; every issued invoice takes the next number.
pub static NEXT_INVOICE: AtomicU64 = AtomicU64::new(1000);

pub fn issue_number() -> u64 {
    NEXT_INVOICE.fetch_add(1, Ordering::SeqCst)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub item: String,
    pub cents: u64,
}

impl Line {
    pub fn new(item: &str, cents: u64) -> Self {
        Self {
            item: item.to_string(),
            cents,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Invoice {
    pub number: u64,
    pub lines: Vec<Line>,
}

impl Invoice {
    /// An unissued invoice (number 0).
    pub fn draft(lines: Vec<Line>) -> Self {
        Self { number: 0, lines }
    }

    pub fn issue(mut self) -> Self {
        self.number = issue_number();
        self
    }

    pub fn total(&self) -> u64 {
        self.lines.iter().map(|l| l.cents).sum()
    }
}
"##,
        fn_c1: r##"/// Render an invoice for printing or preview.
pub fn @FN@(@ARG@: &Invoice) -> String {
    let number = if @ARG@.number == 0 {
        issue_number()
    } else {
        @ARG@.number
    };
    let mut out = format!("INVOICE #{number}\n");
    for line in &@ARG@.lines {
        out.push_str(&format!("{} {}\n", line.item, line.cents));
    }
    out.push_str(&format!("TOTAL {}\n", @ARG@.total()));
    out
}
"##,
        fn_c2: r##"/// Render an invoice for printing or preview.
pub fn @FN@(@ARG@: &Invoice) -> String {
    let mut out = if @ARG@.number == 0 {
        "INVOICE DRAFT\n".to_string()
    } else {
        format!("INVOICE #{}\n", @ARG@.number)
    };
    for line in &@ARG@.lines {
        out.push_str(&format!("{} {}\n", line.item, line.cents));
    }
    out.push_str(&format!("TOTAL {}\n", @ARG@.total()));
    out
}
"##,
        fn_c2_variant: r##"/// Render an invoice for printing or preview.
pub fn @FN@(@ARG@: &Invoice) -> String {
    let header = match @ARG@.number {
        0 => "INVOICE DRAFT".to_string(),
        n => format!("INVOICE #{n}"),
    };
    let body: Vec<String> = @ARG@
        .lines
        .iter()
        .map(|line| format!("{} {}", line.item, line.cents))
        .collect();
    let mut out = header;
    out.push('\n');
    for line in body {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str(&format!("TOTAL {}\n", @ARG@.total()));
    out
}
"##,
        tests_base: r##"    #[test]
    fn issued_invoices_show_their_number() {
        let invoice = Invoice::draft(vec![Line::new("widget", 500)]).issue();
        let text = @FN@(&invoice);
        assert!(text.starts_with(&format!("INVOICE #{}\n", invoice.number)));
        assert!(text.contains("widget 500\n"));
        assert!(text.ends_with("TOTAL 500\n"));
    }
"##,
        test_regression: r##"    #[test]
    fn previewing_a_draft_is_stable() {
        let draft = Invoice::draft(vec![Line::new("widget", 500)]);
        assert_eq!(@FN@(&draft), @FN@(&draft));
        assert!(@FN@(&draft).starts_with("INVOICE DRAFT\n"));
    }
"##,
        commit_c1: "Initial billing service",
        commit_c2: "fix: invoice previews show a different number every time",
        commit_c3_drift: "refactor: tidy the billing module",
        commit_c3_body: "refactor: build the invoice text from header and body",
        capture_task: r##"# Bug: draft invoice preview shows a different number each time

Opening the preview of an unsent invoice twice shows two different invoice
numbers, and those numbers are then missing from the ledger.

Expected: a stable `INVOICE DRAFT` preview. Please fix and add a
regression test.
"##,
        tasks: [
            TaskTemplate {
                title: "Preview with upcoming number",
                description: r##"# Feature: previews show the upcoming invoice number

Customers want to see, on a draft's preview, the number the invoice will
get when it is issued.

Add to `src/@MOD@.rs`:

- `pub fn @FN@_preview(@ARG@: &Invoice) -> String`: identical to `@FN@` for
  issued invoices; for a draft the header line must be
  `INVOICE #<n> (preview)` where `<n>` is the number the invoice will
  receive when issued, followed by the usual lines and total.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{@FN@, @FN@_preview, Invoice, Line, NEXT_INVOICE};
use std::sync::atomic::Ordering;

#[test]
fn previews_show_the_upcoming_number() {
    let draft = Invoice::draft(vec![Line::new("widget", 500)]);
    let upcoming = NEXT_INVOICE.load(Ordering::SeqCst);
    let text = @FN@_preview(&draft);
    assert!(
        text.starts_with(&format!("INVOICE #{upcoming} (preview)\n")),
        "{text:?}"
    );
    assert!(text.contains("widget 500\n"));
    assert!(text.ends_with("TOTAL 500\n"));
    let issued = draft.issue();
    assert_eq!(@FN@_preview(&issued), @FN@(&issued));
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{@FN@_preview, Invoice, Line, NEXT_INVOICE};
use std::sync::atomic::Ordering;

#[test]
fn previewing_does_not_consume_numbers() {
    let draft = Invoice::draft(vec![Line::new("widget", 500)]);
    let before = NEXT_INVOICE.load(Ordering::SeqCst);
    let _ = @FN@_preview(&draft);
    let _ = @FN@_preview(&draft);
    assert_eq!(NEXT_INVOICE.load(Ordering::SeqCst), before);
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Preview of an invoice; drafts show the number they will receive.
pub fn @FN@_preview(@ARG@: &Invoice) -> String {
    if @ARG@.number != 0 {
        return @FN@(@ARG@);
    }
    let upcoming = NEXT_INVOICE.load(Ordering::SeqCst);
    let body = @FN@(@ARG@);
    let body = body.strip_prefix("INVOICE DRAFT\n").unwrap_or(&body);
    format!("INVOICE #{upcoming} (preview)\n{body}")
}
"##,
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Preview of an invoice; drafts show the number they will receive.
pub fn @FN@_preview(@ARG@: &Invoice) -> String {
    if @ARG@.number != 0 {
        return @FN@(@ARG@);
    }
    let upcoming = issue_number();
    let body = @FN@(@ARG@);
    let body = body.strip_prefix("INVOICE DRAFT\n").unwrap_or(&body);
    format!("INVOICE #{upcoming} (preview)\n{body}")
}
"##,
                },
            },
            TaskTemplate {
                title: "Month-end statement",
                description: r##"# Feature: month-end statement

Month-end statements print several invoices at once and tell the account
which number comes next.

Add to `src/@MOD@.rs`:

- `pub fn @FN@_statement(invoices: &[Invoice]) -> String`: every invoice
  rendered with `@FN@`, separated by a blank line, followed by a final line
  `NEXT #<n>` where `<n>` is the number the next issued invoice will get.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{@FN@, @FN@_statement, Invoice, Line, NEXT_INVOICE};
use std::sync::atomic::Ordering;

#[test]
fn statements_list_invoices_and_the_next_number() {
    let a = Invoice::draft(vec![Line::new("widget", 500)]).issue();
    let b = Invoice::draft(vec![Line::new("gadget", 700)]);
    let next = NEXT_INVOICE.load(Ordering::SeqCst);
    let text = @FN@_statement(&[a.clone(), b]);
    assert!(text.contains(&@FN@(&a)));
    assert!(text.contains("gadget 700\n"));
    assert!(text.contains("\n\n"), "{text:?}");
    assert!(text.ends_with(&format!("NEXT #{next}\n")), "{text:?}");
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{@FN@_statement, Invoice, Line, NEXT_INVOICE};
use std::sync::atomic::Ordering;

#[test]
fn printing_a_statement_does_not_consume_numbers() {
    let invoices = vec![
        Invoice::draft(vec![Line::new("widget", 500)]),
        Invoice::draft(vec![Line::new("gadget", 700)]),
    ];
    let before = NEXT_INVOICE.load(Ordering::SeqCst);
    let _ = @FN@_statement(&invoices);
    assert_eq!(NEXT_INVOICE.load(Ordering::SeqCst), before);
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Month-end statement: every invoice, then the next number to be issued.
pub fn @FN@_statement(invoices: &[Invoice]) -> String {
    let mut out = invoices.iter().map(@FN@).collect::<Vec<_>>().join("\n");
    out.push_str(&format!("\nNEXT #{}\n", NEXT_INVOICE.load(Ordering::SeqCst)));
    out
}
"##,
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: None,
                    extras: r##"/// Month-end statement: every invoice, then the next number to be issued.
pub fn @FN@_statement(invoices: &[Invoice]) -> String {
    let mut out = invoices.iter().map(@FN@).collect::<Vec<_>>().join("\n");
    out.push_str(&format!("\nNEXT #{}\n", issue_number()));
    out
}
"##,
                },
            },
        ],
    }
}

fn commutativity() -> Scenario {
    Scenario {
        id: "commutativity",
        invariant_type: "commutativity",
        invariant_text: "`@FN@(a, b) == @FN@(b, a)`: the discovery board is built once per profile and shown to both parties, so scores must agree in both directions.",
        module: "matching",
        module_moved: "affinity",
        fn_name: "affinity_score",
        fn_renamed: "match_score",
        arg: "left",
        arg_renamed: "first",
        types_import: "Profile",
        consumer_module: "discovery",
        consumer: r##"//! The discovery board: the same ranking is shown to both parties.

use crate::@MOD@::{@FN@, Profile};

/// The best candidate for `me`.
pub fn best_match<'a>(me: &Profile, candidates: &'a [Profile]) -> Option<&'a Profile> {
    candidates.iter().max_by_key(|c| @FN@(me, c))
}

/// The board: candidates with their scores, best first.
pub fn board<'a>(me: &Profile, candidates: &'a [Profile]) -> Vec<(&'a Profile, u32)> {
    let mut rows: Vec<(&Profile, u32)> = candidates.iter().map(|c| (c, @FN@(me, c))).collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_match_prefers_the_highest_score() {
        let me = Profile::new("me", &["rust"], "lima");
        let candidates = vec![
            Profile::new("far", &["rust"], "oslo"),
            Profile::new("near", &["rust"], "lima"),
        ];
        assert_eq!(
            best_match(&me, &candidates).map(|p| p.name.as_str()),
            Some("near")
        );
    }

    #[test]
    fn the_board_ranks_best_first() {
        let me = Profile::new("me", &["rust"], "lima");
        let candidates = vec![
            Profile::new("far", &["rust"], "oslo"),
            Profile::new("near", &["rust"], "lima"),
        ];
        let rows = board(&me, &candidates);
        assert_eq!(rows[0].0.name, "near");
        assert!(rows[0].1 > rows[1].1);
    }
}
"##,
        filler_call: "@FN@(&Profile::new(\"a\", &[], \"x\"), &Profile::new(\"b\", &[], \"x\")) as usize",
        module_doc: "Profile matching for the discovery board.",
        types: r##"#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub tags: Vec<String>,
    pub city: String,
    pub blocked: Vec<String>,
}

impl Profile {
    pub fn new(name: &str, tags: &[&str], city: &str) -> Self {
        Self {
            name: name.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            city: city.to_string(),
            blocked: Vec::new(),
        }
    }

    pub fn block(&mut self, name: &str) {
        self.blocked.push(name.to_string());
    }
}
"##,
        fn_c1: r##"/// Affinity between two profiles: shared tags weighted by rank, plus a city bonus.
pub fn @FN@(@ARG@: &Profile, right: &Profile) -> u32 {
    let tags: u32 = @ARG@
        .tags
        .iter()
        .enumerate()
        .filter(|(_, tag)| right.tags.contains(tag))
        .map(|(rank, _)| 3 - rank.min(2) as u32)
        .sum();
    let city = if @ARG@.city == right.city { 3 } else { 0 };
    tags + city
}
"##,
        fn_c2: r##"/// Affinity between two profiles: shared tags, plus a city bonus.
pub fn @FN@(@ARG@: &Profile, right: &Profile) -> u32 {
    let shared = @ARG@
        .tags
        .iter()
        .filter(|tag| right.tags.contains(tag))
        .count() as u32;
    let city = if @ARG@.city == right.city { 3 } else { 0 };
    shared * 2 + city
}
"##,
        fn_c2_variant: r##"/// Affinity between two profiles: shared tags, plus a city bonus.
pub fn @FN@(@ARG@: &Profile, right: &Profile) -> u32 {
    let mut score = 0;
    for tag in &@ARG@.tags {
        if right.tags.contains(tag) {
            score += 2;
        }
    }
    if @ARG@.city == right.city {
        score += 3;
    }
    score
}
"##,
        tests_base: r##"    #[test]
    fn shared_tags_and_city_add_up() {
        let a = Profile::new("ana", &["rust", "chess"], "lima");
        let b = Profile::new("bo", &["chess"], "lima");
        assert_eq!(@FN@(&a, &b), 5);
        assert_eq!(@FN@(&a, &Profile::new("cy", &[], "oslo")), 0);
    }
"##,
        test_regression: r##"    #[test]
    fn board_scores_agree_for_both_parties() {
        let a = Profile::new("ana", &["rust", "chess", "jazz"], "lima");
        let b = Profile::new("bo", &["jazz", "rust"], "oslo");
        assert_eq!(@FN@(&a, &b), @FN@(&b, &a));
    }
"##,
        commit_c1: "Initial matching service",
        commit_c2: "fix: mismatched scores on the discovery board",
        commit_c3_drift: "refactor: tidy the matching module",
        commit_c3_body: "refactor: accumulate the affinity score in a loop",
        capture_task: r##"# Bug: discovery board disagrees between two profiles

Ana sees Bo as a top match but Bo's board ranks Ana far down, although
they share the same tags. The board is computed once per profile and
shown to both parties.

Please fix so both parties see consistent scores, and add a regression
test.
"##,
        tasks: [
            TaskTemplate {
                title: "Blocked profiles",
                description: r##"# Feature: blocked profiles never match

Profiles can block other profiles by name (`Profile::block`).

Change `@FN@` in `src/@MOD@.rs` so that a blocked pair scores 0: when a
profile has blocked the other one, the affinity is 0 regardless of tags.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{@FN@, Profile};

#[test]
fn blocking_zeroes_the_score() {
    let mut a = Profile::new("ana", &["rust"], "lima");
    let b = Profile::new("bo", &["rust"], "lima");
    assert!(@FN@(&a, &b) > 0);
    a.block("bo");
    assert_eq!(@FN@(&a, &b), 0);
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{@FN@, Profile};

#[test]
fn scores_agree_in_both_directions() {
    let mut a = Profile::new("ana", &["rust", "chess"], "lima");
    let b = Profile::new("bo", &["rust"], "lima");
    a.block("bo");
    assert_eq!(@FN@(&a, &b), @FN@(&b, &a));
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Affinity between two profiles: shared tags, plus a city bonus; 0 if blocked.
pub fn @FN@(@ARG@: &Profile, right: &Profile) -> u32 {
    if @ARG@.blocked.contains(&right.name) || right.blocked.contains(&@ARG@.name) {
        return 0;
    }
    let shared = @ARG@
        .tags
        .iter()
        .filter(|tag| right.tags.contains(tag))
        .count() as u32;
    let city = if @ARG@.city == right.city { 3 } else { 0 };
    shared * 2 + city
}
"##),
                    extras: "",
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Affinity between two profiles: shared tags, plus a city bonus; 0 if blocked.
pub fn @FN@(@ARG@: &Profile, right: &Profile) -> u32 {
    if @ARG@.blocked.contains(&right.name) {
        return 0;
    }
    let shared = @ARG@
        .tags
        .iter()
        .filter(|tag| right.tags.contains(tag))
        .count() as u32;
    let city = if @ARG@.city == right.city { 3 } else { 0 };
    shared * 2 + city
}
"##),
                    extras: "",
                },
            },
            TaskTemplate {
                title: "Well-described bonus",
                description: r##"# Feature: bonus for well-described profiles

Well-described profiles (three or more tags) get better matches.

Change `@FN@` in `src/@MOD@.rs` so that a pair of well-described profiles
(both with at least three tags) scores one extra point.
"##,
                primary_test: r##"use @CRATE@::@MOD@::{@FN@, Profile};

#[test]
fn well_described_pairs_get_a_bonus() {
    let a = Profile::new("ana", &["rust", "chess", "jazz"], "lima");
    let b = Profile::new("bo", &["rust", "chess", "jazz"], "oslo");
    assert_eq!(@FN@(&a, &b), 7);
    let c = Profile::new("cy", &["rust"], "lima");
    assert_eq!(@FN@(&c, &b), 2);
}
"##,
                oracle_test: r##"use @CRATE@::@MOD@::{@FN@, Profile};

#[test]
fn scores_agree_in_both_directions() {
    let a = Profile::new("ana", &["rust", "chess", "jazz"], "lima");
    let b = Profile::new("bo", &["rust"], "lima");
    assert_eq!(@FN@(&a, &b), @FN@(&b, &a));
}
"##,
                correct: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Affinity between two profiles: shared tags, a city bonus and a bonus for well-described pairs.
pub fn @FN@(@ARG@: &Profile, right: &Profile) -> u32 {
    let shared = @ARG@
        .tags
        .iter()
        .filter(|tag| right.tags.contains(tag))
        .count() as u32;
    let city = if @ARG@.city == right.city { 3 } else { 0 };
    let described = if @ARG@.tags.len() >= 3 && right.tags.len() >= 3 { 1 } else { 0 };
    shared * 2 + city + described
}
"##),
                    extras: "",
                },
                wrong: Variant {
                    types: None,
                    anchor_fn: Some(r##"/// Affinity between two profiles: shared tags, a city bonus and a bonus for well-described pairs.
pub fn @FN@(@ARG@: &Profile, right: &Profile) -> u32 {
    let shared = @ARG@
        .tags
        .iter()
        .filter(|tag| right.tags.contains(tag))
        .count() as u32;
    let city = if @ARG@.city == right.city { 3 } else { 0 };
    let described = if @ARG@.tags.len() >= 3 { 1 } else { 0 };
    shared * 2 + city + described
}
"##),
                    extras: "",
                },
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_eight_distinct_scenarios() {
        let scenarios = all();
        assert_eq!(scenarios.len(), 8);
        let mut ids: Vec<&str> = scenarios.iter().map(|s| s.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 8);
        assert!(by_id("sorted_output").is_some());
        assert!(by_id("nope").is_none());
    }

    #[test]
    fn every_template_uses_the_anchor_placeholder() {
        for s in all() {
            for (label, text) in [
                ("fn_c1", s.fn_c1),
                ("fn_c2", s.fn_c2),
                ("fn_c2_variant", s.fn_c2_variant),
                ("tests_base", s.tests_base),
                ("invariant_text", s.invariant_text),
                ("filler_call", s.filler_call),
                ("consumer", s.consumer),
            ] {
                assert!(text.contains("@FN@"), "{}::{label} lacks @FN@", s.id);
            }
            assert!(s.fn_c2.contains("@ARG@"), "{}", s.id);
            assert_ne!(s.fn_name, s.fn_renamed, "{}", s.id);
            assert_ne!(s.arg, s.arg_renamed, "{}", s.id);
            assert_ne!(s.module, s.module_moved, "{}", s.id);
            // Non-local invariant: the consumer lives in its own module and
            // imports the provider; the provider text never mentions it.
            assert_ne!(s.consumer_module, s.module, "{}", s.id);
            assert_ne!(s.consumer_module, s.module_moved, "{}", s.id);
            assert!(s.consumer.contains("use crate::@MOD@::"), "{}", s.id);
            assert!(s.consumer.contains("#[cfg(test)]"), "{}", s.id);
            assert!(!s.types.contains(s.consumer_module), "{}", s.id);
            for task in &s.tasks {
                assert!(task.primary_test.contains("@CRATE@::@MOD@"), "{}", s.id);
                assert!(task.oracle_test.contains("@CRATE@::@MOD@"), "{}", s.id);
                assert!(task.description.contains("@MOD@"), "{}", s.id);
            }
        }
    }

    #[test]
    fn commit_messages_do_not_spell_out_the_invariant() {
        // The findability knob (go-no-go.md §3): the fix is mentioned, the
        // rule is not shouted.
        for s in all() {
            let msg = s.commit_c2.to_lowercase();
            for banned in ["invariant", "never", "always", "must", "warning"] {
                assert!(
                    !msg.contains(banned),
                    "{}: {msg:?} contains {banned:?}",
                    s.id
                );
            }
        }
    }
}
