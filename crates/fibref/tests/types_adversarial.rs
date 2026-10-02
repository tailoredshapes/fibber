//! Adversarial programs for name resolution and type inference
//! (spec/types.md §2–§5): programs written to slip through the checker
//! or to catch it depending on order, each with the verdict the spec
//! gives it.

use fibref::types::decls::ModuleId;
use fibref::types::scheme::Scheme;
use fibref::types::ty::{Colour, Ty};
use fibref::types::{check_source, ErrorKind, SourceError, TypeError, TypedProgram};

fn check(src: &str) -> Result<TypedProgram, Vec<TypeError>> {
    match check_source(src, "adv.fib") {
        Ok(p) => Ok(p),
        Err(SourceError::Type(es)) => Err(es),
        Err(e) => panic!("{src}\ndoes not read or expand: {e:?}"),
    }
}

fn ok(src: &str) -> TypedProgram {
    check(src).unwrap_or_else(|es| panic!("{src}\n{}", es[0]))
}

fn fails(src: &str, kind: ErrorKind, text: &str) -> TypeError {
    let es = match check(src) {
        Ok(_) => panic!("{src}\ntype-checked; expected {kind:?}"),
        Err(es) => es,
    };
    assert_eq!(es[0].kind, kind, "{}", es[0]);
    assert!(
        es[0].message.starts_with(text),
        "expected `{text}`, got `{}`",
        es[0].message
    );
    es[0].clone()
}

fn scheme<'p>(p: &'p TypedProgram, name: &str) -> &'p Scheme {
    p.scheme(name)
        .unwrap_or_else(|| panic!("no scheme for {name}"))
}

