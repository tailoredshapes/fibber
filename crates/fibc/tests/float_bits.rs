//! The float bit casts `f64->bits`, `bits->f64`, `f32->bits` and
//! `bits->f32` (syntax §4.3, types §2.12) are the same in the reference
//! interpreter and compiled (method rule 6), and both are Rust's
//! `to_bits` and `from_bits`.
//!
//! One fibber program draws patterns from a xorshift64 generator (so
//! nothing is read or printed), casts each at both widths, in a raw form
//! and with the exponent forced to all ones (NaNs and infinities, whose
//! payloads must survive), and folds into one `i64` both the bits it gets
//! back and the bits of `x + 1` where `x` is not a NaN (that ties the
//! pattern to the number it names; the sum of a NaN is the hardware's to
//! choose). Rust folds the same sequence with `f64::from_bits` and
//! `f32::from_bits`. The interpreter, the JIT and the expected value must
//! all give one number.

#[path = "../../fibref/tests/io_support/mod.rs"]
mod io_support;

use std::process::Command;

use fibc::harness::interp;
use fibref::cases::{Outcome, Value};
use io_support::TempDir;

const SEED: u64 = 88_172_645_463_325_252;
const COUNT: u64 = 3000;

fn program() -> String {
    format!(
        r#"
(defun next (x: i64) -> i64
  (let ((a (bit-xor x (shl x 13))))
    (let ((b (bit-xor a (shr a 7))))
      (bit-xor b (shl b 17)))))

(defun mix (acc: i64 v: i64) -> i64
  (bit-xor (bit-or (shl acc 5) (shr acc 59)) v))

(defun ones64 (n: i64) -> i64
  (bit-or (bit-and n (bit-not 0x7FF0000000000000)) 0x7FF0000000000000))

(defun ones32 (n: i32) -> i32
  (bit-or (bit-and n (bit-not 0x7F800000i32)) 0x7F800000i32))

(defun step64 (acc: i64 n: i64) -> i64
  (let ((x (bits->f64 n)))
    (mix (mix acc (f64->bits x)) (if (!= x x) 0 (f64->bits (+ x 1.0))))))

(defun step32 (acc: i64 n: i32) -> i64
  (let ((x (bits->f32 n)))
    (mix (mix acc (sext i64 (f32->bits x)))
         (if (!= x x) 0 (sext i64 (f32->bits (+ x 1.0f32)))))))

(defun main () -> i64
  (loop ((i 0) (x {SEED}) (acc 0))
    (if (< i {COUNT})
        (let ((y (next x)))
          (let ((n (trunc i32 (shr y 17))))
            (recur (+ i 1) y
                   (step32 (step32 (step64 (step64 acc y) (ones64 y)) n) (ones32 n)))))
        acc)))
"#
    )
}

fn next(x: u64) -> u64 {
    let a = x ^ (x << 13);
    let b = a ^ (a >> 7);
    b ^ (b << 17)
}

fn mix(acc: u64, v: u64) -> u64 {
    acc.rotate_left(5) ^ v
}

fn step64(acc: u64, n: u64) -> u64 {
    let x = f64::from_bits(n);
    let sum = if x.is_nan() { 0 } else { (x + 1.0).to_bits() };
    mix(mix(acc, x.to_bits()), sum)
}

fn step32(acc: u64, n: u32) -> u64 {
    let x = f32::from_bits(n);
    let sext = |b: u32| i64::from(b as i32) as u64;
    let sum = if x.is_nan() {
        0
    } else {
        sext((x + 1.0).to_bits())
    };
    mix(mix(acc, sext(x.to_bits())), sum)
}

/// The fold Rust computes of the same sequence, as `main` returns it.
fn expected() -> i64 {
    let (mut x, mut acc) = (SEED, 0u64);
    for _ in 0..COUNT {
        x = next(x);
        let n = (x >> 17) as u32;
        let forced = |n: u32| (n & !0x7F80_0000) | 0x7F80_0000;
        acc = step64(acc, x);
        acc = step64(acc, (x & !0x7FF0_0000_0000_0000) | 0x7FF0_0000_0000_0000);
        acc = step32(acc, n);
        acc = step32(acc, forced(n));
    }
    acc as i64
}

#[test]
fn the_casts_are_to_bits_and_from_bits_in_the_interpreter_and_compiled() {
    let want = expected();
    let src = program();
    let run = interp::run(&src, "float-bits.fib");
    match run.outcome {
        Outcome::Compiled { result, audit } => {
            assert_eq!(result, Value::Int(want), "interpreter");
            assert!(audit.clean, "{audit}");
        }
        other => panic!("the interpreter did not run it: {other:?}"),
    }
    let dir = TempDir::new("float-bits");
    let file = dir.file("bits.fib", &src);
    let out = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("fibc runs");
    let text = String::from_utf8_lossy(&out.stdout);
    let last = text.lines().last().unwrap_or("");
    assert_eq!(
        last,
        want.to_string(),
        "compiled: {text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    println!("{} patterns at each width and form, fold {want}", COUNT);
}
