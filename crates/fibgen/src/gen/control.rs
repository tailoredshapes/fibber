//! `let`, `if`, `do`, `match` and `loop` at any type.

use crate::ast::{Expr, Kind, Pat, Rest};
use crate::ty::Ty;

use super::{effects, mcalls, tasks, vpat, Ctx, Gen, Var, VarKind};

fn var(name: &str, ty: &Ty, kind: VarKind) -> Var {
    Var {
        name: name.to_string(),
        ty: ty.clone(),
        kind,
        send_fn: false,
    }
}

/// Whether a function value made by `e` in `cx` is sendable: a named
/// function, or a literal whose free variables are all sendable.
pub fn is_send_fn(e: &Expr, cx: &Ctx) -> bool {
    match &e.kind {
        Kind::Global(_) => true,
        Kind::Fn(_, body) | Kind::FnNamed(_, _, body) => {
            let mut ok = true;
            body.walk(&mut |n| {
                if let Kind::Var(name) = &n.kind {
                    if let Some(v) = cx.vars.iter().find(|v| &v.name == name) {
                        ok &= v.ty.is_send() || v.send_fn;
                    }
                }
            });
            ok
        }
        _ => false,
    }
}

/// The fields of a struct type that a pattern can take apart.
fn struct_fields(ty: &Ty) -> Option<(&'static str, Vec<Ty>)> {
    match ty {
        Ty::Pt => Some(("Pt", vec![Ty::Int, Ty::Int])),
        Ty::Wrap => Some(("Wrap", vec![Ty::Str, Ty::vec(Ty::Int)])),
        Ty::Holder => Some(("Holder", vec![Ty::fn_i(), Ty::cell(Ty::Int)])),
        Ty::Boxed(t) => Some(("Box", vec![(**t).clone()])),
        Ty::Hook(_) => Some(("Hook", vec![Ty::fn_i(), Ty::Int])),
        _ => None,
    }
}

/// An irrefutable pattern for `ty` and the variables it binds: a name,
/// or (for a struct) a positional pattern, sometimes nested or `:as`.
pub fn irrefutable(g: &mut Gen, ty: &Ty, depth: u32) -> (Pat, Vec<Var>) {
    if matches!(ty, Ty::Vec(_)) && g.rng.chance(20) {
        return all_rest(g, ty);
    }
    let fields = struct_fields(ty);
    if fields.is_none() || depth == 0 || g.rng.chance(40) {
        if g.rng.chance(10) {
            return (Pat::Wild, Vec::new());
        }
        // Not `p`: `fresh("p")` could make a helper's parameter name
        // `p{i}`, which `let_form` may already have shadowed in the same
        // `let`, and a name may not be bound twice in one `let` (syntax
        // §3.3; seed 958808).
        let n = g.fresh("pv");
        return (Pat::Bind(n.clone()), vec![var(&n, ty, VarKind::Pattern)]);
    }
    let (head, ftys) = fields.unwrap_or(("", Vec::new()));
    let mut pats = Vec::new();
    let mut vars = Vec::new();
    for t in &ftys {
        let (p, vs) = irrefutable(g, t, depth - 1);
        pats.push(p);
        vars.extend(vs);
    }
    let pat = Pat::Ctor(head.into(), pats);
    if g.rng.chance(25) {
        let n = g.fresh("whole");
        vars.push(var(&n, ty, VarKind::Pattern));
        return (Pat::As(Box::new(pat), n), vars);
    }
    (pat, vars)
}

/// `[& r]` (or `[& _]`), the one irrefutable vector pattern: `r` owns
/// a new vector of every element (syntax §3.3).
fn all_rest(g: &mut Gen, ty: &Ty) -> (Pat, Vec<Var>) {
    if g.rng.chance(20) {
        return (Pat::Vector(Vec::new(), Some(Rest::Wild)), Vec::new());
    }
    let r = g.fresh("all");
    let v = var(&r, ty, VarKind::Pattern);
    (Pat::Vector(Vec::new(), Some(Rest::Bind(r))), vec![v])
}

