// Rust twin of map-assoc-get.fib, set-conj.fib, set-disj.fib: `coll map|set-conj|set-disj`. rustc -O coll.rs
// std HashMap/HashSet (SipHash 1-3); iteration order is not used, so checksums agree.
use std::collections::{HashMap, HashSet};

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("map") => {
            let n = 1_000_000i64;
            let mut m: HashMap<i64, i64> = HashMap::new();
            for i in 0..n { m.insert(i * 3, i); }
            let cnt = m.len();
            let mut h = 0i64;
            for i in 0..2_000_000i64 { h += m.get(&i).copied().unwrap_or(1); }
            for i in 0..n { m.insert(i * 3, i + 1); }
            println!("{} {} {} {}", cnt, h, m[&2997], m.len());
        }
        Some("set-conj") => {
            let n = 1_000_000i64;
            let mut s: HashSet<i64> = HashSet::new();
            for i in 0..n { s.insert((i * 7919) % 1000003); }
            let mut h = 0i64;
            for i in 0..n { if s.contains(&i) { h += 1; } }
            println!("{} {}", s.len(), h);
        }
        Some("set-disj") => {
            let n = 20000i64;
            let mut s: HashSet<i64> = HashSet::new();
            for i in 0..n { s.insert((i * 7919) % 1000003); }
            let c1 = s.len();
            let mut i = 0;
            while i < n { s.remove(&((i * 7919) % 1000003)); i += 2; }
            println!("{} {} {}", c1, s.len(), s.iter().sum::<i64>());
        }
        _ => eprintln!("usage: coll map|set-conj|set-disj"),
    }
}
