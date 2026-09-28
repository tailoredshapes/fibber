//! More ownership templates: in-place updates while a snapshot or an
//! iteration holds the old value, closures that escape through a cell,
//! one task joined or awaited by several holders, `&` forwarding through
//! mutual tail calls.

use crate::ast::{Arg, Expr, FunDef, Kind, Param, Pat};
use crate::ty::Ty;

use super::{objects, observe, tasks, Ctx, Gen, Region, Var, VarKind};

fn var(name: &str, ty: &Ty) -> Var {
    Var::new(name.to_string(), ty.clone(), VarKind::Let)
}

fn let1(name: &str, init: Expr, body: Expr) -> Expr {
    Expr::new(
        body.ty.clone(),
        Kind::Let(vec![(Pat::Bind(name.to_string()), init)], Box::new(body)),
    )
}

fn plus(a: Expr, b: Expr) -> Expr {
    Expr::call(Ty::Int, "+", vec![a, b])
}

fn do_(ty: Ty, steps: Vec<Expr>) -> Expr {
    Expr::new(ty, Kind::Do(steps))
}

fn push(cell: &str, x: Expr) -> Expr {
    Expr::new(
        Ty::Unit,
        Kind::Call("push!".into(), vec![Arg::InOut(cell.into()), Arg::Val(x)]),
    )
}

/// An `i64` from one of these templates.
pub fn gadget(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    let defines = g.funs.len() < 2 * g.max_funs;
    let e = match g.rng.below(7) {
        0 => iterate_push(g, cx, d),
        1 => shared_set_field(g, cx, d),
        2 => closure_via_cell(g, cx, d),
        3 => shared_task(g, cx, d),
        4 => counted_push(g, cx, d),
        5 if defines => walk(g, cx, d),
        _ if defines => forward_mutual(g, cx, d),
        _ => return None,
    };
    Some(e)
}

/// `(let ((c (cell v))) (do (for-each @c (fn (x) (push! &c x))) (sum-vec @c)))`:
/// the iteration holds the vector the pushes replace (cases 08, 66).
fn iterate_push(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let vt = Ty::vec(Ty::Int);
    let ct = Ty::cell(vt.clone());
    let (c, x) = (g.fresh("c"), g.fresh("x"));
    let init = Expr::call(ct.clone(), "cell", vec![g.expr(cx, &vt, d - 1)]);
    let read = || objects::deref(&vt, Expr::var(&c, ct.clone()));
    let body = push(&c, Expr::var(&x, Ty::Int));
    let f = Expr::new(
        Ty::Func(vec![Ty::Int], Box::new(Ty::Unit)),
        Kind::Fn(vec![(x, Ty::Int)], Box::new(body)),
    );
    let each = Expr::call(Ty::Unit, "for-each", vec![read(), f]);
    let sum = Expr::call(Ty::Int, "sum-vec", vec![read()]);
    let1(&c, init, do_(Ty::Int, vec![each, sum]))
}

/// `(let ((c (cell w))) (let ((snap @c)) (do (set-field! &c v ..) (+ fold(snap) fold(@c)))))`:
/// a field update on a struct that a snapshot shares must copy.
fn shared_set_field(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let ct = Ty::cell(Ty::Wrap);
    let (c, snap) = (g.fresh("c"), g.fresh("snap"));
    let init = Expr::call(ct.clone(), "cell", vec![g.expr(cx, &Ty::Wrap, d - 1)]);
    let inner = cx.with(var(&c, &ct)).with(var(&snap, &Ty::Wrap));
    let snapv = Expr::var(&snap, Ty::Wrap);
    let old_v = Expr::new(
        Ty::vec(Ty::Int),
        Kind::Field(Box::new(snapv.clone()), "v".into()),
    );
    let newv = Expr::call(
        Ty::vec(Ty::Int),
        "conj",
        vec![old_v, g.expr(&inner, &Ty::Int, d - 1)],
    );
    let set = Expr::new(
        Ty::Unit,
        Kind::SetField(c.clone(), "v".into(), Box::new(newv)),
    );
    let a = observe::observe(g, &inner, snapv, d);
    let b = observe::observe(
        g,
        &inner,
        objects::deref(&Ty::Wrap, Expr::var(&c, ct.clone())),
        d,
    );
    let read = objects::deref(&Ty::Wrap, Expr::var(&c, ct));
    let1(
        &c,
        init,
        let1(&snap, read, do_(Ty::Int, vec![set, plus(a, b)])),
    )
}

