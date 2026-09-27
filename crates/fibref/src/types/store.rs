//! The substitution (spec/types.md §3.1): unification variables as
//! union-find nodes carrying a Rémy level, and colour variables.
//!
//! A variable is either unbound, with the level (SCC depth) at which it
//! was created, or linked to a type. [`Store::resolve`] follows links
//! with path compression; [`Store::zonk`] substitutes every bound
//! variable of a type.

use super::ty::{Colour, CvId, TvId, Ty};

/// One unification variable.
#[derive(Clone, Debug)]
enum Node {
    /// Not bound; created at this level.
    Unbound(u32),
    /// Bound to this type (possibly another variable).
    Link(Ty),
}

/// Unification variables, colour variables and the current level.
#[derive(Clone, Debug, Default)]
pub struct Store {
    nodes: Vec<Node>,
    colours: u32,
    level: u32,
}

impl Store {
    /// An empty store at level 0 (the top level, where only closed
    /// schemes live).
    pub fn new() -> Self {
        Store::default()
    }

    /// The current level.
    pub fn level(&self) -> u32 {
        self.level
    }

    /// Enters a unit (an SCC, an impl, a def): new variables get the
    /// next level.
    pub fn enter(&mut self) {
        self.level += 1;
    }

    /// Leaves the unit.
    pub fn leave(&mut self) {
        self.level = self.level.saturating_sub(1);
    }

    /// A fresh unification variable at the current level.
    pub fn fresh(&mut self) -> Ty {
        let id = TvId(self.nodes.len() as u32);
        self.nodes.push(Node::Unbound(self.level));
        Ty::Var(id)
    }

    /// A fresh colour variable.
    pub fn fresh_colour(&mut self) -> Colour {
        let id = CvId(self.colours);
        self.colours += 1;
        Colour::Var(id)
    }

    /// The level of an unbound variable, `None` if it is bound.
    pub fn level_of(&self, v: TvId) -> Option<u32> {
        match self.nodes.get(v.0 as usize) {
            Some(Node::Unbound(l)) => Some(*l),
            _ => None,
        }
    }

    /// Follows the links from `t` to its representative: an unbound
    /// variable or a non-variable type. Compresses the path.
    pub fn resolve(&mut self, t: &Ty) -> Ty {
        let Ty::Var(v) = t else {
            return t.clone();
        };
        let mut path = Vec::new();
        let mut cur = *v;
        let rep = loop {
            match self.nodes.get(cur.0 as usize) {
                Some(Node::Link(Ty::Var(next))) => {
                    path.push(cur);
                    cur = *next;
                }
                Some(Node::Link(t)) => break t.clone(),
                _ => break Ty::Var(cur),
            }
        };
        for p in path {
            self.nodes[p.0 as usize] = Node::Link(rep.clone());
        }
        rep
    }

    /// Binds the unbound variable `v` to `t`, lowering the level of the
    /// unbound variables of `t` to `v`'s (Rémy). The caller has done
    /// the occurs check.
    pub fn bind(&mut self, v: TvId, t: Ty) {
        let level = self.level_of(v).unwrap_or(self.level);
        for u in t.vars() {
            let u = match self.resolve(&Ty::Var(u)) {
                Ty::Var(u) => u,
                _ => continue,
            };
            if let Some(Node::Unbound(l)) = self.nodes.get_mut(u.0 as usize) {
                *l = (*l).min(level);
            }
        }
        self.nodes[v.0 as usize] = Node::Link(t);
    }

    /// `t` with every bound variable replaced by what it is bound to.
    pub fn zonk(&mut self, t: &Ty) -> Ty {
        match self.resolve(t) {
            Ty::Con(c, args) => Ty::Con(c, args.iter().map(|a| self.zonk(a)).collect()),
            Ty::Fn(k, ps, r) => Ty::Fn(
                k,
                ps.iter().map(|p| self.zonk(p)).collect(),
                Box::new(self.zonk(&r)),
            ),
            other => other,
        }
    }

    /// Whether the unbound variable `v` occurs in `t`.
    pub fn occurs(&mut self, v: TvId, t: &Ty) -> bool {
        self.zonk(t).vars().contains(&v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_follows_links_and_zonk_substitutes() {
        let mut s = Store::new();
        let a = s.fresh();
        let b = s.fresh();
        let (Ty::Var(va), Ty::Var(vb)) = (a.clone(), b.clone()) else {
            unreachable!()
        };
        s.bind(va, b.clone());
        s.bind(vb, Ty::i64());
        assert_eq!(s.resolve(&a), Ty::i64());
        assert_eq!(s.zonk(&Ty::cell(a)), Ty::cell(Ty::i64()));
    }

    #[test]
    fn binding_lowers_levels() {
        let mut s = Store::new();
        s.enter();
        let outer = s.fresh();
        s.enter();
        let inner = s.fresh();
        let Ty::Var(vo) = outer else { unreachable!() };
        let Ty::Var(vi) = inner.clone() else {
            unreachable!()
        };
        assert_eq!(s.level_of(vi), Some(2));
        s.bind(vo, Ty::cell(inner));
        assert_eq!(s.level_of(vi), Some(1));
    }

    #[test]
    fn occurs_sees_through_links() {
        let mut s = Store::new();
        let a = s.fresh();
        let b = s.fresh();
        let Ty::Var(va) = a.clone() else {
            unreachable!()
        };
        let Ty::Var(vb) = b.clone() else {
            unreachable!()
        };
        s.bind(vb, Ty::cell(a));
        assert!(s.occurs(va, &b));
    }
}
