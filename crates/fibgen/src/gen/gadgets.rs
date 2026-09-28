//! Templates for the situations where lexical scope alone does not
//! decide when memory is freed (cases 01–11 and the proposed cases),
//! each with random holes: the object type, the values, the nesting.
//! The random grammar reaches all of them too, but rarely combined.

use crate::ast::{Arg, Expr, FunDef, Kind, Param, Pat};
use crate::ty::Ty;

use super::{objects, observe, tasks, Ctx, Gen, Region, Var, VarKind};

/// The object types the templates are instantiated at.
fn object_type(g: &mut Gen, send: bool) -> Ty {
    let ts = [
        Ty::Wrap,
        Ty::Str,
        Ty::vec(Ty::Int),
        Ty::Pt,
        Ty::Shape,
        Ty::boxed(Ty::Str),
        Ty::opt(Ty::Wrap),
        Ty::vec(Ty::Wrap),
        Ty::Holder,
        Ty::fn_i(),
    ];
    loop {
        let t = ts[g.rng.below(ts.len())].clone();
        if !send || t.is_send() {
            return t;
        }
    }
}

fn var(name: &str, ty: &Ty) -> Var {
    Var::new(name.to_string(), ty.clone(), VarKind::Let)
}

fn param(name: &str, ty: &Ty) -> Param {
    Param {
        name: name.to_string(),
        ty: ty.clone(),
        inout: false,
    }
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

/// Adds a helper and returns its name.
fn define(
    g: &mut Gen,
    prefix: &str,
    params: Vec<Param>,
    ret: Ty,
    body: impl FnOnce(&mut Gen, &str, &Ctx) -> Expr,
) -> String {
    let name = g.fresh(prefix);
    let cx = g.body_ctx(&params);
    let body = body(g, &name, &cx);
    g.funs.push(FunDef {
        name: name.clone(),
        params,
        ret,
        body,
    });
    name
}

/// An `i64` from one of the templates.
pub fn gadget(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    // Templates that define a helper stop once a program has plenty.
    let defines = g.funs.len() < 2 * g.max_funs;
    let e = match g.rng.below(11) {
        0 => snapshot(g, cx, d),
        1 => recur_outer(g, cx, d),
        2 => struct_closure(g, cx, d),
        3 => task_capture(g, cx, d),
        _ if !defines => return None,
        4 => spin(g, cx, d),
        5 => stash(g, cx, d),
        6 => pick(g, cx, d),
        7 => capture_param(g, cx, d),
        8 => whole_pattern(g, cx, d),
        9 => inout_snapshot(g, cx, d),
        _ => loop_param(g, cx, d),
    };
    Some(e)
}

/// `(let ((c (cell a))) (let ((s @c)) (do (set! c b) (+ fold(s) fold(@c)))))`:
/// a value read from a cell outlives the write that replaces it (case 66).
fn snapshot(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let t = object_type(g, false);
    let (c, s) = (g.fresh("c"), g.fresh("snap"));
    let ct = Ty::cell(t.clone());
    let init = Expr::call(
        ct.clone(),
        "cell",
        vec![g.expr(&objects::cell_content_ctx(cx, &t), &t, d - 1)],
    );
    let inner = cx.with(var(&c, &ct)).with(var(&s, &t));
    let newv = g.expr(&objects::cell_content_ctx(&inner, &t), &t, d - 1);
    let set = Expr::new(
        Ty::Unit,
        Kind::Set(Box::new(Expr::var(&c, ct.clone())), Box::new(newv)),
    );
    let a = observe::observe(g, &inner, Expr::var(&s, t.clone()), d);
    let b = observe::observe(g, &inner, objects::deref(&t, Expr::var(&c, ct.clone())), d);
    let body = Expr::new(Ty::Int, Kind::Do(vec![set, plus(a, b)]));
    let read = objects::deref(&t, Expr::var(&c, ct));
    let1(&c, init, let1(&s, read, body))
}

/// `(let ((y a)) (loop ((b c) (i 0)) (if (< i k) (recur y (+ i 1)) fold(b))))`:
/// `recur` retains a binding from outside the loop (case 63).
fn recur_outer(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let t = object_type(g, false);
    let (y, b, i) = (g.fresh("y"), g.fresh("b"), g.fresh("i"));
    let yinit = g.expr(cx, &t, d - 1);
    // Sometimes the loop starts from the outer binding itself (case 76).
    let binit = if g.rng.chance(35) {
        Expr::var(&y, t.clone())
    } else {
        g.expr(cx, &t, d - 1)
    };
    let inner = cx
        .with(var(&y, &t))
        .with(var(&b, &t))
        .with(var(&i, &Ty::Int));
    let iv = Expr::var(&i, Ty::Int);
    let next_b = if g.rng.chance(50) {
        Expr::var(&y, t.clone())
    } else {
        let c = g.expr(&inner, &Ty::Bool, d - 1);
        Expr::new(
            t.clone(),
            Kind::If(
                Box::new(c),
                Box::new(Expr::var(&y, t.clone())),
                Box::new(Expr::var(&b, t.clone())),
            ),
        )
    };
    let rec = Expr::new(
        Ty::Int,
        Kind::Recur(vec![next_b, plus(iv.clone(), Expr::int(1))]),
    );
    let test = Expr::call(Ty::Bool, "<", vec![iv, Expr::int(g.rng.range(0, 3))]);
    let done = observe::observe(g, &inner, Expr::var(&b, t.clone()), d);
    let body = Expr::new(
        Ty::Int,
        Kind::If(Box::new(test), Box::new(rec), Box::new(done)),
    );
    let lp = Expr::new(
        Ty::Int,
        Kind::Loop(vec![(b, binit), (i, Expr::int(0))], Box::new(body)),
    );
    let1(&y, yinit, lp)
}

/// `(defun spin (g n) (let ((x ..)) (if (= n 0) (g) (spin (fn () fold(x)) (- n 1)))))`:
/// a closure over a local passed at a tail call (case 49).
fn spin(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let t = object_type(g, false);
    let params = vec![param("f", &Ty::fn_0()), param("n", &Ty::Int)];
    let name = define(g, "spin", params, Ty::Int, |g, name, pcx| {
        let x = g.fresh("x");
        let xinit = g.expr(pcx, &t, 2);
        let inner = pcx.with(var(&x, &t));
        let folded = observe::observe(g, &inner.for_closure(true), Expr::var(&x, t.clone()), 2);
        let clo = Expr::new(Ty::fn_0(), Kind::Fn(Vec::new(), Box::new(folded)));
        let n = Expr::var("n", Ty::Int);
        let rec = Expr::call(
            Ty::Int,
            name,
            vec![clo, Expr::call(Ty::Int, "-", vec![n.clone(), Expr::int(1)])],
        );
        let call_f = Expr::new(
            Ty::Int,
            Kind::Apply(Box::new(Expr::var("f", Ty::fn_0())), Vec::new()),
        );
        let test = Expr::call(Ty::Bool, "<=", vec![n, Expr::int(0)]);
        let body = Expr::new(
            Ty::Int,
            Kind::If(Box::new(test), Box::new(call_f), Box::new(rec)),
        );
        let1(&x, xinit, body)
    });
    let f0 = g.expr(cx, &Ty::fn_0(), d - 1);
    Expr::call(Ty::Int, &name, vec![f0, Expr::int(g.rng.range(0, 4))])
}

/// `(defun stash (c b) (do (set! c b) (set! c ..) fold(b)))`: an owned
/// parameter stored in a cell, the cell overwritten, the parameter read
/// after (proposed cases 72, 73).
fn stash(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let t = object_type(g, false);
    let ct = Ty::cell(t.clone());
    let params = vec![param("c", &ct), param("b", &t)];
    let name = define(g, "stash", params, Ty::Int, |g, _, pcx| {
        let content = objects::cell_content_ctx(pcx, &t);
        let set1 = Expr::new(
            Ty::Unit,
            Kind::Set(
                Box::new(Expr::var("c", ct.clone())),
                Box::new(Expr::var("b", t.clone())),
            ),
        );
        let other = g.expr(&content, &t, 2);
        let set2 = Expr::new(
            Ty::Unit,
            Kind::Set(Box::new(Expr::var("c", ct.clone())), Box::new(other)),
        );
        let folded = observe::observe(g, pcx, Expr::var("b", t.clone()), 2);
        Expr::new(Ty::Int, Kind::Do(vec![set1, set2, folded]))
    });
    let content = objects::cell_content_ctx(cx, &t);
    let c = Expr::call(ct, "cell", vec![g.expr(&content, &t, d - 1)]);
    let b = g.expr(&content, &t, d - 1);
    Expr::call(Ty::Int, &name, vec![c, b])
}

/// `(defun pick (flag x) (if flag x fresh))` called twice on one binding:
/// a join of a borrow and a fresh value (case 04).
fn pick(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let t = object_type(g, false);
    let params = vec![param("flag", &Ty::Bool), param("x", &t)];
    let name = define(g, "pick", params, t.clone(), |g, _, pcx| {
        let fresh = g.expr(pcx, &t, 2);
        let flag = Expr::var("flag", Ty::Bool);
        Expr::new(
            t.clone(),
            Kind::If(
                Box::new(flag),
                Box::new(Expr::var("x", t.clone())),
                Box::new(fresh),
            ),
        )
    });
    let (s, a, b) = (g.fresh("s"), g.fresh("a"), g.fresh("b"));
    let sinit = g.expr(cx, &t, d - 1);
    let call = |flag: bool| {
        let f = Expr::new(Ty::Bool, Kind::Bool(flag));
        Expr::call(t.clone(), &name, vec![f, Expr::var(&s, t.clone())])
    };
    let inner = cx.with(var(&s, &t)).with(var(&a, &t)).with(var(&b, &t));
    let fa = observe::observe(g, &inner, Expr::var(&a, t.clone()), d);
    let fb = observe::observe(g, &inner, Expr::var(&b, t.clone()), d);
    let body = Expr::new(
        Ty::Int,
        Kind::Let(
            vec![(Pat::Bind(a), call(true)), (Pat::Bind(b), call(false))],
            Box::new(plus(fa, fb)),
        ),
    );
    let1(&s, sinit, body)
}

/// `(let ((m (let ((p ..)) (mk p)))) (m k))`: an escaping closure over a
/// borrowed parameter whose lender dies first (case 06).
fn capture_param(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let t = object_type(g, false);
    let name = define(g, "mk", vec![param("p", &t)], Ty::fn_i(), |g, _, pcx| {
        let k = g.fresh("k");
        let inner = pcx
            .for_closure(true)
            .with(Var::new(k.clone(), Ty::Int, VarKind::Param));
        let folded = observe::observe(g, &inner, Expr::var("p", t.clone()), 2);
        let body = plus(Expr::var(&k, Ty::Int), folded);
        Expr::new(Ty::fn_i(), Kind::Fn(vec![(k, Ty::Int)], Box::new(body)))
    });
    let (m, p) = (g.fresh("m"), g.fresh("p"));
    let pinit = g.expr(cx, &t, d - 1);
    let made = let1(
        &p,
        pinit,
        Expr::call(Ty::fn_i(), &name, vec![Expr::var(&p, t.clone())]),
    );
    let call = Expr::new(
        Ty::Int,
        Kind::Apply(
            Box::new(Expr::var(&m, Ty::fn_i())),
            vec![Expr::int(g.small())],
        ),
    );
    let1(&m, made, call)
}

/// `(defun same (p) (match p (w w)))`: a parameter returned through a
/// whole-object pattern variable (case 22).
fn whole_pattern(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let t = object_type(g, false);
    let name = define(g, "same", vec![param("p", &t)], t.clone(), |g, _, _| {
        let w = g.fresh("w");
        let clause = (Pat::Bind(w.clone()), Expr::var(&w, t.clone()));
        Expr::new(
            t.clone(),
            Kind::Match(Box::new(Expr::var("p", t.clone())), vec![clause]),
        )
    });
    let (r, s) = (g.fresh("r"), g.fresh("s"));
    let sinit = g.expr(cx, &t, d - 1);
    let made = let1(
        &s,
        sinit,
        Expr::call(t.clone(), &name, vec![Expr::var(&s, t.clone())]),
    );
    let inner = cx.with(var(&r, &t));
    let folded = observe::observe(g, &inner, Expr::var(&r, t.clone()), d);
    let1(&r, made, folded)
}

/// `(defun swap-out (&v) (let ((snap @v)) (do (set! v ..) fold(snap))))`:
/// a read through an `&` parameter outlives the write (case 78).
fn inout_snapshot(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let contents = crate::ty::inout_contents();
    let t = contents[g.rng.below(contents.len())].clone();
    let params = vec![Param {
        name: "v".into(),
        ty: t.clone(),
        inout: true,
    }];
    let name = define(g, "swapout", params, Ty::Int, |g, _, pcx| {
        let snap = g.fresh("snap");
        let vt = Ty::cell(t.clone());
        let read = objects::deref(&t, Expr::var("v", vt.clone()));
        let inner = pcx.with(var(&snap, &t));
        let newv = g.expr(&inner, &t, 2);
        let set = Expr::new(
            Ty::Unit,
            Kind::Set(Box::new(Expr::var("v", vt)), Box::new(newv)),
        );
        let folded = observe::observe(g, &inner, Expr::var(&snap, t.clone()), 2);
        let1(&snap, read, Expr::new(Ty::Int, Kind::Do(vec![set, folded])))
    });
    let c = g.fresh("c");
    let ct = Ty::cell(t.clone());
    let init = Expr::call(ct.clone(), "cell", vec![g.expr(cx, &t, d - 1)]);
    let call = Expr::new(Ty::Int, Kind::Call(name, vec![Arg::InOut(c.clone())]));
    let inner = cx.with(var(&c, &ct));
    let after = observe::observe(g, &inner, objects::deref(&t, Expr::var(&c, ct)), d);
    let1(&c, init, plus(call, after))
}

/// `(let ((t (let ((b ..)) (Holder (fn (k) .. b ..) (cell 0))))) ((. t f) 1))`:
/// a closure over a scope-local object stored in a struct that outlives
/// the scope (case 56).
fn struct_closure(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let t = object_type(g, false);
    let (h, b, k) = (g.fresh("h"), g.fresh("b"), g.fresh("k"));
    let binit = g.expr(cx, &t, d - 1);
    let inner =
        cx.with(var(&b, &t))
            .for_closure(true)
            .with(Var::new(k.clone(), Ty::Int, VarKind::Param));
    let folded = observe::observe(g, &inner, Expr::var(&b, t.clone()), d);
    let clo = Expr::new(
        Ty::fn_i(),
        Kind::Fn(
            vec![(k.clone(), Ty::Int)],
            Box::new(plus(Expr::var(&k, Ty::Int), folded)),
        ),
    );
    let cellv = Expr::call(Ty::cell(Ty::Int), "cell", vec![Expr::int(g.small())]);
    let holder = Expr::call(Ty::Holder, "Holder", vec![clo, cellv]);
    let made = let1(&b, binit, holder);
    let f = Expr::new(
        Ty::fn_i(),
        Kind::Field(Box::new(Expr::var(&h, Ty::Holder)), "f".into()),
    );
    let call = Expr::new(
        Ty::Int,
        Kind::Apply(Box::new(f), vec![Expr::int(g.small())]),
    );
    let1(&h, made, call)
}

/// A task made from a scope-local value that dies before the task runs:
/// `async` (case 11) or `spawn`.
fn task_capture(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let t = object_type(g, true);
    let (task, s) = (g.fresh("task"), g.fresh("s"));
    let sinit = g.expr(cx, &t, d - 1);
    let inner = cx.with(var(&s, &t));
    let made = if g.rng.chance(50) {
        let tcx = inner.for_region(Region::Task, true);
        let folded = observe::observe(g, &tcx, Expr::var(&s, t.clone()), d);
        let body = Expr::new(Ty::Int, Kind::Do(vec![tasks::await_yield(), folded]));
        Expr::new(Ty::task(Ty::Int), Kind::Async(Box::new(body)))
    } else {
        let tcx = inner.for_region(Region::Task, false).for_closure(true);
        let folded = observe::observe(g, &tcx, Expr::var(&s, t.clone()), d);
        let clo = Expr::new(Ty::fn_0(), Kind::Fn(Vec::new(), Box::new(folded)));
        Expr::call(Ty::task(Ty::Int), "spawn", vec![clo])
    };
    let join = Expr::call(
        Ty::Int,
        "block-on",
        vec![Expr::var(&task, Ty::task(Ty::Int))],
    );
    let1(&task, let1(&s, sinit, made), join)
}

/// `(defun last-or (default xs) (loop ((best default) (i 0)) ..))`: a loop
/// that returns its parameter (case 75).
fn loop_param(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let t = object_type(g, false);
    let vt = Ty::vec(t.clone());
    let params = vec![param("dflt", &t), param("xs", &vt)];
    let name = define(g, "lastor", params, t.clone(), |g, _, _| {
        let (best, i) = (g.fresh("best"), g.fresh("i"));
        let iv = Expr::var(&i, Ty::Int);
        let xs = Expr::var("xs", vt.clone());
        let test = Expr::call(
            Ty::Bool,
            "<",
            vec![iv.clone(), Expr::call(Ty::Int, "count", vec![xs.clone()])],
        );
        let elem = Expr::call(t.clone(), "nth", vec![xs, iv.clone()]);
        let rec = Expr::new(t.clone(), Kind::Recur(vec![elem, plus(iv, Expr::int(1))]));
        let body = Expr::new(
            t.clone(),
            Kind::If(
                Box::new(test),
                Box::new(rec),
                Box::new(Expr::var(&best, t.clone())),
            ),
        );
        Expr::new(
            t.clone(),
            Kind::Loop(
                vec![(best, Expr::var("dflt", t.clone())), (i, Expr::int(0))],
                Box::new(body),
            ),
        )
    });
    let a = g.expr(cx, &t, d - 1);
    let b = g.expr(cx, &vt, d - 1);
    let r = Expr::call(t.clone(), &name, vec![a, b]);
    observe::observe(g, cx, r, d)
}
