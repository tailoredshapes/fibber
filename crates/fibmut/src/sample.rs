//! The seeded sample of the mutants: a shuffle by a fixed generator, so
//! that a seed names one sample and a rerun reproduces it.

/// xorshift64* from a scrambled seed (splitmix64's mixer), never zero.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Rng(if z == 0 { 1 } else { z })
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..n`, `n` not zero.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// `max` of `all`, chosen by a shuffle under `seed`, in their original
/// order; all of them when there are no more than `max`.
pub fn sample<T>(all: Vec<T>, max: usize, seed: u64) -> Vec<T> {
    if all.len() <= max {
        return all;
    }
    let mut rng = Rng::new(seed);
    let mut index: Vec<usize> = (0..all.len()).collect();
    for i in (1..index.len()).rev() {
        index.swap(i, rng.below(i + 1));
    }
    index.truncate(max);
    index.sort_unstable();
    let mut keep = index.into_iter().peekable();
    all.into_iter()
        .enumerate()
        .filter(|(i, _)| keep.next_if_eq(i).is_some())
        .map(|(_, x)| x)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_names_one_sample() {
        let all: Vec<usize> = (0..200).collect();
        let a = sample(all.clone(), 20, 1);
        assert_eq!(a, sample(all.clone(), 20, 1));
        assert_ne!(a, sample(all.clone(), 20, 2));
    }

    #[test]
    fn the_sample_is_capped_ordered_and_without_repeats() {
        let all: Vec<usize> = (0..200).collect();
        let s = sample(all, 20, 7);
        assert_eq!(s.len(), 20);
        assert!(s.windows(2).all(|w| w[0] < w[1]), "{s:?}");
    }

    #[test]
    fn a_small_input_is_kept_whole() {
        assert_eq!(sample(vec![3, 1, 2], 10, 1), vec![3, 1, 2]);
        assert_eq!(sample(vec![3, 1, 2], 3, 1), vec![3, 1, 2]);
        assert_eq!(sample(Vec::<u8>::new(), 0, 1), Vec::<u8>::new());
    }

    #[test]
    fn the_generator_is_a_fixed_sequence() {
        let mut r = Rng::new(0);
        let first: Vec<u64> = (0..3).map(|_| r.next()).collect();
        let mut again = Rng::new(0);
        assert_eq!(first, (0..3).map(|_| again.next()).collect::<Vec<u64>>());
        assert!(first.iter().all(|&x| x != 0));
        assert_ne!(Rng::new(1).next(), Rng::new(2).next());
    }

    #[test]
    fn every_element_can_be_chosen() {
        let mut hit = [false; 10];
        for seed in 0..60 {
            for x in sample((0..10).collect(), 3, seed) {
                hit[x] = true;
            }
        }
        assert!(hit.iter().all(|&h| h), "{hit:?}");
    }
}
