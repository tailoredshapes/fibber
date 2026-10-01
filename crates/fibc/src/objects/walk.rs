//! The `drop` and `trace` functions of the object types (types §8.2,
//! §8.3): which slots of an object are children, and a function that
//! acts on each. A slot is a child when its lIR type says it is a counted
//! pointer (`ptr`, an object's) or a `dyn`; a raw `ptr` (`LirTy::Raw`,
//! types §8.1) is an address with no count and is never one.
//!
//! A `drop` does not release its children: it queues them on the
//! worklist of the drop in progress (`fib.defer`, rt/core.lir), last
//! child first, so that taking them off in turn releases them in field
//! order, each with its whole subtree before the next, as the recursion
//! did, without a native stack frame per object of a chain. A `share`
//! queues them on the worklist of a share-marking (`fib.share-queue`,
//! rt/atom.lir) for the same reason. A `trace` is the one-level walk
//! that calls a callback on each child (the copies of a unique write
//! retain the children of the copied object with it).

use std::fmt::Write;

use super::{
    ObjInfo, ObjKind, ATOM_VALUE, CAPTURE0, CELL_VALUE, ELEMS, ENUM_TAG, LEN, STRUCT_FIELD0,
    TASK_CAPTURE0, TASK_RESULT, TASK_WAITERS, VARIANT_FIELD0,
};
use crate::ir::LirTy;

/// The child pointers of an object: `(struct name, field index)` for
/// a pointer or `dyn` field, per variant for an enum.
fn child_fields(o: &ObjInfo) -> Vec<(String, Vec<(usize, LirTy)>)> {
    let counted = |fs: &[Option<LirTy>], base: usize| -> Vec<(usize, LirTy)> {
        fs.iter()
            .scan(base, |i, f| {
                let here = *i;
                if f.is_some() {
                    *i += 1;
                }
                Some((here, *f))
            })
            .filter_map(|(i, f)| match f {
                Some(t @ (LirTy::Ptr | LirTy::Dyn)) => Some((i, t)),
                _ => None,
            })
            .collect()
    };
    match &o.kind {
        ObjKind::Struct(fs) => vec![(o.sname.clone(), counted(fs, STRUCT_FIELD0))],
        ObjKind::Enum(vs) => vs
            .iter()
            .enumerate()
            .map(|(i, v)| (o.variant_sname(i), counted(v, VARIANT_FIELD0)))
            .collect(),
        ObjKind::Cell(LirTy::Ptr | LirTy::Dyn) => {
            vec![(o.sname.clone(), vec![(CELL_VALUE, LirTy::Ptr)])]
        }
        ObjKind::Atom(LirTy::Ptr | LirTy::Dyn) => {
            vec![(o.sname.clone(), vec![(ATOM_VALUE, LirTy::Ptr)])]
        }
        ObjKind::Closure(caps) => {
            let fs: Vec<Option<LirTy>> = caps.iter().map(|c| Some(*c)).collect();
            vec![(o.sname.clone(), counted(&fs, CAPTURE0))]
        }
        // The captures first, then the result, as the interpreter's task
        // holds its fields (eval/task.rs: captures, then the result atom).
        ObjKind::Task { caps, .. } => {
            let fs: Vec<Option<LirTy>> = caps.iter().map(|c| Some(*c)).collect();
            let mut fields = counted(&fs, TASK_CAPTURE0);
            fields.push((TASK_RESULT, LirTy::Ptr));
            fields.push((TASK_WAITERS, LirTy::Ptr));
            vec![(o.sname.clone(), fields)]
        }
        _ => vec![(o.sname.clone(), Vec::new())],
    }
}

/// A `drop.N`, `trace.N` or `share.N` function: `act` on each child
/// pointer of the object (an enum's by its tag through `switch`; an
/// array's elements in a loop). A drop goes through the fields in
/// reverse, for the worklist (module comment).
pub(super) fn walker(o: &ObjInfo, what: &str, act: impl Fn(&str) -> String) -> String {
    let drop = what == "drop";
    let params = if what == "trace" {
        "((ptr p) (ptr cb))"
    } else {
        "((ptr p) (ptr wl))"
    };
    let mut s = format!("(define internal ({what}.{} void) {params}\n", o.tid);
    if let ObjKind::Array(LirTy::Ptr | LirTy::Dyn) = o.kind {
        return array_walker(s, o, drop, &act);
    }
    if let (ObjKind::Weak, true) = (&o.kind, drop) {
        s.push_str("  (block entry (call @fib.weak-drop p) (ret)))\n");
        return s;
    }
    let mut groups = child_fields(o);
    if drop {
        groups.iter_mut().for_each(|(_, fields)| fields.reverse());
    }
    if let ObjKind::Enum(_) = o.kind {
        let cases: Vec<String> = (0..groups.len())
            .map(|i| format!("((i32 {i}) v{i})"))
            .collect();
        let _ = writeln!(
            s,
            "  (block entry (switch (load i32 (getelementptr %struct.{} p (i32 0) (i32 {ENUM_TAG}))) done {}))",
            o.sname,
            cases.join(" ")
        );
        for (i, (sn, fields)) in groups.iter().enumerate() {
            let _ = writeln!(s, "  (block v{i} {} (br done))", releases(sn, fields, &act));
        }
        s.push_str("  (block done (ret)))\n");
    } else {
        let (sn, fields) = &groups[0];
        let _ = writeln!(s, "  (block entry {} (ret)))", releases(sn, fields, &act));
    }
    s
}

