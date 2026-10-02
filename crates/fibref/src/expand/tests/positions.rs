//! §1.3: built forms carry the macro call's position, input forms keep
//! their own.

use super::one;
use crate::expand::{expand_expr, ExpandCtx, NoRunner};
use crate::syntax::Form;

fn expand(src: &str) -> Form {
    expand_expr(one(src), &mut ExpandCtx::new(), &mut NoRunner).unwrap_or_else(|e| panic!("{e}"))
}

fn lc(f: &Form) -> (usize, usize) {
    (f.pos.line, f.pos.col)
}

/// `src` expanded and printed with every node's position, `node@line:col`,
/// a list as `(items..)@line:col`.
fn ex_pos(src: &str) -> String {
    fn shown(f: &Form) -> String {
        let at = format!("@{}:{}", f.pos.line, f.pos.col);
        match f.as_list() {
            Some(items) => {
                let inner: Vec<String> = items.iter().map(shown).collect();
                format!("({}){at}", inner.join(" "))
            }
            None => format!("{f}{at}"),
        }
    }
    shown(&expand(src))
}

#[test]
fn macro_built_forms_take_the_call_position() {
    let f = expand("(g\n  (when c\n    x))");
    let when = &f.as_list().unwrap_or(&[])[1];
    let items = when.as_list().unwrap_or(&[]);
    assert_eq!(lc(when), (2, 3));
    assert_eq!(lc(&items[0]), (2, 3), "the built `if`");
    assert_eq!(lc(&items[1]), (2, 9), "the test keeps its own");
    assert_eq!(lc(&items[2]), (3, 5), "the body keeps its own");
    assert_eq!(lc(&items[3]), (2, 3), "the built ()");
}

#[test]
fn nested_expansions_keep_the_inner_call_position() {
    // (and a b c) -> (if a (and b c) false): the inner `and` is built at
    // the outer call, and its own expansion takes that position.
    let f = expand("(and a\n b c)");
    let items = f.as_list().unwrap_or(&[]);
    assert_eq!(lc(&items[1]), (1, 6));
    assert_eq!(lc(&items[2]), (1, 1));
    let inner = items[2].as_list().unwrap_or(&[]);
    assert_eq!(lc(&inner[1]), (2, 2));
}

#[test]
fn collection_rewrite_takes_the_literal_position() {
    let f = expand("(f\n  [x])");
    let conj = &f.as_list().unwrap_or(&[])[1];
    let items = conj.as_list().unwrap_or(&[]);
    assert_eq!(lc(conj), (2, 3));
    assert_eq!(lc(&items[1]), (2, 3), "(vec-empty)");
    assert_eq!(lc(&items[2]), (2, 4), "the element keeps its own");
}

#[test]
fn collection_rewrite_of_several_items_takes_the_literal_position_everywhere() {
    // Every call of the chain, its head and the empty it starts from are
    // built at the literal (2:3); the items keep their own.
    assert_eq!(
        ex_pos("(f\n  [x\n y])"),
        "(f@1:2 (fib.prelude/vec-conj@2:3 (fib.prelude/vec-conj@2:3 \
         (fib.prelude/vec-empty@2:3)@2:3 x@2:4)@2:3 y@3:2)@2:3)@1:1"
    );
    assert_eq!(
        ex_pos("(f\n  {a b\n c d})"),
        "(f@1:2 (fib.prelude/map-assoc@2:3 (fib.prelude/map-assoc@2:3 \
         (fib.prelude/map-empty@2:3)@2:3 a@2:4 b@2:6)@2:3 c@3:2 d@3:4)@2:3)@1:1"
    );
}

#[test]
fn list_rewrite_takes_the_call_position_and_the_elements_keep_theirs() {
    // The Cons calls, their heads and the Empty the list ends in are built
    // at the call (2:3), as the other macros' forms are (§1.3).
    assert_eq!(
        ex_pos("(f\n  (list a\n b))"),
        "(f@1:2 (fib.prelude/Cons@2:3 a@2:9 (fib.prelude/Cons@2:3 b@3:2 \
         fib.prelude/Empty@2:3)@2:3)@2:3)@1:1"
    );
    assert_eq!(ex_pos("(g\n (list))"), "(g@1:2 fib.prelude/Empty@2:2)@1:1");
}

#[test]
fn quasi_rewrite_builds_at_the_template_forms_and_operands_keep_theirs() {
    // `(a ,b): the built `List` and `concat` are at the template list
    // (2:3), the one-element vector that wraps `a` and its `quote` at `a`
    // (2:4), the one that wraps `,b` at the unquote form (3:2); the
    // operand `b` keeps its own position (3:3).
    assert_eq!(
        ex_pos("(f\n `(a\n ,b))"),
        "(f@1:2 (fib.prelude/List@2:3 (fib.prelude/concat@2:3 \
         (fib.prelude/vec-conj@2:4 (fib.prelude/vec-empty@2:4)@2:4 (quote@2:4 a@2:4)@2:4)@2:4 \
         (fib.prelude/vec-conj@3:2 (fib.prelude/vec-empty@3:2)@3:2 b@3:3)@3:2)@2:3)@2:3)@1:1"
    );
}

/// Whether every node of `f` is at `line:col`.
fn all_at(f: &Form, line: usize, col: usize) -> bool {
    let here = (f.pos.line, f.pos.col) == (line, col);
    match f.as_list() {
        Some(items) => here && items.iter().all(|i| all_at(i, line, col)),
        None => here,
    }
}

#[test]
fn every_form_a_derive_builds_takes_the_position_of_the_call() {
    // §1.3: forms a macro builds carry the call's position, whatever the
    // protocol and the type; nothing in an instance is taken from the
    // definition it derives for (here at 1:1 and 2:1, the call at 3:3).
    let defs = "(defstruct (P t) (a: i64 b: t))\n(defenum (T u) (K) (L x: i64 y: u))\n";
    for target in ["P", "T"] {
        for proto in ["Eq", "Ord", "Hash", "Show", "Debug", "ToStr"] {
            let src = format!("{defs}  (derive {proto} {target})");
            let out = super::program(&src).unwrap_or_else(|e| panic!("{src}: {e}"));
            let derived = &out[out.len() - 1];
            assert_eq!(
                derived.as_list().and_then(|l| l[0].as_sym()),
                Some("impl"),
                "{src}"
            );
            assert!(all_at(derived, 3, 3), "{src}: {derived}");
        }
    }
}

#[test]
fn threading_keeps_the_step_position() {
    let f = expand("(-> x\n  (f a))");
    assert_eq!(lc(&f), (2, 3));
    assert_eq!(lc(&f.as_list().unwrap_or(&[])[1]), (1, 5));
}
