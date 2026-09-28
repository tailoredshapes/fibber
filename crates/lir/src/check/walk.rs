//! Walking expressions: sub-expressions in evaluation order, a block's
//! statements, and the names a function binds.

use std::collections::HashMap;

use crate::ast::{Callee, Expr, Function, Kind};
use crate::diag::{err, Pos, Result};
use crate::types::Type;

/// Direct sub-expressions in evaluation order (spec/lir.md §5.2).
pub fn children(e: &Expr) -> Vec<&Expr> {
    use Kind::*;
    match &e.kind {
        Local(_) | Global(_) | Int(..) | Float(..) | Null | Str(_) | Fence(..) | Unreachable
        | Br(_) | Ret(None) => vec![],
        Vector(_, es) | Struct(_, es) => es.iter().collect(),
        Un(_, a) | Cast(_, _, a) | ExtractValue(a, _) | Load(_, a) | AtomicLoad(_, _, _, a) => {
            vec![a]
        }
        Ret(Some(a)) | CondBr(a, _, _) => vec![a],
        Alloca(_, c) => c.iter().map(|b| &**b).collect(),
        Bin(_, a, b)
        | ICmp(_, a, b)
        | FCmp(_, a, b)
        | ExtractElement(a, b)
        | InsertValue(a, b, _)
        | Store(a, b)
        | AtomicStore(_, _, a, b)
        | AtomicRmw(_, _, _, a, b) => vec![a, b],
        Select(a, b, c) | InsertElement(a, b, c) | Shuffle(a, b, c) => vec![a, b, c],
        Gep { ptr, indices, .. } => std::iter::once(&**ptr).chain(indices).collect(),
        CmpXchg {
            ptr, expected, new, ..
        } => vec![ptr, expected, new],
        Call { callee, args, .. } => {
            let head = match callee {
                Callee::Indirect(p, _) => Some(&**p),
                Callee::Direct(_) => None,
            };
            head.into_iter().chain(args).collect()
        }
        Switch(v, _, cases) => std::iter::once(&**v)
            .chain(cases.iter().map(|(c, _)| c))
            .collect(),
        Phi(_, inc) => inc.iter().map(|b| &b.value).collect(),
        Let(binds, body) => binds.iter().map(|b| &b.value).chain(body).collect(),
    }
}

/// A block's statements: its forms, a top-level `let` contributing its
/// body (spec/lir.md §5.1).
pub fn statements(body: &[Expr]) -> Vec<&Expr> {
    let mut out = Vec::new();
    for e in body {
        match &e.kind {
            Kind::Let(_, inner) => out.extend(statements(inner)),
            _ => out.push(e),
        }
    }
    out
}

/// Where each name is bound: `None` for a parameter, `Some(block)` for
/// a `let`; and the names bound directly to an uncounted `alloca`.
#[derive(Default)]
pub struct Bindings {
    pub defs: HashMap<String, Option<usize>>,
    pub allocas: HashMap<String, Type>,
}

/// Collect every binding of `f`, rejecting a name bound twice.
pub fn bindings(f: &Function) -> Result<Bindings> {
    let mut b = Bindings::default();
    for (name, pos) in &f.params {
        add(&mut b, f, name, None, *pos)?;
    }
    for (i, blk) in f.blocks.iter().enumerate() {
        for e in &blk.body {
            collect(&mut b, f, e, i)?;
        }
    }
    Ok(b)
}

fn add(b: &mut Bindings, f: &Function, name: &str, blk: Option<usize>, pos: Pos) -> Result<()> {
    if b.defs.insert(name.to_string(), blk).is_some() {
        return err(pos, format!("duplicate name {name} in @{}", f.name));
    }
    Ok(())
}

fn collect(b: &mut Bindings, f: &Function, e: &Expr, blk: usize) -> Result<()> {
    if let Kind::Let(binds, _) = &e.kind {
        for bind in binds {
            collect(b, f, &bind.value, blk)?;
            add(b, f, &bind.name, Some(blk), bind.pos)?;
            if let Kind::Alloca(t, None) = &bind.value.kind {
                b.allocas.insert(bind.name.clone(), t.clone());
            }
        }
        for x in children(e).into_iter().skip(binds.len()) {
            collect(b, f, x, blk)?;
        }
        return Ok(());
    }
    children(e)
        .into_iter()
        .try_for_each(|x| collect(b, f, x, blk))
}