/// The walker of an array of counted elements: a loop up, or down for
/// a drop.
fn array_walker(mut s: String, o: &ObjInfo, down: bool, act: &impl Fn(&str) -> String) -> String {
    let sn = &o.sname;
    let n = format!("(load i64 (getelementptr %struct.{sn} p (i32 0) (i32 {LEN})))");
    let (start, more, step) = if down {
        ("last", "(icmp sge i (i64 0))", "(sub i (i64 1))")
    } else {
        ("(i64 0)", "(icmp slt i n)", "(add i (i64 1))")
    };
    let _ = write!(
        s,
        "  (block entry (let ((n {n}) (last (sub n (i64 1)))) (br loop)))
  (block loop (let ((i (phi i64 (entry {start}) (next i2)))) (br {more} body done)))
  (block body (let ((e (load ptr (getelementptr %struct.{sn} p (i32 0) (i32 {ELEMS}) i)))) {} (br next)))
  (block next (let ((i2 {step})) (br loop)))
  (block done (ret)))\n",
        act("e")
    );
    s
}

fn releases(sn: &str, fields: &[(usize, LirTy)], act: &impl Fn(&str) -> String) -> String {
    fields
        .iter()
        .map(|(i, _)| {
            // A `dyn` slot's first word is its object, as a `ptr` slot is.
            let gep = format!("(getelementptr %struct.{sn} p (i32 0) (i32 {i}))");
            act(&format!("(load ptr {gep})"))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::super::Objects;
    use super::*;

    /// The text of the lIR function `name` in `src`, up to the next one.
    fn function<'s>(src: &'s str, name: &str) -> &'s str {
        let start = src
            .find(&format!("(define internal ({name} "))
            .unwrap_or_else(|| panic!("no {name} in\n{src}"));
        let rest = &src[start + 1..];
        &src[start..start + 1 + rest.find("(define ").unwrap_or(rest.len())]
    }

    /// A raw `ptr` is an address with no count (types §8.1): the drop
    /// and the trace of an object that holds one leave it alone, and
    /// those of the same object holding an object pointer do not.
    #[test]
    fn a_raw_pointer_slot_is_not_released_or_traced_and_an_object_pointer_is() {
        let kinds = |t: LirTy| {
            vec![
                ObjKind::Struct(vec![Some(LirTy::I64), Some(t)]),
                ObjKind::Enum(vec![vec![], vec![Some(t)]]),
                ObjKind::Cell(t),
                ObjKind::Atom(t),
                ObjKind::Array(t),
                ObjKind::Closure(vec![LirTy::I64, t]),
            ]
        };
        let render = |t: LirTy| {
            let mut o = Objects::default();
            for (i, k) in kinds(t).into_iter().enumerate() {
                o.intern(&format!("o.K{i}"), &format!("K{i}"), k);
            }
            let src = format!(
                "(defstruct fib.typerec (ptr ptr ptr i64 ptr))\n(declare fib.defer void (ptr ptr))\n(declare fib.share-queue void (ptr ptr))\n{}",
                o.render()
            );
            if let Err(e) = lir::parse_and_check(&src) {
                panic!("{}\n{src}", e[0]);
            }
            src
        };
        let (raw, object) = (render(LirTy::Raw), render(LirTy::Ptr));
        for i in 0..kinds(LirTy::Ptr).len() {
            let (drop, trace, share) = (
                format!("drop.{i}"),
                format!("trace.{i}"),
                format!("share.{i}"),
            );
            assert!(
                !function(&raw, &drop).contains("fib.defer"),
                "{}",
                function(&raw, &drop)
            );
            assert!(
                !function(&raw, &trace).contains("indirect-call"),
                "{}",
                function(&raw, &trace)
            );
            assert!(
                !function(&raw, &share).contains("fib.share-queue"),
                "{}",
                function(&raw, &share)
            );
            assert!(function(&object, &share).contains("fib.share-queue"));
            assert!(function(&object, &drop).contains("fib.defer"));
            assert!(function(&object, &trace).contains("indirect-call"));
        }
    }
}
