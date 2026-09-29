//! Threads (types §8.8, §8.10 E4): `spawn` share-marks the consumed
//! closure, allocates the result atom and the task (count 2: the
//! handle and the thread) and hands both to a new OS thread whose
//! entry, generated per result type, calls the closure, publishes the
//! result and releases the task; `join` waits and retains the result.

use std::fmt::Write;

use fibref::types::ast::Expr;
use fibref::types::ty::{Con, Ty};

use super::{Cx, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};
use crate::names::mangle;
use crate::objects::{
    ATOM_LOCK, ATOM_VALUE, TASK_CAPTURE0, TASK_CLOSURE, TASK_RESULT, TASK_RESUME, TASK_STATE,
};
use fibref::own::program::Pass;

impl<'a> Cx<'_, 'a> {
    /// `(spawn f)`: the task handle.
    pub fn spawn(&mut self, e: &Expr, f: &V) -> R<V> {
        let task_ty = self.ty(e)?;
        let result_ty = match &task_ty {
            Ty::Con(Con::Task, args) => args[0].clone(),
            _ => return Err(Unsupported("spawn without a Task type".into())),
        };
        self.b.stmt(&format!("(call @fib.share {})", f.text()));
        let atom_ty = Ty::Con(Con::Atom, vec![result_ty.clone()]);
        let (atid, asname) = self.p.object(&atom_ty)?;
        let asize = self.p.objects.get(atid).size();
        let atom = self.b.val(
            &format!("(call @fib.alloc (i64 {asize}) (i32 {atid}))"),
            LirTy::Ptr,
        );
        let lockp = self.gep(&asname, atom.text(), ATOM_LOCK);
        self.b.stmt(&format!("(store (i32 0) {lockp})"));
        let vp = self.gep(&asname, atom.text(), ATOM_VALUE);
        if let Some(l) = self.p.lir(&result_ty)? {
            let zero = if l == LirTy::Ptr {
                "(ptr null)".to_string()
            } else if l == LirTy::Dyn {
                "(zeroinitializer { ptr ptr })".to_string()
            } else if matches!(l, LirTy::Float | LirTy::Double) {
                format!("({} 0.0)", l.text())
            } else {
                format!("({} 0)", l.text())
            };
            self.b.stmt(&format!("(store {zero} {vp})"));
        }
        let (ttid, tsname) = self.p.object(&task_ty)?;
        let tsize = self.p.objects.get(ttid).size();
        let task = self.b.val(
            &format!("(call @fib.alloc (i64 {tsize}) (i32 {ttid}))"),
            LirTy::Ptr,
        );
        self.b.stmt(&format!("(call @fib.retain {})", task.text()));
        for (i, v) in [
            (TASK_STATE, "(i32 1)"),
            (TASK_STATE + 1, "(i32 0)"),
            (TASK_STATE + 2, "(i32 0)"),
        ] {
            let p = self.gep(&tsname, task.text(), i);
            self.b.stmt(&format!("(store {v} {p})"));
        }
        for i in [TASK_STATE + 3, TASK_RESULT + 1] {
            let p = self.gep(&tsname, task.text(), i);
            self.b.stmt(&format!("(store (ptr null) {p})"));
        }
        let rp = self.gep(&tsname, task.text(), TASK_RESULT);
        self.store(&atom, &rp);
        let cp = self.gep(&tsname, task.text(), TASK_CLOSURE);
        self.store(f, &cp);
        // The task and its result atom cross to the new thread: SHARED
        // before the handoff (§8.8), so both threads count atomically.
        self.b.stmt(&format!("(call @fib.share {})", task.text()));
        let entry = self.thread_entry(&result_ty, &asname, &tsname)?;
        self.b
            .stmt(&format!("(call @fib.spawn {} @{entry})", task.text()));
        Ok(task)
    }

    /// A fresh result atom of `result_ty`, its slot zeroed.
    fn result_atom(&mut self, result_ty: &Ty) -> R<(V, String)> {
        let atom_ty = Ty::Con(Con::Atom, vec![result_ty.clone()]);
        let (atid, asname) = self.p.object(&atom_ty)?;
        let asize = self.p.objects.get(atid).size();
        let atom = self.b.val(
            &format!("(call @fib.alloc (i64 {asize}) (i32 {atid}))"),
            LirTy::Ptr,
        );
        let lockp = self.gep(&asname, atom.text(), ATOM_LOCK);
        self.b.stmt(&format!("(store (i32 0) {lockp})"));
        let vp = self.gep(&asname, atom.text(), ATOM_VALUE);
        if let Some(l) = self.p.lir(result_ty)? {
            let zero = match l {
                LirTy::Ptr => "(ptr null)".to_string(),
                LirTy::Dyn => "(zeroinitializer { ptr ptr })".to_string(),
                LirTy::Float | LirTy::Double => format!("({} 0.0)", l.text()),
                _ => format!("({} 0)", l.text()),
            };
            self.b.stmt(&format!("(store {zero} {vp})"));
        }
        Ok((atom, asname))
    }

    /// `(async body)`: a pending task holding the body's captures
    /// (E3, retained) and its resume function; driven at the first
    /// `join` or `await` (compiler.md §8 question 3).
    pub fn make_async(&mut self, e: &Expr) -> R<V> {
        let own = self
            .own
            .closures
            .get(&e.id)
            .cloned()
            .ok_or_else(|| Unsupported("no plan for an async literal".into()))?;
        let task_ty = self.ty(e)?;
        let result_ty = match &task_ty {
            Ty::Con(Con::Task, args) => args[0].clone(),
            _ => return Err(Unsupported("async without a Task type".into())),
        };
        let caps = self.capture_tys(&own)?;
        let code = self
            .p
            .request_closure(&self.inst.clone(), &self.name.clone(), e.id);
        let (ttid, tsname) = self.p.task_object(&code, caps);
        let tsize = self.p.objects.get(ttid).size();
        let mut vals = Vec::new();
        for c in &own.captures {
            let v = self.local(c.binding)?;
            if c.pass == Pass::Retain {
                self.retain(&v);
            }
            vals.push(v);
        }
        let (atom, asname) = self.result_atom(&result_ty)?;
        let task = self.b.val(
            &format!("(call @fib.alloc (i64 {tsize}) (i32 {ttid}))"),
            LirTy::Ptr,
        );
        for (i, v) in [
            (TASK_STATE, "(i32 0)"),
            (TASK_STATE + 1, "(i32 0)"),
            (TASK_STATE + 2, "(i32 0)"),
        ] {
            let p = self.gep(&tsname, task.text(), i);
            self.b.stmt(&format!("(store {v} {p})"));
        }
        for i in [TASK_RESULT + 1, TASK_CLOSURE] {
            let p = self.gep(&tsname, task.text(), i);
            self.b.stmt(&format!("(store (ptr null) {p})"));
        }
        let rp = self.gep(&tsname, task.text(), TASK_RESULT);
        self.store(&atom, &rp);
        let resume = self.resume_entry(&code, &result_ty, &asname, &tsname)?;
        let rsp = self.gep(&tsname, task.text(), TASK_RESUME);
        self.b.stmt(&format!("(store @{resume} {rsp})"));
        self.store_fields(&tsname, task.text(), TASK_CAPTURE0, &vals);
        Ok(task)
    }

    /// `(await e)`: drive or wait for the task, then its result.
    pub fn await_task(&mut self, x: &'a Expr) -> R<V> {
        let t = self.value(x)?;
        let result_ty = match self.ty(x)? {
            Ty::Con(Con::Task, args) => args[0].clone(),
            _ => return Err(Unsupported("await of a non-task".into())),
        };
        self.task_result(&t, &result_ty)
    }

    /// `(join t)`: the result, retained, once the task is done.
    pub fn join(&mut self, e: &Expr, t: &V) -> R<V> {
        let result_ty = self.ty(e)?;
        self.task_result(t, &result_ty)
    }

    /// Drives a pending task on this thread (claiming its driver),
    /// waits for a running one, then retains and returns its result.
    fn task_result(&mut self, t: &V, result_ty: &Ty) -> R<V> {
        let atom_ty = Ty::Con(Con::Atom, vec![result_ty.clone()]);
        let (_, asname) = self.p.object(&atom_ty)?;
        let (_, tsname) = self
            .p
            .object(&Ty::Con(Con::Task, vec![result_ty.clone()]))?;
        self.b.stmt(&format!("(call @fib.drive {})", t.text()));
        let _ = (&asname, &tsname);
        let atom_ty = Ty::Con(Con::Atom, vec![result_ty.clone()]);
        let (_, asname) = self.p.object(&atom_ty)?;
        let (_, tsname) = self
            .p
            .object(&Ty::Con(Con::Task, vec![result_ty.clone()]))?;
        let rp = self.gep(&tsname, t.text(), TASK_RESULT);
        let atom = self.load(LirTy::Ptr, &rp);
        let vp = self.gep(&asname, atom.text(), ATOM_VALUE);
        let v = match self.p.lir(result_ty)? {
            Some(l) => self.load(l, &vp),
            None => V::Unit,
        };
        self.retain(&v);
        Ok(v)
    }

    /// The thread entry for tasks of this result type, emitted once:
    /// calls the spawned closure, completes the task, releases it.
    fn thread_entry(&mut self, result_ty: &Ty, asname: &str, tsname: &str) -> R<String> {
        let g = self.p.g();
        let name = format!("fib.thread.{}", mangle(g, result_ty));
        if self.p.has_helper(&name) {
            return Ok(name);
        }
        let head = format!(
            "(define internal ({name} ptr) ((ptr task))\n  (block entry\n    (let ((f (load ptr (getelementptr %struct.{tsname} task (i32 0) (i32 {TASK_CLOSURE}))))\n          (code (load ptr (getelementptr %struct.fib.closure f (i32 0) (i32 3))))"
        );
        let call = |r: &str| format!("(indirect-call code (fn tailcc {r} (ptr)) f)");
        let tail = "      (call @fib.release task)\n      (ret (ptr null)))))\n";
        let s = self.completion(&head, &call, tail, result_ty, asname, tsname)?;
        self.p.add_helper(&name, s);
        Ok(name)
    }

    /// The resume function of an `async` literal: runs the body with
    /// the task as `env` and completes the task.
    fn resume_entry(
        &mut self,
        code: &str,
        result_ty: &Ty,
        asname: &str,
        tsname: &str,
    ) -> R<String> {
        let name = format!("fib.resume.{code}");
        if self.p.has_helper(&name) {
            return Ok(name);
        }
        let head =
            format!("(define internal ({name} void) ((ptr task))\n  (block entry\n    (let (");
        let call = |_: &str| format!("(call @{code} task)");
        let s = self.completion(&head, &call, "      (ret))))\n", result_ty, asname, tsname)?;
        self.p.add_helper(&name, s);
        Ok(name)
    }

    /// The completion of a task (§8.8, as the interpreter's `complete`):
    /// the body's value share-marked, stored into the result atom under
    /// its lock (retained; the old content released), released as the
    /// step's temporary, and the state set to done. `call` gives the
    /// call of the body for the lIR text of its result type.
    fn completion(
        &mut self,
        head: &str,
        call: &dyn Fn(&str) -> String,
        tail: &str,
        result_ty: &Ty,
        asname: &str,
        tsname: &str,
    ) -> R<String> {
        let l = self.p.lir(result_ty)?;
        let mut s = head.to_string();
        let _ = writeln!(
            s,
            "\n          (atom (load ptr (getelementptr %struct.{tsname} task (i32 0) (i32 {TASK_RESULT}))))\n          (lockp (getelementptr %struct.{asname} atom (i32 0) (i32 {ATOM_LOCK})))\n          (vp (getelementptr %struct.{asname} atom (i32 0) (i32 {ATOM_VALUE}))))"
        );
        match l {
            None => {
                let _ = writeln!(
                    s,
                    "      {}\n      (call @fib.lock lockp)\n      (call @fib.unlock lockp)",
                    call("void")
                );
            }
            Some(t) => {
                let _ = writeln!(s, "      (let ((v {}))", call(t.text()));
                let word = match t {
                    LirTy::Ptr => Some("v".to_string()),
                    LirTy::Dyn => Some("(extractvalue v 0)".to_string()),
                    _ => None,
                };
                if let Some(w) = &word {
                    let _ = writeln!(s, "        (call @fib.share {w})");
                }
                let _ = writeln!(
                    s,
                    "        (call @fib.lock lockp)\n        (let ((old (load {} vp)))\n          (store v vp)",
                    t.text()
                );
                if let Some(w) = &word {
                    let ow = if t == LirTy::Dyn {
                        "(extractvalue old 0)"
                    } else {
                        "old"
                    };
                    let _ = writeln!(
                        s,
                        "          (call @fib.retain {w})\n          (call @fib.unlock lockp)\n          (call @fib.release {ow})\n          (call @fib.release {w})))"
                    );
                } else {
                    s.push_str("          (call @fib.unlock lockp)))\n");
                }
            }
        }
        s.push_str("      (call @fib.task-complete task)\n");
        s.push_str(tail);
        Ok(s)
    }
}
