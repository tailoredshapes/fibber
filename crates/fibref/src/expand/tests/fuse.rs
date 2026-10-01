//! The fusion rewrite (stdlib design §2.1 rule 2, tranche 1 plan R9): the
//! rewritten forms are quoted, and so is every case where the rewrite must
//! not happen (a name that is the program's own, a position that is not a
//! collection position, a stage it cannot take).

use super::{read, ExpandCtx, NoRunner};
use crate::expand::expand_program;

/// The module `main`, which `:use`s these modules, expanded in a context
/// that has already expanded `earlier` (`(ns NAME ..)` programs).
fn module_in(ctx: &mut ExpandCtx, ns: &str, uses: &[&str], src: &str) -> Vec<String> {
    let uses: Vec<String> = uses.iter().map(|u| u.to_string()).collect();
    ctx.begin_module(ns, (&uses, &[]), Default::default());
    let forms =
        expand_program(read(src), ctx, &mut NoRunner).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    ctx.end_module();
    forms.iter().map(|f| f.to_string()).collect()
}

/// `src` as the module `main`, which `:use`s the three facades.
fn fused(src: &str) -> String {
    let uses = ["fib.core", "fib.seq", "fib.coll"];
    module_in(&mut ExpandCtx::new(), "main", &uses, src).join("\n")
}

/// One expression, as the body of a function of the names `v w f g p n`.
fn expr(e: &str) -> String {
    let all = fused(&format!("(defun m (v w f g p n) {e})"));
    let body = all
        .strip_prefix("(defun m (v w f g p n) ")
        .and_then(|s| s.strip_suffix(')'));
    body.unwrap_or_else(|| panic!("{all}")).to_string()
}

#[test]
fn a_threaded_chain_becomes_recipes_under_the_terminal() {
    assert_eq!(
        expr("(->> v (map f) (filter p) (reduce g 0))"),
        "(reduce g 0 (fib.seq/Filtered (fib.seq/Mapped v f) p))"
    );
    assert_eq!(
        expr("(count (take n (drop n (take-while p (mapcat f v)))))"),
        "(count (fib.seq/Taken (fib.seq/Dropped (fib.seq/TakenWhile (fib.seq/Mapcat v f) p) n) n))"
    );
}

#[test]
fn a_nested_chain_under_vec_fuses_all_three() {
    assert_eq!(
        expr("(vec (map f (filter p (map g v))))"),
        "(vec (fib.seq/Mapped (fib.seq/Filtered (fib.seq/Mapped v g) p) f))"
    );
}

#[test]
fn every_terminal_fuses_the_collection_argument_it_has() {
    let a = "(fib.seq/Mapped v f)";
    for (call, want) in [
        ("(reduce1 g (map f v))", format!("(reduce1 g {a})")),
        (
            "(reduce-while g 0 (map f v))",
            format!("(reduce-while g 0 {a})"),
        ),
        (
            "(reduce-nonempty g (map f v))",
            format!("(reduce-nonempty g {a})"),
        ),
        ("(empty? (map f v))", format!("(empty? {a})")),
        ("(last (map f v))", format!("(last {a})")),
        ("(into w (map f v))", format!("(into w {a})")),
        ("(set (map f v))", format!("(set {a})")),
        ("(run! g (map f v))", format!("(run! g {a})")),
        ("(every? p (map f v))", format!("(every? p {a})")),
        ("(not-any? p (map f v))", format!("(not-any? p {a})")),
        ("(find-first p (map f v))", format!("(find-first p {a})")),
        ("(find-map g (map f v))", format!("(find-map g {a})")),
        ("(group-by g (map f v))", format!("(group-by g {a})")),
        ("(frequencies (map f v))", format!("(frequencies {a})")),
        ("(sort (map f v))", format!("(sort {a})")),
        ("(sort-by g (map f v))", format!("(sort-by g {a})")),
        ("(sort-with g (map f v))", format!("(sort-with g {a})")),
        (
            "(sort-by-with g p (map f v))",
            format!("(sort-by-with g p {a})"),
        ),
        ("(sum (map f v))", format!("(sum {a})")),
    ] {
        assert_eq!(expr(call), want, "{call}");
    }
}

