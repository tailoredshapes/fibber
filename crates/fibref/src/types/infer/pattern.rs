//! Checking a pattern against the scrutinee's type (spec/types.md §2.6):
//! `Γ ⊢ pat : S ⇝ Γ'`.

use crate::syntax::Pos;
use crate::types::ast::{BindingId, PatKind, Pattern, Rest};
use crate::types::decls::Shape;
use crate::types::error::{TResult, TypeError};
use crate::types::ty::Ty;

use super::cx::{Cx, DKind};
use super::expr::lit_type;

impl Cx<'_> {
    /// The type the annotation of the `let` or `loop` binding `b` gives
    /// it (syntax §1.5), if it has one.
    pub fn binding_ann(&mut self, b: BindingId, pos: &Pos) -> TResult<Option<Ty>> {
        match self.g.binding(b).ann.clone() {
            Some(a) => self.ann(&a, pos).map(Some),
            None => Ok(None),
        }
    }

    /// The type a `let` pattern binds at: an annotated variable's
    /// annotation, which the initialiser's type `t` flows into as an
    /// argument's flows into an annotated parameter; else `t`.
    pub fn let_binding_type(&mut self, p: &Pattern, t: Ty, pos: &Pos) -> TResult<Ty> {
        let PatKind::Bind(b) = p.kind else {
            return Ok(t);
        };
        match self.binding_ann(b, pos)? {
            Some(a) => {
                self.flow(&t, &a, pos)?;
                Ok(a)
            }
            None => Ok(t),
        }
    }

    /// Checks `p` against `s`, binding its variables.
    pub fn check_pattern(&mut self, p: &Pattern, s: &Ty) -> TResult<()> {
        match &p.kind {
            PatKind::Wild => Ok(()),
            PatKind::Bind(b) => {
                self.bind(*b, s);
                Ok(())
            }
            PatKind::Lit(l) => {
                let t = lit_type(l);
                self.unify(s, &t, &p.pos)?;
                let Some(eq) = self
                    .g
                    .proto_name(crate::types::decls::ModuleId::BUILTIN, "Eq")
                else {
                    return Err(TypeError::other(&p.pos, "no Eq protocol"));
                };
                self.defer(DKind::Proto(eq, vec![t], None), &p.pos, None);
                Ok(())
            }
            PatKind::As(inner, b) => {
                self.bind(*b, s);
                self.check_pattern(inner, s)
            }
            PatKind::Vec(subs, rest) => self.check_vec_pattern(p, subs, *rest, s),
            PatKind::Ctor(t, v, subs) => {
                let def = self.g.ty(*t);
                // A colour parameter's argument is a colour (§1.3).
                let args: Vec<Ty> = (0..def.params.len())
                    .map(|i| match def.is_colour(i) {
                        true => Ty::colour_arg(self.st.fresh_colour()),
                        false => self.st.fresh(),
                    })
                    .collect();
                let fields = match (&def.shape, v) {
                    (Shape::Struct(fs), None) => fs.clone(),
                    (Shape::Enum(vs), Some(i)) => vs[*i].fields.clone(),
                    _ => Vec::new(),
                };
                self.unify(s, &Ty::nominal(*t, args.clone()), &p.pos)?;
                for (sub, f) in subs.iter().zip(&fields) {
                    let ft = f.ty.subst_gen(&args, &crate::types::ty::colour_args(&args));
                    self.check_pattern(sub, &ft)?;
                }
                Ok(())
            }
        }
    }

    /// `[p.. & r]` against `s`: `s ~ (Vec a)` with the prelude's `Vec`,
    /// each `p : a`, `r : (Vec a)` (§2.6).
    fn check_vec_pattern(
        &mut self,
        p: &Pattern,
        subs: &[Pattern],
        rest: Rest,
        s: &Ty,
    ) -> TResult<()> {
        let Some(vec) = self.g.vec else {
            return Err(TypeError::other(
                &p.pos,
                "vector patterns need the prelude's Vec",
            ));
        };
        let a = self.st.fresh();
        let vt = Ty::nominal(vec, vec![a.clone()]);
        self.unify(s, &vt, &p.pos)?;
        for sub in subs {
            self.check_pattern(sub, &a)?;
        }
        if let Rest::Bind(b) = rest {
            self.bind(b, &vt);
        }
        Ok(())
    }
}
