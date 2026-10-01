//! Strings (syntax §4.3, types §8.3): UTF-8 bytes; lengths and indices
//! are in bytes. Literals are immortal (§8.2), made once per literal.

use crate::heap::ObjId;
use crate::types::ast::ExprId;
use crate::types::ty::Scalar;

use super::alloc::Placement;
use super::error::{RunError, R};
use super::interp::Interp;
use super::object::Obj;
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
            "str-from-bytes" => {
                let id = a
                    .first()
                    .ok_or_else(|| {
                        RunError::internal("str-from-bytes without its array".to_string())
                    })?
                    .expect_obj("an array")?;
                let items = match self.objs.get(&self.heap, id)? {
                    Obj::Array(items) => items.clone(),
                    o => return Err(RunError::internal(format!("{id} is not an array: {o:?}"))),
                };
                let bytes = items
                    .iter()
                    .map(|v| v.as_int().map(|n| n as u8))
                    .collect::<R<Vec<u8>>>()?;
                let text = String::from_utf8(bytes)
                    .map_err(|_| RunError::trap("str-from-bytes: invalid UTF-8".to_string()))?;
                self.new_str(text, Placement::Heap)
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
            "str-byte-at" => {
                let b = byte_at(&s(0)?, int(1)?)?;
                Ok(Val::Int(i64::from(b as i8), Scalar::I8))
            }
            "str-find" => match find_from(&s(0)?, &s(1)?, int(2)?)? {
                Some(at) => {
                    let n = Val::Int(at, Scalar::I64);
                    self.option_value(Some(1), vec![n], Placement::Heap)
                }
                None => self.option_value(Some(0), Vec::new(), Placement::Heap),
            },
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

/// The byte of `s` at `i`.
fn byte_at(s: &str, i: i64) -> R<u8> {
    usize::try_from(i)
        .ok()
        .and_then(|k| s.as_bytes().get(k).copied())
        .ok_or_else(|| {
            RunError::trap(format!(
                "str-byte-at: index {i} out of range 0..{}",
                s.len()
            ))
        })
}

/// The byte offset of the first `pat` at or after byte `from` of `s`:
/// `from` must be in `0..=len` and on a character boundary, and an empty
/// `pat` is found at `from`.
fn find_from(s: &str, pat: &str, from: i64) -> R<Option<i64>> {
    let n = s.len() as i64;
    if !(0 <= from && from <= n) {
        return Err(RunError::trap(format!(
            "str-find: from {from} out of range 0..{n}"
        )));
    }
    let rest = s
        .get(from as usize..)
        .ok_or_else(|| RunError::trap(format!("str-find: from {from} splits a character")))?;
    Ok(rest.find(pat).map(|at| from + at as i64))
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

    #[test]
    fn a_byte_is_read_and_a_bad_index_traps() {
        assert_eq!(byte_at("aé", 0), Ok(b'a'));
        assert_eq!(byte_at("aé", 1), Ok(0xC3));
        assert!(byte_at("aé", 3).is_err());
        assert!(byte_at("aé", -1).is_err());
    }

    #[test]
    fn find_starts_at_from_and_counts_bytes() {
        assert_eq!(find_from("abcabc", "bc", 0), Ok(Some(1)));
        assert_eq!(find_from("abcabc", "bc", 2), Ok(Some(4)));
        assert_eq!(find_from("abcabc", "x", 0), Ok(None));
        assert_eq!(find_from("abc", "", 3), Ok(Some(3)));
        assert_eq!(find_from("aéb", "b", 0), Ok(Some(3)));
    }

    #[test]
    fn find_refuses_a_bad_from() {
        assert!(find_from("abc", "a", 4).is_err());
        assert!(find_from("abc", "a", -1).is_err());
        assert!(find_from("aé", "a", 2).is_err());
    }
}
