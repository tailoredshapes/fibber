//! The compiled `show` of a float is the interpreter's text (types
//! §2.12, method rule 6): the shortest decimal that reads back at the
//! value's width, the nearest to the value among those of that length,
//! an exact tie going up. The interpreter's text is Rust's `{}` (eval/
//! arith.rs `float_text`); the runtime's is `fib.show-fp` in
//! `rt/str.lir`, which prints through libc.
//!
//! Rust writes a seeded list of values as `{:e}` text that reads back
//! exactly, a fibber program built with `fibc build` reads each line with
//! `strtod` or `strtof` and prints `(show x)`, and every line is compared
//! with `float_text` of the same value. The values are chosen where a
//! shortest-digits routine goes wrong: both neighbours of every binade
//! boundary, exact ties `k / 2^m`, values whose shortest spelling has the
//! most digits, the 1e15..1e22 band, subnormals, the extremes and a
//! random sample of bit patterns; every one with both signs.

use std::path::{Path, PathBuf};
use std::process::Command;

use fibref::eval::arith::float_text;
use fibref::types::ty::Scalar;

/// Reads one float per line of `argv[1]` as `argv[0]` (`f64` or `f32`)
/// and prints `(show x)` for each.
const PROGRAM: &str = r#"
(extern strtod :private (ptr ptr) -> f64)
(extern strtof :private (ptr ptr) -> f32)

(defun parse-f64 (s: str) -> f64
  (let ((bs (str-bytes s)))
    (unsafe
      (let ((p (alloc (+ (array-len bs) 1))))
        (dotimes (i (array-len bs)) (store-i8 (ptr+ p i) (array-get bs i)))
        (store-i8 (ptr+ p (array-len bs)) 0i8)
        (let ((e (alloc 8))
              (r (strtod p e)))
          (do (free e) (free p) r))))))

(defun parse-f32 (s: str) -> f32
  (let ((bs (str-bytes s)))
    (unsafe
      (let ((p (alloc (+ (array-len bs) 1))))
        (dotimes (i (array-len bs)) (store-i8 (ptr+ p i) (array-get bs i)))
        (store-i8 (ptr+ p (array-len bs)) 0i8)
        (let ((e (alloc 8))
              (r (strtof p e)))
          (do (free e) (free p) r))))))

(defun show-line (line: str single: bool) -> unit
  (if single (println (show (parse-f32 line))) (println (show (parse-f64 line)))))

(defun show-lines (text: str single: bool) -> i64
  (let ((bs (str-bytes text)))
    (loop ((i 0) (from 0) (n 0))
      (if (= i (array-len bs))
          n
          (if (= (array-get bs i) 10i8)
              (do (show-line (str-slice text from i) single)
                  (recur (+ i 1) (+ i 1) (+ n 1)))
              (recur (+ i 1) from n))))))

(defun main () -> i64
  (let ((argv (args)))
    (match (read-file (nth argv 1))
      ((some text) (show-lines text (= (nth argv 0) "f32")))
      (nil -1))))
"#;

/// splitmix64: a seeded stream that needs no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A float width, and the bit patterns of its values (held in a `u64`).
#[derive(Clone, Copy)]
enum Width {
    F64,
    F32,
}

