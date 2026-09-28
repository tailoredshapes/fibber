//! The units that are not generalised (spec/types.md §3.5 steps 5–6):
//! a `def` (closed and monomorphic, §2.16) and an `impl` method body
//! (checked against its signature under the declared context, §2.7).

use crate::types::ast::DefId;
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::ty::{Colour, Ty};

use super::cx::Cx;
use super::general::{keys, GenMap, Key};

impl Cx<'_> {
    /// Types a `def` (§2.16): a closed monomorphic type.
    pub fn infer_def(&mut self, d: DefId) -> TResult<Ty> {
        let def = self.g.def(d);
        self.u.fun = format!("def {}", def.name);
        let t = self.infer(&def.init)?;
        let ty = match &def.ann {
            Some(a) => {
                let mut send = || Colour::Send;
                let mut env = crate::types::annot::RigidEnv {
                    map: &mut self.u.rigid_map,
                    names: &mut self.u.rigid_names,
                    colour: &mut send,
                };
                let at = crate::types::annot::ann_to_ty(a, &mut env, &def.pos)?;
                self.flow(&t, &at, &def.init.pos)?;
                at
            }
            None => t,
        };
        self.solve_all()?;
        if !keys(self.st, &ty).is_empty() {
            let msg = format!("def {} has an unresolved type; annotate it", def.name);
            return Err(TypeError::new(ErrorKind::DefUnresolved, &def.pos, msg));
        }
        let closed = self.close(&[])?;
        self.check_matches(&def.init)?;
        let map = GenMap::new(self, &[], closed.colours.clone(), &[]);
        let out = map.apply(self.st, &ty);
        self.finalize(&map, &closed);
        Ok(out)
    }

    /// Checks one method body of instance `index` (§2.7, §3.5 step 6).
    pub fn infer_impl_method(&mut self, index: usize, method: usize) -> TResult<()> {
        let g = self.g;
        let inst = &g.instances[index];
        let m = &inst.methods[method];
        let md = &g.proto(inst.proto).methods[m.index];
        self.u.rigid_names = inst.var_names.clone();
        let rig: Vec<Ty> = (0..inst.var_names.len() as u32).map(Ty::Rigid).collect();
        let head = inst.head.subst_gen(&rig, &[]);
        let mut tys = vec![head];
        tys.extend(inst.dets.iter().map(|t| t.subst_gen(&rig, &[])));
        for name in md.scheme.var_names.iter().skip(tys.len()) {
            tys.push(Ty::Rigid(self.u.rigid_names.len() as u32));
            self.u.rigid_names.push(name.clone());
        }
        let context: Vec<_> = inst
            .context
            .iter()
            .map(|p| p.map_tys(&mut |t| t.subst_gen(&rig, &[])))
            .collect();
        // The declared context and what it entails through supertraits
        // (§4.1 rule 2).
        self.u.givens = Some(crate::types::lower::entail_closure(g, &context));
        self.u.fun = format!("{} for {}", md.name, self.show(&tys[0]));
        let colours: Vec<Colour> = (0..md.scheme.n_colours)
            .map(|_| self.st.fresh_colour())
            .collect();
        let Ty::Fn(_, params, ret) = md.scheme.instantiate_with(&tys, &colours).ty else {
            return Err(TypeError::other(
                &m.pos,
                "internal: method type is not a function",
            ));
        };
        for (b, t) in m.params.iter().zip(&params) {
            self.bind(*b, t);
        }
        if let Some(a) = &m.ret {
            let at = self.ann(a, &m.pos)?;
            self.unify(&at, &ret, &m.pos)?;
        }
        let body = self.infer(&m.body)?;
        self.flow(&body, &ret, &m.body.pos)?;
        let closed = self.close(&[])?;
        self.check_matches(&m.body)?;
        let order: Vec<Key> = (0..self.u.rigid_names.len() as u32)
            .map(Key::Rigid)
            .collect();
        let map = GenMap::new(self, &order, closed.colours.clone(), &[]);
        self.finalize(&map, &closed);
        Ok(())
    }
}
