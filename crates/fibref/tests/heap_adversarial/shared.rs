//! Thread crossing (§7): marking propagates to everything reachable,
//! a cell may never cross, and a rejected crossing marks nothing.

use fibref::{AuditError, Heap, Op, Value};

use crate::{atom, cell, imm, shared_ids};

#[test]
fn mark_shared_through_a_vector_reaching_an_atom_marks_everything() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![Value::Int(1)]);
    let a = atom(&mut heap, Value::Ref(x));
    heap.release(x).expect("caller's count");
    let v = imm(&mut heap, vec![Value::Ref(a), Value::Int(2)]);
    let counts = (heap.count(v), heap.count(a), heap.count(x));
    assert_eq!(heap.mark_shared(v), Ok(()));
    for id in [v, a, x] {
        assert_eq!(heap.is_shared(id), Ok(true), "{id}");
    }
    let mut marked = shared_ids(heap.trace());
    marked.sort();
    assert_eq!(marked, vec![x, a, v]);
    assert_eq!(
        (heap.count(v), heap.count(a), heap.count(x)),
        counts,
        "crossing changes no count"
    );
}

#[test]
fn mark_shared_through_a_vector_reaching_a_cell_marks_nothing() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Int(1));
    let c = cell(&mut heap, Value::Int(2));
    let v = imm(&mut heap, vec![Value::Ref(a), Value::Ref(c)]);
    let events = heap.trace().len();
    assert_eq!(heap.mark_shared(v), Err(AuditError::SharedCell { id: c }));
    for id in [v, a, c] {
        assert_eq!(heap.is_shared(id), Ok(false), "{id} was marked anyway");
    }
    assert_eq!(heap.trace().len(), events, "nothing traced");
    assert!(shared_ids(heap.trace()).is_empty());
}

#[test]
fn an_atom_holding_a_cell_may_not_cross() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let a = atom(&mut heap, Value::Ref(c));
    assert_eq!(heap.mark_shared(a), Err(AuditError::SharedCell { id: c }));
    assert_eq!(heap.is_shared(a), Ok(false));
}

#[test]
fn mark_shared_twice_marks_once() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let v = imm(&mut heap, vec![Value::Ref(x)]);
    heap.mark_shared(v).expect("first");
    let events = heap.trace().len();
    heap.mark_shared(v).expect("second");
    heap.mark_shared(x).expect("third, from inside");
    assert_eq!(heap.trace().len(), events);
    assert_eq!(shared_ids(heap.trace()).len(), 2);
}

#[test]
fn mark_shared_over_a_cycle_through_an_atom_terminates() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(a)]);
    heap.write(a, 0, Value::Ref(v)).expect("reset!");
    assert_eq!(heap.mark_shared(a), Ok(()));
    assert_eq!(heap.is_shared(v), Ok(true));
    assert_eq!(shared_ids(heap.trace()).len(), 2);
}

#[test]
fn writing_a_ref_into_a_shared_atom_shares_it_and_its_children() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    heap.mark_shared(a).expect("cross");
    let y = imm(&mut heap, vec![]);
    let x = imm(&mut heap, vec![Value::Ref(y)]);
    assert_eq!(heap.is_shared(x), Ok(false));
    heap.write(a, 0, Value::Ref(x)).expect("reset!");
    assert_eq!(heap.is_shared(x), Ok(true));
    assert_eq!(heap.is_shared(y), Ok(true));
    let mut marked = shared_ids(heap.trace());
    marked.sort();
    assert_eq!(marked, vec![a, y, x]);
    assert_eq!(heap.count(x), Ok(2));
}

#[test]
fn writing_a_value_reaching_a_cell_into_a_shared_atom_changes_nothing() {
    let mut heap = Heap::new();
    let old = imm(&mut heap, vec![]);
    let a = atom(&mut heap, Value::Ref(old));
    heap.mark_shared(a).expect("cross");
    let c = cell(&mut heap, Value::Nil);
    let x = imm(&mut heap, vec![Value::Int(1), Value::Ref(c)]);
    let events = heap.trace().len();
    assert_eq!(
        heap.write(a, 0, Value::Ref(x)),
        Err(AuditError::SharedCell { id: c })
    );
    assert_eq!(heap.read(a, 0), Ok(Value::Ref(old)));
    assert_eq!(heap.count(old), Ok(2));
    assert_eq!(heap.count(x), Ok(1));
    assert_eq!(heap.count(c), Ok(2));
    assert_eq!(heap.is_shared(x), Ok(false));
    assert_eq!(heap.is_shared(c), Ok(false));
    assert_eq!(heap.trace().len(), events + 1, "only the read");
}

#[test]
fn writing_into_an_unshared_atom_shares_nothing() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    let c = cell(&mut heap, Value::Nil);
    let x = imm(&mut heap, vec![Value::Ref(c)]);
    heap.write(a, 0, Value::Ref(x))
        .expect("a cell inside an unshared atom is fine");
    assert_eq!(heap.is_shared(x), Ok(false));
    assert!(shared_ids(heap.trace()).is_empty());
    // But now the atom may not cross.
    assert_eq!(heap.mark_shared(a), Err(AuditError::SharedCell { id: c }));
}

#[test]
fn the_shared_flag_survives_count_operations() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    heap.mark_shared(x).expect("cross");
    heap.retain(x).expect("retain");
    heap.release(x).expect("release");
    assert_eq!(heap.is_shared(x), Ok(true));
    assert_eq!(heap.upgrade(x), Ok(Some(x)));
    assert_eq!(heap.is_shared(x), Ok(true));
    heap.release(x).expect("release upgrade");
    heap.release(x).expect("free");
    assert_eq!(
        heap.is_shared(x),
        Err(AuditError::UseAfterFree {
            id: x,
            op: Op::Inspect
        })
    );
}

#[test]
fn a_new_object_pointing_at_a_shared_one_is_not_itself_shared() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    heap.mark_shared(x).expect("cross");
    let local = imm(&mut heap, vec![Value::Ref(x)]);
    assert_eq!(heap.is_shared(local), Ok(false));
    assert_eq!(heap.is_shared(x), Ok(true));
}

#[test]
fn a_weak_to_a_cell_crosses_the_same_way_by_mark_shared_and_by_atom_write() {
    // §7: a cell that is not an atom may not cross. Whether a weak
    // reference lets a cell cross (the other thread can upgrade it) is
    // one question with one answer; the two routes across a thread
    // boundary must agree on it.
    let mut heap = Heap::new();
    let c1 = cell(&mut heap, Value::Int(1));
    let w1 = heap.weak(c1).expect("weak");
    let holder = imm(&mut heap, vec![w1]);
    let by_mark = heap.mark_shared(holder);

    let c2 = cell(&mut heap, Value::Int(2));
    let w2 = heap.weak(c2).expect("weak");
    let a = atom(&mut heap, Value::Nil);
    heap.mark_shared(a).expect("cross an empty atom");
    let by_write = heap.write(a, 0, w2);

    assert_eq!(
        by_mark.is_err(),
        by_write.is_err(),
        "mark_shared says {by_mark:?}, writing into a shared atom says {by_write:?}"
    );
}

#[test]
fn a_weak_to_a_dead_object_never_blocks_a_crossing() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let w = heap.weak(c).expect("weak");
    let holder = imm(&mut heap, vec![w]);
    heap.release(c).expect("frees the cell");
    assert_eq!(heap.mark_shared(holder), Ok(()));
    assert_eq!(heap.is_shared(holder), Ok(true));
}
