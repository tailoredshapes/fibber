// Rust twin of num-i64.fib, num-f64.fib, num-nbody.fib: `num.rs i64|f64|nbody`. rustc -O num.rs
#[derive(Clone, Copy)]
struct Body { x: f64, y: f64, vx: f64, vy: f64, m: f64 }

fn accel(bs: &[Body], i: usize) -> (f64, f64) {
    let b = bs[i];
    let (mut ax, mut ay) = (0.0, 0.0);
    for j in 0..5 {
        if j == i { continue; }
        let o = bs[j];
        let dx = o.x - b.x;
        let dy = o.y - b.y;
        let d2 = (dx * dx + dy * dy) + 0.01;
        let f = o.m / (d2 * (d2 + 1.0));
        ax += f * dx;
        ay += f * dy;
    }
    (ax, ay)
}

fn step(bs: &Vec<Body>) -> Vec<Body> {
    let mut out = bs.clone();
    for i in 0..5 {
        let a = accel(bs, i);
        let b = bs[i];
        let vx = b.vx + 0.001 * a.0;
        let vy = b.vy + 0.001 * a.1;
        out[i] = Body { x: b.x + 0.001 * vx, y: b.y + 0.001 * vy, vx, vy, m: b.m };
    }
    out
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("i64") => {
            let (mut i, mut acc) = (0i64, 0i64);
            while i < 1_000_000_000 { acc = (acc * 31 + i) % 1000003; i += 1; }
            println!("{}", acc);
        }
        Some("f64") => {
            let (mut i, mut acc) = (0i64, 0.0f64);
            while i < 2_000_000_000 { acc += 1.0 / ((i as f64) + 1.0); i += 1; }
            println!("{}", (acc * 1000000.0) as i64);
        }
        Some("nbody") => {
            let mut bs = vec![
                Body { x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, m: 10.0 },
                Body { x: 1.0, y: 0.0, vx: 0.0, vy: 1.0, m: 1.0 },
                Body { x: 0.0, y: 2.0, vx: -1.0, vy: 0.0, m: 1.0 },
                Body { x: -3.0, y: 0.0, vx: 0.0, vy: -0.5, m: 0.5 },
                Body { x: 0.0, y: -4.0, vx: 0.5, vy: 0.0, m: 0.2 },
            ];
            for _ in 0..4_000_000 { bs = step(&bs); }
            println!("{}", (1000.0 * (bs[1].x + bs[2].y)) as i64);
        }
        _ => eprintln!("usage: num i64|f64|nbody"),
    }
}
