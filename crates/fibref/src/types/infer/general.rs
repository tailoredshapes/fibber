//! The end of a unit (spec/types.md §3.5 steps c–e, §3.6): run the
//! worklist, report unresolved field and `deref` constraints, check
//! ambiguity, solve colours, and generalise every variable left in the
//! members' types (Rémy: all of them were created at the unit's level,
//! and Γ holds only closed schemes). A quantified variable with no
//! constraints that does not occur in the type is instantiated to `i64`
//! (§3.6).

use std::collections::HashMap;

use crate::types::ast::ExprId;
use crate::types::display::letter_name;
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::scheme::{ColourBound, Scheme};
use crate::types::store::Store;
use crate::types::ty::{Colour, CvId, Leaf, Pred, TvId, Ty};

use super::colour::Colours;
use super::cx::{Cx, DKind, Deferred, Resolution};
use super::send::send_eval;

/// A variable that can be quantified.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A unification variable.
    Var(TvId),
    /// A rigid variable.
    Rigid(u32),
}

/// The keys of a type (zonked), in order.
pub fn keys(st: &mut Store, t: &Ty) -> Vec<Key> {
    let mut out = Vec::new();
    st.zonk(t).visit(&mut |l| {
        let k = match l {
            Leaf::Var(v) => Key::Var(v),
            Leaf::Rigid(r) => Key::Rigid(r),
            _ => return,
        };
        if !out.contains(&k) {
            out.push(k);
        }
    });
    out
}

/// What closing a unit leaves: the variables of each root and the
/// stuck constraints (as predicates, with their method sites).
pub struct Closed {
    /// Per root: the variables it quantifies (its own, and those
    /// determined through its constraints).
    pub sets: Vec<Vec<Key>>,
    /// The colour solution.
    pub colours: Colours,
    /// The remaining constraints.
    pub preds: Vec<(Pred, Option<ExprId>)>,
}

/// Maps a unit's variables to a scheme's `Gen`s.
pub struct GenMap {
    vars: HashMap<Key, u32>,
    /// A display name per `Gen`.
    pub names: Vec<String>,
    colours: Colours,
    colour_gen: HashMap<CvId, u32>,
}

impl GenMap {
    /// A map quantifying `order`, and the free colour variables of
    /// `colour_roots`.
    pub fn new(cx: &mut Cx<'_>, order: &[Key], colours: Colours, colour_roots: &[Ty]) -> Self {
        let mut names = Vec::new();
        let mut next = 0;
        for k in order {
            let name = match k {
                Key::Rigid(r) => {
                    cx.u.rigid_names
                        .get(*r as usize)
                        .cloned()
                        .unwrap_or_default()
                }
                Key::Var(_) => loop {
                    let n = letter_name(next);
                    next += 1;
                    if !cx.u.rigid_names.contains(&n) {
                        break n;
                    }
                },
            };
            names.push(name);
        }
        let vars = order
            .iter()
            .enumerate()
            .map(|(i, k)| (*k, i as u32))
            .collect();
        let mut colour_gen = HashMap::new();
        for t in colour_roots {
            cx.st.zonk(t).visit(&mut |l| {
                if let Leaf::Colour(Colour::Var(v)) = l {
                    if colours.value(Colour::Var(v)) == Colour::Var(v)
                        && !colour_gen.contains_key(&v)
                    {
                        colour_gen.insert(v, colour_gen.len() as u32);
                    }
                }
            });
        }
        GenMap {
            vars,
            names,
            colours,
            colour_gen,
        }
    }

    /// The number of quantified colour variables.
    pub fn n_colours(&self) -> u32 {
        self.colour_gen.len() as u32
    }

    /// A colour under the map: solved, quantified, or `send` (least).
    pub fn colour(&self, k: Colour) -> Colour {
        match self.colours.value(k) {
            Colour::Var(v) => self
                .colour_gen
                .get(&v)
                .map_or(Colour::Send, |i| Colour::Gen(*i)),
            other => other,
        }
    }

    fn quantified(&self, k: Colour) -> Option<u32> {
        match k {
            Colour::Var(v) => self.colour_gen.get(&v).copied(),
            _ => None,
        }
    }

    /// A type under the map; a variable it does not quantify is `i64`.
    pub fn apply(&self, st: &mut Store, t: &Ty) -> Ty {
        st.zonk(t)
            .map_leaves(&mut |l| match l {
                Ty::Var(v) => Some(
                    self.vars
                        .get(&Key::Var(*v))
                        .map_or(Ty::i64(), |i| Ty::Gen(*i)),
                ),
                Ty::Rigid(r) => self.vars.get(&Key::Rigid(*r)).map(|i| Ty::Gen(*i)),
                _ => None,
            })
            .map_colours(&mut |k| self.colour(k))
    }

