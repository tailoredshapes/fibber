//! Weak references (§6): uncounted, upgradable while the target lives,
//! `None` afterwards, and never confused with a newer object.

use super::{cell, imm};
use crate::heap::{AuditError, Event, Heap, Op, Value};

#[test]
fn weak_does_not_count() {
    let mut heap = Heap::new();
    let v = imm(&mut heap, vec![]);
    assert_eq!(heap.weak(v), Ok(Value::Weak(v)));
    assert_eq!(heap.count(v), Ok(1));
    assert_eq!(heap.trace().last(), Some(&Event::Weak { id: v }));
}

#[test]
fn upgrade_before_free_retains() {
    let mut heap = Heap::new();
    let v = imm(&mut heap, vec![]);
    let w = heap.weak(v).expect("weak").as_weak().expect("is weak");
    let start = heap.trace().len();
    assert_eq!(heap.upgrade(w), Ok(Some(v)));
    assert_eq!(heap.count(v), Ok(2));
    assert_eq!(
        &heap.trace()[start..],
        &[
            Event::Upgrade { id: v, live: true },
            Event::Retain {
                id: v,
                count_after: 2
            },
        ]
    );
}

#[test]
fn upgrade_after_free_is_none_and_traced() {
    let mut heap = Heap::new();
    let v = imm(&mut heap, vec![]);
    let w = heap.weak(v).expect("weak").as_weak().expect("is weak");
    assert_eq!(heap.release(v), Ok(0));
    let start = heap.trace().len();
    assert_eq!(heap.upgrade(w), Ok(None));
    assert_eq!(
        &heap.trace()[start..],
        &[Event::Upgrade { id: v, live: false }]
    );
    assert!(heap.finish().is_clean());
}

#[test]
fn ids_are_never_reused_so_a_stale_weak_stays_dead() {
    let mut heap = Heap::new();
    let v = imm(&mut heap, vec![]);
    let w = heap.weak(v).expect("weak").as_weak().expect("is weak");
    assert_eq!(heap.release(v), Ok(0));
    let newer = imm(&mut heap, vec![]);
    assert_ne!(newer, v);
    assert_eq!(heap.upgrade(w), Ok(None));
    assert_eq!(heap.count(newer), Ok(1));
}

#[test]
fn weak_of_freed_is_use_after_free() {
    let mut heap = Heap::new();
    let v = imm(&mut heap, vec![]);
    assert_eq!(heap.release(v), Ok(0));
    assert_eq!(
        heap.weak(v),
        Err(AuditError::UseAfterFree {
            id: v,
            op: Op::Weak
        })
    );
}

#[test]
fn a_weak_field_keeps_nothing_alive() {
    // Case 19's shape: a child points at its parent weakly.
    let mut heap = Heap::new();
    let parent = cell(&mut heap, Value::Nil);
    let w = heap.weak(parent).expect("weak");
    let child = imm(&mut heap, vec![w]);
    assert_eq!(heap.write(parent, 0, Value::Ref(child)), Ok(()));
    assert_eq!(heap.release(child), Ok(1));
    assert_eq!(heap.count(parent), Ok(1));
    assert_eq!(heap.release(parent), Ok(0));
    assert!(!heap.is_live(child));
    assert!(heap.finish().is_clean());
}

#[test]
fn a_weak_field_may_outlive_its_target() {
    // Case 20's shape: the holder survives the target and deref gives nil.
    let mut heap = Heap::new();
    let target = imm(&mut heap, vec![Value::Int(1)]);
    let w = heap.weak(target).expect("weak");
    let holder = imm(&mut heap, vec![w]);
    assert_eq!(heap.release(target), Ok(0));
    let stored = heap.read(holder, 0).expect("read").as_weak().expect("weak");
    assert_eq!(heap.upgrade(stored), Ok(None));
    assert_eq!(heap.release(holder), Ok(0));
    assert!(heap.finish().is_clean());
}
