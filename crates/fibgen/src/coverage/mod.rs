//! Which constructs the generated programs exercised, and how often.

mod more;
mod pipelines;
mod protos;

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Arg, Expr, Kind, Pat, Program};

/// Counts per construct over a run.
#[derive(Clone, Debug, Default)]
pub struct Coverage {
    /// Programs counted.
    pub programs: usize,
    /// Construct → (programs containing it, total occurrences).
    pub counts: BTreeMap<String, (usize, usize)>,
}

impl Coverage {
    /// Adds one program's constructs.
    pub fn add(&mut self, p: &Program) {
        self.programs += 1;
        let ls = labels(p);
        let distinct: BTreeSet<&String> = ls.iter().collect();
        for l in &distinct {
            self.counts.entry((*l).clone()).or_default().0 += 1;
        }
        for l in &ls {
            self.counts.entry(l.clone()).or_default().1 += 1;
        }
    }

    /// Adds the labels of what the model's run of one program did (a
    /// guard that failed, a default method that ran), counted once each.
    pub fn add_trace(&mut self, trace: &[&'static str]) {
        for l in trace {
            let e = self.counts.entry((*l).to_string()).or_default();
            e.0 += 1;
            e.1 += 1;
        }
    }

    /// Adds another run's counts.
    pub fn merge(&mut self, other: &Coverage) {
        self.programs += other.programs;
        for (k, (a, b)) in &other.counts {
            let e = self.counts.entry(k.clone()).or_default();
            e.0 += a;
            e.1 += b;
        }
    }

    /// A table: construct, % of programs containing it, occurrences.
    pub fn render(&self) -> String {
        let mut rows: Vec<_> = self.counts.iter().collect();
        rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
        let mut out = format!(
            "{:<34} {:>9} {:>11}\n",
            "construct", "programs", "occurrences"
        );
        let n = self.programs.max(1) as f64;
        for (k, (progs, occ)) in rows {
            let pct = 100.0 * *progs as f64 / n;
            out.push_str(&format!("{k:<34} {pct:>8.1}% {occ:>11}\n"));
        }
        out
    }
}

/// The helper kind named by a helper's name prefix.
fn helper_kind(name: &str) -> Option<&'static str> {
    let kinds = [
        ("plain", "plain"),
        ("tail", "self tail recursion"),
        ("muta", "mutual tail recursion"),
        ("mutb", "mutual tail recursion"),
        ("io", "& parameters"),
        ("async", "async function"),
        ("make", "returns a closure"),
        ("mk", "returns a closure over its parameter"),
        ("spin", "tail call passing a closure over a local"),
        ("stash", "stores its parameter in a cell"),
        ("pick", "returns its parameter or a fresh value"),
        ("same", "returns its parameter via a pattern"),
        ("swapout", "& parameter read, then replaced"),
        ("lastor", "loop returning its parameter"),
        ("fwda", "mutual tail calls forwarding an & parameter"),
        ("walk", "tail call passing a constructed local"),
        ("fwdb", "mutual tail calls forwarding an & parameter"),
        ("gs", "generic over a protocol-bounded type variable"),
    ];
    kinds
        .iter()
        .find(|(p, _)| name.starts_with(p) && name[p.len()..].chars().all(|c| c.is_ascii_digit()))
        .map(|(_, k)| *k)
}

/// Every construct occurrence in `p`.
pub fn labels(p: &Program) -> Vec<String> {
    let mut out = Vec::new();
    for _ in &p.defs {
        out.push("def constant (immortal)".to_string());
    }
    for f in &p.funs {
        if let Some(k) = helper_kind(&f.name) {
            out.push(format!("defun: {k}"));
        }
    }
    protos::item_labels(p, &mut out);
    for b in p.bodies() {
        b.walk(&mut |e| {
            node_labels(e, &mut out);
            protos::node_labels(e, &mut out);
        });
    }
    out
}

