//! Every `AuditError` variant fires, and a failing call changes
//! nothing: no count, no field, no event.

use fibref::{AuditError, Heap, Kind, Op, Value};

use crate::{atom, cell, foreign_id, imm};

fn uaf(id: fibref::ObjId, op: Op) -> AuditError {
    AuditError::UseAfterFree { id, op }
}

#[test]
fn stale_ref_read_after_count_hit_zero_is_use_after_free() {
    let mut heap = Heap::new();
    let child = imm(&mut heap, vec![Value::Int(7)]);
    let holder = imm(&mut heap, vec![Value::Ref(child)]);
    // The program reads the reference out and keeps it uncounted.
    let stale = heap.read(holder, 0).expect("read");
    assert_eq!(stale, Value::Ref(child));
    heap.release(child).expect("caller's count");
    assert_eq!(heap.count(child), Ok(1));
    heap.release(holder).expect("frees holder, then child");
    assert!(!heap.is_live(child));

    assert_eq!(heap.read(child, 0), Err(uaf(child, Op::Read)));
    assert_eq!(heap.retain(child), Err(uaf(child, Op::Retain)));
    assert_eq!(heap.weak(child), Err(uaf(child, Op::Weak)));
    assert_eq!(heap.mark_shared(child), Err(uaf(child, Op::Share)));
    assert_eq!(heap.count(child), Err(uaf(child, Op::Inspect)));
    assert_eq!(heap.kind(child), Err(uaf(child, Op::Inspect)));
    assert_eq!(heap.is_shared(child), Err(uaf(child, Op::Inspect)));
    assert_eq!(heap.field_count(child), Err(uaf(child, Op::Inspect)));
    assert_eq!(
        heap.alloc(Kind::Immutable, vec![stale]),
        Err(uaf(child, Op::Store))
    );
    let c = cell(&mut heap, Value::Nil);
    assert_eq!(heap.write(c, 0, stale), Err(uaf(child, Op::Store)));
    assert_eq!(
        heap.read(c, 0),
        Ok(Value::Nil),
        "failed write stored nothing"
    );
}

#[test]
fn release_after_free_is_double_free_and_stays_one() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    assert_eq!(heap.release(a), Ok(0));
    let events = heap.trace().len();
    for _ in 0..3 {
        assert_eq!(heap.release(a), Err(AuditError::ReleaseOfFreed { id: a }));
    }
    assert_eq!(heap.trace().len(), events, "a double free is not traced");
    assert!(!heap.is_live(a));
}

#[test]
fn release_of_a_child_freed_through_its_parent_is_double_free() {
    let mut heap = Heap::new();
    let child = imm(&mut heap, vec![]);
    let parent = imm(&mut heap, vec![Value::Ref(child)]);
    heap.release(child).expect("caller's count");
    heap.release(parent).expect("frees both");
    assert_eq!(
        heap.release(child),
        Err(AuditError::ReleaseOfFreed { id: child })
    );
}

#[test]
fn write_to_immutable_is_rejected_without_retaining_the_value() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let v = imm(&mut heap, vec![Value::Int(1)]);
    let events = heap.trace().len();
    assert_eq!(
        heap.write(v, 0, Value::Ref(x)),
        Err(AuditError::WriteToImmutable { id: v })
    );
    assert_eq!(heap.count(x), Ok(1), "the rejected value was not retained");
    assert_eq!(heap.read(v, 0), Ok(Value::Int(1)), "the field is unchanged");
    assert_eq!(heap.trace().len(), events + 1, "only the read was traced");
    // An empty immutable: the write is refused as immutable, not as a
    // bad field (either would do, but it must be refused).
    let empty = imm(&mut heap, vec![]);
    assert!(heap.write(empty, 0, Value::Nil).is_err());
}

#[test]
fn write_to_freed_cell_is_use_after_free_without_retaining_the_value() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Int(1));
    heap.release(c).expect("frees the cell");
    let events = heap.trace().len();
    assert_eq!(heap.write(c, 0, Value::Ref(x)), Err(uaf(c, Op::Write)));
    assert_eq!(heap.count(x), Ok(1));
    assert_eq!(heap.trace().len(), events);
}

