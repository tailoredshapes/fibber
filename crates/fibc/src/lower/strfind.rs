//! `(str-find s pat from)` (stdlib §7 L18): the runtime finds the byte
//! offset (`fib.str-find`, -1 for none) and this code makes the
//! `(Option i64)` from it. That `Option` is a heap enum, `nil` too (types
//! §8.1), so a call costs the one object the interpreter's call costs, as
//! `construct_option` would make it for `(some n)` or `nil`; the string
//! itself is never copied.

use fibref::types::ast::Expr;

use super::{Cx, Flow, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};
use crate::objects::{ENUM_TAG, VARIANT_FIELD0};

impl<'a> Cx<'_, 'a> {
    /// The `(Option i64)` of `(str-find ..)` with arguments `a`.
    pub fn str_find(&mut self, e: &Expr, a: &[V]) -> R<V> {
        let at = self.rt_call("fib.str-find", a, Some(LirTy::I64));
        let t = self.ty(e)?;
        let (tid, sname) = self.p.object(&t)?;
        let size = self.p.objects.get(tid).size();
        let found = self
            .b
            .val(&format!("(icmp sge {} (i64 0))", at.text()), LirTy::I1);
        let (lsome, lnone, lj) = (
            self.b.label("found"),
            self.b.label("missing"),
            self.b.label("fjoin"),
        );
        self.b
            .term(&format!("(br {} {lsome} {lnone})", found.text()));
        let mut incoming = Vec::new();
        for (label, tag) in [(lsome, 1), (lnone, 0)] {
            self.b.open(&label);
            let variant = format!("{sname}.v{tag}");
            let p = self.heap_alloc(tid, size);
            let tagp = self.gep(&variant, &p, ENUM_TAG);
            self.b.stmt(&format!("(store (i32 {tag}) {tagp})"));
            if tag == 1 {
                self.store_fields(&variant, &p, VARIANT_FIELD0, std::slice::from_ref(&at));
            }
            incoming.push((self.b.current(), V::Val(p, LirTy::Ptr)));
            self.b.term(&format!("(br {lj})"));
        }
        match self.join_branches(&lj, Some(LirTy::Ptr), incoming)? {
            Flow::Val(v) => Ok(v),
            Flow::Jump => Err(Unsupported("str-find without a result".into())),
        }
    }
}
