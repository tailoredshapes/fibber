//! What the checker prints (types §9; `fibref explain file.fib`): per
//! `defun`, `defun f.owned`, `impl` method and `def` of the user module,
//! and per `fn` or `async` literal inside one, its parameters (count
//! kind with the rule of §6.4 that decided it, escape summary), its
//! bindings (`owns`, `alias-of b`, `derived-of b`, `scope-local`), its
//! closure literals (escaping and heap with the reason, captures), its
//! calls (tail call, or the rule of §6.10 that made it ordinary; how
//! each argument is handed over, the copy-in of every `&` argument, the
//! releases before a jump), its `recur`s, and the operations of
//! [`ExprOwn::after`](super::program::ExprOwn) with their source lines.

mod names;

use std::collections::HashMap;
use std::fmt::Write;

use crate::types::ast::{Expr, ExprId, ExprKind, PatKind, Pattern};
use crate::types::decls::ModuleId;
use crate::types::display::Printer;
use crate::types::TypedProgram;

use super::program::{
    BindKind, BodyKey, BodyOwn, OwnedProgram, OwnedWhy, ParamKind, ParamOwn, Pass,
};
use names::{at, index, op, pass, show, site, tail};

/// The explanation of every body of the user module, in checking order.
pub fn explain(p: &TypedProgram, o: &OwnedProgram) -> String {
    let mut out = String::new();
    for key in &o.order {
        let Some((title, body, module)) = header(p, *key) else {
            continue;
        };
        if module != ModuleId::MAIN {
            continue;
        }
        if let Some(b) = o.bodies.get(key) {
            let ix = index(body);
            let s = Section { p, b, ix: &ix };
            s.frame(&mut out, &title, &b.params, body);
        }
    }
    out
}

fn header(p: &TypedProgram, key: BodyKey) -> Option<(String, &Expr, ModuleId)> {
    let g = &p.globals;
    Some(match key {
        BodyKey::Fun(f) | BodyKey::AllOwned(f) => {
            let def = g.fun(f);
            let ty = p.fun_schemes[f.0 as usize]
                .as_ref()
                .map(|s| p.show_scheme(s))
                .unwrap_or_default();
            let suffix = if matches!(key, BodyKey::AllOwned(_)) {
                ".owned"
            } else {
                ""
            };
            let kw = if def.is_macro { "defmacro" } else { "defun" };
            (
                format!("{kw} {}{suffix} : {ty}", def.name),
                &def.body,
                def.module,
            )
        }
        BodyKey::Method(i, m) | BodyKey::MethodOwned(i, m) => {
            let inst = &g.instances[i];
            let im = &inst.methods[m];
            let proto = g.proto(inst.proto);
            let ty = Printer::with_names(g, &inst.var_names, &[]).ty(&inst.head);
            let name = &proto.methods[im.index].name;
            let suffix = if matches!(key, BodyKey::MethodOwned(..)) {
                ".owned"
            } else {
                ""
            };
            (
                format!("impl {}/{name}{suffix} for {ty}", proto.name),
                &im.body,
                inst.module,
            )
        }
        BodyKey::Def(d) => {
            let def = g.def(d);
            (format!("def {}", def.name), &def.init, def.module)
        }
    })
}

/// The expressions of one frame (not inside nested literals) in
/// pre-order, and the literals directly in it.
fn frame_exprs(body: &Expr) -> (Vec<&Expr>, Vec<&Expr>) {
    fn go<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>, lits: &mut Vec<&'e Expr>) {
        out.push(e);
        match &e.kind {
            ExprKind::Fn(_) | ExprKind::Async(..) => lits.push(e),
            _ => {
                for c in super::syntactic::children_of(e) {
                    go(c, out, lits);
                }
            }
        }
    }
    let (mut out, mut lits) = (Vec::new(), Vec::new());
    go(body, &mut out, &mut lits);
    (out, lits)
}

struct Section<'a> {
    p: &'a TypedProgram,
    b: &'a BodyOwn,
    ix: &'a HashMap<ExprId, &'a Expr>,
}

