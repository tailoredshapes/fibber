//! The state machine of an `async` body (types §8.8, compiler.md §8
//! item 3). The body is lowered like any function, with each `await`
//! ending its block: `store point k`, `fib.await-or-park`, and a
//! branch to a `ret` or to the continuation block of state `k`. This
//! pass then makes the function resumable in place:
//!
//! - the entry block keeps its prologue (the captures) and ends in a
//!   `switch` on the task's resume point to `start` (the body's first
//!   block) or to a state's continuation;
//! - every value live into a continuation lives in a slot of the task
//!   frame: stored right after its definition, loaded before each use,
//!   so no SSA value crosses a park and no `phi` needs rebuilding;
//! - every `alloca` becomes a byte slot of the frame, since a resumed
//!   frame may run on another worker's stack;
//! - every `ret v` becomes the task's completion and a bare `ret`.

use std::collections::{HashMap, HashSet};

use crate::compile::Unsupported;
use crate::ir::{parse_sx, targets, Block, FnBuilder, Item, LirTy, Sx};
use crate::objects::Frame;

/// What the caller fixes: the task struct, where its prologue ends,
/// the continuation block of each state (state `k` is `points[k-1]`),
/// the completion function, and the index of the frame's first field
/// (the resume point; the slots follow it).
pub struct Shape<'a> {
    pub tsname: &'a str,
    pub prologue: usize,
    pub points: &'a [String],
    pub complete: &'a str,
    pub point_field: usize,
}

/// Rewrites `f` as described above and returns the frame's layout:
/// the resume point, then the slots.
pub fn make_resumable(f: &mut FnBuilder, shape: &Shape<'_>) -> Result<Vec<Frame>, Unsupported> {
    f.blocks.retain(|b| !b.dead);
    split_entry(f, shape.prologue);
    complete_returns(f, shape.complete);
    // The frame: the resume point, then the slots.
    let mut frame = vec![Frame::Val(LirTy::I32)];
    let mut first = shape.point_field + 1;
    move_allocas(f, shape.tsname, &mut frame, &mut first);
    let live = liveness(f);
    let mut demoted: Vec<String> = Vec::new();
    for label in shape.points {
        let i = f
            .blocks
            .iter()
            .position(|b| &b.label == label)
            .ok_or_else(|| Unsupported(format!("no continuation block {label}")))?;
        for n in &live.live_in[i] {
            if !demoted.contains(n) {
                demoted.push(n.clone());
            }
        }
    }
    let prologue_defs: HashSet<String> = f.blocks[0]
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Bind(n, _, _) => Some(n.clone()),
            Item::Stmt(_) => None,
        })
        .collect();
    demoted.retain(|n| !prologue_defs.contains(n) && n != "env");
    demote(f, shape.tsname, &demoted, &mut frame, &mut first)?;
    switch_entry(f, shape);
    f.ret = None;
    Ok(frame)
}

/// The prologue stays in `entry`; the rest of it becomes `start`.
fn split_entry(f: &mut FnBuilder, prologue: usize) {
    let rest = f.blocks[0].items.split_off(prologue);
    let closed = f.blocks[0].closed;
    f.blocks[0].closed = true;
    f.blocks.insert(
        1,
        Block {
            label: "start".into(),
            items: rest,
            closed,
            ..Block::default()
        },
    );
    for b in &mut f.blocks {
        for (_, text, _) in &mut b.phis {
            *text = subst_atom(text, "entry", "start");
        }
    }
}

/// `(ret v)` completes the task with `v`; `(ret)` with nothing. The
/// `ret` of a park block (labelled `park..` by `await_task`) is the
/// resume returning with the task parked, and stays.
fn complete_returns(f: &mut FnBuilder, complete: &str) {
    for b in &mut f.blocks {
        if b.label.starts_with("park") {
            continue;
        }
        let Some(Item::Stmt(last)) = b.items.last() else {
            continue;
        };
        let Some(inner) = last.strip_prefix("(ret").and_then(|r| r.strip_suffix(')')) else {
            continue;
        };
        let v = match inner.trim() {
            "" => String::new(),
            value => format!(" {value}"),
        };
        b.items.pop();
        b.items
            .push(Item::Stmt(format!("(call @{complete} env{v})")));
        b.items.push(Item::Stmt("(ret)".into()));
    }
}

