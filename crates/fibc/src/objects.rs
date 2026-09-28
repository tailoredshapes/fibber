//! The object types of a program (types §8.2–§8.7): one type id per
//! layout, its `defstruct`, its `drop` and `trace` functions, and the
//! type table and class table as static data.

use std::collections::HashMap;
use std::fmt::Write;

use crate::ir::LirTy;
use crate::layout::{struct_size, HEADER};

/// What an object is (the layouts of §8.3–§8.8).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ObjKind {
    /// `str`: length, then bytes.
    Str,
    /// `(Array T)`: length, then elements.
    Array(LirTy),
    /// A struct: its fields.
    Struct(Vec<Option<LirTy>>),
    /// An enum with fields: a tag, then the variants' fields.
    Enum(Vec<Vec<Option<LirTy>>>),
    /// A cell of the content type.
    Cell(LirTy),
    /// An atom: lock, value.
    Atom(LirTy),
    /// A weak box: lock, target.
    Weak,
    /// A closure: code, captures.
    Closure(Vec<LirTy>),
    /// A task (§8.8): state, driver, lock, resume, result (an atom of
    /// the result type), waiters, the spawned closure (uncounted), then
    /// an `async` body's captures.
    Task(Vec<LirTy>),
}

impl ObjKind {
    /// The trace class letter (compiler.md §4).
    pub fn class(&self) -> u8 {
        match self {
            ObjKind::Cell(_) => b'c',
            ObjKind::Atom(_) => b'a',
            _ => b'o',
        }
    }
}

/// One registered object type.
#[derive(Clone, Debug)]
pub struct ObjInfo {
    pub tid: u32,
    /// The `%struct.` name (without the prefix) of the object, or of
    /// the tag-only prefix for an enum.
    pub sname: String,
    /// A readable name for the type table.
    pub name: String,
    pub kind: ObjKind,
}

impl ObjInfo {
    /// The struct name of variant `v` of an enum.
    pub fn variant_sname(&self, v: usize) -> String {
        format!("{}.v{v}", self.sname)
    }

    /// The field types of the object struct (an enum's are its tag
    /// prefix; its variants are separate structs).
    fn fields(&self) -> Vec<LirTy> {
        let mut f = HEADER.to_vec();
        match &self.kind {
            ObjKind::Str => f.push(LirTy::I64),
            ObjKind::Array(_) => f.push(LirTy::I64),
            ObjKind::Struct(fs) => f.extend(fs.iter().flatten()),
            ObjKind::Enum(_) => f.push(LirTy::I32),
            ObjKind::Cell(t) => f.push(*t),
            ObjKind::Atom(t) => {
                f.push(LirTy::I32);
                f.push(*t);
            }
            ObjKind::Weak => {
                f.push(LirTy::I32);
                f.push(LirTy::Ptr);
                f.push(LirTy::Ptr);
            }
            ObjKind::Closure(caps) => {
                f.push(LirTy::Ptr);
                f.extend(caps);
            }
            ObjKind::Task(caps) => {
                f.extend([LirTy::I32, LirTy::I32, LirTy::I32]);
                f.extend([LirTy::Ptr, LirTy::Ptr, LirTy::Ptr, LirTy::Ptr]);
                f.extend(caps);
            }
        }
        f
    }

    /// The size of a fixed-size object; an enum's is its largest
    /// variant's. Strings and arrays add their elements at run time.
    pub fn size(&self) -> u64 {
        match &self.kind {
            ObjKind::Enum(vs) => vs
                .iter()
                .map(|v| {
                    let mut f = self.fields();
                    f.extend(v.iter().flatten());
                    struct_size(&f)
                })
                .max()
                .unwrap_or(0),
            _ => struct_size(&self.fields()),
        }
    }
}

/// The registry.
#[derive(Debug, Default)]
pub struct Objects {
    list: Vec<ObjInfo>,
    index: HashMap<String, u32>,
}

