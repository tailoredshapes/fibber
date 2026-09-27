//! Immortal objects (`spec/types.md` §8.2): count 0, count operations
//! are untraced no-ops, never freed, never reported at exit, and their
//! graph is closed.

use super::{cell, imm};
use crate::heap::{AuditError, Event, Heap, Kind, Value};

#[test]
fn an_immortal_has_count_zero_and_one_untraced_alloc() {
    let mut heap = Heap::new();
    let s = heap
        .alloc_immortal(Kind::Immutable, vec![Value::Int(104)])
        .expect("literal");
    assert_eq!(heap.count(s), Ok(0));
    assert_eq!(heap.is_immortal(s), Ok(true));
    assert_eq!(
        heap.trace(),
        &[Event::AllocImmortal {
            id: s,
            kind: Kind::Immutable
        }]
    );
}

#[test]
fn retain_and_release_on_an_immortal_are_untraced_no_ops() {
    let mut heap = Heap::new();
    let s = heap
        .alloc_immortal(Kind::Immutable, vec![])
        .expect("literal");
    let before = heap.trace().len();
    assert_eq!(heap.retain(s), Ok(0));
    assert_eq!(heap.release(s), Ok(0));
    assert_eq!(heap.release(s), Ok(0));
    assert_eq!(heap.trace().len(), before);
    assert!(heap.is_live(s));
    assert_eq!(heap.count(s), Ok(0));
}

#[test]
fn storing_an_immortal_counts_nothing_and_freeing_the_holder_releases_nothing() {
    let mut heap = Heap::new();
    let s = heap
        .alloc_immortal(Kind::Immutable, vec![])
        .expect("literal");
    let v = imm(&mut heap, vec![Value::Ref(s)]);
    let c = cell(&mut heap, Value::Ref(s));
    heap.write(c, 0, Value::Ref(s))
        .expect("set! to the literal again");
    heap.release(v).expect("free v");
    heap.release(c).expect("free c");
    let report = heap.finish();
    assert!(report.is_clean(), "{:?}", report.leaks);
    assert_eq!((report.allocated, report.freed), (2, 2));
    let about_s = report.trace.iter().filter(|e| e.id() == Some(s)).count();
    assert_eq!(about_s, 1, "only the AllocImmortal names the literal");
}

#[test]
fn immortals_are_never_leaks() {
    let mut heap = Heap::new();
    let inner = heap.alloc_immortal(Kind::Immutable, vec![]).expect("tail");
    heap.alloc_immortal(Kind::Immutable, vec![Value::Ref(inner)])
        .expect("literal");
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!(report.allocated, 0);
}

#[test]
fn an_immortal_may_hold_only_immortals() {
    let mut heap = Heap::new();
    let mortal = imm(&mut heap, vec![]);
    let before = heap.trace().len();
    let err = AuditError::ImmortalHoldsMortal { id: mortal };
    assert_eq!(
        heap.alloc_immortal(Kind::Immutable, vec![Value::Ref(mortal)]),
        Err(err)
    );
    assert_eq!(
        heap.alloc_immortal(Kind::Immutable, vec![Value::Weak(mortal)]),
        Err(err)
    );
    assert_eq!(heap.trace().len(), before);
    assert_eq!(heap.count(mortal), Ok(1));
}

#[test]
fn an_immortal_is_immutable() {
    let mut heap = Heap::new();
    for kind in [Kind::Cell, Kind::Atom] {
        assert_eq!(
            heap.alloc_immortal(kind, vec![Value::Nil]),
            Err(AuditError::MutableImmortal { kind })
        );
    }
    let s = heap.alloc_immortal(Kind::Immutable, vec![Value::Int(1)]);
    let s = s.expect("literal");
    assert_eq!(
        heap.write(s, 0, Value::Int(2)),
        Err(AuditError::WriteToImmutable { id: s })
    );
}

#[test]
fn a_weak_to_an_immortal_always_upgrades_without_a_count() {
    let mut heap = Heap::new();
    let s = heap
        .alloc_immortal(Kind::Immutable, vec![])
        .expect("literal");
    let w = heap.weak(s).expect("weak of a literal");
    assert_eq!(heap.upgrade(s), Ok(Some(s)));
    assert_eq!(heap.count(s), Ok(0));
    let tail: Vec<Event> = heap.trace()[1..].to_vec();
    assert_eq!(
        tail,
        vec![Event::Weak { id: s }, Event::Upgrade { id: s, live: true }]
    );
    let holder = imm(&mut heap, vec![w]);
    heap.release(holder).expect("free");
    assert!(heap.finish().is_clean());
}

#[test]
fn sharing_stops_at_an_immortal() {
    let mut heap = Heap::new();
    let s = heap
        .alloc_immortal(Kind::Immutable, vec![])
        .expect("literal");
    let v = imm(&mut heap, vec![Value::Ref(s)]);
    heap.mark_shared(v).expect("share");
    assert_eq!(heap.is_shared(v), Ok(true));
    assert_eq!(heap.is_shared(s), Ok(false));
    heap.mark_shared(s)
        .expect("sharing a literal marks nothing");
    assert_eq!(heap.is_shared(s), Ok(false));
}
