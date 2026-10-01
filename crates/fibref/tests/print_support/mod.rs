//! What case `cases/stdlib/751` writes, shared by the two tools' tests
//! (`crates/fibref/tests/print_macros.rs`, `crates/fibc/tests/print_macros.rs`).
//!
//! A case never prints to compare: the harness reads a program's result and
//! not its text. The text of `str`, `println`, `print`, `prn` and `pr` (stdlib
//! §2.7, tranche 1 R5) is therefore judged here, by running the case under
//! the tool and comparing the program's own output, line for line, with the
//! text the page's table gives.

use std::path::{Path, PathBuf};

/// The case, from the repository root.
pub const CASE: &str = "cases/stdlib/751-println-joins-its-arguments-with-a-space.fib";

/// The text the case writes: thirteen lines. `(print "x" 1)` ends the
/// sixth, `(pr \a 1.5)` the eighth, and `(pr)` then `(prn)` make the empty
/// twelfth.
pub const EXPECTED: &str =
    "5 a true\n\nsolo\nnil\nk nil 2\nx 1\n\"q\" [\"a\" \"b\"] nil\n\\a 1.5\n\
[a b] a 3\n\"a\" \\space\n(1 2) [1 2] #{7}\n\né 😀\n";

/// The repository root.
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The case's file and the library roots it names (`;; roots:`), as
/// `-I` arguments.
pub fn case_and_roots() -> (PathBuf, Vec<PathBuf>) {
    let root = repo();
    (
        root.join(CASE),
        vec![root.join("lib"), root.join("cases/stdlib/support")],
    )
}

/// The first line on which `got` and `EXPECTED` differ, for a message.
pub fn first_difference(got: &str) -> Option<String> {
    let (mut want, mut have) = (EXPECTED.lines(), got.lines());
    for n in 1.. {
        match (want.next(), have.next()) {
            (None, None) => return None,
            (w, h) if w == h => {}
            (w, h) => return Some(format!("line {n}: expected {w:?}, got {h:?}")),
        }
    }
    None
}
