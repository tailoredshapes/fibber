//! Polymorphic recursion that never ends (spec/types.md §3.6; stdlib
//! design §7 B4).
//!
//! A fully annotated `defun` may call itself, or another member of its
//! SCC, at other types than its own: the annotation is the scheme of
//! those occurrences. `fibc` compiles a body once for each type a
//! bounded variable takes, so a function that calls itself at a type
//! built *from its own variable* (`len` over a `c` calling `len` at
//! `(Dropped c e)`) is wanted at `c`, then at `(Dropped c e)`, then at
//! `(Dropped (Dropped c e) e)`, without end, where the interpreter, which
//! compiles nothing, ran it and said 3. Both tools refuse it here, in the
//! checker they share.
//!
//! The rule: draw an edge from each variable `r` of a caller to the
//! variable `v` of the callee that a recursive occurrence instantiates
//! with a type mentioning `r`, **strict** when that type is more than `r`
//! itself. A cycle of edges holding a strict one makes the types grow
//! with every turn around it, so it is an error; a cycle of plain edges
//! only permutes the variables (a finite set of instances), and a strict
//! edge on no cycle (`a` at `(Vec b)`, with `b` never fed back) is
//! finitely deep. A call at a type with no variable of the caller's in it
//! (`f` at `i64`, at `(Cell i64)`) draws no edge.

use std::collections::HashMap;

use crate::syntax::Pos;
use crate::types::ast::{ExprId, FunId};
use crate::types::decls::FunDef;
use crate::types::error::{TResult, TypeError};
use crate::types::ty::{Leaf, Ty};

use super::cx::Cx;

/// One occurrence of an annotated member of the SCC, at its annotation
/// (§3.6), in the body of a member.
#[derive(Clone, Debug)]
pub struct PolyCall {
    /// The member whose body holds the occurrence.
    pub caller: FunId,
    /// The member it names.
    pub callee: FunId,
    /// The occurrence, whose instantiation is recorded.
    pub site: ExprId,
    /// Where it is.
    pub pos: Pos,
}

/// An edge of the instantiation graph.
struct Edge {
    /// The caller's variable (a rigid index).
    from: u32,
    /// The callee's variable.
    to: u32,
    /// The type given to `to`, more than just `from`.
    strict: bool,
    /// The occurrence, by index into the unit's calls.
    call: usize,
    /// The type given to `to`.
    ty: Ty,
}

/// Whether `to` leads back to `from`, through `edges`.
fn reaches(edges: &[Edge], to: u32, from: u32) -> bool {
    let mut seen = vec![to];
    let mut at = 0;
    while let Some(&v) = seen.get(at) {
        if v == from {
            return true;
        }
        at += 1;
        for e in edges.iter().filter(|e| e.from == v) {
            if !seen.contains(&e.to) {
                seen.push(e.to);
            }
        }
    }
    false
}

impl Cx<'_> {
    /// The edges of the occurrences recorded in this unit, once their
    /// types are solved.
    fn instantiation_edges(&mut self) -> Vec<Edge> {
        let mut edges = Vec::new();
        for (call, c) in self.u.poly_calls.clone().iter().enumerate() {
            let (Some(inst), Some(vars)) = (
                self.t.instantiations.get(&c.site).cloned(),
                self.u.poly_vars.get(&c.callee).cloned(),
            ) else {
                continue;
            };
            for (to, t) in vars.iter().zip(&inst.tys) {
                let ty = self.st.zonk(t);
                let mut mentioned = Vec::new();
                ty.visit(&mut |l| match l {
                    Leaf::Rigid(r) if !mentioned.contains(&r) => mentioned.push(r),
                    _ => {}
                });
                for from in mentioned {
                    let strict = ty != Ty::Rigid(from);
                    let ty = ty.clone();
                    edges.push(Edge {
                        from,
                        to: *to,
                        strict,
                        call,
                        ty,
                    });
                }
            }
        }
        edges
    }

    /// Rejects a member that is called at a growing type around a cycle
    /// of the SCC's occurrences (see the module comment).
    pub fn check_polymorphic_recursion(
        &mut self,
        members: &[&FunDef],
        ids: &[FunId],
    ) -> TResult<()> {
        let edges = self.instantiation_edges();
        let Some(e) = edges
            .iter()
            .find(|e| e.strict && reaches(&edges, e.to, e.from))
        else {
            return Ok(());
        };
        let c = self.u.poly_calls[e.call].clone();
        let name_of = |f: FunId| {
            let i = ids.iter().position(|x| *x == f);
            i.map_or("the function", |i| members[i].name.as_str())
        };
        let ty =
            crate::types::display::Printer::with_names(self.g, &[], &self.u.rigid_names).ty(&e.ty);
        let through = match c.caller == c.callee {
            true => String::new(),
            false => format!(" through {}", name_of(c.callee)),
        };
        let msg = format!(
            "{} recurses{through} at {ty}: polymorphic recursion is not supported; use loop or a List",
            name_of(c.caller)
        );
        Err(TypeError::other(&c.pos, msg))
    }
}

/// The rigid variable of each quantified variable of an annotation
/// scheme (`u32::MAX`, which no variable is, for one the member's names
/// do not hold), by the names the scheme gives them.
pub fn poly_vars(var_names: &[String], rigid_map: &HashMap<String, u32>) -> Vec<u32> {
    var_names
        .iter()
        .map(|n| rigid_map.get(n).copied().unwrap_or(u32::MAX))
        .collect()
}
