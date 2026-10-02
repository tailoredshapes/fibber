//! The two deeper levels of the dump (spec/bootstrap.md §6.5): the
//! resolved AST of every body (`--ast`, after lowering) and, under each
//! unit, the typed tables its expressions and bindings have (`--tables`,
//! after inference). Both walk the bodies in one order, depth first, so
//! that the ids the lowering gave (`ExprId`, `BindingId`: allocation
//! order) are part of what the port must reproduce.

use crate::dump::{dump_form, span_in};
use crate::syntax::Pos;
use crate::types::ast::{Arg, BindingId, Expr, ExprKind, PatKind, Pattern, Place, Rest, TypeAnn};
use crate::types::builtins::BUILTINS;
use crate::types::decls::{DefDef, FunDef, Globals, ImplMethod};
use crate::types::display::Printer;
use crate::types::infer::{Resolution, UnitRef};
use crate::types::TypedProgram;

use super::texts::{ann_text, binding_kind, binding_line, colour_text, expr_kind, pat_line};
use super::{push, push_raw, Shown, View};

/// A body that has an AST: a `defun` or `defmacro`, a `def`, an `impl`
/// method.
#[derive(Clone, Copy)]
enum Item<'a> {
    Fun(&'a FunDef),
    Def(&'a DefDef),
    Impl(&'a ImplMethod),
}

/// What a walk of a body meets, with the depth it is at.
enum Ev<'a> {
    Expr(&'a Expr),
    /// A binding introduced outside a pattern: a parameter, the name of
    /// an `fn`, a `loop` variable.
    Binding(BindingId),
    Pat(&'a Pattern),
    /// A `match` clause, whether it has a guard.
    Clause(bool),
    /// `&x`: an argument or a place (which has no position).
    Amp(BindingId, Option<&'a Pos>),
    /// A written result type (or the annotation of a `def`).
    Ret(&'a TypeAnn),
    Quote(&'a crate::syntax::Form),
}

type Visit<'v> = &'v mut dyn FnMut(Ev<'_>, usize);

fn walk_item(item: Item<'_>, d: usize, f: Visit<'_>) {
    match item {
        Item::Fun(x) => {
            x.params.iter().for_each(|p| f(Ev::Binding(p.binding), d));
            x.ret.iter().for_each(|r| f(Ev::Ret(r), d));
            walk(&x.body, d, f);
        }
        Item::Def(x) => {
            x.ann.iter().for_each(|a| f(Ev::Ret(a), d));
            walk(&x.init, d, f);
        }
        Item::Impl(x) => {
            x.params.iter().for_each(|b| f(Ev::Binding(*b), d));
            x.ret.iter().for_each(|r| f(Ev::Ret(r), d));
            walk(&x.body, d, f);
        }
    }
}

fn walk_place(p: &Place, d: usize, f: Visit<'_>) {
    match p {
        Place::Expr(e) => walk(e, d, f),
        Place::Amp(b) => f(Ev::Amp(*b, None), d),
    }
}

fn walk_pat(p: &Pattern, d: usize, f: Visit<'_>) {
    f(Ev::Pat(p), d);
    match &p.kind {
        PatKind::Ctor(_, _, subs) | PatKind::Vec(subs, _) => {
            subs.iter().for_each(|s| walk_pat(s, d + 1, f));
        }
        PatKind::As(inner, _) => walk_pat(inner, d + 1, f),
        PatKind::Wild | PatKind::Bind(_) | PatKind::Lit(_) => {}
    }
}

fn walk(e: &Expr, d: usize, f: Visit<'_>) {
    f(Ev::Expr(e), d);
    let d = d + 1;
    match &e.kind {
        ExprKind::Lit(_) | ExprKind::Local(_) | ExprKind::Global(_) => {}
        ExprKind::Quote(form) => f(Ev::Quote(form), d),
        _ => walk_children(e, d, f),
    }
}

fn walk_fn(lit: &crate::types::ast::FnLit, d: usize, f: Visit<'_>) {
    lit.name.iter().for_each(|b| f(Ev::Binding(*b), d));
    lit.params.iter().for_each(|(b, _)| f(Ev::Binding(*b), d));
    lit.ret.iter().for_each(|r| f(Ev::Ret(r), d));
    walk(&lit.body, d, f);
}

fn walk_clause(c: &crate::types::ast::Clause, d: usize, f: Visit<'_>) {
    f(Ev::Clause(c.guard.is_some()), d);
    walk_pat(&c.pat, d + 1, f);
    c.guard.iter().for_each(|g| walk(g, d + 1, f));
    walk(&c.body, d + 1, f);
}

/// The children of a node that has some, at depth `d`.
fn walk_children(e: &Expr, d: usize, f: Visit<'_>) {
    match &e.kind {
        ExprKind::Lit(_) | ExprKind::Local(_) | ExprKind::Global(_) | ExprKind::Quote(_) => {}
        ExprKind::Call(head, args) => {
            walk(head, d, f);
            for a in args {
                match a {
                    Arg::Expr(x) => walk(x, d, f),
                    Arg::Amp(b, pos) => f(Ev::Amp(*b, Some(pos)), d),
                }
            }
        }
        ExprKind::Fn(lit) => walk_fn(lit, d, f),
        ExprKind::Let(bs, body) => {
            for (p, init) in bs {
                walk_pat(p, d, f);
                walk(init, d, f);
            }
            walk(body, d, f);
        }
        ExprKind::If(c, t, el) => [c, t, el].into_iter().for_each(|x| walk(x, d, f)),
        ExprKind::Do(es) | ExprKind::Recur(es) | ExprKind::Concat(es) => {
            es.iter().for_each(|x| walk(x, d, f));
        }
        ExprKind::Match(scrutinee, clauses) => {
            walk(scrutinee, d, f);
            clauses.iter().for_each(|c| walk_clause(c, d, f));
        }
        ExprKind::Loop(vars, body) => {
            for (b, init) in vars {
                f(Ev::Binding(*b), d);
                walk(init, d, f);
            }
            walk(body, d, f);
        }
        ExprKind::Deref(p, _) => walk_place(p, d, f),
        ExprKind::Set(p, v) => {
            walk_place(p, d, f);
            walk(v, d, f);
        }
        ExprKind::Field(x, _, _)
        | ExprKind::SetField(_, _, x)
        | ExprKind::Async(x, _)
        | ExprKind::Await(x)
        | ExprKind::Unsafe(x)
        | ExprKind::Dyn(_, _, _, x)
        | ExprKind::Convert(_, _, x) => walk(x, d, f),
    }
}

fn ast_event(g: &Globals, home: &str, ev: Ev<'_>, d: usize, out: &mut String) {
    let pad = "  ".repeat(d);
    let line = match ev {
        Ev::Expr(e) => format!("E{} {} {}", e.id.0, expr_kind(g, e), span_in(&e.pos, home)),
        Ev::Binding(b) => binding_line(g, b, home),
        Ev::Pat(p) => pat_line(g, p, home),
        Ev::Clause(guard) => format!("clause{}", if guard { " guard" } else { "" }),
        Ev::Amp(b, pos) => {
            let at = pos.map_or(String::new(), |p| format!(" {}", span_in(p, home)));
            format!("amp B{} {}{at}", b.0, g.binding(b).name)
        }
        Ev::Ret(a) => format!("ret {}", ann_text(g, a)),
        Ev::Quote(form) => {
            let mut text = String::new();
            dump_form(form, d, Some(home), &mut text);
            out.push_str(&text);
            return;
        }
    };
    push_raw(out, &format!("{pad}{line}"));
}

/// The `ast` section of a module: every body, in the order funs, defs,
/// impl methods, macros, each under a line `ast fun NAME`, `ast def
/// NAME`, `ast impl INDEX METHOD` or `ast macro NAME`.
pub(super) fn ast(v: &View, s: &Shown, out: &mut String) {
    let g = v.g;
    let item = |head: String, it: Item<'_>, out: &mut String| {
        push_raw(out, &head);
        walk_item(it, 1, &mut |ev, d| ast_event(g, &s.file, ev, d, out));
    };
    for f in g.funs.iter().filter(|f| f.module == s.id && !f.is_macro) {
        item(format!("ast fun {}", f.name), Item::Fun(f), out);
    }
    for d in g.defs.iter().filter(|d| d.module == s.id) {
        item(format!("ast def {}", d.name), Item::Def(d), out);
    }
    for (i, inst) in g
        .instances
        .iter()
        .enumerate()
        .filter(|(_, x)| x.module == s.id)
    {
        for m in &inst.methods {
            let name = &g.proto(inst.proto).methods[m.index].name;
            item(format!("ast impl {i} {name}"), Item::Impl(m), out);
        }
    }
    for f in g.funs.iter().filter(|f| f.module == s.id && f.is_macro) {
        item(format!("ast macro {}", f.name), Item::Fun(f), out);
    }
}

/// The type variables each body of a unit is printed with, and the body.
fn unit_items<'a>(g: &'a Globals, t: &TypedProgram, u: &UnitRef) -> Vec<(Vec<String>, Item<'a>)> {
    let fun = |f: &crate::types::ast::FunId| {
        let names = t.fun_schemes[f.0 as usize]
            .as_ref()
            .map(|s| s.var_names.clone());
        (names.unwrap_or_default(), Item::Fun(g.fun(*f)))
    };
    match u {
        UnitRef::Scc(fs) => fs.iter().map(fun).collect(),
        UnitRef::Macro(f) => vec![fun(f)],
        UnitRef::Def(d) => vec![(Vec::new(), Item::Def(g.def(*d)))],
        UnitRef::ImplMethod(i, k) => {
            let inst = &g.instances[*i];
            vec![(inst.var_names.clone(), Item::Impl(&inst.methods[*k]))]
        }
    }
}

fn pattern_bindings(p: &Pattern) -> Vec<BindingId> {
    match &p.kind {
        PatKind::Bind(b) | PatKind::As(_, b) | PatKind::Vec(_, Rest::Bind(b)) => vec![*b],
        _ => Vec::new(),
    }
}

fn binding_record(t: &TypedProgram, p: &Printer<'_>, b: BindingId, out: &mut String) {
    let info = t.globals.binding(b);
    let ty = t
        .binding_types
        .get(&b)
        .map_or("_".to_string(), |ty| p.ty(ty));
    let kind = binding_kind(info.kind);
    push(
        out,
        &format!("  binding B{} {} {kind} {ty}", b.0, info.name),
    );
}

fn expr_records(t: &TypedProgram, p: &Printer<'_>, home: &str, e: &Expr, out: &mut String) {
    let (id, n) = (e.id, e.id.0);
    let tys = |xs: &[crate::types::ty::Ty]| -> Vec<String> { xs.iter().map(|x| p.ty(x)).collect() };
    if let Some(ty) = t.expr_types.get(&id) {
        push(
            out,
            &format!("  expr E{n} {} {}", span_in(&e.pos, home), p.ty(ty)),
        );
    }
    if let Some(i) = t.instantiations.get(&id) {
        let mut words = tys(&i.tys);
        words.push("|".into());
        words.extend(i.colours.iter().map(|c| colour_text(*c)));
        push(out, &format!("  inst E{n} {}", words.join(" ")));
    }
    if let Some(r) = t.resolutions.get(&id) {
        let how = match r {
            Resolution::Instance { index, args } => {
                format!(
                    "instance {index}{}",
                    tys(args)
                        .iter()
                        .map(|a| format!(" {a}"))
                        .collect::<String>()
                )
            }
            Resolution::Bound(pred) => format!("bound {}", p.pred(pred)),
            Resolution::Dyn => "dyn".to_string(),
        };
        push(out, &format!("  res E{n} {how}"));
    }
    if let Some(k) = t.fn_colours.get(&id) {
        push(out, &format!("  fn E{n} {}", colour_text(*k)));
    }
}

/// The records of the tables under a unit, two spaces in: `expr E POS
/// TYPE`, `binding B NAME KIND TYPE`, `inst E TYPE.. | COLOUR..`, `res E
/// instance I TYPE.. | bound PRED | dyn`, `fn E COLOUR`, in the order a
/// walk of the bodies meets the expressions and bindings.
pub(super) fn unit_tables(
    v: &View,
    t: &TypedProgram,
    (u, home): (&UnitRef, &str),
    out: &mut String,
) {
    for (names, item) in unit_items(v.g, t, u) {
        let p = Printer::with_names(v.g, &names, &[]);
        walk_item(item, 0, &mut |ev, _| match ev {
            Ev::Expr(e) => expr_records(t, &p, home, e, out),
            Ev::Binding(b) => binding_record(t, &p, b, out),
            Ev::Pat(pat) => pattern_bindings(pat)
                .into_iter()
                .for_each(|b| binding_record(t, &p, b, out)),
            _ => {}
        });
    }
}

/// The static table of builtins (types `builtins.rs`), printed once per
/// run when `tables` is asked for: `builtin INDEX NAME SIG [where BOUNDS]
/// escapes ESCAPE.. [unsafe]`.
pub(super) fn builtins_text() -> String {
    let mut out = String::from("-- builtins\n");
    for (i, b) in BUILTINS.iter().enumerate() {
        let bounds = if b.bounds.is_empty() {
            String::new()
        } else {
            format!(" where {}", b.bounds)
        };
        let escapes: String = b
            .escapes
            .iter()
            .map(|e| format!(" {}", format!("{e:?}").to_lowercase()))
            .collect();
        let unsafe_only = if b.unsafe_only { " unsafe" } else { "" };
        out.push_str(&format!(
            "builtin {i} {} {}{bounds} escapes{escapes}{unsafe_only}\n",
            b.name, b.sig
        ));
    }
    out
}
