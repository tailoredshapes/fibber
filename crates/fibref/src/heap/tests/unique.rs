//! Unique writes (`spec/types.md` §6.6, §8.2): legal exactly under
//! `fib.unique?`, with the retain-new, write, release-old discipline of
//! a cell write, and closing a cycle only through a cell.

use super::{atom, cell, imm};
use crate::heap::{AuditError, Event, Heap, Kind, LeakClass, ObjId, Uniqueness, Value};

/// A cell holding a fresh immutable object with `fields`, the caller's
/// temporary on the object released, so the cell alone holds it.
fn place(heap: &mut Heap, fields: Vec<Value>) -> (ObjId, ObjId) {
    let x = imm(heap, fields);
    let c = cell(heap, Value::Ref(x));
    heap.release(x).expect("the cell holds it now");
    (c, x)
}

fn refused(id: ObjId, why: Uniqueness) -> Result<ObjId, AuditError> {
    Err(AuditError::NotUnique { id, why })
}

#[test]
fn a_unique_object_is_written_in_place() {
    let mut heap = Heap::new();
    let (c, x) = place(&mut heap, vec![Value::Int(1), Value::Int(2)]);
    assert_eq!(heap.is_unique(x), Ok(true));
    assert_eq!(heap.write_unique(c, 1, Value::Int(9)), Ok(x));
    assert_eq!(heap.read(x, 1), Ok(Value::Int(9)));
    heap.release(c).expect("free");
    assert!(heap.finish().is_clean());
}

#[test]
fn a_unique_write_retains_new_writes_then_releases_old() {
    let mut heap = Heap::new();
    let old = imm(&mut heap, vec![]);
    let (c, x) = place(&mut heap, vec![Value::Ref(old)]);
    heap.release(old).expect("x alone holds old");
    let new = imm(&mut heap, vec![]);
    let start = heap.trace().len();
    assert_eq!(heap.write_unique(c, 0, Value::Ref(new)), Ok(x));
    let expected = vec![
        Event::Retain {
            id: new,
            count_after: 2,
        },
        Event::WriteUnique {
            place: c,
            id: x,
            field: 0,
        },
        Event::Release {
            id: old,
            count_after: 0,
        },
        Event::Free { id: old },
    ];
    assert_eq!(heap.trace()[start..].to_vec(), expected);
}

#[test]
fn a_unique_write_whose_old_release_would_fail_changes_nothing() {
    let mut heap = Heap::new();
    let y = imm(&mut heap, vec![]);
    let (c, x) = place(&mut heap, vec![Value::Ref(y)]);
    heap.release(y).expect("x holds y");
    heap.release(y)
        .expect("over-release: y freed while x holds it");
    let new = imm(&mut heap, vec![]);
    let before = heap.trace().len();
    assert_eq!(
        heap.write_unique(c, 0, Value::Ref(new)),
        Err(AuditError::ReleaseOfFreed { id: y })
    );
    assert_eq!(heap.trace().len(), before);
    assert_eq!(heap.count(new), Ok(1));
    assert_eq!(heap.read(x, 0), Ok(Value::Ref(y)));
}

#[test]
fn count_above_one_is_refused() {
    let mut heap = Heap::new();
    let (c, x) = place(&mut heap, vec![Value::Int(1)]);
    heap.retain(x).expect("another binding");
    assert_eq!(heap.is_unique(x), Ok(false));
    assert_eq!(
        heap.write_unique(c, 0, Value::Int(2)),
        refused(x, Uniqueness::Count(2))
    );
    assert_eq!(heap.read(x, 0), Ok(Value::Int(1)));
}

#[test]
fn shared_is_refused_before_the_count() {
    let mut heap = Heap::new();
    let (c, x) = place(&mut heap, vec![Value::Int(1)]);
    heap.mark_shared(x).expect("share");
    assert_eq!(heap.count(x), Ok(1));
    assert_eq!(
        heap.write_unique(c, 0, Value::Int(2)),
        refused(x, Uniqueness::Shared)
    );
}

#[test]
fn immortal_is_refused() {
    let mut heap = Heap::new();
    let lit = heap
        .alloc_immortal(Kind::Immutable, vec![Value::Int(1)])
        .expect("literal");
    let c = cell(&mut heap, Value::Ref(lit));
    assert_eq!(heap.is_unique(lit), Ok(false));
    assert_eq!(
        heap.write_unique(c, 0, Value::Int(2)),
        refused(lit, Uniqueness::Immortal)
    );
}

