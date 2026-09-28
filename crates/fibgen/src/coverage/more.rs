//! Coverage labels for macro calls and for numbers of other widths.

use crate::ast::{Arg, Expr, Kind};
use crate::macros::Mac;
use crate::ty::Ty;

/// Labels for a macro call.
pub fn macro_labels(m: Mac, args: &[Expr], out: &mut Vec<String>) {
    out.push(format!("macro call: {}", m.name()));
    let l = match m {
        Mac::SwapSub | Mac::FlipIf if m.rewrites(args) => {
            "macro takes its argument's form apart by a vector pattern"
        }
        Mac::SwapSub | Mac::FlipIf => "macro's form match falls through to the form",
        Mac::SumAll if args.len() > 1 => "macro expands to a call of itself (spliced rest)",
        Mac::Nargs if !args.is_empty() => "macro discards its arguments unevaluated",
        Mac::Twice => "macro runs its argument twice",
        Mac::Once2 => "macro binds its argument to a gensym",
        _ => return,
    };
    out.push(l.into());
}

/// Labels for arithmetic at a width other than `i64`, on floats, and
/// conversions.
pub fn num_labels(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        Kind::Conv(op, _, _) => out.push(format!("conversion {op}")),
        Kind::Call(h, args) => {
            let first = args.first().and_then(|a| match a {
                Arg::Val(x) => Some(&x.ty),
                Arg::InOut(_) => None,
            });
            if let Some(Ty::Num(t)) = first {
                out.push(format!("arithmetic or comparison at {}", t.name()));
            }
            let cmp = matches!(h.as_str(), "=" | "!=" | "<" | "<=" | ">" | ">=");
            let float = matches!(first, Some(Ty::Num(t)) if t.is_float());
            if float && h == "rem" {
                out.push("float rem".into());
            }
            if cmp && matches!(first, Some(Ty::Derived(_))) {
                out.push("comparison through derived Eq/Ord".into());
            }
            if matches!(first, Some(Ty::Int))
                && matches!(h.as_str(), "/" | "neg" | "shl" | "shr" | "sar")
            {
                out.push("i64 division, negation or shift".into());
            }
        }
        _ => {}
    }
}
