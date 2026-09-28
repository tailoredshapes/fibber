//! `Option` values as types §8.1 represents them. For a payload type
//! that is an object other than an `Option`, the nullable pointer:
//! [`Val::Some`] and [`Val::None`], which allocate nothing and whose
//! counts are the payload's. For a scalar, `dyn` or `Option` payload,
//! a heap enum object: an [`Obj::Variant`] of `Option` with tag `0`
//! (`nil`) or `1` (`some`, one field), counted and audited like any
//! other enum, which is what lets `(some nil)` and `nil` stay distinct.
//!
//! The plan decides by the static type (`own::objects::option_rep`).
//! In a generic body the payload type is a quantified variable and the
//! interpreter, which does not monomorphise (types §4.3), decides by
//! the payload it gets: a `some` of a scalar or of an `Option` is a heap
//! enum, a `some` of any other object is not, and a `nil` is the bare
//! `Val::None` (a generic `nil` has no payload to decide by; `match`
//! accepts both spellings of an `Option`, so only the audit can tell,
//! and only in generic code). A `some` of a `dyn` over an object in
//! generic code is therefore not boxed either.

use crate::heap::{Kind, ObjId};

use super::alloc::Placement;
use super::error::{RunError, R};
use super::interp::Interp;
use super::object::Obj;
use super::value::Val;

impl Interp<'_> {
    /// `nil` (`variant` 0, no field) or `(some v)` (`variant` 1), the
    /// payload consumed (E2), placed as the plan decided.
    pub fn option_value(
        &mut self,
        variant: Option<usize>,
        fields: Vec<Val>,
        at: Placement,
    ) -> R<Val> {
        let boxed = match at {
            Placement::Unboxed => false,
            Placement::Undecided => fields.first().is_some_and(|v| self.needs_box(v)),
            Placement::Heap | Placement::Stack | Placement::Immortal => true,
        };
        if !boxed {
            return match (variant, fields.into_iter().next()) {
                (Some(1), Some(v)) => Ok(Val::Some(Box::new(v))),
                (Some(0), None) => Ok(Val::None),
                _ => Err(RunError::internal("a malformed Option constructor")),
            };
        }
        let tag = match (variant, fields.len()) {
            (Some(0), 0) => 0,
            (Some(1), 1) => 1,
            _ => return Err(RunError::internal("a malformed Option constructor")),
        };
        // Never on the stack (§6.11: an `Option` is never a candidate).
        let at = match at {
            Placement::Immortal => Placement::Immortal,
            _ => Placement::Heap,
        };
        let ty = self.p.globals.option;
        let projected = fields.iter().map(Val::project).collect();
        let obj = Obj::Variant { ty, tag, fields };
        let id = self.alloc(Kind::Immutable, projected, obj, at)?;
        Ok(Val::Obj(id))
    }

    /// Whether a `some` of `v` in generic code is a heap enum: `v` is a
    /// scalar or an `Option` (bare or boxed).
    fn needs_box(&self, v: &Val) -> bool {
        match v {
            Val::Obj(id) => self.is_boxed_option(*id),
            _ => true,
        }
    }

    /// Whether `id` is a heap-enum `Option`.
    pub fn is_boxed_option(&self, id: ObjId) -> bool {
        let option = self.p.globals.option;
        matches!(self.objs.get(&self.heap, id), Ok(Obj::Variant { ty, .. }) if *ty == option)
    }
}
