//! The C interface (spec/compiler.md §9) as a C program would use it,
//! driven from Rust through the exported `extern "C"` items with raw
//! pointers and byte lengths: sessions, calling addresses, errors and
//! executables. `capi_mailbox.rs` and `capi_misuse.rs` do the mailbox;
//! `c_consumer.rs` does it all from real C through `lair.h`. Here the
//! Rust API is the oracle for error texts.

mod common;

use std::ffi::c_char;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr;
use std::thread;

use common::*;
use lair::capi::*;
use lair::{Jit, JitOptions};

const MATH: &str = include_str!("c/math.lir");

// ---- a session ----

#[test]
fn a_session_adds_a_module_and_gives_addresses_that_run() {
    unsafe {
        let j = new_jit(0);
        assert_eq!(add(j, "math", MATH), None);
        let sq = address(j, "square").unwrap();
        assert_eq!(call(sq, &[-12]), 144);
        assert_eq!(
            c_entry(j, "square").unwrap(),
            sq,
            "a ccc function is its own entry"
        );
        // A tailcc function: its address is its own convention's, its
        // c_entry a trampoline of the session's making, once.
        let raw = address(j, "count").unwrap();
        let entry = c_entry(j, "count").unwrap();
        assert_ne!(raw, entry);
        assert_eq!(c_entry(j, "count").unwrap(), entry);
        assert_eq!(call(entry, &[1_000_000, 0]), 2_000_000);
        // The errors are the Rust API's.
        let mut rust = Jit::new(JitOptions::default()).unwrap();
        rust.add_source("math", MATH).unwrap();
        assert_eq!(
            address(j, "nope").unwrap_err(),
            rust.address("nope").unwrap_err().to_string()
        );
        assert_eq!(
            address(j, "counter").unwrap_err(),
            rust.address("counter").unwrap_err().to_string()
        );
        assert!(address(j, "counter")
            .unwrap_err()
            .contains("@counter is a global, not a function"));
        lair_jit_free(j);
    }
}

/// `(define (fK i64) ((i64 a0) ..) w)` where `w` weighs the arguments
/// by powers of ten, and `gK` its double `w / 4`.
fn arity_module() -> String {
    let mut m = String::new();
    for k in 0..=8usize {
        let params: String = (0..k).map(|i| format!("(i64 a{i}) ")).collect();
        let mut w = "(i64 42)".to_string();
        if k > 0 {
            w = "(i64 0)".to_string();
            for i in 0..k {
                w = format!(
                    "(add {w} (mul a{i} (i64 {})))",
                    10i64.pow((k - 1 - i) as u32)
                );
            }
        }
        m += &format!("(define (f{k} i64) ({params}) (block entry (ret {w})))\n");
        m += &format!(
            "(define (g{k} double) ({params}) (block entry (ret (fdiv (sitofp double {w}) (double 4.0)))))\n"
        );
    }
    m + "(define (deref i64) ((ptr p) (i64 k)) (block entry (ret (add (load i64 p) k))))\n"
}

fn weighed(args: &[i64]) -> i64 {
    if args.is_empty() {
        return 42;
    }
    let k = args.len();
    args.iter()
        .enumerate()
        .map(|(i, a)| a * 10i64.pow((k - 1 - i) as u32))
        .sum()
}

#[test]
fn lair_call_passes_0_to_8_arguments_in_order_and_reads_either_result() {
    unsafe {
        for opt in [0, 3] {
            let j = new_jit(opt);
            assert_eq!(add(j, "arity", &arity_module()), None);
            for k in 0..=8usize {
                let args: Vec<i64> = (0..k as i64)
                    .map(|i| if i % 2 == 0 { i + 1 } else { -(i + 1) })
                    .collect();
                let f = address(j, &format!("f{k}")).unwrap();
                let g = address(j, &format!("g{k}")).unwrap();
                assert_eq!(call(f, &args), weighed(&args), "f{k} at -O{opt}");
                assert_eq!(
                    lair_call_f64(g, args.as_ptr(), k),
                    weighed(&args) as f64 / 4.0,
                    "g{k} at -O{opt}"
                );
            }
            // A pointer argument, and a full 64-bit one.
            let cell: i64 = 1000;
            let d = address(j, "deref").unwrap();
            assert_eq!(call(d, &[ptr::addr_of!(cell) as i64, 7]), 1007);
            assert_eq!(
                call(address(j, "f1").unwrap(), &[i64::MAX / 10]),
                i64::MAX / 10
            );
            lair_jit_free(j);
        }
    }
}

