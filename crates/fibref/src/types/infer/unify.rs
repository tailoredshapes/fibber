//! Unification (spec/types.md §3.2): Robinson with the occurs check over
//! union-find variables with levels; nominal and built-in constructors
//! unify only with themselves; rigid variables only with themselves or
//! an unbound variable; function colours are never unified but
//! constrained, `κ_from ⊑ κ_to` at a flow site and both ways elsewhere.
//!
//! At a flow site a variable meeting a function type is bound to a copy
//! of it with a fresh colour below or above the original, so that a
//! `send` closure passed through a variable-typed position keeps its own
//! colour (§5.4: colours are ordered, not equal).

use crate::syntax::Pos;

use crate::types::decls::Globals;
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::store::Store;
use crate::types::ty::{Colour, Con, TvId, Ty};

use super::cx::{ColourCon, Cx, Witness};

/// Why unification failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UErr {
    Mismatch,
    Occurs,
}

struct Unifier<'s> {
    g: &'s Globals,
    st: &'s mut Store,
    cons: &'s mut Vec<ColourCon>,
    pos: &'s Pos,
}

impl Unifier<'_> {
    fn unify(&mut self, a: &Ty, b: &Ty, flow: bool) -> Result<(), UErr> {
        let a = self.st.resolve(a);
        let b = self.st.resolve(b);
        match (&a, &b) {
            (Ty::Var(x), Ty::Var(y)) if x == y => Ok(()),
            (Ty::Var(x), Ty::Var(_)) => {
                self.st.bind(*x, b.clone());
                Ok(())
            }
            (Ty::Var(x), t) => self.bind_var(*x, t, flow, true),
            (t, Ty::Var(y)) => self.bind_var(*y, t, flow, false),
            (Ty::Rigid(i), Ty::Rigid(j)) | (Ty::Gen(i), Ty::Gen(j)) if i == j => Ok(()),
            (Ty::Con(c1, a1), Ty::Con(c2, a2)) if c1 == c2 && a1.len() == a2.len() => {
                for (i, (x, y)) in a1.iter().zip(a2).enumerate() {
                    self.fixed_colours_differ(*c1, i, x, y)?;
                    self.unify(x, y, false)?;
                }
                Ok(())
            }
            (Ty::Fn(k1, p1, r1), Ty::Fn(k2, p2, r2)) if p1.len() == p2.len() => {
                self.colour(*k1, *k2, flow, &a, &b);
                for (x, y) in p1.iter().zip(p2) {
                    self.unify(x, y, false)?;
                }
                self.unify(r1, r2, false)
            }
            _ => Err(UErr::Mismatch),
        }
    }

    /// Two different written colours at a colour parameter of a nominal
    /// type (§1.3): the arguments are invariant, so `(N :send)` and `(N
    /// :local)` do not unify, and say so, rather than as a colour flow.
    fn fixed_colours_differ(&mut self, c: Con, i: usize, x: &Ty, y: &Ty) -> Result<(), UErr> {
        let Con::Nominal(id) = c else {
            return Ok(());
        };
        if !self.g.ty(id).is_colour(i) {
            return Ok(());
        }
        let fixed = |t: Ty| match t {
            Ty::Fn(k @ (Colour::Send | Colour::Local), _, _) => Some(k),
            _ => None,
        };
        match (fixed(self.st.resolve(x)), fixed(self.st.resolve(y))) {
            (Some(a), Some(b)) if a != b => Err(UErr::Mismatch),
            _ => Ok(()),
        }
    }

    /// Binds `v` to `t`; `v_is_from` says which side of a flow `v` is.
    fn bind_var(&mut self, v: TvId, t: &Ty, flow: bool, v_is_from: bool) -> Result<(), UErr> {
        if self.st.occurs(v, t) {
            return Err(UErr::Occurs);
        }
        match t {
            Ty::Fn(k, ps, r) if flow => {
                let k2 = self.st.fresh_colour();
                let copy = Ty::Fn(k2, ps.clone(), r.clone());
                self.st.bind(v, copy.clone());
                if v_is_from {
                    self.colour(k2, *k, true, &copy, t);
                } else {
                    self.colour(*k, k2, true, t, &copy);
                }
            }
            _ => self.st.bind(v, t.clone()),
        }
        Ok(())
    }

    /// `from ⊑ to`, and `to ⊑ from` unless at a flow site.
    fn colour(&mut self, from: Colour, to: Colour, flow: bool, from_ty: &Ty, to_ty: &Ty) {
        if from == to {
            return;
        }
        self.push(from, to, from_ty);
        if !flow {
            self.push(to, from, to_ty);
        }
    }

    fn push(&mut self, from: Colour, to: Colour, from_ty: &Ty) {
        let origin = (from == Colour::Local).then(|| Witness {
            path: Vec::new(),
            offending: self.st.zonk(from_ty),
        });
        self.cons.push(ColourCon {
            from,
            to,
            label: None,
            origin,
            pos: self.pos.clone(),
        });
    }
}

