//! The mutation fuzzer over the accept cases (spec/lir.md §10,
//! method.md rule 7): a small budget on every `cargo test`, a larger
//! one through `LAIR_FUZZ_COUNT`, and a check that it reports what it
//! finds.

use std::path::PathBuf;
use std::process::Command;

/// Success, standard output and standard error of `lair args..`.
fn lair(args: &[&str]) -> (bool, String, String) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(env!("CARGO_BIN_EXE_lair"))
        .args(args)
        .current_dir(root.join("../.."))
        .output()
        .expect("lair runs");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn mutants_of_every_accept_case_are_rejected_or_run() {
    let count = std::env::var("LAIR_FUZZ_COUNT").unwrap_or_else(|_| "300".into());
    let seed = std::env::var("LAIR_FUZZ_SEED").unwrap_or_else(|_| "1".into());
    let out_dir = std::env::temp_dir().join(format!("lair-fuzz-test-{}", std::process::id()));
    let (ok, out, _) = lair(&[
        "fuzz",
        "--count",
        &count,
        "--seed",
        &seed,
        "-o",
        &out_dir.to_string_lossy(),
        "cases/lir",
    ]);
    let _ = std::fs::remove_dir_all(&out_dir);
    assert!(ok, "{out}");
    assert!(out.contains(", 0 findings"), "{out}");
    assert!(out.contains(&format!("{count} mutants")), "{out}");
}

#[test]
fn the_worker_marks_how_far_a_module_got() {
    // Checked and compiled, then the program aborts: the marks say the
    // death is the program's own, which the fuzzer does not count.
    let (ok, _, err) = lair(&["fuzz-one", "crates/lair/tests/must-fail/crashes.lir"]);
    assert!(!ok);
    assert!(
        err.contains("fuzz-one: checked") && err.contains("fuzz-one: compiled"),
        "{err}"
    );
    // Rejected: no mark, a diagnostic.
    let (ok, _, err) = lair(&["fuzz-one", "cases/lir/audit/tc-mismatch.lir"]);
    assert!(!ok);
    assert!(
        !err.contains("fuzz-one:") && err.contains(": error: "),
        "{err}"
    );
}

#[test]
fn keep_writes_every_mutant_as_a_file() {
    let dir = std::env::temp_dir().join(format!("lair-fuzz-keep-{}", std::process::id()));
    let out_dir = dir.join("findings");
    let keep = dir.join("kept");
    let (ok, out, _) = lair(&[
        "fuzz",
        "--count",
        "12",
        "--seed",
        "7",
        "-o",
        &out_dir.to_string_lossy(),
        "--keep",
        &keep.to_string_lossy(),
        "cases/lir",
    ]);
    let mut names: Vec<String> = std::fs::read_dir(&keep)
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let first = std::fs::read_to_string(keep.join("seed7-m0.lir")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "{out}");
    assert_eq!(names.len(), 12, "{names:?}");
    assert!(names.contains(&"seed7-m11.lir".to_string()), "{names:?}");
    assert!(
        first.contains("(define") || first.contains("(declare"),
        "{first}"
    );
}