#[test]
fn a_call_that_cannot_be_made_is_not_made() {
    unsafe {
        let j = new_jit(0);
        assert_eq!(add(j, "arity", &arity_module()), None);
        let f = address(j, "f3").unwrap();
        let nine = [1i64; 9];
        assert_eq!(
            lair_call_i64(f, nine.as_ptr(), 9),
            0,
            "more than 8 arguments"
        );
        assert_eq!(lair_call_i64(f, ptr::null(), 3), 0, "null args");
        assert_eq!(lair_call_i64(0, nine.as_ptr(), 3), 0, "no address");
        assert_eq!(lair_call_f64(0, nine.as_ptr(), 3), 0.0);
        let f0 = address(j, "f0").unwrap();
        assert_eq!(
            lair_call_i64(f0, ptr::null(), 0),
            42,
            "no arguments may come with a null pointer"
        );
        lair_jit_free(j);
    }
}

// ---- errors ----

#[test]
fn a_bad_module_is_an_error_with_the_text_the_rust_api_reports() {
    unsafe {
        let j = new_jit(0);
        assert_eq!(add(j, "math", MATH), None);
        let cases: [(&str, &str, &str); 5] = [
            (
                "invalid",
                "(define (h i32) () (block entry (ret (i64 1))))",
                "ret type i64 does not match @h's result i32",
            ),
            (
                "dup",
                "(define (square i64) ((i64 x)) (block entry (ret x)))",
                "duplicate definition of @square (first in module math)",
            ),
            (
                "bad-decl",
                "(declare square i32 (i32)) (define (g i32) () (block entry (ret (call @square (i32 2)))))",
                "declaration of @square does not match its definition in module math",
            ),
            (
                "unresolved",
                "(declare no_such_symbol_anywhere i64 ()) (define (f i64) () (block entry (ret (call @no_such_symbol_anywhere))))",
                "undefined symbol @no_such_symbol_anywhere",
            ),
            ("syntax", "(define (broken i64", ""),
        ];
        for (name, src, expect) in cases {
            let got = add(j, name, src).unwrap_or_else(|| panic!("{name} was accepted"));
            assert_eq!(got, rust_says(&[("math", MATH)], name, src), "{name}");
            assert!(got.contains(expect) && !got.is_empty(), "{name}: {got}");
        }
        // Nothing of a refused module was added, and the session works.
        assert!(address(j, "h").is_err() && address(j, "f").is_err());
        assert_eq!(call(address(j, "square").unwrap(), &[9]), 81);
        lair_jit_free(j);
    }
}

#[test]
fn check_source_is_null_for_a_valid_module_and_the_diagnostics_otherwise() {
    unsafe {
        let (m, ml) = s(MATH);
        assert_eq!(take(lair_check_source(m, ml)), None);
        let bad = "(define (h i32) () (block entry (ret (i64 1))))";
        let (b, bl) = s(bad);
        let got = take(lair_check_source(b, bl)).expect("rejected");
        assert_eq!(got, lair::check_source(bad).unwrap_err().to_string());
        assert!(
            got.contains("ret type i64 does not match @h's result i32"),
            "{got}"
        );
        // Two diagnostics are two lines, as `lair check` prints them.
        let two = "(define (h i32) () (block entry (ret (i64 1)))) (define (k i32) () (block entry (ret (i64 2))))";
        let (t, tl) = s(two);
        let got = take(lair_check_source(t, tl)).unwrap();
        assert_eq!(got.lines().count(), 2, "{got}");
        // No code was made, so nothing of this was a session either.
        assert_eq!(
            take(lair_check_source(ptr::null(), 0)).is_none(),
            lair::check_source("").is_ok()
        );
    }
}

#[test]
fn null_handles_and_out_parameters_are_errors_not_undefined_behaviour() {
    unsafe {
        let mut j: *mut LairJit = std::ptr::dangling_mut();
        assert_eq!(
            take(lair_jit_new(0, ptr::null_mut())).unwrap(),
            "out is null"
        );
        let e = take(lair_jit_new(7, &mut j)).unwrap();
        assert_eq!(
            (e.as_str(), j.is_null()),
            ("opt_level is 7, not 0 to 3", true)
        );
        assert!(take(lair_jit_new(-1, &mut j)).is_some());
        lair_jit_free(ptr::null_mut());
        lair_error_free(ptr::null_mut());
        let mut len = 77usize;
        assert!(!lair_error_text(ptr::null(), &mut len).is_null());
        assert_eq!(len, 0);
    }
}

