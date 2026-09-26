//! Weak references (§6): uncounted, upgrade retains, and a stale id
//! never names a newer object.

use fibref::{AuditError, Event, Heap, Op, Value};

use crate::{cell, imm, releases_of, retains_of};

#[test]
fn upgrade_before_free_retains_and_that_count_must_be_released() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Int(1)]);
    assert_eq!(heap.weak(a), Ok(Value::Weak(a)));
    assert_eq!(heap.count(a), Ok(1), "weak does not count");
    let before = heap.trace().len();
    assert_eq!(heap.upgrade(a), Ok(Some(a)));
    assert_eq!(heap.count(a), Ok(2), "upgrade retains");
    assert_eq!(
        &heap.trace()[before..],
        &[
            Event::Upgrade { id: a, live: true },
            Event::Retain {
                id: a,
                count_after: 2
            }
        ]
    );
    assert_eq!(heap.release(a), Ok(1));
    assert!(heap.is_live(a), "the upgraded reference keeps it alive");
    assert_eq!(heap.release(a), Ok(0));
    assert!(!heap.is_live(a));
    assert!(heap.finish().is_clean());
}

#[test]
fn upgrade_after_free_is_none_traced_and_retains_nothing() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    heap.weak(a).expect("weak");
    heap.release(a).expect("free");
    let before = heap.trace().len();
    assert_eq!(heap.upgrade(a), Ok(None));
    assert_eq!(heap.upgrade(a), Ok(None), "and again");
    assert_eq!(
        &heap.trace()[before..],
        &[
            Event::Upgrade { id: a, live: false },
            Event::Upgrade { id: a, live: false }
        ]
    );
    assert_eq!(retains_of(heap.trace(), a), 0);
    assert!(!heap.is_live(a));
}

#[test]
fn ids_are_never_reused_so_a_stale_weak_stays_dead() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    let w = heap.weak(a).expect("weak");
    heap.release(a).expect("free");
    let mut fresh = Vec::new();
    for i in 0..5000 {
        fresh.push(imm(&mut heap, vec![Value::Int(i)]));
    }
    assert!(fresh.iter().all(|&id| id != a));
    assert!(fresh.iter().all(|&id| heap.is_live(id)));
    assert_eq!(w, Value::Weak(a));
    assert_eq!(heap.upgrade(a), Ok(None));
    assert!(!heap.is_live(a));
    assert!(heap.read(a, 0).is_err());
    for id in fresh {
        heap.release(id).expect("release");
    }
    assert!(heap.finish().is_clean());
}

#[test]
fn weak_of_freed_is_use_after_free() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    heap.release(a).expect("free");
    assert_eq!(
        heap.weak(a),
        Err(AuditError::UseAfterFree {
            id: a,
            op: Op::Weak
        })
    );
}

#[test]
fn a_weak_field_does_not_keep_its_target_alive() {
    let mut heap = Heap::new();
    let t = imm(&mut heap, vec![Value::Int(1)]);
    let w = heap.weak(t).expect("weak");
    let holder = imm(&mut heap, vec![w]);
    assert_eq!(heap.count(t), Ok(1), "storing a weak is not +1");
    heap.release(t).expect("frees t");
    assert!(!heap.is_live(t));
    assert!(heap.is_live(holder));
    assert_eq!(heap.read(holder, 0), Ok(Value::Weak(t)));
    assert_eq!(heap.upgrade(t), Ok(None));
    heap.release(holder).expect("frees holder");
    assert_eq!(releases_of(heap.trace(), t), 1, "only the caller's");
    assert!(heap.finish().is_clean());
}

#[test]
fn overwriting_a_weak_in_a_cell_releases_nothing() {
    let mut heap = Heap::new();
    let t = imm(&mut heap, vec![]);
    let w = heap.weak(t).expect("weak");
    let c = cell(&mut heap, w);
    let before = heap.trace().len();
    heap.write(c, 0, Value::Nil).expect("set!");
    assert_eq!(heap.count(t), Ok(1));
    assert_eq!(&heap.trace()[before..], &[Event::Write { id: c, field: 0 }]);
    // And the other way: a weak written over a ref releases the ref.
    let x = imm(&mut heap, vec![]);
    heap.write(c, 0, Value::Ref(x)).expect("set! x");
    heap.release(x).expect("caller's count");
    heap.write(c, 0, w).expect("set! weak");
    assert!(!heap.is_live(x));
    assert_eq!(heap.count(t), Ok(1));
}

#[test]
fn upgrade_of_a_weak_read_from_a_cell_after_the_target_died_is_none() {
    let mut heap = Heap::new();
    let t = imm(&mut heap, vec![]);
    let w = heap.weak(t).expect("weak");
    let c = cell(&mut heap, w);
    heap.release(t).expect("frees t");
    let Value::Weak(id) = heap.read(c, 0).expect("read") else {
        panic!("the cell holds a weak");
    };
    assert_eq!(id, t);
    assert_eq!(heap.upgrade(id), Ok(None));
    assert!(
        heap.retain(id).is_err(),
        "cannot retain through a dead weak"
    );
}

#[test]
fn a_weak_stored_at_alloc_and_upgraded_keeps_counts_straight() {
    let mut heap = Heap::new();
    let t = imm(&mut heap, vec![]);
    let w = heap.weak(t).expect("weak");
    let holder = imm(&mut heap, vec![w, w]);
    let Value::Weak(id) = heap.read(holder, 1).expect("read") else {
        panic!("weak")
    };
    assert_eq!(heap.upgrade(id), Ok(Some(t)));
    assert_eq!(heap.count(t), Ok(2));
    heap.release(t).expect("caller's original");
    assert!(heap.is_live(t));
    heap.release(t).expect("the upgraded one");
    assert!(!heap.is_live(t));
    heap.release(holder).expect("holder");
    assert!(heap.finish().is_clean());
}
