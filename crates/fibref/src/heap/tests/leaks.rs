//! The end-of-run audit (§6): cycle leaks through cells are permitted,
//! every other live object is a bug, and an immutable cycle is loud.

use super::{atom, cell, imm};
use crate::heap::{Heap, Kind, LeakClass, Value};

#[test]
fn clean_run_reports_nothing() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Int(1)]);
    let b = imm(&mut heap, vec![Value::Ref(a)]);
    heap.release(a).expect("release a");
    heap.release(b).expect("release b");
    let report = heap.finish();
    assert!(report.is_clean());
    assert!(!report.is_cycle_leak_only());
    assert_eq!((report.allocated, report.freed), (2, 2));
}

#[test]
fn cycle_through_a_cell_is_a_leak_cycle() {
    // Case 15: (let ((c (cell []))) (set! c [c]) ...)
    let mut heap = Heap::new();
    let empty = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Ref(empty));
    heap.release(empty).expect("release temp");
    let vec = imm(&mut heap, vec![Value::Ref(c)]);
    heap.write(c, 0, Value::Ref(vec)).expect("set!");
    heap.release(vec).expect("release temp");
    heap.release(c).expect("let ends");
    let report = heap.finish();
    assert!(report.is_cycle_leak_only());
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, vec]);
    assert_eq!(report.freed, 1);
}

#[test]
fn cycle_through_an_atom_is_a_leak_cycle() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    let holder = imm(&mut heap, vec![Value::Ref(a)]);
    heap.write(a, 0, Value::Ref(holder)).expect("reset!");
    heap.release(holder).expect("release temp");
    heap.release(a).expect("scope ends");
    let report = heap.finish();
    assert!(report.is_cycle_leak_only());
    assert_eq!(report.leaks.len(), 2);
}

#[test]
fn objects_hanging_off_a_cell_cycle_are_leak_cycle_too() {
    let mut heap = Heap::new();
    let payload = imm(&mut heap, vec![Value::Int(7)]);
    let c = cell(&mut heap, Value::Nil);
    let node = imm(&mut heap, vec![Value::Ref(c), Value::Ref(payload)]);
    heap.write(c, 0, Value::Ref(node)).expect("set!");
    for held in [payload, node, c] {
        heap.release(held).expect("scope ends");
    }
    let report = heap.finish();
    assert!(report.is_cycle_leak_only());
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![payload, c, node]);
}

#[test]
fn acyclic_live_object_is_a_leak() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Int(1)]);
    let _forgotten = imm(&mut heap, vec![Value::Ref(a)]);
    heap.release(a).expect("release a");
    let report = heap.finish();
    assert!(!report.is_clean());
    assert!(!report.is_cycle_leak_only());
    assert_eq!(report.ids_in(LeakClass::Leak).len(), 2);
    assert_eq!(report.bugs().count(), 2);
}

#[test]
fn a_leaked_cell_with_no_cycle_is_a_plain_leak() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Int(1));
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::Leak), vec![c]);
    assert!(report.ids_in(LeakClass::LeakCycle).is_empty());
}

#[test]
fn a_bug_beside_a_cycle_leak_is_still_reported() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let node = imm(&mut heap, vec![Value::Ref(c)]);
    heap.write(c, 0, Value::Ref(node)).expect("set!");
    heap.release(node).expect("release temp");
    heap.release(c).expect("scope ends");
    let forgotten = imm(&mut heap, vec![]);
    let report = heap.finish();
    assert!(!report.is_cycle_leak_only());
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, node]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![forgotten]);
}

#[test]
fn a_cycle_with_no_cell_is_an_immutable_cycle() {
    // The public API cannot build this (§1); the field is forged to
    // prove the detector fires if a caller ever manages it.
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Nil]);
    let b = imm(&mut heap, vec![Value::Ref(a)]);
    heap.retain(b).expect("count the forged reference to b");
    heap.forge_field(a, 0, Value::Ref(b));
    heap.release(a).expect("scope ends");
    heap.release(b).expect("scope ends");
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::ImmutableCycle), vec![a, b]);
    assert!(!report.is_cycle_leak_only());
    assert_eq!(report.bugs().count(), 2);
}

#[test]
fn immutable_cycle_wins_over_leak_cycle() {
    // A cell on a cycle points into a forged immutable cycle: the
    // immutable cycle is the louder finding for its members.
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Nil]);
    let b = imm(&mut heap, vec![Value::Ref(a)]);
    heap.retain(b).expect("count the forged reference to b");
    heap.forge_field(a, 0, Value::Ref(b));
    let c = cell(&mut heap, Value::Nil);
    let node = imm(&mut heap, vec![Value::Ref(c), Value::Ref(a)]);
    heap.write(c, 0, Value::Ref(node)).expect("set!");
    for held in [a, b, c, node] {
        heap.release(held).expect("scope ends");
    }
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::ImmutableCycle), vec![a, b]);
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, node]);
}

#[test]
fn report_records_kind_and_count() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Int(1));
    heap.retain(c).expect("retain");
    let report = heap.finish();
    assert_eq!(report.leaks[0].kind, Kind::Cell);
    assert_eq!(report.leaks[0].count, 2);
}
