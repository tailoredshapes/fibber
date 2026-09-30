//! The builtins of syntax §4.3 (types §2.9–§2.13, §8.3): cells,
//! arrays, strings, traps, the unsafe primitives.

use fibref::types::ast::Expr;
use fibref::types::ty::{Con, Ty};

use super::{Cx, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};
use crate::objects::{ELEMS, LEN};

impl<'a> Cx<'_, 'a> {
    /// A builtin call; `None` when it does not return (`trap`).
    pub fn builtin(&mut self, name: &str, e: &Expr, a: &[V], tys: &[Ty]) -> R<Option<V>> {
        let arg = |i: usize| -> R<&V> {
            a.get(i)
                .ok_or_else(|| Unsupported(format!("{name} without argument {i}")))
        };
        Ok(Some(match name {
            "cell" => self.new_slot(e, arg(0)?, false)?,
            "atom" => self.new_slot(e, arg(0)?, true)?,
            "set!" => {
                let (c, t) = (arg(0)?.clone(), tys[0].clone());
                self.set_cell(&c, &t, arg(1)?)?
            }
            "reset!" => {
                let (a, t) = (arg(0)?.clone(), tys[0].clone());
                self.reset(&a, &t, arg(1)?)?
            }
            "swap!" => {
                let (a, t) = (arg(0)?.clone(), tys[0].clone());
                self.swap(&a, &t, arg(1)?)?
            }
            "weak" => {
                let x = arg(0)?.clone();
                let (tid, _) = self.p.object(&self.ty(e)?)?;
                match x.ty() {
                    Some(LirTy::Ptr) => self.b.val(
                        &format!("(call @fib.weak {} (i32 {tid}))", x.text()),
                        LirTy::Ptr,
                    ),
                    // §8.7: the box of the object behind the `dyn`, with
                    // its vtable.
                    Some(LirTy::Dyn) => {
                        let obj = self
                            .b
                            .val(&format!("(extractvalue {} 0)", x.text()), LirTy::Ptr);
                        let vt = self
                            .b
                            .val(&format!("(extractvalue {} 1)", x.text()), LirTy::Ptr);
                        let bx = self.b.val(
                            &format!("(call @fib.weak {} (i32 {tid}))", obj.text()),
                            LirTy::Ptr,
                        );
                        self.b
                            .val(&format!("{{ {} {} }}", bx.text(), vt.text()), LirTy::Dyn)
                    }
                    _ => return Err(Unsupported("weak of a scalar".into())),
                }
            }
            "gensym" => {
                let hook = self.load(LirTy::Ptr, "@fibm.gensym-hook");
                let cx = self.load(LirTy::Ptr, "@fibm.hook-cx");
                self.b.val(
                    &format!(
                        "(indirect-call {} (fn ptr (ptr ptr)) {} {})",
                        hook.text(),
                        cx.text(),
                        arg(0)?.text()
                    ),
                    LirTy::Ptr,
                )
            }
            "struct?" | "struct-fields" | "struct-params" | "struct-field-types" | "enum?"
            | "enum-params" | "enum-variants" => {
                let hook = self.load(LirTy::Ptr, "@fibm.reflect-hook");
                let cx = self.load(LirTy::Ptr, "@fibm.hook-cx");
                let r = self.b.val(
                    &format!(
                        "(indirect-call {} (fn ptr (ptr ptr ptr)) {} (string \"{name}\") {})",
                        hook.text(),
                        cx.text(),
                        arg(0)?.text()
                    ),
                    LirTy::Ptr,
                );
                // A Bool answer is read out of its Form (eval/forms.rs
                // `reflection_value`); a Vec answer is the Form's items.
                if name == "struct?" || name == "enum?" {
                    let (_, sname) = self.p.object(&self.form_ty()?)?;
                    let f = self.gep(
                        &format!("{sname}.v6"),
                        r.text(),
                        crate::objects::VARIANT_FIELD0,
                    );
                    self.load(LirTy::I1, &f)
                } else {
                    let (_, sname) = self.p.object(&self.form_ty()?)?;
                    let f = self.gep(
                        &format!("{sname}.v9"),
                        r.text(),
                        crate::objects::VARIANT_FIELD0,
                    );
                    self.load(LirTy::Ptr, &f)
                }
            }
            "spawn" => self.spawn(e, arg(0)?)?,
            "join" => self.join(e, arg(0)?)?,
            "not" => self
                .b
                .val(&format!("(xor {} (i1 1))", arg(0)?.text()), LirTy::I1),
            "trap" => {
                self.b.stmt(&format!("(call @fib.trap {})", arg(0)?.text()));
                self.b.term("(unreachable)");
                return Ok(None);
            }
            "array" | "array-len" | "array-get" | "array-with" | "array-copy" | "array-set!" => {
                self.array_builtin(name, e, a, tys)?
            }
            "str-len" => self.rt_call("fib.str-len", a, Some(LirTy::I64)),
            "str-eq" => self.rt_call("fib.str-eq", a, Some(LirTy::I1)),
            "starts-with?" => self.rt_call("fib.str-starts-with", a, Some(LirTy::I1)),
            "str-concat" => self.rt_call("fib.str-concat", a, Some(LirTy::Ptr)),
            "str-slice" => self.rt_call("fib.str-slice", a, Some(LirTy::Ptr)),
            "str-bytes" => {
                let (tid, _) = self.p.object(&Ty::Con(
                    Con::Array,
                    vec![Ty::scalar(fibref::types::ty::Scalar::I8)],
                ))?;
                let v = vec![arg(0)?.clone(), V::int(LirTy::I32, i64::from(tid))];
                self.rt_call("fib.str-bytes", &v, Some(LirTy::Ptr))
            }
            "str-from-bytes" => self.rt_call("fib.str-from-array", a, Some(LirTy::Ptr)),
            "char->i32" => arg(0)?.clone(),
            "i32->char" => self.rt_call("fib.i32-to-char", a, Some(LirTy::I32)),
            "ptr+" => self.b.val(
                &format!("(getelementptr i8 {} {})", arg(0)?.text(), arg(1)?.text()),
                LirTy::Ptr,
            ),
            "load-i8" => self.load(LirTy::I8, arg(0)?.text()),
            "load-i16" => self.load(LirTy::I16, arg(0)?.text()),
            "load-i32" => self.load(LirTy::I32, arg(0)?.text()),
            "load-i64" => self.load(LirTy::I64, arg(0)?.text()),
            "load-ptr" => self.load(LirTy::Ptr, arg(0)?.text()),
            "store-i8" | "store-i16" | "store-i32" | "store-i64" | "store-ptr" => {
                let (p, v) = (arg(0)?.text().to_string(), arg(1)?.clone());
                self.store(&v, &p);
                V::Unit
            }
            "alloc" => self.rt_call("malloc", a, Some(LirTy::Ptr)),
            "free" => self.rt_call("free", a, None),
            "raw" | "raw-retained" => match arg(0)? {
                V::Val(s, LirTy::Ptr) => V::Val(s.clone(), LirTy::Ptr),
                v => self
                    .obj_word(v)
                    .map(|w| V::Val(w, LirTy::Ptr))
                    .ok_or_else(|| Unsupported("raw of a scalar".into()))?,
            },
            "release-raw" => self.rt_call("fib.release", a, None),
            other => return Err(Unsupported(format!("builtin {other}"))),
        }))
    }

    /// The prelude's `Form` type.
    fn form_ty(&self) -> R<Ty> {
        let id = self
            .p
            .g()
            .form
            .ok_or_else(|| Unsupported("the prelude defines no Form".into()))?;
        Ok(Ty::nominal(id, Vec::new()))
    }

    /// A call of a runtime function with these arguments.
    pub fn rt_call(&mut self, f: &str, a: &[V], ret: Option<LirTy>) -> V {
        let args: Vec<String> = a
            .iter()
            .filter(|v| !matches!(v, V::Unit))
            .map(|v| v.text().to_string())
            .collect();
        let instr = format!("(call @{f} {})", args.join(" "));
        match ret {
            Some(t) => self.b.val(&instr, t),
            None => {
                self.b.stmt(&instr);
                V::Unit
            }
        }
    }

    /// `(set! c v)` as the builtin: store the consumed value, release
    /// the old.
    fn set_cell(&mut self, c: &V, t: &Ty, v: &V) -> R<V> {
        let content = match t {
            Ty::Con(Con::Cell, args) => args[0].clone(),
            _ => return Err(Unsupported("set! on a non-cell".into())),
        };
        let (_, sname) = self.p.object(t)?;
        let old = self.cell_load(&sname, c.text(), &content)?;
        let field = self.gep(&sname, c.text(), crate::objects::CELL_VALUE);
        self.store(v, &field);
        self.release(&old);
        Ok(V::Unit)
    }

    fn array_builtin(&mut self, name: &str, e: &Expr, a: &[V], tys: &[Ty]) -> R<V> {
        let arr_ty = match name {
            "array" | "array-copy" | "array-with" => self.ty(e)?,
            "array-set!" => match &tys[0] {
                Ty::Con(Con::Cell, args) => args[0].clone(),
                _ => return Err(Unsupported("array-set! on a non-cell".into())),
            },
            _ => tys[0].clone(),
        };
        let elem_ty = match &arr_ty {
            Ty::Con(Con::Array, args) => args[0].clone(),
            _ => return Err(Unsupported(format!("{name} on a non-array"))),
        };
        let (tid, sname) = self.p.object(&arr_ty)?;
        let el = self
            .p
            .lir(&elem_ty)?
            .ok_or_else(|| Unsupported("arrays of unit".into()))?;
        let esize = crate::layout::size_align(el).0;
        let counted = matches!(el, LirTy::Ptr | LirTy::Dyn);
        match name {
            "array" => {
                let n = a[0].clone();
                let p = self.b.val(
                    &format!(
                        "(call @fib.array-alloc {} (i32 {tid}) (i64 {esize}))",
                        n.text()
                    ),
                    LirTy::Ptr,
                );
                self.array_fill(&sname, &p, &n, &a[1], counted);
                Ok(p)
            }
            "array-len" => {
                let lp = self.gep(&sname, a[0].text(), LEN);
                Ok(self.load(LirTy::I64, &lp))
            }
            "array-get" => {
                let ep = self.array_index(&sname, a[0].text(), &a[1])?;
                let v = self.load(el, &ep);
                self.retain(&v);
                Ok(v)
            }
            "array-with" => {
                let copy = self.array_dup(&sname, tid, esize, a[0].text());
                let ep = self.array_index(&sname, copy.text(), &a[1])?;
                let old = self.load(el, &ep);
                self.store(&a[2], &ep);
                self.release(&old);
                Ok(copy)
            }
            "array-copy" => {
                let (i, j) = (a[1].text().to_string(), a[2].text().to_string());
                let p = self.b.val(
                    &format!(
                        "(call @fib.array-slice {} {i} {j} (i32 {tid}) (i64 {esize}))",
                        a[0].text()
                    ),
                    LirTy::Ptr,
                );
                self.b
                    .stmt(&format!("(call @trace.{tid} {} @fib.retain)", p.text()));
                Ok(p)
            }
            _ => self.array_set(&tys[0], &sname, tid, esize, el, &elem_ty, a),
        }
    }

    /// Stores `x` into every element of a fresh array of `n`, taking
    /// `n - 1` further counts on it (the consume brought the first).
    fn array_fill(&mut self, sname: &str, p: &V, n: &V, x: &V, counted: bool) {
        let (lh, lb, ld) = (
            self.b.label("fill"),
            self.b.label("fillb"),
            self.b.label("filled"),
        );
        let entry = self.b.current();
        self.b.term(&format!("(br {lh})"));
        self.b.open(&lh);
        let i = self.b.phi(LirTy::I64, &[(entry, "(i64 0)".into())]);
        let c = self
            .b
            .val(&format!("(icmp slt {} {})", i.text(), n.text()), LirTy::I1);
        self.b.term(&format!("(br {} {lb} {ld})", c.text()));
        self.b.open(&lb);
        let ep = self.b.val(
            &format!(
                "(getelementptr %struct.{sname} {} (i32 0) (i32 {ELEMS}) {})",
                p.text(),
                i.text()
            ),
            LirTy::Ptr,
        );
        self.store(x, ep.text());
        if counted {
            let not_first = self
                .b
                .val(&format!("(icmp ne {} (i64 0))", i.text()), LirTy::I1);
            let (lr, ln) = (self.b.label("fillr"), self.b.label("filln"));
            self.b.term(&format!("(br {} {lr} {ln})", not_first.text()));
            self.b.open(&lr);
            self.retain(x);
            self.b.term(&format!("(br {ln})"));
            self.b.open(&ln);
        }
        let i2 = self
            .b
            .val(&format!("(add {} (i64 1))", i.text()), LirTy::I64);
        let back = self.b.current();
        self.b.term(&format!("(br {lh})"));
        // The phi's second entry: the increment from the loop's last block.
        self.b.patch_phi(&lh, i.text(), &back, i2.text());
        self.b.open(&ld);
    }

    /// The address of element `i`, after the bounds check (a trap with
    /// the interpreter's message, `eval/arrays.rs`).
    fn array_index(&mut self, sname: &str, arr: &str, i: &V) -> R<String> {
        let lp = self.gep(sname, arr, LEN);
        let n = self.load(LirTy::I64, &lp);
        let ok = self
            .b
            .val(&format!("(icmp ult {} {})", i.text(), n.text()), LirTy::I1);
        let (lo, lt) = (self.b.label("inb"), self.b.label("oob"));
        self.b.term(&format!("(br {} {lo} {lt})", ok.text()));
        self.b.open(&lt);
        self.b
            .stmt(&format!("(call @fib.trap-index {} {})", i.text(), n.text()));
        self.b.term("(unreachable)");
        self.b.open(&lo);
        Ok(self
            .b
            .val(
                &format!(
                    "(getelementptr %struct.{sname} {arr} (i32 0) (i32 {ELEMS}) {})",
                    i.text()
                ),
                LirTy::Ptr,
            )
            .text()
            .to_string())
    }

    /// A copy of an array with every element retained.
    fn array_dup(&mut self, sname: &str, tid: u32, esize: u64, arr: &str) -> V {
        let lp = self.gep(sname, arr, LEN);
        let n = self.load(LirTy::I64, &lp);
        let p = self.b.val(
            &format!(
                "(call @fib.array-slice {arr} (i64 0) {} (i32 {tid}) (i64 {esize}))",
                n.text()
            ),
            LirTy::Ptr,
        );
        self.b
            .stmt(&format!("(call @trace.{tid} {} @fib.retain)", p.text()));
        p
    }

    /// `(array-set! &a i x)`: in place when unique, else a changed
    /// copy stored into the cell (§8.3).
    #[allow(clippy::too_many_arguments)]
    fn array_set(
        &mut self,
        cell_ty: &Ty,
        sname: &str,
        tid: u32,
        esize: u64,
        el: LirTy,
        elem_ty: &Ty,
        a: &[V],
    ) -> R<V> {
        let (_, cell_s) = self.p.object(cell_ty)?;
        let cell = a[0].text().to_string();
        let arr_ty = Ty::Con(Con::Array, vec![elem_ty.clone()]);
        let content = self.cell_load(&cell_s, &cell, &arr_ty)?;
        let ep = self.array_index(sname, content.text(), &a[1])?;
        let unique = self.b.val(
            &format!("(call @fib.unique? {})", content.text()),
            LirTy::I1,
        );
        let (li, lc, lj) = (
            self.b.label("inplace"),
            self.b.label("copy"),
            self.b.label("written"),
        );
        self.b.term(&format!("(br {} {li} {lc})", unique.text()));
        self.b.open(&li);
        let old = self.load(el, &ep);
        self.store(&a[2], &ep);
        self.release(&old);
        self.b.term(&format!("(br {lj})"));
        self.b.open(&lc);
        let copy = self.array_dup(sname, tid, esize, content.text());
        let cep = self.b.val(
            &format!(
                "(getelementptr %struct.{sname} {} (i32 0) (i32 {ELEMS}) {})",
                copy.text(),
                a[1].text()
            ),
            LirTy::Ptr,
        );
        let old2 = self.load(el, cep.text());
        self.store(&a[2], cep.text());
        self.release(&old2);
        let cf = self.gep(&cell_s, &cell, crate::objects::CELL_VALUE);
        self.store(&copy, &cf);
        self.release(&content);
        self.b.term(&format!("(br {lj})"));
        self.b.open(&lj);
        Ok(V::Unit)
    }
}
