use orbitdesk::scheduling::{merge_windows, Window};

#[test]
fn pinned_windows_are_kept_verbatim() {
    let out = merge_windows(&[Window::new(1, 4), Window::pinned(2, 3), Window::new(3, 6)]);
    assert!(out.contains(&Window::pinned(2, 3)), "{out:?}");
    assert!(out.contains(&Window::new(1, 6)), "{out:?}");
    assert_eq!(out.len(), 2);
    assert!(Window::pinned(2, 3).is_pinned());
    assert!(!Window::new(2, 3).is_pinned());
}
