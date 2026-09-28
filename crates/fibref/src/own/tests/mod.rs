//! Unit tests for the ownership pass: the twenty cases of
//! `cases/ownership` with the decisions §7 and §6 state for each
//! (`cases`, `cases2`), and rules of §6 one at a time (`rules`).

mod captured;
mod cases;
mod cases2;
mod methods;
mod patterns;
mod rules;

use std::path::PathBuf;

use crate::types::ast::{Expr, ExprKind};

use super::program::{
    BindingOwn, BodyKey, BodyOwn, CallOwn, ClosureOwn, Op, OpKind, ParamOwn, Site, Why,
};
use super::syntactic::{children_of, head_name};
use super::{check_source, CheckError, Checked};

/// Checks `src`, which must pass the whole front end.
pub(super) fn ok(src: &str) -> Checked {
    check_source(src, "t.fib").unwrap_or_else(|e| panic!("{src}\nfailed:\n{e}"))
}

/// Checks `src`, which must fail.
pub(super) fn rejected(src: &str) -> CheckError {
    match check_source(src, "t.fib") {
        Ok(_) => panic!("{src}\nwas accepted"),
        Err(e) => e,
    }
}

pub(super) fn case_source(name: &str) -> String {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership");
    let path = std::fs::read_dir(&dir)
        .expect("cases/ownership exists")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(&format!("{name}-")))
        })
        .unwrap_or_else(|| panic!("no case {name}"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The case whose file name starts with `name`, which must be accepted.
pub(super) fn case(name: &str) -> Checked {
    ok(&case_source(name))
}

/// The `defun` `name` of the user module: its body's decisions and AST.
pub(super) fn fun<'c>(c: &'c Checked, name: &str) -> (&'c BodyOwn, &'c Expr) {
    let f = c
        .typed
        .fun(name)
        .unwrap_or_else(|| panic!("no defun {name}"));
    let body = c
        .owned
        .bodies
        .get(&BodyKey::Fun(f))
        .unwrap_or_else(|| panic!("no body for {name}"));
    (body, &c.typed.globals.fun(f).body)
}

/// Every expression of `e`, nested literals included, in pre-order.
pub(super) fn all(e: &Expr) -> Vec<&Expr> {
    let mut out = vec![e];
    for c in children_of(e) {
        out.extend(all(c));
    }
    out
}

/// The calls in `fun` (nested literals included) whose head is named
/// `callee`, in source order.
pub(super) fn calls<'c>(
    c: &'c Checked,
    fun_name: &str,
    callee: &str,
) -> Vec<(&'c Expr, &'c CallOwn)> {
    let (b, body) = fun(c, fun_name);
    all(body)
        .into_iter()
        .filter(|e| match &e.kind {
            ExprKind::Call(h, _) => head_name(&c.typed.globals, h) == callee,
            _ => false,
        })
        .map(|e| (e, &b.calls[&e.id]))
        .collect()
}

/// The one call to `callee` in `fun`.
pub(super) fn call<'c>(c: &'c Checked, fun_name: &str, callee: &str) -> (&'c Expr, &'c CallOwn) {
    let cs = calls(c, fun_name, callee);
    assert_eq!(cs.len(), 1, "calls to {callee} in {fun_name}");
    cs[0]
}

/// The parameter `name` of `fun`.
pub(super) fn param<'c>(c: &'c Checked, fun_name: &str, name: &str) -> &'c ParamOwn {
    let (b, _) = fun(c, fun_name);
    b.params
        .iter()
        .find(|p| c.typed.globals.binding(p.binding).name == name)
        .unwrap_or_else(|| panic!("no parameter {name} in {fun_name}"))
}

/// The binding `name` bound in `fun` (the first so named).
pub(super) fn binding(c: &Checked, fun_name: &str, name: &str) -> BindingOwn {
    let (b, _) = fun(c, fun_name);
    let mut found: Vec<_> = b
        .bindings
        .iter()
        .filter(|(id, _)| c.typed.globals.binding(**id).name == name)
        .collect();
    found.sort_by_key(|(id, _)| **id);
    *found
        .first()
        .unwrap_or_else(|| panic!("no binding {name} in {fun_name}"))
        .1
}

/// The closure and task literals of `fun`, in source order.
pub(super) fn closures<'c>(c: &'c Checked, fun_name: &str) -> Vec<&'c ClosureOwn> {
    let (b, body) = fun(c, fun_name);
    all(body)
        .into_iter()
        .filter_map(|e| b.closures.get(&e.id))
        .collect()
}

/// An operation as text: `release x (exit)`, naming a site by its
/// binding, or by the head of the call that produced its value.
pub(super) fn op_text(c: &Checked, body: &Expr, o: &Op) -> String {
    let kind = match o.kind {
        OpKind::Retain => "retain",
        OpKind::Release => "release",
        OpKind::EndStack => "end-stack",
    };
    let site = match o.site {
        Site::Bind(b) | Site::Capture(_, b) => c.typed.globals.binding(b).name.clone(),
        Site::Value(e) => all(body)
            .into_iter()
            .find(|x| x.id == e)
            .map(|x| describe(c, x))
            .unwrap_or_else(|| "?".into()),
        Site::Env(_) => "env".into(),
        Site::Global(d) => c.typed.globals.def(d).name.clone(),
    };
    format!("{kind} {site} ({})", why(o.why))
}

fn describe(c: &Checked, e: &Expr) -> String {
    match &e.kind {
        ExprKind::Local(b) => c.typed.globals.binding(*b).name.clone(),
        ExprKind::Call(h, _) => format!("({})", head_name(&c.typed.globals, h)),
        ExprKind::Deref(..) => "@".into(),
        ExprKind::Fn(_) => "fn".into(),
        _ => "expr".into(),
    }
}

fn why(w: Why) -> &'static str {
    match w {
        Why::Return => "return",
        Why::Store => "store",
        Why::Join => "join",
        Why::ScopeExit => "exit",
        Why::StepEnd => "step",
        Why::DerivedExit => "derived",
        Why::Discard => "discard",
        Why::ParamExit => "param",
        Why::LoopInit => "loop",
        Why::Jump => "jump",
        Why::RecurOld => "old",
    }
}

/// Every `after` operation of `fun` (its literals included), in the
/// order they run (sub-expressions first), as text.
pub(super) fn ops(c: &Checked, fun_name: &str) -> Vec<String> {
    let (b, body) = fun(c, fun_name);
    fn post(c: &Checked, b: &BodyOwn, root: &Expr, e: &Expr, out: &mut Vec<String>) {
        for k in children_of(e) {
            post(c, b, root, k, out);
        }
        if let Some(own) = b.exprs.get(&e.id) {
            out.extend(own.after.iter().map(|o| op_text(c, root, o)));
        }
    }
    let mut out = Vec::new();
    post(c, b, body, body, &mut out);
    out
}

/// The releases before a tail call's jump, as text.
pub(super) fn jump(c: &Checked, fun_name: &str, call: &CallOwn) -> Vec<String> {
    let (_, body) = fun(c, fun_name);
    call.jump.iter().map(|o| op_text(c, body, o)).collect()
}
