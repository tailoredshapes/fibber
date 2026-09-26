//! Thread crossing (§7): `mark_shared` propagates to everything
//! reachable, rejects cells, and admits atoms.

use super::{atom, cell, imm};
use crate::heap::{AuditError, Event, Heap, Value};

#[test]
fn mark_shared_propagates_through_refs() {
    let mut heap = Heap::new();
    let leaf = imm(&mut heap, vec![Value::Int(1)]);
    let mid = imm(&mut heap, vec![Value::Ref(leaf)]);
    let root = imm(&mut heap, vec![Value::Ref(mid)]);
    let aside = imm(&mut heap, vec![]);
    assert_eq!(heap.mark_shared(root), Ok(()));
    for id in [root, mid, leaf] {
        assert_eq!(heap.is_shared(id), Ok(true), "{id}");
    }
    assert_eq!(heap.is_shared(aside), Ok(false));
    let shared: Vec<Event> = heap
        .trace()
        .iter()
        .copied()
        .filter(|e| matches!(e, Event::Shared { .. }))
        .collect();
    assert_eq!(
        shared,
        vec![
            Event::Shared { id: root },
            Event::Shared { id: mid },
            Event::Shared { id: leaf },
        ]
    );
}

#[test]
fn already_shared_objects_are_not_marked_twice() {
    let mut heap = Heap::new();
    let leaf = imm(&mut heap, vec![]);
    let root = imm(&mut heap, vec![Value::Ref(leaf)]);
    assert_eq!(heap.mark_shared(root), Ok(()));
    let before = heap.trace().len();
    assert_eq!(heap.mark_shared(root), Ok(()));
    assert_eq!(heap.trace().len(), before);
}

#[test]
fn a_reachable_cell_rejects_the_crossing_and_marks_nothing() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Int(1));
    let mid = imm(&mut heap, vec![Value::Ref(c)]);
    let root = imm(&mut heap, vec![Value::Ref(mid)]);
    let before = heap.trace().len();
    assert_eq!(
        heap.mark_shared(root),
        Err(AuditError::SharedCell { id: c })
    );
    for id in [root, mid, c] {
        assert_eq!(heap.is_shared(id), Ok(false), "{id}");
    }
    assert_eq!(heap.trace().len(), before);
}

#[test]
fn a_reachable_atom_may_cross() {
    let mut heap = Heap::new();
    let inner = imm(&mut heap, vec![]);
    let a = atom(&mut heap, Value::Ref(inner));
    let root = imm(&mut heap, vec![Value::Ref(a)]);
    assert_eq!(heap.mark_shared(root), Ok(()));
    assert_eq!(heap.is_shared(a), Ok(true));
    assert_eq!(heap.is_shared(inner), Ok(true));
}

#[test]
fn a_cycle_through_an_atom_terminates() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    let node = imm(&mut heap, vec![Value::Ref(a)]);
    heap.write(a, 0, Value::Ref(node)).expect("reset!");
    assert_eq!(heap.mark_shared(a), Ok(()));
    assert_eq!(heap.is_shared(node), Ok(true));
}

#[test]
fn writing_into_a_shared_atom_shares_the_value() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    assert_eq!(heap.mark_shared(a), Ok(()));
    let v = imm(&mut heap, vec![]);
    assert_eq!(heap.is_shared(v), Ok(false));
    assert_eq!(heap.write(a, 0, Value::Ref(v)), Ok(()));
    assert_eq!(heap.is_shared(v), Ok(true));
}

#[test]
fn writing_a_cell_into_a_shared_atom_is_rejected_unchanged() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Int(0));
    assert_eq!(heap.mark_shared(a), Ok(()));
    let c = cell(&mut heap, Value::Nil);
    let holder = imm(&mut heap, vec![Value::Ref(c)]);
    assert_eq!(
        heap.write(a, 0, Value::Ref(holder)),
        Err(AuditError::SharedCell { id: c })
    );
    assert_eq!(heap.read(a, 0), Ok(Value::Int(0)));
    assert_eq!(heap.count(holder), Ok(1));
    assert_eq!(heap.is_shared(holder), Ok(false));
}

#[test]
fn writing_into_an_unshared_atom_shares_nothing() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    let c = cell(&mut heap, Value::Nil);
    assert_eq!(heap.write(a, 0, Value::Ref(c)), Ok(()));
    assert_eq!(heap.is_shared(c), Ok(false));
}

#[test]
fn a_live_weak_target_counts_as_reachable() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let w = heap.weak(c).expect("weak");
    let root = imm(&mut heap, vec![w]);
    assert_eq!(
        heap.mark_shared(root),
        Err(AuditError::SharedCell { id: c })
    );
    heap.release(c).expect("free the cell");
    assert_eq!(heap.mark_shared(root), Ok(()));
}

#[test]
fn writing_a_weak_to_a_live_cell_into_a_shared_atom_is_rejected() {
    // The write route must answer the weak-to-cell question the same
    // way `mark_shared` does: the other thread could upgrade the weak.
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    heap.mark_shared(a).expect("cross an empty atom");
    let c = cell(&mut heap, Value::Int(1));
    let w = heap.weak(c).expect("weak");
    let events = heap.trace().len();
    assert_eq!(heap.write(a, 0, w), Err(AuditError::SharedCell { id: c }));
    assert_eq!(heap.read(a, 0), Ok(Value::Nil), "slot untouched");
    assert_eq!(heap.is_shared(c), Ok(false));
    assert_eq!(heap.trace().len(), events + 1, "only the read");
    heap.release(c).expect("free the cell");
    assert_eq!(heap.write(a, 0, w), Ok(()), "a dead target cannot cross");
}
