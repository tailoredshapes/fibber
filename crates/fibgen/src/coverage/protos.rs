//! Coverage labels for protocols, `dyn`, the colour-parameterised
//! `Hook`, vector patterns and guards.

use crate::ast::{Arg, Clause, Expr, Kind, Pat, Program, Rest};
use crate::gen::self_as_value;
use crate::ty::{Proto, Ty};

/// Labels for the `impl`s and generic helpers.
pub fn item_labels(p: &Program, out: &mut Vec<String>) {
    for i in &p.impls {
        out.push(format!("impl {}", i.proto.name()));
        if i.proto == Proto::Rank {
            out.push("impl of a protocol with a supertrait".into());
        }
        if matches!(i.target, Ty::Hook(_)) {
            out.push("impl on a colour-parameterised struct".into());
            if i.colour_var {
                out.push("impl on (Hook k), a rigid colour".into());
            } else {
                out.push("impl on (Hook :local), a head that gives a colour".into());
            }
            if i.methods.iter().any(|m| self_as_value(&m.body)) {
                out.push("impl on (Hook :local) uses self as a (Hook :local) value".into());
            }
        }
        if i.methods.len() > 1 {
            out.push("impl overrides a default method".into());
        } else {
            out.push("impl takes a default method".into());
        }
    }
    for f in &p.funs {
        if f.params.iter().any(|q| matches!(q.ty, Ty::Gen(_, true))) {
            out.push("generic helper with a written :where bound".into());
        }
        if crate::print::is_private(&f.name) {
            out.push("defun :private (used in its own module)".into());
        }
    }
}

/// Whether `p` has a vector pattern anywhere inside it.
pub fn has_vector(p: &Pat) -> bool {
    match p {
        Pat::Vector(..) => true,
        Pat::Some(q) | Pat::As(q, _) => has_vector(q),
        Pat::Ctor(_, ps) => ps.iter().any(has_vector),
        _ => false,
    }
}

/// Labels for the patterns of one clause.
fn pat_labels(p: &Pat, top: bool, out: &mut Vec<String>) {
    match p {
        Pat::Vector(ps, rest) => {
            out.push("vector pattern".into());
            match rest {
                Some(Rest::Bind(_)) => out.push("vector pattern binding a rest".into()),
                Some(Rest::Wild) => out.push("vector pattern with & _".into()),
                None => {}
            }
            if ps.iter().any(|q| matches!(q, Pat::Ctor(..) | Pat::As(..))) {
                out.push("struct pattern inside a vector pattern".into());
            }
            ps.iter().for_each(|q| pat_labels(q, false, out));
        }
        Pat::Ctor(_, ps) => {
            if !top && ps.iter().any(has_vector) {
                out.push("vector pattern inside a struct inside a vector".into());
            } else if ps.iter().any(|q| matches!(q, Pat::Vector(..))) {
                out.push("vector pattern inside a struct pattern".into());
            }
            ps.iter().for_each(|q| pat_labels(q, false, out));
        }
        Pat::Lit(_) => out.push("literal pattern".into()),
        Pat::Some(q) | Pat::As(q, _) => pat_labels(q, false, out),
        _ => {}
    }
}

/// Whether evaluating `e` may write a cell or an atom, or suspend.
fn has_effect(e: &Expr) -> bool {
    let mut yes = false;
    e.walk(&mut |n| {
        yes |= match &n.kind {
            Kind::Set(..) | Kind::SetField(..) | Kind::Await(..) => true,
            Kind::Call(h, args) => {
                matches!(h.as_str(), "swap!" | "reset!")
                    || args.iter().any(|a| matches!(a, Arg::InOut(_)))
            }
            _ => false,
        }
    });
    yes
}

/// Labels for a guarded `match`.
fn gmatch_labels(cl: &[Clause], out: &mut Vec<String>) {
    out.push("match (vector patterns / guards form)".into());
    for c in cl {
        pat_labels(&c.pat, true, out);
        if let Some(g) = &c.guard {
            out.push("guarded clause".into());
            if has_effect(g) {
                out.push("guard with a side effect".into());
            }
        }
        let rest = match &c.pat {
            Pat::Vector(_, Some(Rest::Bind(r))) => Some(r),
            _ => None,
        };
        if matches!((&c.body.kind, rest), (Kind::Var(v), Some(r)) if v == r) {
            out.push("rest leaves as the clause's value".into());
        }
    }
}

/// Whether `e` mentions a value of a sendable `dyn` or `Hook` type.
fn mentions(e: &Expr, pred: &dyn Fn(&Ty) -> bool) -> bool {
    let mut yes = false;
    e.walk(&mut |n| yes |= pred(&n.ty));
    yes
}

