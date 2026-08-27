//! Maintenance windows and the merge step that turns requests into a schedule.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// First free slot between scheduled windows.
pub fn first_gap(schedule: &[Window]) -> Option<u32> {
    schedule
        .windows(2)
        .find(|pair| pair[1].start > pair[0].end + 1)
        .map(|pair| pair[0].end + 1)
}

/// Merge overlapping maintenance windows into a compact schedule.
pub fn merge_windows(windows: &[Window]) -> Vec<Window> {
    let mut ordered: Vec<Window> = windows.to_vec();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_windows_merge() {
        let merged = merge_windows(&[Window::new(1, 3), Window::new(2, 5)]);
        assert_eq!(merged, vec![Window::new(1, 5)]);
    }

    #[test]
    fn disjoint_windows_stay_apart() {
        let merged = merge_windows(&[Window::new(1, 2), Window::new(4, 6)]);
        assert_eq!(merged.len(), 2);
        assert_eq!(first_gap(&merged), Some(3));
    }

    #[test]
    fn unordered_requests_still_merge() {
        let merged = merge_windows(&[Window::new(5, 7), Window::new(1, 2), Window::new(2, 6)]);
        assert_eq!(merged, vec![Window::new(1, 7)]);
    }
}
