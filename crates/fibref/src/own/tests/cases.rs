//! Cases 01–10 of `cases/ownership`: the decisions types §7 states for
//! each, and the count traces of §6.3 and §6.6 where there is one.

use super::super::program::{Because, BindKind, OwnedWhy, ParamKind, Pass, Tail};
use super::*;

fn owned(p: &ParamOwn) -> Vec<OwnedWhy> {
    match &p.kind {
        ParamKind::Owned(ws) => ws.clone(),
        other => panic!("expected owned, got {other:?}"),
    }
}

fn has(ops: &[String], op: &str) -> bool {
    ops.iter().any(|o| o == op)
}

/// §6.3 after the flip: `first` is the library's and answers an
/// `(Option e)`, so `head` matches it. The element it returns is memory
/// the list owns: `head` retains it at the join of the match's branches
/// (the `some` clause returns the bound `x`, the other traps) and releases
/// the `Option` temporary at its exit, so the caller's binding and the list
/// each hold a count. `xs` is borrowed, and the call that reads it is not in
/// tail position (its result is matched), so it passes the borrow on.
#[test]
fn case_01_first_retains_the_part_it_returns() {
    let c = case("01");
    let (_, first) = call(&c, "head", "first");
    assert_eq!(
        (first.tail, first.args.clone()),
        (Tail::NotInTail, vec![Pass::Borrow])
    );
    assert_eq!(param(&c, "head", "xs").kind, ParamKind::Borrowed);
    assert_eq!(
        ops(&c, "head"),
        vec!["retain x (join)", "release (first) (exit)"]
    );
    // main's h owns one count (a call result, never scope-local).
    for b in ["l", "h"] {
        assert_eq!(binding(&c, "main", b).kind, BindKind::Owns);
        assert!(!binding(&c, "main", b).scope_local, "{b}");
    }
    let ops = ops(&c, "main");
    assert!(
        has(&ops, "release h (exit)") && has(&ops, "release l (exit)"),
        "{ops:?}"
    );
}

/// `conj` stores (E2 inside the library); `make`'s `v` is released at its
/// scope exit because `(add4 v)` passes a frame-owned `v` at a borrowed
/// position (rule (e)).
#[test]
fn case_02_make_releases_v_after_add4() {
    let c = case("02");
    let (_, add4) = call(&c, "make", "add4");
    assert_eq!(add4.tail, Tail::Ordinary(Because::FrameOwned { arg: 0 }));
    assert!(has(&ops(&c, "make"), "release v (exit)"));
    assert_eq!(call(&c, "add4", "conj").1.tail, Tail::TailCall);
    assert_eq!(param(&c, "add4", "v").kind, ParamKind::Borrowed);
    // main's (+ ..) is a tail call: w is released before its jump.
    let (_, plus) = call(&c, "main", "+");
    assert_eq!(plus.tail, Tail::TailCall);
    assert_eq!(jump(&c, "main", plus), vec!["release w (jump)"]);
}

/// §6.3: the inner `let`'s body is `Borrowed(a)` → move out; `b` and `s`
/// released; `keep` released at `main`'s exit. `item` escapes.
#[test]
fn case_03_inner_let_moves_a_out() {
    let c = case("03");
    let item = param(&c, "remember", "item");
    assert!(item.escapes);
    assert_eq!(item.kind, ParamKind::Borrowed);
    let ops = ops(&c, "main");
    for op in [
        "release b (exit)",
        "release s (exit)",
        "release keep (exit)",
    ] {
        assert!(has(&ops, op), "{op} in {ops:?}");
    }
    assert!(!has(&ops, "release a (exit)"), "a is moved out: {ops:?}");
    assert_eq!(call(&c, "remember", "conj").1.tail, Tail::TailCall);
}

/// §6.3: `pick`'s `if` joins `Borrowed(x)` with `Owned` → retain on the
/// `x` branch; `x` owned (rule 1), released at the exit; `main`
/// retains `s` at each call.
#[test]
fn case_04_join_retains_the_borrowed_branch() {
    let c = case("04");
    assert_eq!(
        ops(&c, "pick"),
        vec!["retain x (join)", "release x (param)"]
    );
    assert_eq!(
        owned(param(&c, "pick", "x")),
        vec![OwnedWhy::Rule1("join retain".into())]
    );
    for (_, p) in calls(&c, "main", "pick") {
        assert_eq!(p.args, vec![Pass::Scalar, Pass::Retain]);
    }
}

/// Both `fn`s are stored (E2 arguments of `cons`) → escaping → heap,
/// each retaining the cell `n`; the `let` releases its own count.
#[test]
fn case_05_two_escaping_closures_retain_the_cell() {
    let c = case("05");
    let cls = closures(&c, "make-counter");
    assert_eq!(cls.len(), 2);
    for cl in cls {
        assert_eq!(cl.escaping.as_deref(), Some("stored"));
        assert!(cl.heap.is_some());
        assert_eq!(cl.captures.len(), 1);
        assert_eq!(cl.captures[0].pass, Pass::Retain);
    }
    let n = binding(&c, "make-counter", "n");
    assert!(!n.scope_local);
    assert!(has(&ops(&c, "make-counter"), "release n (exit)"));
}

