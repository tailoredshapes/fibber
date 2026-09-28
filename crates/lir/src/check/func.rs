//! One function: structure, control flow, names, then types
//! (spec/lir.md §5).

use std::collections::HashMap;

use super::cfg::Cfg;
use super::env::Env;
use super::fcx::Fcx;
use super::walk::{bindings, statements};
use crate::ast::{Function, Kind};
use crate::diag::{err, Result};

/// Check `f` completely; the first error.
pub fn check_function(env: &Env, f: &Function) -> Result<()> {
    if f.blocks.is_empty() {
        return err(f.pos, format!("function @{} has no blocks", f.name));
    }
    let labels = labels(f)?;
    structure(f)?;
    let cfg = Cfg::new(successors(f, &labels)?);
    let binds = bindings(f)?;
    let mut cx = Fcx::new(env, f, &cfg, &labels, binds);
    let order = cfg
        .rpo
        .iter()
        .copied()
        .chain((0..f.blocks.len()).filter(|&b| !cfg.reachable(b)));
    for b in order.collect::<Vec<_>>() {
        cx.block(b)?;
    }
    cx.finish_phis()
}

/// The order in which blocks are checked and lowered: reachable blocks
/// in reverse postorder (dominators first), then unreachable ones in
/// source order. For a checked function only.
pub fn block_order(f: &Function) -> Result<Vec<usize>> {
    let labels = labels(f)?;
    let cfg = Cfg::new(successors(f, &labels)?);
    let mut order = cfg.rpo.clone();
    order.extend((0..f.blocks.len()).filter(|&b| !cfg.reachable(b)));
    Ok(order)
}

fn labels(f: &Function) -> Result<HashMap<String, usize>> {
    let mut map = HashMap::new();
    for (i, b) in f.blocks.iter().enumerate() {
        if map.insert(b.label.clone(), i).is_some() {
            return err(b.pos, format!("duplicate block label {}", b.label));
        }
    }
    Ok(map)
}

/// Each block ends in exactly one terminator (spec/lir.md §5.1).
fn structure(f: &Function) -> Result<()> {
    for b in &f.blocks {
        let stmts = statements(&b.body);
        match stmts.last() {
            Some(last) if last.kind.is_terminator() => {}
            _ => {
                return err(
                    b.pos,
                    format!("block {} does not end in a terminator", b.label),
                )
            }
        }
        if let Some(t) = stmts[..stmts.len() - 1]
            .iter()
            .find(|e| e.kind.is_terminator())
        {
            return err(
                t.pos,
                format!("terminator in the middle of block {}", b.label),
            );
        }
    }
    Ok(())
}

/// Successors from each block's terminator; undefined labels and
/// branches to the entry are errors.
fn successors(f: &Function, labels: &HashMap<String, usize>) -> Result<Vec<Vec<usize>>> {
    let entry = &f.blocks[0].label;
    let mut out = Vec::new();
    for b in &f.blocks {
        let stmts = statements(&b.body);
        // Cannot fail: structure() checked every block ends in a terminator.
        let term = stmts.last().expect("checked by structure()");
        let targets: Vec<&String> = match &term.kind {
            Kind::Br(l) => vec![l],
            Kind::CondBr(_, t, e) => vec![t, e],
            Kind::Switch(_, d, cases) => std::iter::once(d)
                .chain(cases.iter().map(|c| &c.1))
                .collect(),
            _ => vec![],
        };
        let mut succ = Vec::new();
        for l in targets {
            let Some(&i) = labels.get(l) else {
                return err(term.pos, format!("undefined block {l}"));
            };
            if l == entry {
                return err(term.pos, format!("branch to the entry block {l}"));
            }
            if !succ.contains(&i) {
                succ.push(i);
            }
        }
        out.push(succ);
    }
    Ok(out)
}
