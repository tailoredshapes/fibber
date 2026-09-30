//! `def` values as static data (types §8.10, syntax §3.19): each
//! initialiser is evaluated at compile time and the resulting graph is
//! emitted as `IMMORTAL` constants referring to each other by address,
//! `def`s in source order, each seeing the constants of the ones
//! before it. With the `llvm` feature the initialiser is compiled and
//! run through the JIT that runs macros ([`jit`], compiler.md §8 item
//! 4); without it the reference interpreter evaluates it ([`interp`]),
//! and `tests/defs.rs` requires the two to emit the same text.

pub mod interp;
#[cfg(feature = "llvm")]
pub mod jit;

use std::collections::HashMap;
use std::fmt::Write;

use fibref::types::ast::DefId;
use fibref::types::infer::UnitRef;
use fibref::types::ty::Ty;

use crate::compile::Unsupported;
use crate::ir::LirTy;
use crate::layout::{variants, IMMORTAL};
use crate::program::Program;

/// The constants of every `def`, and each `def`'s value as lIR text.
#[derive(Debug, Default)]
pub struct Defs {
    pub text: String,
    pub values: HashMap<DefId, (String, Option<LirTy>)>,
    /// The constants emitted so far, by name (`@def.N`).
    pub constants: Vec<String>,
    counter: usize,
}

/// Evaluates every `def` of the user module and serialises its value.
pub fn emit_defs(p: &mut Program<'_>) -> Result<Defs, Unsupported> {
    #[cfg(feature = "llvm")]
    {
        jit::emit_defs(p)
    }
    #[cfg(not(feature = "llvm"))]
    {
        interp::emit_defs(p)
    }
}

/// The `def`s of the user module, in source order.
pub fn def_ids(p: &Program<'_>) -> Vec<DefId> {
    p.c.typed
        .units
        .iter()
        .filter_map(|u| match u {
            UnitRef::Def(d) => Some(*d),
            _ => None,
        })
        .collect()
}

impl Defs {
    /// An array constant of `t = (Array E)` with these element texts.
    pub fn emit_array(
        &mut self,
        p: &mut Program<'_>,
        t: &Ty,
        el: LirTy,
        elems: &[String],
    ) -> Result<String, Unsupported> {
        let (tid, _) = p.object(t)?;
        let n = elems.len();
        let name = self.fresh();
        let _ = writeln!(
            self.text,
            "(constant internal {} {{ i64 i32 i32 i64 [{n} x {}] }} {{ (i64 0) (i32 {tid}) (i32 {IMMORTAL}) (i64 {n}) ([{n} x {}] {}) }})",
            &name[1..],
            el.text(),
            el.text(),
            elems.join(" ")
        );
        Ok(name)
    }

    /// A struct or variant constant of nominal type `t` with these
    /// field texts (a unit field's is empty and is skipped).
    pub fn emit_object(
        &mut self,
        p: &mut Program<'_>,
        t: &Ty,
        tag: Option<usize>,
        fields: &[String],
    ) -> Result<String, Unsupported> {
        let (tid, sname) = p.object(t)?;
        let sname = match tag {
            None => sname,
            Some(i) => format!("{sname}.v{i}"),
        };
        let mut texts = vec![
            "(i64 0)".to_string(),
            format!("(i32 {tid})"),
            format!("(i32 {IMMORTAL})"),
        ];
        if let Some(i) = tag {
            texts.push(format!("(i32 {i})"));
        }
        texts.extend(fields.iter().filter(|s| !s.is_empty()).cloned());
        let name = self.fresh();
        let _ = writeln!(
            self.text,
            "(constant internal {} %struct.{sname} (%struct.{sname} {}))",
            &name[1..],
            texts.join(" ")
        );
        Ok(name)
    }

    /// The field types of the struct, or of variant `tag`, of nominal
    /// type `t`.
    pub fn field_types(
        p: &Program<'_>,
        t: &Ty,
        tag: Option<usize>,
    ) -> Result<Vec<Ty>, Unsupported> {
        let Ty::Con(fibref::types::ty::Con::Nominal(id), args) = t else {
            return Err(Unsupported("an object at a non-nominal type".into()));
        };
        let vs = variants(p.g(), *id, args)?;
        Ok(vs[tag.unwrap_or(0)].1.clone())
    }

    fn fresh(&mut self) -> String {
        self.counter += 1;
        let name = format!("@def.{}", self.counter);
        self.constants.push(name.clone());
        name
    }
}