#[test]
fn nth_reads_its_collection_first() {
    assert_eq!(expr("(nth (map f v) n)"), "(nth (fib.seq/Mapped v f) n)");
    assert_eq!(expr("(nth v (map f w))"), "(nth v (map f w))");
}

#[test]
fn concat_rewrites_both_collections() {
    assert_eq!(
        expr("(count (concat (map f v) (filter p w)))"),
        "(count (fib.seq/Cat (fib.seq/Mapped v f) (fib.seq/Filtered w p)))"
    );
}

#[test]
fn remove_is_filtered_over_the_complement() {
    assert_eq!(
        expr("(count (remove p v))"),
        "(count (fib.seq/Filtered v (fn (#fuse.1) (fib.prelude/not (fib.core/truthy? (p #fuse.1))))))"
    );
    assert_eq!(
        expr("(count (remove (fn (x) (f x)) v))"),
        "(count (fib.seq/Filtered v (fn (x) (fib.prelude/not (fib.core/truthy? (f x))))))"
    );
}

#[test]
fn a_literal_function_is_taken_whole_and_a_call_is_not() {
    assert_eq!(
        expr("(count (map (fn (x) (f x)) v))"),
        "(count (fib.seq/Mapped v (fn (x) (f x))))"
    );
    assert_eq!(expr("(count (take 3 v))"), "(count (fib.seq/Taken v 3))");
    assert_eq!(expr("(count (take (dec n) v))"), "(count (take (dec n) v))");
    assert_eq!(
        expr("(count (map (make-f) (filter p v)))"),
        "(count (map (make-f) (filter p v)))"
    );
    assert_eq!(
        expr("(count (filter p (map (make-f) v)))"),
        "(count (fib.seq/Filtered (map (make-f) v) p))"
    );
}

#[test]
fn a_qualified_head_is_the_librarys_whatever_the_program_binds() {
    assert_eq!(
        expr("(let ((map 1)) (fib.seq/count (fib.seq/map f v)))"),
        "(let ((map 1)) (fib.seq/count (fib.seq/Mapped v f)))"
    );
    assert_eq!(
        expr("(fib.coll/vec (fib.seq/map f v))"),
        "(fib.coll/vec (fib.seq/Mapped v f))"
    );
    assert_eq!(
        expr("(fib.seq/reduce g 0 (fib.seq/filter p v))"),
        "(fib.seq/reduce g 0 (fib.seq/Filtered v p))"
    );
}

#[test]
fn a_chain_that_is_not_read_by_a_terminal_stays_lazy() {
    assert_eq!(expr("(map f (filter p v))"), "(map f (filter p v))");
    assert_eq!(
        expr("(let ((m (map f v))) (reduce g 0 m))"),
        "(let ((m (map f v))) (reduce g 0 m))"
    );
    assert_eq!(expr("(first (map f v))"), "(first (map f v))");
    assert_eq!(expr("(rest (map f v))"), "(rest (map f v))");
    assert_eq!(expr("(seq (filter p v))"), "(seq (filter p v))");
}

#[test]
fn a_terminal_inside_a_stage_or_its_function_fuses_on_its_own() {
    assert_eq!(
        expr("(count (map (fn (x) (count (filter p x))) (vec (map f v))))"),
        "(count (fib.seq/Mapped (vec (fib.seq/Mapped v f)) (fn (x) (count (fib.seq/Filtered x p)))))"
    );
}

#[test]
fn a_name_the_program_defines_is_not_the_librarys() {
    let src = |own: &str| format!("{own} (defun m (v f) (count (map f v)))");
    assert_eq!(
        fused(&src("(defun map (f x) x)")).lines().last(),
        Some("(defun m (v f) (count (map f v)))")
    );
    assert_eq!(
        fused(&src("(defun count (x) 1)")).lines().last(),
        Some("(defun m (v f) (count (map f v)))")
    );
    assert_eq!(
        fused(&src("(def map: i64 1)")).lines().last(),
        Some("(defun m (v f) (count (map f v)))")
    );
    assert_eq!(
        fused(&src("(defprotocol P (map (self) -> i64))"))
            .lines()
            .last(),
        Some("(defun m (v f) (count (map f v)))")
    );
}

