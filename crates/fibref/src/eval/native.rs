//! Calls that no plan describes because they are primitives: builtins
//! (syntax §4.3), the methods of built-in instances (types §2.9,
//! §2.12), constructors and externs. Each performs the count
//! operations that are part of its definition (own/program.rs, "what
//! the primitives do by themselves"): a stored operand arrives consumed
//! and the store keeps it; an object result is owned by the caller.

use crate::heap::Kind;
use crate::types::builtins::{Escape, BUILTINS};

use crate::syntax::Pos;

use super::alloc::Placement;
use super::call::Target;
use super::error::{RunError, R};
use super::interp::Interp;
use super::value::Val;

impl<'p> Interp<'p> {
    /// Runs a primitive call.
    pub fn native_call(
        &mut self,
        target: &Target,
        args: &[Val],
        pos: &Pos,
        placement: Placement,
    ) -> R<Val> {
        match *target {
            Target::Builtin(b, owned) => {
                let v = self.builtin(b.0 as usize, args, pos, placement)?;
                if owned {
                    self.release_borrowed(b.0 as usize, args)?;
                }
                Ok(v)
            }
            Target::Native(inst, m, owned) => {
                let v = self.native_method(inst, m, args)?;
                if owned {
                    self.release_unowned(inst, m, args)?;
                }
                Ok(v)
            }
            Target::Ctor(t, variant) => {
                let v = self.new_data(t, variant, args.to_vec(), placement)?;
                if !matches!(v, Val::Some(_) | Val::None) {
                    self.give_back(args)?;
                }
                Ok(v)
            }
            Target::Extern(x) => self.call_extern(x, args),
            Target::Body(_) | Target::Lambda(_) => {
                Err(RunError::internal("a body target at a native call"))
            }
        }
    }

    /// Through a function value every object argument is the callee's
    /// (§8.4): a builtin's all-owned form releases, at its exit, the
    /// arguments that its own convention only borrows.
    fn release_borrowed(&mut self, b: usize, args: &[Val]) -> R<()> {
        for (e, v) in BUILTINS[b].escapes.iter().zip(args) {
            if matches!(e, Escape::Borrow | Escape::Weak | Escape::Raw) {
                self.release(v)?;
            }
        }
        Ok(())
    }

    /// The built-in implementation of method `m` of instance `inst`
    /// reached through a method value (§8.4): it releases, at its exit,
    /// the arguments at the positions the protocol does not declare
    /// `:owned` (§6.4), which its own convention only borrows.
    fn release_unowned(&mut self, inst: usize, m: usize, args: &[Val]) -> R<()> {
        let g = &self.p.globals;
        let md = &g.proto(g.instances[inst].proto).methods[m];
        for (j, v) in args.iter().enumerate() {
            if !md.params.get(j).is_some_and(|mp| mp.owned) {
                self.release(v)?;
            }
        }
        Ok(())
    }

    fn builtin(&mut self, b: usize, a: &[Val], pos: &Pos, placement: Placement) -> R<Val> {
        let arg = |i: usize| -> R<&Val> {
            a.get(i).ok_or_else(|| {
                RunError::internal(format!("{} without argument {i}", BUILTINS[b].name))
            })
        };
        match BUILTINS[b].name {
            "cell" => self.store_new(Kind::Cell, arg(0)?, placement),
            "atom" => self.store_new(Kind::Atom, arg(0)?, placement),
            "set!" => self.set_cell(arg(0)?, arg(1)?),
            "reset!" => self.set_cell(arg(0)?, arg(1)?),
            "swap!" => self.swap(arg(0)?, arg(1)?, pos),
            "weak" => self.weak(arg(0)?),
            "spawn" => self.spawn(arg(0)?, pos),
            "join" => self.join(arg(0)?),
            "trap" => Err(RunError::trap(self.string(arg(0)?)?.to_string())),
            "not" => Ok(Val::Bool(!arg(0)?.as_bool()?)),
            "concat" => {
                let parts: Vec<Val> = a.to_vec();
                self.concat_vals(&parts)
            }
            name @ ("array" | "array-len" | "array-get" | "array-with" | "array-copy"
            | "array-set!") => self.array_builtin(name, a),
            name @ ("str-len" | "str-bytes" | "str-concat" | "str-slice" | "str-eq"
            | "starts-with?") => self.string_builtin(name, a),
            "char->i32" | "i32->char" => super::arith::char_conv(BUILTINS[b].name, arg(0)?),
            "gensym" | "struct?" | "struct-fields" | "struct-params" | "struct-field-types"
            | "enum?" | "enum-params" | "enum-variants" => {
                self.expansion_builtin(BUILTINS[b].name, arg(0)?, pos)
            }
            name => self.unsafe_builtin(name, a),
        }
    }

    /// `(cell v)`, `(atom v)`: a new slot holding the consumed `v`.
    fn store_new(&mut self, kind: Kind, v: &Val, at: Placement) -> R<Val> {
        let c = self.new_slot(kind, v.clone(), at)?;
        self.give_back(std::slice::from_ref(v))?;
        Ok(c)
    }

    /// `(set! c v)`, `(reset! a v)`: store the consumed `v`, release the
    /// old content (§6.7, §8.6; a write into a shared atom share-marks
    /// `v` first, which the heap does).
    pub fn set_cell(&mut self, c: &Val, v: &Val) -> R<Val> {
        let id = c.expect_obj("a cell")?;
        self.write_slot(id, v.clone())?;
        self.give_back(std::slice::from_ref(v))?;
        Ok(Val::Unit)
    }
}
