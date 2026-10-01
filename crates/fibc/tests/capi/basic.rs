//! `compiler/jit-demo.fib basic DIR`: a fibber program drives lair through
//! the C interface (spec/compiler.md §9) in a JIT session, the checker,
//! the ahead-of-time build and a hook round trip, and every line it
//! prints is compared with what the Rust API of `lair` says for the same
//! text. The executables it builds are run here.

use std::process::Command;

use lair::{Jit, JitOptions};

use super::support::{built, demo_stdout};

const SQUARE: &str = "(define (square i64) ((i64 x)) (block entry (ret (mul x x))))
(define (sum3 i64) ((i64 a) (i64 b) (i64 c)) (block entry (ret (add a (add b c)))))
(define (half double) () (block entry (ret (double 0.5))))";
const BAD: &str = "(define (h i32) () (block entry (ret (i64 1))))";
const BROKEN: &str = "(define (broken i64";
const NO_MAIN: &str = "(define (helper i64) () (block entry (ret (i64 1))))";

/// What `Jit::add_source` says of `src` added to a session that holds
/// SQUARE under the name `square`.
fn add_error(name: &str, src: &str) -> String {
    let mut jit = Jit::new(JitOptions::default()).expect("a session");
    jit.add_source("square", SQUARE).expect("square is valid");
    jit.add_source(name, src).expect_err(name).to_string()
}

/// The first part of the output, up to the executables.
fn expected_session() -> String {
    let mut jit = Jit::new(JitOptions::default()).expect("a session");
    jit.add_source("square", SQUARE).expect("square is valid");
    let missing = jit.address("nope").expect_err("no such function");
    let check = |src: &str| lair::check_source(src).expect_err(src).to_string();
    let lines = [
        "add square: ok".to_string(),
        "square(12): 144".into(),
        "sum3(1, 20, 300): 321".into(),
        "half(): 0.5".into(),
        "-- call-i64 with 9 arguments".into(),
        "call-i64: more than 8 arguments".into(),
        "-- call-i64 at address 0".into(),
        "call-i64: address 0".into(),
        "-- address of a missing function".into(),
        missing.to_string(),
        "-- add a module that fails the checker".into(),
        add_error("bad", BAD),
        "-- add a module that fails the parser".into(),
        add_error("broken", BROKEN),
        "-- add square again".into(),
        add_error("again", SQUARE),
        "square(9) after the failures: 81".into(),
        "check square: ok".into(),
        "-- check a module that fails the checker".into(),
        check(BAD),
        "-- check a module that fails the parser".into(),
        check(BROKEN),
        "build hello: ok".into(),
        "build exit7 with libm at -O2: ok".into(),
        "-- build a module with no main".into(),
        lair::for_executable(NO_MAIN)
            .expect_err("no main")
            .to_string(),
        "-- build with a library that does not exist".into(),
    ];
    lines.join("\n") + "\n"
}

/// The hook part: 1000 requests in one call, both arities, one mailbox
/// reused, and the mailbox's own errors for each misuse.
const EXPECTED_HOOKS: &str = "add hooked: ok
install the hooks: ok
one(21): 1000042 after 1 hooks of arity 1 and 0 of arity 2
two(3, 4): 2000304 after 0 hooks of arity 1 and 1 of arity 2
many(1000): 999000 after 1000 hooks of arity 1 and 0 of arity 2
one(5) again on the same mailbox: 1000010 after 1 hooks of arity 1 and 0 of arity 2
wait after a reply with no hook waiting: error: lair_call_hook_reply: no hook call is waiting
the fault text: lair_call_hook_reply: no hook call is waiting
-- start at address 0
call-start: address 0
-- start with 9 arguments
call-start: more than 8 arguments
start one(1): ok
wait: a hook of arity 1
-- start again while it is parked
lair_call_start: a call is still running
argument 5 of a hook that has one: 0
wait after the misuses: error: lair_call_start: a call is still running
wait again: done
result of the call that was parked: 1000005
";

fn run_basic(tag: &str) -> (String, std::path::PathBuf) {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("capi-basic-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory for the executables");
    let out = demo_stdout(&["basic", dir.to_str().expect("utf-8")]);
    (out, dir)
}

#[test]
fn the_demo_reports_what_lair_reports() {
    let (out, dir) = run_basic("report");
    let session = expected_session();
    assert!(
        out.starts_with(&session),
        "the demo's first part differs from lair's own texts.\n--- expected\n{session}--- got\n{out}"
    );
    let rest = &out[session.len()..];
    let (linker, hooks) = rest
        .split_once("add hooked: ok\n")
        .expect("the hook part follows the linker's error");
    assert!(
        linker.starts_with("error: linker failed: ") && linker.contains("-lno-such-lib-fibber"),
        "{linker}"
    );
    assert_eq!(format!("add hooked: ok\n{hooks}"), EXPECTED_HOOKS);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_executables_the_demo_builds_run_and_the_refused_ones_are_not_left_behind() {
    let (_, dir) = run_basic("exe");
    for (name, status) in [("hello", 42), ("exit7", 7)] {
        let exe = dir.join(name);
        let run = Command::new(&exe).output().expect("the executable runs");
        assert_eq!(run.status.code(), Some(status), "{name}");
    }
    for name in ["nomain", "nolib"] {
        assert!(!dir.join(name).exists(), "{name} must not exist");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// The demo's text for a module that fails the checker is what `lair
/// check` prints, a file name apart: the text of `lair check FILE` is the
/// lines of the demo's, each after `FILE:`.
#[test]
fn the_checkers_text_is_what_lair_check_prints() {
    let (out, dir) = run_basic("check");
    let file = dir.join("bad.lir");
    for (label, src) in [
        ("-- check a module that fails the checker\n", BAD),
        ("-- check a module that fails the parser\n", BROKEN),
    ] {
        std::fs::write(&file, src).expect("the module is written");
        let run = Command::new(&built().lair)
            .arg("check")
            .arg(&file)
            .output()
            .expect("lair runs");
        assert_eq!(run.status.code(), Some(1), "lair check accepts {src}");
        let printed = String::from_utf8_lossy(&run.stderr).into_owned();
        let prefix = format!("{}:", file.display());
        let from_lair: Vec<&str> = printed
            .lines()
            .map(|l| l.strip_prefix(&prefix).expect("each line names the file"))
            .collect();
        let after = out.split_once(label).expect("the section is there").1;
        let from_demo: Vec<&str> = after.lines().take(from_lair.len()).collect();
        assert_eq!(from_demo, from_lair, "{label}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
