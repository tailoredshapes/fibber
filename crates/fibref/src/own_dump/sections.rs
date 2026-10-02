//! The sections of one module of the ownership dump (spec/bootstrap.md
//! §7.3): `body`, `facts`, `summary`, `taken` and `explain`. Every table
//! that is a `HashMap` in the pass is printed in order of its id; the
//! others are already ordered.

use std::collections::HashMap;

use crate::own::explain::explain_module;
use crate::own::program::{BodyKey, BodyOwn, ClosureOwn};
use crate::own::{Facts, OwnedProgram};
use crate::types::decls::Globals;
use crate::types::TypedProgram;
use crate::types_dump::{impl_method_name, push_raw, Shown};

use super::texts::{
    alloc, bind_kind, body_key, callee, key_module, mode, ops, param, passes, reason, tail, whys,
};
use super::{Options, Section};

/// What the pass decided and saw, for the sections to read.
pub(crate) struct Seen<'a> {
    pub typed: &'a TypedProgram,
    pub owned: &'a OwnedProgram,
    /// Per unit, in the order they were decided: its bodies' keys and
    /// its finished facts.
    pub facts: &'a [(Vec<BodyKey>, Facts)],
}

/// Appends the sections `opts` asks for of the module `s`.
pub(super) fn module(v: &Seen, s: &Shown, opts: &Options, out: &mut String) {
    let g = &v.typed.globals;
    if opts.wants(Section::Body) {
        for key in v.owned.order.iter().filter(|k| key_module(g, **k) == s.id) {
            body(g, *key, &v.owned.bodies[key], out);
        }
    }
    if opts.wants(Section::Facts) {
        for (keys, facts) in v.facts.iter().filter(|(k, _)| key_module(g, k[0]) == s.id) {
            unit_facts(g, keys, facts, out);
        }
    }
    if opts.wants(Section::Summary) {
        summaries(g, v.owned, s, out);
    }
    if opts.wants(Section::Taken) {
        taken(g, v.owned, s, out);
    }
    if opts.wants(Section::Explain) {
        for line in explain_module(v.typed, v.owned, s.id).lines() {
            push_raw(out, line);
        }
    }
}

/// The entries of `map` in order of key.
fn sorted<K: Ord + Copy, V>(map: &HashMap<K, V>) -> Vec<(K, &V)> {
    let mut all: Vec<(K, &V)> = map.iter().map(|(k, v)| (*k, v)).collect();
    all.sort_by_key(|(k, _)| *k);
    all
}

/// `HEAD` and then ` after OPS` when there are operations.
fn after(head: String, list: &[crate::own::program::Op]) -> String {
    match list {
        [] => head,
        _ => format!("{head} after {}", ops(list)),
    }
}

fn body(g: &Globals, key: BodyKey, b: &BodyOwn, out: &mut String) {
    push_raw(out, &format!("body {}", body_key(g, key)));
    for p in &b.params {
        push_raw(out, &format!("  param {}", param(g, p)));
    }
    for (e, x) in sorted(&b.exprs) {
        push_raw(
            out,
            &after(format!("  expr E{} {}", e.0, mode(x.mode)), &x.after),
        );
    }
    for (id, x) in sorted(&b.bindings) {
        let (name, kind) = (&g.binding(id).name, bind_kind(x.kind));
        let local = u8::from(x.scope_local);
        push_raw(
            out,
            &format!("  binding B{} {name} {kind} scope-local={local}", id.0),
        );
    }
    calls(g, b, out);
    closures(g, b, out);
    for (e, a) in sorted(&b.allocs) {
        push_raw(out, &format!("  alloc E{} {}", e.0, alloc(*a)));
    }
    for e in &b.stack_temps {
        push_raw(out, &format!("  stack-temp E{}", e.0));
    }
    for (e, list) in sorted(&b.guard_fail) {
        let line = format!("  guard-fail E{}", e.0);
        push_raw(
            out,
            &if list.is_empty() {
                line
            } else {
                format!("{line} {}", ops(list))
            },
        );
    }
}

