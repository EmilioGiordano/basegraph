use quotaflow::scheduling::{merge_windows, Window};

#[test]
fn inverted_windows_are_normalised() {
    assert_eq!(Window::new(6, 2).normalised(), Window::new(2, 6));
    assert_eq!(Window::new(2, 6).normalised(), Window::new(2, 6));
    assert_eq!(merge_windows(&[Window::new(6, 2)]), vec![Window::new(2, 6)]);
}
