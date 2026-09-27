//! What is used as a function value (types §8.4): the `defun`s whose
//! value is taken, which get an all-owned body with their SCC, and the
//! method implementations a method value may run, which get one each.
//!
//! A method value is dispatched like a method call (§4.2, §4.5): where
//! the checker resolved the use to an instance, the value runs that
//! instance's implementation; where it resolved it to a bound of the
//! enclosing scheme or to `dyn`, the instance is the receiver's, known
//! per specialisation to the compiler (§4.3) and from the header's type
//! id to the interpreter (§4.5), so every instance of the protocol with
//! an implementation of the method may be run.

use std::collections::BTreeSet;

use crate::types::ast::{Expr, ExprId, ExprKind, FunId, GlobalRef};
use crate::types::infer::Resolution;
use crate::types::ty::ProtoId;
use crate::types::TypedProgram;

/// The `defun`s used as values anywhere: a function name in any
/// position but the head of a call (§8.4).
pub fn value_taken(p: &TypedProgram) -> BTreeSet<FunId> {
    let mut out = BTreeSet::new();
    for_each_value(p, &mut |_, g| {
        if let GlobalRef::Fun(f) = g {
            out.insert(f);
        }
    });
    out
}

/// The method implementations `(instance, method)` that a method value
/// may run (see the module documentation). A built-in instance has no
/// implementation to run and is left out.
pub fn methods_taken(p: &TypedProgram) -> BTreeSet<(usize, usize)> {
    let mut out = BTreeSet::new();
    for_each_value(p, &mut |e, g| {
        if let GlobalRef::Method(proto, i) = g {
            out.extend(implementations(p, e, proto, i));
        }
    });
    out
}

/// The implementations the method value `e` of method `i` of `proto`
/// may run.
fn implementations(p: &TypedProgram, e: ExprId, proto: ProtoId, i: usize) -> Vec<(usize, usize)> {
    let insts = &p.globals.instances;
    let candidates: Vec<usize> = match p.resolutions.get(&e) {
        Some(Resolution::Instance { index, .. }) => vec![*index],
        _ => (0..insts.len())
            .filter(|k| insts[*k].proto == proto)
            .collect(),
    };
    candidates
        .into_iter()
        .filter_map(|k| {
            let m = insts[k].methods.iter().position(|m| m.index == i)?;
            Some((k, m))
        })
        .collect()
}

/// Calls `f` on every global named as a value (not as the head of a
/// call) in every body of the program.
fn for_each_value(p: &TypedProgram, f: &mut dyn FnMut(ExprId, GlobalRef)) {
    for d in &p.globals.funs {
        values_in(&d.body, f);
    }
    for inst in &p.globals.instances {
        for m in &inst.methods {
            values_in(&m.body, f);
        }
    }
    for d in &p.globals.defs {
        values_in(&d.init, f);
    }
}

fn values_in(e: &Expr, f: &mut dyn FnMut(ExprId, GlobalRef)) {
    match &e.kind {
        ExprKind::Global(g) => f(e.id, *g),
        ExprKind::Call(h, _) if matches!(h.kind, ExprKind::Global(_)) => {
            e.children(&mut |c| {
                if !std::ptr::eq(c, h.as_ref()) {
                    values_in(c, f);
                }
            });
        }
        _ => e.children(&mut |c| values_in(c, f)),
    }
}
