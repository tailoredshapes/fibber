//! The macros of tranche 2 X3 (stdlib §4.3 to §4.14, §6.3, D2), each
//! against the text of its expansion in the dump format, and each error
//! against its message: `if-not when-not some declare vector hash-set
//! array-map hash-map max-key min-key vswap! print-str pr-str println-str
//! prn-str` and the six of D2, `try-let if-some when-some when-first
//! doseq for`.

use super::{ex, ex_err};
use crate::expand::error::ExpandErrorKind as K;

const CONCAT: &str = "fib.prelude/str-concat";

#[test]
fn if_not_negates_the_test_and_the_one_armed_form_has_the_unit_else() {
    assert_eq!(ex("(if-not c a b)"), "(if (fib.prelude/not c) a b)");
    assert_eq!(ex("(if-not c a)"), "(if (fib.prelude/not c) a ())");
    assert!(matches!(ex_err("(if-not c)").kind, K::MacroArity { .. }));
    assert!(matches!(
        ex_err("(if-not c a b d)").kind,
        K::MacroArity { .. }
    ));
}

#[test]
fn when_not_is_unless_under_clojures_name() {
    assert_eq!(ex("(when-not c x)"), "(if c () x)");
    assert_eq!(ex("(when-not c x y)"), "(if c () (do x y))");
    assert_eq!(
        ex_err("(when-not)").to_string(),
        "t.fib:1:1: macro when-not takes at least 1 argument(s), got 0"
    );
}

#[test]
fn some_of_two_arguments_is_some_p_and_of_one_is_the_constructor() {
    assert_eq!(ex("(some p c)"), "(fib.seq/some-p p c)");
    assert_eq!(ex("(some x)"), "(some x)");
    assert_eq!(ex("(some (f a) (g b))"), "(fib.seq/some-p (f a) (g b))");
    assert_eq!(
        ex("(match o ((some x) x) (_ 0))"),
        "(match o ((some x) x) (_ 0))"
    );
    for src in ["(some)", "(some a b c)"] {
        let message = ex_err(src).to_string();
        assert!(
            message.ends_with(" macro some takes 1 or 2 argument(s), got 3")
                || message.ends_with(" macro some takes 1 or 2 argument(s), got 0"),
            "{src}: {message}"
        );
    }
}

#[test]
fn declare_is_an_empty_do_and_takes_symbols() {
    assert_eq!(ex("(declare a b)"), "(do)");
    assert_eq!(ex("(declare)"), "(do)");
    let kind = ex_err("(declare a 1)").kind;
    assert!(matches!(kind, K::Malformed { ref head, .. } if head == "declare"));
}

#[test]
fn vector_is_the_literal() {
    // the literal `[a b]` is itself lowered by the expander: the macro's
    // output is the literal, so its dump is the literal's
    assert_eq!(ex("(vector a b)"), ex("[a b]"));
    assert_eq!(ex("(vector)"), ex("[]"));
    assert_eq!(ex("(vector (f a))"), ex("[(f a)]"));
    assert_eq!(
        ex("(vector a b)"),
        "(fib.prelude/vec-conj (fib.prelude/vec-conj (fib.prelude/vec-empty) a) b)"
    );
}

#[test]
fn hash_set_is_a_conj_per_element_over_the_empty_set() {
    assert_eq!(ex("(hash-set)"), "(fib.prelude/set-empty)");
    assert_eq!(
        ex("(hash-set a b)"),
        "(fib.coll/conj (fib.coll/conj (fib.prelude/set-empty) a) b)"
    );
}

#[test]
fn array_map_is_the_map_literal_and_an_odd_count_is_malformed() {
    assert_eq!(ex("(array-map k v)"), ex("{k v}"));
    assert_eq!(ex("(array-map)"), ex("{}"));
    assert_eq!(
        ex("(array-map k v)"),
        "(fib.prelude/map-assoc (fib.prelude/map-empty) k v)"
    );
    assert_eq!(
        ex_err("(array-map k v w)").to_string(),
        "t.fib:1:1: malformed array-map: a key without a value"
    );
}

#[test]
fn hash_map_assocs_over_the_hashed_map() {
    assert_eq!(ex("(hash-map)"), "(fib.prelude/map-empty-hashed)");
    assert_eq!(
        ex("(hash-map a 1 b 2)"),
        "(fib.prelude/map-assoc (fib.prelude/map-assoc (fib.prelude/map-empty-hashed) a 1) b 2)"
    );
    assert_eq!(
        ex_err("(hash-map a 1 b)").to_string(),
        "t.fib:1:1: malformed hash-map: a key without a value"
    );
}