#[test]
fn null_and_malformed_strings_are_errors_not_undefined_behaviour() {
    unsafe {
        let (n, nl) = s("m");
        assert_eq!(
            take(lair_jit_add_source(ptr::null_mut(), n, nl, n, nl)).unwrap(),
            "jit is null"
        );
        let j = new_jit(0);
        assert_eq!(
            take(lair_jit_add_source(j, ptr::null(), 4, n, nl)).unwrap(),
            "name is null but its length is 4"
        );
        assert_eq!(
            take(lair_jit_add_source(j, n, nl, ptr::null(), 9)).unwrap(),
            "src is null but its length is 9"
        );
        let bad = [b'(', b'd', 0xff, b')'];
        assert_eq!(
            take(lair_jit_add_source(j, n, nl, bad.as_ptr().cast(), 4)).unwrap(),
            "src is not valid UTF-8 (valid up to byte 2)"
        );
        assert_eq!(
            take(lair_jit_address(j, n, nl, ptr::null_mut())).unwrap(),
            "out is null"
        );
        assert_eq!(
            take(lair_jit_c_entry(ptr::null_mut(), n, nl, &mut 0usize)).unwrap(),
            "jit is null"
        );
        assert_eq!(
            take(lair_check_source(ptr::null(), 3)).unwrap(),
            "src is null but its length is 3"
        );
        lair_jit_free(j);
    }
}

// ---- an executable ----

unsafe fn build(src: &str, out: &Path, opt: i32, libs: &[&str]) -> Option<String> {
    let path = out.to_str().unwrap();
    let (c, cl) = s(src);
    let (p, pl) = s(path);
    let ptrs: Vec<*const c_char> = libs.iter().map(|l| l.as_ptr().cast()).collect();
    let lens: Vec<usize> = libs.iter().map(|l| l.len()).collect();
    let (lp, ll) = if libs.is_empty() {
        (ptr::null(), ptr::null())
    } else {
        (ptrs.as_ptr(), lens.as_ptr())
    };
    take(lair_build_executable(c, cl, p, pl, opt, lp, ll, libs.len()))
}

#[test]
fn an_executable_built_through_the_interface_exits_with_mains_result() {
    let dir = Scratch::new("capi-exe");
    unsafe {
        let src = "(declare printf i32 (ptr ...))
          (define (main i32) () (block entry (call @printf (string \"hello from lair\\n\")) (ret (i32 37))))";
        for (opt, libs) in [(0, vec![]), (2, vec!["m", "c"])] {
            let exe = dir.path().join(format!("hello-{opt}"));
            assert_eq!(build(src, &exe, opt, &libs), None);
            let out = Command::new(&exe).output().unwrap();
            assert_eq!(out.status.code(), Some(37), "-O{opt}");
            assert_eq!(String::from_utf8_lossy(&out.stdout), "hello from lair\n");
            std::fs::remove_file(&exe).unwrap();
        }
        // A module that is valid lIR but not a program: the main rule's text.
        let lib = "(define (square i64) ((i64 x)) (block entry (ret (mul x x))))";
        let exe = dir.path().join("nomain");
        let e = build(lib, &exe, 0, &[]).unwrap();
        assert_eq!(e, lair::for_executable(lib).unwrap_err().to_string());
        assert!(e.contains("main"), "{e}");
        assert!(!exe.exists(), "nothing is left behind");
        // A library the linker cannot find: its output, as Rust's error.
        let e = build(
            "(define (main i32) () (block entry (ret (i32 0))))",
            &exe,
            0,
            &["no_such_lib_xyz"],
        )
        .unwrap();
        assert!(
            e.starts_with("error: linker failed:") && e.contains("no_such_lib_xyz"),
            "{e}"
        );
        assert!(build("", &exe, 9, &[]).unwrap().contains("not 0 to 3"));
        assert_eq!(build("", &PathBuf::new(), 0, &[]).unwrap(), "path is empty");
    }
}

#[test]
fn many_sessions_compile_and_run_in_threads_at_once() {
    let threads: Vec<_> = (0..4i64)
        .map(|t| {
            thread::spawn(move || unsafe {
                for round in 0..3 {
                    let j = new_jit((t as i32 + round) % 4);
                    assert_eq!(add(j, "arity", &arity_module()), None);
                    assert_eq!(add(j, "hooked", HOOKED), None);
                    let args = [t + 1, 2, 3, 4, 5];
                    assert_eq!(call(address(j, "f5").unwrap(), &args), weighed(&args));
                    let e =
                        add(j, "dup", "(define (f5 i64) () (block entry (ret (i64 0))))").unwrap();
                    assert!(
                        e.contains("duplicate definition of @f5 (first in module arity)"),
                        "{e}"
                    );
                    lair_jit_free(j);
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
}