/// The `fn` is at E1 → escaping → E3 retains `prefix`, which is owned
/// and escapes; `p`'s `let` releases its count.
#[test]
fn case_06_escaping_closure_retains_prefix() {
    let c = case("06");
    let cl = closures(&c, "matcher");
    assert_eq!(cl[0].escaping.as_deref(), Some("returned"));
    assert_eq!(cl[0].captures[0].pass, Pass::Retain);
    let prefix = param(&c, "matcher", "prefix");
    assert!(prefix.escapes);
    assert_eq!(
        owned(prefix),
        vec![OwnedWhy::Rule1("captured by a heap closure".into())]
    );
    assert_eq!(call(&c, "main", "matcher").1.args, vec![Pass::Retain]);
    assert!(has(&ops(&c, "main"), "release p (exit)"));
}

/// §6.10's account of case 07: `acc` owned by rule 1 (the join) and rule
/// 3 (the self tail call passes a fresh vector); `(conj acc n)` moved
/// into the call; `acc` released before the jump; on the base path a
/// retain (join) and a release (exit), not a move.
#[test]
fn case_07_accumulator_is_owned_and_the_self_call_is_a_tail_call() {
    let c = case("07");
    let ws = owned(param(&c, "build", "acc"));
    assert!(
        ws.contains(&OwnedWhy::Rule1("join retain".into())),
        "{ws:?}"
    );
    assert!(ws.iter().any(|w| matches!(w, OwnedWhy::Rule3(_))), "{ws:?}");
    let (_, rec) = call(&c, "build", "build");
    assert_eq!(rec.tail, Tail::TailCall);
    assert_eq!(rec.args, vec![Pass::Scalar, Pass::Move]);
    assert_eq!(jump(&c, "build", rec), vec!["release acc (jump)"]);
    assert_eq!(
        ops(&c, "build"),
        vec!["retain acc (join)", "release acc (param)"]
    );
    // main: (count ..) is ordinary by rule (e); [] moved into acc.
    let (_, count) = call(&c, "main", "count");
    assert_eq!(count.tail, Tail::Ordinary(Because::FrameOwned { arg: 0 }));
    assert_eq!(
        call(&c, "main", "build").1.args,
        vec![Pass::Scalar, Pass::Move]
    );
    assert_eq!(ops(&c, "main"), vec!["release (build) (step)"]);
}

/// The trace of §6.6 for case 08: the copy-in acquires; `(for-each @v
/// ..)` is ordinary (rule (e), a fresh `@v` at a borrowed position), so
/// not a tail site and the closure is a stack closure capturing `v` as
/// its own cell; each `append` acquires and writes back; the step's
/// temporary `@v` is released when `for-each` returns; `main`'s `v` is
/// scope-local and written back once.
#[test]
fn case_08_stack_closure_capturing_the_private_cell() {
    let c = case("08");
    let (_, fe) = call(&c, "dup-all", "run!");
    assert_eq!(fe.tail, Tail::Ordinary(Because::FrameOwned { arg: 0 }));
    assert_eq!(fe.args, vec![Pass::Borrow, Pass::Borrow]);
    let cl = closures(&c, "dup-all");
    assert_eq!((cl[0].escaping.clone(), cl[0].heap.clone()), (None, None));
    assert_eq!(cl[0].captures[0].pass, Pass::OwnCell);
    let (_, app) = call(&c, "dup-all", "append");
    assert_eq!(app.tail, Tail::Ordinary(Because::AmpArgument));
    assert_eq!(app.args[0], Pass::Acquire);
    assert_eq!(app.write_backs.len(), 1);
    let steps: Vec<String> = ops(&c, "dup-all")
        .into_iter()
        .filter(|o| o.ends_with("(step)"))
        .collect();
    assert_eq!(steps, vec!["release @ (step)", "end-stack fn (step)"]);
    let (_, d) = call(&c, "main", "dup-all");
    assert_eq!(
        (d.args.clone(), d.write_backs.len()),
        (vec![Pass::Acquire], 1)
    );
    assert!(binding(&c, "main", "v").scope_local);
    let main_ops = ops(&c, "main");
    assert_eq!(
        main_ops.last().map(String::as_str),
        Some("end-stack v (exit)")
    );
    assert!(has(&main_ops, "release @ (step)"));
}

/// `filter` stores `v` in the recipe it returns (the parameter escapes,
/// and is borrowed from `evens`'s caller); `evens`'s call to `filter` is a
/// tail call that moves the predicate and passes `v` on as a borrow, the
/// callee taking its own count; `v`'s `let` releases it.
#[test]
fn case_09_iterator_retains_its_vector() {
    let c = case("09");
    let v = param(&c, "evens", "v");
    assert!(v.escapes);
    assert_eq!(v.kind, ParamKind::Borrowed);
    let (_, fi) = call(&c, "evens", "filter");
    assert_eq!(
        (fi.tail, fi.args.clone()),
        (Tail::TailCall, vec![Pass::Move, Pass::Borrow])
    );
    assert!(has(&ops(&c, "main"), "release v (exit)"));
}

/// The `pmap` closure escapes (`pmap`'s `f` is spawned) and retains the
/// atom; `swap!`'s closure is a stack closure (`swap!`'s `f` is `:borrow`).
#[test]
fn case_10_pmap_closure_escapes_swap_closure_does_not() {
    let c = case("10");
    let cls = closures(&c, "main");
    let pmap_fn = cls
        .iter()
        .find(|k| k.captures.iter().any(|x| x.pass == Pass::Retain))
        .expect("pmap's fn");
    assert!(pmap_fn.escaping.is_some() && pmap_fn.heap.is_some());
    let stack: Vec<_> = cls.iter().filter(|k| k.heap.is_none()).collect();
    assert_eq!(stack.len(), 1, "the swap! closure");
}
