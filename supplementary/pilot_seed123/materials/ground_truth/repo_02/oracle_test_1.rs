use quotaflow::scheduling::{merge_windows, Window};

#[test]
fn schedule_stays_sorted_with_pinned_windows() {
    let out = merge_windows(&[
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
