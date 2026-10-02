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

use common::{bounded, skip, Scratch};

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
    let out = bounded::output_within(&mut cargo, bounded::CARGO);
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
    let out = bounded::output_within(
        Command::new("cc")
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
            .arg("-pthread"),
        bounded::COMPILE,
    );
    assert!(out.status.success(), "cc failed:\n{}", text(&out.stderr));
    exe
}

fn run(exe: &Path, src_dir: &Path, tmp: &Path) -> Output {
    bounded::output(
        Command::new(exe)
            .arg(src_dir)
            .arg(tmp)
            .env("ASAN_OPTIONS", "detect_leaks=1"),
    )
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

/// `text`, which must say each of `phrases`. The C program's error texts
/// are compared with the ones the Rust API gives, and the library takes
/// them from the same functions, so that comparison shows the marshalling
/// (the length, the terminator, the copy) and not the wording: two texts
/// that were both empty, or both about something else, would agree. The
/// phrases pin what each error is about.
fn says(text: String, phrases: &[&str]) -> String {
    for phrase in phrases {
        assert!(text.contains(phrase), "`{text}` does not say `{phrase}`");
    }
    text
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
        format!(
            "address(nope): {}",
            says(
                jit.address("nope").unwrap_err().to_string(),
                &["no function @nope"]
            )
        ),
        format!(
            "add invalid: {}",
            says(
                rust_add("invalid", BAD),
                &["ret type i64", "@h's result i32"]
            )
        ),
        format!(
            "add duplicate: {}",
            says(
                rust_add("dup", dup),
                &["duplicate definition of @square", "module math"]
            )
        ),
        format!(
            "add syntax: {}",
            says(
                rust_add("syntax", "(define (broken i64"),
                &["unclosed list"]
            )
        ),
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
        format!(
            "check invalid: {}",
            says(
                lair::check_source(BAD).unwrap_err().to_string(),
                &["ret type i64", "@h's result i32"]
            )
        ),
        "exe: status 37, said hello from lair".into(),
        format!(
            "exe without main: {}",
            says(
                lair::for_executable(no_main).unwrap_err().to_string(),
                &["no main function"]
            )
        ),
        "exe bad level: opt_level is 9, not 0 to 3".into(),
        "exe with a missing library directory: error: -L /no/such/lair/library/directory: No such file or directory (os error 2)".into(),
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

/// Whether a program built with the sanitizers runs here: the compiler
/// has their runtimes (libasan, libubsan) and the process may map the
/// shadow memory, which an address-space limit (`ulimit -v`) forbids. The
/// reason, when not.
fn sanitizers_work(dir: &Path) -> Result<(), String> {
    let src = dir.join("probe.c");
    std::fs::write(&src, "int main(void) { return 0; }\n").unwrap();
    let exe = dir.join("probe");
    let built = bounded::output_within(
        Command::new("cc")
            .arg("-fsanitize=address,undefined")
            .arg(&src)
            .arg("-o")
            .arg(&exe),
        bounded::COMPILE,
    );
    if !built.status.success() {
        let why = text(&built.stderr);
        return Err(format!(
            "cc cannot build with -fsanitize=address,undefined: {}",
            why.lines().next().unwrap_or("no message")
        ));
    }
    let ran = bounded::output(&mut Command::new(&exe));
    if ran.status.success() {
        return Ok(());
    }
    let why = text(&ran.stderr);
    Err(format!(
        "a program built with the sanitizers does not run here (a `ulimit -v`?): {}",
        why.lines().next().unwrap_or("no message")
    ))
}

#[test]
fn the_same_program_under_address_sanitizer_leaks_and_corrupts_nothing() {
    let lib = library();
    let guard = Scratch::new("c-consumer-asan");
    let dir = guard.path();
    if let Err(why) = sanitizers_work(dir) {
        skip(
            "the_same_program_under_address_sanitizer_leaks_and_corrupts_nothing",
            &why,
        );
        return;
    }
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

/// Whether `program` is installed, by asking it for its version.
fn installed(program: &str) -> bool {
    Command::new(program).arg("--version").output().is_ok()
}

#[test]
fn the_header_compiles_as_c_twice_over() {
    let guard = Scratch::new("c-consumer-header");
    let c = guard.path().join("twice.c");
    std::fs::write(
        &c,
        "#include \"lair.h\"\n#include \"lair.h\"\nint main(void) { return lair_hook1_address() == 0; }\n",
    )
    .unwrap();
    let o = bounded::output_within(
        Command::new("cc")
            .args([
                "-std=c99",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-pedantic",
                "-fsyntax-only",
            ])
            .arg("-I")
            .arg(manifest().join("include"))
            .arg(&c),
        bounded::COMPILE,
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
}

/// A C++ compiler is not part of what the library needs, so a machine
/// without one skips this, with a line that says so.
#[test]
fn the_header_compiles_as_cplusplus() {
    if !installed("c++") {
        skip(
            "the_header_compiles_as_cplusplus",
            "there is no c++ compiler",
        );
        return;
    }
    let guard = Scratch::new("c-consumer-header-cpp");
    let cc = guard.path().join("twice.cc");
    std::fs::write(
        &cc,
        "#include \"lair.h\"\n#include \"lair.h\"\nint main() { lair_error *e = lair_check_source(\"\", 0); lair_error_free(e); return 0; }\n",
    )
    .unwrap();
    let o = bounded::output_within(
        Command::new("c++")
            .args(["-std=c++17", "-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
            .arg("-I")
            .arg(manifest().join("include"))
            .arg(&cc),
        bounded::COMPILE,
    );
    assert!(o.status.success(), "{}", text(&o.stderr));
}

/// The pin on the wording fails on a text that is not about the thing.
#[test]
fn the_wording_check_notices_an_error_that_says_something_else() {
    assert_eq!(says("a b c".into(), &["a b", "c"]), "a b c");
    let caught =
        std::panic::catch_unwind(|| says("1:1: error: ok".into(), &["duplicate definition"]));
    assert!(caught.is_err());
    let caught = std::panic::catch_unwind(|| says(String::new(), &["ret type"]));
    assert!(caught.is_err());
}
