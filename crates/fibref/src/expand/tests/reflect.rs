//! Expansion-time reflection (§3.16) through `ExpandCtx::reflect`.

use super::{one, read};
use crate::expand::error::ExpandErrorKind as K;
use crate::expand::types::TypeTable;
use crate::expand::{expand_program, ExpandCtx, NoRunner};

fn ctx_with(decls: &str) -> ExpandCtx {
    let mut ctx = ExpandCtx::new();
    expand_program(read(decls), &mut ctx, &mut NoRunner).unwrap_or_else(|e| panic!("{e}"));
    ctx
}

fn call(ctx: &ExpandCtx, op: &str, name: &str) -> Result<String, K> {
    let arg = one(name);
    ctx.reflect(op, &arg, &arg.pos)
        .map(|f| f.to_string())
        .map_err(|e| e.kind)
}

const DECLS: &str = "(defstruct (Pair t) (a: (Vec t) b)) (defstruct Mono (x: i64))
                     (defenum (E a) (Nothing) (One v: a) (Two a (Vec a)))";

#[test]
fn struct_reflection() {
    let ctx = ctx_with(DECLS);
    assert_eq!(call(&ctx, "struct?", "Pair"), Ok("true".into()));
    assert_eq!(call(&ctx, "struct?", "E"), Ok("false".into()));
    assert_eq!(call(&ctx, "struct-fields", "Pair"), Ok("[a b]".into()));
    assert_eq!(call(&ctx, "struct-params", "Pair"), Ok("[t b]".into()));
    assert_eq!(call(&ctx, "struct-params", "Mono"), Ok("[]".into()));
    assert_eq!(
        call(&ctx, "struct-field-types", "Pair"),
        Ok("[(Vec t) b]".into())
    );
}

#[test]
fn enum_reflection() {
    let ctx = ctx_with(DECLS);
    assert_eq!(call(&ctx, "enum?", "E"), Ok("true".into()));
    assert_eq!(call(&ctx, "enum?", "Pair"), Ok("false".into()));
    assert_eq!(call(&ctx, "enum-params", "E"), Ok("[a]".into()));
    assert_eq!(
        call(&ctx, "enum-variants", "E"),
        Ok("[(Nothing) (One a) (Two a (Vec a))]".into())
    );
}

#[test]
fn builtin_enums() {
    let ctx = ExpandCtx::new();
    assert_eq!(
        call(&ctx, "enum-variants", "Option"),
        Ok("[(nil) (some a)]".into())
    );
    assert_eq!(call(&ctx, "enum-params", "Form"), Ok("[]".into()));
    assert_eq!(
        call(&ctx, "enum-variants", "Form"),
        Ok(
            "[(Sym str) (Kw str) (Int i64 keyword) (Flt f64 keyword) (Str str) (Chr char) \
            (Bool bool) (Nil) (List (Vec Form)) (Vec (Vec Form)) (Map (Vec Form))]"
                .into()
        )
    );
}

#[test]
fn builtin_form_enum_parses() {
    let table = TypeTable::with_builtins();
    assert_eq!(table.enums.len(), 2);
    assert_eq!(table.enums["Form"].variants.len(), 11);
}

#[test]
fn reflection_on_the_wrong_kind_is_an_error() {
    let ctx = ctx_with(DECLS);
    for op in ["struct-fields", "struct-params", "struct-field-types"] {
        let name = "E".to_string();
        let expected = K::NotAStruct {
            op: crate::expand::REFLECTION_CALLS
                .iter()
                .find(|o| **o == op)
                .copied()
                .unwrap_or(""),
            name,
        };
        assert_eq!(call(&ctx, op, "E"), Err(expected));
        assert!(matches!(
            call(&ctx, op, "Unknown"),
            Err(K::NotAStruct { .. })
        ));
    }
    for op in ["enum-params", "enum-variants"] {
        assert!(matches!(call(&ctx, op, "Pair"), Err(K::NotAnEnum { .. })));
        assert!(matches!(
            call(&ctx, op, "Unknown"),
            Err(K::NotAnEnum { .. })
        ));
    }
}

#[test]
fn reflection_needs_a_symbol_and_a_known_call() {
    let ctx = ctx_with(DECLS);
    assert_eq!(
        call(&ctx, "struct?", "1"),
        Err(K::BadReflection {
            op: "struct?".into()
        })
    );
    assert_eq!(
        call(&ctx, "struct-size", "Pair"),
        Err(K::BadReflection {
            op: "struct-size".into()
        })
    );
}
