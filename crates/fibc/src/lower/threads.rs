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

/// The frame of the `async` body being lowered: its task struct, the
/// index of the resume point, and the continuation block of each
/// state so far (state `k` is `states[k-1]`).
#[derive(Clone, Debug)]
pub struct AsyncFrame {
    pub tsname: String,
    pub point: usize,
    pub states: Vec<String>,
}

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
            let zero = if matches!(l, LirTy::Ptr | LirTy::Raw) {
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
        // Running, and its driver held for good by the thread (§8.8:
        // a joiner never resumes a spawned task, it waits).
        for (i, v) in [
            (TASK_STATE, "(i32 1)"),
            (TASK_STATE + 1, "(i32 1)"),
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
        let entry = self.thread_entry(&result_ty, &tsname)?;
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
                LirTy::Ptr | LirTy::Raw => "(ptr null)".to_string(),
                LirTy::Dyn => "(zeroinitializer { ptr ptr })".to_string(),
                LirTy::Float | LirTy::Double => format!("({} 0.0)", l.text()),
                _ => format!("({} 0)", l.text()),
            };
            self.b.stmt(&format!("(store {zero} {vp})"));
        }
        Ok((atom, asname))
    }

    /// `(async body)`: a pending task holding the body's captures
    /// (E3, retained), its resume point 0 and its resume function, the
    /// body's state machine (`resume.rs`); run when joined or awaited
    /// (syntax §3.12, types §8.8).
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
        let ncaps = caps.len();
        let code = self
            .p
            .request_closure(&self.inst.clone(), &self.name.clone(), e.id);
        let (ttid, tsname) = self.p.task_object(&code, caps);
        let mut vals = Vec::new();
        for c in &own.captures {
            let v = self.local(c.binding)?;
            if c.pass == Pass::Retain {
                self.retain(&v);
            }
            vals.push(v);
        }
        let (atom, _) = self.result_atom(&result_ty)?;
        // The frame's size is the type table's: the body's slots are
        // known only once it is lowered.
        let task = self.b.val(
            &format!("(call @fib.alloc (call @fib.size-of (i32 {ttid})) (i32 {ttid}))"),
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
        let rsp = self.gep(&tsname, task.text(), TASK_RESUME);
        self.b.stmt(&format!("(store @{code} {rsp})"));
        self.store_fields(&tsname, task.text(), TASK_CAPTURE0, &vals);
        let pp = self.gep(&tsname, task.text(), TASK_CAPTURE0 + ncaps);
        self.b.stmt(&format!("(store (i32 0) {pp})"));
        Ok(task)
    }

    /// `(await e)` in an `async` body (§8.8): the next state's point
    /// is stored, then `fib.await-or-park` either finds `e` done or
    /// registers this task as its waiter, in which case the resume
    /// returns here and continues, in a later resume, at the state's
    /// block, where `e`'s result is read.
    pub fn await_task(&mut self, x: &'a Expr) -> R<V> {
        let t = self.value(x)?;
        let result_ty = match self.ty(x)? {
            Ty::Con(Con::Task, args) => args[0].clone(),
            _ => return Err(Unsupported("await of a non-task".into())),
        };
        let Some(frame) = self.async_frame.clone() else {
            return Err(Unsupported("await outside an async body".into()));
        };
        let k = frame.states.len() + 1;
        let pp = self.gep(&frame.tsname, "env", frame.point);
        self.b.stmt(&format!("(store (i32 {k}) {pp})"));
        let parked = self.b.val(
            &format!("(call @fib.await-or-park env {})", t.text()),
            LirTy::I1,
        );
        let (lpark, lcont) = (self.b.label("park"), self.b.label("resume"));
        self.b
            .term(&format!("(br {} {lpark} {lcont})", parked.text()));
        self.b.open(&lpark);
        self.b.term("(ret)");
        self.b.open(&lcont);
        if let Some(f) = self.async_frame.as_mut() {
            f.states.push(lcont);
        }
        self.task_value(&t, &result_ty)
    }

    /// `(join t)`: drives the executor until the task is done (§8.8),
    /// then its result, retained.
    pub fn join(&mut self, e: &Expr, t: &V) -> R<V> {
        let result_ty = self.ty(e)?;
        self.join_value(t, &result_ty)
    }

    /// `(join t)` of a task whose result has type `result_ty`; also
    /// `@t` (types §2.9, the `Deref` instance of `(Task a)`).
    pub fn join_value(&mut self, t: &V, result_ty: &Ty) -> R<V> {
        self.b.stmt(&format!("(call @fib.drive {})", t.text()));
        self.task_value(t, result_ty)
    }

    /// The result of a task that is done, retained (under its atom's
    /// lock, as any atom read).
    fn task_value(&mut self, t: &V, result_ty: &Ty) -> R<V> {
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
    fn thread_entry(&mut self, result_ty: &Ty, tsname: &str) -> R<String> {
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
        let s = self.completion(&head, &call, true, tail, result_ty)?;
        self.p.add_helper(&name, s);
        Ok(name)
    }

    /// The completion function of an `async` body's task: called by
    /// the state machine with the body's value in place of its `ret`
    /// (`resume.rs`).
    pub fn complete_helper(&mut self, code: &str, result_ty: &Ty) -> R<String> {
        let name = format!("fib.complete.{code}");
        if self.p.has_helper(&name) {
            return Ok(name);
        }
        let param = match self.p.lir(result_ty)? {
            Some(t) => format!(" ({} v0)", t.text()),
            None => String::new(),
        };
        let head = format!(
            "(define internal ({name} void) ((ptr task){param})\n  (block entry\n    (let ("
        );
        let call = |_: &str| "v0".to_string();
        let s = self.completion(
            &head,
            &call,
            false,
            "      (ret))))
",
            result_ty,
        )?;
        self.p.add_helper(&name, s);
        Ok(name)
    }

    /// The completion of a task (§8.8, as the interpreter's `complete`):
    /// the body's value share-marked, stored into the result atom under
    /// its lock (retained; the old content released), released as the
    /// step's temporary, and the state set to done, which wakes the
    /// waiters. `call` gives, for the lIR text of the result type, the
    /// instruction producing the value when `bind` holds, else the name
    /// it is already bound to.
    fn completion(
        &mut self,
        head: &str,
        call: &dyn Fn(&str) -> String,
        bind: bool,
        tail: &str,
        result_ty: &Ty,
    ) -> R<String> {
        let l = self.p.lir(result_ty)?;
        let (_, asname) = self
            .p
            .object(&Ty::Con(Con::Atom, vec![result_ty.clone()]))?;
        let (_, tsname) = self
            .p
            .object(&Ty::Con(Con::Task, vec![result_ty.clone()]))?;
        let mut s = head.to_string();
        let _ = writeln!(
            s,
            "\n          (atom (load ptr (getelementptr %struct.{tsname} task (i32 0) (i32 {TASK_RESULT}))))\n          (lockp (getelementptr %struct.{asname} atom (i32 0) (i32 {ATOM_LOCK})))\n          (vp (getelementptr %struct.{asname} atom (i32 0) (i32 {ATOM_VALUE}))))"
        );
        let close = if bind { "))" } else { ")" };
        match l {
            None => {
                if bind {
                    let _ = writeln!(s, "      {}", call("void"));
                }
                let _ = writeln!(
                    s,
                    "      (call @fib.lock lockp)\n      (call @fib.unlock lockp)"
                );
            }
            Some(t) => {
                let v = if bind {
                    let _ = writeln!(s, "      (let ((v {}))", call(t.text()));
                    "v".to_string()
                } else {
                    call(t.text())
                };
                let word = match t {
                    LirTy::Ptr => Some(v.clone()),
                    LirTy::Dyn => Some(format!("(extractvalue {v} 0)")),
                    _ => None,
                };
                if let Some(w) = &word {
                    let _ = writeln!(s, "        (call @fib.share {w})");
                }
                let _ = writeln!(
                    s,
                    "        (call @fib.lock lockp)\n        (let ((old (load {} vp)))\n          (store {v} vp)",
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
                        "          (call @fib.retain {w})\n          (call @fib.unlock lockp)\n          (call @fib.release {ow})\n          (call @fib.release {w}){close}"
                    );
                } else {
                    let _ = writeln!(s, "          (call @fib.unlock lockp){close}");
                }
            }
        }
        s.push_str("      (call @fib.task-complete task)\n");
        s.push_str(tail);
        Ok(s)
    }
}