/// `(let ((pat e) ...) body)`.
pub fn let_form(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Option<Expr> {
    let mut cur = cx.clone();
    let mut binds = Vec::new();
    for _ in 0..g.rng.range(1, 3) {
        let t = if g.rng.chance(30) {
            mutable_type(g, &cur)
        } else {
            g.any_type(&cur)
        };
        let init = g.expr(&cur, &t, d - 1);
        let destructure = if matches!(t, Ty::Vec(_)) { 35 } else { 20 };
        let (pat, vars) = if g.rng.chance(destructure) {
            irrefutable(g, &t, 2)
        } else {
            let taken: Vec<String> = binds
                .iter()
                .flat_map(|(p, _): &(Pat, Expr)| pat_names(p))
                .collect();
            let name = shadow_or_fresh(g, cx, &taken);
            let mut v = var(&name, &t, VarKind::Let);
            v.send_fn = t.is_fn() && is_send_fn(&init, &cur);
            (Pat::Bind(name), vec![v])
        };
        cur = cur.with_all(vars);
        binds.push((pat, init));
    }
    let body = g.expr(&cur, ty, d - 1);
    Some(Expr::new(ty.clone(), Kind::Let(binds, Box::new(body))))
}

/// A cell or atom type (atoms only where they may be read): bound by a
/// `let`, it is what `set!`, `push!`, `set-field!`, `&` calls, `swap!`
/// and `reset!` act on.
fn mutable_type(g: &mut Gen, cx: &Ctx) -> Ty {
    let mut ts = vec![
        Ty::cell(Ty::Int),
        Ty::cell(Ty::vec(Ty::Int)),
        Ty::cell(Ty::Pt),
        Ty::cell(Ty::Wrap),
        Ty::cell(Ty::Str),
        Ty::cell(Ty::fn_i()),
    ];
    if cx.atoms_readable() {
        ts.push(Ty::atom(Ty::Int));
        ts.push(Ty::atom(Ty::vec(Ty::Int)));
    }
    ts[g.rng.below(ts.len())].clone()
}

/// A fresh name, or now and then the name of a variable in scope that is
/// not an `&` parameter, which the new binding shadows.
fn shadow_or_fresh(g: &mut Gen, cx: &Ctx, taken: &[String]) -> String {
    let olds: Vec<&Var> = cx
        .vars
        .iter()
        .filter(|v| v.kind != VarKind::InOut && !taken.contains(&v.name))
        .collect();
    if g.rng.chance(8) {
        if let Some(v) = g.rng.pick(&olds) {
            return v.name.clone();
        }
    }
    g.fresh("x")
}

/// The names a pattern binds.
pub fn pat_names(p: &Pat) -> Vec<String> {
    match p {
        Pat::Bind(n) => vec![n.clone()],
        Pat::As(q, n) => pat_names(q).into_iter().chain([n.clone()]).collect(),
        Pat::Some(q) => pat_names(q),
        Pat::Ctor(_, ps) => ps.iter().flat_map(pat_names).collect(),
        Pat::Vector(ps, r) => {
            let mut out: Vec<String> = ps.iter().flat_map(pat_names).collect();
            if let Some(Rest::Bind(n)) = r {
                out.push(n.clone());
            }
            out
        }
        Pat::Wild | Pat::Nil | Pat::Lit(_) => Vec::new(),
    }
}

/// `(if c t e)`.
pub fn if_form(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Option<Expr> {
    let c = g.expr(cx, &Ty::Bool, d - 1);
    let t = g.expr(cx, ty, d - 1);
    let e = g.expr(cx, ty, d - 1);
    Some(Expr::new(
        ty.clone(),
        Kind::If(Box::new(c), Box::new(t), Box::new(e)),
    ))
}

/// `(do step ... last)`: statements, discarded values and fire-and-forget
/// spawns, then a value of `ty`.
pub fn do_form(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Option<Expr> {
    if g.rng.chance(12) {
        return Some(mcalls::seq(g, cx, ty, d - 1));
    }
    let mut steps = Vec::new();
    for _ in 0..g.rng.range(1, 3) {
        let s = match g.rng.below(5) {
            0 | 1 => effects::stmt(g, cx, d - 1),
            2 => tasks::spawn_discarded(g, cx, d - 1),
            _ => {
                let t = g.any_type(cx);
                g.expr(cx, &t, d - 1)
            }
        };
        steps.push(s);
    }
    steps.push(g.expr(cx, ty, d - 1));
    Some(Expr::new(ty.clone(), Kind::Do(steps)))
}

/// `(match e clause ...)` over a random scrutinee type, the clauses
/// binding parts of the scrutinee.
pub fn match_form(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Option<Expr> {
    match g.rng.below(20) {
        0..=6 => return Some(vpat::vector_match(g, cx, ty, d)),
        7..=9 => return Some(vpat::guarded_match(g, cx, ty, d)),
        _ => {}
    }
    let st = [
        Ty::Shape,
        Ty::opt(Ty::Int),
        Ty::opt(Ty::Str),
        Ty::opt(Ty::Wrap),
        Ty::List,
        Ty::Pt,
        Ty::Wrap,
        Ty::boxed(Ty::Wrap),
    ][g.rng.below(8)]
    .clone();
    let s = g.expr(cx, &st, d - 1);
    let pats = clause_patterns(g, &st);
    let mut clauses = Vec::new();
    for (p, vars) in pats {
        let body = g.expr(&cx.with_all(vars), ty, d - 1);
        clauses.push((p, body));
    }
    Some(Expr::new(ty.clone(), Kind::Match(Box::new(s), clauses)))
}

/// Exhaustive, non-redundant clause patterns for `st` (syntax §3.6).
pub fn clause_patterns(g: &mut Gen, st: &Ty) -> Vec<(Pat, Vec<Var>)> {
    let ctor = |g: &mut Gen, head: &str, ftys: &[Ty]| {
        let mut pats = Vec::new();
        let mut vars = Vec::new();
        for t in ftys {
            let (p, vs) = irrefutable(g, t, 1);
            pats.push(p);
            vars.extend(vs);
        }
        (Pat::Ctor(head.into(), pats), vars)
    };
    let mut out = match st {
        Ty::Shape => vec![
            ctor(g, "Circle", &[Ty::Int]),
            ctor(g, "Rect", &[Ty::Pt, Ty::Pt]),
            ctor(g, "Named", &[Ty::Str, Ty::Wrap]),
        ],
        Ty::Opt(t) => {
            let (p, vs) = irrefutable(g, t, 2);
            vec![(Pat::Some(Box::new(p)), vs), (Pat::Nil, Vec::new())]
        }
        Ty::List => {
            let (h, t) = (g.fresh("h"), g.fresh("t"));
            let vars = vec![
                var(&h, &Ty::Int, VarKind::Pattern),
                var(&t, &Ty::List, VarKind::Pattern),
            ];
            let cons = Pat::Ctor("cons".into(), vec![Pat::Bind(h), Pat::Bind(t)]);
            vec![
                (cons, vars),
                (Pat::Ctor("empty".into(), Vec::new()), Vec::new()),
            ]
        }
        _ => vec![irrefutable(g, st, 2)],
    };
    if out.len() > 1 && g.rng.chance(30) {
        out.truncate(out.len() - 1);
        out.push((Pat::Wild, Vec::new()));
    }
    if out.len() > 1 && g.rng.chance(50) {
        let last_is_wild = matches!(out.last(), Some((Pat::Wild, _)));
        if !last_is_wild {
            out.reverse();
        }
    }
    out
}

/// `(loop ((i 0) (acc init)) (if (< i k) (recur (+ i 1) step) last))`,
/// or for `unit` a counted loop of statements; `await`s itself inside an
/// `async` body (proposed case 32).
pub fn loop_form(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Option<Expr> {
    let i = g.fresh("i");
    let k = Expr::int(g.rng.range(0, 4));
    let iv = Expr::var(&i, Ty::Int);
    let test = Expr::call(Ty::Bool, "<", vec![iv.clone(), k]);
    let next = Expr::call(Ty::Int, "+", vec![iv, Expr::int(1)]);
    let mut inner = cx.with(var(&i, &Ty::Int, VarKind::Loop));
    let mut binds = vec![(i, Expr::int(0))];
    let (recur_args, last) = if *ty == Ty::Unit {
        (vec![next], Expr::new(Ty::Unit, Kind::Unit))
    } else {
        let acc = g.fresh("acc");
        binds.push((acc.clone(), g.expr(cx, ty, d - 1)));
        inner = inner.with(var(&acc, ty, VarKind::Loop));
        let step = g.expr(&inner, ty, d - 1);
        let last = if g.rng.chance(60) {
            Expr::var(&acc, ty.clone())
        } else {
            g.expr(&inner, ty, d - 1)
        };
        (vec![next, step], last)
    };
    let mut recur = Expr::new(ty.clone(), Kind::Recur(recur_args));
    if *ty == Ty::Unit || g.rng.chance(30) {
        let s = effects::stmt(g, &inner, d - 1);
        recur = Expr::new(ty.clone(), Kind::Do(vec![s, recur]));
    }
    let mut body = Expr::new(
        ty.clone(),
        Kind::If(Box::new(test), Box::new(recur), Box::new(last)),
    );
    if cx.in_async && g.rng.chance(50) {
        let y = tasks::await_yield();
        body = Expr::new(ty.clone(), Kind::Do(vec![y, body]));
    }
    Some(Expr::new(ty.clone(), Kind::Loop(binds, Box::new(body))))
}
