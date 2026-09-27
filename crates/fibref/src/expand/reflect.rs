//! Expansion-time reflection (§3.16): `struct?`, `struct-fields`,
//! `struct-params`, `struct-field-types`, `enum?`, `enum-params`,
//! `enum-variants`, answered from the context's type table. The
//! evaluator calls [`ExpandCtx::reflect`] when a macro body calls one.

use crate::syntax::{Form, Pos};

use super::build::{boolean, vector};
use super::ctx::ExpandCtx;
use super::error::{ExpandError, ExpandErrorKind};

/// The reflection calls, by name.
pub const REFLECTION_CALLS: [&str; 7] = [
    "struct?",
    "struct-fields",
    "struct-params",
    "struct-field-types",
    "enum?",
    "enum-params",
    "enum-variants",
];

impl ExpandCtx {
    /// Evaluates the reflection call `(op arg)`, where `arg` is the value
    /// of the operand (a `Sym` form, e.g. from `'Name`), at `pos`.
    ///
    /// `struct?` and `enum?` return a `Bool` form; the others a `Vec`
    /// form. The struct calls fail with [`ExpandErrorKind::NotAStruct`]
    /// and the enum calls with [`ExpandErrorKind::NotAnEnum`] when the
    /// name is not one (§3.16). Returned field names and types are the
    /// forms of the declaration, with their own positions.
    pub fn reflect(&self, op: &str, arg: &Form, pos: &Pos) -> Result<Form, ExpandError> {
        let bad = || {
            let op = op.to_string();
            ExpandError::new(ExpandErrorKind::BadReflection { op }, pos)
        };
        let Some(name) = arg.as_sym() else {
            return Err(bad());
        };
        match op {
            "struct?" => Ok(boolean(self.struct_info(name).is_some(), pos)),
            "enum?" => Ok(boolean(self.enum_info(name).is_some(), pos)),
            "struct-fields" | "struct-params" | "struct-field-types" => {
                self.reflect_struct(op, name, pos)
            }
            "enum-params" | "enum-variants" => self.reflect_enum(op, name, pos),
            _ => Err(bad()),
        }
    }

    fn reflect_struct(&self, op: &str, name: &str, pos: &Pos) -> Result<Form, ExpandError> {
        let Some(info) = self.struct_info(name) else {
            let op = static_name(op);
            let name = name.to_string();
            return Err(ExpandError::new(
                ExpandErrorKind::NotAStruct { op, name },
                pos,
            ));
        };
        let items: Vec<Form> = match op {
            "struct-fields" => info.fields.iter().map(|(n, _)| n.clone()).collect(),
            "struct-params" => info.params.clone(),
            _ => info.fields.iter().map(|(_, t)| t.clone()).collect(),
        };
        Ok(vector(items, pos))
    }

    fn reflect_enum(&self, op: &str, name: &str, pos: &Pos) -> Result<Form, ExpandError> {
        let Some(info) = self.enum_info(name) else {
            let op = static_name(op);
            let name = name.to_string();
            return Err(ExpandError::new(
                ExpandErrorKind::NotAnEnum { op, name },
                pos,
            ));
        };
        let items: Vec<Form> = match op {
            "enum-params" => info.params.clone(),
            _ => info.variants.iter().map(|v| v.as_form(pos)).collect(),
        };
        Ok(vector(items, pos))
    }
}

/// The `'static` spelling of a reflection call name.
fn static_name(op: &str) -> &'static str {
    REFLECTION_CALLS
        .iter()
        .find(|n| **n == op)
        .copied()
        .unwrap_or("reflection")
}
