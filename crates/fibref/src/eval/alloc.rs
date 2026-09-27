//! Allocation (types §6.11, §8.2): every object goes onto the audited
//! heap as a counted heap object, a `STACK` object in a scope of its
//! own, or an `IMMORTAL` object, and into the object table.
//!
//! A value that a primitive stores into a new object arrives consumed
//! (the plan moved or retained it for the store, E2); the heap counts
//! the store itself, so the primitive then gives the consumed count
//! back ([`Interp::give_back`]). The net effect is the store holding
//! one count, as in compiled code; the trace shows the retain and the
//! release that compiled code folds into a move.

use crate::heap::{Kind, ObjId, Value};
use crate::own::program::Alloc;
use crate::types::ty::TypeId;

use super::error::{RunError, R};
use super::interp::Interp;
use super::object::Obj;
use super::value::Val;

/// Where a new object lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Counted, on the heap (count 1 for the caller).
    Heap,
    /// Scope-local: `STACK`, ended by the plan's `EndStack`.
    Stack,
    /// Static data: literals, `def` values, function values.
    Immortal,
    /// An `Option` site whose type is a nullable pointer (§8.1,
    /// `Alloc::Nothing`): no object. Any other object: the heap.
    Unboxed,
    /// A site the plan decided nothing for: a generic `Option` site, or a
    /// call through a function value. An object goes on the heap; an
    /// `Option` is a heap enum iff its payload at run time is a scalar
    /// or an `Option` (see `Interp::option_value`).
    Undecided,
}

impl Placement {
    /// The placement the plan decided for an allocation site.
    pub fn of(alloc: Option<Alloc>) -> Placement {
        match alloc {
            Some(Alloc::Stack) => Placement::Stack,
            Some(Alloc::Heap) => Placement::Heap,
            Some(Alloc::Nothing) => Placement::Unboxed,
            None => Placement::Undecided,
        }
    }
}

impl Interp<'_> {
    /// Allocates an object whose heap fields are `fields` and which is
    /// `obj`.
    pub fn alloc(&mut self, kind: Kind, fields: Vec<Value>, obj: Obj, at: Placement) -> R<ObjId> {
        let id = match at {
            Placement::Heap | Placement::Unboxed | Placement::Undecided => {
                self.heap.alloc(kind, fields)?
            }
            Placement::Immortal => self.heap.alloc_immortal(kind, fields)?,
            Placement::Stack => {
                let scope = self.heap.open_scope();
                let id = self.heap.alloc_in_scope(scope, kind, fields)?;
                self.stack_scopes.insert(id, scope);
                id
            }
        };
        self.objs.insert(id, obj);
        Ok(id)
    }

    /// Releases the counts that consumed values brought to a store the
    /// heap has already counted.
    pub fn give_back(&mut self, vals: &[Val]) -> R<()> {
        for v in vals {
            self.release(v)?;
        }
        Ok(())
    }

    /// A new string.
    pub fn new_str(&mut self, s: String, at: Placement) -> R<Val> {
        let id = self.alloc(Kind::Immutable, Vec::new(), Obj::Str(s), at)?;
        Ok(Val::Obj(id))
    }

    /// A new array of `items` (each stored: E2, not given back here).
    pub fn new_array(&mut self, items: Vec<Val>, at: Placement) -> R<Val> {
        let fields = items.iter().map(Val::project).collect();
        let id = self.alloc(Kind::Immutable, fields, Obj::Array(items), at)?;
        Ok(Val::Obj(id))
    }

    /// A new struct (variant `None`) or variant of `ty` holding
    /// `fields`; an `Option` as §8.1 represents it (`option_value`).
    pub fn new_data(
        &mut self,
        ty: TypeId,
        variant: Option<usize>,
        fields: Vec<Val>,
        at: Placement,
    ) -> R<Val> {
        if ty == self.p.globals.option {
            return self.option_value(variant, fields, at);
        }
        let projected = fields.iter().map(Val::project).collect();
        let obj = match variant {
            None => Obj::Struct { ty, fields },
            Some(tag) => Obj::Variant {
                ty,
                tag: tag as u32,
                fields,
            },
        };
        let id = self.alloc(Kind::Immutable, projected, obj, at)?;
        Ok(Val::Obj(id))
    }

    /// A new cell or atom holding `v` (stored, not given back here).
    pub fn new_slot(&mut self, kind: Kind, v: Val, at: Placement) -> R<Val> {
        let obj = match kind {
            Kind::Atom => Obj::Atom(v.clone()),
            _ => Obj::Cell(v.clone()),
        };
        let id = self.alloc(kind, vec![v.project()], obj, at)?;
        Ok(Val::Obj(id))
    }

    /// The content of the cell or atom `id` (read on the heap).
    pub fn slot(&mut self, id: ObjId) -> R<Val> {
        self.heap.read(id, 0)?;
        match self.objs.get(id)? {
            Obj::Cell(v) | Obj::Atom(v) => Ok(v.clone()),
            o => Err(RunError::internal(format!("{id} is not a cell: {o:?}"))),
        }
    }

    /// Writes the cell or atom `id` with `set!` semantics on the heap
    /// (retain the new, release the old); the caller gives back the
    /// consumed count of `v`.
    pub fn write_slot(&mut self, id: ObjId, v: Val) -> R<()> {
        self.heap.write(id, 0, v.project())?;
        match self.objs.get_mut(id)? {
            Obj::Cell(slot) | Obj::Atom(slot) => *slot = v,
            o => return Err(RunError::internal(format!("{id} is not a cell: {o:?}"))),
        }
        Ok(())
    }

    /// The fields of a struct, variant or array (for reads).
    pub fn fields(&self, id: ObjId) -> R<&Vec<Val>> {
        match self.objs.get(id)? {
            Obj::Struct { fields, .. } | Obj::Variant { fields, .. } | Obj::Array(fields) => {
                Ok(fields)
            }
            o => Err(RunError::internal(format!("{id} has no fields: {o:?}"))),
        }
    }

    /// Field `i` of a struct, variant or array, read on the heap.
    pub fn field(&mut self, id: ObjId, i: usize) -> R<Val> {
        self.heap.read(id, i)?;
        self.fields(id)?
            .get(i)
            .cloned()
            .ok_or_else(|| RunError::internal(format!("{id} has no field {i}")))
    }

    /// The string `id`.
    pub fn string(&self, v: &Val) -> R<&str> {
        let id = v.expect_obj("a string")?;
        match self.objs.get(id)? {
            Obj::Str(s) => Ok(s),
            o => Err(RunError::internal(format!("{id} is not a string: {o:?}"))),
        }
    }
}
