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

/// A runner whose every macro returns `(sext i64 <lit>)` around one
/// fixed literal, as a macro that builds it with `Int` or `Flt` would.
struct Literal(crate::syntax::FormKind);

impl crate::expand::MacroRunner for Literal {
    fn run(
        &mut self,
        _: &crate::expand::MacroDef,
        _: Vec<crate::syntax::Form>,
        ctx: &ExpandCtx,
    ) -> Result<crate::syntax::Form, crate::expand::ExpandError> {
        let pos = ctx.call_pos().clone();
        let f = |k| crate::syntax::Form::new(k, pos.clone());
        let sym = |s: &str| f(crate::syntax::FormKind::Sym(s.to_string()));
        let items = vec![sym("sext"), sym("i64"), f(self.0.clone())];
        Ok(f(crate::syntax::FormKind::List(items)))
    }
}

#[test]
fn a_macro_built_literal_is_range_checked() {
    use crate::syntax::{FormKind, IntWidth, ReadErrorKind};
    let src = "(defmacro m () 0) (defun f () (m))";
    let width = IntWidth::I8;
    let mut run = Literal(FormKind::Int { v: 300, width });
    let r = expand_program(read(src), &mut ExpandCtx::new(), &mut run);
    let text = "300i8".to_string();
    let error = ReadErrorKind::IntegerOutOfRange { text, width };
    assert_eq!(r.map_err(|e| e.kind), Err(K::BadLiteral { error }));
    let mut run = Literal(FormKind::Int { v: 127, width });
    let r = expand_program(read(src), &mut ExpandCtx::new(), &mut run);
    assert!(r.is_ok(), "{r:?}");
}

#[test]
fn a_macro_built_f32_is_rounded_and_finite() {
    use crate::syntax::{FltWidth, FormKind};
    let src = "(defmacro m () 0) (defun f () (m))";
    let width = FltWidth::F32;
    let mut run = Literal(FormKind::Flt { v: 1e300, width });
    let r = expand_program(read(src), &mut ExpandCtx::new(), &mut run);
    assert!(
        matches!(r, Err(ref e) if matches!(e.kind, K::BadLiteral { .. })),
        "{r:?}"
    );
    let mut run = Literal(FormKind::Flt { v: 0.1, width });
    let out = expand_program(read(src), &mut ExpandCtx::new(), &mut run).expect("expands");
    let text = out.last().map(|f| f.to_string()).unwrap_or_default();
    assert!(text.contains("0.1f32"), "{text}");
}

#[test]
fn too_many_forms() {
    let mut ctx = ExpandCtx::new();
    ctx.limits = Limits {
        max_forms: 10,
        ..Limits::default()
    };
    let r = expand_expr(one("(and a b c d)"), &mut ctx, &mut NoRunner);
    assert_eq!(r.map_err(|e| e.kind), Err(K::TooLarge { limit: 10 }));
    let r = expand_expr(one("(and a b)"), &mut ctx, &mut NoRunner);
    assert!(r.is_ok(), "{r:?}");
}

#[test]
fn a_result_that_grows_at_each_step_is_stopped() {
    // A runner standing in for (defmacro g (x) `(g (do ,x ,x))): each
    // step doubles the form, so the steps would take forever.
    struct Doubling;
    impl crate::expand::MacroRunner for Doubling {
        fn run(
            &mut self,
            _: &crate::expand::MacroDef,
            args: Vec<crate::syntax::Form>,
            ctx: &ExpandCtx,
        ) -> Result<crate::syntax::Form, crate::expand::ExpandError> {
            use crate::syntax::{Form, FormKind};
            let pos = ctx.call_pos().clone();
            let sym = |s: &str| Form::new(FormKind::Sym(s.to_string()), pos.clone());
            let x = args[0].clone();
            let body = Form::new(FormKind::List(vec![sym("do"), x.clone(), x]), pos.clone());
            Ok(Form::new(FormKind::List(vec![sym("g"), body]), pos))
        }
    }
    let src = "(defmacro g (x) x) (defun f () (g 1))";
    let r = expand_program(read(src), &mut ExpandCtx::new(), &mut Doubling);
    let limit = crate::expand::MAX_EXPANDED_FORMS;
    assert_eq!(r.map_err(|e| e.kind), Err(K::TooLarge { limit }));
}
