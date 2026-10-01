//! Inference (spec/types.md §3): the per-module algorithm over the
//! lowered AST, one unit at a time, with one substitution for the whole
//! program. A unit that fails reports its first error; the units that
//! depend on it are skipped rather than reporting consequences of it.

mod call;
mod colour;
mod cx;
mod deps;
mod exhaust;
mod expr;
mod general;
mod pattern;
mod polyrec;
mod scc;
mod send;
mod solve;
mod unify;
mod units;

use std::collections::{HashMap, HashSet};

use crate::types::ast::{Expr, FunId};
use crate::types::decls::Globals;
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::lower::ModuleItems;
use crate::types::scheme::Scheme;
use crate::types::store::Store;
use crate::types::ty::{Colour, Ty};

use cx::{Cx, Unit};
pub use cx::{Env, Instantiation, Resolution, Tables};
use deps::{refs, sccs, Node};

/// One unit as it was checked, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitRef {
    /// An SCC of `defun`s, generalised together.
    Scc(Vec<FunId>),
    /// A `def`.
    Def(crate::types::ast::DefId),
    /// Method `method` (index into the instance's bodies) of instance
    /// `instance`.
    ImplMethod(usize, usize),
    /// A `defmacro`.
    Macro(FunId),
}

/// Checks modules in order against one set of global tables.
pub struct Checker<'g> {
    g: &'g Globals,
    /// Schemes and `def` types so far.
    pub env: Env,
    st: Store,
    /// The typed program's tables.
    pub t: Tables,
    /// Every error, in unit order.
    pub errors: Vec<TypeError>,
    /// The units in the order they were checked.
    pub order: Vec<UnitRef>,
    failed: HashSet<Node>,
}

impl<'g> Checker<'g> {
    /// A checker over `g`.
    pub fn new(g: &'g Globals) -> Self {
        let env = Env {
            funs: vec![None; g.funs.len()],
            defs: vec![None; g.defs.len()],
        };
        Checker {
            g,
            env,
            st: Store::new(),
            t: Tables::default(),
            errors: Vec::new(),
            order: Vec::new(),
            failed: HashSet::new(),
        }
    }

