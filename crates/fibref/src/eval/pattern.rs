//! Patterns (syntax §3.6): matching a value and binding its parts. The
//! bindings are plain values; what they count is the plan's business
//! (pattern variables are aliases or parts of the scrutinee, §6.1),
//! except a rest variable's, which owns the new vector built for it
//! once the whole pattern has matched (§6.3).

use crate::heap::ObjId;
use crate::types::ast::{BindingId, Lit, PatKind, Pattern, Rest};
use crate::types::ty::TypeId;

use super::error::{RunError, R};
use super::interp::Interp;
use super::object::Obj;
use super::value::Val;

/// The rests of a pattern that has not finished matching: the variable,
/// the vector, and how many elements its pattern names.
type Rests = Vec<(BindingId, Val, usize)>;

impl Interp<'_> {
    /// Matches `v` against `pat`, binding its variables as it goes;
    /// whether it matched. Only when the whole pattern matched are its
    /// rest vectors built and bound, left to right.
    pub fn bind_pattern(&mut self, pat: &Pattern, v: &Val) -> R<bool> {
        let mut rests = Rests::new();
        if !self.match_pattern(pat, v, &mut rests)? {
            return Ok(false);
        }
        for (b, vec, k) in rests {
            let r = self.vec_drop(&vec, k)?;
            self.bind(b, r)?;
        }
        Ok(true)
    }

    fn match_pattern(&mut self, pat: &Pattern, v: &Val, rests: &mut Rests) -> R<bool> {
        match &pat.kind {
            PatKind::Wild => Ok(true),
            PatKind::Bind(b) => {
                self.bind(*b, v.clone())?;
                Ok(true)
            }
            PatKind::Lit(l) => self.lit_matches(l, v),
            PatKind::As(p, b) => {
                self.bind(*b, v.clone())?;
                self.match_pattern(p, v, rests)
            }
            PatKind::Ctor(t, variant, subs) => self.ctor_matches(*t, *variant, subs, v, rests),
            PatKind::Vec(subs, rest) => self.vec_matches(subs, *rest, v, rests),
        }
    }

    /// `[p.. & r]`: the length first, then the elements left to right,
    /// read through the core view (types §8.3); the rest is noted.
    fn vec_matches(&mut self, subs: &[Pattern], rest: Rest, v: &Val, rests: &mut Rests) -> R<bool> {
        let n = self.vec_len(v)?;
        let k = subs.len();
        if n < k || (rest == Rest::Exact && n != k) {
            return Ok(false);
        }
        for (i, p) in subs.iter().enumerate() {
            if matches!(p.kind, PatKind::Wild) {
                continue;
            }
            let e = self.vec_elem(v, i)?;
            if !self.match_pattern(p, &e, rests)? {
                return Ok(false);
            }
        }
        if let Rest::Bind(b) = rest {
            rests.push((b, v.clone(), k));
        }
        Ok(true)
    }

    fn ctor_matches(
        &mut self,
        t: TypeId,
        variant: Option<usize>,
        subs: &[Pattern],
        v: &Val,
        rests: &mut Rests,
    ) -> R<bool> {
        // A bare `Option` (§8.1's nullable pointer); a heap-enum one is
        // an ordinary variant object, matched below.
        if t == self.p.globals.option && !matches!(v, Val::Obj(_)) {
            return match (variant, v, subs) {
                (Some(0), Val::None, _) => Ok(true),
                (Some(1), Val::Some(inner), [p]) => self.match_pattern(p, inner, rests),
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
        let tag = match self.objs.get(&self.heap, id)? {
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
        self.sub_patterns(id, subs, rests)
    }

    fn sub_patterns(&mut self, id: ObjId, subs: &[Pattern], rests: &mut Rests) -> R<bool> {
        for (i, p) in subs.iter().enumerate() {
            if matches!(p.kind, PatKind::Wild) {
                continue;
            }
            let f = self.field(id, i)?;
            if !self.match_pattern(p, &f, rests)? {
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
