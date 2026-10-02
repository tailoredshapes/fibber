//! The prelude type-checks; the 20 cases of `cases/ownership` type-check
//! except case 13 (and case 14, see `case_14_*`), with the types §7 and
//! syntax Appendix A describe.

use std::path::PathBuf;

use crate::expand::ExpandCtx;
use crate::types::ast::{ExprKind, FunId};
use crate::types::decls::ModuleId;
use crate::types::scheme::Scheme;
use crate::types::ty::{Colour, Leaf, Pred, Ty};
use crate::types::{check_library, prelude_forms, ErrorKind, TypedProgram};

use super::{binding_type, ok_lib};

fn case_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership")
}

fn case_source(name: &str) -> String {
    let path = case_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The case through the module loader, with the implicit library.
fn case(name: &str) -> TypedProgram {
    ok_lib(&case_source(name))
}

fn fun(p: &TypedProgram, name: &str) -> FunId {
    p.fun(name).unwrap_or_else(|| panic!("no defun {name}"))
}

fn scheme<'p>(p: &'p TypedProgram, name: &str) -> &'p Scheme {
    p.scheme(name)
        .unwrap_or_else(|| panic!("no scheme for {name}"))
}

/// The colours of the `fn` literals in `f`'s body, in source order.
fn fn_colours(p: &TypedProgram, f: FunId) -> Vec<(usize, Colour, String)> {
    let mut out = Vec::new();
    let mut stack = vec![p.globals.fun(f).body.clone()];
    while let Some(e) = stack.pop() {
        if let ExprKind::Fn(_) = &e.kind {
            let t = p.show_in(f, &p.expr_types[&e.id]);
            out.push((e.pos.line, p.fn_colours[&e.id], t));
        }
        e.children(&mut |c| stack.push(c.clone()));
    }
    out.sort_by_key(|x| x.0);
    out
}

#[test]
fn the_prelude_type_checks() {
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx).expect("the prelude reads and expands");
    let p = check_library(&[], &prelude).unwrap_or_else(|es| {
        panic!(
            "{}",
            es.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    let show = |n: &str| p.show_fun(n).unwrap_or_else(|| panic!("no {n}"));
    assert_eq!(
        show("pmap"),
        "∀a b. (Send b) (Send a) ⇒ (fn :send ((fn :send (a) b) (Vec a)) (Vec b))"
    );
    assert_eq!(show("push!"), "∀a. (fn ((& (Vec a)) a) unit)");
    assert_eq!(show("unbox"), "∀a. (fn :send ((Box a)) a)");
    assert_eq!(show("nil?"), "∀a. (fn :send ((Option a)) bool)");
    assert_eq!(show("block-on"), "∀a. (fn :send ((Task a)) a)");
    // Every prelude defun and impl body was checked: nothing is missing.
    let prelude_funs = p
        .globals
        .funs
        .iter()
        .filter(|f| f.module == ModuleId::PRELUDE)
        .count();
    let schemes = p.fun_schemes.iter().flatten().count();
    assert_eq!(prelude_funs, schemes);
    assert!(p
        .globals
        .instances
        .iter()
        .filter(|i| i.module == ModuleId::PRELUDE)
        .all(|i| !i.methods.is_empty()));
}

#[test]
fn every_accept_case_type_checks_and_13_and_14_do_not() {
    let mut names: Vec<String> = std::fs::read_dir(case_dir())
        .expect("cases/ownership exists")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".fib"))
        .collect();
    names.sort();
    assert!(names.len() >= 20, "{names:?}");
    for name in &names {
        // User macros need the evaluator; the case runner covers them.
        if case_source(name).contains("(defmacro") {
            continue;
        }
        let r = crate::own::check_source(&case_source(name), name);
        // By number: "13" is a prefix of "131".
        match (name.split('-').next().unwrap_or_default(), r) {
            ("13" | "14", r) => assert!(r.is_err(), "{name} type-checked"),
            (_, Ok(_)) => {}
            // Other reject cases may be rejected by typing or by the
            // ownership pass; the case runner checks their texts.
            (_, Err(_)) if name.contains("reject") => {}
            (_, Err(e)) => panic!("{name}: {e:?}"),
        }
    }
}

#[test]
fn case_13_fails_with_the_canonical_text_and_witness() {
    let err =
        match crate::own::check_source(&case_source("13-reject-cell-crosses-thread.fib"), "13.fib")
        {
            Err(crate::own::CheckError::Type(es)) => es[0].clone(),
            other => panic!("expected a type error, got {:?}", other.map(|_| ())),
        };
    assert_eq!(err.kind, ErrorKind::CellNotSend);
    assert_eq!(
        err.message,
        "cell cannot be shared between threads: closure capture n has type (Cell i64)"
    );
    // The position is the closure that reaches pmap (§3.5: the form whose
    // rule generated the failing constraint).
    assert_eq!((err.pos.line, err.pos.col), (6, 11));
}