impl Width {
    fn name(self) -> &'static str {
        match self {
            Width::F64 => "f64",
            Width::F32 => "f32",
        }
    }

    fn mant_bits(self) -> u32 {
        match self {
            Width::F64 => 52,
            Width::F32 => 23,
        }
    }

    /// The biased exponent of 1.0, and of infinity and NaN.
    fn bias(self) -> u64 {
        match self {
            Width::F64 => 1023,
            Width::F32 => 127,
        }
    }

    fn sign(self) -> u64 {
        1 << (self.mant_bits() + self.exp_bits())
    }

    fn exp_bits(self) -> u32 {
        match self {
            Width::F64 => 11,
            Width::F32 => 8,
        }
    }

    /// The value of a bit pattern; exact for `f32`.
    fn value(self, bits: u64) -> f64 {
        match self {
            Width::F64 => f64::from_bits(bits),
            Width::F32 => f64::from(f32::from_bits(bits as u32)),
        }
    }

    /// The bit pattern of a value that the width holds exactly or that
    /// is to be rounded to it.
    fn bits(self, v: f64) -> u64 {
        match self {
            Width::F64 => v.to_bits(),
            Width::F32 => u64::from((v as f32).to_bits()),
        }
    }

    /// The text the fibber program reads back to the value exactly.
    fn input(self, bits: u64) -> String {
        match self {
            Width::F64 => format!("{:.16e}", f64::from_bits(bits)),
            Width::F32 => format!("{:.8e}", f32::from_bits(bits as u32)),
        }
    }

    /// The interpreter's `show` of the value.
    fn expected(self, bits: u64) -> String {
        match self {
            Width::F64 => float_text(f64::from_bits(bits), Scalar::F64),
            Width::F32 => float_text(self.value(bits), Scalar::F32),
        }
    }

    fn finite(self, bits: u64) -> bool {
        (bits >> self.mant_bits()) & ((1 << self.exp_bits()) - 1) != (1 << self.exp_bits()) - 1
    }

    /// The correctly rounded value of `digits` e `exp`, as bits.
    fn parse(self, digits: u64, exp: i64) -> u64 {
        let text = format!("{digits}e{exp}");
        match self {
            Width::F64 => text.parse::<f64>().map_or(0, f64::to_bits),
            Width::F32 => text.parse::<f32>().map_or(0, |v| u64::from(v.to_bits())),
        }
    }
}

/// The bit patterns around every binade boundary: the value just below,
/// at and just above the first and the last of each exponent.
fn boundaries(w: Width, out: &mut Vec<u64>) {
    let top = (1u64 << w.exp_bits()) - 1;
    for e in 0..top {
        let first = e << w.mant_bits();
        let last = first + (1 << w.mant_bits()) - 1;
        out.extend([first.wrapping_sub(1), first, first + 1, last - 1, last]);
    }
}

/// `k / 2^m` for every odd `k` below 128 and `m` up to 60, and for
/// random odd `k` of up to a full mantissa: the exact decimal ties.
fn ties(w: Width, rng: &mut Rng, out: &mut Vec<u64>) {
    let mut push = |k: u64, m: i32| out.push(w.bits(k as f64 * 2f64.powi(-m)));
    for k in (1..128).step_by(2) {
        for m in 0..=60 {
            push(k, m);
        }
    }
    for _ in 0..600 {
        let k = (rng.next() >> (64 - w.mant_bits() - 1)) | 1;
        push(k, rng.below(61) as i32);
    }
}

/// Decimal spellings of 15, 16 and 17 digits (6 to 9 for `f32`) at random
/// exponents, rounded to the width: the values whose shortest text is
/// longest.
fn long_spellings(w: Width, rng: &mut Rng, out: &mut Vec<u64>) {
    let (lo, span, reach) = match w {
        Width::F64 => (15, 3, 300),
        Width::F32 => (6, 4, 37),
    };
    for _ in 0..1500 {
        let n = lo + rng.below(span) as u32;
        let d = 10u64.pow(n - 1) + rng.below(9 * 10u64.pow(n - 1));
        out.push(w.parse(d, rng.below(2 * reach) as i64 - reach as i64));
    }
}

/// The 1e15..1e22 band: each power of ten, its neighbours, and random
/// values of the band.
fn big_band(w: Width, rng: &mut Rng, out: &mut Vec<u64>) {
    for k in 15..=22 {
        let b = w.parse(1, k);
        out.extend((0..7).map(|i| b - 3 + i));
        for _ in 0..100 {
            out.push(w.parse(
                1 + rng.below(9_999_999_999_999_999),
                k - 16 + rng.below(8) as i64,
            ));
        }
    }
}

/// Subnormals, mid-range exponents, and any bit pattern at all.
fn random_bits(w: Width, rng: &mut Rng, out: &mut Vec<u64>) {
    let mant = (1u64 << w.mant_bits()) - 1;
    let magnitude = w.sign() - 1;
    for _ in 0..800 {
        out.push(rng.next() & mant);
        let e = w.bias() - 40 + rng.below(81);
        out.push((e << w.mant_bits()) | (rng.next() & mant));
        out.push(rng.next() & magnitude);
    }
}