#[test]
fn let_bound_fn_is_not_polymorphic() {
    // §2.4: let never generalises.
    fails(
        "(defun main () -> i64 (let ((id (fn (x) x))) (do (id 1) (str-len (id \"a\")))))",
        ErrorKind::Unify,
        "cannot unify str with i64",
    );
    // Nor through a second let, nor a closure returned by a defun and
    // bound once.
    fails(
        "(defun mk () (fn (x) x))
         (defun main () -> i64 (let ((f (mk)) (g f)) (do (g 1) (str-len (g \"a\")))))",
        ErrorKind::Unify,
        "cannot unify",
    );
    // A cell cannot acquire a polymorphic type (no value restriction needed).
    fails(
        "(defun main () -> i64 (let ((c (cell nil))) (do (set! c (some 1)) (set! c (some \"a\")) 0)))",
        ErrorKind::Unify,
        "cannot unify",
    );
    // The defun itself is generalised: two instantiations are fine.
    ok("(defun id (x) x) (defun main () -> i64 (do (str-len (id \"a\")) (id 1)))");
}

#[test]
fn argument_order_does_not_change_the_verdict() {
    // SYNTHESIS.md: with a T ↝ (Option T) coercion, (f (some 1) 1) and
    // (f 1 (some 1)) were accepted or rejected by solving order. D3: no
    // coercions, so both are the same error.
    let f = "(defun f (x y) (if true x y))";
    let a = fails(
        &format!("{f} (defun main () -> i64 (do (f (some 1) 1) 0))"),
        ErrorKind::Unify,
        "cannot unify",
    );
    let b = fails(
        &format!("{f} (defun main () -> i64 (do (f 1 (some 1)) 0))"),
        ErrorKind::Unify,
        "cannot unify",
    );
    assert!(
        a.message.contains("(Option i64)") && a.message.contains("i64"),
        "{a}"
    );
    assert!(
        b.message.contains("(Option i64)") && b.message.contains("i64"),
        "{b}"
    );
}

#[test]
fn occurs_check_fails() {
    fails(
        "(defun f (x) (f [x])) (defun main () -> i64 0)",
        ErrorKind::Infinite,
        "cannot construct the infinite type",
    );
    fails(
        "(defun main () -> i64 (let ((g (fn (h) (h h)))) 0))",
        ErrorKind::Infinite,
        "cannot construct the infinite type",
    );
}

#[test]
fn field_access_on_an_unannotated_parameter_is_an_error() {
    // §3.4: no search for "the unique struct with a field x", even when
    // only one struct has one.
    fails(
        "(defstruct P (x: i64)) (defun getx (p) (. p x)) (defun main () -> i64 (getx (P 1)))",
        ErrorKind::FieldUnresolved,
        "cannot infer the struct type of p for field x; annotate it",
    );
    // A use in the same SCC that fixes the head resolves it.
    ok(
        "(defstruct P (x: i64)) (defun getx (p) (do (P 0) (. (id p) x))) (defun id (q: P) -> P q)
        (defun main () -> i64 (getx (P 1)))",
    );
}

#[test]
fn mutual_recursion_is_generalised_together() {
    let p = ok("(defun f (x n) (if (= n 0) x (g x (- n 1))))
                (defun g (x n) (f x n))
                (defun main () -> i64 (do (str-len (f \"s\" 3)) (g 1 2)))");
    for name in ["f", "g"] {
        let s = scheme(&p, name);
        assert_eq!(p.show_scheme(s), "∀a. (fn :send (a i64) a)", "{name}");
    }
    // Inside the SCC each is monomorphic: a use at two types fails ...
    fails(
        "(defun f (x) (do (g 1) (str-len (g \"a\")) x))
         (defun g (y) (f y))
         (defun main () -> i64 0)",
        ErrorKind::Unify,
        "cannot unify",
    );
    // ... unless the used function is fully annotated (§3.6).
    ok("(defun f (x) (do (g 1) (str-len (g \"a\")) x))
        (defun g (y: a) -> a (do (f 0) y))
        (defun main () -> i64 0)");
}

#[test]
fn protocol_method_without_an_instance() {
    fails(
        "(defprotocol Describe (describe (self) -> str))
         (defstruct Tag (name: str))
         (impl Describe Tag (describe (self) (. self name)))
         (defun main () -> i64 (str-len (describe 1)))",
        ErrorKind::NoInstance,
        "no implementation of Describe for i64",
    );
    // Through an instance context (proposed case 34).
    fails(
        "(defstruct (Wrap a) (v: a))
         (defprotocol Describe (describe (self) -> str))
         (impl Describe i64 (describe (self) \"n\"))
         (impl Describe (Wrap a) :where ((Describe a)) (describe (self) (describe (. self v))))
         (defun f (b) (describe b))
         (defun main () -> i64 (str-len (f (Wrap (cell 0)))))",
        ErrorKind::NoInstance,
        "no implementation of Describe for (Cell i64)",
    );
}

#[test]
fn overlapping_instances_are_rejected() {
    let proto = "(defprotocol Describe (describe (self) -> str))";
    let es = check(&format!(
        "{proto} (impl Describe i64 (describe (self) \"a\")) (impl Describe i64 (describe (self) \"b\"))
         (defun main () -> i64 0)"
    ))
    .expect_err("rejected");
    assert!(
        es[0]
            .message
            .starts_with("overlapping instances: Describe for i64"),
        "{}",
        es[0]
    );
    // An instance for a bare variable would overlap every other one.
    let es = check(&format!(
        "{proto} (impl Describe a (describe (self) \"a\")) (defun main () -> i64 0)"
    ))
    .expect_err("rejected");
    assert!(
        es[0]
            .message
            .starts_with("an instance head must not be a type variable"),
        "{}",
        es[0]
    );
    // A user instance overlapping a built-in one.
    let es = check("(impl Show i64 (show (self) \"x\")) (defun main () -> i64 0)")
        .expect_err("rejected");
    assert!(
        es[0]
            .message
            .starts_with("overlapping instances: Show for i64"),
        "{}",
        es[0]
    );
}

#[test]
fn local_closure_reaches_pmap_through_two_higher_order_functions() {
    let src = "(defun par (f xs) (pmap f xs))
               (defun par2 (g ys) (par g ys))
               (defun main () -> i64
                 (let ((n (cell 0)))
                   (do (par2 (fn (i) (+ @n i)) [0 1 2]) 0)))";
    let e = fails(
        src,
        ErrorKind::CellNotSend,
        "cell cannot be shared between threads",
    );
    assert_eq!(
        e.message,
        "cell cannot be shared between threads: closure capture n has type (Cell i64)"
    );
    // The requirement travelled through both schemes as a `send` colour.
    let p = ok(
        "(defun par (f xs) (pmap f xs)) (defun par2 (g ys) (par g ys)) (defun main () -> i64 0)",
    );
    assert_eq!(
        p.show_fun("par2").as_deref(),
        Some("∀a b. (Send b) (Send a) ⇒ (fn :send ((fn :send (a) b) (Vec a)) (Vec b))")
    );
    // And a closure that is not local passes.
    ok(
        "(defun par (f xs) (pmap f xs)) (defun par2 (g ys) (par g ys))
        (defun main () -> i64 (let ((k 1)) (vec-count (par2 (fn (i) (+ k i)) [0 1 2]))))",
    );
}

#[test]
fn a_closure_stored_and_called_later_keeps_its_colour_through_a_variable() {
    // A send closure passed through a generic position stays send even
    // when a local closure of the same type flows to the same place.
    ok("(defun main () -> i64
          (let ((c (cell 0)) (g (fn () 1)) (xs (list g (fn () @c))))
            (join (spawn g))))");
}

#[test]
fn send_is_checked_through_a_struct_field_and_an_option() {
    let s = "(defstruct S (c: (Cell i64)))";
    let e = fails(
        &format!("{s} (defun main () -> i64 (do (spawn (fn () (some (S (cell 0))))) 0))"),
        ErrorKind::CellNotSend,
        "cell cannot be shared between threads",
    );
    assert_eq!(
        e.message,
        "cell cannot be shared between threads: type argument a of spawn, payload of some, field c of S has type (Cell i64)"
    );
    let e = fails(
        &format!("{s} (defun main () -> i64 (let ((o (some (S (cell 0))))) (do (spawn (fn () (do o 0))) 0)))"),
        ErrorKind::CellNotSend,
        "cell cannot be shared between threads",
    );
    assert_eq!(e.message, "cell cannot be shared between threads: closure capture o, payload of some, field c of S has type (Cell i64)");
    // Through an element of a Vec (the library's Vec is a trie over Array;
    // the witness is the first path found, through the trie's root).
    let e = fails(
        "(defun main () -> i64 (let ((v [(cell 0)])) (do (spawn (fn () (vec-count v))) 0)))",
        ErrorKind::CellNotSend,
        "cell cannot be shared between threads: closure capture v, payload 2 of VecOf, payload of some, payload of VLeaf, element of (Array (Cell i64)) has type (Cell i64)",
    );
    assert_eq!(e.pos.line, 1);
}

#[test]
fn an_atom_containing_a_cell_is_rejected() {
    let e = fails(
        "(defun main () -> i64 (do (atom (cell 0)) 0))",
        ErrorKind::CellNotSend,
        "cell cannot be shared between threads",
    );
    assert_eq!(
        e.message,
        "cell cannot be shared between threads: type argument a of atom has type (Cell i64)"
    );
    // Inside a struct, too; and a generic function that makes an atom of
    // its argument carries the bound (§3.7).
    fails(
        "(defstruct B (c: (Cell i64))) (defun main () -> i64 (do (atom (B (cell 0))) 0))",
        ErrorKind::CellNotSend,
        "cell cannot be shared between threads",
    );
    let p = ok("(defun wrap (x) (atom x)) (defun main () -> i64 0)");
    assert_eq!(
        p.show_fun("wrap").as_deref(),
        Some("∀a. (Send a) ⇒ (fn :send (a) (Atom a))")
    );
    fails(
        "(defun wrap (x) (atom x)) (defun main () -> i64 (do (wrap (cell 0)) 0))",
        ErrorKind::CellNotSend,
        "cell cannot be shared between threads: type argument a of wrap has type (Cell i64)",
    );
}

#[test]
fn closure_colours_are_quantified_and_reinstantiated() {
    // §5.4: twice is usable in pmap iff its argument is.
    let p = ok("(defun twice (f) (fn (x) (f (f x)))) (defun main () -> i64 0)");
    let s = scheme(&p, "twice");
    assert_eq!(
        p.show_scheme(s),
        "∀a ς0 ς1. ς0 ⊑ ς1 ⇒ (fn :send ((fn ς0 (a) a)) (fn ς1 (a) a))"
    );
    let Ty::Fn(Colour::Send, ps, r) = &s.ty else {
        panic!("{:?}", s.ty)
    };
    assert!(
        matches!(&ps[0], Ty::Fn(Colour::Gen(_), _, _))
            && matches!(**r, Ty::Fn(Colour::Gen(_), _, _))
    );
    ok("(defun twice (f) (fn (x) (f (f x))))
        (defun main () -> i64 (vec-count (pmap (twice (fn (x) (+ x 1))) [0 1 2])))");
    fails(
        "(defun twice (f) (fn (x) (f (f x))))
         (defun main () -> i64 (let ((c (cell 1))) (vec-count (pmap (twice (fn (x) (+ x @c))) [0 1 2]))))",
        ErrorKind::CellNotSend,
        "cell cannot be shared between threads",
    );
}

#[test]
fn literals_have_fixed_types_and_nothing_is_coerced() {
    fails(
        "(defun f (x: i32) -> i32 x) (defun main () -> i64 (do (f 1) 0))",
        ErrorKind::Unify,
        "cannot unify i64 with i32",
    );
    ok("(defun f (x: i32) -> i32 x) (defun main () -> i64 (do (f 1i32) 0))");
    fails(
        "(defun f (x: (Option i64)) -> i64 0) (defun main () -> i64 (f 1))",
        ErrorKind::Unify,
        "cannot unify i64 with (Option i64)",
    );
}

#[test]
fn prelude_names_are_shadowed_by_user_definitions() {
    // Proposed case 45 defines its own Box.
    let p = ok("(defstruct (Box a) (v: a)) (defun main () -> i64 (. (Box 1) v))");
    let user_box = p
        .globals
        .type_name(ModuleId::MAIN, "Box")
        .expect("user Box");
    let prelude_box = p
        .globals
        .type_name(ModuleId::PRELUDE, "Box")
        .expect("prelude Box");
    assert_ne!(user_box, prelude_box);
    // The literal rewrite still reaches the prelude's conj.
    ok("(defun conj (a b) a) (defun main () -> i64 (vec-count [1 2 3]))");
}

#[test]
fn polymorphic_recursion_must_declare_its_bounds() {
    // §3.6: the annotation is the scheme of the recursive occurrences, so
    // a bound the body infers on the annotation's variables, if not
    // declared, would go unchecked at those occurrences.
    let e = fails(
        "(defun f (x: a) -> str (if true (show x) (f (cell 1)))) (defun main () -> i64 0)",
        ErrorKind::Other,
        "f is used polymorphically in its own definition, so its :where must list (Show a)",
    );
    assert_eq!(e.pos.line, 1);
    fails(
        "(defun f (x: a) :where ((Show a)) -> str (if true (show x) (f (cell 1)))) (defun main () -> i64 0)",
        ErrorKind::NoInstance,
        "no implementation of Show for (Cell i64)",
    );
    let p = ok("(defun f (x: a) :where ((Show a)) -> str (if true (show x) (f 1))) (defun main () -> i64 (str-len (f true)))");
    assert_eq!(
        p.show_fun("f").as_deref(),
        Some("∀a. (Show a) ⇒ (fn :send (a) str)")
    );
}

#[test]
fn a_generic_closure_maker_carries_its_capture_constraint() {
    // mk : ∀a ς. ς ⊒ Caps{a} ⇒ (fn (a) (fn ς () a)): the colour of the
    // closure it returns is decided at each call by the argument's type.
    let p = ok("(defun mk (x) (fn () x)) (defun main () -> i64 0)");
    assert_eq!(
        p.show_fun("mk").as_deref(),
        Some("∀a ς0. ς0 ⊒ Caps{a} ⇒ (fn :send (a) (fn ς0 () a))")
    );
    // A thunk whose result is sendable but whose capture is a cell.
    let mk = "(defun mk (x) (fn () (do x 1)))";
    ok(&format!(
        "{mk} (defun main () -> i64 (join (spawn (mk 1))))"
    ));
    fails(
        &format!("{mk} (defun main () -> i64 (let ((c (cell 1))) (join (spawn (mk c)))))"),
        ErrorKind::CellNotSend,
        "cell cannot be shared between threads: a capture of a closure made by mk has type (Cell i64)",
    );
    // The same through a closure that captures the maker's closure.
    fails(
        &format!("{mk} (defun main () -> i64 (let ((c (cell 1)) (k (mk c))) (join (spawn (fn () (k))))))"),
        ErrorKind::CellNotSend,
        "cell cannot be shared between threads: closure capture k, a capture of a closure made by mk has type (Cell i64)",
    );
}

#[test]
fn deep_nesting_does_not_exhaust_the_stack() {
    // The reader admits 1000 levels and macros may build up to the
    // expander's 2000 (expand::MAX_EXPAND_DEPTH); every pass is
    // recursive, so the checker runs on its own large stack.
    let mut src = String::from("(defun main () -> i64 ");
    src.push_str(&"(+ 1 ".repeat(995));
    src.push('1');
    src.push_str(&")".repeat(996));
    ok(&src);
    let args = vec!["true"; 1990].join(" ");
    ok(&format!("(defun main () -> i64 (if (and {args}) 1 0))"));
    let args = vec!["(some 1)"; 1990].join(" ");
    ok(&format!("(defun main () -> i64 (do (list {args}) 0))"));
}