/// Case 14 is rejected in the full pipeline by the syntactic `&` check of
/// types §6.9, which §3.5 step 3 runs before typing. The typing rules
/// alone reject it too: its `async` captures the `&` parameter `buf`, a
/// `(Cell T)`, and §2.8 requires `send ⊒ Caps` of every `async`. This
/// pins that outcome; see the report on the conflict with "case 14 must
/// type-check".
#[test]
fn case_14_is_rejected_by_the_async_send_rule() {
    // the typing rules alone, with the prelude only: the library's `count` is
    // not in scope there, so the prelude's `vec-count` stands for it
    let src = case_source("14-reject-inout-in-async.fib").replace("(count @b)", "(vec-count @b)");
    let err = match crate::types::check_source(&src, "14.fib") {
        Err(crate::types::SourceError::Type(es)) => es[0].clone(),
        other => panic!("expected a type error, got {:?}", other.map(|_| ())),
    };
    assert_eq!(err.kind, ErrorKind::CellNotSend);
    assert!(
        err.message
            .starts_with("cell cannot be shared between threads: async capture buf has type (Cell"),
        "{err}"
    );
}

#[test]
fn case_01_head_is_generic_over_reducible() {
    let p = case("01-return-part-of-argument.fib");
    let s = scheme(&p, "head");
    // §Appendix A: head : ∀c e. (Reducible c e) ⇒ (fn (c) e), the library's.
    let proto = p
        .globals
        .protos
        .iter()
        .position(|g| g.name == "Reducible")
        .expect("Reducible");
    assert_eq!(s.n_vars, 2);
    assert_eq!(
        s.preds,
        vec![Pred::Proto(
            crate::types::ty::ProtoId(proto as u32),
            vec![Ty::Gen(0), Ty::Gen(1)]
        )]
    );
    assert_eq!(
        s.ty,
        Ty::Fn(Colour::Send, vec![Ty::Gen(0)], Box::new(Ty::Gen(1)))
    );
    assert_eq!(binding_type(&p, "l"), "(List (Box i64))");
    assert_eq!(binding_type(&p, "h"), "(Box i64)");
}

#[test]
fn case_04_pick_is_monomorphic_in_str() {
    let p = case("04-branch-dependent-owner.fib");
    assert_eq!(
        p.show_fun("pick").as_deref(),
        Some("(fn :send (bool str) str)")
    );
    assert_eq!(binding_type(&p, "a"), "str");
    assert_eq!(binding_type(&p, "b"), "str");
}

#[test]
fn case_05_make_counter_returns_local_closures() {
    let p = case("05-closures-share-state.fib");
    assert_eq!(
        p.show_fun("make-counter").as_deref(),
        Some("(fn :send () (List (fn :local () i64)))")
    );
    let f = fun(&p, "make-counter");
    let colours: Vec<Colour> = fn_colours(&p, f).into_iter().map(|c| c.1).collect();
    assert_eq!(colours, vec![Colour::Local, Colour::Local]);
    assert_eq!(binding_type(&p, "n"), "(Cell i64)");
    assert_eq!(binding_type(&p, "inc"), "(fn :local () i64)");
}

#[test]
fn case_10_the_pmap_closure_is_send() {
    let p = case("10-atom-old-value.fib");
    let colours = fn_colours(&p, fun(&p, "main"));
    // Line 7: plet's thunk; line 8: the pmap closure; line 10: swap!'s.
    let pmap_closure = colours.iter().find(|c| c.0 == 8).expect("the pmap closure");
    assert_eq!(pmap_closure.1, Colour::Send);
    assert_eq!(pmap_closure.2, "(fn :send (i64) i64)");
    assert!(colours.iter().all(|c| c.1 == Colour::Send), "{colours:?}");
    assert_eq!(binding_type(&p, "a"), "(Atom (Vec i64))");
    assert_eq!(binding_type(&p, "snapshot"), "(Vec i64)");
}

#[test]
fn case_19_add_child_needs_no_annotation() {
    let p = case("19-weak-parent-pointer.fib");
    assert_eq!(
        p.show_fun("add-child").as_deref(),
        Some("(fn :send (Node) Node)")
    );
    assert_eq!(
        p.show_fun("depth").as_deref(),
        Some("(fn :send (Node) i64)")
    );
    assert_eq!(binding_type(&p, "w"), "(Weak Node)");
    assert_eq!(binding_type(&p, "p"), "Node");
}

