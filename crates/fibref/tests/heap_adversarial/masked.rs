//! Leaks that the permitted cycle leak could mask.
//!
//! §6 permits exactly one kind of leak: "an object unreachable except
//! through a cycle of cells". At the end of the run every binding is
//! gone, so under the counting semantics of §2 a live object's count
//! must equal the number of live `Ref` fields that name it. Any
//! surplus is a count some binding never gave back: a missing release,
//! which is a bug even when the object happens to sit on, or hang off,
//! a cycle through a cell. A classifier that looks only at reachability
//! from a cell on a cycle accepts such runs as "cycle leak only".

use fibref::{Heap, LeakClass, Value};

use crate::{cell, cell_cycle, imm, release_all};

#[test]
fn a_self_holding_cell_whose_binding_was_never_released_is_a_bug() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    heap.write(c, 0, Value::Ref(c)).expect("self reference");
    // The binding's count (the first of the two) is never released.
    assert_eq!(heap.count(c), Ok(2));
    let report = heap.finish();
    assert_eq!(report.leaks.len(), 1);
    assert_eq!(report.leaks[0].count, 2);
    assert!(
        report.bugs().next().is_some(),
        "a forgotten release was reported as the permitted cycle leak: {:?}",
        report.leaks
    );
    assert!(!report.is_cycle_leak_only());
}

#[test]
fn a_cycle_member_with_a_surplus_count_is_a_leak_not_leak_cycle() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    heap.release(c).expect("the binding's count on c");
    // The binding's count on v is never released: v has count 2 but
    // only one live Ref (the cell's slot) names it.
    assert_eq!(heap.count(v), Ok(2));
    assert_eq!(heap.count(c), Ok(1));
    let report = heap.finish();
    assert!(
        report.ids_in(LeakClass::Leak).contains(&v),
        "v's surplus count was hidden by the cycle: {:?}",
        report.leaks
    );
    assert!(!report.is_cycle_leak_only());
}

#[test]
fn a_surplus_count_on_a_tail_hanging_off_a_cycle_is_a_leak() {
    let mut heap = Heap::new();
    let tail = imm(&mut heap, vec![Value::Int(7)]);
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c), Value::Ref(tail)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    release_all(&mut heap, &[v, c]);
    // tail's binding is never released: count 2, one live Ref names it.
    assert_eq!(heap.count(tail), Ok(2));
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert_eq!(
        report.ids_in(LeakClass::Leak),
        vec![tail],
        "a tail with a forgotten release was laundered by the cycle: {:?}",
        report.leaks
    );
}

#[test]
fn a_cycle_reaching_a_second_cycle_is_leak_cycle_throughout() {
    let mut heap = Heap::new();
    let (c2, v2) = cell_cycle(&mut heap);
    let c1 = cell(&mut heap, Value::Nil);
    let v1 = imm(&mut heap, vec![Value::Ref(c1), Value::Ref(c2)]);
    heap.write(c1, 0, Value::Ref(v1)).expect("set!");
    release_all(&mut heap, &[v1, c1]);
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c2, v2, c1, v1]);
    assert!(report.is_cycle_leak_only());
}

#[test]
fn cells_that_only_point_into_a_cycle_are_leaks_not_cycle_roots() {
    // A chain of cells leading into a cycle is not on the cycle and
    // not reachable from it. Each is a missing release, whatever its
    // kind.
    let mut heap = Heap::new();
    let (c, v) = cell_cycle(&mut heap);
    let p2 = cell(&mut heap, Value::Ref(c));
    let p1 = cell(&mut heap, Value::Ref(p2));
    heap.release(p2).expect("p1 holds it");
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![p2, p1]);
    assert!(!report.is_cycle_leak_only());
}

#[test]
fn a_cycle_whose_only_cell_holds_a_weak_is_not_a_cycle() {
    // c -> Weak(v), v -> Ref(c): the only edge out of the cell is not
    // counted, so nothing is on a cycle. Releasing v frees both.
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c)]);
    let w = heap.weak(v).expect("weak");
    heap.write(c, 0, w).expect("set! weak");
    heap.release(c).expect("v holds it");
    assert_eq!(heap.count(c), Ok(1));
    heap.release(v).expect("frees v then c");
    assert!(!heap.is_live(v));
    assert!(!heap.is_live(c));
    assert!(heap.finish().is_clean());
}

#[test]
fn an_object_held_only_by_a_cycle_and_a_weak_holder_is_leak_cycle() {
    let mut heap = Heap::new();
    let t = imm(&mut heap, vec![Value::Int(1)]);
    let w = heap.weak(t).expect("weak");
    let weak_holder = imm(&mut heap, vec![w]);
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c), Value::Ref(t)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    release_all(&mut heap, &[t, v, c]);
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![t, c, v]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![weak_holder]);
}
