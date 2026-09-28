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
    let e = message(jit.address("counter").unwrap_err());
    assert!(e.contains("@counter is a global, not a function"), "{e}");
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

/// A `tailcc` function is reached from Rust through the `ccc`
/// trampoline the `Jit` generates (spec/lir.md §11).
#[test]
fn a_tailcc_function_is_called_through_a_generated_trampoline() {
    let src = "(define tailcc (count i64) ((i64 n) (i64 acc)) (block entry (br (icmp eq n (i64 0)) done more))
        (block done (ret acc)) (block more (tailcall @count (sub n (i64 1)) (add acc (i64 2)))))
      (define tailcc (say void) ((ptr s)) (block entry (call @puts s) (ret)))
      (declare puts i32 (ptr))";
    let mut jit = Jit::new(JitOptions::default()).unwrap();
    jit.add_source("t", src).unwrap();
    assert_eq!(
        jit.signature("count").unwrap().to_string(),
        "(fn tailcc i64 (i64 i64))"
    );
    let raw = jit.address("count").unwrap();
    let entry = jit.c_entry("count").unwrap();
    assert_ne!(raw, entry, "the trampoline is a function of its own");
    assert_eq!(jit.c_entry("count").unwrap(), entry, "generated once");
    assert_eq!(
        jit.signature("count.tramp").unwrap().to_string(),
        "(fn i64 (i64 i64))"
    );
    // SAFETY: (fn i64 (i64 i64)) through its ccc trampoline; (fn void (ptr)) likewise.
    let count: extern "C" fn(i64, i64) -> i64 = unsafe { jit.function("count").unwrap() };
    assert_eq!(count(1_000_000, 0), 2_000_000);
    let say: extern "C" fn(*const u8) = unsafe { jit.function("say").unwrap() };
    say(c"trampoline".as_ptr().cast());
}

/// The mechanism a compiler runs a macro with (ROADMAP M4, M6):
/// compile the macro-time module, take a function pointer, call it,
/// then add a later module that references what the first defined.
#[test]
fn a_compiler_runs_a_macro_then_links_a_module_that_uses_it() {
    let mut jit = Jit::new(JitOptions::default()).unwrap();
    // The macro-time module: a "macro" that expands by counting, over a
    // global it owns, plus a tailcc helper it exposes.
    let macro_module = "(global expansions i64 (i64 0))
      (define private (bump i64) () (block entry
        (ret (add (atomicrmw add seq_cst @expansions (i64 1)) (i64 1)))))
      (define (expand-macro i64) ((i64 form)) (block entry
        (ret (add (mul form (i64 10)) (call @bump)))))
      (define tailcc (twice i64) ((i64 x)) (block entry (ret (mul x (i64 2)))))";
    jit.add_source("macros", macro_module).unwrap();
    // The compiler calls the macro while it compiles.
    // SAFETY: (fn i64 (i64)).
    let expand: extern "C" fn(i64) -> i64 = unsafe { jit.function("expand-macro").unwrap() };
    assert_eq!((expand(4), expand(5)), (41, 52));
    // The program module references the macro module by declare and
    // declare-global; the private helper is not reachable.
    let program = "(declare expand-macro i64 (i64))
      (declare tailcc twice i64 (i64))
      (declare-global expansions i64)
      (define (run i64) () (block entry
        (ret (add (call @twice (call @expand-macro (i64 7))) (load i64 @expansions)))))";
    jit.add_source("program", program).unwrap();
    // SAFETY: (fn i64 ()).
    let run: extern "C" fn() -> i64 = unsafe { jit.function("run").unwrap() };
    assert_eq!(run(), (73 * 2) + 3);
    assert_eq!(
        expand(0),
        4,
        "the macro keeps running after more modules were added"
    );
    let uses_private = "(declare bump i64 ()) (define (g i64) () (block entry (ret (call @bump))))";
    let e = message(jit.add_source("p2", uses_private).unwrap_err());
    assert!(e.contains("undefined symbol @bump"), "{e}");
    let e = message(jit.signature("bump").unwrap_err());
    assert!(e.contains("@bump is private to module macros"), "{e}");
    let wrong_global = "(declare-global expansions i32) (define (h i32) () (block entry (ret (load i32 @expansions))))";
    let e = message(jit.add_source("p3", wrong_global).unwrap_err());
    assert!(
        e.contains("declaration of @expansions does not match its definition in module macros"),
        "{e}"
    );
}

/// `private` and `internal` names are not in the cross-module
/// namespace: a later module may define them again.
#[test]
fn private_names_do_not_collide_across_modules() {
    let mut jit = Jit::new(JitOptions::default()).unwrap();
    let a = "(define private (helper i64) () (block entry (ret (i64 1))))
      (global internal state i64 (i64 10))
      (define (a i64) () (block entry (ret (add (call @helper) (load i64 @state)))))";
    let b = "(define internal (helper i64) () (block entry (ret (i64 2))))
      (global private state i64 (i64 20))
      (define (b i64) () (block entry (ret (add (call @helper) (load i64 @state)))))";
    jit.add_source("a", a).unwrap();
    jit.add_source("b", b).unwrap();
    // SAFETY: (fn i64 ()) both.
    let fa: extern "C" fn() -> i64 = unsafe { jit.function("a").unwrap() };
    let fb: extern "C" fn() -> i64 = unsafe { jit.function("b").unwrap() };
    assert_eq!((fa(), fb()), (11, 22));
    let e = message(
        jit.add_source(
            "c",
            "(declare-global state i64) (define (c i64) () (block entry (ret (load i64 @state))))",
        )
        .unwrap_err(),
    );
    assert!(e.contains("undefined symbol @state"), "{e}");
}
