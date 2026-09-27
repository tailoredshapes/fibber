//! Programs chosen to break the ownership pass (types §6), each with the
//! decision that keeps it sound asserted. Every program here is
//! accepted; what is checked is how, since a wrong decision shows only
//! as a use-after-free or a leak once an evaluator runs it.

use fibref::own::program::{
    Alloc, Because, BindKind, BodyKey, BodyOwn, CallOwn, ClosureOwn, OpKind, OwnedWhy, ParamKind,
    Pass, Site, Tail,
};
use fibref::own::{check_source, Checked};
use fibref::types::ast::{Expr, ExprKind, GlobalRef};
use fibref::types::builtins::BUILTINS;

fn ok(src: &str) -> Checked {
    check_source(src, "adv.fib").unwrap_or_else(|e| panic!("{src}\nrejected:\n{e}"))
}

fn body<'c>(c: &'c Checked, f: &str) -> (&'c BodyOwn, Expr) {
    let id = c.typed.fun(f).unwrap_or_else(|| panic!("no defun {f}"));
    (
        &c.owned.bodies[&BodyKey::Fun(id)],
        c.typed.globals.fun(id).body.clone(),
    )
}

fn exprs(e: &Expr) -> Vec<Expr> {
    let mut out = vec![e.clone()];
    e.children(&mut |c| out.extend(exprs(c)));
    out
}

fn head_name(c: &Checked, h: &Expr) -> String {
    let g = &c.typed.globals;
    match &h.kind {
        ExprKind::Global(GlobalRef::Fun(f)) => g.fun(*f).name.clone(),
        ExprKind::Global(GlobalRef::Builtin(b)) => BUILTINS[b.0 as usize].name.to_string(),
        ExprKind::Global(GlobalRef::Method(p, i)) => g.proto(*p).methods[*i].name.clone(),
        ExprKind::Global(GlobalRef::Ctor(t, _)) => g.ty(*t).name.clone(),
        ExprKind::Local(b) => g.binding(*b).name.clone(),
        _ => "<value>".into(),
    }
}

/// The calls to `callee` in `f` (its literals included), in source order.
fn calls(c: &Checked, f: &str, callee: &str) -> Vec<CallOwn> {
    let (b, e) = body(c, f);
    exprs(&e)
        .into_iter()
        .filter(|x| matches!(&x.kind, ExprKind::Call(h, _) if head_name(c, h) == callee))
        .map(|x| b.calls[&x.id].clone())
        .collect()
}

fn call(c: &Checked, f: &str, callee: &str) -> CallOwn {
    let cs = calls(c, f, callee);
    assert_eq!(cs.len(), 1, "calls to {callee} in {f}");
    cs[0].clone()
}

fn param(c: &Checked, f: &str, name: &str) -> (ParamKind, bool) {
    let (b, _) = body(c, f);
    let p = b
        .params
        .iter()
        .find(|p| c.typed.globals.binding(p.binding).name == name)
        .unwrap_or_else(|| panic!("no parameter {name}"));
    (p.kind.clone(), p.escapes)
}

fn is_owned(k: &ParamKind) -> bool {
    matches!(k, ParamKind::Owned(_))
}

fn closures(c: &Checked, f: &str) -> Vec<ClosureOwn> {
    let (b, e) = body(c, f);
    exprs(&e)
        .iter()
        .filter_map(|x| b.closures.get(&x.id).cloned())
        .collect()
}

fn binding_kind(c: &Checked, f: &str, name: &str) -> (BindKind, bool) {
    let (b, _) = body(c, f);
    let (_, own) = b
        .bindings
        .iter()
        .find(|(id, _)| c.typed.globals.binding(**id).name == name)
        .unwrap_or_else(|| panic!("no binding {name}"));
    (own.kind, own.scope_local)
}

/// A tail call's jump: the sites it releases, by binding name.
fn released_at_jump(c: &Checked, call: &CallOwn) -> Vec<String> {
    call.jump
        .iter()
        .map(|o| match o.site {
            Site::Bind(b) => c.typed.globals.binding(b).name.clone(),
            Site::Env(_) => "env".into(),
            other => format!("{other:?}"),
        })
        .collect()
}

