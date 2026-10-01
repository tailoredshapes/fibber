//! The trace records every operation, in order, with the documented
//! events (`spec/method.md`, rule 6).

use super::{atom, cell, imm};
use crate::heap::{trace_allocs, traced, Event, Heap, Kind, ObjId, Value};

/// The events the scripted run below must produce, for a cell `c`
/// holding an immutable `v`.
fn scripted_events(v: ObjId, c: ObjId) -> Vec<Event> {
    vec![
        Event::Alloc {
            id: v,
            kind: Kind::Immutable,
        },
        Event::Alloc {
            id: c,
            kind: Kind::Cell,
        },
        Event::Retain {
            id: v,
            count_after: 2,
        },
        Event::Read { id: c, field: 0 },
        Event::Release {
            id: v,
            count_after: 1,
        },
        Event::Weak { id: v },
        Event::Write { id: c, field: 0 },
        Event::Release {
            id: v,
            count_after: 0,
        },
        Event::Free { id: v },
        Event::Upgrade { id: v, live: false },
        Event::Release {
            id: c,
            count_after: 0,
        },
        Event::Free { id: c },
    ]
}

#[test]
fn a_scripted_run_traces_every_event_in_order() {
    let mut heap = Heap::new();
    let v = imm(&mut heap, vec![Value::Int(1)]);
    let c = cell(&mut heap, Value::Ref(v));
    assert_eq!(heap.read(c, 0), Ok(Value::Ref(v)));
    assert_eq!(heap.release(v), Ok(1));
    let w = heap.weak(v).expect("weak").as_weak().expect("is weak");
    assert_eq!(heap.write(c, 0, Value::Nil), Ok(()));
    assert_eq!(heap.upgrade(w), Ok(None));
    assert_eq!(
        heap.mark_shared(c),
        Err(crate::heap::AuditError::SharedCell { id: c })
    );
    assert_eq!(heap.release(c), Ok(0));
    let expected = scripted_events(v, c);
    assert_eq!(heap.trace(), &expected[..]);
    let report = heap.finish();
    assert_eq!(report.trace, expected);
    assert!(report.is_clean());
}

#[test]
fn every_event_names_its_object() {
    let mut heap = Heap::new();
    let v = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Ref(v));
    heap.mark_shared(v).expect("share");
    let ids: Vec<_> = heap.trace().iter().map(|e| e.id()).collect();
    assert_eq!(ids, vec![Some(v), Some(c), Some(v), Some(v)]);
}

#[test]
fn failed_operations_leave_no_trace() {
    let mut heap = Heap::new();
    let v = imm(&mut heap, vec![]);
    let before = heap.trace().len();
    assert!(heap.write(v, 0, Value::Nil).is_err());
    assert!(heap.read(v, 5).is_err());
    assert!(heap.alloc(Kind::Cell, vec![]).is_err());
    assert_eq!(heap.trace().len(), before);
}

#[test]
fn allocs_are_the_a_lines_of_the_trace() {
    let mut heap = Heap::new();
    assert_eq!(trace_allocs(heap.trace()), 0);
    heap.alloc_immortal(Kind::Immutable, vec![])
        .expect("a literal is not an A line");
    let a = imm(&mut heap, vec![]);
    cell(&mut heap, Value::Int(0));
    atom(&mut heap, Value::Int(0));
    let s = heap.open_scope();
    heap.alloc_in_scope(s, Kind::Immutable, vec![Value::Ref(a)])
        .expect("a stack object is an S line");
    assert_eq!(trace_allocs(heap.trace()), 3);
}

#[test]
fn what_the_defs_allocated_is_not_traced() {
    let mut heap = Heap::new();
    let leaf = imm(&mut heap, vec![]);
    let root = imm(&mut heap, vec![Value::Ref(leaf)]);
    heap.release(leaf).expect("root holds leaf");
    heap.immortalise(root).expect("a def's value");
    assert_eq!(trace_allocs(heap.trace()), 0);
    let kept = imm(&mut heap, vec![]);
    imm(&mut heap, vec![Value::Ref(kept)]);
    assert_eq!(trace_allocs(heap.trace()), 2);
    assert_eq!(
        traced(heap.trace()).first(),
        Some(&Event::Alloc {
            id: kept,
            kind: Kind::Immutable
        })
    );
}
