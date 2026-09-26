//! A double free the heap only sees later: an object released once
//! more than its holders own is freed while a live object still holds a
//! `Ref` to it. Every later touch of that dangling `Ref` is an error
//! that changes nothing (`spec/method.md` rule 2).

use super::{cell, imm};
use crate::heap::{AuditError, DanglingRef, Heap, LeakClass, Op, Value};

/// `(parent, x)`: `x` freed by an over-release while `parent`'s field 0
/// still holds `Ref(x)`.
fn parent_with_dangling_child(heap: &mut Heap) -> (crate::heap::ObjId, crate::heap::ObjId) {
    let x = imm(heap, vec![Value::Int(1)]);
    let parent = imm(heap, vec![Value::Ref(x)]);
    assert_eq!(heap.release(x), Ok(1));
    assert_eq!(heap.release(x), Ok(0), "one too many");
    assert!(!heap.is_live(x));
    (parent, x)
}

#[test]
fn a_cascade_into_a_freed_object_is_release_of_freed_and_changes_nothing() {
    let mut heap = Heap::new();
    let (parent, x) = parent_with_dangling_child(&mut heap);
    let events = heap.trace().len();
    assert_eq!(
        heap.release(parent),
        Err(AuditError::ReleaseOfFreed { id: x })
    );
    assert!(heap.is_live(parent));
    assert_eq!(heap.count(parent), Ok(1));
    assert_eq!(heap.trace().len(), events);
}

#[test]
fn a_cascade_that_reaches_what_it_just_freed_is_refused_whole() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    heap.write(c, 0, Value::Ref(c)).expect("self reference");
    assert_eq!(heap.release(c), Ok(1), "the binding's count");
    let events = heap.trace().len();
    // Releasing the slot's count would free c and then release c's own
    // slot: the plan sees that before anything changes.
    assert_eq!(heap.release(c), Err(AuditError::ReleaseOfFreed { id: c }));
    assert!(heap.is_live(c));
    assert_eq!(heap.count(c), Ok(1));
    assert_eq!(heap.trace().len(), events);
}

#[test]
fn a_write_whose_old_value_dangles_retains_and_stores_nothing() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Ref(x));
    assert_eq!(heap.release(x), Ok(1));
    assert_eq!(heap.release(x), Ok(0), "one too many");
    let y = imm(&mut heap, vec![]);
    let events = heap.trace().len();
    assert_eq!(
        heap.write(c, 0, Value::Ref(y)),
        Err(AuditError::ReleaseOfFreed { id: x })
    );
    assert_eq!(heap.count(y), Ok(1), "the new value was retained");
    assert_eq!(heap.read(c, 0), Ok(Value::Ref(x)), "the slot was replaced");
    assert_eq!(heap.trace().len(), events + 1, "only the read");
}

#[test]
fn a_write_plans_the_release_with_the_slot_already_replaced() {
    // c -> v -> c, held only by each other. Writing nil into c frees v,
    // whose release of c frees c; c's slot is nil by then, so v is not
    // reached a second time.
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    assert_eq!(heap.release(v), Ok(1));
    assert_eq!(heap.release(c), Ok(1));
    assert_eq!(heap.write(c, 0, Value::Nil), Ok(()));
    assert!(!heap.is_live(v));
    assert!(!heap.is_live(c));
    assert!(heap.finish().is_clean());
}

#[test]
fn mark_shared_through_a_dangling_ref_is_use_after_free_and_marks_nothing() {
    let mut heap = Heap::new();
    let (parent, x) = parent_with_dangling_child(&mut heap);
    let holder = imm(&mut heap, vec![Value::Ref(parent)]);
    let events = heap.trace().len();
    assert_eq!(
        heap.mark_shared(holder),
        Err(AuditError::UseAfterFree {
            id: x,
            op: Op::Share
        })
    );
    assert_eq!(heap.is_shared(holder), Ok(false));
    assert_eq!(heap.is_shared(parent), Ok(false));
    assert_eq!(heap.trace().len(), events);
}

#[test]
fn a_dangling_ref_to_a_freed_cell_is_use_after_free_before_shared_cell() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let holder = imm(&mut heap, vec![Value::Ref(c)]);
    assert_eq!(heap.release(c), Ok(1));
    assert_eq!(heap.release(c), Ok(0), "one too many");
    assert_eq!(
        heap.mark_shared(holder),
        Err(AuditError::UseAfterFree {
            id: c,
            op: Op::Share
        })
    );
}

#[test]
fn finish_lists_a_dangling_ref_and_is_neither_clean_nor_cycle_leak_only() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c), Value::Ref(x)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    for held in [v, c, x] {
        heap.release(held).expect("scope ends");
    }
    assert_eq!(heap.release(x), Ok(0), "one too many: freed under v");
    let report = heap.finish();
    assert_eq!(
        report.dangling,
        vec![DanglingRef {
            holder: v,
            field: 1,
            target: x
        }]
    );
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert!(!report.is_clean());
    assert!(!report.is_cycle_leak_only());
    assert_eq!(report.freed, 1);
}