impl Section<'_> {
    fn frame(&self, out: &mut String, title: &str, params: &[ParamOwn], body: &Expr) {
        let (exprs, lits) = frame_exprs(body);
        let _ = writeln!(out, "{title}");
        let _ = writeln!(out, "  params:    {}", self.params(params));
        self.bindings(out, &exprs);
        for l in &lits {
            self.closure_line(out, l);
        }
        self.calls(out, &exprs);
        self.ops(out, body);
        let _ = writeln!(out);
        for l in lits {
            let (title, params, inner) = match &l.kind {
                ExprKind::Fn(lit) => (format!("fn {}", at(l)), self.closure_params(l), &lit.body),
                ExprKind::Async(inner, _) => {
                    (format!("async {}", at(l)), Vec::new(), inner.as_ref())
                }
                _ => continue,
            };
            self.frame(out, &title, &params, inner);
        }
    }

    fn closure_params(&self, l: &Expr) -> Vec<ParamOwn> {
        self.b
            .closures
            .get(&l.id)
            .map(|c| c.params.clone())
            .unwrap_or_default()
    }

    fn params(&self, ps: &[ParamOwn]) -> String {
        if ps.is_empty() {
            return "(none)".into();
        }
        let items: Vec<String> = ps.iter().map(|p| self.param(p)).collect();
        items.join(";  ")
    }

    fn param(&self, p: &ParamOwn) -> String {
        let name = &self.p.globals.binding(p.binding).name;
        let kind = match &p.kind {
            ParamKind::Scalar => return format!("{name}  scalar"),
            ParamKind::Amp => return format!("{name}  &param"),
            ParamKind::Borrowed => "borrowed".to_string(),
            ParamKind::Owned(ws) => {
                let ws: Vec<String> = ws.iter().map(owned_why).collect();
                format!("owned ({})", ws.join("; "))
            }
        };
        let esc = if p.declared_borrow {
            "no (declared :borrow)"
        } else if p.escapes {
            "yes"
        } else {
            "no"
        };
        format!("{name}  {kind}  escapes={esc}")
    }

    fn bindings(&self, out: &mut String, exprs: &[&Expr]) {
        let mut items = Vec::new();
        for e in exprs {
            for b in bound_by(e) {
                if let Some(own) = self.b.bindings.get(&b) {
                    let name = &self.p.globals.binding(b).name;
                    let kind = match own.kind {
                        BindKind::Scalar => continue,
                        BindKind::Owns => "owns".to_string(),
                        BindKind::AliasOf(s) => format!("alias-of {}", site(self.p, self.ix, s)),
                        BindKind::DerivedOf(s) => {
                            format!("derived-of {}", site(self.p, self.ix, s))
                        }
                        BindKind::BorrowedParam | BindKind::OwnedParam | BindKind::AmpParam => {
                            continue
                        }
                    };
                    let local = if own.scope_local { " scope-local" } else { "" };
                    items.push(format!("{name}  {kind}{local}"));
                }
            }
        }
        for e in exprs {
            if self.b.stack_temps.contains(&e.id) {
                items.push(format!("{}  temporary scope-local", show(self.p, e)));
            }
        }
        let text = if items.is_empty() {
            "(none)".to_string()
        } else {
            items.join(";  ")
        };
        let _ = writeln!(out, "  bindings:  {text}");
    }

    fn closure_line(&self, out: &mut String, l: &Expr) {
        let Some(c) = self.b.closures.get(&l.id) else {
            return;
        };
        let yn = |r: &Option<String>| match r {
            Some(why) => format!("yes ({why})"),
            None => "no".into(),
        };
        let caps: Vec<String> = c
            .captures
            .iter()
            .filter(|k| k.pass != Pass::Scalar)
            .map(|k| {
                let what = match k.pass {
                    Pass::Retain => "owns",
                    Pass::Alias => "alias",
                    Pass::OwnCell => "&-cell",
                    other => pass(other),
                };
                format!("{} ({what})", self.p.globals.binding(k.binding).name)
            })
            .collect();
        let kw = if c.is_async { "async" } else { "closure" };
        let _ = writeln!(
            out,
            "  {kw} {}  escaping={}  heap={}  captures: {}",
            at(l),
            yn(&c.escaping),
            yn(&c.heap),
            if caps.is_empty() {
                "(none)".into()
            } else {
                caps.join(", ")
            }
        );
    }

    fn calls(&self, out: &mut String, exprs: &[&Expr]) {
        let mut first = true;
        for e in exprs {
            let line = if let Some(c) = self.b.calls.get(&e.id) {
                let ExprKind::Call(_, args) = &e.kind else {
                    continue;
                };
                let mut parts = vec![
                    format!("{} {}", at(e), show(self.p, e)),
                    tail(self.p, c.tail, c.callee),
                ];
                if c.head != Pass::Scalar {
                    parts.push(format!("head: {}", pass(c.head)));
                }
                let argv: Vec<String> = c
                    .args
                    .iter()
                    .enumerate()
                    .map(|(i, x)| self.arg_text(args, i, *x))
                    .collect();
                if !argv.is_empty() {
                    parts.push(argv.join("; "));
                }
                if !c.jump.is_empty() {
                    let ops: Vec<String> = c.jump.iter().map(|o| op(self.p, self.ix, o)).collect();
                    parts.push(format!("before the jump: {}", ops.join(", ")));
                }
                parts.join("   ")
            } else if let Some(r) = self.b.recurs.get(&e.id) {
                let argv: Vec<String> = r
                    .args
                    .iter()
                    .enumerate()
                    .map(|(i, x)| format!("arg {}: {}", i + 1, pass(*x)))
                    .collect();
                let ops: Vec<String> = r.jump.iter().map(|o| op(self.p, self.ix, o)).collect();
                format!(
                    "{} recur   {}   before the jump: {}",
                    at(e),
                    argv.join("; "),
                    ops.join(", ")
                )
            } else {
                continue;
            };
            let label = if first { "calls:   " } else { "         " };
            first = false;
            let _ = writeln!(out, "  {label}  {line}");
        }
    }

    fn arg_text(&self, args: &[crate::types::ast::Arg], i: usize, x: Pass) -> String {
        match &args[i] {
            crate::types::ast::Arg::Amp(b, _) => {
                format!("&{}: {}", self.p.globals.binding(*b).name, pass(x))
            }
            crate::types::ast::Arg::Expr(e) => {
                format!("arg {} {}: {}", i + 1, show(self.p, e), pass(x))
            }
        }
    }

    /// The operations of the frame in the order they run: an
    /// expression's `after` list follows those of its sub-expressions.
    fn ops(&self, out: &mut String, body: &Expr) {
        let mut items = Vec::new();
        self.ops_post(body, &mut items);
        let text = if items.is_empty() {
            "(none)".to_string()
        } else {
            items.join("; ")
        };
        let _ = writeln!(out, "  ops:       {text}");
    }

    fn ops_post(&self, e: &Expr, items: &mut Vec<String>) {
        if !matches!(e.kind, ExprKind::Fn(_) | ExprKind::Async(..)) {
            for c in super::syntactic::children_of(e) {
                self.ops_post(c, items);
            }
        }
        if let Some(own) = self.b.exprs.get(&e.id) {
            for o in &own.after {
                items.push(format!("L{} {}", e.pos.line, op(self.p, self.ix, o)));
            }
        }
        for o in self.b.guard_fail.get(&e.id).into_iter().flatten() {
            let text = op(self.p, self.ix, o);
            items.push(format!("L{} guard false: {text}", e.pos.line));
        }
    }
}

