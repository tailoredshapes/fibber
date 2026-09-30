//! The binary's output shape: one row per case on stdout, nothing on
//! stderr for a normal run, help spellings, and exit 1 for a case file
//! that cannot be read as text.

use std::path::Path;
use std::process::{Command, Output};

use super::support::{accept_case, reject_case, TempDir};

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

fn run_in(dir: &TempDir) -> Output {
    fibref(&["cases", &dir.path().to_string_lossy()], dir.path())
}

#[test]
fn stdout_has_one_row_per_case_and_stderr_is_empty_on_a_normal_run() {
    let dir = TempDir::new("cli-shape");
    for i in 1..=5 {
        dir.write(&format!("{i:02}.fib"), &accept_case(i));
    }
    let output = run_in(&dir);
    assert_eq!(code(&output), 0);
    assert_eq!(stderr(&output), "", "nothing belongs on stderr");
    let out = stdout(&output);
    let table: Vec<&str> = out.split("\n\n").next().unwrap_or("").lines().collect();
    assert_eq!(table.len(), 6, "heading + 5 rows:\n{out}");
    for (row, i) in table[1..].iter().zip(1..=5) {
        assert!(row.starts_with(&format!("{i:02}.fib")), "{row}");
        assert!(row.contains("pass"), "{row}");
    }
    assert!(out.ends_with('\n'), "output must end with a newline");
}

#[test]
fn exit_1_when_a_case_file_is_not_valid_utf8() {
    let dir = TempDir::new("cli-utf8");
    dir.write("01-ok.fib", &reject_case());
    dir.write_bytes(
        "02-bin.fib",
        b";; spec: \xff\xfe\n;; expect: reject\n;; error: x\n",
    );
    let output = run_in(&dir);
    let out = stdout(&output);
    assert_eq!(
        code(&output),
        1,
        "stdout:\n{out}\nstderr:\n{}",
        stderr(&output)
    );
    assert!(out.contains("02-bin.fib"), "{out}");
    assert!(out.contains("1 header error"), "{out}");
    assert!(out.contains("1 pass"), "{out}");
}

#[test]
fn header_error_row_names_the_file_the_line_and_the_problem() {
    let dir = TempDir::new("cli-header-row");
    dir.write(
        "01-bad.fib",
        ";; spec: §4\n;; expect: accept\n;; result: one\n;; audit: clean\n",
    );
    let output = run_in(&dir);
    assert_eq!(code(&output), 1);
    let out = stdout(&output);
    let row = out
        .lines()
        .find(|l| l.starts_with("01-bad.fib"))
        .unwrap_or_else(|| panic!("no row for 01-bad.fib in:\n{out}"));
    assert!(row.contains("line 3"), "{row}");
    assert!(row.contains("result"), "{row}");
    assert!(row.contains("one"), "{row}");
    assert!(!row.contains("PENDING"), "{row}");
}

#[test]
fn help_flag_spellings_exit_0_and_print_usage_to_stdout() {
    let dir = TempDir::new("cli-help-flags");
    for flag in ["--help", "-h", "help"] {
        let output = fibref(&[flag], dir.path());
        assert_eq!(code(&output), 0, "{flag}");
        assert!(
            stdout(&output).contains("usage"),
            "{flag}: {}",
            stdout(&output)
        );
        assert!(
            stdout(&output).contains("cases"),
            "{flag}: {}",
            stdout(&output)
        );
    }
}

#[test]
fn help_with_extra_arguments_is_a_usage_error() {
    let dir = TempDir::new("cli-help-extra");
    let output = fibref(&["help", "cases"], dir.path());
    assert_eq!(code(&output), 2);
}

#[test]
fn a_relative_directory_is_resolved_against_the_working_directory() {
    let dir = TempDir::new("cli-relative");
    dir.write("sub/01.fib", &accept_case(1));
    let output = fibref(&["cases", "sub"], dir.path());
    let out = stdout(&output);
    assert_eq!(code(&output), 0, "{out}\n{}", stderr(&output));
    assert!(out.contains("01.fib"), "{out}");
    assert!(out.contains("1 cases:"), "{out}");
}

#[test]
fn pending_line_is_absent_when_nothing_is_pending() {
    let dir = TempDir::new("cli-no-pending");
    dir.write("01-bad.fib", ";; spec: §4\n");
    let output = run_in(&dir);
    assert_eq!(code(&output), 1);
    let out = stdout(&output);
    assert!(!out.contains("PENDING"), "{out}");
    assert!(out.contains("0 pending"), "{out}");
}

#[test]
fn the_real_cases_print_a_pass_row_each_and_no_pending_line() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let output = fibref(&["cases", "cases/ownership"], root);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let out = stdout(&output);
    let pass_rows = out
        .lines()
        .filter(|l| l.trim_end().ends_with(" pass"))
        .count();
    assert_eq!(pass_rows, 180, "{out}");
    assert!(!out.contains("PENDING"), "{out}");
    assert!(
        out.contains("180 cases: 180 pass, 0 fail, 0 pending, 0 header error"),
        "{out}"
    );
}
