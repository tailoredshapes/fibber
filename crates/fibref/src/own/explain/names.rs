//! How expressions, sites, passes and reasons are written in the
//! `--explain` output.

use std::collections::HashMap;

use crate::types::ast::{Expr, ExprId, ExprKind, GlobalRef, Lit, Place};
use crate::types::TypedProgram;

use super::super::program::{Because, Callee, Op, OpKind, Pass, Site, Tail, Why};

/// Every expression of a body by id, nested literals included.
pub(super) fn index(body: &Expr) -> HashMap<ExprId, &Expr> {
    let mut out = HashMap::new();
    fn go<'e>(e: &'e Expr, out: &mut HashMap<ExprId, &'e Expr>) {
        out.insert(e.id, e);
        for c in super::super::syntactic::children_of(e) {
            go(c, out);
        }
    }
    go(body, &mut out);
    out
}

/// `@line:col`.
pub(super) fn at(e: &Expr) -> String {
    format!("@{}:{}", e.pos.line, e.pos.col)
}

/// A short rendering of an expression.
pub(super) fn show(p: &TypedProgram, e: &Expr) -> String {
    let g = &p.globals;
    match &e.kind {
        ExprKind::Local(b) => g.binding(*b).name.clone(),
        ExprKind::Global(GlobalRef::Def(d)) => g.def(*d).name.clone(),
        ExprKind::Global(GlobalRef::Fun(f)) => g.fun(*f).name.clone(),
        ExprKind::Lit(Lit::Str(s)) => format!("{s:?}"),
        ExprKind::Lit(Lit::Int(n, _)) => n.to_string(),
        ExprKind::Lit(Lit::Bool(b)) => b.to_string(),
        ExprKind::Global(g2) if !matches!(g2, GlobalRef::Fun(_) | GlobalRef::Def(_)) => {
            super::super::syntactic::head_name(g, e)
        }
        ExprKind::Deref(Place::Amp(b), _) => format!("@{}", g.binding(*b).name),
        ExprKind::Deref(Place::Expr(x), _) => format!("@{}", show(p, x)),
        ExprKind::Field(x, f, _) => format!("(. {} {f})", show(p, x)),
        ExprKind::Call(h, args) => {
            let head = super::super::syntactic::head_name(g, h);
            let head = if matches!(h.kind, ExprKind::Global(_) | ExprKind::Local(_)) {
                head
            } else {
                show(p, h)
            };
            if args.is_empty() {
                format!("({head})")
            } else {
                format!("({head} ..)")
            }
        }
        ExprKind::Fn(_) => format!("<closure {}>", at(e)),
        ExprKind::Async(..) => format!("<async {}>", at(e)),
        _ => format!("<expr {}>", at(e)),
    }
}

/// A site's name.
pub(super) fn site(p: &TypedProgram, ix: &HashMap<ExprId, &Expr>, s: Site) -> String {
    let g = &p.globals;
    match s {
        Site::Bind(b) => g.binding(b).name.clone(),
        Site::Value(e) => ix
            .get(&e)
            .map_or_else(|| format!("<value {}>", e.0), |x| show(p, x)),
        Site::Capture(_, b) => g.binding(b).name.clone(),
        Site::Env(l) => ix
            .get(&l)
            .map_or("env".into(), |x| format!("env of {}", show(p, x))),
        Site::Global(d) => g.def(d).name.clone(),
    }
}

/// `retain [x] (join)`.
pub(super) fn op(p: &TypedProgram, ix: &HashMap<ExprId, &Expr>, o: &Op) -> String {
    let kind = match o.kind {
        OpKind::Retain => "retain",
        OpKind::Release => "release",
        OpKind::EndStack => "end-stack",
    };
    format!("{kind} [{}] ({})", site(p, ix, o.site), why(o.why))
}

fn why(w: Why) -> &'static str {
    match w {
        Why::Return => "return",
        Why::Store => "store",
        Why::Join => "join",
        Why::ScopeExit => "exit",
        Why::StepEnd => "step end",
        Why::DerivedExit => "derived exit",
        Why::Discard => "discard",
        Why::ParamExit => "exit",
        Why::LoopInit => "loop init",
        Why::Jump => "jump",
        Why::RecurOld => "old loop value",
    }
}

/// How an argument is handed over.
pub(super) fn pass(x: Pass) -> &'static str {
    match x {
        Pass::Scalar => "scalar",
        Pass::Borrow => "borrow",
        Pass::Move => "moved",
        Pass::Retain => "retain",
        Pass::Acquire => "acquire",
        Pass::Forward => "forward",
        Pass::OwnCell => "own cell",
        Pass::Alias => "alias",
        Pass::KeepEnv => "env kept",
    }
}

/// `tail-call`, or `call` with the rule that made it ordinary.
pub(super) fn tail(t: Tail, callee: Callee) -> String {
    match t {
        Tail::TailCall => "tail-call".into(),
        Tail::NotInTail if callee == Callee::Ctor => "construct".into(),
        Tail::NotInTail => "call".into(),
        Tail::Ordinary(b) => format!("call ({})", because(b)),
    }
}

fn because(b: Because) -> String {
    match b {
        Because::AmpArgument => "b: & argument".into(),
        Because::FrameOwned { arg } => {
            format!("e: frame-owned argument {} at a borrowed position", arg + 1)
        }
        Because::AsyncBody => "f: async body".into(),
        Because::Extern => "extern".into(),
        Because::Store => "a store, not a call".into(),
    }
}
