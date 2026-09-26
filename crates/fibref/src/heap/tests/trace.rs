//! The trace records every operation, in order, with the documented
//! events (`spec/method.md`, rule 6).

use super::{cell, imm};
use crate::heap::{Event, Heap, Kind, ObjId, Value};

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
    assert_eq!(ids, vec![v, c, v, v]);
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
