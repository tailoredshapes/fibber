//! The `fibref` binary: `cases <dir>` prints the table and the counts,
//! exits 0 only when there is no Fail and no HeaderError, 1 otherwise,
//! 2 on bad usage or an unreadable directory. The binary runs the
//! reference interpreter, so cases carry programs; Pending cannot be
//! forced from outside (the harness's Pending rows are tested with a
//! scripted evaluator).

use std::path::Path;
use std::process::{Command, Output};

use super::support::{accept_case, accept_header, reject_case, TempDir};

const REPO_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

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

#[test]
fn exit_0_when_every_case_passes() {
    let dir = TempDir::new("cli-pass");
    dir.write("01-a.fib", &accept_case(1));
    dir.write("02-b.fib", &reject_case());
    let output = fibref(&["cases", &dir.path().to_string_lossy()], dir.path());
    let out = stdout(&output);
    assert_eq!(
        code(&output),
        0,
        "stdout:\n{out}\nstderr:\n{}",
        stderr(&output)
    );
    assert!(out.contains("01-a.fib"), "{out}");
    assert!(out.contains("02-b.fib"), "{out}");
    assert!(!out.contains("PENDING"), "{out}");
    assert!(out.contains("2 pass, 0 fail, 0 pending"), "counts:\n{out}");
}

#[test]
fn exit_1_when_a_case_fails() {
    let dir = TempDir::new("cli-fail");
    dir.write(
        "01-wrong.fib",
        &format!("{}(defun main () -> i64 2)\n", accept_header(1)),
    );
    let output = fibref(&["cases", &dir.path().to_string_lossy()], dir.path());
    let out = stdout(&output);
    assert_eq!(code(&output), 1, "stdout:\n{out}");
    assert!(out.contains("FAIL"), "{out}");
    assert!(out.contains("result: expected 1, got 2"), "{out}");
    assert!(out.contains("0 pass, 1 fail"), "{out}");
}

#[test]
fn exit_1_when_a_header_error_exists() {
    let dir = TempDir::new("cli-header-error");
    dir.write("01-a.fib", &accept_case(1));
    dir.write(
        "02-bad.fib",
        ";; spec: §4\n;; expect: accept\n;; audit: clean\n",
    );
    let output = fibref(&["cases", &dir.path().to_string_lossy()], dir.path());
    let out = stdout(&output);
    assert_eq!(
        code(&output),
        1,
        "stdout:\n{out}\nstderr:\n{}",
        stderr(&output)
    );
    assert!(out.contains("02-bad.fib"), "{out}");
    assert!(out.contains("1 header error"), "{out}");
    assert!(
        out.contains("result"),
        "the missing key must be named:\n{out}"
    );
}

#[test]
fn exit_1_when_the_only_file_has_no_header() {
    let dir = TempDir::new("cli-no-header");
    dir.write("01-a.fib", "(defun main () 1)\n");
    let output = fibref(&["cases", &dir.path().to_string_lossy()], dir.path());
    assert_eq!(code(&output), 1, "{}", stdout(&output));
}

#[test]
fn exit_2_on_a_missing_directory() {
    let dir = TempDir::new("cli-missing-dir");
    let missing = dir.path().join("nope");
    let output = fibref(&["cases", &missing.to_string_lossy()], dir.path());
    assert_eq!(code(&output), 2, "{}", stdout(&output));
    assert!(!stderr(&output).is_empty(), "an error must be reported");
}

#[test]
fn exit_2_when_the_directory_is_a_file() {
    let dir = TempDir::new("cli-dir-is-file");
    let file = dir.write("a.fib", &accept_header(1));
    let output = fibref(&["cases", &file.to_string_lossy()], dir.path());
    assert_eq!(code(&output), 2, "{}", stdout(&output));
}

#[test]
fn exit_2_without_a_command() {
    let dir = TempDir::new("cli-no-args");
    let output = fibref(&[], dir.path());
    assert_eq!(code(&output), 2);
    assert!(stderr(&output).contains("usage"), "{}", stderr(&output));
}

