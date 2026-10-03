// Rust twin of vec-conj-pop.fib, vec-assoc.fib, vec-index.fib, vec-sort.fib: `vec conj-pop|assoc|index|sort`. rustc -O vec.rs
fn rnd(n: usize) -> Vec<i64> {
    let (mut x, mut v) = (42i64, Vec::new());
    for _ in 0..n { x = (x * 1103515245 + 12345) % 2147483648; v.push(x); }
    v
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("conj-pop") => {
            let n = 20_000_000i64;
            let mut v: Vec<i64> = Vec::new();
            for i in 0..n { v.push((i * 31) % 1009); }
            let cnt = v.len();
            let mut acc = 0i64;
            while let Some(&t) = v.last() { acc += t; v.pop(); }
            println!("{} {}", cnt, acc);
        }
        Some("assoc") => {
            let n = 100000i64;
            let mut v: Vec<i64> = (0..n).collect();
            let mut x = 12345i64;
            for i in 0..10_000_000i64 {
                x = (x * 1103515245 + 12345) % 2147483648;
                v[(x % n) as usize] = i;
            }
            println!("{} {}", v.len(), v.iter().sum::<i64>());
        }
        Some("index") => {
            let n = 100000i64;
            let v: Vec<i64> = (0..n).map(|i| (i * 7) % 1000).collect();
            let (mut x, mut acc) = (777i64, 0i64);
            for _ in 0..100_000_000i64 {
                x = (x * 1103515245 + 12345) % 2147483648;
                acc += v[(x % n) as usize];
            }
            println!("{} {}", v.len(), acc);
        }
        Some("sort") => {
            let mut a = rnd(2_000_000);
            a.sort();
            let mut b = rnd(1_000_000);
            b.sort_by_key(|x| x % 1000);
            let mut c = rnd(1_000_000);
            c.sort_by(|p, q| q.cmp(p));
            println!("{} {} {} {} {} {}", a[0], a[1999999], b[0], b[999999], c[0], c[999999]);
        }
        _ => eprintln!("usage: vec conj-pop|assoc|index|sort"),
    }
}
