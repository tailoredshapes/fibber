//! The worklist of deferred constraints (spec/types.md §3.3), run to a
//! fixpoint: each constraint is retried until it is solved (possibly
//! replaced by an instance's context), fails, or nothing changes. Stuck
//! protocol constraints with equal dispatch types are improved through
//! the functional dependency `s → d̄` (§3, Jones-style improvement).

use std::collections::HashMap;

use crate::types::decls::Shape;
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::ty::{Colour, Con, Pred, ProtoId, Ty};

use super::cx::{ColourCon, Cx, DKind, Deferred, Resolution};
use super::send::{send_error, send_eval};

/// What one attempt did.
pub(super) enum Step {
    /// Solved; these constraints replace it.
    Done(Vec<Deferred>),
    /// Not decidable yet.
    Stuck,
}

impl Cx<'_> {
    /// Runs the worklist to a fixpoint. Stops at the first failure.
    pub fn solve_all(&mut self) -> TResult<()> {
        loop {
            let work = std::mem::take(&mut self.u.deferred);
            let mut progress = false;
            let mut keep = Vec::new();
            for d in work {
                match self.try_solve(&d)? {
                    Step::Done(new) => {
                        progress = true;
                        keep.extend(new);
                    }
                    Step::Stuck => keep.push(d),
                }
            }
            keep.append(&mut self.u.deferred);
            self.u.deferred = keep;
            progress |= self.improve()?;
            if !progress {
                return Ok(());
            }
        }
    }

    fn try_solve(&mut self, d: &Deferred) -> TResult<Step> {
        match &d.kind {
            DKind::Proto(p, args, method) => self.solve_proto(d, *p, args, *method),
            DKind::Send(t, path) => self.solve_send(d, t, path),
            DKind::Object(t, who) => self.solve_object(d, t, who),
            DKind::Weakable(t) => self.solve_weakable(d, t),
            DKind::Field(t, f, r, _) => self.solve_field(d, t, f, r),
            DKind::Deref(t, r, _) => self.solve_deref(d, t, r),
            DKind::Keyword(site, s, k, r, dflt) => self.solve_keyword(d, *site, s, k, r, dflt),
            DKind::VecPat(s, parts) => match self.st.resolve(s) {
                Ty::Var(_) => Ok(Step::Stuck),
                _ => self
                    .settle_vec_pattern(s, parts, &d.pos)
                    .map(|()| Step::Done(Vec::new())),
            },
            DKind::Float(t) => match self.st.resolve(t) {
                Ty::Var(_) => Ok(Step::Stuck),
                Ty::Con(Con::Scalar(s), _) if s.is_float() => Ok(Step::Done(Vec::new())),
                other => {
                    let msg = format!("cannot unify {} with a float type", self.show(&other));
                    Err(TypeError::new(ErrorKind::Unify, &d.pos, msg))
                }
            },
        }
    }

    fn solve_proto(
        &mut self,
        d: &Deferred,
        p: ProtoId,
        args: &[Ty],
        method: Option<usize>,
    ) -> TResult<Step> {
        let t1 = self.st.resolve(&args[0]);
        match &t1 {
            Ty::Var(_) | Ty::Gen(_) => Ok(Step::Stuck),
            Ty::Rigid(r) => self.rigid_proto(d, p, *r, args),
            Ty::Con(Con::Dyn(q, _), dargs) => {
                // `P` itself, or a supertrait of it with the determined
                // arguments read off the dyn's (§4.1 rule 3).
                let Some(dargs) = self.dyn_args(*q, dargs, p, &t1) else {
                    return Err(self.no_instance(p, &t1, &d.pos));
                };
                if let Some(m) = method {
                    let md = &self.g.proto(p).methods[m];
                    if md.self_elsewhere {
                        let msg = format!(
                            "method {} of {} is not callable through dyn",
                            md.name,
                            self.g.proto(p).name
                        );
                        return Err(TypeError::other(&d.pos, msg));
                    }
                }
                for (a, b) in dargs.iter().zip(&args[1..]) {
                    self.unify(a, b, &d.pos)?;
                }
                self.resolved(d, Resolution::Dyn);
                Ok(Step::Done(Vec::new()))
            }
            Ty::Con(c, _) => self.apply_instance(d, p, *c, &t1, args),
            Ty::Fn(..) => Err(self.no_instance(p, &t1, &d.pos)),
        }
    }

    /// The determined arguments of `(p t1 ..)` for `t1 = (dyn q dargs)`:
    /// `dargs` when `p` is `q`, else those of the supertrait `p` of `q`
    /// (§4.1 rule 3); `None` when `p` is neither.
    fn dyn_args(&self, q: ProtoId, dargs: &[Ty], p: ProtoId, t1: &Ty) -> Option<Vec<Ty>> {
        if q == p {
            return Some(dargs.to_vec());
        }
        let mut own = vec![t1.clone()];
        own.extend(dargs.iter().cloned());
        crate::types::lower::super_closure(self.g, &Pred::Proto(q, own))
            .into_iter()
            .find_map(|s| match s {
                Pred::Proto(r, args) if r == p => Some(args[1..].to_vec()),
                _ => None,
            })
    }

    /// `no implementation of P for T`.
    pub fn no_instance(&mut self, p: ProtoId, t: &Ty, pos: &crate::syntax::Pos) -> TypeError {
        let msg = format!(
            "no implementation of {} for {}",
            self.g.proto(p).name,
            self.show(t)
        );
        TypeError::new(ErrorKind::NoInstance, pos, msg)
    }

    fn apply_instance(
        &mut self,
        d: &Deferred,
        p: ProtoId,
        c: Con,
        t1: &Ty,
        args: &[Ty],
    ) -> TResult<Step> {
        let Some(&index) = self.g.instance_index.get(&(p, c)) else {
            return Err(self.no_instance(p, t1, &d.pos));
        };
        let g = self.g;
        let inst = &g.instances[index];
        if !self.covers(&inst.head, t1) {
            return Err(self.no_instance(p, t1, &d.pos));
        }
        let tys: Vec<Ty> = inst.var_names.iter().map(|_| self.st.fresh()).collect();
        let head = inst.head.subst_gen(&tys, &[]);
        let dets: Vec<Ty> = inst.dets.iter().map(|t| t.subst_gen(&tys, &[])).collect();
        let context: Vec<Pred> = inst
            .context
            .iter()
            .map(|q| q.map_tys(&mut |t| t.subst_gen(&tys, &[])))
            .collect();
        self.unify(&head, t1, &d.pos)?;
        for (a, b) in dets.iter().zip(&args[1..]) {
            self.unify(a, b, &d.pos)?;
        }
        self.resolved(d, Resolution::Instance { index, args: tys });
        let new = context
            .into_iter()
            .map(|q| self.pred_deferred(q, d))
            .collect();
        Ok(Step::Done(new))
    }

    /// Whether an instance head covers `t`'s colour arguments (§1.3): a
    /// colour the head gives is covered only by itself, where `t` has a
    /// constant colour (`:send`, `:local`, a rigid one) there; a colour
    /// variable of the unit is left to unification.
    fn covers(&mut self, head: &Ty, t: &Ty) -> bool {
        let Ty::Con(_, args) = t else { return true };
        let fixed = crate::types::lower::fixed_colours(self.g, head);
        fixed.iter().zip(args).all(|(k, a)| {
            let k2 = match self.st.resolve(a) {
                Ty::Fn(c @ (Colour::Send | Colour::Local | Colour::Rigid(_)), _, _) => Some(c),
                _ => None,
            };
            matches!((k, k2), (None, _) | (_, None)) || k == &k2
        })
    }

    /// A predicate as a deferred constraint arising from `d`.
    fn pred_deferred(&self, q: Pred, d: &Deferred) -> Deferred {
        let kind = match q {
            Pred::Proto(p, args) => DKind::Proto(p, args, None),
            Pred::Send(t) => DKind::Send(t, Vec::new()),
            Pred::Object(t) => DKind::Object(t, "an instance context".to_string()),
            Pred::Weakable(t) => DKind::Weakable(t),
        };
        Deferred {
            kind,
            pos: d.pos.clone(),
            site: None,
            fun: d.fun.clone(),
        }
    }

    fn resolved(&mut self, d: &Deferred, r: Resolution) {
        if let Some(site) = d.site {
            self.t.resolutions.insert(site, r);
            self.u.sites.push(site);
        }
    }

    /// `(P a ..)` for a rigid `a`: entailed by a given in an `impl`
    /// body (§2.7); a bound of the scheme in a `defun`.
    fn rigid_proto(&mut self, d: &Deferred, p: ProtoId, r: u32, args: &[Ty]) -> TResult<Step> {
        let Some(givens) = self.u.givens.clone() else {
            return Ok(Step::Stuck);
        };
        for gv in &givens {
            if let Pred::Proto(q, gargs) = gv {
                if *q == p && gargs[0] == Ty::Rigid(r) {
                    for (a, b) in gargs[1..].iter().zip(&args[1..]) {
                        self.unify(a, b, &d.pos)?;
                    }
                    self.resolved(d, Resolution::Bound(gv.clone()));
                    return Ok(Step::Done(Vec::new()));
                }
            }
        }
        let name = self.g.proto(p).name.clone();
        let a = self.show(&Ty::Rigid(r));
        let msg = format!(
            "no implementation of {name} for {a}; add ({name} {a}) to the :where of the impl"
        );
        Err(TypeError::new(ErrorKind::ImplContext, &d.pos, msg))
    }

    fn solve_send(&mut self, d: &Deferred, t: &Ty, path: &[String]) -> TResult<Step> {
        let needs = match send_eval(self.g, self.st, t, path) {
            Ok(n) => n,
            Err(w) => return Err(send_error(self.g, &w, &d.pos, &self.u.rigid_names)),
        };
        let mut open = false;
        for (v, _) in &needs.vars {
            match v {
                Ty::Rigid(r) if self.u.givens.is_some() => self.given_send(*r, d)?,
                _ => open = true,
            }
        }
        if open {
            return Ok(Step::Stuck);
        }
        for (k, p) in needs.colours {
            let label = (!p.is_empty()).then(|| p.join(", "));
            self.u.colours.push(ColourCon {
                from: k,
                to: Colour::Send,
                label,
                origin: None,
                pos: d.pos.clone(),
            });
        }
        Ok(Step::Done(Vec::new()))
    }

    fn given_send(&mut self, r: u32, d: &Deferred) -> TResult<()> {
        let given = self
            .u
            .givens
            .as_ref()
            .is_some_and(|gs| gs.contains(&Pred::Send(Ty::Rigid(r))));
        if given {
            return Ok(());
        }
        let a = self.show(&Ty::Rigid(r));
        let msg =
            format!("no implementation of Send for {a}; add (Send {a}) to the :where of the impl");
        Err(TypeError::new(ErrorKind::ImplContext, &d.pos, msg))
    }

    /// `(Object T)`: `T` is not a scalar (§2.11, §2.15). `who` names
    /// what requires it, for the message.
    fn solve_object(&mut self, d: &Deferred, t: &Ty, who: &str) -> TResult<Step> {
        let Some(scalar) = self.is_scalar(t) else {
            return Ok(Step::Stuck);
        };
        if scalar {
            let msg = format!("{who} requires an object type, not {}", self.show(t));
            return Err(TypeError::new(ErrorKind::NotObject, &d.pos, msg));
        }
        Ok(Step::Done(Vec::new()))
    }

    /// `(Weakable T)`: `T` is an object type and not an `Option`
    /// (§2.11). A scalar keeps the catalogue's `weak requires an object
    /// type`.
    fn solve_weakable(&mut self, d: &Deferred, t: &Ty) -> TResult<Step> {
        let Some(scalar) = self.is_scalar(t) else {
            return Ok(Step::Stuck);
        };
        if scalar {
            let msg = "weak requires an object type";
            return Err(TypeError::new(ErrorKind::WeakScalar, &d.pos, msg));
        }
        if let Ty::Con(Con::Nominal(id), _) = self.st.resolve(t) {
            if id == self.g.option {
                let msg = format!(
                    "weak of an Option is not allowed: {} has no object of its own to observe; \
                     take the weak reference of the object inside it",
                    self.show(t)
                );
                return Err(TypeError::new(ErrorKind::WeakOption, &d.pos, msg));
            }
        }
        Ok(Step::Done(Vec::new()))
    }

    /// Whether `t` is a scalar, or `None` while that is undecided. A
    /// rigid variable is an object iff a given says so: `(Object a)`,
    /// or `(Weakable a)`, which implies it.
    fn is_scalar(&mut self, t: &Ty) -> Option<bool> {
        Some(match self.st.resolve(t) {
            Ty::Var(_) | Ty::Gen(_) => return None,
            Ty::Rigid(_) if self.u.givens.is_none() => return None,
            Ty::Rigid(r) => !self.u.givens.as_ref().is_some_and(|gs| {
                gs.contains(&Pred::Object(Ty::Rigid(r)))
                    || gs.contains(&Pred::Weakable(Ty::Rigid(r)))
            }),
            Ty::Con(Con::Scalar(_), _) => true,
            Ty::Con(Con::Nominal(id), _) => self.g.ty(id).is_fieldless_enum(),
            _ => false,
        })
    }

    fn solve_field(&mut self, d: &Deferred, t: &Ty, f: &str, r: &Ty) -> TResult<Step> {
        let rt = self.st.resolve(t);
        if let Ty::Var(_) = rt {
            return Ok(Step::Stuck);
        }
        if let Ty::Con(Con::Nominal(id), args) = &rt {
            if let Shape::Struct(fields) = &self.g.ty(*id).shape {
                if let Some(fd) = fields.iter().find(|fd| fd.name == f) {
                    let args: Vec<Ty> = args.iter().map(|a| self.st.zonk(a)).collect();
                    let fty = fd
                        .ty
                        .subst_gen(&args, &crate::types::ty::colour_args(&args));
                    self.unify(r, &fty, &d.pos)?;
                    return Ok(Step::Done(Vec::new()));
                }
            }
        }
        let msg = format!("{} has no field {f}", self.show(&rt));
        Err(TypeError::new(ErrorKind::NoField, &d.pos, msg))
    }

    fn solve_deref(&mut self, d: &Deferred, t: &Ty, r: &Ty) -> TResult<Step> {
        let rt = self.st.resolve(t);
        let content = match &rt {
            Ty::Var(_) => return Ok(Step::Stuck),
            Ty::Con(Con::Cell | Con::Atom | Con::Task, a) => a[0].clone(),
            Ty::Con(Con::Weak, a) => Ty::nominal(self.g.option, vec![a[0].clone()]),
            _ => {
                let Some(p) = self.g.deref_proto else {
                    return Err(TypeError::other(&d.pos, "no Deref protocol"));
                };
                return Err(self.no_instance(p, &rt, &d.pos));
            }
        };
        self.unify(r, &content, &d.pos)?;
        Ok(Step::Done(Vec::new()))
    }

    /// Improvement: stuck `(P s d̄)` and `(P s d̄')` (or a given) with the
    /// same `s` must have `d̄ = d̄'`. Returns whether anything changed.
    /// Constraints are grouped by protocol and zonked dispatch type, so a
    /// pass is linear in the worklist.
    pub fn improve(&mut self) -> TResult<bool> {
        let mut preds: Vec<(ProtoId, Vec<Ty>, crate::syntax::Pos)> = Vec::new();
        for d in &self.u.deferred {
            if let DKind::Proto(p, args, _) = &d.kind {
                if args.len() > 1 {
                    preds.push((*p, args.clone(), d.pos.clone()));
                }
            }
        }
        let Some(pos) = preds.first().map(|x| x.2.clone()) else {
            return Ok(false);
        };
        for q in self.u.givens.clone().unwrap_or_default() {
            if let Pred::Proto(p, args) = q {
                preds.push((p, args, pos.clone()));
            }
        }
        let mut first: HashMap<(ProtoId, Ty), usize> = HashMap::new();
        let mut changed = false;
        for i in 0..preds.len() {
            let key = (preds[i].0, self.zonk(&preds[i].1[0]));
            let Some(&j) = first.get(&key) else {
                first.insert(key, i);
                continue;
            };
            let (a, b, pos) = (preds[j].1.clone(), preds[i].1.clone(), preds[i].2.clone());
            for (x, y) in a[1..].iter().zip(&b[1..]) {
                if self.zonk(x) != self.zonk(y) {
                    self.unify(x, y, &pos)?;
                    changed = true;
                }
            }
        }
        Ok(changed)
    }
}