#[test]
fn exit_2_on_an_unknown_command() {
    let dir = TempDir::new("cli-unknown");
    let output = fibref(&["run", "cases"], dir.path());
    assert_eq!(code(&output), 2);
}

#[test]
fn exit_2_on_extra_arguments() {
    let dir = TempDir::new("cli-extra");
    let output = fibref(&["cases", "a", "b"], dir.path());
    assert_eq!(code(&output), 2);
}

#[test]
fn help_exits_0() {
    let dir = TempDir::new("cli-help");
    let output = fibref(&["help"], dir.path());
    assert_eq!(code(&output), 0);
    assert!(stdout(&output).contains("cases"), "{}", stdout(&output));
}

#[test]
fn default_directory_is_cases_ownership_relative_to_cwd() {
    // Run from the repo root: the 20 real cases are found and all pass.
    let output = fibref(&["cases"], Path::new(REPO_ROOT));
    let out = stdout(&output);
    assert_eq!(
        code(&output),
        0,
        "stdout:\n{out}\nstderr:\n{}",
        stderr(&output)
    );
    assert!(out.contains("01-return-part-of-argument.fib"), "{out}");
    assert!(out.contains("20-weak-ref-to-dead-object.fib"), "{out}");
    assert!(
        out.contains("80-unique-write-closes-cycle-through-cell.fib"),
        "{out}"
    );
    assert!(
        out.contains("95-option-of-scalar-is-a-heap-object.fib"),
        "{out}"
    );
    assert!(
        out.contains("100-annotated-let-loop-and-plet-bindings.fib"),
        "{out}"
    );
    assert!(out.contains("149-element-outlives-its-vector.fib"), "{out}");
    assert!(
        out.contains("153-two-inout-arguments-one-written-by-argument.fib"),
        "{out}"
    );
    assert!(
        out.contains("154-float-comparisons-are-ieee-not-ord-defaults.fib"),
        "{out}"
    );
    assert!(
        out.contains("161-rigid-impl-stores-send-closure-at-both-colours.fib"),
        "{out}"
    );
    assert!(
        out.contains("165-captured-inout-at-non-tail-call-unchanged.fib"),
        "{out}"
    );
    assert!(
        out.contains("168-swap-contention-between-threads.fib"),
        "{out}"
    );
    assert!(out.contains("191 cases:"), "{out}");
    assert!(out.contains("191 pass, 0 fail, 0 pending"), "{out}");
    assert!(out.contains("0 header error"), "{out}");
}

#[test]
fn default_directory_fails_with_exit_2_when_absent_from_cwd() {
    let dir = TempDir::new("cli-default-absent");
    let output = fibref(&["cases"], dir.path());
    assert_eq!(code(&output), 2, "{}", stdout(&output));
}

#[test]
fn empty_directory_exits_1_because_nothing_ran() {
    let dir = TempDir::new("cli-empty");
    let output = fibref(&["cases", &dir.path().to_string_lossy()], dir.path());
    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(stdout(&output).contains("0 cases"), "{}", stdout(&output));
    assert!(
        stderr(&output).contains("no case files"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn table_lists_cases_in_file_name_order() {
    let dir = TempDir::new("cli-order");
    for name in ["03-c.fib", "01-a.fib", "02-b.fib"] {
        dir.write(name, &accept_case(1));
    }
    let output = fibref(&["cases", &dir.path().to_string_lossy()], dir.path());
    let out = stdout(&output);
    let a = out.find("01-a.fib").unwrap();
    let b = out.find("02-b.fib").unwrap();
    let c = out.find("03-c.fib").unwrap();
    assert!(a < b && b < c, "{out}");
}

#[test]
fn run_hands_the_arguments_after_the_dashes_to_args() {
    let out = fibref(
        &[
            "run",
            "cases/ownership/186-args-and-println.fib",
            "--",
            "p",
            "q",
        ],
        Path::new(REPO_ROOT),
    );
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert!(stdout(&out).contains("result: 2"), "{}", stdout(&out));
}
