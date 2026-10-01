//! A bug of the compiler that the fibber bindings of lair had to work
//! around, kept as a test now that it is fixed.
//!
//! `ptr` is a scalar with no count (types §8.1, syntax §3.15), and the
//! interpreter treats it so. The compiler's drop of an object released
//! every field whose lIR type was `ptr` (`objects/walk.rs` `child_fields`),
//! and a raw `ptr` and the pointer to an object were both `ptr` there:
//! dropping a struct, an enum variant, a `Cell` or an `Option` that held
//! a raw pointer called `fib.release` on the user's block, which
//! decremented the first word of it and, at zero, freed it. A raw `ptr`
//! is now its own lIR type, `LirTy::Raw`, which prints as `ptr` and is
//! not counted anywhere. The bindings still hold such blocks as `i64`
//! addresses (compiler/lair/ffi.fib); case 193 covers every container.
//! The program below failed with 40 for 41 before.

use std::process::Command;

use fibc::harness::interp;
use fibref::cases::{Outcome, Value};

/// 41 interpreted, and compiled now. It was 40 compiled: `hold` drops
/// `h`, which released `p`.
const PROGRAM: &str = "(defstruct H (p: ptr))
(defun hold (p: ptr) -> i64 (let ((h (H p))) 1))
(defun main () -> i64
  (unsafe
    (let ((p (alloc 8)))
      (do (store-i64 p 41)
          (hold p)
          (let ((v (load-i64 p)))
            (do (free p) v))))))
";

#[test]
fn a_struct_that_holds_a_raw_pointer_does_not_release_the_block_it_addresses() {
    let run = interp::run(PROGRAM, "ptr-field.fib");
    let Outcome::Compiled {
        result: Value::Int(interpreted),
        ..
    } = run.outcome
    else {
        panic!("the interpreter did not run it: {:?}", run.outcome);
    };
    assert_eq!(interpreted, 41);
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let file = dir.join(format!("ptr-field-{}.fib", std::process::id()));
    std::fs::write(&file, PROGRAM).expect("the program is written");
    let out = Command::new(env!("CARGO_BIN_EXE_fibc"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("fibc runs");
    let _ = std::fs::remove_file(&file);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "41", "compiled; the interpreter says 41");
}
