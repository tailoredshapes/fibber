//! The optimisation level (compiler.md §1, stdlib design §7 C6): `fibc
//! build` compiles at level 2 unless `-O N` says otherwise, `fibc run`
//! at level 0 (a run compiles the program afresh every time, and level 2
//! costs about two and a half times as long to compile).
//!
//! The level of a build is observed in the executable it writes, which
//! is the same bytes every time for one program at one level: the
//! default is the file that `-O 2` writes, and not the one `-O 0` does,
//! which is more than twice the size for a small loop. The level of a
//! run leaves nothing to look at but the time it takes, so what is tested
//! of `run` is what the command line says (`command.rs` tests that the
//! default is 0): the flag is taken, a level that is not 0 to 3 is a
//! usage error, and every level runs the program to the same result.

use std::path::Path;
use std::process::Command;

use super::io_support::TempDir;
use super::{bounded, fibc};

/// A loop whose machine code differs most between the levels.
const LOOP: &str = r#"
(defun sum-to (n: i64) -> i64
  (loop ((i 0) (acc 0))
    (if (> i n) acc (recur (+ i 1) (+ acc i)))))

(defun main () -> i64 (rem (sum-to 100000) 251))
"#;

/// The status `LOOP`'s `main` returns: 5000050000 mod 251.
const LOOP_RESULT: i32 = 233;

/// `fibc build SRC -o OUT` and then `flags`; the bytes of the executable.
fn built(src: &Path, out: &Path, flags: &[&str]) -> Vec<u8> {
    let made = bounded::output_within(
        fibc().arg("build").arg(src).arg("-o").arg(out).args(flags),
        bounded::COMPILE,
    );
    assert!(
        made.status.success(),
        "build {flags:?}: {}",
        String::from_utf8_lossy(&made.stderr)
    );
    std::fs::read(out).expect("the executable is there")
}

fn status(exe: &Path) -> Option<i32> {
    bounded::output(&mut Command::new(exe)).status.code()
}

#[test]
fn build_without_a_level_compiles_at_level_2() {
    let dir = TempDir::new("opt-build");
    let src = dir.file("loop.fib", LOOP);
    let out = dir.path().join("loop");
    let default = built(&src, &out, &[]);
    assert_eq!(status(&out), Some(LOOP_RESULT));
    let again = built(&src, &out, &[]);
    assert!(default == again, "a build is the same bytes every time");
    assert!(
        default == built(&src, &out, &["-O", "2"]),
        "default is -O 2"
    );
    assert!(default == built(&src, &out, &["-O2"]), "-O2 is -O 2");
    let none = built(&src, &out, &["-O", "0"]);
    assert_eq!(status(&out), Some(LOOP_RESULT));
    assert!(default != none, "the default is not -O 0");
    assert!(
        default.len() * 2 < none.len(),
        "-O 0 is {} bytes and the default {}",
        none.len(),
        default.len()
    );
}

#[test]
fn every_level_builds_an_executable_with_the_same_result() {
    let dir = TempDir::new("opt-levels");
    let src = dir.file("loop.fib", LOOP);
    let out = dir.path().join("loop");
    for level in ["0", "1", "2", "3"] {
        built(&src, &out, &["-O", level]);
        assert_eq!(status(&out), Some(LOOP_RESULT), "-O {level}");
    }
}

#[test]
fn run_takes_a_level_and_every_level_gives_the_same_result() {
    let dir = TempDir::new("opt-run");
    let src = dir.file("loop.fib", LOOP);
    for flags in [
        &[][..],
        &["-O", "0"][..],
        &["-O1"][..],
        &["-O", "2"][..],
        &["--trace", "-O3"][..],
    ] {
        let out = bounded::output_within(fibc().arg("run").args(flags).arg(&src), bounded::COMPILE);
        assert!(out.status.success(), "{flags:?}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(stdout.lines().last(), Some("233"), "{flags:?}: {stdout}");
    }
}

#[test]
fn a_level_that_is_not_0_to_3_is_a_usage_error_before_anything_compiles() {
    let dir = TempDir::new("opt-bad");
    let src = dir.file("loop.fib", LOOP);
    let exe = dir.path().join("loop");
    let bad: [Vec<&str>; 4] = [
        vec!["build", "SRC", "-o", "EXE", "-O", "4"],
        vec!["build", "SRC", "-o", "EXE", "-O"],
        vec!["run", "-O", "x", "SRC"],
        vec!["run", "-O", "9", "SRC"],
    ];
    for line in bad {
        let mut command = fibc();
        for word in &line {
            match *word {
                "SRC" => command.arg(&src),
                "EXE" => command.arg(&exe),
                other => command.arg(other),
            };
        }
        let out = bounded::output(&mut command);
        assert_eq!(out.status.code(), Some(2), "{line:?}");
        assert!(String::from_utf8_lossy(&out.stderr).starts_with("usage: fibc"));
        assert!(out.stdout.is_empty(), "{line:?}");
        assert!(!exe.exists(), "{line:?} wrote an executable");
    }
}
