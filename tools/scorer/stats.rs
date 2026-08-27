//! Rates with Wilson score intervals: small n, proportions with confidence
//! intervals rather than p-values (go-no-go.md §8).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rate {
    pub hits: usize,
    pub n: usize,
    pub rate: f64,
    pub ci_low: f64,
    pub ci_high: f64,
}

impl Rate {
    pub fn new(hits: usize, n: usize) -> Rate {
        if n == 0 {
            return Rate {
                hits,
                n,
                rate: f64::NAN,
                ci_low: f64::NAN,
                ci_high: f64::NAN,
            };
        }
        let (low, high) = wilson(hits, n, 1.96);
        Rate {
            hits,
            n,
            rate: hits as f64 / n as f64,
            ci_low: low,
            ci_high: high,
        }
    }
}

/// 95% Wilson score interval for `hits` successes in `n` trials.
pub fn wilson(hits: usize, n: usize, z: f64) -> (f64, f64) {
    if n == 0 {
        return (f64::NAN, f64::NAN);
    }
    let n_f = n as f64;
    let p = hits as f64 / n_f;
    let z2 = z * z;
    let denominator = 1.0 + z2 / n_f;
    let centre = (p + z2 / (2.0 * n_f)) / denominator;
    let half = z * ((p * (1.0 - p) / n_f) + z2 / (4.0 * n_f * n_f)).sqrt() / denominator;
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wilson_matches_known_values() {
        // 0/20 -> upper bound ~0.161; 20/20 -> lower bound ~0.839.
        let (low, high) = wilson(0, 20, 1.96);
        assert_eq!(low, 0.0);
        assert!((high - 0.1611).abs() < 0.002, "{high}");
        let (low, high) = wilson(20, 20, 1.96);
        assert!((low - 0.8389).abs() < 0.002, "{low}");
        assert_eq!(high, 1.0);
        // 10/20 is symmetric around 0.5.
        let (low, high) = wilson(10, 20, 1.96);
        assert!((low + high - 1.0).abs() < 1e-9);
        assert!((low - 0.299).abs() < 0.002, "{low}");
    }

    #[test]
    fn rate_handles_empty_samples() {
        let r = Rate::new(0, 0);
        assert!(r.rate.is_nan());
        let r = Rate::new(3, 4);
        assert_eq!(r.rate, 0.75);
        assert!(r.ci_low < 0.75 && r.ci_high > 0.75);
    }
}
