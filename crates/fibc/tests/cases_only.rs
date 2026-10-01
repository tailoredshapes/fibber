//! `fibc cases DIR --only PREFIX..` and `Harness::run_dir_only`: the same
//! block selection as `fibref cases`, with every selected case run
//! interpreted and compiled; a prefix that matches no case is exit 2.

use std::path::{Path, PathBuf};
use std::process::Command;

use fibc::harness::Harness;
use fibref::cases::{SelectError, Status};

const STDLIB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cases/stdlib");

fn harness() -> Harness {
    Harness {
        fibc: PathBuf::from(env!("CARGO_BIN_EXE_fibc")),
    }
}

#[test]
fn the_harness_runs_only_the_selected_cases_both_ways() {
    let prefixes = ["000-".to_string(), "003-".to_string()];
    let report = harness()
        .run_dir_only(Path::new(STDLIB), &prefixes)
        .expect("selected");
    let names: Vec<String> = report.results.iter().map(|r| r.name()).collect();
    assert_eq!(
        names,
        [
            "000-each-while-early-exit.fib",
            "003-trap-a-seq-that-forces-itself.fib"
        ]
    );
    assert!(
        report.results.iter().all(|r| r.status == Status::Pass),
        "{report:?}"
    );
}

#[test]
fn a_prefix_that_matches_no_case_is_an_error_not_an_empty_pass() {
    let prefixes = ["000-".to_string(), "999-".to_string()];
    match harness().run_dir_only(Path::new(STDLIB), &prefixes) {
        Err(SelectError::NoMatch(p)) => assert_eq!(p, "999-"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_binary_exits_2_naming_the_prefix() {
    let out = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .args(["cases", STDLIB, "--only", "999-"])
        .output()
        .expect("fibc runs");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no case matches 999-"), "{stderr}");
}