#[test]
fn case_19_depth_without_its_annotation_is_an_error() {
    // §3.4: depth has only a field access and a recursive call.
    let src = case_source("19-weak-parent-pointer.fib")
        .replace("(defun depth (n: Node) -> i64", "(defun depth (n)");
    super::fails_lib(
        &src,
        ErrorKind::FieldUnresolved,
        "cannot infer the struct type of n for field parent; annotate it",
    );
}

/// Whether `t` still has a unification variable, a rigid variable or an
/// unsolved colour variable.
fn open(t: &Ty) -> bool {
    let mut found = false;
    t.visit(&mut |l| {
        found |= matches!(
            l,
            Leaf::Var(_) | Leaf::Rigid(_) | Leaf::Colour(Colour::Var(_))
        );
    });
    found
}

/// Every expression of `e` has a type in `p`.
fn all_typed(p: &TypedProgram, e: &crate::types::ast::Expr, missing: &mut Vec<String>) {
    if !p.expr_types.contains_key(&e.id) {
        missing.push(format!("{} {:?}", e.pos, e.kind));
    }
    e.children(&mut |c| all_typed(p, c, missing));
}

/// Every expression of every body in `p` has a type.
fn assert_complete(p: &TypedProgram, name: &str) {
    let mut missing = Vec::new();
    for f in &p.globals.funs {
        all_typed(p, &f.body, &mut missing);
    }
    for inst in &p.globals.instances {
        inst.methods
            .iter()
            .for_each(|m| all_typed(p, &m.body, &mut missing));
    }
    assert!(missing.is_empty(), "{name}: untyped {missing:?}");
}

#[test]
fn typed_programs_are_closed_and_complete() {
    for name in [
        "01-return-part-of-argument.fib",
        "05-closures-share-state.fib",
        "08-mutate-while-iterating.fib",
        "10-atom-old-value.fib",
        "16-coordinated-update-single-atom.fib",
        "19-weak-parent-pointer.fib",
    ] {
        let p = case(name);
        assert_complete(&p, name);
        assert!(
            p.expr_types.values().all(|t| !open(t)),
            "{name}: open expression type"
        );
        assert!(
            p.binding_types.values().all(|t| !open(t)),
            "{name}: open binding type"
        );
        for (id, i) in &p.instantiations {
            assert!(i.tys.iter().all(|t| !open(t)), "{name}: {id:?} {i:?}");
            assert!(
                i.colours.iter().all(|k| !matches!(k, Colour::Var(_))),
                "{name}: {id:?} {i:?}"
            );
        }
        for r in p.resolutions.values() {
            match r {
                crate::types::infer::Resolution::Instance { args, .. } => {
                    assert!(args.iter().all(|t| !open(t)))
                }
                crate::types::infer::Resolution::Bound(q) => {
                    assert!(q.tys().iter().all(|t| !open(t)))
                }
                crate::types::infer::Resolution::Dyn => {}
            }
        }
        assert_eq!(
            p.globals.bindings.len(),
            p.binding_types.len(),
            "{name}: untyped binding"
        );
    }
}

#[test]
fn method_uses_record_their_instances() {
    let p = case("02-structural-sharing.fib");
    let main = fun(&p, "main");
    let mut heads = Vec::new();
    let mut stack = vec![p.globals.fun(main).body.clone()];
    while let Some(e) = stack.pop() {
        if let ExprKind::Global(crate::types::ast::GlobalRef::Method(pr, m)) = &e.kind {
            heads.push((
                p.globals.proto(*pr).methods[*m].name.clone(),
                p.resolutions.get(&e.id).cloned(),
            ));
        }
        e.children(&mut |c| stack.push(c.clone()));
    }
    heads.sort_by(|a, b| a.0.cmp(&b.0));
    let names: Vec<&str> = heads.iter().map(|h| h.0.as_str()).collect();
    // `count` is the library's function over `size` now, not a method
    assert_eq!(names, ["+", "nth"]);
    for (name, r) in &heads {
        let Some(crate::types::infer::Resolution::Instance { index, args }) = r else {
            panic!("{name}: {r:?}")
        };
        let inst = &p.globals.instances[*index];
        let head =
            crate::types::display::Printer::new(&p.globals).ty(&inst.head.subst_gen(args, &[]));
        let want = if name == "+" { "i64" } else { "(Vec i64)" };
        assert_eq!(head, want, "{name}");
    }
    // add4's conj is resolved by its bound, per specialisation (§4.3).
    let add4 = fun(&p, "add4");
    let mut stack = vec![p.globals.fun(add4).body.clone()];
    while let Some(e) = stack.pop() {
        if let ExprKind::Global(crate::types::ast::GlobalRef::Method(..)) = &e.kind {
            assert!(matches!(
                p.resolutions.get(&e.id),
                Some(crate::types::infer::Resolution::Bound(_))
            ));
        }
        e.children(&mut |c| stack.push(c.clone()));
    }
}
