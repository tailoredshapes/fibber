//! Strings (syntax §4.3, types §8.3): UTF-8 bytes; lengths and indices
//! are in bytes. Literals are immortal (§8.2), made once per literal.

use crate::heap::ObjId;
use crate::types::ast::ExprId;
use crate::types::ty::Scalar;

use super::alloc::Placement;
use super::error::{RunError, R};
use super::interp::Interp;
use super::value::Val;

impl Interp<'_> {
    /// The immortal string of the literal `e`.
    pub fn string_literal(&mut self, e: ExprId, s: &str) -> R<Val> {
        if let Some(id) = self.statics.lits.get(&e) {
            return Ok(Val::Obj(*id));
        }
        let v = self.new_str(s.to_string(), Placement::Immortal)?;
        let id: ObjId = v.expect_obj("a literal")?;
        self.statics.lits.insert(e, id);
        Ok(v)
    }

    /// The string builtins, by name.
    pub fn string_builtin(&mut self, name: &str, a: &[Val]) -> R<Val> {
        let s = |i: usize| -> R<String> {
            let v = a
                .get(i)
                .ok_or_else(|| RunError::internal(format!("{name} without argument {i}")))?;
            Ok(self.string(v)?.to_string())
        };
        let int = |i: usize| -> R<i64> {
            a.get(i)
                .ok_or_else(|| RunError::internal(format!("{name} without argument {i}")))?
                .as_int()
        };
        match name {
            "str-len" => Ok(Val::Int(s(0)?.len() as i64, Scalar::I64)),
            "str-bytes" => {
                let bytes = s(0)?
                    .bytes()
                    .map(|b| Val::Int(i64::from(b as i8), Scalar::I8))
                    .collect();
                self.new_array(bytes, Placement::Heap)
            }
            "str-concat" => {
                let joined = s(0)? + &s(1)?;
                self.new_str(joined, Placement::Heap)
            }
            "str-slice" => {
                let (text, i, j) = (s(0)?, int(1)?, int(2)?);
                let slice = byte_slice(&text, i, j)?;
                self.new_str(slice, Placement::Heap)
            }
            "str-eq" => Ok(Val::Bool(s(0)? == s(1)?)),
            "starts-with?" => Ok(Val::Bool(s(0)?.starts_with(&s(1)?))),
            _ => Err(RunError::internal(format!("no string builtin {name}"))),
        }
    }
}

/// Bytes `[i, j)` of `s`, which must fall on character boundaries.
fn byte_slice(s: &str, i: i64, j: i64) -> R<String> {
    let n = s.len() as i64;
    if !(0 <= i && i <= j && j <= n) {
        return Err(RunError::trap(format!(
            "str-slice [{i}, {j}) out of range 0..{n}"
        )));
    }
    s.get(i as usize..j as usize)
        .map(str::to_string)
        .ok_or_else(|| RunError::trap(format!("str-slice [{i}, {j}) splits a character")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_are_in_bytes_and_respect_characters() {
        assert_eq!(byte_slice("hello", 1, 3), Ok("el".to_string()));
        assert!(byte_slice("hello", 3, 9).is_err());
        assert!(byte_slice("é", 0, 1).is_err());
    }
}
