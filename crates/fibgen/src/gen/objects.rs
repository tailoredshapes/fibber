//! Productions for the object types: structs, the enum, boxes, options,
//! vectors, lists, cells and atoms, and extraction of parts of them.

use crate::ast::{Expr, Kind, Pat};
use crate::ty::Ty;

use super::{funcs, protos, tasks, vpat, Ctx, Gen, Var, VarKind};

/// The struct fields: (struct, field, field type).
fn fields() -> Vec<(Ty, &'static str, Ty)> {
    vec![
        (Ty::Pt, "x", Ty::Int),
        (Ty::Pt, "y", Ty::Int),
        (Ty::Wrap, "s", Ty::Str),
        (Ty::Wrap, "v", Ty::vec(Ty::Int)),
        (Ty::Holder, "f", Ty::fn_i()),
        (Ty::Holder, "c", Ty::cell(Ty::Int)),
    ]
}

/// An expression of an object type, or `None` to fall back to a leaf.
pub fn object(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Option<Expr> {
    if g.rng.chance(25) {
        if let Some(e) = extract(g, cx, ty, d) {
            return Some(e);
        }
    }
    let sub = d - 1;
    let e = match ty {
        Ty::Pt => ctor(g, cx, ty, "Pt", &[Ty::Int, Ty::Int], sub),
        Ty::Wrap => ctor(g, cx, ty, "Wrap", &[Ty::Str, Ty::vec(Ty::Int)], sub),
        Ty::Holder => ctor(g, cx, ty, "Holder", &[Ty::fn_i(), Ty::cell(Ty::Int)], sub),
        Ty::Shape => shape(g, cx, sub),
        Ty::Boxed(t) => {
            let head = if g.rng.chance(70) { "Box" } else { "box" };
            ctor(g, cx, ty, head, &[(**t).clone()], sub)
        }
        Ty::Opt(t) if g.rng.chance(75) => ctor(g, cx, ty, "some", &[(**t).clone()], sub),
        Ty::Opt(_) => empty(g, cx, ty),
        Ty::Vec(t) => vector(g, cx, t, sub),
        Ty::List => list(g, cx, sub),
        Ty::Cell(t) => {
            let inner = cell_content_ctx(cx, t);
            let v = g.expr(&inner, t, sub);
            Expr::call(ty.clone(), "cell", vec![v])
        }
        Ty::Atom(t) => ctor(g, cx, ty, "atom", &[(**t).clone()], sub),
        _ => return None,
    };
    Some(e)
}

/// The context for a value about to be stored in a cell of content `t`:
/// if such a value could reach a cell, existing values that could are
/// excluded, so no cycle through the cell can form.
pub fn cell_content_ctx(cx: &Ctx, t: &Ty) -> Ctx {
    let mut c = cx.clone();
    if t.may_reach_cell() {
        c.cell_free = true;
    }
    c
}

fn ctor(g: &mut Gen, cx: &Ctx, ty: &Ty, head: &str, args: &[Ty], d: u32) -> Expr {
    let args = args.iter().map(|t| g.expr(cx, t, d)).collect();
    Expr::call(ty.clone(), head, args)
}

fn shape(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    match g.rng.below(3) {
        0 => ctor(g, cx, &Ty::Shape, "Circle", &[Ty::Int], d),
        1 => ctor(g, cx, &Ty::Shape, "Rect", &[Ty::Pt, Ty::Pt], d),
        _ => ctor(g, cx, &Ty::Shape, "Named", &[Ty::Str, Ty::Wrap], d),
    }
}

fn vector(g: &mut Gen, cx: &Ctx, t: &Ty, d: u32) -> Expr {
    let vt = Ty::vec(t.clone());
    match g.rng.below(7) {
        6 => vpat::rest_match(g, cx, t, d + 1),
        0 | 1 => {
            let n = g.rng.below(4);
            if n == 0 {
                return empty(g, cx, &vt);
            }
            let items = (0..n).map(|_| g.expr(cx, t, d)).collect();
            Expr::new(vt, Kind::VecLit(items))
        }
        2 | 3 => {
            let v = g.expr(cx, &vt, d);
            let x = g.expr(cx, t, d);
            Expr::call(vt, "conj", vec![v, x])
        }
        4 if *t == Ty::Int => {
            if g.rng.chance(50) {
                Expr::call(vt, "range", vec![Expr::int(g.rng.range(0, 5))])
            } else {
                funcs::map_form(g, cx, d)
            }
        }
        5 if *t == Ty::Int => tasks::pmap(g, cx, d),
        _ => match extract(g, cx, &vt, d + 1) {
            Some(e) => e,
            None => empty(g, cx, &vt),
        },
    }
}

fn list(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    match g.rng.below(4) {
        0 => empty(g, cx, &Ty::List),
        1 => {
            let n = g.rng.range(1, 3) as usize;
            let items = (0..n).map(|_| g.expr(cx, &Ty::Int, d)).collect();
            Expr::call(Ty::List, "list", items)
        }
        2 => {
            let h = g.expr(cx, &Ty::Int, d);
            let t = g.expr(cx, &Ty::List, d);
            Expr::call(Ty::List, "cons", vec![h, t])
        }
        _ => {
            let l = g.expr(cx, &Ty::List, d);
            let x = g.expr(cx, &Ty::Int, d);
            Expr::call(Ty::List, "conj", vec![l, x])
        }
    }
}

/// A value of type `ty` taken out of a container: a field, a box, a
/// cell, an atom, a vector element or an option's payload.
pub fn extract(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Option<Expr> {
    let sub = d.saturating_sub(1);
    let mut options: Vec<u8> = Vec::new();
    let flds: Vec<_> = fields().into_iter().filter(|(_, _, t)| t == ty).collect();
    for (tag, wrap) in [
        (0, Ty::boxed(ty.clone())),
        (2, Ty::cell(ty.clone())),
        (3, Ty::atom(ty.clone())),
        (4, Ty::vec(ty.clone())),
        (5, Ty::opt(ty.clone())),
    ] {
        let atom_ok = tag != 3 || cx.atoms_readable();
        if g.in_universe(&wrap) && atom_ok {
            options.push(tag);
        }
    }
    if !flds.is_empty() {
        options.push(1);
    }
    let tag = *g.rng.pick(&options)?;
    let e = match tag {
        0 => Expr::call(
            ty.clone(),
            "unbox",
            vec![g.expr(cx, &Ty::boxed(ty.clone()), sub)],
        ),
        1 => {
            let (st, f, _) = flds[g.rng.below(flds.len())].clone();
            let s = g.expr(cx, &st, sub);
            Expr::new(ty.clone(), Kind::Field(Box::new(s), f.to_string()))
        }
        2 => deref(ty, g.expr(cx, &Ty::cell(ty.clone()), sub)),
        3 => deref(ty, g.expr(cx, &Ty::atom(ty.clone()), sub)),
        4 => safe_nth(g, cx, ty, sub),
        _ => {
            let o = g.expr(cx, &Ty::opt(ty.clone()), sub);
            let dflt = g.expr(cx, ty, sub);
            Expr::call(ty.clone(), "unwrap-or", vec![o, dflt])
        }
    };
    Some(e)
}

/// `@e` of type `ty`.
pub fn deref(ty: &Ty, e: Expr) -> Expr {
    Expr::new(ty.clone(), Kind::Deref(Box::new(e)))
}

/// `(let ((v vec)) (if (< k (count v)) (nth v k) default))`: never traps.
fn safe_nth(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Expr {
    let vt = Ty::vec(ty.clone());
    let v = g.fresh("v");
    let init = g.expr(cx, &vt, d);
    let inner = cx.with(Var::new(v.clone(), vt.clone(), VarKind::Let));
    let k = Expr::int(g.rng.range(0, 2));
    let count = Expr::call(Ty::Int, "count", vec![Expr::var(&v, vt.clone())]);
    let test = Expr::call(Ty::Bool, "<", vec![k.clone(), count]);
    let nth = Expr::call(ty.clone(), "nth", vec![Expr::var(&v, vt), k]);
    let dflt = g.expr(&inner, ty, d);
    let body = Expr::new(
        ty.clone(),
        Kind::If(Box::new(test), Box::new(nth), Box::new(dflt)),
    );
    Expr::new(
        ty.clone(),
        Kind::Let(vec![(Pat::Bind(v), init)], Box::new(body)),
    )
}

/// The smallest constructed value of an object type.
pub fn construct_leaf(g: &mut Gen, cx: &Ctx, ty: &Ty) -> Expr {
    let args: Vec<Expr> = match ty {
        Ty::Pt => vec![Expr::int(g.small()), Expr::int(g.small())],
        Ty::Wrap => vec![
            g.leaf_value(cx, &Ty::Str),
            g.leaf_value(cx, &Ty::vec(Ty::Int)),
        ],
        Ty::Holder => vec![funcs::const_fn(g, 1), g.leaf_value(cx, &Ty::cell(Ty::Int))],
        Ty::Shape => return Expr::call(Ty::Shape, "Circle", vec![Expr::int(g.small())]),
        Ty::Dyn(p, send) => return protos::dyn_leaf(g, *p, *send),
        Ty::Boxed(t) | Ty::Cell(t) | Ty::Atom(t) => vec![g.leaf_value(cx, t)],
        _ => return Expr::new(ty.clone(), Kind::Unit),
    };
    let head = match ty {
        Ty::Pt => "Pt",
        Ty::Wrap => "Wrap",
        Ty::Holder => "Holder",
        Ty::Boxed(_) => "Box",
        Ty::Cell(_) => "cell",
        _ => "atom",
    };
    Expr::call(ty.clone(), head, args)
}

/// An object made at run time, held by nothing else: the target of a
/// weak reference that must die with its `let` (case 20).
pub fn fresh_object(g: &mut Gen, cx: &Ctx, ty: &Ty) -> Expr {
    let lit = |s: &str| Expr::new(Ty::Str, Kind::Str(s.to_string()));
    let s = Expr::call(Ty::Str, "str-concat", vec![lit("ab"), lit("c")]);
    match ty {
        Ty::Str => s,
        Ty::Wrap => {
            let empty = Expr::new(Ty::vec(Ty::Int), Kind::VecLit(Vec::new()));
            let v = Expr::call(Ty::vec(Ty::Int), "conj", vec![empty, Expr::int(g.small())]);
            Expr::call(Ty::Wrap, "Wrap", vec![s, v])
        }
        Ty::Vec(_) => {
            let empty = Expr::new(ty.clone(), Kind::VecLit(Vec::new()));
            Expr::call(ty.clone(), "conj", vec![empty, Expr::int(g.small())])
        }
        _ => construct_leaf(g, cx, ty),
    }
}

/// `nil`, `empty` or `[]` of `ty`, written `(if false (some x) nil)`,
/// `(if false (list x) empty)` or `(if false [x] [])`, so that the type
/// is fixed by the expression
/// itself (the value is still empty): a pattern variable bound from it
/// and only compared with itself would otherwise be ambiguous.
pub fn empty(g: &mut Gen, cx: &Ctx, ty: &Ty) -> Expr {
    let int = Ty::Int;
    let (plain, inner, pin) = match ty {
        Ty::Opt(t) => (Kind::Nil, &**t, true),
        Ty::List => (Kind::Empty, &int, true),
        // Always pinned: a vector pattern binds its elements, which a
        // clause may only compare with each other.
        Ty::Vec(t) => (Kind::VecLit(Vec::new()), &**t, true),
        _ => return g.leaf_value(cx, ty),
    };
    let plain = Expr::new(ty.clone(), plain);
    if !pin {
        return plain;
    }
    let x = g.leaf_value(cx, inner);
    let full = match ty {
        Ty::Opt(_) => Expr::call(ty.clone(), "some", vec![x]),
        Ty::List => Expr::call(ty.clone(), "list", vec![x]),
        _ => Expr::new(ty.clone(), Kind::VecLit(vec![x])),
    };
    let no = Expr::new(Ty::Bool, Kind::Bool(false));
    Expr::new(
        ty.clone(),
        Kind::If(Box::new(no), Box::new(full), Box::new(plain)),
    )
}