/// The extremes, the small decimals, the integers and the powers of ten.
fn named(w: Width, rng: &mut Rng, out: &mut Vec<u64>) {
    let mant = 1u64 << w.mant_bits();
    let top = ((1u64 << w.exp_bits()) - 1) << w.mant_bits();
    out.extend([
        0,
        1,
        mant - 1,
        mant,
        top - 1,
        top,
        top + (1 << (w.mant_bits() - 1)),
    ]);
    for v in [
        0.1, 0.2, 0.3, 0.5, 1.0, 1.5, 2.5, 3.0, 100.0, 0.7, 5e-324, 1e21,
    ] {
        out.push(w.bits(v));
    }
    out.extend((0..2000).map(|i| w.bits(f64::from(i))));
    out.extend((0..300).map(|_| w.bits(rng.below(1 << 53) as f64)));
    for k in -45..=45 {
        out.push(w.parse(1, k));
    }
}

/// The bit patterns to test at a width: every kind above, both signs,
/// the non-finite ones removed and then put back as the three special
/// values, each once.
fn bit_patterns(w: Width, seed: u64) -> Vec<u64> {
    let mut rng = Rng(seed);
    let mut all = Vec::new();
    boundaries(w, &mut all);
    ties(w, &mut rng, &mut all);
    long_spellings(w, &mut rng, &mut all);
    big_band(w, &mut rng, &mut all);
    random_bits(w, &mut rng, &mut all);
    named(w, &mut rng, &mut all);
    let mut kept: Vec<u64> = all.into_iter().filter(|&b| w.finite(b)).collect();
    kept.extend(kept.clone().iter().map(|b| b | w.sign()));
    kept.sort_unstable();
    kept.dedup();
    let inf = ((1u64 << w.exp_bits()) - 1) << w.mant_bits();
    kept.extend([inf, inf | w.sign(), inf | (1 << (w.mant_bits() - 1))]);
    kept
}

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fibc-floats-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp dir");
    dir
}

/// The executable that shows the floats of a file.
fn build(dir: &Path) -> PathBuf {
    let src = dir.join("show.fib");
    let exe = dir.join("show");
    std::fs::write(&src, PROGRAM).expect("the program is written");
    let built = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .args(["build"])
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("fibc builds");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    exe
}

/// Compares the compiled `show` of every pattern with the interpreter's
/// and returns the number compared and the lines that differ.
fn compare(dir: &Path, exe: &Path, w: Width, seed: u64) -> (usize, Vec<String>) {
    let patterns = bit_patterns(w, seed);
    let input = dir.join(format!("{}.txt", w.name()));
    let text: String = patterns.iter().map(|&b| w.input(b) + "\n").collect();
    std::fs::write(&input, text).expect("the input is written");
    let out = Command::new(exe)
        .args([w.name()])
        .arg(&input)
        .output()
        .expect("the program runs");
    let shown = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = shown.lines().collect();
    assert_eq!(
        lines.len(),
        patterns.len(),
        "the program printed {} of {} values: {}",
        lines.len(),
        patterns.len(),
        String::from_utf8_lossy(&out.stderr)
    );
    let bad = patterns
        .iter()
        .zip(&lines)
        .filter(|(&b, line)| w.expected(b) != **line)
        .map(|(&b, line)| {
            format!(
                "{}: interpreter {} compiled {}",
                w.input(b),
                squeeze(&w.expected(b)),
                squeeze(line)
            )
        })
        .collect();
    (patterns.len(), bad)
}

/// A text with a long run of zeros after the point written as `0*N`, so
/// a report about a subnormal fits a line.
fn squeeze(text: &str) -> String {
    let Some((head, tail)) = text.split_once("0.") else {
        return text.to_string();
    };
    let zeros = tail.len() - tail.trim_start_matches('0').len();
    if zeros < 8 {
        return text.to_string();
    }
    format!("{head}0.0*{zeros}{}", &tail[zeros..])
}

#[test]
fn the_compiled_show_of_a_float_is_the_interpreters() {
    let dir = scratch();
    let exe = build(&dir);
    let mut failures = Vec::new();
    for (w, seed) in [(Width::F64, 0x5EED_0064), (Width::F32, 0x5EED_0032)] {
        let (n, bad) = compare(&dir, &exe, w, seed);
        println!(
            "{}: {n} values compared, {} identical",
            w.name(),
            n - bad.len()
        );
        assert!(n >= 4000, "{}: only {n} values", w.name());
        if !bad.is_empty() {
            let first: Vec<&String> = bad.iter().take(10).collect();
            failures.push(format!(
                "{}: {} of {n} differ, the first ten:\n{}",
                w.name(),
                bad.len(),
                first
                    .iter()
                    .map(|l| l.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let _ = std::fs::remove_dir_all(&dir);
}
