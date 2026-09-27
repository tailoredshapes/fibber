//! The step and depth limits, and that the reader's deepest input still
//! expands at the default limits.

use super::{one, read};
use crate::expand::error::ExpandErrorKind as K;
use crate::expand::{expand_expr, expand_program, ExpandCtx, Limits, NoRunner, MAX_EXPAND_DEPTH};
use crate::syntax::MAX_DEPTH;

#[test]
fn too_many_steps() {
    let mut ctx = ExpandCtx::new();
    ctx.limits = Limits {
        max_steps: 2,
        ..Limits::default()
    };
    let r = expand_expr(one("(and a b c d)"), &mut ctx, &mut NoRunner);
    assert_eq!(r.map_err(|e| e.kind), Err(K::TooManySteps { limit: 2 }));
}

#[test]
fn steps_reset_per_top_level_form() {
    let mut ctx = ExpandCtx::new();
    ctx.limits = Limits {
        max_steps: 3,
        ..Limits::default()
    };
    let src = "(defun f () (and a b c)) (defun g () (and a b c))";
    let r = expand_program(read(src), &mut ctx, &mut NoRunner);
    assert!(r.is_ok(), "{r:?}");
}

#[test]
fn too_deep() {
    let mut ctx = ExpandCtx::new();
    ctx.limits = Limits {
        max_depth: 10,
        ..Limits::default()
    };
    let src = format!("{}x{}", "(f ".repeat(20), ")".repeat(20));
    let r = expand_expr(one(&src), &mut ctx, &mut NoRunner);
    assert_eq!(r.map_err(|e| e.kind), Err(K::TooDeep { limit: 10 }));
}

#[test]
fn a_long_and_expands_to_the_depth_limit_and_no_further() {
    // (and a1 .. an) nests n - 1 ifs; the walk needs no native stack
    // for them, so this runs on the default test thread.
    let n = MAX_EXPAND_DEPTH - 10;
    let args = "a ".repeat(n);
    let r = expand_expr(
        one(&format!("(and {args})")),
        &mut ExpandCtx::new(),
        &mut NoRunner,
    );
    assert!(r.is_ok(), "{:?}", r.err());
    let args = "a ".repeat(MAX_EXPAND_DEPTH + 10);
    let r = expand_expr(
        one(&format!("(and {args})")),
        &mut ExpandCtx::new(),
        &mut NoRunner,
    );
    let limit = MAX_EXPAND_DEPTH;
    assert_eq!(r.map_err(|e| e.kind), Err(K::TooDeep { limit }));
}

#[test]
fn deepest_readable_program_expands() {
    let n = MAX_DEPTH - 2;
    let src = format!("(defun f () {}1{})", "(g ".repeat(n), ")".repeat(n));
    let r = expand_program(read(&src), &mut ExpandCtx::new(), &mut NoRunner);
    assert!(r.is_ok(), "{:?}", r.err());
}

#[test]
fn deepest_readable_vectors_expand() {
    let n = MAX_DEPTH - 2;
    let src = format!("(defun f () {}1{})", "[".repeat(n), "]".repeat(n));
    let r = expand_program(read(&src), &mut ExpandCtx::new(), &mut NoRunner);
    assert!(r.is_ok(), "{:?}", r.err());
}