/// The index of the first user field of a struct object.
pub const STRUCT_FIELD0: usize = 3;
/// The index of the tag of an enum object, and of a variant's first field.
pub const ENUM_TAG: usize = 3;
pub const VARIANT_FIELD0: usize = 4;
/// The index of a string's or an array's length, and of its bytes/elements.
pub const LEN: usize = 3;
pub const ELEMS: usize = 4;
/// A cell's content; an atom's lock and value; a weak box's lock and target.
pub const CELL_VALUE: usize = 3;
pub const ATOM_LOCK: usize = 3;
pub const ATOM_VALUE: usize = 4;
/// A task's fields (§8.8).
pub const TASK_STATE: usize = 3;
pub const TASK_RESULT: usize = 7;
pub const TASK_WAITERS: usize = 8;
pub const TASK_CLOSURE: usize = 9;
pub const TASK_RESUME: usize = 6;
pub const TASK_CAPTURE0: usize = 10;
/// A closure's code pointer and first capture.
pub const CLOSURE_CODE: usize = 3;
pub const CAPTURE0: usize = 4;

impl Objects {
    /// The type id of the object type named `sname` of kind `kind`,
    /// registering it on first use.
    pub fn intern(&mut self, sname: &str, name: &str, kind: ObjKind) -> u32 {
        if let Some(t) = self.index.get(sname) {
            return *t;
        }
        let tid = self.list.len() as u32;
        self.list.push(ObjInfo {
            tid,
            sname: sname.to_string(),
            name: name.to_string(),
            kind,
        });
        self.index.insert(sname.to_string(), tid);
        tid
    }

    pub fn get(&self, tid: u32) -> &ObjInfo {
        &self.list[tid as usize]
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Every `defstruct`, `drop` and `trace` function, and the type
    /// and class tables.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for o in &self.list {
            self.render_structs(&mut out, o);
        }
        for o in &self.list {
            out.push_str(&walker(o, "drop", |child| {
                format!("(call @fib.release {child})")
            }));
            out.push_str(&walker(o, "trace", |child| {
                format!("(indirect-call cb (fn void (ptr)) {child})")
            }));
        }
        self.render_tables(&mut out);
        out
    }

    fn render_structs(&self, out: &mut String, o: &ObjInfo) {
        let words = |fs: &[LirTy]| fs.iter().map(|t| t.text()).collect::<Vec<_>>().join(" ");
        let mut fs = o.fields();
        match &o.kind {
            ObjKind::Str => fs.push(LirTy::I8),
            ObjKind::Array(t) => fs.push(*t),
            _ => {}
        }
        let trailing = match &o.kind {
            ObjKind::Str => " [0 x i8]".to_string(),
            ObjKind::Array(t) => format!(" [0 x {}]", t.text()),
            _ => String::new(),
        };
        let fixed = &fs[..fs.len() - usize::from(!trailing.is_empty())];
        let _ = writeln!(out, "(defstruct {} ({}{trailing}))", o.sname, words(fixed));
        if let ObjKind::Enum(vs) = &o.kind {
            for (i, v) in vs.iter().enumerate() {
                let mut f = o.fields();
                f.extend(v.iter().flatten());
                let _ = writeln!(out, "(defstruct {} ({}))", o.variant_sname(i), words(&f));
            }
        }
    }

    fn render_tables(&self, out: &mut String) {
        let n = self.list.len();
        let recs: Vec<String> = self
            .list
            .iter()
            .map(|o| {
                format!(
                    "(%struct.fib.typerec @drop.{} @trace.{} (string \"{}\") (i64 {}))",
                    o.tid,
                    o.tid,
                    o.name.replace('\\', "\\\\").replace('"', "\\\""),
                    o.size()
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "(constant internal fib.types [{n} x %struct.fib.typerec] ([{n} x %struct.fib.typerec] {}))",
            recs.join(" ")
        );
        let classes: Vec<String> = self
            .list
            .iter()
            .map(|o| format!("(i8 {})", o.kind.class()))
            .collect();
        let _ = writeln!(
            out,
            "(constant internal fib.classes [{n} x i8] ([{n} x i8] {}))",
            classes.join(" ")
        );
    }
}

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
        ObjKind::Task(caps) => {
            let fs: Vec<Option<LirTy>> = caps.iter().map(|c| Some(*c)).collect();
            let mut fields = counted(&fs, TASK_CAPTURE0);
            fields.push((TASK_RESULT, LirTy::Ptr));
            fields.push((TASK_WAITERS, LirTy::Ptr));
            vec![(o.sname.clone(), fields)]
        }
        _ => vec![(o.sname.clone(), Vec::new())],
    }
}

