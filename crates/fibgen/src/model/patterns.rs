//! Vector patterns and guarded clauses (syntax §3.6), as the model
//! evaluates them: a clause's pattern is tested and bound, then its
//! guard runs with the clause's variables in scope; a false guard moves
//! on to the next clause against the same scrutinee value.

use std::rc::Rc;

use crate::ast::{Clause, Expr, Kind, Pat, Rest};

use super::eval::{bind_pat, trap, unsupported, Env, Machine, Res};
use super::value::V;

/// Binds `[p .. & rest]` against the elements `xs`, or `None` if the
/// length or an element does not match. A rest variable is bound to a
/// new vector of the remaining elements.
pub fn bind_vector(ps: &[Pat], rest: &Option<Rest>, xs: &[V], env: &Env) -> Option<Env> {
    let fits = match rest {
        None => xs.len() == ps.len(),
        Some(_) => xs.len() >= ps.len(),
    };
    if !fits {
        return None;
    }
    let mut env2 = env.clone();
    for (p, x) in ps.iter().zip(xs) {
        env2 = bind_pat(p, x, &env2)?;
    }
    if let Some(Rest::Bind(r)) = rest {
        let tail = xs[ps.len()..].to_vec();
        env2 = env2.bind(r, V::Vector(Rc::new(tail)));
    }
    Some(env2)
}

/// Whether `p` binds a rest vector anywhere inside it.
fn binds_rest(p: &Pat) -> bool {
    match p {
        Pat::Vector(ps, r) => matches!(r, Some(Rest::Bind(_))) || ps.iter().any(binds_rest),
        Pat::Some(q) | Pat::As(q, _) => binds_rest(q),
        Pat::Ctor(_, ps) => ps.iter().any(binds_rest),
        _ => false,
    }
}

impl Machine<'_> {
    /// `(dyn P e)`, the value of `e` (types §4.5), and a `match` with
    /// guards.
    pub(super) fn ev_new(&mut self, e: &Expr, env: &Env) -> Res {
        match &e.kind {
            Kind::Dyn(_, _, x) => {
                self.trace.insert("run: dyn value made");
                self.ev(x, env)
            }
            Kind::GMatch(s, cl) => self.ev_gmatch(s, cl, env),
            Kind::Macro(m, args) => self.ev(&crate::macros::expand(*m, args, &e.ty), env),
            other => Err(unsupported(format!("node {other:?}"))),
        }
    }

    /// `(match s clause ..)` with guards.
    fn ev_gmatch(&mut self, s: &Expr, cl: &[Clause], env: &Env) -> Res {
        let v = self.ev(s, env)?;
        for c in cl {
            let Some(env2) = bind_pat(&c.pat, &v, env) else {
                continue;
            };
            if binds_rest(&c.pat) {
                self.trace.insert("run: rest vector bound");
            }
            if let Some(g) = &c.guard {
                match self.ev(g, &env2)? {
                    V::Bool(true) => {
                        self.trace.insert("run: guard true, clause taken");
                    }
                    V::Bool(false) => {
                        self.trace.insert("run: guard false, next clause tried");
                        continue;
                    }
                    other => return Err(unsupported(format!("guard gave {other:?}"))),
                }
            }
            return self.ev(&c.body, &env2);
        }
        Err(trap("match: no clause"))
    }
}
