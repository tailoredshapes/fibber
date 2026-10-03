//! Arrays and unique writes (types §2.13, §6.6, §8.3): `array`,
//! `array-len`, `array-get`, `array-with`, `array-copy`, and the two
//! in-place primitives `array-set!` and `set-field!`, which write in
//! place iff `fib.unique?` holds of the place's content and otherwise
//! store a changed copy into the place.

use crate::heap::{Kind, ObjId};
use crate::types::ast::{BindingId, Expr};

use super::alloc::Placement;
use super::error::{RunError, R};
use super::interp::Interp;
use super::object::Obj;
use super::value::Val;

impl<'p> Interp<'p> {
    /// The array builtins, by name.
    pub fn array_builtin(&mut self, name: &str, a: &[Val]) -> R<Val> {
        let arg = |i: usize| {
            a.get(i)
                .ok_or_else(|| RunError::internal(format!("{name} without argument {i}")))
        };
        match name {
            "array" => {
                let n = arg(0)?.as_int()?;
                let n = usize::try_from(n)
                    .map_err(|_| RunError::trap(format!("array of negative length {n}")))?;
                let x = arg(1)?.clone();
                let v = self.new_array(vec![x.clone(); n], Placement::Heap)?;
                self.give_back(&[x])?;
                Ok(v)
            }
            "array-len" => Ok(Val::Int(
                self.items(arg(0)?)?.len() as i64,
                crate::types::ty::Scalar::I64,
            )),
            "array-get" => {
                let id = arg(0)?.expect_obj("an array")?;
                let i = self.index(id, arg(1)?)?;
                let v = self.field(id, i)?;
                self.retain(&v)?;
                Ok(v)
            }
            "array-with" => {
                let id = arg(0)?.expect_obj("an array")?;
                let i = self.index(id, arg(1)?)?;
                let x = arg(2)?.clone();
                let v = self.changed_copy(id, i, x.clone())?;
                self.give_back(&[x])?;
                Ok(v)
            }
            "array-copy" => self.array_copy(arg(0)?, arg(1)?, arg(2)?),
            "array-set!" => {
                let cell = arg(0)?.expect_obj("the place of array-set!")?;
                let content = self.slot(cell)?.expect_obj("an array")?;
                let i = self.index(content, arg(1)?)?;
                self.unique_write(cell, i, arg(2)?.clone())
            }
            _ => Err(RunError::internal(format!("no array builtin {name}"))),
        }
    }

    /// The elements of an array value.
    pub(super) fn items(&self, v: &Val) -> R<&Vec<Val>> {
        match self.objs.get(&self.heap, v.expect_obj("an array")?)? {
            Obj::Array(items) => Ok(items),
            o => Err(RunError::internal(format!("not an array: {o:?}"))),
        }
    }

    /// `i` as an index of the array `id`, or a trap.
    fn index(&self, id: ObjId, i: &Val) -> R<usize> {
        let n = self.items(&Val::Obj(id))?.len();
        let i = i.as_int()?;
        usize::try_from(i)
            .ok()
            .filter(|i| *i < n)
            .ok_or_else(|| RunError::trap(format!("array index {i} out of range 0..{n}")))
    }

    fn array_copy(&mut self, a: &Val, i: &Val, j: &Val) -> R<Val> {
        let items = self.items(a)?;
        let (i, j, n) = (i.as_int()?, j.as_int()?, items.len() as i64);
        if !(0 <= i && i <= j && j <= n) {
            return Err(RunError::trap(format!(
                "array-copy [{i}, {j}) out of range 0..{n}"
            )));
        }
        let slice = items[i as usize..j as usize].to_vec();
        self.new_array(slice, Placement::Heap)
    }

    /// A copy of the struct or array `id` with field `i` set to `x`
    /// (which the copy stores; the caller gives back `x`'s count).
    fn changed_copy(&mut self, id: ObjId, i: usize, x: Val) -> R<Val> {
        let obj = match self.objs.get(&self.heap, id)? {
            Obj::Array(items) => Obj::Array(items.clone()),
            Obj::Struct { ty, fields } => Obj::Struct {
                ty: *ty,
                fields: fields.clone(),
            },
            o => return Err(RunError::internal(format!("a unique write to {o:?}"))),
        };
        let obj = match obj {
            Obj::Array(mut items) => {
                items[i] = x;
                Obj::Array(items)
            }
            Obj::Struct { ty, mut fields } => {
                fields[i] = x;
                Obj::Struct { ty, fields }
            }
            o => o,
        };
        let fields = match &obj {
            Obj::Array(f) | Obj::Struct { fields: f, .. } => f.iter().map(Val::project).collect(),
            _ => Vec::new(),
        };
        Ok(Val::Obj(self.alloc(
            Kind::Immutable,
            fields,
            obj,
            Placement::Heap,
        )?))
    }

    /// The unique write of §6.6 through the cell `place`: in place when
    /// `fib.unique?` holds of its content, else a copy with the change
    /// stored into the place (the old object released). `x` arrives
    /// consumed and is stored.
    fn unique_write(&mut self, place: ObjId, i: usize, x: Val) -> R<Val> {
        let content = self.slot(place)?.expect_obj("the content of a place")?;
        if self.heap.is_unique(content)? {
            self.heap.write_unique(place, i, x.project())?;
            let w = self.w();
            match w.objs.get_mut(&w.heap, content)? {
                Obj::Array(items) => items[i] = x.clone(),
                Obj::Struct { fields, .. } => fields[i] = x.clone(),
                o => return Err(RunError::internal(format!("a unique write to {o:?}"))),
            }
        } else {
            let copy = self.changed_copy(content, i, x.clone())?;
            self.write_slot(place, copy.clone())?;
            self.release(&copy)?;
        }
        self.give_back(&[x])?;
        Ok(Val::Unit)
    }

    /// `(set-field! &b f v)` (§2.13): the unique write of field `f` of
    /// the struct in `b`'s own cell.
    pub fn set_field(&mut self, b: BindingId, f: &str, v: &'p Expr) -> R<Val> {
        let place = self.local(b)?.expect_obj("the place of set-field!")?;
        let x = self.val(v)?;
        let content = self.slot(place)?.expect_obj("a struct")?;
        let i = self.field_index(content, f)?;
        self.unique_write(place, i, x)
    }
}
