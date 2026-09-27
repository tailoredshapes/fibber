//! The dependency graph of a module's `defun`s and `def`s and its SCCs
//! (spec/types.md §3.5 step 4), dependencies before dependents, ties
//! broken in source order.

use std::collections::HashMap;

use crate::types::ast::{DefId, Expr, ExprKind, FunId, GlobalRef};

/// A node of the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Node {
    /// A `defun`.
    Fun(FunId),
    /// A `def`.
    Def(DefId),
}

/// The functions and `def`s `e` names.
pub fn refs(e: &Expr, out: &mut Vec<Node>) {
    match &e.kind {
        ExprKind::Global(GlobalRef::Fun(f)) => push(out, Node::Fun(*f)),
        ExprKind::Global(GlobalRef::Def(d)) => push(out, Node::Def(*d)),
        _ => {}
    }
    e.children(&mut |c| refs(c, out));
}

fn push(out: &mut Vec<Node>, n: Node) {
    if !out.contains(&n) {
        out.push(n);
    }
}

/// The SCCs of `nodes` (in source order), whose edges are given by
/// `edges`, dependencies first (Tarjan).
pub fn sccs(nodes: &[Node], edges: &HashMap<Node, Vec<Node>>) -> Vec<Vec<Node>> {
    let mut t = Tarjan {
        edges,
        index: HashMap::new(),
        low: HashMap::new(),
        stack: Vec::new(),
        on: Vec::new(),
        next: 0,
        out: Vec::new(),
    };
    for n in nodes {
        if !t.index.contains_key(n) {
            t.visit(*n);
        }
    }
    for scc in &mut t.out {
        scc.sort_by_key(|n| nodes.iter().position(|m| m == n));
    }
    t.out
}

struct Tarjan<'a> {
    edges: &'a HashMap<Node, Vec<Node>>,
    index: HashMap<Node, usize>,
    low: HashMap<Node, usize>,
    stack: Vec<Node>,
    on: Vec<Node>,
    next: usize,
    out: Vec<Vec<Node>>,
}

impl Tarjan<'_> {
    fn visit(&mut self, v: Node) {
        self.index.insert(v, self.next);
        self.low.insert(v, self.next);
        self.next += 1;
        self.stack.push(v);
        self.on.push(v);
        let succ = self.edges.get(&v).cloned().unwrap_or_default();
        for w in succ {
            if !self.index.contains_key(&w) {
                self.visit(w);
                let lw = self.low[&w];
                if let Some(l) = self.low.get_mut(&v) {
                    *l = (*l).min(lw);
                }
            } else if self.on.contains(&w) {
                let iw = self.index[&w];
                if let Some(l) = self.low.get_mut(&v) {
                    *l = (*l).min(iw);
                }
            }
        }
        if self.low[&v] == self.index[&v] {
            let mut scc = Vec::new();
            while let Some(w) = self.stack.pop() {
                self.on.retain(|x| *x != w);
                scc.push(w);
                if w == v {
                    break;
                }
            }
            self.out.push(scc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependencies_come_first_and_cycles_group() {
        let (a, b, c) = (
            Node::Fun(FunId(0)),
            Node::Fun(FunId(1)),
            Node::Fun(FunId(2)),
        );
        let mut edges = HashMap::new();
        edges.insert(a, vec![b]);
        edges.insert(b, vec![c, a]);
        let out = sccs(&[a, b, c], &edges);
        assert_eq!(out, vec![vec![c], vec![a, b]]);
    }
}