/// A `drop.N` or `trace.N` function: `act` on each child pointer of
/// the object, in field order (an enum's by its tag through `switch`;
/// an array's elements in a loop).
fn walker(o: &ObjInfo, what: &str, act: impl Fn(&str) -> String) -> String {
    let params = if what == "trace" {
        "((ptr p) (ptr cb))"
    } else {
        "((ptr p))"
    };
    let mut s = format!("(define internal ({what}.{} void) {params}\n", o.tid);
    if let ObjKind::Array(LirTy::Ptr | LirTy::Dyn) = o.kind {
        let _ = write!(
            s,
            "  (block entry (let ((n (load i64 (getelementptr %struct.{sn} p (i32 0) (i32 {LEN}))))) (br loop)))
  (block loop (let ((i (phi i64 (entry (i64 0)) (next i2)))) (br (icmp slt i n) body done)))
  (block body (let ((e (load ptr (getelementptr %struct.{sn} p (i32 0) (i32 {ELEMS}) i)))) {} (br next)))
  (block next (let ((i2 (add i (i64 1)))) (br loop)))
  (block done (ret)))\n",
            act("e"),
            sn = o.sname
        );
        return s;
    }
    if let (ObjKind::Weak, "drop") = (&o.kind, what) {
        s.push_str("  (block entry (call @fib.weak-drop p) (ret)))\n");
        return s;
    }
    let groups = child_fields(o);
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

fn releases(sn: &str, fields: &[(usize, LirTy)], act: &impl Fn(&str) -> String) -> String {
    fields
        .iter()
        .map(|(i, t)| {
            let gep = format!("(getelementptr %struct.{sn} p (i32 0) (i32 {i}))");
            let child = match t {
                LirTy::Dyn => format!("(load ptr {gep})"),
                _ => format!("(load ptr {gep})"),
            };
            act(&child)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_once_and_renders_checkable_lir() {
        let mut o = Objects::default();
        let a = o.intern(
            "o.Box",
            "(Box i64)",
            ObjKind::Struct(vec![Some(LirTy::I64)]),
        );
        let b = o.intern(
            "o.Box",
            "(Box i64)",
            ObjKind::Struct(vec![Some(LirTy::I64)]),
        );
        assert_eq!(a, b);
        o.intern(
            "o.List",
            "(List str)",
            ObjKind::Enum(vec![vec![], vec![Some(LirTy::Ptr), Some(LirTy::Ptr)]]),
        );
        o.intern("fib.array.ptr", "(Array str)", ObjKind::Array(LirTy::Ptr));
        o.intern("fib.cell.ptr", "(Cell str)", ObjKind::Cell(LirTy::Ptr));
        o.intern(
            "clo.k",
            "closure",
            ObjKind::Closure(vec![LirTy::I64, LirTy::Ptr]),
        );
        assert_eq!(o.get(1).size(), 40);
        assert_eq!(o.get(2).size(), 24);
        let src = format!(
            "(defstruct fib.typerec (ptr ptr ptr i64))\n(declare fib.release void (ptr))\n{}",
            o.render()
        );
        if let Err(e) = lir::parse_and_check(&src) {
            panic!("{}\n{src}", e[0]);
        }
        assert!(src.contains("(defstruct o.List.v1 (i64 i32 i32 i32 ptr ptr))"));
    }
}
