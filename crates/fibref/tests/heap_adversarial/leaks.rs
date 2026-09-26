//! End-of-run classification (§6): only what is reachable from a cell
//! or atom on a cycle is `LeakCycle`; every other live object is a bug.

use fibref::{AuditError, Heap, Kind, LeakClass, Value};

use crate::{atom, cell, cell_cycle, imm, release_all};

#[test]
fn clean_run_is_clean() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Int(1)]);
    let c = cell(&mut heap, Value::Ref(a));
    release_all(&mut heap, &[a, c]);
    let report = heap.finish();
    assert!(report.is_clean());
    assert!(!report.is_cycle_leak_only());
    assert_eq!(report.bugs().count(), 0);
    assert_eq!((report.allocated, report.freed), (2, 2));
}

#[test]
fn cell_to_vector_back_to_cell_is_leak_cycle_not_leak() {
    let mut heap = Heap::new();
    let (c, v) = cell_cycle(&mut heap);
    assert!(heap.is_live(c) && heap.is_live(v));
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert!(report.ids_in(LeakClass::Leak).is_empty());
    assert!(report.ids_in(LeakClass::ImmutableCycle).is_empty());
    assert!(report.is_cycle_leak_only());
    assert_eq!(report.bugs().count(), 0);
}

#[test]
fn unrelated_acyclic_live_object_beside_a_cycle_is_leak() {
    let mut heap = Heap::new();
    let (c, v) = cell_cycle(&mut heap);
    let stray = imm(&mut heap, vec![Value::Int(3)]);
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![stray]);
    assert!(!report.is_cycle_leak_only());
    assert_eq!(report.bugs().map(|l| l.id).collect::<Vec<_>>(), vec![stray]);
}

#[test]
fn two_disjoint_cycles_are_both_leak_cycle() {
    let mut heap = Heap::new();
    let (c1, v1) = cell_cycle(&mut heap);
    let (c2, v2) = cell_cycle(&mut heap);
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c1, v1, c2, v2]);
    assert!(report.is_cycle_leak_only());
}

#[test]
fn a_tail_hanging_off_a_cycle_is_leak_cycle_too() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let tail2 = imm(&mut heap, vec![Value::Int(2)]);
    let tail1 = imm(&mut heap, vec![Value::Ref(tail2)]);
    let v = imm(&mut heap, vec![Value::Ref(c), Value::Ref(tail1)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    release_all(&mut heap, &[tail2, tail1, v, c]);
    let report = heap.finish();
    assert_eq!(
        report.ids_in(LeakClass::LeakCycle),
        vec![c, tail2, tail1, v]
    );
    assert!(report.ids_in(LeakClass::Leak).is_empty());
    assert!(report.is_cycle_leak_only());
}

#[test]
fn a_cell_hanging_off_a_cycle_is_leak_cycle() {
    let mut heap = Heap::new();
    let c2 = cell(&mut heap, Value::Int(9));
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c), Value::Ref(c2)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    release_all(&mut heap, &[c2, v, c]);
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c2, c, v]);
    assert!(report.is_cycle_leak_only());
}

#[test]
fn a_parent_of_a_cycle_that_the_cycle_cannot_reach_is_leak() {
    // The parent was never released: that is a missing release, not
    // a permitted cycle leak, even though it points into a cycle.
    let mut heap = Heap::new();
    let (c, v) = cell_cycle(&mut heap);
    let parent = imm(&mut heap, vec![Value::Ref(c)]);
    assert_eq!(heap.count(c), Ok(2));
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![parent]);
    assert!(!report.is_cycle_leak_only());
}

#[test]
fn a_weak_back_pointer_does_not_make_a_cycle() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let w = heap.weak(c).expect("weak");
    let v = imm(&mut heap, vec![w]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    heap.release(v).expect("v held by c only");
    // c is never released: a plain leak, and v with it.
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::Leak), vec![c, v]);
    assert!(report.ids_in(LeakClass::LeakCycle).is_empty());
    assert!(!report.is_cycle_leak_only());
}

#[test]
fn a_weak_edge_out_of_a_cycle_does_not_launder_a_leak() {
    let mut heap = Heap::new();
    let t = imm(&mut heap, vec![Value::Int(1)]);
    let w = heap.weak(t).expect("weak");
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c), w]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    release_all(&mut heap, &[v, c]);
    // t is never released.
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![t]);
    assert!(!report.is_cycle_leak_only());
}

#[test]
fn breaking_a_cycle_before_finish_is_clean() {
    let mut heap = Heap::new();
    let (c, v) = cell_cycle(&mut heap);
    heap.write(c, 0, Value::Nil).expect("break the cycle");
    assert!(!heap.is_live(v));
    assert!(!heap.is_live(c), "the cell was held only by v");
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!(report.freed, 2);
}

#[test]
fn an_atom_holding_itself_is_a_leak_cycle() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    heap.write(a, 0, Value::Ref(a)).expect("reset!");
    heap.release(a).expect("scope ends");
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![a]);
    assert!(report.is_cycle_leak_only());
    assert_eq!(report.leaks[0].kind, Kind::Atom);
    assert_eq!(report.leaks[0].count, 1);
}

#[test]
fn a_cycle_through_two_cells_is_leak_cycle_for_all_members() {
    let mut heap = Heap::new();
    let c1 = cell(&mut heap, Value::Nil);
    let c2 = cell(&mut heap, Value::Ref(c1));
    let v = imm(&mut heap, vec![Value::Ref(c2)]);
    heap.write(c1, 0, Value::Ref(v)).expect("set!");
    release_all(&mut heap, &[v, c2, c1]);
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c1, c2, v]);
    assert!(report.is_cycle_leak_only());
}

#[test]
fn a_leaked_cell_with_no_cycle_is_a_plain_leak_with_its_contents() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Ref(x));
    heap.release(x).expect("caller's count");
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::Leak), vec![x, c]);
    assert!(report.ids_in(LeakClass::LeakCycle).is_empty());
    assert_eq!(report.bugs().count(), 2);
}

#[test]
fn an_immutable_cycle_cannot_be_built_through_the_public_api() {
    // §1: immutable objects refer only to older objects, and the heap
    // refuses the one route around that (a write). So the detector
    // stays quiet on a real run; the in-crate tests forge a field to
    // prove it fires.
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Nil]);
    let b = imm(&mut heap, vec![Value::Ref(a)]);
    assert_eq!(
        heap.write(a, 0, Value::Ref(b)),
        Err(AuditError::WriteToImmutable { id: a })
    );
    let report = heap.finish();
    assert!(report.ids_in(LeakClass::ImmutableCycle).is_empty());
    assert_eq!(report.ids_in(LeakClass::Leak), vec![a, b]);
}

#[test]
fn report_lists_leaks_in_allocation_order_with_kind_and_count() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Int(1)]);
    let c = cell(&mut heap, Value::Ref(a));
    let at = atom(&mut heap, Value::Ref(c));
    heap.retain(at).expect("retain");
    let report = heap.finish();
    let summary: Vec<_> = report
        .leaks
        .iter()
        .map(|l| (l.id, l.kind, l.count, l.class))
        .collect();
    assert_eq!(
        summary,
        vec![
            (a, Kind::Immutable, 2, LeakClass::Leak),
            (c, Kind::Cell, 2, LeakClass::Leak),
            (at, Kind::Atom, 2, LeakClass::Leak),
        ]
    );
    assert_eq!((report.allocated, report.freed), (3, 0));
}
