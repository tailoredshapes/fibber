//! The native `Show` and `Hash` instances (types §2.12) on scalars,
//! field-less enums and `str`, computing what the interpreter computes
//! (eval/arith.rs `show`, `hash`, types §2.12); every `show` allocates
//! one fresh `str`.

use fibref::types::decls::Shape;
use fibref::types::ty::{Con, Scalar, Ty, TypeId};

use super::{Cx, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};
use crate::names::mangle;

impl<'a> Cx<'_, 'a> {
    pub fn native_show(&mut self, x: &V, t: &Ty) -> R<V> {
        let g = self.p.g();
        let call = match t {
            Ty::Con(Con::Scalar(s), _) => match s {
                Scalar::Bool => format!("(call @fib.show-bool {})", x.text()),
                Scalar::Char => format!("(call @fib.show-char {})", x.text()),
                Scalar::I64 => format!("(call @fib.show-int {})", x.text()),
                Scalar::I8 | Scalar::I16 | Scalar::I32 => {
                    let w = self.b.val(&format!("(sext i64 {})", x.text()), LirTy::I64);
                    format!("(call @fib.show-int {})", w.text())
                }
                Scalar::Ptr => format!("(call @fib.show-ptr {})", x.text()),
                Scalar::Unit => {
                    let s = self.p.statics.string("()", 0);
                    format!("(call @fib.str-copy {s})")
                }
                Scalar::Keyword => {
                    self.p.keyword_helpers = true;
                    format!("(call @kw.show {})", x.text())
                }
                Scalar::F32 => format!("(call @fib.show-float {})", x.text()),
                Scalar::F64 => format!("(call @fib.show-double {})", x.text()),
            },
            Ty::Con(Con::Nominal(id), _) if g.ty(*id).is_fieldless_enum() => {
                let f = self.enum_show_helper(*id)?;
                format!("(call @{f} {})", x.text())
            }
            Ty::Con(Con::Str, _) => format!("(call @fib.str-copy {})", x.text()),
            _ => return Err(Unsupported(format!("show of {}", mangle(g, t)))),
        };
        Ok(self.b.val(&call, LirTy::Ptr))
    }

    pub fn native_hash(&mut self, x: &V, t: &Ty) -> R<V> {
        let g = self.p.g();
        let conv = |cx: &mut Self, instr: &str| cx.b.val(instr, LirTy::I64);
        Ok(match t {
            Ty::Con(Con::Scalar(s), _) => match s {
                Scalar::I64 => x.clone(),
                Scalar::I8 | Scalar::I16 | Scalar::I32 => {
                    conv(self, &format!("(sext i64 {})", x.text()))
                }
                Scalar::Bool | Scalar::Char => conv(self, &format!("(zext i64 {})", x.text())),
                Scalar::F64 => conv(self, &format!("(call @fib.double-hash {})", x.text())),
                Scalar::F32 => {
                    let d = self
                        .b
                        .val(&format!("(fpext double {})", x.text()), LirTy::Double);
                    conv(self, &format!("(call @fib.double-hash {})", d.text()))
                }
                Scalar::Ptr => conv(self, &format!("(ptrtoint i64 {})", x.text())),
                Scalar::Unit => V::int(LirTy::I64, 0),
                Scalar::Keyword => {
                    self.p.keyword_helpers = true;
                    conv(self, &format!("(call @kw.hash {})", x.text()))
                }
            },
            Ty::Con(Con::Nominal(id), _) if g.ty(*id).is_fieldless_enum() => {
                conv(self, &format!("(sext i64 {})", x.text()))
            }
            Ty::Con(Con::Str, _) => conv(self, &format!("(call @fib.str-hash {})", x.text())),
            _ => return Err(Unsupported(format!("hash of {}", mangle(g, t)))),
        })
    }

    /// `show` on a field-less enum: a fresh copy of the variant's name.
    fn enum_show_helper(&mut self, id: TypeId) -> R<String> {
        let g = self.p.g();
        let name = format!("show.enum.{}", id.0);
        if self.p.has_helper(&name) {
            return Ok(name);
        }
        let Shape::Enum(vs) = &g.ty(id).shape else {
            return Err(Unsupported("show of a struct".into()));
        };
        let mut cases = Vec::new();
        let mut blocks = String::new();
        for (i, v) in vs.iter().enumerate() {
            let s = self.p.statics.string(&v.name, 0);
            cases.push(format!("((i32 {i}) v{i})"));
            blocks.push_str(&format!("  (block v{i} (ret (call @fib.str-copy {s})))\n"));
        }
        let text = format!(
            "(define internal ({name} ptr) ((i32 i))\n  (block entry (switch i bad {}))\n{blocks}  (block bad (unreachable)))\n",
            cases.join(" ")
        );
        self.p.add_helper(&name, text);
        Ok(name)
    }
}
