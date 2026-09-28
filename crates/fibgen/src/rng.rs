//! A small deterministic PRNG (SplitMix64). The generator needs nothing
//! stronger: programs must be reproducible from a seed, not unpredictable,
//! so no external randomness crate is used.

/// SplitMix64 (Steele, Lea, Flood 2014): one `u64` of state, full period.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A generator whose whole output is fixed by `seed`.
    pub fn new(seed: u64) -> Self {
        Rng { state: seed }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`; `0` when `n` is `0`.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

    /// A number in `lo..=hi`.
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = (hi - lo + 1).max(1) as usize;
        lo + self.below(span) as i64
    }

    /// True with probability `pct` percent.
    pub fn chance(&mut self, pct: usize) -> bool {
        self.below(100) < pct
    }

    /// One element of a non-empty slice, or `None` for an empty one.
    pub fn pick<'a, T>(&mut self, xs: &'a [T]) -> Option<&'a T> {
        if xs.is_empty() {
            None
        } else {
            Some(&xs[self.below(xs.len())])
        }
    }

    /// An index chosen with probability proportional to its weight;
    /// `None` when every weight is zero.
    pub fn weighted(&mut self, weights: &[usize]) -> Option<usize> {
        let total: usize = weights.iter().sum();
        if total == 0 {
            return None;
        }
        let mut r = self.below(total);
        for (i, w) in weights.iter().enumerate() {
            if r < *w {
                return Some(i);
            }
            r -= w;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn below_stays_in_range() {
        let mut r = Rng::new(1);
        assert!((0..1000).all(|_| r.below(5) < 5));
        assert_eq!(r.below(0), 0);
    }

    #[test]
    fn weighted_skips_zero_weights() {
        let mut r = Rng::new(3);
        assert!((0..200).all(|_| r.weighted(&[0, 4, 0]) == Some(1)));
        assert_eq!(r.weighted(&[0, 0]), None);
    }
}
