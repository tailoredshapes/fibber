//! Patterns (syntax §3.6): matching a value and binding its parts. The
//! bindings are plain values; what they count is the plan's business
//! (pattern variables are aliases or parts of the scrutinee, §6.1).

use crate::heap::ObjId;
use crate::types::ast::{Lit, PatKind, Pattern};
use crate::types::ty::TypeId;

use super::error::{RunError, R};
use super::interp::Interp;
use super::object::Obj;
use super::value::Val;

impl Interp<'_> {
    /// Matches `v` against `pat`, binding its variables as it goes;
    /// whether it matched.
    pub fn bind_pattern(&mut self, pat: &Pattern, v: &Val) -> R<bool> {
        match &pat.kind {
            PatKind::Wild => Ok(true),
            PatKind::Bind(b) => {
                self.bind(*b, v.clone())?;
                Ok(true)
            }
            PatKind::Lit(l) => self.lit_matches(l, v),
            PatKind::As(p, b) => {
                self.bind(*b, v.clone())?;
                self.bind_pattern(p, v)
            }
            PatKind::Ctor(t, variant, subs) => self.ctor_matches(*t, *variant, subs, v),
        }
    }

    fn ctor_matches(
        &mut self,
        t: TypeId,
        variant: Option<usize>,
        subs: &[Pattern],
        v: &Val,
    ) -> R<bool> {
        if t == self.p.globals.option {
            return match (variant, v, subs) {
                (Some(0), Val::None, _) => Ok(true),
                (Some(1), Val::Some(inner), [p]) => self.bind_pattern(p, inner),
                (Some(0) | Some(1), Val::None | Val::Some(_), _) => Ok(false),
                _ => Err(RunError::internal(format!(
                    "an Option pattern against {v:?}"
                ))),
            };
        }
        if let Val::Tag(ty, i) = v {
            return Ok(*ty == t && variant == Some(*i as usize));
        }
        let id = v.expect_obj("a constructor pattern's scrutinee")?;
        let tag = match self.objs.get(id)? {
            Obj::Struct { .. } => None,
            Obj::Variant { tag, .. } => Some(*tag as usize),
            o => {
                return Err(RunError::internal(format!(
                    "a constructor pattern against {o:?}"
                )))
            }
        };
        if tag != variant {
            return Ok(false);
        }
        self.sub_patterns(id, subs)
    }

    fn sub_patterns(&mut self, id: ObjId, subs: &[Pattern]) -> R<bool> {
        for (i, p) in subs.iter().enumerate() {
            if matches!(p.kind, PatKind::Wild) {
                continue;
            }
            let f = self.field(id, i)?;
            if !self.bind_pattern(p, &f)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn lit_matches(&self, l: &Lit, v: &Val) -> R<bool> {
        Ok(match (l, v) {
            (Lit::Int(n, _), Val::Int(m, _)) => n == m,
            (Lit::Float(x, _), Val::Float(y, _)) => x == y,
            (Lit::Char(c), Val::Char(d)) => c == d,
            (Lit::Bool(a), Val::Bool(b)) => a == b,
            (Lit::Unit, Val::Unit) => true,
            (Lit::Keyword(k), Val::Kw(id)) => self.statics.keyword_ids.get(k.as_str()) == Some(id),
            (Lit::Str(s), v @ Val::Obj(_)) => self.string(v)? == s.as_str(),
            _ => return Err(RunError::internal(format!("pattern {l:?} against {v:?}"))),
        })
    }
}
