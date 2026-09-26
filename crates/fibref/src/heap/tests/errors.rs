//! Every `AuditError` variant fires, and a failing call changes nothing.

use super::{atom, cell, imm};
use crate::heap::{AuditError, Heap, Kind, ObjId, Op, Value};

#[test]
fn unknown_id_is_an_error_everywhere() {
    let mut heap = Heap::new();
    let bogus = ObjId::from_index(7);
    let err = AuditError::UnknownId { id: bogus };
    assert_eq!(heap.retain(bogus), Err(err));
    assert_eq!(heap.release(bogus), Err(err));
    assert_eq!(heap.read(bogus, 0), Err(err));
    assert_eq!(heap.write(bogus, 0, Value::Nil), Err(err));
    assert_eq!(heap.weak(bogus), Err(err));
    assert_eq!(heap.upgrade(bogus), Err(err));
    assert_eq!(heap.mark_shared(bogus), Err(err));
    assert_eq!(heap.count(bogus), Err(err));
    assert_eq!(heap.kind(bogus), Err(err));
    assert_eq!(
        heap.alloc(Kind::Immutable, vec![Value::Ref(bogus)]),
        Err(err)
    );
    assert_eq!(
        heap.alloc(Kind::Immutable, vec![Value::Weak(bogus)]),
        Err(err)
    );
    assert!(!heap.is_live(bogus));
    assert!(heap.trace().is_empty());
}

#[test]
fn use_after_free_fires_for_each_operation() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Int(1));
    assert_eq!(heap.release(c), Ok(0));
    let uaf = |op| AuditError::UseAfterFree { id: c, op };
    assert_eq!(heap.retain(c), Err(uaf(Op::Retain)));
    assert_eq!(heap.read(c, 0), Err(uaf(Op::Read)));
    assert_eq!(heap.write(c, 0, Value::Nil), Err(uaf(Op::Write)));
    assert_eq!(heap.weak(c), Err(uaf(Op::Weak)));
    assert_eq!(heap.mark_shared(c), Err(uaf(Op::Share)));
    assert_eq!(heap.count(c), Err(uaf(Op::Inspect)));
    assert_eq!(heap.kind(c), Err(uaf(Op::Inspect)));
    assert_eq!(heap.is_shared(c), Err(uaf(Op::Inspect)));
    assert_eq!(heap.field_count(c), Err(uaf(Op::Inspect)));
    assert_eq!(
        heap.alloc(Kind::Immutable, vec![Value::Ref(c)]),
        Err(uaf(Op::Store))
    );
    assert!(!heap.is_live(c));
}

#[test]
fn storing_a_freed_ref_in_a_cell_is_use_after_free() {
    let mut heap = Heap::new();
    let dead = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Nil);
    assert_eq!(heap.release(dead), Ok(0));
    assert_eq!(
        heap.write(c, 0, Value::Ref(dead)),
        Err(AuditError::UseAfterFree {
            id: dead,
            op: Op::Store
        })
    );
    assert_eq!(heap.read(c, 0), Ok(Value::Nil));
}

#[test]
fn release_of_freed_is_double_free() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Int(1)]);
    assert_eq!(heap.release(a), Ok(0));
    assert_eq!(heap.release(a), Err(AuditError::ReleaseOfFreed { id: a }));
    // Only one Release and one Free were recorded.
    let releases = heap
        .trace()
        .iter()
        .filter(|e| matches!(e, crate::heap::Event::Release { .. }))
        .count();
    assert_eq!(releases, 1);
}

#[test]
fn write_to_immutable_is_an_error() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Int(1)]);
    assert_eq!(
        heap.write(a, 0, Value::Int(2)),
        Err(AuditError::WriteToImmutable { id: a })
    );
    assert_eq!(heap.read(a, 0), Ok(Value::Int(1)));
}

#[test]
fn bad_field_on_read_and_write() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Int(1), Value::Int(2)]);
    let c = cell(&mut heap, Value::Int(3));
    assert_eq!(
        heap.read(a, 2),
        Err(AuditError::BadField { id: a, index: 2 })
    );
    assert_eq!(
        heap.write(c, 1, Value::Nil),
        Err(AuditError::BadField { id: c, index: 1 })
    );
    assert_eq!(heap.read(c, 0), Ok(Value::Int(3)));
}

#[test]
fn shared_cell_is_an_error() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Int(1));
    assert_eq!(heap.mark_shared(c), Err(AuditError::SharedCell { id: c }));
    assert_eq!(heap.is_shared(c), Ok(false));
}

#[test]
fn cell_and_atom_take_exactly_one_slot() {
    let mut heap = Heap::new();
    for kind in [Kind::Cell, Kind::Atom] {
        assert_eq!(
            heap.alloc(kind, vec![]),
            Err(AuditError::WrongSlotCount { kind, given: 0 })
        );
        assert_eq!(
            heap.alloc(kind, vec![Value::Nil, Value::Nil]),
            Err(AuditError::WrongSlotCount { kind, given: 2 })
        );
    }
    assert!(heap.trace().is_empty());
    let a = atom(&mut heap, Value::Nil);
    assert_eq!(heap.field_count(a), Ok(1));
}

#[test]
fn failed_alloc_retains_nothing_and_allocates_nothing() {
    let mut heap = Heap::new();
    let live = imm(&mut heap, vec![]);
    let dead = imm(&mut heap, vec![]);
    assert_eq!(heap.release(dead), Ok(0));
    let before = heap.trace().len();
    let result = heap.alloc(Kind::Immutable, vec![Value::Ref(live), Value::Ref(dead)]);
    assert_eq!(
        result,
        Err(AuditError::UseAfterFree {
            id: dead,
            op: Op::Store
        })
    );
    assert_eq!(heap.count(live), Ok(1));
    assert_eq!(heap.trace().len(), before);
    // The next allocation gets the id the failed one would have had.
    let next = imm(&mut heap, vec![]);
    assert_eq!(next.index(), 2);
}

#[test]
fn errors_display_their_object() {
    let id = ObjId::from_index(3);
    let text = AuditError::UseAfterFree { id, op: Op::Read }.to_string();
    assert!(text.contains("#3"), "{text}");
    assert!(text.contains("read"), "{text}");
    let text = AuditError::WrongSlotCount {
        kind: Kind::Cell,
        given: 2,
    }
    .to_string();
    assert!(text.contains("cell"), "{text}");
}
