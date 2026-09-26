//! Leaks the permitted cycle leak could hide: at the end of the run a
//! live object's count must equal the live `Ref` fields naming it, on
//! or off a cycle through a cell (§2, §6).

use super::{cell, imm};
use crate::heap::{Heap, LeakClass, Value};

#[test]
fn a_self_holding_cell_with_a_surplus_count_is_a_leak() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    heap.write(c, 0, Value::Ref(c)).expect("self reference");
    // The binding's count is never released.
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::Leak), vec![c]);
    assert_eq!((report.leaks[0].count, report.leaks[0].held), (2, 1));
    assert!(!report.is_cycle_leak_only());
}

#[test]
fn a_cycle_member_with_a_surplus_count_is_a_leak() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    heap.release(c).expect("the binding's count on c");
    // v's binding is never released.
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![v]);
    assert_eq!((report.leaks[1].count, report.leaks[1].held), (2, 1));
}

#[test]
fn a_cycle_member_with_a_deficit_count_is_a_leak() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    let other = imm(&mut heap, vec![Value::Ref(v)]);
    heap.release(c).expect("the binding's count on c");
    assert_eq!(heap.release(v), Ok(2), "the binding's count on v");
    assert_eq!(heap.release(v), Ok(1), "one too many; two holders remain");
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![v, other]);
    assert_eq!((report.leaks[1].count, report.leaks[1].held), (1, 2));
}

#[test]
fn a_tail_with_a_surplus_count_hanging_off_a_cycle_is_a_leak() {
    let mut heap = Heap::new();
    let tail = imm(&mut heap, vec![Value::Int(7)]);
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c), Value::Ref(tail)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    for held in [v, c] {
        heap.release(held).expect("scope ends");
    }
    // tail's binding is never released.
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![tail]);
    assert!(!report.is_cycle_leak_only());
}

#[test]
fn a_matching_count_on_a_cycle_stays_leak_cycle() {
    let mut heap = Heap::new();
    let tail = imm(&mut heap, vec![Value::Int(7)]);
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c), Value::Ref(tail)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    for held in [tail, v, c] {
        heap.release(held).expect("scope ends");
    }
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![tail, c, v]);
    assert!(report.leaks.iter().all(|l| l.count == l.held));
    assert!(report.is_cycle_leak_only());
}
