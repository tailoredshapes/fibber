//! A fast hasher for the evaluator's maps, whose keys are small ids
//! (expressions, bindings, objects): the multiply-rotate hash of rustc's
//! `FxHasher`. Not collision resistant, which ids chosen by the checker
//! do not need.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

const K: u64 = 0x517c_c1b7_2722_0a95;

/// The hasher.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fx(u64);

impl Fx {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(K);
    }
}

impl Hasher for Fx {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.add(u64::from(*b));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// A map with the fast hasher.
pub type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<Fx>>;
/// A set with the fast hasher.
pub type FxSet<K> = HashSet<K, BuildHasherDefault<Fx>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_small_ids_hash_apart() {
        let mut m: FxMap<u32, u32> = FxMap::default();
        for i in 0..1000 {
            m.insert(i, i * 2);
        }
        assert_eq!(m.len(), 1000);
        assert_eq!(m.get(&999), Some(&1998));
    }
}