#[test]
fn max_key_and_min_key_fold_the_pair_function_with_the_key_bound_once() {
    assert_eq!(ex("(max-key k x)"), "x");
    assert_eq!(
        ex("(max-key k x y)"),
        "(let ((#k.1 k)) (fib.seq/max-key-pair #k.1 x y))"
    );
    assert_eq!(
        ex("(min-key (fn (v) (f v)) x y z)"),
        "(let ((#k.1 (fn (v) (f v)))) (fib.seq/min-key-pair #k.1 (fib.seq/min-key-pair #k.1 x y) z))"
    );
    assert_eq!(
        ex_err("(max-key k)").to_string(),
        "t.fib:1:1: macro max-key takes at least 2 argument(s), got 1"
    );
    assert!(matches!(
        ex_err("(min-key)").kind,
        K::MacroArity { found: 0, .. }
    ));
}

#[test]
fn vswap_reads_the_volatile_once_and_answers_the_new_value() {
    assert_eq!(
        ex("(vswap! v f a b)"),
        "(let ((#c.1 v)) (let ((#n.2 (f (fib.prelude/deref #c.1) a b))) \
         (do (fib.prelude/reset! #c.1 #n.2) #n.2)))"
    );
    assert!(matches!(
        ex_err("(vswap! v)").kind,
        K::MacroArity { found: 1, .. }
    ));
}

#[test]
fn the_text_printers_are_the_texts_the_writers_write() {
    assert_eq!(ex("(print-str)"), "\"\"");
    assert_eq!(ex("(pr-str)"), "\"\"");
    assert_eq!(ex("(println-str)"), "\"\\n\"");
    assert_eq!(ex("(prn-str)"), "\"\\n\"");
    assert_eq!(ex("(print-str a)"), "(fib.prelude/show a)");
    assert_eq!(ex("(pr-str a)"), "(fib.core/debug a)");
    assert_eq!(ex("(print-str nil)"), "\"nil\"");
    assert_eq!(
        ex("(print-str a \"b\")"),
        format!("({CONCAT} (fib.prelude/show a) ({CONCAT} \" \" (fib.prelude/show \"b\")))")
    );
    assert_eq!(
        ex("(println-str a b)"),
        format!(
            "({CONCAT} ({CONCAT} (fib.prelude/show a) ({CONCAT} \" \" (fib.prelude/show b))) \"\\n\")"
        )
    );
    assert_eq!(
        ex("(prn-str a)"),
        format!("({CONCAT} (fib.core/debug a) \"\\n\")")
    );
}

#[test]
fn the_writer_without_a_newline_is_print_raw() {
    assert_eq!(
        ex("(print a)"),
        "(fib.prelude/print-raw (fib.prelude/show a))"
    );
    assert_eq!(ex("(pr a)"), "(fib.prelude/print-raw (fib.core/debug a))");
    assert_eq!(ex("(fib.prelude/print-raw s)"), "(fib.prelude/print-raw s)");
}

#[test]
fn try_let_nests_matches_and_rebuilds_the_first_err() {
    assert_eq!(ex("(try-let () body)"), "body");
    assert_eq!(
        ex("(try-let ((x e)) body)"),
        "(match e ((fib.prelude/Ok x) body) ((fib.prelude/Err #err.1) (fib.prelude/Err #err.1)))"
    );
    assert_eq!(
        ex("(try-let ((x e) (y f)) a b)"),
        "(match e ((fib.prelude/Ok x) (match f ((fib.prelude/Ok y) (do a b)) \
         ((fib.prelude/Err #err.1) (fib.prelude/Err #err.1)))) \
         ((fib.prelude/Err #err.2) (fib.prelude/Err #err.2)))"
    );
    assert_eq!(ex("(try-let [x e] x)"), ex("(try-let ((x e)) x)"));
    assert!(matches!(
        ex_err("(try-let ((x)) b)").kind,
        K::Malformed { .. }
    ));
    assert!(matches!(
        ex_err("(try-let [x] b)").kind,
        K::Malformed { .. }
    ));
    assert!(matches!(
        ex_err("(try-let ((x e)))").kind,
        K::MacroArity { .. }
    ));
}

