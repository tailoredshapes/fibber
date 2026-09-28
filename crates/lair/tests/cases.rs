//! The lIR case suite (cases/lir, spec/lir.md §13) through the JIT and
//! AOT, and a check that the harness can fail.

use std::path::PathBuf;
use std::process::Command;

fn lair(args: &[&str]) -> (bool, String) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(env!("CARGO_BIN_EXE_lair"))
        .args(args)
        .current_dir(root.join("../.."))
        .output()
        .expect("lair runs");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn every_lir_case_passes_on_both_paths() {
    let (ok, out) = lair(&["cases", "cases/lir"]);
    let failures: Vec<&str> = out.lines().filter(|l| l.starts_with("FAIL")).collect();
    assert!(ok && failures.is_empty(), "{}", failures.join("\n"));
    assert!(out.contains("total: "), "{out}");
}

#[test]
fn the_harness_fails_every_wrong_case() {
    let (ok, out) = lair(&["cases", "crates/lair/tests/must-fail"]);
    assert!(!ok, "{out}");
    let passed: Vec<&str> = out.lines().filter(|l| l.starts_with("ok")).collect();
    assert!(
        passed.is_empty(),
        "wrong cases passed:\n{}",
        passed.join("\n")
    );
    assert!(out.contains("total: 10 cases, 0 pass, 10 fail"), "{out}");
}

#[test]
fn no_cases_is_an_error() {
    let (ok, _) = lair(&["cases", "crates/lair/tests/nothing-here"]);
    assert!(!ok);
}
