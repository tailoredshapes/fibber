//! `fibref cases DIR --only PREFIX..` and the `open` label, through the
//! binary: a package runs its own block of `cases/stdlib` in place; a
//! prefix that matches no case is exit 2 and never a silent pass; a case
//! that waits for an item is OPEN (listed, exit 0) while it fails and a
//! failure (exit 1) the day it passes.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn fibref(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fibref"))
        .args(args)
        .current_dir(ROOT)
        .output()
        .expect("fibref runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn only_runs_the_named_block_and_nothing_else() {
    let out = fibref(&["cases", "cases/stdlib", "--only", "000-", "001-"]);
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "{stdout}\n{stderr}");
    assert!(stdout.contains("000-each-while-early-exit.fib"), "{stdout}");
    assert!(stdout.contains("001-a-user-tree"), "{stdout}");
    assert!(!stdout.contains("002-"), "{stdout}");
    assert!(
        stdout.contains("2 cases: 2 pass, 0 fail, 0 pending, 0 header error"),
        "{stdout}"
    );
}

#[test]
fn a_prefix_that_matches_no_case_is_exit_2_and_names_the_prefix() {
    for prefixes in [vec!["999-"], vec!["000-", "999-"]] {
        let mut args = vec!["cases", "cases/stdlib", "--only"];
        args.extend(prefixes.iter().copied());
        let out = fibref(&args);
        assert_eq!(out.status.code(), Some(2), "{prefixes:?}");
        assert!(
            text(&out.stderr).contains("no case matches 999-"),
            "{}",
            text(&out.stderr)
        );
        assert_eq!(text(&out.stdout), "", "nothing ran");
    }
}

#[test]
fn an_open_case_is_listed_with_its_item_and_is_not_a_failure() {
    let out = fibref(&["cases", "cases/stdlib", "--only", "900-"]);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("OPEN"), "{stdout}");
    assert!(
        stdout.contains("L20: expected accept, but rejected"),
        "{stdout}"
    );
    assert!(
        stdout.contains("1 cases: 0 pass, 0 fail, 0 pending, 0 header error, 1 open"),
        "{stdout}"
    );
}

#[test]
fn an_open_case_that_passes_is_exit_1_and_says_to_remove_the_label() {
    let dir: PathBuf = std::env::temp_dir().join(format!("fibref-open-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let case = ";; spec: s\n;; expect: accept\n;; result: 3\n;; audit: clean\n;; open: L20\n(defun main () -> i64 3)\n";
    fs::write(dir.join("01-stale.fib"), case).expect("write");
    let out = fibref(&["cases", &dir.to_string_lossy()]);
    let _ = fs::remove_dir_all(&dir);
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("the item landed: remove `open`"),
        "{stdout}"
    );
    assert!(stdout.contains("1 cases: 0 pass, 1 fail"), "{stdout}");
}