/// A borrowed parameter reaching a tail call through a `let` alias. At
/// an owned position of the callee the parameter must become owned
/// (rule 2 sees through the alias, whose reads are `Borrowed(p)`) and
/// its count moves; at a borrowed position it stays borrowed and the
/// call stays a tail call; an alias of an owning `let` binding at a
/// borrowed position makes the call ordinary (rule (e)).
#[test]
fn borrowed_parameter_reaching_a_tail_call_through_a_let_alias() {
    let c = ok("(defun keep (b n) (if (= n 0) b (keep b (- n 1))))
                (defun f (p) (let ((q p)) (keep q 3)))
                (defun peek (b) (str-len b))
                (defun g (p) (let ((q p)) (peek q)))
                (defun h () (let ((s (str-concat \"a\" \"b\"))) (let ((q s)) (peek q))))
                (defun main () -> i64 (+ (str-len (f (str-concat \"a\" \"b\"))) (+ (g \"x\") (h))))");
    let (kind, _) = param(&c, "f", "p");
    assert!(
        matches!(&kind, ParamKind::Owned(ws) if ws.iter().any(|w| matches!(w, OwnedWhy::Rule2(_)))),
        "{kind:?}"
    );
    let k = call(&c, "f", "keep");
    assert_eq!(
        (k.tail, k.args.clone()),
        (Tail::TailCall, vec![Pass::Move, Pass::Scalar])
    );
    assert!(
        released_at_jump(&c, &k).is_empty(),
        "p's count travels: {:?}",
        k.jump
    );
    assert_eq!(param(&c, "g", "p").0, ParamKind::Borrowed);
    assert_eq!(call(&c, "g", "peek").tail, Tail::TailCall);
    assert_eq!(
        call(&c, "h", "peek").tail,
        Tail::Ordinary(Because::FrameOwned { arg: 0 })
    );
}

/// A closure passed through a tail call: whatever the callee's summary
/// says, the literal is a heap closure (E6) that owns its capture, the
/// jump releases the `let` binding it captured, and inside the closure
/// the call that borrows the capture is ordinary.
#[test]
fn closure_passed_through_a_tail_call() {
    let c = ok("(defun run (k n) (if (= n 0) (k) (run k (- n 1))))
                (defun go (s) (let ((x (str-concat s \"!\"))) (run (fn () (str-len x)) 2)))
                (defun main () -> i64 (go \"ab\"))");
    let (kind, escapes) = param(&c, "run", "k");
    assert!(is_owned(&kind) && !escapes, "{kind:?} {escapes}");
    let cl = &closures(&c, "go")[0];
    assert_eq!(
        (cl.escaping.clone(), cl.heap.as_deref()),
        (None, Some("arg-of-tail-call"))
    );
    assert_eq!(cl.captures[0].pass, Pass::Retain);
    let r = call(&c, "go", "run");
    assert_eq!(r.tail, Tail::TailCall);
    assert_eq!(released_at_jump(&c, &r), vec!["x"]);
    assert_eq!(
        call(&c, "go", "str-len").tail,
        Tail::Ordinary(Because::FrameOwned { arg: 0 })
    );
}

/// A would-be stack object returned through a whole-scrutinee pattern
/// variable: the variable is `Borrowed(p)`, so the `let` moves `p` out
/// and `p` is never scope-local; the same through a function.
#[test]
fn stack_object_returned_via_a_match_pattern_variable() {
    let c = ok("(defstruct P (a: i64))
                (defun pick (p) (match p (q q)))
                (defun main () -> i64
                  (let ((r (let ((p (P 1))) (match p (q q))))
                        (s (P 2)))
                    (+ (. r a) (. (pick s) a))))");
    assert_eq!(binding_kind(&c, "main", "p"), (BindKind::Owns, false));
    assert_eq!(binding_kind(&c, "main", "s"), (BindKind::Owns, false));
    let (b, _) = body(&c, "main");
    assert!(
        b.allocs.values().all(|a| *a == Alloc::Heap),
        "{:?}",
        b.allocs
    );
    let (kind, escapes) = param(&c, "pick", "p");
    assert!(is_owned(&kind) && escapes);
}

/// A join of `Borrowed(p)` and `Derived(p)`: `Owned`, with a retain on
/// both branches, and `p` escapes and is owned (§6.3's mixed case).
#[test]
fn join_of_borrowed_and_derived_of_the_same_binding() {
    let c = ok("(defenum T (Leaf) (Node inner: T))
                (defun unwrap1 (p) (match p ((Node inner) inner) ((Leaf) p)))
                (defun main () -> i64 (match (unwrap1 (Node (Node Leaf))) ((Node _) 1) ((Leaf) 0)))");
    let (kind, escapes) = param(&c, "unwrap1", "p");
    assert!(
        matches!(&kind, ParamKind::Owned(ws) if ws.contains(&OwnedWhy::Rule1("join retain".into())))
    );
    assert!(escapes);
    let (b, _) = body(&c, "unwrap1");
    let joins = b
        .exprs
        .values()
        .flat_map(|o| &o.after)
        .filter(|o| o.kind == OpKind::Retain)
        .count();
    assert_eq!(joins, 2, "one retain per branch");
}

/// Mutual recursion where only `ev` returns its parameter: `od` passes
/// its `y` at `ev`'s owned `x` in a tail call, so `y` is owned too
/// (rule 2) and both mutual calls are tail calls moving the count.
#[test]
fn mutual_recursion_where_only_one_parameter_is_returned() {
    let c = ok("(defun ev (x n) (if (= n 0) x (od x (- n 1))))
                (defun od (y n) (if (= n 0) (str-concat \"z\" \"\") (ev y (- n 1))))
                (defun main () -> i64 (let ((s (str-concat \"a\" \"b\"))) (str-len (ev s 3))))");
    assert!(is_owned(&param(&c, "ev", "x").0));
    assert!(is_owned(&param(&c, "od", "y").0));
    for (f, g) in [("ev", "od"), ("od", "ev")] {
        let k = call(&c, f, g);
        assert_eq!(
            (k.tail, k.args[0]),
            (Tail::TailCall, Pass::Move),
            "{f} -> {g}"
        );
    }
    let base = call(&c, "od", "str-concat");
    assert_eq!(
        (base.tail, released_at_jump(&c, &base)),
        (Tail::TailCall, vec!["y".to_string()])
    );
    assert_eq!(call(&c, "main", "ev").args[0], Pass::Retain);
}

/// A named `fn` storing its own self-name in a struct: the self-name is
/// a use of the literal, so the closure is escaping and on the heap even
/// though its binding is only called; the store retains `env`. The
/// self tail call hands `env` on without releasing it.
#[test]
fn named_fn_self_reference_stored_in_a_struct() {
    let c = ok("(defstruct R (k: (fn (i64) i64)))
                (defun mk () (let ((g (fn go (n: i64) (do (R go) (if (= n 0) 0 (go (- n 1))))))) (g 2)))
                (defun main () -> i64 (mk))");
    let cl = &closures(&c, "mk")[0];
    assert_eq!(cl.escaping.as_deref(), Some("stored"));
    assert!(cl.heap.is_some());
    assert_eq!(call(&c, "mk", "R").args, vec![Pass::Retain]);
    let rec = call(&c, "mk", "go");
    assert_eq!(rec.head, Pass::KeepEnv);
    assert!(
        !released_at_jump(&c, &rec).contains(&"env".to_string()),
        "{:?}",
        rec.jump
    );
}

/// `recur` inside a `match` inside a `loop`: the new value is built from
/// a part of the loop variable before the old value is released; the
/// base path's borrowing call is ordinary (the part is frame-owned).
#[test]
fn recur_inside_a_match_inside_a_loop() {
    let c = ok("(defun main () -> i64
                  (loop ((o (some (str-concat \"a\" \"b\"))) (i 0))
                    (match o
                      ((some s) (if (< i 3) (recur (some (str-concat s \"x\")) (+ i 1)) (str-len s)))
                      (nil 0))))");
    assert_eq!(binding_kind(&c, "main", "s").0, {
        let (b, _) = body(&c, "main");
        let o = b
            .bindings
            .keys()
            .find(|k| c.typed.globals.binding(**k).name == "o")
            .copied()
            .expect("o");
        BindKind::DerivedOf(Site::Bind(o))
    });
    let (b, _) = body(&c, "main");
    let r = b.recurs.values().next().expect("one recur");
    assert_eq!(r.args, vec![Pass::Move, Pass::Scalar]);
    assert_eq!(r.jump.len(), 1, "the old o only");
    assert_eq!(
        call(&c, "main", "str-len").tail,
        Tail::Ordinary(Because::FrameOwned { arg: 0 })
    );
}

/// A call in tail position of an `async` body is never a tail call (rule
/// (f)): directly, through a closure value, and with a borrowed argument
/// that rule (e) would have let through.
#[test]
fn async_body_with_a_call_in_tail_position() {
    let c = ok("(defun pass (b) b)
                (defun len2 (s) (str-len s))
                (defun run-later (k) (async (k)))
                (defun main () -> i64
                  (let ((b (Box 5)))
                    (+ (unbox (block-on (async (pass b))))
                       (+ (block-on (async (len2 \"hello\"))) (block-on (run-later (fn () 42)))))))");
    let p = call(&c, "main", "pass");
    assert_eq!(
        (p.tail, p.args.clone()),
        (Tail::Ordinary(Because::AsyncBody), vec![Pass::Retain])
    );
    assert_eq!(
        call(&c, "main", "len2").tail,
        Tail::Ordinary(Because::AsyncBody)
    );
    assert_eq!(
        call(&c, "run-later", "k").tail,
        Tail::Ordinary(Because::AsyncBody)
    );
    assert!(closures(&c, "main")
        .iter()
        .filter(|k| k.is_async)
        .all(|k| k.heap.is_some()));
}

/// `&` forwarding through mutual recursion (the owner's relaxed rule
/// (b)): each mutual call forwards the private cell and stays a tail
/// call with no write-back; the non-tail `push!` acquires and writes back.
#[test]
fn amp_forwarding_through_mutual_recursion() {
    let c = ok(
        "(defun even-fill (&v n) (if (= n 0) () (do (push! &v n) (odd-fill &v (- n 1)))))
                (defun odd-fill (&v n) (if (= n 0) () (do (push! &v n) (even-fill &v (- n 1)))))
                (defun main () -> i64 (let ((c (cell []))) (do (even-fill &c 4) (count @c))))",
    );
    for (f, g) in [("even-fill", "odd-fill"), ("odd-fill", "even-fill")] {
        let k = call(&c, f, g);
        assert_eq!(
            (k.tail, k.args[0], k.write_backs.len()),
            (Tail::TailCall, Pass::Forward, 0)
        );
        let p = call(&c, f, "push!");
        assert_eq!(
            (p.tail, p.args[0], p.write_backs.len()),
            (Tail::NotInTail, Pass::Acquire, 1)
        );
    }
    let m = call(&c, "main", "even-fill");
    assert_eq!((m.args[0], m.write_backs.len()), (Pass::Acquire, 1));
}

/// Two names for one cell in one call (the owner's option (A)): the
/// distinct-variables check is on names, so the call is accepted; each
/// argument acquires into its own private cell and the write-backs run
/// in parameter order.
#[test]
fn two_names_for_one_cell_in_one_call() {
    let c = ok("(defun bar (&a &b) (do (push! &a 1) (push! &b 2)))
                (defun main () -> i64 (let ((x (cell [3 4]))) (let ((y x)) (do (bar &x &y) (nth @x 2)))))");
    let b = call(&c, "main", "bar");
    assert_eq!(b.args, vec![Pass::Acquire, Pass::Acquire]);
    let names: Vec<(usize, String)> = b
        .write_backs
        .iter()
        .map(|(i, v)| (*i, c.typed.globals.binding(*v).name.clone()))
        .collect();
    assert_eq!(names, vec![(0, "x".to_string()), (1, "y".to_string())]);
    let (b_body, _) = body(&c, "main");
    let x = b_body
        .bindings
        .keys()
        .find(|k| c.typed.globals.binding(**k).name == "x")
        .copied()
        .expect("x");
    assert_eq!(
        binding_kind(&c, "main", "y").0,
        BindKind::AliasOf(Site::Bind(x))
    );
}
