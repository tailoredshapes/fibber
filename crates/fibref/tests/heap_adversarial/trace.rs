//! The trace (`spec/method.md` rule 6): every operation leaves exactly
//! its events, in order, and a failed operation leaves none.

use fibref::{Event, Heap, Kind, Value};

use crate::{cell, foreign_id, imm};

#[test]
fn alloc_store_release_is_traced_in_order() {
    let mut heap = Heap::new();
    let child = heap
        .alloc(Kind::Immutable, vec![Value::Int(1)])
        .expect("child");
    let v = heap
        .alloc(Kind::Immutable, vec![Value::Ref(child), Value::Nil])
        .expect("v");
    heap.release(child).expect("caller's count");
    heap.release(v).expect("frees both");
    assert_eq!(
        heap.trace(),
        &[
            Event::Alloc {
                id: child,
                kind: Kind::Immutable
            },
            Event::Alloc {
                id: v,
                kind: Kind::Immutable
            },
            Event::Retain {
                id: child,
                count_after: 2
            },
            Event::Release {
                id: child,
                count_after: 1
            },
            Event::Release {
                id: v,
                count_after: 0
            },
            Event::Free { id: v },
            Event::Release {
                id: child,
                count_after: 0
            },
            Event::Free { id: child },
        ]
    );
}

#[test]
fn write_is_traced_as_retain_new_write_release_old() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Ref(a));
    heap.release(a).expect("caller's count");
    let b = imm(&mut heap, vec![]);
    let before = heap.trace().len();
    heap.write(c, 0, Value::Ref(b)).expect("set!");
    assert_eq!(
        &heap.trace()[before..],
        &[
            Event::Retain {
                id: b,
                count_after: 2
            },
            Event::Write { id: c, field: 0 },
            Event::Release {
                id: a,
                count_after: 0
            },
            Event::Free { id: a },
        ]
    );
    assert_eq!(
        heap.trace()[..before].last(),
        Some(&Event::Alloc {
            id: b,
            kind: Kind::Immutable
        })
    );
}

#[test]
fn read_is_traced_and_changes_no_count() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    let v = imm(&mut heap, vec![Value::Int(0), Value::Ref(a)]);
    let before = heap.trace().len();
    assert_eq!(heap.read(v, 1), Ok(Value::Ref(a)));
    assert_eq!(heap.read(v, 0), Ok(Value::Int(0)));
    assert_eq!(
        &heap.trace()[before..],
        &[
            Event::Read { id: v, field: 1 },
            Event::Read { id: v, field: 0 }
        ]
    );
    assert_eq!(heap.count(a), Ok(2));
}

#[test]
fn count_after_in_events_matches_the_count() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    for expected in 2..6 {
        assert_eq!(heap.retain(a), Ok(expected));
        assert_eq!(heap.count(a), Ok(expected));
        assert_eq!(
            heap.trace().last(),
            Some(&Event::Retain {
                id: a,
                count_after: expected
            })
        );
    }
    for expected in (0..5).rev() {
        assert_eq!(heap.release(a), Ok(expected));
        let release = Event::Release {
            id: a,
            count_after: expected,
        };
        if expected > 0 {
            assert_eq!(heap.trace().last(), Some(&release));
            assert_eq!(heap.count(a), Ok(expected));
        } else {
            let n = heap.trace().len();
            assert_eq!(&heap.trace()[n - 2..], &[release, Event::Free { id: a }]);
        }
    }
}

#[test]
fn the_report_carries_the_whole_trace() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    heap.weak(a).expect("weak");
    heap.upgrade(a).expect("upgrade");
    heap.release(a).expect("release");
    heap.mark_shared(a).expect("cross");
    let snapshot = heap.trace().to_vec();
    assert_eq!(snapshot.len(), 6);
    let report = heap.finish();
    assert_eq!(report.trace, snapshot);
    assert!(report.trace.iter().all(|e| e.id() == a));
}

#[test]
fn failed_operations_leave_no_trace() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Int(1)]);
    let c = cell(&mut heap, Value::Nil);
    let dead = imm(&mut heap, vec![]);
    heap.release(dead).expect("free");
    let stranger = foreign_id();
    let before = heap.trace().to_vec();
    let _ = heap.write(a, 0, Value::Nil);
    let _ = heap.read(a, 7);
    let _ = heap.write(c, 3, Value::Nil);
    let _ = heap.write(c, 0, Value::Ref(dead));
    let _ = heap.mark_shared(c);
    let _ = heap.retain(dead);
    let _ = heap.release(dead);
    let _ = heap.weak(dead);
    let _ = heap.read(dead, 0);
    let _ = heap.release(stranger);
    let _ = heap.upgrade(stranger);
    let _ = heap.alloc(Kind::Cell, vec![]);
    let _ = heap.alloc(Kind::Immutable, vec![Value::Ref(dead)]);
    let _ = heap.alloc(Kind::Immutable, vec![Value::Ref(stranger)]);
    assert_eq!(heap.trace(), &before[..]);
}
