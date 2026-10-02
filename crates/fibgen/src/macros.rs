//! The preamble's macros (syntax §3.16) and what each call expands to.
//!
//! The expansion here is written from each macro's meaning, not taken
//! from the interpreter's expander: it is what the model evaluates, and
//! it must agree with what `fibref` makes of the printed definition.
//! Several of them take their argument apart by a vector pattern on
//! its `Form` (syntax §3.6, matching forms), fall through to the form
//! itself when it does not have that shape, splice a rest parameter,
//! recurse, or discard their arguments unevaluated.

use crate::ast::{Arg, Expr, Kind, Pat};
use crate::ty::Ty;

/// A preamble macro.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mac {
    /// `(twice e)` is `(+ e e)`: `e` runs twice.
    Twice,
    /// `(once2 e)` is `(let ((g e)) (+ g g))` with `g` a gensym.
    Once2,
    /// `(swap-sub (- a b))` is `(- b a)`; any other form is itself.
    SwapSub,
    /// `(flip-if (if c t e))` is `(if (not c) e t)`; any other form is itself.
    FlipIf,
    /// `(sum-all x ..)` is `(+ x (sum-all ..))`, and `0` for no arguments.
    SumAll,
    /// `(nargs x ..)` is the number of its arguments, which never run.
    Nargs,
    /// `(seq s .. e)` is `(do s .. e)`.
    Seq,
}

/// Every macro.
pub const ALL: [Mac; 7] = [
    Mac::Twice,
    Mac::Once2,
    Mac::SwapSub,
    Mac::FlipIf,
    Mac::SumAll,
    Mac::Nargs,
    Mac::Seq,
];

impl Mac {
    /// The macro's name.
    pub fn name(self) -> &'static str {
        match self {
            Mac::Twice => "twice",
            Mac::Once2 => "once2",
            Mac::SwapSub => "swap-sub",
            Mac::FlipIf => "flip-if",
            Mac::SumAll => "sum-all",
            Mac::Nargs => "nargs",
            Mac::Seq => "seq",
        }
    }

    /// The `defmacro`.
    pub fn definition(self) -> &'static str {
        match self {
            Mac::Twice => "(defmacro twice (e) `(+ ,e ,e))",
            Mac::Once2 => "(defmacro once2 (e) (let ((t (gensym \"t\"))) `(let ((,t ,e)) (+ ,t ,t))))",
            Mac::SwapSub => "(defmacro swap-sub (form)\n  (match form ((List [(Sym \"-\") a b]) `(- ,b ,a)) (_ form)))",
            Mac::FlipIf => "(defmacro flip-if (form)\n  (match form ((List [(Sym \"if\") c t e]) `(if (not ,c) ,e ,t)) (_ form)))",
            Mac::SumAll => "(defmacro sum-all (... xs)\n  (match xs ([] '0) ([x & more] `(+ ,x (sum-all ,@more)))))",
            Mac::Nargs => "(defmacro nargs (... xs) (Int (vec-count xs) :i64))",
            Mac::Seq => "(defmacro seq (... steps) `(do ,@steps))",
        }
    }

    /// Whether the call's argument has the shape the macro rewrites (for
    /// the two that fall through otherwise).
    pub fn rewrites(self, args: &[Expr]) -> bool {
        match (self, args) {
            (Mac::SwapSub, [a]) => sub_operands(a).is_some(),
            (Mac::FlipIf, [a]) => matches!(a.kind, Kind::If(..)),
            _ => true,
        }
    }
}

/// `(a, b)` of a form `(- a b)`.
fn sub_operands(e: &Expr) -> Option<(&Expr, &Expr)> {
    match &e.kind {
        Kind::Call(h, args) if h == "-" => match args.as_slice() {
            [Arg::Val(a), Arg::Val(b)] => Some((a, b)),
            _ => None,
        },
        _ => None,
    }
}

fn plus(a: Expr, b: Expr) -> Expr {
    Expr::call(Ty::Int, "+", vec![a, b])
}

/// The code the call `(m args ..)` of type `ty` expands to (repeatedly:
/// a result may be another macro call, which the model expands in turn).
pub fn expand(m: Mac, args: &[Expr], ty: &Ty) -> Expr {
    let first = || args.first().cloned().unwrap_or_else(|| Expr::int(0));
    match m {
        Mac::Twice => plus(first(), first()),
        Mac::Once2 => {
            // A name no source symbol can spell, as a gensym's.
            let t = "%gensym-t";
            let body = plus(Expr::var(t, Ty::Int), Expr::var(t, Ty::Int));
            Expr::new(
                Ty::Int,
                Kind::Let(vec![(Pat::Bind(t.into()), first())], Box::new(body)),
            )
        }
        Mac::SwapSub => match args.first().and_then(sub_operands) {
            Some((a, b)) => Expr::call(Ty::Int, "-", vec![b.clone(), a.clone()]),
            None => first(),
        },
        Mac::FlipIf => match args.first().map(|a| &a.kind) {
            Some(Kind::If(c, t, e)) => {
                let not = Expr::call(Ty::Bool, "not", vec![(**c).clone()]);
                Expr::new(ty.clone(), Kind::If(Box::new(not), e.clone(), t.clone()))
            }
            _ => first(),
        },
        Mac::SumAll => match args.split_first() {
            None => Expr::int(0),
            Some((x, more)) => {
                let rest = Expr::new(Ty::Int, Kind::Macro(Mac::SumAll, more.to_vec()));
                plus(x.clone(), rest)
            }
        },
        Mac::Nargs => Expr::int(args.len() as i64),
        Mac::Seq if args.is_empty() => Expr::new(Ty::Unit, Kind::Unit),
        Mac::Seq => Expr::new(ty.clone(), Kind::Do(args.to_vec())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_sub_rewrites_only_subtractions() {
        let sub = Expr::call(Ty::Int, "-", vec![Expr::int(1), Expr::int(9)]);
        let e = expand(Mac::SwapSub, std::slice::from_ref(&sub), &Ty::Int);
        assert_eq!(
            e,
            Expr::call(Ty::Int, "-", vec![Expr::int(9), Expr::int(1)])
        );
        let add = Expr::call(Ty::Int, "+", vec![Expr::int(1), Expr::int(9)]);
        assert_eq!(
            expand(Mac::SwapSub, std::slice::from_ref(&add), &Ty::Int),
            add
        );
        assert!(Mac::SwapSub.rewrites(&[sub]) && !Mac::SwapSub.rewrites(&[add]));
    }

    #[test]
    fn nargs_counts_without_evaluating() {
        let args = vec![Expr::int(1), Expr::new(Ty::Str, Kind::Str("x".into()))];
        assert_eq!(expand(Mac::Nargs, &args, &Ty::Int), Expr::int(2));
    }
}