    /// A predicate under the map.
    pub fn pred(&self, st: &mut Store, p: &Pred) -> Pred {
        p.map_tys(&mut |t| self.apply(st, t))
    }

    /// The colour bounds that remain between quantified colours (§5.4
    /// step 4), following flows through unquantified variables.
    pub fn colour_bounds(&self, st: &mut Store) -> Vec<ColourBound> {
        let mut out = Vec::new();
        for &v in self.colour_gen.keys() {
            for w in self.reach(Colour::Var(v)) {
                let b = ColourBound::Flow(self.colour(Colour::Var(v)), Colour::Gen(w));
                if w != self.colour_gen[&v] && !out.contains(&b) {
                    out.push(b);
                }
            }
        }
        for (k, t) in self.colours.symbolic.clone() {
            for w in self.reach(k) {
                let b = ColourBound::Caps(Colour::Gen(w), self.apply(st, &t));
                if !out.contains(&b) {
                    out.push(b);
                }
            }
        }
        out.sort_by_key(|b| format!("{b:?}"));
        out
    }

    /// Quantified colours reachable from `k` along flows, `k` included.
    fn reach(&self, k: Colour) -> Vec<u32> {
        let mut seen = vec![k];
        let mut i = 0;
        while i < seen.len() {
            let cur = seen[i];
            for (a, b) in &self.colours.flows {
                if *a == cur && !seen.contains(b) {
                    seen.push(*b);
                }
            }
            i += 1;
        }
        seen.into_iter()
            .filter_map(|c| self.quantified(c))
            .collect()
    }
}

