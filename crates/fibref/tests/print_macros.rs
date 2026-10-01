//! The printing macros' text (stdlib §2.7, §4.14, tranche 1 R5) in the
//! interpreter: `fibref run` of case 751 writes exactly the lines of
//! `print_support::EXPECTED`, then a clean result.

#![cfg(unix)]

mod print_support;

use std::process::Command;

#[test]
fn the_printing_macros_write_the_text_of_the_table() {
    let (case, roots) = print_support::case_and_roots();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fibref"));
    command.arg("run");
    for root in &roots {
        command.arg("-I").arg(root);
    }
    let out = command.arg(&case).output().expect("fibref runs");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let at = stdout.rfind("result: ").expect("fibref prints a result");
    let (own, report) = stdout.split_at(at);
    if let Some(d) = print_support::first_difference(own) {
        panic!("{d}\n{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    }
    assert!(
        report.starts_with("result: 0\naudit:  clean=true"),
        "{report}"
    );
}
