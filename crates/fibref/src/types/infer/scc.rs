//! An SCC of `defun`s (spec/types.md §3.5 step 5, §3.6): each member
//! bound to a fresh monomorphic type (or, when fully annotated and used
//! polymorphically inside the SCC, to its annotation), the bodies
//! inferred, the unit closed, one scheme per member. A `defmacro` is an
//! SCC of its own, typed as a function over `Form` (§2.8).

use std::collections::HashMap;

use crate::types::ast::FunId;
use crate::types::decls::{FunDef, PredAnn};
use crate::types::error::{TResult, TypeError};
use crate::types::scheme::Scheme;
use crate::types::ty::{Colour, Leaf, Pred, Ty};

use super::cx::{Cx, DKind, MonoSig};
use super::general::{keys, Closed, GenMap, Key};

impl Cx<'_> {
    /// The monomorphic signature of an SCC member, binding its
    /// parameters; its annotation scheme when fully annotated (§3.6).
    fn member_sig(&mut self, f: &FunDef) -> TResult<(MonoSig, Option<Scheme>)> {
        let mut params = Vec::new();
        for p in &f.params {
            let t = match &p.ann {
                Some(a) => self.ann(a, &f.pos)?,
                None => self.fresh(),
            };
            let bt = if p.amp {
                Ty::cell(t.clone())
            } else {
                t.clone()
            };
            self.bind(p.binding, &bt);
            params.push(t);
        }
        let ret = match &f.ret {
            Some(a) => self.ann(a, &f.pos)?,
            None => self.fresh(),
        };
        let amps: Vec<bool> = f.params.iter().map(|p| p.amp).collect();
        let names: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
        let full = f.ret.is_some() && f.params.iter().all(|p| p.ann.is_some());
        let sig = MonoSig {
            params,
            amps,
            names,
            ret,
        };
        let poly = if full {
            Some(self.annotation_scheme(f, &sig)?)
        } else {
            None
        };
        Ok((sig, poly))
    }

    /// A fully annotated `defun`'s annotation as a scheme for its
    /// recursive occurrences: its rigid variables quantified, its
    /// `:where` bounds as the context. Its colour variables are not
    /// quantified: the recursive occurrences share them with the body,
    /// so what the body requires of a colour holds at every occurrence.
    fn annotation_scheme(&mut self, f: &FunDef, sig: &MonoSig) -> TResult<Scheme> {
        let ty = Ty::Fn(Colour::Send, sig.params.clone(), Box::new(sig.ret.clone()));
        let bounds = self.bounds(f)?;
        let mut rigid: Vec<u32> = Vec::new();
        let mut note = |t: &Ty| {
            t.visit(&mut |l| {
                if let Leaf::Rigid(r) = l {
                    if !rigid.contains(&r) {
                        rigid.push(r);
                    }
                }
            })
        };
        note(&ty);
        bounds
            .iter()
            .for_each(|b| b.tys().into_iter().for_each(&mut note));
        let map = |t: &Ty| {
            t.map_leaves(&mut |l| match l {
                Ty::Rigid(r) => rigid.iter().position(|x| x == r).map(|i| Ty::Gen(i as u32)),
                _ => None,
            })
        };
        let mut s = Scheme::mono(map(&ty));
        s.n_vars = rigid.len() as u32;
        s.var_names = rigid
            .iter()
            .map(|r| self.u.rigid_names[*r as usize].clone())
            .collect();
        s.preds = bounds.iter().map(|b| b.map_tys(&mut |t| map(t))).collect();
        s.amps = sig.amps.clone();
        s.params = sig.names.clone();
        Ok(s)
    }

    /// The `:where` bounds of `f` as predicates.
    fn bounds(&mut self, f: &FunDef) -> TResult<Vec<Pred>> {
        let mut out = Vec::new();
        for b in &f.bounds {
            out.push(match b {
                PredAnn::Proto(p, anns) => {
                    let tys = anns
                        .iter()
                        .map(|a| self.ann(a, &f.pos))
                        .collect::<TResult<Vec<_>>>()?;
                    Pred::Proto(*p, tys)
                }
                PredAnn::Send(a) => Pred::Send(self.ann(a, &f.pos)?),
            });
        }
        Ok(out)
    }

    /// Step a: every member's signature. Returns each member's rigid
    /// variable map.
    fn scc_signatures(
        &mut self,
        members: &[&FunDef],
        ids: &[FunId],
    ) -> TResult<Vec<HashMap<String, u32>>> {
        let mut maps = Vec::new();
        for (f, id) in members.iter().zip(ids) {
            self.u.rigid_map = HashMap::new();
            let (sig, poly) = self.member_sig(f)?;
            self.u.mono.insert(*id, sig);
            if let Some(p) = poly {
                self.u.poly.insert(*id, p);
            }
            maps.push(std::mem::take(&mut self.u.rigid_map));
        }
        Ok(maps)
    }

    /// Step b: every member's body. Returns each member's declared
    /// bounds.
    fn scc_bodies(
        &mut self,
        members: &[&FunDef],
        ids: &[FunId],
        maps: &[HashMap<String, u32>],
    ) -> TResult<Vec<Vec<Pred>>> {
        let mut declared = Vec::new();
        for ((f, id), map) in members.iter().zip(ids).zip(maps) {
            self.u.rigid_map = map.clone();
            self.u.fun = f.name.clone();
            let bounds = self.bounds(f)?;
            for b in &bounds {
                self.defer(pred_kind(b.clone(), &f.name), &f.pos, None);
            }
            declared.push(bounds);
            let body = self.infer(&f.body)?;
            let ret = self.u.mono[id].ret.clone();
            self.flow(&body, &ret, &f.body.pos)?;
        }
        Ok(declared)
    }

    /// A member whose annotation served as its scheme at a recursive
    /// occurrence (§3.6) must declare every bound its body needs on its
    /// own variables, or those occurrences went unchecked.
    fn check_poly_bounds(
        &mut self,
        members: &[&FunDef],
        ids: &[FunId],
        maps: &[HashMap<String, u32>],
        declared: &[Vec<Pred>],
        closed: &Closed,
    ) -> TResult<()> {
        for (i, id) in ids.iter().enumerate() {
            if !self.u.poly_used.contains(id) {
                continue;
            }
            let own: Vec<Key> = maps[i].values().map(|r| Key::Rigid(*r)).collect();
            let decl: Vec<Pred> = declared[i]
                .iter()
                .map(|p| p.map_tys(&mut |t| self.st.zonk(t)))
                .collect();
            let decl = crate::types::lower::entail_closure(self.g, &decl);
            for (p, _) in &closed.preds {
                let ks: Vec<Key> = p.tys().iter().flat_map(|t| keys(self.st, t)).collect();
                let z = p.map_tys(&mut |t| self.st.zonk(t));
                if !ks.is_empty() && ks.iter().all(|k| own.contains(k)) && !decl.contains(&z) {
                    let text = crate::types::display::Printer::with_names(
                        self.g,
                        &[],
                        &self.u.rigid_names,
                    )
                    .pred(&z);
                    let msg = format!("{} is used polymorphically in its own definition, so its :where must list {text}", members[i].name);
                    return Err(TypeError::other(&members[i].pos, msg));
                }
            }
        }
        Ok(())
    }

    /// Infers an SCC of `defun`s and generalises it (§3.5 a–e).
    pub fn infer_scc(&mut self, members: &[&FunDef], ids: &[FunId]) -> TResult<Vec<Scheme>> {
        let maps = self.scc_signatures(members, ids)?;
        let declared = self.scc_bodies(members, ids, &maps)?;
        let roots: Vec<Ty> = ids
            .iter()
            .map(|id| {
                let m = &self.u.mono[id];
                Ty::Fn(Colour::Send, m.params.clone(), Box::new(m.ret.clone()))
            })
            .collect();
        let closed = self.close(&roots)?;
        self.check_poly_bounds(members, ids, &maps, &declared, &closed)?;
        for f in members {
            self.check_matches(&f.body)?;
        }
        let mut order: Vec<Key> = Vec::new();
        for k in closed.sets.iter().flatten() {
            if !order.contains(k) {
                order.push(*k);
            }
        }
        // Rémy: only variables created at this unit's level (not lowered
        // by unification with an enclosing level's) are generalised.
        let outer = self.st.level().saturating_sub(1);
        order.retain(|k| match k {
            Key::Var(v) => self.st.level_of(*v).is_some_and(|l| l > outer),
            Key::Rigid(_) => true,
        });
        let map = GenMap::new(self, &order, closed.colours.clone(), &roots);
        let mut schemes = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            let m = self.u.mono[id].clone();
            schemes.push(self.scheme(&map, &closed, i, &roots[i], m.amps, m.names));
        }
        self.finalize(&map, &closed);
        Ok(schemes)
    }
}

fn pred_kind(p: Pred, who: &str) -> DKind {
    match p {
        Pred::Proto(q, args) => DKind::Proto(q, args, None),
        Pred::Send(t) => DKind::Send(t, Vec::new()),
        Pred::Object(t) => DKind::Object(t, who.to_string()),
        Pred::Weakable(t) => DKind::Weakable(t),
    }
}