#[test]
fn a_name_the_form_binds_is_not_the_librarys() {
    for bound in [
        "(let ((map f)) (count (map f v)))",
        "(let ((count f)) (count (map f v)))",
        "(fn (map) (count (map f v)))",
        "(match v ((some map) (count (map f v))) (nil 0))",
        "(loop ((map f)) (count (map f v)))",
    ] {
        assert_eq!(expr(bound), bound, "{bound}");
    }
    assert_eq!(
        fused("(defun m (v f) (count (map f v))) (defun k (map v) (count (map v v)))"),
        "(defun m (v f) (count (fib.seq/Mapped v f)))\n(defun k (map v) (count (map v v)))"
    );
}

#[test]
fn a_name_a_later_definition_of_the_module_shadows_is_not_the_librarys() {
    assert_eq!(
        fused("(defun m (v f) (count (map f v))) (defun map (f x) x)"),
        "(defun m (v f) (count (map f v)))\n(defun map (f x) x)"
    );
}

#[test]
fn a_name_a_used_user_module_exports_is_not_the_librarys() {
    let mut ctx = ExpandCtx::new();
    module_in(
        &mut ctx,
        "u",
        &[],
        "(defun map (f x) x) (defun hid :private (x) x)",
    );
    let main = "(defun m (v f) (count (map f v)) (hid v))";
    let uses = ["fib.core", "fib.seq", "u"];
    let out = module_in(&mut ctx, "main", &uses, main);
    assert_eq!(out, ["(defun m (v f) (count (map f v)) (hid v))"]);
    // a module it does not use does not
    let out = module_in(&mut ctx, "main2", &["fib.core", "fib.seq"], main);
    assert_eq!(
        out,
        ["(defun m (v f) (count (fib.seq/Mapped v f)) (hid v))"]
    );
}

#[test]
fn what_a_used_module_re_exports_counts_as_exported() {
    let mut ctx = ExpandCtx::new();
    module_in(&mut ctx, "u.inner", &[], "(defun map (f x) x)");
    ctx.reexport("u", &["u.inner".to_string()]);
    module_in(&mut ctx, "u", &[], "(defun other () 1)");
    let main = "(defun m (v f) (count (map f v)))";
    let out = module_in(&mut ctx, "main", &["fib.seq", "u"], main);
    assert_eq!(out, [main]);
}

#[test]
fn a_module_that_does_not_see_the_facade_is_not_rewritten() {
    let src = "(defun m (v f) (count (map f v)))";
    let mut ctx = ExpandCtx::new();
    assert_eq!(module_in(&mut ctx, "main", &[], src), [src]);
    assert_eq!(
        module_in(&mut ctx, "main", &["fib.core", "fib.coll"], src),
        [src]
    );
    let v = "(defun m (v f) (vec (map f v)))";
    assert_eq!(module_in(&mut ctx, "main", &["fib.seq"], v), [v]);
    assert_eq!(
        module_in(&mut ctx, "main", &["fib.seq", "fib.coll"], v),
        ["(defun m (v f) (vec (fib.seq/Mapped v f)))"]
    );
}

#[test]
fn the_librarys_own_modules_are_not_rewritten() {
    let src = "(defun m (v f) (count (map f v)))";
    let mut ctx = ExpandCtx::new();
    assert_eq!(
        module_in(&mut ctx, "fib.seq.thing", &["fib.seq"], src),
        [src]
    );
}

#[test]
fn remove_needs_fib_core_and_the_other_stages_do_not() {
    let src = "(defun m (v p) (count (remove p v)))";
    let mut ctx = ExpandCtx::new();
    assert_eq!(module_in(&mut ctx, "main", &["fib.seq"], src), [src]);
    let other = "(defun m (v p) (count (filter p v)))";
    assert_eq!(
        module_in(&mut ctx, "main", &["fib.seq"], other),
        ["(defun m (v p) (count (fib.seq/Filtered v p)))"]
    );
}

