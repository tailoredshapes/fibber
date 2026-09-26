//! State after a failing call. The heap's own contract on `AuditError`
//! says: "The heap's state after an `Err` is exactly what it was
//! before the failing call: every operation validates its arguments
//! before it changes anything." Round 1 checked the early-failing
//! paths; these tests push the failure to the last step of a call.

use fibref::{AuditError, Heap, Value};

use crate::{atom, cell, imm};

/// A cell holding `x`, after `x` was released once too often so that
/// the cell's slot dangles. Returns `(c, x)`.
fn cell_with_dangling_slot(heap: &mut Heap) -> (fibref::ObjId, fibref::ObjId) {
    let x = imm(heap, vec![Value::Int(1)]);
    let c = cell(heap, Value::Ref(x));
    heap.release(x).expect("the binding's count");
    assert_eq!(heap.release(x), Ok(0), "one too many");
    assert!(!heap.is_live(x));
    assert_eq!(heap.read(c, 0), Ok(Value::Ref(x)), "the slot dangles");
    (c, x)
}

#[test]
fn a_write_whose_old_value_release_fails_retains_nothing_and_traces_nothing() {
    let mut heap = Heap::new();
    let (c, x) = cell_with_dangling_slot(&mut heap);
    let y = imm(&mut heap, vec![Value::Int(2)]);
    let before = heap.trace().to_vec();
    let result = heap.write(c, 0, Value::Ref(y));
    assert!(
        matches!(
            result,
            Err(AuditError::ReleaseOfFreed { id }) | Err(AuditError::UseAfterFree { id, .. })
                if id == x
        ),
        "set! over a dangling slot gave {result:?}"
    );
    assert_eq!(
        heap.count(y),
        Ok(1),
        "the new value was retained by a call that reported failure"
    );
    assert_eq!(
        heap.trace(),
        &before[..],
        "a failed write left events in the trace"
    );
    assert_eq!(
        heap.read(c, 0),
        Ok(Value::Ref(x)),
        "a failed write replaced the slot"
    );
}

#[test]
fn a_write_of_nil_over_a_dangling_slot_reports_the_double_free_once() {
    let mut heap = Heap::new();
    let (c, x) = cell_with_dangling_slot(&mut heap);
    let first = heap.write(c, 0, Value::Nil);
    assert!(first.is_err(), "first set! gave {first:?}");
    // Whatever the first call did to the slot, the double free of x
    // has now been reported. A second identical call must not report
    // a *different* object, and must not succeed silently if the slot
    // still dangles.
    let second = heap.write(c, 0, Value::Nil);
    match second {
        Ok(()) => assert_eq!(
            heap.read(c, 0),
            Ok(Value::Nil),
            "the second set! succeeded but the slot is not nil"
        ),
        Err(AuditError::ReleaseOfFreed { id }) | Err(AuditError::UseAfterFree { id, .. }) => {
            assert_eq!(id, x)
        }
        Err(other) => panic!("second set! gave {other:?}"),
    }
}

#[test]
fn a_write_into_a_shared_atom_that_fails_on_a_cell_retains_and_shares_nothing() {
    // Round 1 checked this with the cell one hop away; here the cell
    // is deep, behind an already shared prefix, so that a marker that
    // marks as it goes would have marked the prefix before failing.
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    heap.mark_shared(a).expect("cross");
    let c = cell(&mut heap, Value::Nil);
    let inner = imm(&mut heap, vec![Value::Ref(c)]);
    let shared_leaf = imm(&mut heap, vec![]);
    heap.mark_shared(shared_leaf).expect("already crossed");
    let outer = imm(&mut heap, vec![Value::Ref(shared_leaf), Value::Ref(inner)]);
    let events = heap.trace().len();
    assert_eq!(
        heap.write(a, 0, Value::Ref(outer)),
        Err(AuditError::SharedCell { id: c })
    );
    assert_eq!(heap.is_shared(outer), Ok(false));
    assert_eq!(heap.is_shared(inner), Ok(false));
    assert_eq!(heap.is_shared(c), Ok(false));
    assert_eq!(heap.count(outer), Ok(1));
    assert_eq!(heap.read(a, 0), Ok(Value::Nil));
    assert_eq!(heap.trace().len(), events + 1, "only the read");
}

#[test]
fn a_failed_alloc_after_a_partial_valid_prefix_retains_nothing() {
    let mut heap = Heap::new();
    let live = imm(&mut heap, vec![]);
    let dead = imm(&mut heap, vec![]);
    heap.release(dead).expect("free");
    let events = heap.trace().len();
    let fields = vec![
        Value::Ref(live),
        Value::Ref(live),
        Value::Ref(live),
        Value::Ref(dead),
    ];
    assert!(heap.alloc(fibref::Kind::Immutable, fields).is_err());
    assert_eq!(heap.count(live), Ok(1), "the valid prefix was retained");
    assert_eq!(heap.trace().len(), events);
    assert_eq!(heap.finish().allocated, 2, "a failed alloc took a slot");
}
