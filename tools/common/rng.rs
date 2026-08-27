//! Tiny deterministic PRNG (SplitMix64) so generated materials and run
//! orderings are reproducible from a seed without pulling in a crate.

#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`; `n == 0` yields 0.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// Uniform in `lo..=hi`.
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi.saturating_sub(lo) + 1)
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    pub fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    /// Fisher–Yates, in place.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }

    /// A child generator for an independent stream (e.g. one per repo).
    pub fn fork(&mut self, label: &str) -> Rng {
        let mut h = self.next_u64();
        for b in label.bytes() {
            h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
        Rng(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_differ() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn range_is_inclusive_and_bounded() {
        let mut rng = Rng::new(7);
        let mut seen = [false; 4];
        for _ in 0..500 {
            let v = rng.range(3, 6);
            assert!((3..=6).contains(&v));
            seen[v - 3] = true;
        }
        assert!(seen.iter().all(|s| *s));
        assert_eq!(rng.below(0), 0);
        assert_eq!(rng.range(5, 5), 5);
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut rng = Rng::new(3);
        let mut items: Vec<u32> = (0..20).collect();
        rng.shuffle(&mut items);
        let mut sorted = items.clone();
        sorted.sort();
        assert_eq!(sorted, (0..20).collect::<Vec<_>>());
        assert_ne!(items, sorted, "20 items should not stay in order");
    }

    #[test]
    fn forks_are_label_dependent_and_deterministic() {
        let mut root_a = Rng::new(9);
        let mut root_b = Rng::new(9);
        assert_eq!(root_a.fork("x").next_u64(), root_b.fork("x").next_u64());
        let mut root_c = Rng::new(9);
        let mut root_d = Rng::new(9);
        assert_ne!(root_c.fork("x").next_u64(), root_d.fork("y").next_u64());
    }
}