#[test]
fn a_stage_of_the_wrong_arity_is_not_a_stage() {
    assert_eq!(expr("(count (map f v w))"), "(count (map f v w))");
    assert_eq!(expr("(count (map f))"), "(count (map f))");
    assert_eq!(
        expr("(reduce g (map f v))"),
        "(fib.seq/reduce-nonempty g (fib.seq/Mapped v f))"
    );
}

#[test]
fn quoted_data_and_patterns_are_left_alone() {
    assert_eq!(
        expr("(quote (count (map f v)))"),
        "(quote (count (map f v)))"
    );
}

#[test]
fn a_private_definition_keeps_its_marker_and_a_def_initialiser_is_rewritten() {
    assert_eq!(
        fused("(defun m :private (v f) (count (map f v)))"),
        "(defun m :private (v f) (count (fib.seq/Mapped v f)))"
    );
    assert_eq!(
        fused("(def n: i64 (count (map f v)))"),
        "(def n: i64 (count (fib.seq/Mapped v f)))"
    );
}

#[test]
fn an_impl_method_and_a_protocol_default_are_rewritten() {
    assert_eq!(
        fused("(impl P Q (m (self) (count (map f self))))"),
        "(impl P Q (m (self) (count (fib.seq/Mapped self f))))"
    );
    assert_eq!(
        fused("(defprotocol (R a) (m (self) -> i64 (count (map f self))))"),
        "(defprotocol (R a) (m (self) -> i64 (count (fib.seq/Mapped self f))))"
    );
}

#[test]
fn the_gensyms_of_two_removes_are_distinct_and_counted_in_the_context() {
    let mut ctx = ExpandCtx::new();
    let uses = ["fib.core", "fib.seq"].map(String::from);
    ctx.begin_module("main", (&uses, &[]), Default::default());
    let src = "(defun m (v p) (+ (count (remove p v)) (count (remove p v))))";
    let forms = expand_program(read(src), &mut ctx, &mut NoRunner).expect("expands");
    let text = forms[0].to_string();
    assert!(
        text.contains("#fuse.1") && text.contains("#fuse.2"),
        "{text}"
    );
    assert_eq!(ctx.gensym_count(), 2);
}

#[test]
fn a_form_the_fusing_walk_cannot_finish_is_kept_as_the_first_pass_left_it() {
    // The rewrite of `remove` puts a `fn`, a `not` and a `truthy?` between the
    // stage and its predicate: three levels more than the program has, which a
    // limit of exactly the program's depth does not allow. The program is
    // expanded all the same, as it was, and a limit with room has it fused.
    let src = "(defun m (v p) (do (do (do (count (remove p v))))))";
    let mut ctx = ExpandCtx::new();
    ctx.limits.max_depth = 6;
    let tight = module_in(&mut ctx, "main", &["fib.core", "fib.seq"], src);
    assert_eq!(
        tight,
        ["(defun m (v p) (do (do (do (count (remove p v))))))"]
    );
    let mut ctx = ExpandCtx::new();
    ctx.limits.max_depth = 12;
    let roomy = module_in(&mut ctx, "main", &["fib.core", "fib.seq"], src);
    assert!(roomy[0].contains("fib.seq/Filtered"), "{roomy:?}");
}

#[test]
fn the_expression_a_binding_calls_does_not_bind_the_names_it_uses() {
    // `map`, `filter` and `v` occur in the initialiser of `s`, which binds
    // only `s`: the chain under `count` is still the library's.
    assert_eq!(
        expr("(let ((s (map f (filter p v)))) (+ (count s) (count (map f (filter p v)))))"),
        "(let ((s (map f (filter p v)))) (+ (count s) (count (fib.seq/Mapped (fib.seq/Filtered v p) f))))"
    );
    assert_eq!(
        expr("(loop ((acc (take 2 v))) (count (filter p acc)))"),
        "(loop ((acc (take 2 v))) (count (fib.seq/Filtered acc p)))"
    );
}
