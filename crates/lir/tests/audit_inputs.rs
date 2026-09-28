//! lir-audit's malformed and pathological inputs: every one is an
//! error with a message, never a panic or a stack overflow.

use std::path::PathBuf;

fn audit(file: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../lir-audit")
        .join(file);
    std::fs::read_to_string(p).expect("lir-audit file")
}

#[test]
fn every_fuzz_line_is_rejected_with_a_message() {
    for line in audit("fuzz2.txt").lines() {
        let only_comment = line.trim_start().starts_with(';') || line.trim().is_empty();
        match lir::parse_and_check(line) {
            Ok(_) => assert!(only_comment, "accepted: {line}"),
            Err(ds) => assert!(!ds.is_empty() && !ds[0].message.is_empty(), "{line}"),
        }
    }
}

#[test]
fn deep_nesting_is_rejected_before_the_stack_runs_out() {
    let src = format!(
        "(define (main i32) () (block entry (ret (trunc i32 {}))))",
        audit("deepadd.txt").trim()
    );
    let ds = lir::parse_and_check(&src).unwrap_err();
    assert!(
        ds[0].message.contains("nesting deeper than 512"),
        "{:?}",
        ds[0]
    );
}

#[test]
fn legal_depth_checks_on_a_small_stack() {
    // 500 nested adds inside define/block/ret: 504 levels.
    let mut e = String::from("(i32 1)");
    for _ in 0..500 {
        e = format!("(add (i32 1) {e})");
    }
    let src = format!("(define (main i32) () (block entry (ret {e})))");
    let r = std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || lir::parse_and_check(&src).is_ok())
        .expect("thread")
        .join()
        .expect("no stack overflow");
    assert!(r);
}