fn calls(g: &Globals, b: &BodyOwn, out: &mut String) {
    for (e, c) in sorted(&b.calls) {
        let wb: Vec<String> = c
            .write_backs
            .iter()
            .map(|(i, v)| format!("{i}:{}", v.0))
            .collect();
        let line = format!(
            "  call E{} {} tail={} head={} args={} write-backs={} jump={}",
            e.0,
            callee(g, c.callee),
            tail(c.tail),
            super::texts::pass(c.head),
            passes(&c.args),
            wb.join(","),
            ops(&c.jump)
        );
        push_raw(out, &line);
    }
    for (e, r) in sorted(&b.recurs) {
        let line = format!(
            "  recur E{} args={} jump={}",
            e.0,
            passes(&r.args),
            ops(&r.jump)
        );
        push_raw(out, &line);
    }
}

fn closures(g: &Globals, b: &BodyOwn, out: &mut String) {
    for (e, c) in sorted(&b.closures) {
        push_raw(out, &closure_line(e.0, c));
        for p in &c.params {
            push_raw(out, &format!("    closure-param E{} {}", e.0, param(g, p)));
        }
    }
}

fn closure_line(e: u32, c: &ClosureOwn) -> String {
    let caps: Vec<String> = c
        .captures
        .iter()
        .map(|k| format!("{}:{}", k.binding.0, super::texts::pass(k.pass)))
        .collect();
    format!(
        "  closure E{e} async={} captures={} escaping={} heap={}",
        u8::from(c.is_async),
        caps.join(","),
        reason(&c.escaping),
        reason(&c.heap)
    )
}

/// One line for the unit whose bodies are `keys`, then its facts: the
/// owned parameters with every reason, the escaping parameters and loop
/// variables with the first reason, the loop variables of rule 1, the
/// escaping closure literals and the heap ones, each in order of id.
fn unit_facts(g: &Globals, keys: &[BodyKey], f: &Facts, out: &mut String) {
    let names: Vec<String> = keys.iter().map(|k| body_key(g, *k)).collect();
    push_raw(out, &format!("facts {}", names.join(", ")));
    for (b, ws) in &f.owned {
        push_raw(out, &format!("  owned B{} {}", b.0, whys(ws)));
    }
    for (b, why) in &f.escapes {
        push_raw(out, &format!("  escapes B{} {why}", b.0));
    }
    for b in &f.loop_rule1 {
        push_raw(out, &format!("  loop-rule1 B{}", b.0));
    }
    for (e, why) in sorted(&f.escaping) {
        push_raw(out, &format!("  escaping E{} {why}", e.0));
    }
    for (e, why) in sorted(&f.heap) {
        push_raw(out, &format!("  heap E{} {why}", e.0));
    }
}

/// `summary NAME (OWNED,ESCAPES)..` per `defun` of the module, in order
/// of function id.
fn summaries(g: &Globals, o: &OwnedProgram, s: &Shown, out: &mut String) {
    for (f, sum) in sorted(&o.summaries) {
        let def = g.fun(f);
        if def.module == s.id {
            let ps: Vec<String> = sum
                .params
                .iter()
                .map(|(o, e)| format!(" ({},{})", u8::from(*o), u8::from(*e)))
                .collect();
            push_raw(out, &format!("summary {}{}", def.name, ps.concat()));
        }
    }
}

/// `taken value NAME` per function of the module whose value is taken,
/// `taken method I NAME` per method implementation, in order.
fn taken(g: &Globals, o: &OwnedProgram, s: &Shown, out: &mut String) {
    for f in o.value_taken.iter().filter(|f| g.fun(**f).module == s.id) {
        push_raw(out, &format!("taken value {}", g.fun(*f).name));
    }
    for (i, m) in o
        .methods_taken
        .iter()
        .filter(|(i, _)| g.instances[*i].module == s.id)
    {
        push_raw(
            out,
            &format!("taken method {i} {}", impl_method_name(g, *i, *m)),
        );
    }
}
