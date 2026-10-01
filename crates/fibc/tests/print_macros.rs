//! The printing macros' text (stdlib §2.7, §4.14, tranche 1 R5) compiled:
//! `fibc run` of case 751 writes exactly the lines the interpreter's test
//! (`crates/fibref/tests/print_macros.rs`) expects, then its result, 0.

#![cfg(unix)]

#[path = "../../lair/tests/common/bounded.rs"]
mod bounded;
#[path = "../../fibref/tests/print_support/mod.rs"]
mod print_support;

use std::process::Command;

#[test]
fn the_printing_macros_write_the_same_text_when_compiled() {
    let (case, roots) = print_support::case_and_roots();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fibc"));
    command.arg("run").env_remove("FIB_LIB");
    for root in &roots {
        command.arg("-I").arg(root);
    }
    command.arg(&case);
    let out = bounded::output(&mut command);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let at = stdout.trim_end().rfind('\n').map_or(0, |i| i + 1);
    let (own, result) = stdout.split_at(at);
    if let Some(d) = print_support::first_difference(own) {
        panic!("{d}\n{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    }
    assert_eq!(result.trim_end(), "0", "{stdout}");
}
