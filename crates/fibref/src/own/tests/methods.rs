//! Method implementations used as values (types §8.4): which get an
//! all-owned body ([`BodyKey::MethodOwned`]), and what it decides.

use crate::types::ast::GlobalRef;
use crate::types::display::Printer;
use crate::types::infer::Resolution;

use super::super::program::{BodyKey, OpKind, OwnedWhy, ParamKind, Pass, Site, Tail, Why};
use super::*;

const TWO_SHOWS: &str = "(defstruct Pt (x: i64 y: i64))
     (impl Show Pt (show (self) (str-concat \"p\" (show (. self x)))))
     (defstruct Tag (n: i64))
     (impl Show Tag (show (self) (str-concat \"t\" (show (. self n)))))";

/// The index of the user instance of `proto` for the type printed `ty`.
fn instance(c: &Checked, proto: &str, ty: &str) -> usize {
    let g = &c.typed.globals;
    (0..g.instances.len())
        .find(|i| {
            let inst = &g.instances[*i];
            let head = Printer::with_names(g, &inst.var_names, &[]).ty(&inst.head);
            g.proto(inst.proto).name == proto && head == ty
        })
        .unwrap_or_else(|| panic!("no instance {proto} {ty}"))
}

/// How the one use of `show` in `fun` (a value there) was resolved.
fn value_resolution<'c>(c: &'c Checked, fun_name: &str) -> &'c Resolution {
    let (_, body) = fun(c, fun_name);
    let g = &c.typed.globals;
    let uses: Vec<_> = all(body)
        .into_iter()
        .filter(|e| match e.kind {
            ExprKind::Global(GlobalRef::Method(p, i)) => g.proto(p).methods[i].name == "show",
            _ => false,
        })
        .collect();
    assert_eq!(uses.len(), 1, "uses of show in {fun_name}");
    &c.typed.resolutions[&uses[0].id]
}

/// A method value resolved to an instance gets that instance's
/// all-owned body, and only that one: `self` owned (declared, not
/// inferred), released at the body's exit; the method's own body keeps
/// the protocol's borrowed `self`.
#[test]
fn a_resolved_method_value_gets_its_instance_all_owned_body() {
    let src = format!(
        "{TWO_SHOWS}
         (defun main () -> i64 (str-len (nth (map show (conj (vec-empty) (Pt 1 2))) 0)))"
    );
    let c = ok(&src);
    assert!(matches!(
        value_resolution(&c, "main"),
        Resolution::Instance { .. }
    ));
    let (pt, tag) = (instance(&c, "Show", "Pt"), instance(&c, "Show", "Tag"));
    assert_eq!(
        c.owned.methods_taken.iter().copied().collect::<Vec<_>>(),
        vec![(pt, 0)]
    );
    assert!(!c.owned.bodies.contains_key(&BodyKey::MethodOwned(tag, 0)));
    let owned = &c.owned.bodies[&BodyKey::MethodOwned(pt, 0)];
    let own_self = &owned.params[0];
    assert_eq!(own_self.kind, ParamKind::Owned(vec![OwnedWhy::Declared]));
    let exits: Vec<_> = owned
        .exprs
        .values()
        .flat_map(|x| x.after.iter())
        .filter(|o| o.why == Why::ParamExit)
        .collect();
    assert_eq!(exits.len(), 1, "{exits:?}");
    assert_eq!(exits[0].kind, OpKind::Release);
    assert_eq!(exits[0].site, Site::Bind(own_self.binding));
    let plain = &c.owned.bodies[&BodyKey::Method(pt, 0)];
    assert_eq!(plain.params[0].kind, ParamKind::Borrowed);
    let order = &c.owned.order;
    let at = |k| order.iter().position(|x| *x == k).expect("in order");
    assert_eq!(
        at(BodyKey::MethodOwned(pt, 0)),
        at(BodyKey::Method(pt, 0)) + 1
    );
}

/// A method value in a generic caller is resolved to a bound (§4.3):
/// the instance is the receiver's, so every implementation of the
/// method gets an all-owned body.
#[test]
fn a_bound_method_value_gets_every_implementation_all_owned() {
    let src = format!(
        "{TWO_SHOWS}
         (defun lens (v) (str-len (nth (map show v) 0)))
         (defun main () -> i64
           (+ (lens (conj (vec-empty) (Pt 1 2))) (lens (conj (vec-empty) (Tag 3)))))"
    );
    let c = ok(&src);
    assert!(matches!(value_resolution(&c, "lens"), Resolution::Bound(_)));
    let (pt, tag) = (instance(&c, "Show", "Pt"), instance(&c, "Show", "Tag"));
    for i in [pt, tag] {
        assert!(c.owned.methods_taken.contains(&(i, 0)));
        let b = &c.owned.bodies[&BodyKey::MethodOwned(i, 0)];
        assert!(matches!(b.params[0].kind, ParamKind::Owned(_)));
    }
}

/// Methods only called, and built-in instances (which have no body),
/// get no all-owned body.
#[test]
fn called_methods_and_builtin_instances_get_no_all_owned_body() {
    let src = format!(
        "{TWO_SHOWS}
         (defun main () -> i64
           (+ (str-len (show (Pt 1 2)))
              (str-len (nth (map show (conj (vec-empty) 42)) 0))))"
    );
    let c = ok(&src);
    assert!(c.owned.methods_taken.is_empty());
    assert!(!c
        .owned
        .bodies
        .keys()
        .any(|k| matches!(k, BodyKey::MethodOwned(..))));
}

/// The all-owned body is decided afresh (§8.4, §3.5 step 5f): with
/// `self` owned, the tail call that stores it moves it, where the
/// method's own body, whose `self` is borrowed, retains it.
#[test]
fn the_all_owned_body_moves_self_into_a_tail_call() {
    let c = ok("(defstruct Pt (x: i64 y: i64))
                (defstruct Box (p: Pt))
                (defun keep (p: Pt) -> str (let ((b (Box p))) (show (. (. b p) x))))
                (impl Show Pt (show (self) (keep self)))
                (defun main () -> i64 (str-len (nth (map show (conj (vec-empty) (Pt 1 2))) 0)))");
    let pt = instance(&c, "Show", "Pt");
    let tail_arg = |k: BodyKey| {
        let calls: Vec<_> = c.owned.bodies[&k].calls.values().collect();
        assert_eq!(calls.len(), 1, "{k:?}");
        assert_eq!(calls[0].tail, Tail::TailCall, "{k:?}");
        calls[0].args[0]
    };
    assert_eq!(tail_arg(BodyKey::Method(pt, 0)), Pass::Retain);
    assert_eq!(tail_arg(BodyKey::MethodOwned(pt, 0)), Pass::Move);
}
