//! `fibref run` and the program's surroundings (syntax §4.3, §4.5), as
//! the shell sees them: an argument that is not UTF-8 reaches `(args)`
//! as `String::from_utf8_lossy` makes it, and a write that fails or is cut
//! short is not dropped: `println` and `eprintln` finish the write or end
//! the run with the trap `println: write failed` (`eprintln: ...`). The
//! compiled side of each is `crates/fibc/tests/cli.rs`, which asks the
//! same questions of `fibc run` and of a built executable.

#![cfg(unix)]

mod io_support;

use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use io_support::{
    bytes_lines, case_192_stdout, chunking_shim, first_difference, full_device, limited,
    long_stderr_program, os_words, stderr_long_stderr, words, TempDir, BYTES_PROGRAM, CASE_192,
    CASE_192_RESULT, STDERR_LONG_PROGRAM, STDERR_THEN_STDOUT, STDOUT_THEN_STDERR,
};

const FIBREF: &str = env!("CARGO_BIN_EXE_fibref");

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

/// `fibref run FILE -- WORD..`.
fn run(src: &Path, words: &[OsString]) -> Command {
    let mut command = Command::new(FIBREF);
    command.arg("run").arg(src).arg("--").args(words);
    command
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn an_argument_that_is_not_utf8_reaches_args_as_from_utf8_lossy_makes_it() {
    let dir = TempDir::new("run-args");
    let src = dir.file("bytes.fib", BYTES_PROGRAM);
    let words = words();
    let out = run(&src, &os_words(&words)).output().expect("fibref runs");
    let stdout = text(&out.stdout);
    let at = stdout.rfind("result: ").expect("fibref prints the result");
    let (lines, report) = stdout.split_at(at);
    if let Some(d) = first_difference(&words, lines, &bytes_lines(&words)) {
        panic!("{d}\n{}", text(&out.stderr));
    }
    assert!(
        report.starts_with("result: 0\naudit:  clean=true"),
        "{report}"
    );
    assert!(out.status.success(), "{:?}", out.status);
    assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
}

#[test]
fn a_name_before_the_dashes_that_is_not_utf8_is_refused_not_guessed() {
    let out = Command::new(FIBREF)
        .arg("run")
        .arg(OsString::from_vec(b"a\xffb.fib".to_vec()))
        .output()
        .expect("fibref runs");
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("an argument before `--` is not UTF-8: a\u{fffd}b.fib"),
        "{stderr}"
    );
    assert!(out.stdout.is_empty(), "{}", text(&out.stdout));
}

/// The run of `source` with standard output (when `out_full`) or standard
/// error on `/dev/full`, the other a pipe.
fn run_on_a_full_device(source: &str, out_full: bool) -> Output {
    let dir = TempDir::new(if out_full { "full-out" } else { "full-err" });
    let src = dir.file("p.fib", source);
    let mut command = run(&src, &[]);
    if out_full {
        command.stdout(full_device()).stderr(Stdio::piped());
    } else {
        command.stderr(full_device()).stdout(Stdio::piped());
    }
    command.output().expect("fibref runs")
}

/// A write to a full standard output traps in the program: the program
/// stops at its `println` (what it would print next is not printed), and
/// the report of the trap cannot be written to the same device, which
/// `fibref` says on standard error and exits 2, not 0.
#[test]
fn println_on_a_full_device_ends_the_run() {
    let out = run_on_a_full_device(STDOUT_THEN_STDERR, true);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("fibref: cannot write the report: "),
        "{stderr}"
    );
    assert!(!stderr.contains("after"), "the program went on: {stderr}");
}

/// The same on standard error: the trap is reported on standard output,
/// with the message the program's `eprintln` gives it.
#[test]
fn eprintln_on_a_full_device_is_the_trap_and_exit_1() {
    let out = run_on_a_full_device(STDERR_THEN_STDOUT, false);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.starts_with("trapped:\n"), "{stdout}");
    assert!(stdout.contains("trap: eprintln: write failed"), "{stdout}");
    assert!(!stdout.contains("after"), "the program went on: {stdout}");
}

/// A write that crosses the limit of the file that is standard error is
/// cut short, not failed: the rest is written by the loop of the
/// prelude's `write-str`, whose next call fails, and that is the trap.
/// A write that was dropped when short ended the run with `after` printed.
#[test]
fn a_write_cut_short_is_continued_and_the_failure_after_it_is_the_trap() {
    let dir = TempDir::new("short-err");
    let src = dir.file("p.fib", &long_stderr_program());
    let out_file = dir.path().join("stderr.txt");
    let argv = vec![
        OsString::from(FIBREF),
        OsString::from("run"),
        src.clone().into_os_string(),
    ];
    let out = limited(2, &argv)
        .env("OUT", &out_file)
        .stdout(Stdio::piped())
        .output()
        .expect("sh runs fibref");
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("trap: eprintln: write failed"), "{stdout}");
    assert!(!stdout.contains("after"), "the program went on: {stdout}");
    let len = fs::metadata(&out_file).expect("the file exists").len();
    assert!(len == 512 || len == 1024, "{len} bytes reached the file");
}

/// Case 192 (64 short lines and one of 81920 bytes on standard output)
/// and its standard-error twin, run as `fibref run` with `preload`, a
/// library for `LD_PRELOAD`, when given: every byte written, none twice,
/// and nothing on the other stream.
fn assert_every_byte_is_written(dir: &TempDir, preload: Option<&Path>) {
    let run_with = |src: &Path| {
        let mut command = run(src, &[]);
        if let Some(library) = preload {
            command.env("LD_PRELOAD", library);
        }
        command.output().expect("fibref runs")
    };
    let report = |result: i64| {
        format!("result: {result}\naudit:  clean=true leak-cycles=0 leaks=0 errors=0\n")
    };

    let out = run_with(&repo(CASE_192));
    assert!(out.status.success(), "{:?}", out.status);
    let want = case_192_stdout() + &report(CASE_192_RESULT);
    assert!(
        out.stdout == want.as_bytes(),
        "case 192: the output differs"
    );
    assert_eq!(text(&out.stderr), "");

    let out = run_with(&dir.file("twin.fib", STDERR_LONG_PROGRAM));
    assert!(out.status.success(), "{:?}", out.status);
    assert_eq!(text(&out.stdout), format!("done\n{}", report(5)));
    assert!(
        out.stderr == stderr_long_stderr().as_bytes(),
        "the twin: the standard error differs"
    );
}

/// A normal run still prints, and every byte of it.
#[test]
fn a_normal_run_prints_every_byte_of_a_long_line() {
    assert_every_byte_is_written(&TempDir::new("normal"), None);
}

/// A write the system takes a few bytes at a time (the library that
/// `chunking_shim` makes, preloaded, takes 7) is finished by the loop:
/// the same 82 KB of output, byte for byte, as with whole writes. A loop
/// that did not continue, or went on from the wrong byte, changes it.
#[test]
fn a_write_the_system_takes_a_few_bytes_at_a_time_is_finished() {
    let dir = TempDir::new("chunked");
    let shim = chunking_shim(&dir);
    assert_every_byte_is_written(&dir, Some(&shim));
}
