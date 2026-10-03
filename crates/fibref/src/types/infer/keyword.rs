//! A keyword as a function (stdlib §7 L14): typed by a deferred
//! constraint, settled when the argument's type is known; `elab` writes
//! the field read or the `map-get` it chose.

use crate::syntax::Pos;

use crate::types::ast::{Arg, Expr, ExprId, ExprKind, GlobalRef, Lit};
use crate::types::decls::{ModuleId, Shape};
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::scheme::Scheme;
use crate::types::ty::{Con, Scalar, Ty};

use super::cx::{Cx, DKind, Deferred, KwSite};
use super::solve::Step;

impl Cx<'_> {
    /// Whether a parameter type is a function type (a keyword passed
    /// there is the function of L14).
    pub(super) fn expects_fn(&mut self, t: &Ty) -> bool {
        matches!(self.st.resolve(t), Ty::Fn(..))
    }

    /// A keyword where a function is expected: `(fn (S) R)`, `S` and `R`
    /// settled by the argument's type (L14).
    pub(super) fn keyword_fn(&mut self, x: &Expr, k: &str) -> Ty {
        let (s, r) = (self.fresh(), self.fresh());
        let colour = self.st.fresh_colour();
        let t = Ty::Fn(colour, vec![s.clone()], Box::new(r.clone()));
        self.record(x.id, &t);
        self.u.fn_lits.push((x.id, colour));
        self.defer(
            DKind::Keyword(x.id, s, k.to_string(), r, None),
            &x.pos,
            None,
        );
        t
    }

    /// `(:k x)` and `(:k x d)` (L14): the field read or the lookup,
    /// settled when the type of `x` is.
    pub(super) fn keyword_call(&mut self, head: &Expr, args: &[Arg]) -> TResult<Option<Ty>> {
        let ExprKind::Lit(Lit::Keyword(k)) = &head.kind else {
            return Ok(None);
        };
        let (Some(Arg::Expr(x)), None | Some(Arg::Expr(_)), true) =
            (args.first(), args.get(1), args.len() <= 2)
        else {
            return Ok(None);
        };
        let s = self.infer(x)?;
        let dflt = match args.get(1) {
            Some(Arg::Expr(d)) => Some(self.infer(d)?),
            _ => None,
        };
        let r = self.fresh();
        let kind = DKind::Keyword(head.id, s, k.clone(), r.clone(), dflt);
        self.defer(kind, &head.pos, None);
        Ok(Some(r))
    }

    /// The scheme of the prelude's `map-get`, which `(:k m)` is.
    pub(super) fn map_get_scheme(&mut self, pos: &Pos) -> TResult<Scheme> {
        let found = match self.g.value(ModuleId::PRELUDE, "map-get") {
            Some(GlobalRef::Fun(f)) => self.env.funs.get(f.0 as usize).cloned().flatten(),
            _ => None,
        };
        found.ok_or_else(|| TypeError::other(pos, "internal: map-get has no scheme"))
    }

    /// A keyword as a function (L14): the field read when `S` is a struct
    /// with the field, `map-get` when it is a `(Map keyword v)`.
    pub(super) fn solve_keyword(
        &mut self,
        d: &Deferred,
        site: ExprId,
        s: &Ty,
        k: &str,
        r: &Ty,
        dflt: &Option<Ty>,
    ) -> TResult<Step> {
        let rt = self.st.resolve(s);
        if let Ty::Var(_) = rt {
            return Ok(Step::Stuck);
        }
        if let Ty::Con(Con::Nominal(id), args) = &rt {
            let args: Vec<Ty> = args.iter().map(|a| self.st.zonk(a)).collect();
            if let Shape::Struct(fields) = &self.g.ty(*id).shape {
                let Some(fd) = fields.iter().find(|fd| fd.name == k) else {
                    let msg = format!("{} has no field :{k}", self.show(&rt));
                    return Err(TypeError::new(ErrorKind::NoField, &d.pos, msg));
                };
                if dflt.is_some() {
                    let msg = format!(
                        "(:{k} x d): {} is a struct, a default needs a map",
                        self.show(&rt)
                    );
                    return Err(TypeError::other(&d.pos, msg));
                }
                let fty = fd
                    .ty
                    .subst_gen(&args, &crate::types::ty::colour_args(&args));
                self.unify(r, &fty, &d.pos)?;
                self.t.kw_sites.insert(site, KwSite::Field);
                return Ok(Step::Done(Vec::new()));
            }
            if self.g.type_name(ModuleId::PRELUDE, "Map") == Some(*id) {
                self.solve_keyword_map(d, site, s, k, r, dflt)?;
                return Ok(Step::Done(Vec::new()));
            }
        }
        let msg = format!(
            "cannot apply :{k} to {}: it is neither a struct nor a (Map keyword v)",
            self.show(&rt)
        );
        Err(TypeError::other(&d.pos, msg))
    }

    fn solve_keyword_map(
        &mut self,
        d: &Deferred,
        site: ExprId,
        s: &Ty,
        k: &str,
        r: &Ty,
        dflt: &Option<Ty>,
    ) -> TResult<()> {
        let scheme = self.map_get_scheme(&d.pos)?;
        let Ty::Fn(_, ps, ret) = self.instantiate(&scheme, site, "map-get", &d.pos, None) else {
            return Err(TypeError::other(
                &d.pos,
                "internal: map-get is not a function",
            ));
        };
        self.unify(&ps[0], s, &d.pos)?;
        self.unify(&ps[1], &Ty::scalar(Scalar::Keyword), &d.pos)
            .map_err(|_| {
                let msg = format!("cannot apply :{k}: the map's keys are not keywords");
                TypeError::other(&d.pos, msg)
            })?;
        match dflt {
            None => self.unify(r, &ret, &d.pos)?,
            Some(dt) => {
                let v = self.kw_payload(&ret);
                self.unify(dt, &v, &d.pos)?;
                self.unify(r, &v, &d.pos)?;
            }
        }
        self.t.kw_sites.insert(site, KwSite::Map);
        Ok(())
    }

    fn kw_payload(&mut self, t: &Ty) -> Ty {
        match self.st.resolve(t) {
            Ty::Con(_, args) if args.len() == 1 => args[0].clone(),
            other => other,
        }
    }
}
