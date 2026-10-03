// Rust twin of lazy-fused, lazy-bound, strings, dispatch, recursion, binary-trees:
// `misc lazy-fused|lazy-bound|strings|dispatch|recursion|binary-trees`. rustc -O misc.rs
// lazy-fused is a plain iterator chain; lazy-bound materialises each stage into a Vec (the nearest Rust shape to a memoised seq).

enum Tree { Leaf, Node(Box<Tree>, Box<Tree>) }
fn make(d: i64) -> Tree {
    if d == 0 { Tree::Node(Box::new(Tree::Leaf), Box::new(Tree::Leaf)) } else { Tree::Node(Box::new(make(d - 1)), Box::new(make(d - 1))) }
}
fn check(t: &Tree) -> i64 { match t { Tree::Leaf => 0, Tree::Node(l, r) => 1 + check(l) + check(r) } }

fn fib(n: i64) -> i64 { if n < 2 { n } else { fib(n - 1) + fib(n - 2) } }
fn sumto(n: i64, acc: i64) -> i64 { if n == 0 { acc } else { sumto(n - 1, (acc + n) % 1000003) } }

#[derive(Clone, Copy)]
enum Shape { Circle(i64), Rect(i64, i64), Tri(i64, i64) }
struct Sq(i64);
struct Rc(i64, i64);
trait Area { fn area(&self) -> i64; }
impl Area for Shape {
    fn area(&self) -> i64 {
        match *self { Shape::Circle(r) => 3 * (r * r), Shape::Rect(w, h) => w * h, Shape::Tri(b, h) => (b * h) / 2 }
    }
}
impl Area for Sq { fn area(&self) -> i64 { self.0 * self.0 } }
impl Area for Rc { fn area(&self) -> i64 { self.0 * self.1 } }
fn mk(i: i64) -> Shape {
    if i % 3 == 0 { Shape::Circle(1 + i % 7) } else if i % 3 == 1 { Shape::Rect(1 + i % 5, 2 + i % 3) } else { Shape::Tri(2 + i % 9, 1 + i % 4) }
}
fn adder(k: i64) -> Box<dyn Fn(i64) -> i64> { Box::new(move |x| x + k) }
fn twice(f: Box<dyn Fn(i64) -> i64>) -> Box<dyn Fn(i64) -> i64> { Box::new(move |x| f(f(x))) }

fn strings() {
    let mut g = String::new();
    for i in 0..20000i64 { g.push_str("ab"); g.push_str(&(i % 10).to_string()); }
    let mut nums = 0i64;
    for i in 0..1_000_000i64 { nums += format!("item-{}:{}", i, i * 2).chars().count() as i64; }
    let line = (0..1_000_000i64).map(|i| ((i * 37) % 1000).to_string()).collect::<Vec<_>>().join(",");
    let parts: Vec<&str> = line.split(',').collect();
    let total: usize = parts.iter().map(|p| p.chars().count()).sum();
    let rejoined = parts.join(";");
    let (mut from, mut acc) = (0usize, 0usize);
    for _ in 0..2000 {
        match g[from..].find("9ab") {
            Some(k) => { acc += from + k; from = from + k + 1; }
            None => { from = 0; }
        }
    }
    println!("{} {} {} {} {} {} {}", g.chars().count(), nums, line.chars().count(), parts.len(), total, rejoined.chars().count(), acc);
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("lazy-fused") => {
            let a: i64 = (0..100_000_000i64).map(|x| (x * 3) % 1000).filter(|x| x % 2 == 0).sum();
            let (mut x, mut b) = (1i64, 0i64);
            for i in 0..20_000_010i64 { if i >= 10 { b += x; } x = (x * 5 + 1) % 1000003; }
            println!("{} {}", a, b);
        }
        Some("lazy-bound") => {
            let r: Vec<i64> = (0..5_000_000i64).collect();
            let m: Vec<i64> = r.iter().map(|x| (x * 3) % 1000).collect();
            let f: Vec<i64> = m.iter().copied().filter(|x| x % 2 == 0).collect();
            println!("{}", f.iter().sum::<i64>());
        }
        Some("strings") => strings(),
        Some("dispatch") => {
            let shapes: Vec<Shape> = (0..1000i64).map(mk).collect();
            let mut a = 0i64;
            for _ in 0..50000 { a += shapes.iter().fold(0i64, |s, x| s + x.area()); }
            let mut b = 0i64;
            for i in 0..50_000_000i64 { b += Sq(i % 100).area() + Rc(i % 7, 3).area(); }
            let f = twice(twice(adder(3)));
            let mut c = 0i64;
            for _ in 0..100_000_000i64 { c = f(c) % 1000003; }
            println!("{} {} {}", a, b, c);
        }
        Some("recursion") => {
            let a = fib(44);
            let mut b = 0i64;
            for _ in 0..30000 { b += sumto(1000, 0); }
            println!("{} {}", a, b);
        }
        Some("binary-trees") => {
            let maxd = 18;
            let stretch = check(&make(maxd + 1));
            let long_lived = make(maxd);
            let mut total = 0i64;
            let mut d = 4;
            while d <= maxd {
                let iters = 1i64 << (maxd - d + 4);
                let mut s = 0i64;
                for _ in 0..iters { s += check(&make(d)); }
                total += s;
                d += 2;
            }
            println!("{} {} {}", stretch, total, check(&long_lived));
        }
        _ => eprintln!("usage: misc lazy-fused|lazy-bound|strings|dispatch|recursion|binary-trees"),
    }
}
