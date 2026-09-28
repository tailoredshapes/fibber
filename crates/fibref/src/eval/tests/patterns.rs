//! Vector patterns and guards (syntax §3.6; types §6.3, §8.3): the core
//! view of the prelude's `Vec` read natively, and whole programs.

use crate::eval::alloc::Placement;
use crate::eval::interp::Interp;
use crate::eval::value::Val;
use crate::heap::Event;
use crate::types::ty::Scalar;

use super::{clean, failed};

fn ints(n: usize) -> Vec<Val> {
    (0..n).map(|i| Val::Int(i as i64, Scalar::I64)).collect()
}

#[test]
fn the_view_reads_every_element_of_tries_of_one_two_and_three_levels() {
    let c = crate::own::check_source("(defun main () -> i64 0)", "<test>").expect("checks");
    let mut it = Interp::new(&c.typed, &c.owned);
    // 40: root leaf + tail; 1100: shift 10 (two branch levels); 33 * 32
    // + 5 = 1061 crosses 1056, the last length with shift 5.
    for n in [0, 1, 32, 33, 40, 1056, 1061, 1100] {
        let v = it.build_vec(&ints(n), Placement::Heap).expect("builds");
        assert_eq!(it.vec_len(&v).expect("len"), n);
        for i in 0..n {
            let e = it.vec_elem(&v, i).expect("elem");
            assert_eq!(e.as_int().expect("int"), i as i64, "n={n} i={i}");
        }
        it.release(&v).expect("frees");
    }
}

#[test]
fn an_element_read_takes_no_count_and_a_drop_retains_each_element_it_keeps() {
    let c = crate::own::check_source("(defun main () -> i64 0)", "<test>").expect("checks");
    let mut it = Interp::new(&c.typed, &c.owned);
    let strs: Vec<Val> = (0..3)
        .map(|i| it.new_str(format!("s{i}"), Placement::Heap).expect("str"))
        .collect();
    let v = it.build_vec(&strs, Placement::Heap).expect("builds");
    it.give_back(&strs).expect("the vector holds them");
    let ids: Vec<_> = strs.iter().filter_map(Val::obj).collect();
    let retains = |it: &Interp| {
        it.heap
            .trace()
            .iter()
            .filter(|e| matches!(e, Event::Retain { id, .. } if ids.contains(id)))
            .count()
    };
    let before = retains(&it);
    let e = it.vec_elem(&v, 1).expect("elem");
    assert_eq!(it.string(&e).expect("live"), "s1");
    assert_eq!(retains(&it), before, "an element read is a derived read");
    let r = it.vec_drop(&v, 1).expect("drops");
    assert_eq!(it.vec_len(&r).expect("len"), 2);
    assert_eq!(retains(&it), before + 2, "the rest holds s1 and s2");
    it.release(&v).expect("frees v");
    let e = it.vec_elem(&r, 0).expect("elem of the rest");
    assert_eq!(it.string(&e).expect("still live"), "s1");
    it.release(&r).expect("frees r");
}

#[test]
fn guards_fall_through_in_order_and_rests_are_built_per_clause() {
    clean(
        "(defun f (v: (Vec i64)) -> i64
           (match v
             ([x & r] :when (> x 5) (count r))
             ([x & r] :when (> (count r) 1) (* 10 x))
             ([_ & r] (* 100 (count r)))
             ([] 1000)))
         (defun main () -> i64
           (+ (f [9 1 1]) (+ (f [1 2 3]) (+ (f [1]) (f [])))))",
        2 + 10 + 1000,
    );
}

#[test]
fn a_deep_rest_recursion_frees_every_rest() {
    clean(
        "(defun sum (v: (Vec i64) acc: i64) -> i64
           (match v ([x & r] (sum r (+ acc x))) ([] acc)))
         (defun main () -> i64 (sum (range 100) 0))",
        4950,
    );
}

#[test]
fn a_nested_rest_inside_an_option_inside_a_struct() {
    clean(
        "(defstruct W (o: (Option (Vec str))))
         (defun main () -> i64
           (match (W (some [\"a\" \"b\" \"c\"]))
             ((W (some [\"a\" & r])) (count r))
             ((W _) 0)))",
        2,
    );
}

#[test]
fn a_guard_that_traps_is_a_trap_not_a_fallthrough() {
    let m = failed(
        "(defun main () -> i64
           (match [1] ([x] :when (> (/ 1 (- x 1)) 0) 1) (_ 2)))",
    );
    assert!(m.contains("trap: integer / by zero"), "{m}");
}
