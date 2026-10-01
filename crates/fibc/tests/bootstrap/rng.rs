//! The deterministic generator of the fuzz inputs: the xorshift of
//! `crates/fibref/src/syntax/tests/gen.rs`, driven by a seed, no external
//! crates, so the same seed gives the same inputs on every machine.

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// A number in `0..n`; `n` must not be 0.
    pub fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).expect("below n fits usize")
    }

    /// A number in `lo..=hi`.
    pub fn between(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }

    /// True once in `n` on average.
    pub fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }

    /// An index into `weights`, each chosen in proportion to its weight.
    pub fn weighted(&mut self, weights: &[usize]) -> usize {
        let mut roll = self.below(weights.iter().sum());
        for (i, w) in weights.iter().enumerate() {
            if roll < *w {
                return i;
            }
            roll -= w;
        }
        weights.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_numbers_and_another_seed_does_not() {
        let run = |seed| {
            let mut r = Rng::new(seed);
            (0..8).map(|_| r.next()).collect::<Vec<_>>()
        };
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }

    #[test]
    fn the_first_numbers_of_a_seed_are_fixed() {
        // Pinned so a change to the generator, which would change every
        // fuzz input, is noticed.
        let mut r = Rng::new(1);
        assert_eq!(r.next(), 0xdc1b_77ae_0bf3_4dad);
    }

    #[test]
    fn below_between_and_weighted_stay_in_range() {
        let mut r = Rng::new(3);
        for _ in 0..1000 {
            assert!(r.below(7) < 7);
            assert!((5..=9).contains(&r.between(5, 9)));
            assert!(r.weighted(&[3, 0, 1]) != 1);
        }
    }

    #[test]
    fn weighted_reaches_every_nonzero_weight() {
        let mut r = Rng::new(4);
        let mut seen = [false; 3];
        for _ in 0..200 {
            seen[r.weighted(&[1, 5, 1])] = true;
        }
        assert_eq!(seen, [true; 3]);
    }
}
