use orbitdesk::scheduling::{merge_windows, Window};

#[test]
fn schedule_stays_sorted_with_inverted_windows() {
    let out = merge_windows(&[Window::new(3, 4), Window::new(9, 1), Window::new(12, 12)]);
    assert!(
        out.windows(2).all(|p| p[0].start <= p[1].start),
        "schedule is not sorted by start: {out:?}"
    );
}