fn dyn_send(t: &Ty) -> bool {
    match t {
        Ty::Dyn(_, true) => true,
        Ty::Vec(t) => dyn_send(t),
        _ => false,
    }
}

/// A test on a type.
type TyPred = dyn Fn(&Ty) -> bool;

/// Labels for code that runs on another thread or as a task.
fn crossing_labels(e: &Expr, out: &mut Vec<String>) {
    let crossing: Vec<&Expr> = match &e.kind {
        Kind::Call(h, args) if matches!(h.as_str(), "spawn" | "pmap") => args
            .iter()
            .filter_map(|a| match a {
                Arg::Val(x) => Some(x),
                Arg::InOut(_) => None,
            })
            .collect(),
        Kind::Plet(bs, _) => bs.iter().map(|(_, x)| x).collect(),
        Kind::Async(b) => vec![b],
        _ => return,
    };
    if crossing.iter().any(|x| mentions(x, &dyn_send)) {
        out.push("(dyn P :send) crosses a thread or task".into());
    }
    let labels: [(&str, &TyPred); 4] = [
        ("(Hook :send) crosses a thread or task", &|t| {
            *t == Ty::Hook(true)
        }),
        ("(Job :send) crosses a thread or task", &|t| {
            *t == Ty::Job(true)
        }),
        (
            "(Weak (dyn P :send)) crosses a thread or task",
            &|t| matches!(t, Ty::Weak(d) if matches!(**d, Ty::Dyn(_, true))),
        ),
        (
            "(Atom (dyn P :send)) crosses a thread or task",
            &|t| matches!(t, Ty::Atom(d) if matches!(**d, Ty::Dyn(_, true))),
        ),
    ];
    for (l, pred) in labels {
        if crossing.iter().any(|x| mentions(x, pred)) {
            out.push(l.into());
        }
    }
}

/// Labels for method calls by their receiver.
fn method_labels(h: &str, args: &[Arg], out: &mut Vec<String>) {
    let Some(Arg::Val(recv)) = args.first() else {
        return;
    };
    if !["score", "bonus", "rank", "tier"].contains(&h) {
        if h.starts_with("gs") && matches!(recv.ty, Ty::Dyn(..)) {
            out.push("generic helper instantiated at a dyn type".into());
        }
        return;
    }
    let l = match &recv.ty {
        Ty::Dyn(Proto::Rank, _) if matches!(h, "score" | "bonus") => {
            "supertrait method called through (dyn Rank)"
        }
        Ty::Dyn(..) => "method called through dyn",
        Ty::Gen(..) => "method called on a bounded type variable",
        _ => "method called with static dispatch",
    };
    out.push(l.into());
}

/// Labels for one node.
pub fn node_labels(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        Kind::Macro(m, args) => super::more::macro_labels(*m, args, out),
        Kind::Dyn(_, send, x) => {
            out.push(
                if *send {
                    "(dyn P :send e)"
                } else {
                    "(dyn P e)"
                }
                .into(),
            );
            if matches!(x.ty, Ty::Dyn(..)) {
                out.push("dyn upcast or :send-to-plain conversion".into());
            }
        }
        Kind::VecLit(items) if matches!(e.ty, Ty::Vec(ref t) if matches!(**t, Ty::Dyn(..))) => {
            if !items.is_empty() {
                out.push("vector of dyn values".into());
            }
        }
        Kind::GMatch(_, cl) => gmatch_labels(cl, out),
        Kind::Call(h, args) if h == "Hook" => {
            let colour = if e.ty == Ty::Hook(true) {
                "send"
            } else {
                "local"
            };
            out.push(format!("(Hook :{colour}) constructed"));
            let over_cell = args.first().is_some_and(
                |a| matches!(a, Arg::Val(f) if mentions(f, &|t| matches!(t, Ty::Cell(_)))),
            );
            if colour == "local" && over_cell {
                out.push("(Hook :local) holding a closure over a cell".into());
            }
        }
        Kind::Call(h, _) if h == "Ready" => {
            let colour = if e.ty == Ty::Job(true) {
                "send"
            } else {
                "local"
            };
            out.push(format!(
                "(Job :{colour}) constructed (colour-parameterised enum)"
            ));
        }
        Kind::Var(n) if n == "Idle" => out.push("Idle (field-less variant of Job)".into()),
        Kind::Call(h, args) => method_labels(h, args, out),
        _ => {}
    }
    crossing_labels(e, out);
    super::more::num_labels(e, out);
    if let Kind::Match(_, cl) = &e.kind {
        cl.iter().for_each(|(p, _)| pat_labels(p, true, out));
    }
}
