//! Patterns (types §2.6, §8.3): tests that branch to a failure label,
//! binding variables on the way; `Option` by its representation, an
//! enum by its tag through the variant struct.

use fibref::types::ast::{Lit, PatKind, Pattern, Rest};
use fibref::types::decls::Shape;
use fibref::types::ty::{Con, Ty};

use super::{Cx, Local, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};
use crate::layout::{option_payload, option_rep, variants, OptRep};
use crate::objects::{ENUM_TAG, STRUCT_FIELD0, VARIANT_FIELD0};

impl<'a> Cx<'_, 'a> {
    /// Binds an irrefutable pattern (a `let`'s); a failure traps.
    pub fn bind_irrefutable(&mut self, pat: &Pattern, v: &V, t: &Ty) -> R<()> {
        let fail = self.b.label("nomatch");
        let ok = self.b.label("bound");
        self.match_pattern(pat, v, t, &fail)?;
        self.build_rests()?;
        self.b.term(&format!("(br {ok})"));
        self.b.open(&fail);
        self.trap_c("no match clause matched");
        self.b.open(&ok);
        Ok(())
    }

    /// Tests `v : t` against `pat`, branching to `fail` on a mismatch
    /// and binding the pattern's variables; the current block on
    /// return is the success path.
    pub fn match_pattern(&mut self, pat: &Pattern, v: &V, t: &Ty, fail: &str) -> R<()> {
        match &pat.kind {
            PatKind::Wild => Ok(()),
            PatKind::Bind(b) => {
                self.locals.insert(*b, Local::Val(v.clone()));
                Ok(())
            }
            PatKind::As(p, b) => {
                self.locals.insert(*b, Local::Val(v.clone()));
                self.match_pattern(p, v, t, fail)
            }
            PatKind::Lit(l) => {
                let c = self.lit_test(l, v)?;
                self.branch(&c, fail);
                Ok(())
            }
            PatKind::Ctor(id, variant, subs) => self.ctor_pattern(*id, *variant, subs, v, t, fail),
            PatKind::Vec(subs, rest) => self.vec_pattern(subs, *rest, v, t, fail),
        }
    }

    /// A vector pattern (§8.3): the length test, then each element
    /// read as `vec-nth` reads it with no count operation; a rest is
    /// noted for `build_rests`.
    fn vec_pattern(&mut self, subs: &[Pattern], rest: Rest, v: &V, t: &Ty, fail: &str) -> R<()> {
        let elem = match t {
            Ty::Con(Con::Nominal(_), args) => args
                .first()
                .cloned()
                .ok_or_else(|| Unsupported("a Vec without its element type".into()))?,
            _ => return Err(Unsupported("a vector pattern on a non-Vec".into())),
        };
        let n = self
            .b
            .val(&format!("(call @fib.vec-len {})", v.text()), LirTy::I64);
        let k = subs.len();
        let pred = if rest == Rest::Exact { "eq" } else { "sge" };
        let c = self
            .b
            .val(&format!("(icmp {pred} {} (i64 {k}))", n.text()), LirTy::I1);
        self.branch(&c, fail);
        let el = self.p.lir(&elem)?;
        let esize = el.map_or(0, |l| crate::layout::size_align(l).0);
        for (i, p) in subs.iter().enumerate() {
            if matches!(p.kind, PatKind::Wild) {
                continue;
            }
            let ev = match el {
                Some(l) => {
                    let ptr = self.b.val(
                        &format!(
                            "(call @fib.vec-elem-ptr {} (i64 {i}) (i64 {esize}))",
                            v.text()
                        ),
                        LirTy::Ptr,
                    );
                    self.load(l, ptr.text())
                }
                None => V::Unit,
            };
            self.match_pattern(p, &ev, &elem, fail)?;
        }
        if let Rest::Bind(b) = rest {
            self.rests.push((b, v.clone(), k, t.clone()));
        }
        Ok(())
    }

    /// Binds every rest variable noted by the pattern just matched to
    /// a new vector (`fib.vec-drop`), left to right (§6.3).
    pub fn build_rests(&mut self) -> R<()> {
        let rests = std::mem::take(&mut self.rests);
        for (b, v, k, t) in rests {
            let r = self.vec_drop(&v, k, &t)?;
            self.locals.insert(b, Local::Val(r));
        }
        Ok(())
    }

    /// `(fib.vec-drop v k)`: the vector of `v`'s elements from `k` on.
    fn vec_drop(&mut self, v: &V, k: usize, t: &Ty) -> R<V> {
        let g = self.p.g();
        let (vec_id, elem) = match t {
            Ty::Con(Con::Nominal(id), args) if Some(*id) == g.vec => (*id, args[0].clone()),
            _ => return Err(Unsupported("a rest of a non-Vec".into())),
        };
        let node_id = g
            .type_name(fibref::types::decls::ModuleId::Prelude, "VNode")
            .ok_or_else(|| Unsupported("the prelude defines no VNode".into()))?;
        let _ = vec_id;
        let node_ty = Ty::nominal(node_id, vec![elem.clone()]);
        let (tvec, _) = self.p.object(t)?;
        let (tnode, _) = self.p.object(&node_ty)?;
        let (tarr, _) = self.p.object(&Ty::Con(Con::Array, vec![elem.clone()]))?;
        let (tnarr, _) = self.p.object(&Ty::Con(Con::Array, vec![node_ty]))?;
        let el = self
            .p
            .lir(&elem)?
            .ok_or_else(|| Unsupported("a Vec of unit".into()))?;
        let esize = crate::layout::size_align(el).0;
        let counted = match el {
            LirTy::Ptr => 1,
            LirTy::Dyn => 2,
            _ => 0,
        };
        Ok(self.b.val(
            &format!(
                "(call @fib.vec-drop {} (i64 {k}) (i64 {esize}) (i8 {counted}) (i32 {tvec}) (i32 {tnode}) (i32 {tarr}) (i32 {tnarr}))",
                v.text()
            ),
            LirTy::Ptr,
        ))
    }

    /// Continues in a fresh block when `c` holds, else goes to `fail`.
    fn branch(&mut self, c: &V, fail: &str) {
        let ok = self.b.label("ok");
        self.b.term(&format!("(br {} {ok} {fail})", c.text()));
        self.b.open(&ok);
    }

    fn lit_test(&mut self, l: &Lit, v: &V) -> R<V> {
        let x = v.text().to_string();
        Ok(match (l, v.ty()) {
            (Lit::Int(n, _), Some(t)) => self
                .b
                .val(&format!("(icmp eq {x} ({} {n}))", t.text()), LirTy::I1),
            (Lit::Float(f, _), Some(t)) => self.b.val(
                &format!("(fcmp oeq {x} ({} {}))", t.text(), super::float_text(*f)),
                LirTy::I1,
            ),
            (Lit::Char(c), _) => self
                .b
                .val(&format!("(icmp eq {x} (i32 {}))", u32::from(*c)), LirTy::I1),
            (Lit::Bool(b), _) => self
                .b
                .val(&format!("(icmp eq {x} (i1 {}))", i32::from(*b)), LirTy::I1),
            (Lit::Keyword(k), _) => {
                let id = self.p.statics.keyword(k);
                self.b.val(&format!("(icmp eq {x} (i64 {id}))"), LirTy::I1)
            }
            (Lit::Str(s), _) => {
                let lit = self.p.statics.string(s, 0);
                self.b
                    .val(&format!("(call @fib.str-eq {x} {lit})"), LirTy::I1)
            }
            (Lit::Unit, _) => V::int(LirTy::I1, 1),
            (l, None) => return Err(Unsupported(format!("literal pattern {l:?} on unit"))),
        })
    }

    fn ctor_pattern(
        &mut self,
        id: fibref::types::ty::TypeId,
        variant: Option<usize>,
        subs: &[Pattern],
        v: &V,
        t: &Ty,
        fail: &str,
    ) -> R<()> {
        let g = self.p.g();
        if id == g.option {
            return self.option_pattern(variant, subs, v, t, fail);
        }
        let def = g.ty(id);
        if def.is_fieldless_enum() {
            let want = variant.unwrap_or(0);
            let c = self
                .b
                .val(&format!("(icmp eq {} (i32 {want}))", v.text()), LirTy::I1);
            self.branch(&c, fail);
            return Ok(());
        }
        let args = match t {
            Ty::Con(Con::Nominal(_), args) => args.clone(),
            _ => {
                return Err(Unsupported(
                    "a constructor pattern on a non-nominal type".into(),
                ))
            }
        };
        let (tid, sname) = self.p.object(t)?;
        let _ = tid;
        let vs = variants(self.p.g(), id, &args)?;
        let (sname, base, fields) = match (&def.shape, variant) {
            (Shape::Struct(_), _) => (sname, STRUCT_FIELD0, vs[0].1.clone()),
            (Shape::Enum(_), Some(i)) => {
                let tagp = self.gep(&sname, v.text(), ENUM_TAG);
                let tag = self.load(LirTy::I32, &tagp);
                let c = self
                    .b
                    .val(&format!("(icmp eq {} (i32 {i}))", tag.text()), LirTy::I1);
                self.branch(&c, fail);
                (format!("{sname}.v{i}"), VARIANT_FIELD0, vs[i].1.clone())
            }
            (Shape::Enum(_), None) => {
                return Err(Unsupported("an enum pattern without a variant".into()))
            }
        };
        self.sub_patterns(&sname, base, &fields, subs, v, fail)
    }

    /// The sub-patterns of a struct or variant, each against the
    /// field loaded from the object.
    fn sub_patterns(
        &mut self,
        sname: &str,
        base: usize,
        fields: &[Ty],
        subs: &[Pattern],
        v: &V,
        fail: &str,
    ) -> R<()> {
        let mut slot = base;
        for (p, ft) in subs.iter().zip(fields) {
            let lt = self.p.lir(ft)?;
            let fv = match lt {
                Some(l) => {
                    let ptr = self.gep(sname, v.text(), slot);
                    slot += 1;
                    self.load(l, &ptr)
                }
                None => V::Unit,
            };
            if !matches!(p.kind, PatKind::Wild) {
                self.match_pattern(p, &fv, ft, fail)?;
            }
        }
        Ok(())
    }

    fn option_pattern(
        &mut self,
        variant: Option<usize>,
        subs: &[Pattern],
        v: &V,
        t: &Ty,
        fail: &str,
    ) -> R<()> {
        let g = self.p.g();
        let payload = option_payload(g, t)
            .cloned()
            .ok_or_else(|| Unsupported("an Option pattern on a non-Option".into()))?;
        let is_some = variant == Some(1);
        match option_rep(g, &payload)? {
            OptRep::Null => {
                let pred = if is_some { "ne" } else { "eq" };
                let c = self
                    .b
                    .val(&format!("(icmp {pred} {} (ptr null))", v.text()), LirTy::I1);
                self.branch(&c, fail);
                if let (true, [p]) = (is_some, subs) {
                    self.match_pattern(p, v, &payload, fail)?;
                }
                Ok(())
            }
            OptRep::Boxed => {
                // A boxed `nil` may be null (objects.rs), so the tag is
                // read only from a non-null value.
                let (_, sname) = self.p.object(t)?;
                let null = self
                    .b
                    .val(&format!("(icmp eq {} (ptr null))", v.text()), LirTy::I1);
                let (lnull, ltag, lok) = (
                    self.b.label("onull"),
                    self.b.label("otag"),
                    self.b.label("ok"),
                );
                self.b.term(&format!("(br {} {lnull} {ltag})", null.text()));
                self.b.open(&lnull);
                self.b
                    .term(&format!("(br {})", if is_some { fail } else { &lok }));
                self.b.open(&ltag);
                let tagp = self.gep(&sname, v.text(), ENUM_TAG);
                let tag = self.load(LirTy::I32, &tagp);
                let c = self.b.val(
                    &format!("(icmp eq {} (i32 {}))", tag.text(), i32::from(is_some)),
                    LirTy::I1,
                );
                self.b.term(&format!("(br {} {lok} {fail})", c.text()));
                self.b.open(&lok);
                if is_some {
                    self.sub_patterns(
                        &format!("{sname}.v1"),
                        VARIANT_FIELD0,
                        &[payload],
                        subs,
                        v,
                        fail,
                    )?;
                }
                Ok(())
            }
        }
    }
}