impl Cx<'_> {
    /// Steps c–e for a unit whose roots are `roots` (the members' types;
    /// none for an `impl` body or a `def`).
    pub fn close(&mut self, roots: &[Ty]) -> TResult<Closed> {
        self.solve_all()?;
        self.check_unresolved()?;
        self.reduce_sends();
        let mut sets: Vec<Vec<Key>> = roots.iter().map(|t| keys(self.st, t)).collect();
        for set in &mut sets {
            self.determined(set);
        }
        let union: Vec<Key> = sets.iter().flatten().copied().collect();
        self.check_ambiguous(&union)?;
        self.default_outside(&union)?;
        let colours = self.solve_colours()?;
        self.solve_all()?;
        self.default_outside(&union)?;
        let preds = self.u.deferred.iter().filter_map(pred_of).collect();
        Ok(Closed {
            sets,
            colours,
            preds,
        })
    }

    fn check_unresolved(&mut self) -> TResult<()> {
        for d in self.u.deferred.clone() {
            let (kind, msg) = match &d.kind {
                DKind::Field(_, f, _, text) => (
                    ErrorKind::FieldUnresolved,
                    format!("cannot infer the struct type of {text} for field {f}; annotate it"),
                ),
                DKind::Deref(_, _, text) => (
                    ErrorKind::DerefUnresolved,
                    format!("cannot infer whether {text} is a cell, an atom or a weak reference"),
                ),
                DKind::Float(t) => (
                    ErrorKind::Ambiguous,
                    format!(
                        "cannot infer which float type {} is; annotate it",
                        self.show(t)
                    ),
                ),
                _ => continue,
            };
            return Err(TypeError::new(kind, &d.pos, msg));
        }
        Ok(())
    }

    /// Replaces each stuck `Send T` by `Send a` for its variables, and
    /// emits `κ ⊑ send` for its colour variables.
    fn reduce_sends(&mut self) {
        let mut out = Vec::new();
        for d in std::mem::take(&mut self.u.deferred) {
            let DKind::Send(t, path) = &d.kind else {
                out.push(d);
                continue;
            };
            let Ok(needs) = send_eval(self.g, self.st, t, path) else {
                out.push(d);
                continue;
            };
            for (k, p) in needs.colours {
                let label = (!p.is_empty()).then(|| p.join(", "));
                self.u.colours.push(super::cx::ColourCon {
                    from: k,
                    to: Colour::Send,
                    label,
                    origin: None,
                    pos: d.pos.clone(),
                });
            }
            for (v, p) in needs.vars {
                out.push(Deferred {
                    kind: DKind::Send(v, p),
                    ..d.clone()
                });
            }
        }
        self.u.deferred = out;
    }

    /// Adds to `set` the variables determined by its protocol constraints.
    fn determined(&mut self, set: &mut Vec<Key>) {
        loop {
            let mut changed = false;
            for d in self.u.deferred.clone() {
                if let DKind::Proto(_, args, _) = &d.kind {
                    let dispatch = keys(self.st, &args[0]);
                    if dispatch.iter().any(|k| set.contains(k)) {
                        for a in args {
                            for k in keys(self.st, a) {
                                if !set.contains(&k) {
                                    set.push(k);
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
            if !changed {
                return;
            }
        }
    }

    fn check_ambiguous(&mut self, union: &[Key]) -> TResult<()> {
        for d in self.u.deferred.clone() {
            if let DKind::Proto(p, args, _) = &d.kind {
                let ks = keys(self.st, &args[0]);
                if ks.iter().any(|k| !union.contains(k)) {
                    let msg = format!(
                        "ambiguous constraint {} {} in {}; add an annotation",
                        self.g.proto(*p).name,
                        self.show(&args[0]),
                        d.fun
                    );
                    return Err(TypeError::new(ErrorKind::Ambiguous, &d.pos, msg));
                }
            }
        }
        Ok(())
    }

    /// `Send`/`Object` on a variable outside every root: `i64` (§3.6).
    fn default_outside(&mut self, union: &[Key]) -> TResult<()> {
        let mut changed = false;
        for d in self.u.deferred.clone() {
            if let DKind::Send(t, _) | DKind::Object(t) = &d.kind {
                for k in keys(self.st, t) {
                    if let (Key::Var(v), false) = (k, union.contains(&k)) {
                        self.unify(&Ty::Var(v), &Ty::i64(), &d.pos)?;
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.solve_all()?;
        }
        Ok(())
    }

    /// Records every remaining method site as resolved by a bound, and
    /// every type of the unit under `map`.
    pub fn finalize(&mut self, map: &GenMap, closed: &Closed) {
        for (p, site) in &closed.preds {
            if let Some(site) = site {
                let bound = map.pred(self.st, p);
                self.t.resolutions.insert(*site, Resolution::Bound(bound));
            }
        }
        for id in std::mem::take(&mut self.u.exprs) {
            if let Some(t) = self.t.expr_types.get(&id).cloned() {
                self.t.expr_types.insert(id, map.apply(self.st, &t));
            }
        }
        for b in std::mem::take(&mut self.u.bindings) {
            if let Some(t) = self.t.binding_types.get(&b).cloned() {
                self.t.binding_types.insert(b, map.apply(self.st, &t));
            }
        }
        self.finalize_uses(map);
    }

    fn finalize_uses(&mut self, map: &GenMap) {
        for id in std::mem::take(&mut self.u.insts) {
            if let Some(mut i) = self.t.instantiations.get(&id).cloned() {
                i.tys = i.tys.iter().map(|t| map.apply(self.st, t)).collect();
                i.colours = i.colours.iter().map(|k| map.colour(*k)).collect();
                self.t.instantiations.insert(id, i);
            }
        }
        for id in std::mem::take(&mut self.u.sites) {
            let mapped = match self.t.resolutions.get(&id).cloned() {
                Some(Resolution::Instance { index, args }) => {
                    let args = args.iter().map(|t| map.apply(self.st, t)).collect();
                    Resolution::Instance { index, args }
                }
                Some(Resolution::Bound(p)) => Resolution::Bound(map.pred(self.st, &p)),
                Some(Resolution::Dyn) | None => continue,
            };
            self.t.resolutions.insert(id, mapped);
        }
        for (id, k) in std::mem::take(&mut self.u.fn_lits) {
            self.t.fn_colours.insert(id, map.colour(k));
        }
    }

    /// The scheme of root `i` of a closed unit.
    pub fn scheme(
        &mut self,
        map: &GenMap,
        closed: &Closed,
        i: usize,
        root: &Ty,
        amps: Vec<bool>,
        params: Vec<String>,
    ) -> Scheme {
        let set = &closed.sets[i];
        let mut preds = Vec::new();
        for (p, _) in &closed.preds {
            let ks: Vec<Key> = p.tys().iter().flat_map(|t| keys(self.st, t)).collect();
            let q = map.pred(self.st, p);
            if ks.iter().all(|k| set.contains(k)) && !preds.contains(&q) {
                preds.push(q);
            }
        }
        Scheme {
            n_vars: map.names.len() as u32,
            n_colours: map.n_colours(),
            var_names: map.names.clone(),
            preds,
            colour_bounds: map.colour_bounds(self.st),
            ty: map.apply(self.st, root),
            amps,
            params,
        }
    }
}

fn pred_of(d: &Deferred) -> Option<(Pred, Option<ExprId>)> {
    let p = match &d.kind {
        DKind::Proto(p, args, _) => Pred::Proto(*p, args.clone()),
        DKind::Send(t, _) => Pred::Send(t.clone()),
        DKind::Object(t) => Pred::Object(t.clone()),
        _ => return None,
    };
    Some((p, d.site))
}