    fn cx(&mut self) -> Cx<'_> {
        Cx {
            g: self.g,
            env: &self.env,
            st: &mut self.st,
            t: &mut self.t,
            u: Unit::default(),
        }
    }

    /// Whether `e` names a unit that failed.
    fn blocked(&self, e: &Expr) -> bool {
        let mut out = Vec::new();
        refs(e, &mut out);
        out.iter().any(|n| self.failed.contains(n))
    }

    /// Steps 4–6 for one module.
    pub fn module(&mut self, items: &ModuleItems) {
        let mut nodes: Vec<Node> = items.funs.iter().map(|f| Node::Fun(*f)).collect();
        nodes.extend(items.defs.iter().map(|d| Node::Def(*d)));
        let mut edges = HashMap::new();
        for n in &nodes {
            let mut out = Vec::new();
            refs(self.node_expr(*n), &mut out);
            out.retain(|m| nodes.contains(m));
            edges.insert(*n, out);
        }
        for scc in sccs(&nodes, &edges) {
            self.scc_unit(&scc, &edges);
        }
        for inst in &items.impls {
            for m in 0..self.g.instances[*inst].methods.len() {
                self.impl_unit(*inst, m);
            }
        }
        for f in &items.macros {
            self.macro_unit(*f);
        }
    }

    fn node_expr(&self, n: Node) -> &'g Expr {
        match n {
            Node::Fun(f) => &self.g.fun(f).body,
            Node::Def(d) => &self.g.def(d).init,
        }
    }

    fn scc_unit(&mut self, scc: &[Node], edges: &HashMap<Node, Vec<Node>>) {
        if scc.iter().any(|n| self.blocked(self.node_expr(*n))) {
            self.failed.extend(scc.iter().copied());
            return;
        }
        if let Err(e) = self.def_cycle(scc, edges) {
            self.fail(scc, e);
            return;
        }
        match scc {
            [Node::Def(d)] => {
                self.order.push(UnitRef::Def(*d));
                let r = self.cx().infer_def(*d);
                match r {
                    Ok(t) => self.env.defs[d.0 as usize] = Some(t),
                    Err(e) => self.fail(scc, e),
                }
            }
            _ => self.fun_scc(scc),
        }
    }

    /// `def g and defun f depend on each other` (§2.16).
    fn def_cycle(&self, scc: &[Node], edges: &HashMap<Node, Vec<Node>>) -> TResult<()> {
        let self_loop = |n: &Node| edges.get(n).is_some_and(|es| es.contains(n));
        let def = scc.iter().find_map(|n| match n {
            Node::Def(d) if scc.len() > 1 || self_loop(n) => Some(*d),
            _ => None,
        });
        let Some(d) = def else { return Ok(()) };
        let other = scc.iter().find_map(|n| match n {
            Node::Fun(f) => Some(self.g.fun(*f).name.clone()),
            Node::Def(e) if *e != d => Some(self.g.def(*e).name.clone()),
            _ => None,
        });
        let dd = self.g.def(d);
        let other = other.unwrap_or_else(|| dd.name.clone());
        let msg = format!("def {} and defun {other} depend on each other", dd.name);
        Err(TypeError::new(ErrorKind::DefCycle, &dd.pos, msg))
    }

    fn fun_scc(&mut self, scc: &[Node]) {
        let ids: Vec<FunId> = scc
            .iter()
            .filter_map(|n| if let Node::Fun(f) = n { Some(*f) } else { None })
            .collect();
        self.order.push(UnitRef::Scc(ids.clone()));
        let g = self.g;
        let members: Vec<_> = ids.iter().map(|f| g.fun(*f)).collect();
        self.st.enter();
        let r = self.cx().infer_scc(&members, &ids);
        self.st.leave();
        match r {
            Ok(schemes) => {
                for (f, s) in ids.iter().zip(schemes) {
                    self.env.funs[f.0 as usize] = Some(s);
                }
            }
            Err(e) => self.fail(scc, e),
        }
    }

    fn fail(&mut self, scc: &[Node], e: TypeError) {
        self.errors.push(e);
        self.failed.extend(scc.iter().copied());
    }

    fn impl_unit(&mut self, inst: usize, m: usize) {
        if self.blocked(&self.g.instances[inst].methods[m].body) {
            return;
        }
        self.order.push(UnitRef::ImplMethod(inst, m));
        self.st.enter();
        let r = self.cx().infer_impl_method(inst, m);
        self.st.leave();
        if let Err(e) = r {
            self.errors.push(e);
        }
    }

    fn macro_unit(&mut self, f: FunId) {
        if self.blocked(&self.g.fun(f).body) {
            return;
        }
        self.order.push(UnitRef::Macro(f));
        let g = self.g;
        self.st.enter();
        let r = self.cx().infer_scc(&[g.fun(f)], &[f]);
        self.st.leave();
        match r {
            Ok(mut s) => self.env.funs[f.0 as usize] = s.pop(),
            Err(e) => self.errors.push(e),
        }
    }

    /// Step 7: `main : (fn () i64)`.
    pub fn check_main(&mut self) {
        let g = self.g;
        let Some(crate::types::ast::GlobalRef::Fun(f)) =
            g.names(g.main).values.get("main").copied()
        else {
            let pos = crate::types::init::builtin_pos();
            self.errors
                .push(TypeError::other(&pos, "the program has no main"));
            return;
        };
        let Some(Some(s)) = self.env.funs.get(f.0 as usize) else {
            return;
        };
        if !is_main_type(s) {
            let t = crate::types::display::Printer::with_names(g, &s.var_names, &[]).ty(&s.ty);
            let msg = format!("main must have type (fn () i64), not {t}");
            self.errors.push(TypeError::other(&g.fun(f).pos, msg));
        }
    }
}

fn is_main_type(s: &Scheme) -> bool {
    matches!(&s.ty, Ty::Fn(Colour::Send, ps, r) if ps.is_empty() && **r == Ty::i64())
        && s.preds.is_empty()
}
