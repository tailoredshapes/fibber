//! A real C program (`tests/c/consumer.c`) using `liblair.so` through
//! `include/lair.h` alone, compiled with `cc` and run; its output is
//! compared with what the Rust API says. It also runs under
//! AddressSanitizer, which reports a leaked handle or error, and the
//! header is compiled as C (twice, for the guards) and as C++.
//!
//! The test builds the library itself (`cargo build -p lair --lib`) so
//! that it exists whichever way `cargo test` was invoked.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod common;

use common::Scratch;

const MATH: &str = include_str!("c/math.lir");

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `target/<profile>`, found from this test's own executable
/// (`target/<profile>/deps/c_consumer-HASH`), which is right whatever
/// `CARGO_TARGET_DIR` or `--release` said.
fn profile_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("own path");
    exe.parent()
        .and_then(Path::parent)
        .expect("target/<profile>/deps")
        .to_path_buf()
}

/// Build `liblair.so` for the profile this test was built with; the
/// directory it is in.
fn library() -> PathBuf {
    let dir = profile_dir();
    let profile = dir.file_name().and_then(|p| p.to_str()).unwrap_or("debug");
    let mut cargo = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo
        .args(["build", "-p", "lair", "--lib", "-j2"])
        .current_dir(manifest());
    match profile {
        "debug" => {}
        "release" => {
            cargo.arg("--release");
        }
        other => {
            cargo.args(["--profile", other]);
        }
    }
    if let Some(t) = std::env::var_os("CARGO_TARGET_DIR") {
        cargo.env("CARGO_TARGET_DIR", t);
    }
    let out = cargo.output().expect("cargo runs");
    assert!(
        out.status.success(),
        "cargo build -p lair --lib failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let so = dir.join("liblair.so");
    assert!(so.exists(), "{} was not built", so.display());
    dir
}

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

/// Compile `consumer.c` against `liblair.so` into `dir/consumer`.
fn compile(dir: &Path, lib: &Path, flags: &[&str]) -> PathBuf {
    let exe = dir.join("consumer");
    let out = Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pedantic", "-g"])
        .args(flags)
        .arg("-I")
        .arg(manifest().join("include"))
        .arg(manifest().join("tests/c/consumer.c"))
        .arg("-o")
        .arg(&exe)
        .arg("-L")
        .arg(lib)
        .arg("-llair")
        .arg(format!("-Wl,-rpath,{}", lib.display()))
        .arg("-pthread")
        .output()
        .expect("cc runs");
    assert!(out.status.success(), "cc failed:\n{}", text(&out.stderr));
    exe
}

fn run(exe: &Path, src_dir: &Path, tmp: &Path) -> Output {
    Command::new(exe)
        .arg(src_dir)
        .arg(tmp)
        .env("ASAN_OPTIONS", "detect_leaks=1")
        .output()
        .expect("consumer runs")
}

/// The numbers the arity part prints: `f0 = 42` up to `g8`.
fn weighed(args: &[i64]) -> i64 {
    let k = args.len();
    if k == 0 {
        return 42;
    }
    (0..k)
        .map(|i| args[i] * 10i64.pow((k - 1 - i) as u32))
        .sum()
}

/// The error `jit.add_source(name, src)` gives after `math` was added.
fn rust_add(name: &str, src: &str) -> String {
    let mut jit = lair::Jit::new(lair::JitOptions::default()).unwrap();
    jit.add_source("math", MATH).unwrap();
    jit.add_source(name, src).unwrap_err().to_string()
}

const BAD: &str = "(define (h i32) () (block entry (ret (i64 1))))";

/// The first part of the output: a session and its errors.
fn expected_session() -> Vec<String> {
    let mut jit = lair::Jit::new(lair::JitOptions::default()).unwrap();
    jit.add_source("math", MATH).unwrap();
    let dup = "(define (square i64) ((i64 x)) (block entry (ret x)))";
    vec![
        "session: new ok".into(),
        "square(-12) = 144".into(),
        "c_entry(square) == address: yes".into(),
        "c_entry(count) != address: yes".into(),
        "count(1000000, 0) = 2000000".into(),
        format!("address(nope): {}", jit.address("nope").unwrap_err()),
        format!("add invalid: {}", rust_add("invalid", BAD)),
        format!("add duplicate: {}", rust_add("dup", dup)),
        format!("add syntax: {}", rust_add("syntax", "(define (broken i64")),
        "session still usable: square(9) = 81".into(),
    ]
}

/// `f0 = 42`, `g0 = 10.50`, .. `g8`, then the pointer and refused calls.
fn expected_arity() -> Vec<String> {
    let mut lines = Vec::new();
    for k in 0..=8usize {
        let args: Vec<i64> = (0..k as i64)
            .map(|i| if i % 2 == 0 { i + 1 } else { -(i + 1) })
            .collect();
        lines.push(format!("f{k} = {}", weighed(&args)));
        lines.push(format!("g{k} = {:.2}", weighed(&args) as f64 / 4.0));
    }
    lines.push("deref = 1007".into());
    lines.push("refused calls: 0 0 0 0.0".into());
    lines
}

/// Checking, the executable, and the null arguments.
fn expected_checks() -> Vec<String> {
    let no_main = "(define (square i64) ((i64 x)) (block entry (ret (mul x x))))";
    vec![
        "check valid: ok".into(),
        format!("check invalid: {}", lair::check_source(BAD).unwrap_err()),
        "exe: status 37, said hello from lair".into(),
        format!(
            "exe without main: {}",
            lair::for_executable(no_main).unwrap_err()
        ),
        "exe bad level: opt_level is 9, not 0 to 3".into(),
        "bad opt level: opt_level is 7, not 0 to 3".into(),
        "null jit: jit is null".into(),
        "null src: src is null but its length is 3".into(),
        "null out: out is null".into(),
        "null arguments: ok".into(),
    ]
}

/// The mailbox, the misuse, the threads.
fn expected_mailbox() -> Vec<String> {
    [
        "mailbox one(20):",
        "  hook1(20) -> 61",
        "  result 1000061",
        "mailbox two(5, 6):",
        "  hook2(5, 6) -> 506",
        "  result 2000506",
        "mailbox both(4):",
        "  hook1(4) -> 13",
        "  hook2(4, 7) -> 407",
        "  result 420",
        "mailbox many(1000): 1000 requests, result 1499500",
        "mailbox two at once: 1000 2000",
        "mailbox misuse: lair_call_hook_reply: no hook call is waiting; wait gives -1",
        "mailbox after the fault: result 1000004",
        "threads: 900 + 900 requests, results 404550 404550",
        "done",
    ]
    .map(String::from)
    .to_vec()
}

/// What `consumer.c` must print: numbers from arithmetic done here,
/// error texts from the Rust API, which the library's errors equal.
fn expected() -> String {
    [
        expected_session(),
        expected_arity(),
        expected_checks(),
        expected_mailbox(),
    ]
    .concat()
    .iter()
    .map(|l| format!("{l}\n"))
    .collect()
}

/// The lines of `a` that differ from `b`, for a failure message.
fn first_difference(got: &str, want: &str) -> String {
    for (i, (g, w)) in got.lines().zip(want.lines()).enumerate() {
        if g != w {
            return format!("line {}:\n  got:  {g}\n  want: {w}", i + 1);
        }
    }
    format!(
        "same lines, got {} and want {} of them",
        got.lines().count(),
        want.lines().count()
    )
}

fn check_output(o: &Output, what: &str) {
    let (got, want) = (text(&o.stdout), expected());
    assert!(
        o.status.success() && got == want,
        "{what}: status {:?}\n{}\nstderr:\n{}",
        o.status.code(),
        first_difference(&got, &want),
        text(&o.stderr)
    );
}

#[test]
fn a_c_program_uses_the_library_through_the_header() {
    let lib = library();
    let guard = Scratch::new("c-consumer-plain");
    let dir = guard.path();
    let exe = compile(dir, &lib, &[]);
    let o = run(&exe, &manifest().join("tests/c"), dir);
    check_output(&o, "plain build");
}

#[test]
fn the_same_program_under_address_sanitizer_leaks_and_corrupts_nothing() {
    let lib = library();
    let guard = Scratch::new("c-consumer-asan");
    let dir = guard.path();
    let exe = compile(
        dir,
        &lib,
        &[
            "-fsanitize=address,undefined",
            "-fno-sanitize-recover=all",
            "-fno-omit-frame-pointer",
        ],
    );
    let o = run(&exe, &manifest().join("tests/c"), dir);
    check_output(&o, "address sanitizer build");
    let err = text(&o.stderr);
    assert!(
        !err.contains("Sanitizer") && !err.contains("runtime error"),
        "{err}"
    );
}

#[test]
fn the_harness_notices_a_c_program_that_fails() {
    let lib = library();
    let guard = Scratch::new("c-consumer-fails");
    let dir = guard.path();
    let exe = compile(dir, &lib, &[]);
    // Without its sources the program's own checks fire: exit status 1.
    let o = run(&exe, &dir.join("no-such-dir"), dir);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stderr));
    assert!(
        text(&o.stderr).contains("check failed"),
        "{}",
        text(&o.stderr)
    );
    // And output that differs from what the Rust API says fails the comparison.
    let mut ok = run(&exe, &manifest().join("tests/c"), dir);
    check_output(&ok, "the unchanged output passes");
    ok.stdout = text(&ok.stdout).replace("= 144", "= 145").into_bytes();
    let caught = std::panic::catch_unwind(|| check_output(&ok, "altered"));
    assert!(caught.is_err(), "a changed line went unnoticed");
}

#[test]
fn the_header_compiles_as_c_twice_over_and_as_cplusplus() {
    let include = manifest().join("include");
    let guard = Scratch::new("c-consumer-header");
    let dir = guard.path();
    let c = dir.join("twice.c");
    std::fs::write(
        &c,
        "#include \"lair.h\"\n#include \"lair.h\"\nint main(void) { return lair_hook1_address() == 0; }\n",
    )
    .unwrap();
    let o = Command::new("cc")
        .args([
            "-std=c99",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-fsyntax-only",
        ])
        .arg("-I")
        .arg(&include)
        .arg(&c)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o.stderr));
    let cc = dir.join("twice.cc");
    std::fs::write(
        &cc,
        "#include \"lair.h\"\n#include \"lair.h\"\nint main() { lair_error *e = lair_check_source(\"\", 0); lair_error_free(e); return 0; }\n",
    )
    .unwrap();
    let o = Command::new("c++")
        .args(["-std=c++17", "-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
        .arg("-I")
        .arg(&include)
        .arg(&cc)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", text(&o.stderr));
}