/// Each `alloca` becomes a byte slot of the frame, its name bound to
/// the slot's address in the prologue.
fn move_allocas(f: &mut FnBuilder, tsname: &str, frame: &mut Vec<Frame>, next: &mut usize) {
    let allocas = std::mem::take(&mut f.allocas);
    let mut binds = Vec::new();
    for (name, _, size) in allocas {
        binds.push(Item::Bind(
            name,
            format!("(getelementptr %struct.{tsname} env (i32 0) (i32 {next}))"),
            LirTy::Ptr,
        ));
        frame.push(Frame::Bytes(size.max(1)));
        *next += 1;
    }
    let entry = &mut f.blocks[0].items;
    binds.append(entry);
    *entry = binds;
}

struct Live {
    live_in: Vec<HashSet<String>>,
}

/// Backward liveness over the blocks: a `phi`'s operand is a use at
/// the end of the block it names.
fn liveness(f: &FnBuilder) -> Live {
    let n = f.blocks.len();
    let index: HashMap<&str, usize> = f
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.label.as_str(), i))
        .collect();
    let mut defs = vec![HashSet::new(); n];
    let mut uses = vec![HashSet::new(); n];
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut phi_uses: Vec<HashMap<usize, HashSet<String>>> = vec![HashMap::new(); n];
    for (i, b) in f.blocks.iter().enumerate() {
        for (name, text, _) in &b.phis {
            defs[i].insert(name.clone());
            for (from, v) in phi_edges(text) {
                if let Some(&p) = index.get(from.as_str()) {
                    if is_name(&v) {
                        phi_uses[p].entry(i).or_default().insert(v);
                    }
                }
            }
        }
        for it in &b.items {
            let (text, bound) = match it {
                Item::Bind(nm, t, _) => (t, Some(nm)),
                Item::Stmt(t) => (t, None),
            };
            for u in names_in(text) {
                if !defs[i].contains(&u) {
                    uses[i].insert(u);
                }
            }
            if let Some(nm) = bound {
                defs[i].insert(nm.clone());
            }
        }
        if let Some(Item::Stmt(last)) = b.items.last() {
            if b.closed {
                succ[i] = targets(last)
                    .iter()
                    .filter_map(|l| index.get(l.as_str()).copied())
                    .collect();
            }
        }
    }
    let mut live_in: Vec<HashSet<String>> = vec![HashSet::new(); n];
    loop {
        let mut changed = false;
        for i in (0..n).rev() {
            let mut out: HashSet<String> = HashSet::new();
            for &s in &succ[i] {
                out.extend(live_in[s].iter().cloned());
                if let Some(pu) = phi_uses[i].get(&s) {
                    out.extend(pu.iter().cloned());
                }
            }
            let mut inn: HashSet<String> = uses[i].clone();
            inn.extend(out.into_iter().filter(|v| !defs[i].contains(v)));
            if inn != live_in[i] {
                live_in[i] = inn;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Live { live_in }
}

/// Gives each demoted value a slot: a store after its definition, a
/// load before each use.
fn demote(
    f: &mut FnBuilder,
    tsname: &str,
    names: &[String],
    frame: &mut Vec<Frame>,
    next: &mut usize,
) -> Result<(), Unsupported> {
    let mut types: HashMap<String, LirTy> = HashMap::new();
    for b in &f.blocks {
        for (nm, _, t) in &b.phis {
            types.insert(nm.clone(), *t);
        }
        for it in &b.items {
            if let Item::Bind(nm, _, t) = it {
                types.insert(nm.clone(), *t);
            }
        }
    }
    let mut slots: HashMap<String, (String, LirTy)> = HashMap::new();
    let mut geps = Vec::new();
    for nm in names {
        let t = *types
            .get(nm)
            .ok_or_else(|| Unsupported(format!("live value {nm} of unknown type")))?;
        let slot = format!("s{next}");
        geps.push(Item::Bind(
            slot.clone(),
            format!("(getelementptr %struct.{tsname} env (i32 0) (i32 {next}))"),
            LirTy::Ptr,
        ));
        slots.insert(nm.clone(), (slot, t));
        frame.push(Frame::Val(t));
        *next += 1;
    }
    f.blocks[0].items.splice(0..0, geps);
    // Loads for phi operands go at the end of the block the edge
    // comes from: (block index, value) -> the loaded name.
    let mut edge_loads: HashMap<(usize, String), String> = HashMap::new();
    let labels: Vec<String> = f.blocks.iter().map(|b| b.label.clone()).collect();
    for bi in 1..f.blocks.len() {
        let phis = f.blocks[bi].phis.clone();
        let mut new_phis = Vec::new();
        for (nm, text, t) in phis {
            let mut text = text;
            for (from, v) in phi_edges(&text) {
                let Some((_, _)) = slots.get(&v) else {
                    continue;
                };
                let Some(p) = labels.iter().position(|l| *l == from) else {
                    continue;
                };
                let key = (p, v.clone());
                let loaded = match edge_loads.get(&key) {
                    Some(l) => l.clone(),
                    None => {
                        let l = f.fresh();
                        edge_loads.insert(key, l.clone());
                        l
                    }
                };
                text = subst_phi_operand(&text, &from, &v, &loaded);
            }
            new_phis.push((nm, text, t));
        }
        f.blocks[bi].phis = new_phis;
    }
    for bi in 1..f.blocks.len() {
        let items = std::mem::take(&mut f.blocks[bi].items);
        let mut out = Vec::new();
        for (nm, _, _) in &f.blocks[bi].phis {
            if let Some((slot, _)) = slots.get(nm) {
                out.push(Item::Stmt(format!("(store {nm} {slot})")));
            }
        }
        let last = items.len().saturating_sub(1);
        for (k, it) in items.into_iter().enumerate() {
            if k == last && f.blocks[bi].closed {
                for ((p, v), loaded) in &edge_loads {
                    if *p == bi {
                        let (slot, t) = &slots[v];
                        out.push(Item::Bind(
                            loaded.clone(),
                            format!("(load {} {slot})", t.text()),
                            *t,
                        ));
                    }
                }
            }
            let (text, bound) = match &it {
                Item::Bind(nm, t, ty) => (t.clone(), Some((nm.clone(), *ty))),
                Item::Stmt(t) => (t.clone(), None),
            };
            let mut text = text;
            for u in names_in(&text) {
                let Some((slot, t)) = slots.get(&u) else {
                    continue;
                };
                let l = f.fresh();
                out.push(Item::Bind(
                    l.clone(),
                    format!("(load {} {slot})", t.text()),
                    *t,
                ));
                text = subst_atom(&text, &u, &l);
            }
            match bound {
                Some((nm, ty)) => {
                    out.push(Item::Bind(nm.clone(), text, ty));
                    if let Some((slot, _)) = slots.get(&nm) {
                        out.push(Item::Stmt(format!("(store {nm} {slot})")));
                    }
                }
                None => out.push(Item::Stmt(text)),
            }
        }
        f.blocks[bi].items = out;
    }
    Ok(())
}

/// The entry's terminator: a `switch` on the resume point.
fn switch_entry(f: &mut FnBuilder, shape: &Shape<'_>) {
    let pp = f.fresh();
    let pt = f.fresh();
    let entry = &mut f.blocks[0].items;
    entry.push(Item::Bind(
        pp.clone(),
        format!(
            "(getelementptr %struct.{} env (i32 0) (i32 {}))",
            shape.tsname, shape.point_field
        ),
        LirTy::Ptr,
    ));
    entry.push(Item::Bind(
        pt.clone(),
        format!("(load i32 {pp})"),
        LirTy::I32,
    ));
    let cases: Vec<String> = shape
        .points
        .iter()
        .enumerate()
        .map(|(i, l)| format!("((i32 {}) {l})", i + 1))
        .collect();
    entry.push(Item::Stmt(format!(
        "(switch {pt} start {})",
        cases.join(" ")
    )));
}

/// Whether an atom is an SSA name of the builder (`tN`).
fn is_name(a: &str) -> bool {
    a.len() > 1 && a.starts_with('t') && a[1..].chars().all(|c| c.is_ascii_digit())
}

/// The SSA names an instruction's text mentions, each once.
fn names_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for a in atoms(text) {
        if is_name(&a) && !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

/// The atoms of an instruction's text, in order.
fn atoms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for c in text.chars() {
        if in_str {
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
        }
        if c.is_whitespace() || c == '(' || c == ')' || c == '"' {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `text` with every atom equal to `from` replaced by `to`, string
/// literals untouched.
fn subst_atom(text: &str, from: &str, to: &str) -> String {
    let mut out = String::new();
    let mut cur = String::new();
    let mut in_str = false;
    let flush = |cur: &mut String, out: &mut String| {
        if !cur.is_empty() {
            out.push_str(if cur == from { to } else { cur });
            cur.clear();
        }
    };
    for c in text.chars() {
        if in_str {
            out.push(c);
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        if c.is_whitespace() || c == '(' || c == ')' || c == '"' {
            flush(&mut cur, &mut out);
            out.push(c);
            if c == '"' {
                in_str = true;
            }
        } else {
            cur.push(c);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// The `(label value)` pairs of a `phi`'s text.
fn phi_edges(text: &str) -> Vec<(String, String)> {
    let Sx::List(items) = parse_sx(&mut text.chars().peekable()) else {
        return Vec::new();
    };
    items
        .iter()
        .skip(2)
        .filter_map(|e| match e {
            Sx::List(pair) => match (pair.first(), pair.get(1)) {
                (Some(Sx::Atom(l)), Some(Sx::Atom(v))) => Some((l.clone(), v.clone())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The phi text with the operand of edge `from` changed from `v` to
/// `to`.
fn subst_phi_operand(text: &str, from: &str, v: &str, to: &str) -> String {
    text.replace(&format!("({from} {v})"), &format!("({from} {to})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atoms_and_substitution_respect_strings_and_parens() {
        assert_eq!(atoms("(add t1 (i64 2))"), ["add", "t1", "i64", "2"]);
        assert_eq!(names_in("(call @f t3 t12 t3)"), ["t3", "t12"]);
        assert_eq!(subst_atom("(add t1 t12)", "t1", "t9"), "(add t9 t12)");
        assert_eq!(
            subst_atom("(call @g (string \"t1\") t1)", "t1", "t2"),
            "(call @g (string \"t1\") t2)"
        );
        assert_eq!(
            phi_edges("(phi i64 (entry t1) (loop3 (i64 2)))"),
            [("entry".to_string(), "t1".to_string())]
        );
    }

    #[test]
    fn a_value_live_across_a_park_lives_in_the_frame() {
        // entry: prologue (a capture load); start: x = 1 + cap; park;
        // resume1: ret x. The value x must be stored after its
        // definition and loaded again in the continuation.
        let mut f = FnBuilder::new(
            "l.f.1",
            Some(LirTy::I64),
            vec![(LirTy::Ptr, "env".into())],
            false,
        );
        let cap = f.val(
            "(load i64 (getelementptr %struct.task.t env (i32 0) (i32 10)))",
            LirTy::I64,
        );
        let prologue = 1;
        let x = f.val(&format!("(add {} (i64 1))", cap.text()), LirTy::I64);
        f.stmt("(store (i32 1) (getelementptr %struct.task.t env (i32 0) (i32 11)))");
        let parked = f.val("(call @fib.await-or-park env (ptr null))", LirTy::I1);
        f.term(&format!("(br {} park2 resume3)", parked.text()));
        f.open("park2");
        f.term("(ret)");
        f.open("resume3");
        f.term(&format!("(ret {})", x.text()));
        let shape = Shape {
            tsname: "task.t",
            prologue,
            points: &["resume3".to_string()],
            complete: "fib.complete.t",
            point_field: 11,
        };
        let frame = make_resumable(&mut f, &shape).expect("resumable");
        assert_eq!(frame, vec![Frame::Val(LirTy::I32), Frame::Val(LirTy::I64)]);
        let text = f.render();
        assert!(text.contains("(switch t"), "{text}");
        assert!(text.contains("(store t2 s12)"), "{text}");
        assert!(text.contains("(load i64 s12)"), "{text}");
        assert!(text.contains("(call @fib.complete.t env t"), "{text}");
        let module = format!(
            "(defstruct task.t (i64 i32 i32 i32 i32 i32 ptr ptr ptr ptr i64 i32 i64))\n(declare fib.await-or-park i1 (ptr ptr))\n(declare fib.complete.t void (ptr i64))\n{text}"
        );
        if let Err(e) = lir::parse_and_check(&module) {
            panic!("{}\n{module}", e[0]);
        }
    }
}