#[test]
fn bad_field_on_read_and_write_changes_nothing() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let pair = imm(&mut heap, vec![Value::Int(1), Value::Int(2)]);
    let empty = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Nil);
    let a = atom(&mut heap, Value::Nil);
    let events = heap.trace().len();
    assert_eq!(
        heap.read(pair, 2),
        Err(AuditError::BadField { id: pair, index: 2 })
    );
    assert_eq!(
        heap.read(empty, 0),
        Err(AuditError::BadField {
            id: empty,
            index: 0
        })
    );
    assert_eq!(
        heap.write(c, 1, Value::Ref(x)),
        Err(AuditError::BadField { id: c, index: 1 })
    );
    assert_eq!(
        heap.write(a, usize::MAX, Value::Ref(x)),
        Err(AuditError::BadField {
            id: a,
            index: usize::MAX
        })
    );
    assert_eq!(heap.count(x), Ok(1));
    assert_eq!(heap.trace().len(), events);
}

#[test]
fn freed_object_with_bad_index_is_still_an_error() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    heap.release(c).expect("free");
    assert!(matches!(
        heap.read(c, 99),
        Err(AuditError::UseAfterFree { .. }) | Err(AuditError::BadField { .. })
    ));
    assert!(matches!(
        heap.write(c, 99, Value::Nil),
        Err(AuditError::UseAfterFree { .. }) | Err(AuditError::BadField { .. })
    ));
}

#[test]
fn unknown_id_is_rejected_by_every_operation() {
    let mut heap = Heap::new();
    let stranger = foreign_id();
    let unknown = AuditError::UnknownId { id: stranger };
    assert!(!heap.is_live(stranger));
    assert_eq!(heap.read(stranger, 0), Err(unknown));
    assert_eq!(heap.write(stranger, 0, Value::Nil), Err(unknown));
    assert_eq!(heap.retain(stranger), Err(unknown));
    assert_eq!(heap.release(stranger), Err(unknown));
    assert_eq!(heap.weak(stranger), Err(unknown));
    assert_eq!(heap.upgrade(stranger), Err(unknown));
    assert_eq!(heap.mark_shared(stranger), Err(unknown));
    assert_eq!(heap.count(stranger), Err(unknown));
    assert_eq!(heap.kind(stranger), Err(unknown));
    assert_eq!(heap.is_shared(stranger), Err(unknown));
    assert_eq!(heap.field_count(stranger), Err(unknown));
    assert_eq!(
        heap.alloc(Kind::Immutable, vec![Value::Ref(stranger)]),
        Err(unknown)
    );
    assert_eq!(
        heap.alloc(Kind::Cell, vec![Value::Weak(stranger)]),
        Err(unknown)
    );
    assert!(heap.trace().is_empty(), "nothing was traced");
}

#[test]
fn alloc_with_a_freed_ref_allocates_and_retains_nothing() {
    let mut heap = Heap::new();
    let live = imm(&mut heap, vec![]);
    let dead = imm(&mut heap, vec![]);
    heap.release(dead).expect("free");
    let events = heap.trace().len();
    assert_eq!(
        heap.alloc(Kind::Immutable, vec![Value::Ref(live), Value::Ref(dead)]),
        Err(uaf(dead, Op::Store))
    );
    assert_eq!(heap.count(live), Ok(1), "the live field was not retained");
    assert_eq!(heap.trace().len(), events, "no Alloc, no Retain");
    let after = imm(&mut heap, vec![Value::Ref(live)]);
    assert_ne!(after, live);
    assert_ne!(after, dead);
    assert_eq!(heap.count(live), Ok(2));
}

#[test]
fn cell_and_atom_take_exactly_one_slot() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    assert_eq!(
        heap.alloc(Kind::Cell, vec![]),
        Err(AuditError::WrongSlotCount {
            kind: Kind::Cell,
            given: 0
        })
    );
    assert_eq!(
        heap.alloc(Kind::Atom, vec![Value::Ref(x), Value::Ref(x)]),
        Err(AuditError::WrongSlotCount {
            kind: Kind::Atom,
            given: 2
        })
    );
    assert_eq!(heap.count(x), Ok(1), "nothing was retained");
    assert!(heap.alloc(Kind::Immutable, vec![]).is_ok());
    assert!(heap
        .alloc(Kind::Immutable, vec![Value::Ref(x), Value::Ref(x)])
        .is_ok());
}

#[test]
fn shared_cell_fires_for_a_cell_crossing_directly() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Int(1));
    let events = heap.trace().len();
    assert_eq!(heap.mark_shared(c), Err(AuditError::SharedCell { id: c }));
    assert_eq!(heap.is_shared(c), Ok(false));
    assert_eq!(heap.trace().len(), events);
}

#[test]
fn errors_display_the_object_they_are_about() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    heap.release(a).expect("free");
    let err = heap.release(a).expect_err("double free");
    assert!(err.to_string().contains(&a.to_string()), "{err}");
}
