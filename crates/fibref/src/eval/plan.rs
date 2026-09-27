//! The ownership plan of each body, indexed for the evaluator: the
//! same tables as [`BodyOwn`], keyed with the fast hasher, built once
//! per run. Every entry borrows the plan itself.

use crate::own::program::{
    Alloc, BodyKey, BodyOwn, CallOwn, ClosureOwn, Op, OwnedProgram, RecurOwn, Site,
};
use crate::types::ast::{Expr, ExprId, ExprKind};
use crate::types::TypedProgram;

use super::fx::{FxMap, FxSet};

/// One body's plan.
#[derive(Debug)]
pub struct Plan<'p> {
    /// The plan as the checker gave it.
    pub own: &'p BodyOwn,
    /// The non-empty `after` lists.
    pub after: FxMap<ExprId, &'p [Op]>,
    /// The calls.
    pub calls: FxMap<ExprId, &'p CallOwn>,
    /// The `recur`s.
    pub recurs: FxMap<ExprId, &'p RecurOwn>,
    /// The allocation sites.
    pub allocs: FxMap<ExprId, Alloc>,
    /// The `fn` and `async` literals.
    pub closures: FxMap<ExprId, &'p ClosureOwn>,
}

impl<'p> Plan<'p> {
    fn new(own: &'p BodyOwn) -> Self {
        Plan {
            own,
            after: own
                .exprs
                .iter()
                .filter(|(_, x)| !x.after.is_empty())
                .map(|(e, x)| (*e, x.after.as_slice()))
                .collect(),
            calls: own.calls.iter().map(|(e, c)| (*e, c)).collect(),
            recurs: own.recurs.iter().map(|(e, r)| (*e, r)).collect(),
            allocs: own.allocs.iter().map(|(e, a)| (*e, *a)).collect(),
            closures: own.closures.iter().map(|(e, c)| (*e, c)).collect(),
        }
    }
}

/// Every body's plan, by key and by index.
#[derive(Debug, Default)]
pub struct Plans<'p> {
    /// The plans.
    pub list: Vec<Plan<'p>>,
    /// Each body's index in `list`.
    pub index: FxMap<BodyKey, usize>,
}

impl<'p> Plans<'p> {
    /// Indexes every body of `o`.
    pub fn new(o: &'p OwnedProgram) -> Self {
        let mut plans = Plans::default();
        for (k, b) in &o.bodies {
            plans.index.insert(*k, plans.list.len());
            plans.list.push(Plan::new(b));
        }
        plans
    }
}

/// Every `fn` and `async` literal of the program, by id.
pub fn literals(p: &TypedProgram) -> FxMap<ExprId, &Expr> {
    let mut out = FxMap::default();
    for f in &p.globals.funs {
        collect_lits(&f.body, &mut out);
    }
    for inst in &p.globals.instances {
        for m in &inst.methods {
            collect_lits(&m.body, &mut out);
        }
    }
    for d in &p.globals.defs {
        collect_lits(&d.init, &mut out);
    }
    out
}

fn collect_lits<'p>(e: &'p Expr, out: &mut FxMap<ExprId, &'p Expr>) {
    let mut stack = vec![e];
    while let Some(e) = stack.pop() {
        if matches!(e.kind, ExprKind::Fn(_) | ExprKind::Async(..)) {
            out.insert(e.id, e);
        }
        stack.extend(super::ast::children(e));
    }
}

/// The expressions some operation of some body names by value.
pub fn value_sites(o: &OwnedProgram) -> FxSet<ExprId> {
    let mut out = FxSet::default();
    let mut add = |ops: &[Op]| {
        for op in ops {
            if let Site::Value(e) = op.site {
                out.insert(e);
            }
        }
    };
    for body in o.bodies.values() {
        body.exprs.values().for_each(|x| add(&x.after));
        body.calls.values().for_each(|c| add(&c.jump));
        body.recurs.values().for_each(|r| add(&r.jump));
    }
    out
}

/// The instance of every method use resolved to one (types §4.2).
pub fn instances(p: &TypedProgram) -> FxMap<ExprId, usize> {
    p.resolutions
        .iter()
        .filter_map(|(e, r)| match r {
            crate::types::infer::Resolution::Instance { index, .. } => Some((*e, *index)),
            _ => None,
        })
        .collect()
}
