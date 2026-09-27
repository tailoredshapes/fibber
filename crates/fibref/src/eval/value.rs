//! Run-time values (types §8.1): scalars unboxed, every object an
//! [`ObjId`] of the audited heap, and an `Option` of a non-`Option`
//! object unboxed with real tags (types §4.5: the interpreter carries
//! real tags; §8.1: `some` of an object allocates nothing). Every other
//! `Option` is a heap enum object (see `option`).

use crate::heap::{ObjId, Value};
use crate::types::ty::{Con, Scalar, TypeId};

/// A value as the evaluator holds it.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    /// `()`.
    Unit,
    /// `true` / `false`.
    Bool(bool),
    /// An integer of its width (`I8` .. `I64`), stored sign-extended.
    Int(i64, Scalar),
    /// A float of its width; an `F32` holds the nearest `f32` exactly.
    Float(f64, Scalar),
    /// A character.
    Char(char),
    /// A keyword: its interned id (types §8.1: `i64`, an interned id).
    Kw(u32),
    /// A raw pointer (`unsafe`), an address in the interpreter's arena.
    Ptr(u64),
    /// A variant of a field-less enum, a scalar (§1, §8.1).
    Tag(TypeId, u32),
    /// An object on the audited heap.
    Obj(ObjId),
    /// `nil` of a nullable-pointer `Option`.
    None,
    /// `(some v)` of a nullable-pointer `Option`: allocates nothing;
    /// counts are its payload's.
    Some(Box<Val>),
}

impl Val {
    /// The object whose count this value carries, if any: the object
    /// itself, or the payload's through any number of `some`s.
    pub fn obj(&self) -> Option<ObjId> {
        match self {
            Val::Obj(id) => Some(*id),
            Val::Some(v) => v.obj(),
            _ => None,
        }
    }

    /// The object, or an internal error naming what was expected.
    pub fn expect_obj(&self, what: &str) -> Result<ObjId, super::RunError> {
        self.obj()
            .ok_or_else(|| super::RunError::internal(format!("{what}: not an object: {self:?}")))
    }

    /// The heap's view of the value, as stored in a field: a `Ref` for
    /// an object (through `some`), a scalar otherwise.
    pub fn project(&self) -> Value {
        match self {
            Val::Obj(id) => Value::Ref(*id),
            Val::Some(v) => v.project(),
            Val::None | Val::Unit => Value::Nil,
            Val::Bool(b) => Value::Bool(*b),
            Val::Int(n, _) => Value::Int(*n),
            Val::Float(x, _) => Value::Float(*x),
            Val::Char(c) => Value::Char(*c),
            Val::Kw(k) => Value::Int(i64::from(*k)),
            Val::Ptr(p) => Value::Int(*p as i64),
            Val::Tag(_, i) => Value::Int(i64::from(*i)),
        }
    }

    /// The boolean, or an internal error.
    pub fn as_bool(&self) -> Result<bool, super::RunError> {
        match self {
            Val::Bool(b) => Ok(*b),
            v => Err(super::RunError::internal(format!("not a bool: {v:?}"))),
        }
    }

    /// The integer of any width, or an internal error.
    pub fn as_int(&self) -> Result<i64, super::RunError> {
        match self {
            Val::Int(n, _) => Ok(*n),
            v => Err(super::RunError::internal(format!("not an integer: {v:?}"))),
        }
    }

    /// The head constructor of a scalar value's type, for dispatch
    /// (types §4.5); `None` for objects and `Option`s, whose head the
    /// object table or the caller knows.
    pub fn scalar_con(&self) -> Option<Con> {
        let s = match self {
            Val::Unit => Scalar::Unit,
            Val::Bool(_) => Scalar::Bool,
            Val::Int(_, w) | Val::Float(_, w) => *w,
            Val::Char(_) => Scalar::Char,
            Val::Kw(_) => Scalar::Keyword,
            Val::Ptr(_) => Scalar::Ptr,
            Val::Tag(t, _) => return Some(Con::Nominal(*t)),
            _ => return None,
        };
        Some(Con::Scalar(s))
    }
}

/// Wraps an integer to the width `w`, sign-extending (§2.12: signed
/// overflow wraps).
pub fn wrap(n: i64, w: Scalar) -> i64 {
    match w {
        Scalar::I8 => i64::from(n as i8),
        Scalar::I16 => i64::from(n as i16),
        Scalar::I32 => i64::from(n as i32),
        _ => n,
    }
}

/// The number of bits of an integer width.
pub fn bits(w: Scalar) -> u32 {
    match w {
        Scalar::I8 => 8,
        Scalar::I16 => 16,
        Scalar::I32 => 32,
        _ => 64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn some_carries_its_payloads_count() {
        let id = crate::heap::Heap::new()
            .alloc(crate::heap::Kind::Immutable, vec![])
            .expect("alloc");
        let v = Val::Some(Box::new(Val::Some(Box::new(Val::Obj(id)))));
        assert_eq!(v.obj(), Some(id));
        assert_eq!(v.project(), Value::Ref(id));
        assert_eq!(Val::Some(Box::new(Val::Int(3, Scalar::I64))).obj(), None);
    }

    #[test]
    fn integers_wrap_at_their_width() {
        assert_eq!(wrap(128, Scalar::I8), -128);
        assert_eq!(wrap(70000, Scalar::I16), 70000 - 65536);
        assert_eq!(wrap(i64::MAX, Scalar::I64), i64::MAX);
        assert_eq!(bits(Scalar::I32), 32);
    }
}