fn node_labels(e: &Expr, out: &mut Vec<String>) {
    let mut push = |s: &str| out.push(s.to_string());
    match &e.kind {
        Kind::Let(bs, _) => let_labels(bs, out),
        Kind::If(..) if e.ty.is_object() => push("if joining object values"),
        Kind::If(..) => push("if"),
        Kind::Match(_, cl) => {
            push("match");
            if cl.iter().any(|(p, _)| protos::has_vector(p)) {
                push("vector pattern in match");
            }
            if cl.iter().any(|(p, _)| nested(p)) {
                push("match binding sub-objects (nested pattern)");
            }
        }
        Kind::Loop(bs, _) if bs.is_empty() => push("loop spinning on an atom another thread sets"),
        Kind::Loop(..) => push("loop"),
        Kind::Recur(..) => push("recur"),
        Kind::Fn(..) => push("fn literal"),
        Kind::FnNamed(..) => push("named fn literal (self-recursive)"),
        Kind::Global(_) => push("named function as value"),
        Kind::Apply(..) => push("call through function value"),
        Kind::Field(..) => push("field access"),
        Kind::Deref(..) => push("@ read"),
        Kind::Set(..) => push("set!"),
        Kind::SetField(..) => push("set-field!"),
        Kind::Async(..) => push("async"),
        Kind::Await(..) => push("await"),
        Kind::Plet(..) => push("plet"),
        Kind::WeakDead(..) => push("weak to a dead target"),
        Kind::VecLit(items) => {
            push("vector literal");
            if items.iter().any(|i| i.ty.is_fn()) {
                push("closure stored in vector");
            }
        }
        Kind::Call(h, args) => call_labels(h, args, out),
        Kind::Pipe(p) => pipelines::labels(p, out),
        _ => {}
    }
}

fn let_labels(bs: &[(Pat, Expr)], out: &mut Vec<String>) {
    out.push("let".into());
    if bs.iter().any(|(p, _)| protos::has_vector(p)) {
        out.push("let destructuring with [& r] (vector pattern)".into());
    }
    if bs
        .iter()
        .any(|(p, _)| matches!(p, Pat::Ctor(..) | Pat::As(..)))
    {
        out.push("let destructuring / :as".into());
    }
    if bs
        .iter()
        .any(|(_, i)| i.ty.is_object() && matches!(i.kind, Kind::Let(..)))
    {
        out.push("nested let with object value".into());
    }
}

fn nested(p: &Pat) -> bool {
    match p {
        Pat::Ctor(_, ps) => ps.iter().any(|q| matches!(q, Pat::Ctor(..) | Pat::As(..))),
        Pat::Some(q) => matches!(**q, Pat::Ctor(..) | Pat::As(..)),
        Pat::As(q, _) => nested(q) || matches!(**q, Pat::Ctor(..)),
        _ => false,
    }
}

fn call_labels(h: &str, args: &[Arg], out: &mut Vec<String>) {
    let inouts = args.iter().filter(|a| matches!(a, Arg::InOut(_))).count();
    if inouts > 0 {
        out.push("& argument".into());
    }
    if inouts > 1 {
        out.push("call with two & arguments".into());
    }
    let fn_arg = args
        .iter()
        .any(|a| matches!(a, Arg::Val(e) if e.ty.is_fn()));
    match (h, fn_arg) {
        ("Holder" | "Box" | "box", true) => out.push("closure stored in struct".into()),
        ("cell", true) => out.push("closure stored in cell".into()),
        ("conj", true) => out.push("closure stored in vector".into()),
        _ => {}
    }
    match helper_kind(h) {
        Some(k) => out.push(format!("call helper: {k}")),
        None => out.push(format!("call {h}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ty::Ty;

    #[test]
    fn counts_programs_and_occurrences() {
        let e = Expr::call(
            Ty::Int,
            "+",
            vec![
                Expr::call(Ty::Int, "+", vec![Expr::int(1), Expr::int(2)]),
                Expr::int(3),
            ],
        );
        let mut c = Coverage::default();
        c.add(&Program {
            defs: Vec::new(),
            impls: Vec::new(),
            funs: Vec::new(),
            main: e,
        });
        assert_eq!(c.counts.get("call +"), Some(&(1, 2)));
        assert!(c.render().contains("call +"));
    }

    #[test]
    fn helper_kinds_come_from_names() {
        assert_eq!(helper_kind("tail12"), Some("self tail recursion"));
        assert_eq!(helper_kind("io3"), Some("& parameters"));
        assert_eq!(helper_kind("inc1"), None);
    }
}