#[test]
fn stack_is_refused() {
    let mut heap = Heap::new();
    let s = heap.open_scope();
    let x = heap
        .alloc_in_scope(s, Kind::Immutable, vec![Value::Int(1)])
        .expect("scope-local");
    let c = heap
        .alloc_in_scope(s, Kind::Cell, vec![Value::Ref(x)])
        .expect("scope-local cell");
    assert_eq!(
        heap.write_unique(c, 0, Value::Int(2)),
        refused(x, Uniqueness::Stack)
    );
}

#[test]
fn the_place_must_be_a_cell_holding_an_immutable() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![Value::Int(1)]);
    let a = atom(&mut heap, Value::Ref(x));
    let num = cell(&mut heap, Value::Int(3));
    let inner = cell(&mut heap, Value::Nil);
    let outer = cell(&mut heap, Value::Ref(inner));
    let v = Value::Int(2);
    assert_eq!(
        heap.write_unique(x, 0, v),
        refused(x, Uniqueness::PlaceNotCell)
    );
    assert_eq!(
        heap.write_unique(a, 0, v),
        refused(a, Uniqueness::PlaceNotCell)
    );
    assert_eq!(
        heap.write_unique(num, 0, v),
        refused(num, Uniqueness::ContentNotImmutable)
    );
    assert_eq!(
        heap.write_unique(outer, 0, v),
        refused(inner, Uniqueness::ContentNotImmutable)
    );
}

#[test]
fn a_bad_field_is_refused() {
    let mut heap = Heap::new();
    let (c, x) = place(&mut heap, vec![Value::Int(1)]);
    assert_eq!(
        heap.write_unique(c, 1, Value::Nil),
        Err(AuditError::BadField { id: x, index: 1 })
    );
}

#[test]
fn a_unique_write_closes_a_cycle_only_through_the_cell() {
    // Proposed case 80: (let ((c (cell (K nil)))) (set-field! &c back
    // (some c))): the unique K is written in place, tying c -> K -> c.
    let mut heap = Heap::new();
    let (c, k) = place(&mut heap, vec![Value::Nil]);
    assert_eq!(heap.write_unique(c, 0, Value::Ref(c)), Ok(k));
    heap.release(c).expect("the let ends");
    let report = heap.finish();
    assert!(report.is_cycle_leak_only(), "{:?}", report.leaks);
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![k, c]);
    assert!(report.ids_in(LeakClass::ImmutableCycle).is_empty());
}

#[test]
fn a_unique_write_cannot_tie_an_immutable_cycle() {
    let mut heap = Heap::new();
    // Writing x into itself: the caller holds x, so it cannot be unique.
    let (c, x) = place(&mut heap, vec![Value::Nil]);
    assert_eq!(
        heap.write_unique(c, 0, Value::Ref(x)),
        refused(x, Uniqueness::StoresItself)
    );
    // z holds x, so writing z into x would tie x -> z -> x; but then the
    // cell and z both hold x, and its count is 2.
    let z = imm(&mut heap, vec![Value::Ref(x)]);
    assert_eq!(
        heap.write_unique(c, 0, Value::Ref(z)),
        refused(x, Uniqueness::Count(2))
    );
    // With the cell gone, z alone holds x; z is no place to write through.
    heap.write(c, 0, Value::Nil).expect("cell lets go of x");
    assert_eq!(heap.count(x), Ok(1));
    assert_eq!(
        heap.write_unique(z, 0, Value::Ref(z)),
        refused(z, Uniqueness::PlaceNotCell)
    );
    heap.release(z).expect("free z and x");
    heap.release(c).expect("free c");
    let report = heap.finish();
    assert!(report.is_clean(), "{:?}", report.leaks);
}

#[test]
fn an_object_that_had_a_weak_reference_is_never_unique() {
    // ownership.md §5, types §6.6 (Decided, owner, 2026-09-27; case 89):
    // HAS-WEAK is sticky, so even after the weak value is gone the
    // object is copied on update rather than written in place.
    let mut heap = Heap::new();
    let (c, x) = place(&mut heap, vec![Value::Int(0)]);
    assert_eq!(heap.is_unique(x), Ok(true));
    heap.weak(x).expect("weak");
    assert_eq!(heap.is_unique(x), Ok(false));
    assert_eq!(
        heap.write_unique(c, 0, Value::Int(9)),
        refused(x, Uniqueness::HasWeak)
    );
    assert_eq!(heap.read(x, 0), Ok(Value::Int(0)));
    heap.release(c).expect("free");
    assert!(heap.finish().is_clean());
}