/// `(let ((c (cell inc1))) (do (let ((x ..)) (set! c (fn (k) .. x ..))) (@c k)))`:
/// a closure over a scope-local value escapes its scope through a cell.
fn closure_via_cell(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let ts = [
        Ty::Wrap,
        Ty::Str,
        Ty::vec(Ty::Int),
        Ty::Shape,
        Ty::boxed(Ty::Str),
    ];
    let t = ts[g.rng.below(ts.len())].clone();
    let ct = Ty::cell(Ty::fn_i());
    let (c, x, k) = (g.fresh("c"), g.fresh("x"), g.fresh("k"));
    let init = Expr::call(
        ct.clone(),
        "cell",
        vec![Expr::new(Ty::fn_i(), Kind::Global("inc1".into()))],
    );
    let xinit = g.expr(&objects::cell_content_ctx(cx, &Ty::fn_i()), &t, d - 1);
    let mut body_cx = objects::cell_content_ctx(cx, &Ty::fn_i())
        .with(var(&x, &t))
        .for_closure(true);
    body_cx = body_cx.with(Var::new(k.clone(), Ty::Int, VarKind::Param));
    let folded = observe::observe(g, &body_cx, Expr::var(&x, t.clone()), d);
    let clo = Expr::new(
        Ty::fn_i(),
        Kind::Fn(
            vec![(k.clone(), Ty::Int)],
            Box::new(plus(Expr::var(&k, Ty::Int), folded)),
        ),
    );
    let set = Expr::new(
        Ty::Unit,
        Kind::Set(Box::new(Expr::var(&c, ct.clone())), Box::new(clo)),
    );
    let scoped = let1(&x, xinit, set);
    let f = objects::deref(&Ty::fn_i(), Expr::var(&c, ct));
    let call = Expr::new(
        Ty::Int,
        Kind::Apply(Box::new(f), vec![Expr::int(g.small())]),
    );
    let1(&c, init, do_(Ty::Int, vec![scoped, call]))
}

/// One task joined from two threads (case 43) or awaited by two tasks (case 44).
fn shared_task(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let tt = Ty::task(Ty::Int);
    let t = g.fresh("task");
    let tcx = cx.for_region(Region::Task, true);
    let body = g.expr(&tcx, &Ty::Int, d - 1);
    let made = Expr::new(
        tt.clone(),
        Kind::Async(Box::new(do_(Ty::Int, vec![tasks::await_yield(), body]))),
    );
    let tv = || Expr::var(&t, tt.clone());
    let (a, b) = (g.fresh("a"), g.fresh("b"));
    let use_ = if g.rng.chance(50) {
        let join = || Expr::call(Ty::Int, "join", vec![tv()]);
        let sum = plus(Expr::var(&a, Ty::Int), Expr::var(&b, Ty::Int));
        Expr::new(
            Ty::Int,
            Kind::Plet(vec![(a, join()), (b, join())], Box::new(sum)),
        )
    } else {
        let waiter = || {
            Expr::new(
                tt.clone(),
                Kind::Async(Box::new(Expr::new(Ty::Int, Kind::Await(Box::new(tv()))))),
            )
        };
        let bo = |n: &str| Expr::call(Ty::Int, "block-on", vec![Expr::var(n, tt.clone())]);
        let sum = plus(bo(&a), bo(&b));
        Expr::new(
            Ty::Int,
            Kind::Let(
                vec![
                    (Pat::Bind(a.clone()), waiter()),
                    (Pat::Bind(b.clone()), waiter()),
                ],
                Box::new(sum),
            ),
        )
    };
    let1(&t, made, use_)
}

/// `(let ((v (cell []))) (do (loop ((i 0)) (if (< i k) (do (push! &v i) (recur (+ i 1))) ())) (sum-vec @v)))`:
/// pushes in a loop on a cell the function holds alone (case 29).
fn counted_push(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let vt = Ty::vec(Ty::Int);
    let ct = Ty::cell(vt.clone());
    let (c, i) = (g.fresh("c"), g.fresh("i"));
    let init = Expr::call(ct.clone(), "cell", vec![g.expr(cx, &vt, d - 1)]);
    let iv = Expr::var(&i, Ty::Int);
    let test = Expr::call(
        Ty::Bool,
        "<",
        vec![iv.clone(), Expr::int(g.rng.range(0, 6))],
    );
    let rec = Expr::new(Ty::Unit, Kind::Recur(vec![plus(iv.clone(), Expr::int(1))]));
    let step = do_(Ty::Unit, vec![push(&c, iv), rec]);
    let body = Expr::new(
        Ty::Unit,
        Kind::If(
            Box::new(test),
            Box::new(step),
            Box::new(Expr::new(Ty::Unit, Kind::Unit)),
        ),
    );
    let lp = Expr::new(
        Ty::Unit,
        Kind::Loop(vec![(i, Expr::int(0))], Box::new(body)),
    );
    let sum = Expr::call(
        Ty::Int,
        "sum-vec",
        vec![objects::deref(&vt, Expr::var(&c, ct))],
    );
    let1(&c, init, do_(Ty::Int, vec![lp, sum]))
}

