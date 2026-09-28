//! Walking the typed AST with the program's lifetime (the AST's own
//! `Expr::children` lends its children only to a callback).

use crate::types::ast::{Arg, Expr, ExprKind, Place};

/// The direct sub-expressions of `e`, in order.
pub fn children(e: &Expr) -> Vec<&Expr> {
    match &e.kind {
        ExprKind::Lit(_) | ExprKind::Local(_) | ExprKind::Global(_) | ExprKind::Quote(_) => {
            Vec::new()
        }
        ExprKind::Call(h, args) => std::iter::once(h.as_ref())
            .chain(args.iter().filter_map(|a| match a {
                Arg::Expr(x) => Some(x),
                Arg::Amp(..) => None,
            }))
            .collect(),
        ExprKind::Fn(lit) => vec![&lit.body],
        ExprKind::Let(bs, body) => bs
            .iter()
            .map(|(_, x)| x)
            .chain(std::iter::once(body.as_ref()))
            .collect(),
        ExprKind::Match(s, cls) => std::iter::once(s.as_ref())
            .chain(
                cls.iter()
                    .flat_map(|c| c.guard.iter().chain(std::iter::once(&c.body))),
            )
            .collect(),
        ExprKind::If(c, t, f) => vec![c, t, f],
        ExprKind::Do(es) | ExprKind::Recur(es) | ExprKind::Concat(es) => es.iter().collect(),
        ExprKind::Loop(vs, body) => vs
            .iter()
            .map(|(_, x)| x)
            .chain(std::iter::once(body.as_ref()))
            .collect(),
        ExprKind::Deref(p, _) => place(p),
        ExprKind::Set(p, v) => {
            let mut out = place(p);
            out.push(v);
            out
        }
        ExprKind::Field(x, _, _)
        | ExprKind::SetField(_, _, x)
        | ExprKind::Async(x, _)
        | ExprKind::Await(x)
        | ExprKind::Unsafe(x)
        | ExprKind::Dyn(_, _, _, x)
        | ExprKind::Convert(_, _, x) => vec![x],
    }
}

fn place(p: &Place) -> Vec<&Expr> {
    match p {
        Place::Expr(e) => vec![e],
        Place::Amp(_) => Vec::new(),
    }
}
