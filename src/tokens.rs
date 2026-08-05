//! Token counting utilities.

/// Trait for counting tokens in a text.
pub trait TokenCounter {
    /// Count the number of tokens in the given text.
    fn count(&self, text: &str) -> usize;
}

/// A simple heuristic token counter that estimates tokens as characters divided by 4.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeuristicCounter;

impl TokenCounter for HeuristicCounter {
    fn count(&self, text: &str) -> usize {
        text.chars().count().div_ceil(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heuristic_counter() {
        let counter = HeuristicCounter;
        assert_eq!(counter.count(""), 0);
        assert_eq!(counter.count("abcd"), 1);
        assert_eq!(counter.count("abcdefgh"), 2);
        assert!(counter.count("abcdefghij") > 0);
    }
}