#[test]
fn the_presence_macros_are_a_match_on_some() {
    assert_eq!(
        ex("(if-some [x e] a b)"),
        "(match e ((fib.prelude/some x) a) (_ b))"
    );
    assert_eq!(
        ex("(if-some (x e) a)"),
        "(match e ((fib.prelude/some x) a) (_ ()))"
    );
    assert_eq!(
        ex("(when-some [x e] a b)"),
        "(match e ((fib.prelude/some x) (do a b)) (_ ()))"
    );
    assert_eq!(
        ex("(when-first [x c] a)"),
        "(match (fib.seq/first c) ((fib.prelude/some x) a) (_ ()))"
    );
    for src in [
        "(if-some x a b)",
        "(when-some [x] a)",
        "(when-first [x c d] a)",
    ] {
        assert!(matches!(ex_err(src).kind, K::Malformed { .. }), "{src}");
    }
}

#[test]
fn for_maps_filters_cuts_and_nests() {
    assert_eq!(ex("(for [x xs] b)"), "(fib.seq/map (fn (x) b) xs)");
    assert_eq!(
        ex("(for [x xs :when p] b)"),
        "(fib.seq/map (fn (x) b) (fib.seq/filter (fn (x) p) xs))"
    );
    assert_eq!(
        ex("(for [x xs :when p :while q] b c)"),
        "(fib.seq/map (fn (x) (do b c)) (fib.seq/take-while (fn (x) q) (fib.seq/filter (fn (x) p) xs)))"
    );
    assert_eq!(
        ex("(for [x xs :let [y e z f]] b)"),
        "(fib.seq/map (fn (x) (let ((y e) (z f)) b)) xs)"
    );
    assert_eq!(
        ex("(for [x xs y ys] b)"),
        "(fib.seq/mapcat (fn (x) (fib.seq/map (fn (y) b) ys)) xs)"
    );
}

#[test]
fn for_guards_after_a_let_by_keep_inside_and_take_zero_outside() {
    assert_eq!(
        ex("(for [x xs :let [y e] :when p] b)"),
        "(fib.seq/keep (fn (x) (let ((y e)) (if p (fib.prelude/some b) nil))) xs)"
    );
    assert_eq!(
        ex("(for [x xs :let [y e] :when p z zs] b)"),
        "(fib.seq/mapcat (fn (x) (let ((y e)) (let ((#s.1 (fib.seq/map (fn (z) b) zs))) \
         (if p #s.1 (fib.seq/take 0 #s.1))))) xs)"
    );
}

#[test]
fn for_refuses_what_it_cannot_say() {
    let cases = [
        (
            "(for [x xs :let [y e] :while p] b)",
            "malformed for: :while after :let is not supported",
        ),
        (
            "(for [:when p] b)",
            "malformed for: a modifier follows a binding",
        ),
        (
            "(for [x] b)",
            "malformed for: a binding is a pattern and a source",
        ),
        (
            "(for [x xs :when] b)",
            "malformed for: a binding is a pattern and a source",
        ),
        (
            "(for [x xs :wat p] b)",
            "malformed for: a modifier is :when, :while or :let",
        ),
        (
            "(for [x xs :let [y]] b)",
            "malformed for: :let takes [pattern expression ..]",
        ),
        (
            "(for x b)",
            "malformed for: the bindings are [pattern source ..]",
        ),
        (
            "(for [] b)",
            "malformed for: the bindings are [pattern source ..]",
        ),
    ];
    for (src, text) in cases {
        let message = ex_err(src).to_string();
        assert!(message.contains(text), "{src}: {message}");
    }
    assert!(matches!(ex_err("(for [x xs])").kind, K::MacroArity { .. }));
}

#[test]
fn doseq_is_for_each_over_the_same_levels_with_a_unit_body() {
    assert_eq!(
        ex("(doseq [x xs] a b)"),
        "(fib.seq/run! (fn (x) (do a b ())) xs)"
    );
    assert_eq!(
        ex("(doseq [x xs :when p y ys] b)"),
        "(fib.seq/run! (fn (x) (fib.seq/run! (fn (y) (do b ())) ys)) (fib.seq/filter (fn (x) p) xs))"
    );
    assert_eq!(
        ex("(doseq [x xs :let [y e] :when p] b)"),
        "(fib.seq/run! (fn (x) (let ((y e)) (if p (do b ()) ()))) xs)"
    );
    assert_eq!(ex("(doseq [x xs])"), "(fib.seq/run! (fn (x) (do ())) xs)");
}

#[test]
fn doseq_over_a_literal_range_is_the_counting_loop() {
    let out = ex("(doseq [i (range 0 n)] (f i))");
    assert!(
        out.starts_with("(let ((#s.1 0) (#m.2 n)) (loop ((i #s.1))"),
        "{out}"
    );
    assert!(!out.contains("run!"), "{out}");
}
