//! A `ccc` entry to a `tailcc` function (spec/lir.md §11): Rust speaks
//! only the C convention, so the `Jit` generates, as an lIR module
//! through the same pipeline, a function of the same parameters and
//! result that calls the `tailcc` one.

use lir::types::{show_ret, Cc, FnType};

/// The name of `name`'s trampoline.
pub fn name_of(name: &str) -> String {
    format!("{name}.tramp")
}

/// The lIR source of the trampoline for `name : ty`, a `tailcc`
/// function defined in the `Jit`.
pub fn source(name: &str, ty: &FnType) -> String {
    debug_assert_eq!(ty.cc, Cc::Tail);
    let ret = show_ret(&ty.ret);
    let params: Vec<String> = ty
        .params
        .iter()
        .enumerate()
        .map(|(i, t)| format!("({t} a{i})"))
        .collect();
    let args: Vec<String> = (0..ty.params.len()).map(|i| format!("a{i}")).collect();
    let call = format!("(call @{name} {})", args.join(" "));
    let body = match ty.ret {
        Some(_) => format!("(ret {call})"),
        None => format!("{call} (ret)"),
    };
    format!(
        "(declare tailcc {name} {ret} {})\n(define ({} {ret}) ({}) (block entry {body}))\n",
        ty.show_params(),
        name_of(name),
        params.join(" ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lir::Type;

    #[test]
    fn writes_a_ccc_function_that_calls_the_tailcc_one() {
        let ty = FnType {
            cc: Cc::Tail,
            ret: Some(Type::Int(64)),
            params: vec![Type::Int(64), Type::Ptr],
            varargs: false,
        };
        let src = source("loop", &ty);
        assert_eq!(
            src,
            "(declare tailcc loop i64 (i64 ptr))\n(define (loop.tramp i64) ((i64 a0) (ptr a1)) (block entry (ret (call @loop a0 a1))))\n"
        );
        let m = lir::parse_and_check(&src).unwrap();
        assert_eq!(m.items.len(), 2);
        let void = FnType {
            ret: None,
            params: vec![],
            ..ty
        };
        assert!(source("f", &void).contains("(block entry (call @f ) (ret))"));
        assert!(lir::parse_and_check(&source("f", &void)).is_ok());
    }
}
