//! The program's surroundings (syntax §4.3, M5): its command-line
//! arguments, and whole files read and written as strings. The
//! interpreter reaches them through the standard library; the compiled
//! runtime through the C library (`rt/io.lir`), with the same results.

use super::alloc::Placement;
use super::error::{RunError, R};
use super::interp::Interp;
use super::value::Val;

impl Interp<'_> {
    /// `(args)`, `(read-file path)`, `(write-file path text)`.
    pub fn io_builtin(&mut self, name: &str, a: &[Val]) -> R<Val> {
        let s = |i: usize| -> R<String> {
            let v = a
                .get(i)
                .ok_or_else(|| RunError::internal(format!("{name} without argument {i}")))?;
            Ok(self.string(v)?.to_string())
        };
        match name {
            "args" => {
                let args = self.args.clone();
                let mut vals = Vec::with_capacity(args.len());
                for arg in args {
                    vals.push(self.new_str(arg, Placement::Heap)?);
                }
                let v = self.build_vec(&vals, Placement::Heap)?;
                self.give_back(&vals)?;
                Ok(v)
            }
            "read-file" => match std::fs::read(s(0)?) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => {
                        let v = self.new_str(text, Placement::Heap)?;
                        Ok(Val::Some(Box::new(v)))
                    }
                    Err(_) => Ok(Val::None),
                },
                Err(_) => Ok(Val::None),
            },
            "write-file" => Ok(Val::Bool(std::fs::write(s(0)?, s(1)?).is_ok())),
            _ => Err(RunError::internal(format!("no io builtin {name}"))),
        }
    }
}
