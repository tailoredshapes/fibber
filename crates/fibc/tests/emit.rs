//! The emitted lIR is a real artefact (spec/compiler.md §1): for every
//! case the compiler can lower, the text `fibc emit` would print must
//! re-read and re-check cleanly with `crates/lir` (spec/lir.md §10),
//! with no LLVM involved. A case the compiler does not lower yet is
//! listed as pending, never counted.

use std::path::Path;

use fibc::compile::compile;
use fibc::front::{check, Front};
use fibref::cases::{list_cases, parse_header, Verdict};

#[test]
fn every_emitted_module_rereads_and_rechecks() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership");
    let cases = list_cases(&dir).expect("cases/ownership readable");
    assert!(!cases.is_empty(), "no cases");
    let (mut emitted, mut pending, mut bad) = (0, Vec::new(), Vec::new());
    for path in &cases {
        let source = std::fs::read_to_string(path).expect("readable");
        let header = parse_header(path, &source).expect("header");
        if matches!(header.verdict, Verdict::Reject { .. }) {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let checked = match check(&source, name) {
            Front::Checked(c) => c,
            _ => {
                bad.push(format!("{name}: the front end rejected an accept case"));
                continue;
            }
        };
        match compile(&checked) {
            Err(u) => pending.push(format!("{name} ({})", u.0)),
            Ok(text) => match lir::parse_and_check(&text) {
                Ok(_) => emitted += 1,
                Err(ds) => bad.push(format!("{name}: {}", ds[0])),
            },
        }
    }
    assert!(
        bad.is_empty(),
        "emitted lIR that lir rejects:\n{}",
        bad.join("\n")
    );
    assert!(emitted > 0, "nothing was emitted");
    eprintln!(
        "emitted and rechecked {emitted} modules; pending {}: {}",
        pending.len(),
        pending.join(", ")
    );
}

/// The emitted text is a function of the program alone: an `async` body
/// that keeps several values live across an `await` gets its frame slots
/// in one order, run after run (stage 2 and stage 3 must emit identical
/// lIR, spec/bootstrap.md). Each std `HashSet` and `HashMap` is seeded
/// differently, so ten compilations in one process show any hash order
/// that reaches the text.
#[test]
fn emission_is_deterministic_for_async_bodies_with_live_values() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership");
    for name in [
        "174-awaits-in-a-loop-with-live-locals.fib",
        "148-await-in-guard.fib",
    ] {
        let source = std::fs::read_to_string(dir.join(name)).expect("case readable");
        let mut texts = std::collections::BTreeSet::new();
        for _ in 0..10 {
            let Front::Checked(checked) = check(&source, name) else {
                panic!("{name}: the front end rejected the case");
            };
            texts.insert(compile(&checked).expect("the case lowers"));
        }
        assert_eq!(
            texts.len(),
            1,
            "{name}: {} different texts in 10 runs",
            texts.len()
        );
    }
}

/// `(alloc n)` is `n` zero bytes (syntax §3.15): the lowering is `calloc`
/// of one block, not `malloc`, whose block is zero only when the system
/// has just given it. Case 190 shows the difference at run time; this
/// shows which call the compiler emits for the program's own `alloc`
/// (the runtime's `fib.alloc` for objects stays `malloc`: it initialises
/// every field itself).
#[test]
fn alloc_is_lowered_to_calloc_so_that_its_block_is_zeroed() {
    let source = "(defun main () -> i64 \
        (unsafe (let ((p (alloc 24))) (let ((v (load-i64 p))) (do (free p) v)))))";
    let Front::Checked(checked) = check(source, "alloc.fib") else {
        panic!("the front end rejected the program");
    };
    let text = compile(&checked).expect("the program lowers");
    assert!(
        text.contains("(call @calloc (i64 1) (i64 24))"),
        "no calloc of the block in the emitted lIR"
    );
    assert!(
        !text.contains("(call @malloc (i64 24))"),
        "the program's alloc is a plain malloc"
    );
    // And what calloc returns is checked: null is the trap "out of memory"
    // (case 195), not a pointer the program writes through.
    assert!(
        text.contains("(call @fib.alloc-check "),
        "the program's alloc does not check the block"
    );
}