fn owned_why(w: &OwnedWhy) -> String {
    match w {
        OwnedWhy::Rule1(s) => format!("rule 1: {s}"),
        OwnedWhy::Rule2(s) => format!("rule 2: {s}"),
        OwnedWhy::Rule3(s) => format!("rule 3: {s}"),
        OwnedWhy::Loop(s) => format!("rule 1 {s}"),
        OwnedWhy::Declared => "not inferred".into(),
    }
}

/// The bindings an expression introduces.
fn bound_by(e: &Expr) -> Vec<crate::types::ast::BindingId> {
    let mut out = Vec::new();
    match &e.kind {
        ExprKind::Let(bs, _) => bs.iter().for_each(|(p, _)| pattern_vars(p, &mut out)),
        ExprKind::Match(_, cls) => cls.iter().for_each(|c| pattern_vars(&c.pat, &mut out)),
        ExprKind::Loop(vs, _) => out.extend(vs.iter().map(|(b, _)| *b)),
        _ => {}
    }
    out
}

fn pattern_vars(p: &Pattern, out: &mut Vec<crate::types::ast::BindingId>) {
    match &p.kind {
        PatKind::Bind(b) => out.push(*b),
        PatKind::As(q, b) => {
            out.push(*b);
            pattern_vars(q, out);
        }
        PatKind::Ctor(_, _, subs) => subs.iter().for_each(|s| pattern_vars(s, out)),
        PatKind::Vec(subs, rest) => {
            subs.iter().for_each(|s| pattern_vars(s, out));
            if let crate::types::ast::Rest::Bind(b) = rest {
                out.push(*b);
            }
        }
        PatKind::Wild | PatKind::Lit(_) => {}
    }
}
