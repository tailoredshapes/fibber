//! Round 3: the binary under conditions a CI pipeline produces. A
//! closed stdout (`fibref cases | head`), an empty directory argument,
//! a directory reached through a symlink, a broken case link, and what
//! goes to stdout versus stderr on each exit code.

use std::io::Read;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use super::support::{accept_header, reject_header, TempDir};

fn fibref(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fibref"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("fibref binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("fibref exits normally")
}

/// Enough cases that the table is larger than a pipe buffer (64 KiB on
/// Linux), so the binary must block on stdout and see the reader gone.
fn big_dir() -> TempDir {
    let dir = TempDir::new("cli-big");
    for i in 0..4000 {
        dir.write(&format!("{i:04}-case.fib"), &reject_header("x"));
    }
    dir
}

#[test]
fn a_closed_stdout_does_not_panic_the_binary() {
    // `fibref cases | head -1` closes stdout early. A Rust `print!` to
    // a closed pipe panics with "failed printing to stdout" and exits
    // 101, which CI reads as a crash. The exit code must come from the
    // report (0 here: every case is pending), or at worst from a clean
    // broken-pipe exit; never from a panic.
    let dir = big_dir();
    let mut child = Command::new(env!("CARGO_BIN_EXE_fibref"))
        .args(["cases", &dir.path().to_string_lossy()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("fibref binary runs");
    // Close the read end without reading: the first write past the
    // pipe buffer fails with EPIPE.
    drop(child.stdout.take());
    let mut err = String::new();
    child
        .stderr
        .take()
        .expect("stderr is piped")
        .read_to_string(&mut err)
        .unwrap();
    let status = child.wait().unwrap();
    assert!(
        !err.contains("panicked"),
        "the binary panicked on a closed stdout:\n{err}"
    );
    assert_ne!(status.code(), Some(101), "exit 101 is a panic: {err}");
}

#[test]
fn a_large_report_is_printed_completely_when_stdout_is_read() {
    let dir = big_dir();
    let output = fibref(&["cases", &dir.path().to_string_lossy()], dir.path());
    let out = stdout(&output);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        out.contains("4000 cases: 0 pass, 0 fail, 4000 pending"),
        "{}",
        &out[out.len().saturating_sub(200)..]
    );
    assert_eq!(out.matches("PENDING  no interpreter yet").count(), 4000);
}

#[test]
fn an_empty_directory_argument_is_exit_2_with_nothing_on_stdout() {
    let dir = TempDir::new("cli-empty-arg");
    let output = fibref(&["cases", ""], dir.path());
    assert_eq!(code(&output), 2, "{}", stdout(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(!stderr(&output).is_empty());
}

#[test]
fn a_missing_directory_puts_nothing_on_stdout() {
    let dir = TempDir::new("cli-missing-stdout");
    let missing = dir.path().join("nope");
    let output = fibref(&["cases", &missing.to_string_lossy()], dir.path());
    assert_eq!(code(&output), 2);
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    let err = stderr(&output);
    assert!(
        err.contains("nope"),
        "the message must name the directory: {err}"
    );
}

#[test]
fn a_usage_error_puts_nothing_on_stdout() {
    let dir = TempDir::new("cli-usage-stdout");
    let output = fibref(&["cases", "a", "b"], dir.path());
    assert_eq!(code(&output), 2);
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(stderr(&output).contains("usage"), "{}", stderr(&output));
}

#[test]
fn a_directory_reached_through_a_symlink_runs() {
    let dir = TempDir::new("cli-symlink-dir");
    dir.write("real/01.fib", &reject_header("x"));
    symlink(dir.path().join("real"), dir.path().join("link")).unwrap();
    let output = fibref(&["cases", "link"], dir.path());
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stdout(&output).contains("01.fib"), "{}", stdout(&output));
}

#[test]
fn a_broken_case_link_is_a_header_error_row_and_exit_1() {
    // Same interpretation as the runner test: an entry named like a
    // case that cannot be read is reported, not dropped, so CI sees it.
    let dir = TempDir::new("cli-dangling");
    dir.write("01-ok.fib", &reject_header("x"));
    symlink(
        dir.path().join("nowhere.fib"),
        dir.path().join("02-gone.fib"),
    )
    .unwrap();
    let output = fibref(&["cases", "."], dir.path());
    let out = stdout(&output);
    assert_eq!(code(&output), 1, "stdout:\n{out}");
    assert!(out.contains("02-gone.fib"), "{out}");
    assert!(out.contains("HEADER"), "{out}");
    assert!(out.contains("1 header error"), "{out}");
}

#[test]
fn stdout_ends_with_the_counts_or_the_pending_line() {
    let dir = TempDir::new("cli-tail");
    dir.write("01.fib", &accept_header(1));
    let output = fibref(&["cases", "."], dir.path());
    let out = stdout(&output);
    assert_eq!(code(&output), 0);
    let last = out.lines().last().unwrap_or("");
    assert!(last.starts_with("PENDING:"), "{out}");
    assert!(
        out.contains("\n1 cases: 0 pass, 0 fail, 1 pending, 0 header error\n"),
        "{out}"
    );
}

#[test]
fn the_dot_directory_and_its_absolute_path_give_the_same_table() {
    let dir = TempDir::new("cli-dot-vs-abs");
    dir.write("01.fib", &reject_header("x"));
    dir.write("02.fib", "no header\n");
    let relative = fibref(&["cases", "."], dir.path());
    let absolute = fibref(&["cases", &dir.path().to_string_lossy()], dir.path());
    assert_eq!(code(&relative), 1);
    assert_eq!(code(&absolute), 1);
    assert_eq!(stdout(&relative), stdout(&absolute));
}