/// Two `&` helpers that tail-call each other, forwarding their `&`
/// parameter (syntax §3.13, forwarding at a tail call; case 25).
fn forward_mutual(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let vt = Ty::vec(Ty::Int);
    let (a, b) = (g.fresh("fwda"), g.fresh("fwdb"));
    let params = vec![param("v", &vt, true), param("n", &Ty::Int, false)];
    let (body_a, body_b) = (forward_body(&b, 0), forward_body(&a, 100));
    g.funs.push(fundef(&b, params.clone(), Ty::Unit, body_b));
    g.funs.push(fundef(&a, params, Ty::Unit, body_a));
    let c = g.fresh("c");
    let ct = Ty::cell(vt.clone());
    let init = Expr::call(ct.clone(), "cell", vec![g.expr(cx, &vt, d - 1)]);
    let n = Expr::int(g.rng.range(0, 5));
    let call = Expr::new(
        Ty::Unit,
        Kind::Call(a, vec![Arg::InOut(c.clone()), Arg::Val(n)]),
    );
    let read = objects::deref(&vt, Expr::var(&c, ct));
    let sum = Expr::call(Ty::Int, "sum-vec", vec![read]);
    let1(&c, init, do_(Ty::Int, vec![call, sum]))
}

/// `(if (<= n 0) () (do (push! &v (+ n x)) (callee &v (- n 1))))`.
fn forward_body(callee: &str, x: i64) -> Expr {
    let n = Expr::var("n", Ty::Int);
    let test = Expr::call(Ty::Bool, "<=", vec![n.clone(), Expr::int(0)]);
    let dec = Expr::call(Ty::Int, "-", vec![n.clone(), Expr::int(1)]);
    let args = vec![Arg::InOut("v".into()), Arg::Val(dec)];
    let rec = Expr::new(Ty::Unit, Kind::Call(callee.into(), args));
    let step = do_(Ty::Unit, vec![push("v", plus(n, Expr::int(x))), rec]);
    let unit = Expr::new(Ty::Unit, Kind::Unit);
    Expr::new(
        Ty::Unit,
        Kind::If(Box::new(test), Box::new(unit), Box::new(step)),
    )
}

fn param(name: &str, ty: &Ty, inout: bool) -> Param {
    Param {
        name: name.to_string(),
        ty: ty.clone(),
        inout,
    }
}

fn fundef(name: &str, params: Vec<Param>, ret: Ty, body: Expr) -> FunDef {
    FunDef {
        name: name.to_string(),
        params,
        ret,
        body,
    }
}

/// `(defun walk (b n) (let ((w (Ctor ..))) (if (<= n 0) fold(b) (walk w (- n 1)))))`:
/// an object the frame constructed, passed at a tail call, so it must be
/// a heap object whose count moves into the call (case 54).
fn walk(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let ts = [Ty::Wrap, Ty::Pt, Ty::Shape, Ty::boxed(Ty::Str), Ty::Holder];
    let t = ts[g.rng.below(ts.len())].clone();
    let name = g.fresh("walk");
    let params = vec![param("b", &t, false), param("n", &Ty::Int, false)];
    let pcx = g.body_ctx(&params);
    let w = g.fresh("w");
    let made = objects::object(g, &pcx, &t, 3).unwrap_or_else(|| g.leaf_value(&pcx, &t));
    let folded = observe::observe(g, &pcx, Expr::var("b", t.clone()), 2);
    let n = Expr::var("n", Ty::Int);
    let test = Expr::call(Ty::Bool, "<=", vec![n.clone(), Expr::int(0)]);
    let dec = Expr::call(Ty::Int, "-", vec![n, Expr::int(1)]);
    let rec = Expr::call(Ty::Int, &name, vec![Expr::var(&w, t.clone()), dec]);
    let body = let1(
        &w,
        made,
        Expr::new(
            Ty::Int,
            Kind::If(Box::new(test), Box::new(folded), Box::new(rec)),
        ),
    );
    g.funs.push(fundef(&name, params, Ty::Int, body));
    let start = g.expr(cx, &t, d - 1);
    Expr::call(Ty::Int, &name, vec![start, Expr::int(g.rng.range(0, 4))])
}
