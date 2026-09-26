//! Nothing in the heap may recurse: a 200000-long list frees, audits
//! and crosses a thread on the ordinary test-thread stack.

use fibref::{Event, Heap, LeakClass, Value};

use crate::{cell, chain, imm, release_all};

const LONG: usize = 200_000;

#[test]
fn a_200000_long_chain_frees_without_stack_overflow() {
    let mut heap = Heap::new();
    let head = chain(&mut heap, LONG, Value::Nil);
    assert_eq!(heap.release(head), Ok(0));
    assert!(!heap.is_live(head));
    let frees = heap
        .trace()
        .iter()
        .filter(|e| matches!(e, Event::Free { .. }))
        .count();
    assert_eq!(frees, LONG);
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!((report.allocated, report.freed), (LONG, LONG));
}

#[test]
fn a_200000_long_chain_frees_through_a_cell_write_without_overflow() {
    let mut heap = Heap::new();
    let head = chain(&mut heap, LONG, Value::Nil);
    let c = cell(&mut heap, Value::Ref(head));
    heap.release(head).expect("cell holds it");
    heap.write(c, 0, Value::Nil)
        .expect("set! nil frees the whole list");
    assert!(!heap.is_live(head));
    heap.release(c).expect("scope ends");
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!(report.freed, LONG + 1);
}

#[test]
fn a_200000_long_leaked_chain_is_classified_without_overflow() {
    let mut heap = Heap::new();
    let head = chain(&mut heap, LONG, Value::Nil);
    let report = heap.finish();
    assert_eq!(report.leaks.len(), LONG);
    assert!(report.leaks.iter().all(|l| l.class == LeakClass::Leak));
    assert_eq!(report.leaks.last().map(|l| l.id), Some(head));
}

#[test]
fn a_200000_long_chain_crosses_a_thread_without_overflow() {
    let mut heap = Heap::new();
    let head = chain(&mut heap, LONG, Value::Nil);
    heap.mark_shared(head).expect("cross");
    let marked = heap
        .trace()
        .iter()
        .filter(|e| matches!(e, Event::Shared { .. }))
        .count();
    assert_eq!(marked, LONG);
    heap.release(head).expect("free");
    assert!(heap.finish().is_clean());
}

#[test]
fn a_100000_long_cycle_through_one_cell_is_leak_cycle_without_overflow() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let head = chain(&mut heap, 100_000, Value::Ref(c));
    heap.write(c, 0, Value::Ref(head)).expect("close the cycle");
    release_all(&mut heap, &[head, c]);
    let report = heap.finish();
    assert_eq!(report.leaks.len(), 100_001);
    assert!(report.leaks.iter().all(|l| l.class == LeakClass::LeakCycle));
    assert!(report.is_cycle_leak_only());
}

#[test]
fn a_100000_wide_object_frees_every_child() {
    let mut heap = Heap::new();
    let children: Vec<_> = (0..100_000)
        .map(|i| imm(&mut heap, vec![Value::Int(i)]))
        .collect();
    let fields = children.iter().map(|&id| Value::Ref(id)).collect();
    let parent = imm(&mut heap, fields);
    release_all(&mut heap, &children);
    assert!(children.iter().all(|&id| heap.count(id) == Ok(1)));
    heap.release(parent).expect("frees all");
    assert!(children.iter().all(|&id| !heap.is_live(id)));
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!(report.freed, 100_001);
}