impl Cx<'_> {
    fn unify_mode(&mut self, a: &Ty, b: &Ty, flow: bool, pos: &Pos) -> TResult<()> {
        let mut u = Unifier {
            g: self.g,
            st: &mut *self.st,
            cons: &mut self.u.colours,
            pos,
        };
        match u.unify(a, b, flow) {
            Ok(()) => Ok(()),
            Err(UErr::Mismatch) => Err(self.mismatch(a, b, pos)),
            Err(UErr::Occurs) => {
                let msg = format!(
                    "cannot construct the infinite type: {} = {}",
                    self.show(a),
                    self.show(b)
                );
                Err(TypeError::new(ErrorKind::Infinite, pos, msg))
            }
        }
    }

    /// Unifies `a` and `b` (not a flow site: colours made equal).
    pub fn unify(&mut self, a: &Ty, b: &Ty, pos: &Pos) -> TResult<()> {
        self.unify_mode(a, b, false, pos)
    }

    /// `from` flows into a position of type `to` (§3.2 flow sites).
    pub fn flow(&mut self, from: &Ty, to: &Ty, pos: &Pos) -> TResult<()> {
        self.unify_mode(from, to, true, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn pos() -> Pos {
        Pos {
            file: Arc::from("t"),
            line: 1,
            col: 1,
            start: 0,
            end: 0,
        }
    }

    fn run(st: &mut Store, a: &Ty, b: &Ty, flow: bool) -> (Result<(), UErr>, Vec<ColourCon>) {
        let mut cons = Vec::new();
        let p = pos();
        let g = crate::types::init::new_globals().expect("builtins");
        let r = Unifier {
            g: &g,
            st,
            cons: &mut cons,
            pos: &p,
        }
        .unify(a, b, flow);
        (r, cons)
    }

    #[test]
    fn occurs_check_fails() {
        let mut st = Store::new();
        let a = st.fresh();
        let (r, _) = run(&mut st, &a, &Ty::cell(a.clone()), false);
        assert_eq!(r, Err(UErr::Occurs));
    }

    #[test]
    fn rigid_unifies_only_with_itself_or_a_variable() {
        let mut st = Store::new();
        assert_eq!(
            run(&mut st, &Ty::Rigid(0), &Ty::i64(), false).0,
            Err(UErr::Mismatch)
        );
        assert_eq!(
            run(&mut st, &Ty::Rigid(0), &Ty::Rigid(1), false).0,
            Err(UErr::Mismatch)
        );
        let v = st.fresh();
        assert_eq!(run(&mut st, &v, &Ty::Rigid(0), false).0, Ok(()));
    }

    #[test]
    fn colours_flow_one_way_at_flow_sites_and_both_ways_elsewhere() {
        let mut st = Store::new();
        let f = |k| Ty::Fn(k, vec![], Box::new(Ty::i64()));
        let (_, cons) = run(&mut st, &f(Colour::Local), &f(Colour::Send), true);
        assert_eq!(cons.len(), 1);
        assert!(cons[0].origin.is_some());
        let (_, cons) = run(&mut st, &f(Colour::Local), &f(Colour::Send), false);
        assert_eq!(cons.len(), 2);
    }

    #[test]
    fn a_variable_at_a_flow_site_gets_a_copy_with_its_own_colour() {
        let mut st = Store::new();
        let v = st.fresh();
        let f = Ty::Fn(Colour::Send, vec![], Box::new(Ty::i64()));
        let (r, cons) = run(&mut st, &f, &v, true);
        assert_eq!(r, Ok(()));
        assert_eq!(cons.len(), 1);
        assert_eq!(cons[0].from, Colour::Send);
        assert!(matches!(st.zonk(&v), Ty::Fn(Colour::Var(_), _, _)));
        let (r, _) = run(&mut st, &Ty::Con(Con::Str, vec![]), &Ty::i64(), false);
        assert_eq!(r, Err(UErr::Mismatch));
    }
}
