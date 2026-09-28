//! The JIT as a library (spec/lir.md §11): modules compiled in-process,
//! functions called through pointers, modules linked by `declare`.

use lair::{Error, Jit, JitOptions};

const MATH: &str = "
(define (square i64) ((i64 x)) (block entry (ret (mul x x))))
(define (fadd3 double) ((double a) (double b) (double c)) (block entry (ret (fadd (fadd a b) c))))
(global counter i64 (i64 0))
(define (bump i64) () (block entry
  (ret (add (atomicrmw add seq_cst @counter (i64 1)) (i64 1)))))";

#[test]
fn compiles_a_module_and_calls_its_functions() {
    let mut jit = Jit::new(JitOptions::default()).unwrap();
    jit.add_source("math", MATH).unwrap();
    let sig = jit.signature("square").unwrap();
    assert_eq!(sig.to_string(), "(fn i64 (i64))");
    // SAFETY: the types match the signatures just read.
    let square: extern "C" fn(i64) -> i64 = unsafe { jit.function("square").unwrap() };
    let fadd3: extern "C" fn(f64, f64, f64) -> f64 = unsafe { jit.function("fadd3").unwrap() };
    let bump: extern "C" fn() -> i64 = unsafe { jit.function("bump").unwrap() };
    assert_eq!(square(-12), 144);
    assert_eq!(fadd3(0.5, 0.25, 2.0), 2.75);
    assert_eq!((bump(), bump(), bump()), (1, 2, 3));
}

#[test]
fn a_later_module_calls_an_earlier_one_by_declare() {
    let mut jit = Jit::new(JitOptions { opt_level: 2 }).unwrap();
    jit.add_source("math", MATH).unwrap();
    let user = "(declare square i64 (i64))
      (declare strlen i64 (ptr))
      (define (f i64) ((i64 x)) (block entry
        (ret (add (call @square x) (call @strlen (string \"four\"))))))";
    jit.add_source("user", user).unwrap();
    // SAFETY: (fn i64 (i64)).
    let f: extern "C" fn(i64) -> i64 = unsafe { jit.function("f").unwrap() };
    assert_eq!(f(5), 29);
}

fn message(e: Error) -> String {
    e.to_string()
}

#[test]
fn cross_module_mismatches_are_errors_and_the_jit_stays_usable() {
    let mut jit = Jit::new(JitOptions::default()).unwrap();
    jit.add_source("math", MATH).unwrap();
    let bad_decl =
        "(declare square i32 (i32)) (define (g i32) () (block entry (ret (call @square (i32 2)))))";
    let e = message(jit.add_source("bad", bad_decl).unwrap_err());
    assert!(
        e.contains("declaration of @square does not match its definition in module math"),
        "{e}"
    );
    let dup = "(define (square i64) ((i64 x)) (block entry (ret x)))";
    let e = message(jit.add_source("dup", dup).unwrap_err());
    assert!(
        e.contains("duplicate definition of @square (first in module math)"),
        "{e}"
    );
    let invalid = "(define (h i32) () (block entry (ret (i64 1))))";
    let e = message(jit.add_source("invalid", invalid).unwrap_err());
    assert!(
        e.contains("ret type i64 does not match @h's result i32"),
        "{e}"
    );
    // SAFETY: (fn i64 (i64)).
    let square: extern "C" fn(i64) -> i64 = unsafe { jit.function("square").unwrap() };
    assert_eq!(square(3), 9);
    assert!(jit.signature("h").is_err());
    assert!(jit.address("counter").is_err(), "globals are not functions");
}

#[test]
fn an_unresolved_declaration_is_an_error_when_added() {
    let mut jit = Jit::new(JitOptions::default()).unwrap();
    let src = "(declare no_such_symbol_anywhere i64 ())
      (define (f i64) () (block entry (ret (call @no_such_symbol_anywhere))))";
    let e = message(jit.add_source("m", src).unwrap_err());
    assert!(
        e.contains("undefined symbol @no_such_symbol_anywhere"),
        "{e}"
    );
    assert!(
        jit.signature("f").is_err(),
        "nothing of the module was added"
    );
}

#[test]
fn tail_calls_in_the_jit_do_not_grow_the_stack() {
    let src = "(define tailcc (loop i64) ((i64 n) (i64 acc)) (block entry (br (icmp eq n (i64 0)) done more))
        (block done (ret acc)) (block more (tailcall @loop2 (sub n (i64 1)) (add acc (i64 1)) (i64 0))))
      (define tailcc (loop2 i64) ((i64 n) (i64 acc) (i64 pad)) (block entry (tailcall @loop n acc)))
      (define (run i64) ((i64 n)) (block entry (ret (call @loop n (i64 0)))))";
    let mut jit = Jit::new(JitOptions::default()).unwrap();
    jit.add_source("t", src).unwrap();
    // SAFETY: (fn i64 (i64)), C convention.
    let run: extern "C" fn(i64) -> i64 = unsafe { jit.function("run").unwrap() };
    // On a 2 MB test thread, 10^7 frames of any size would overflow.
    assert_eq!(run(10_000_000), 10_000_000);
}

#[test]
fn function_rejects_a_non_pointer_type() {
    let mut jit = Jit::new(JitOptions::default()).unwrap();
    jit.add_source("math", MATH).unwrap();
    // SAFETY: rejected before any transmute.
    let r: Result<u8, _> = unsafe { jit.function("square") };
    assert!(r.is_err());
}
