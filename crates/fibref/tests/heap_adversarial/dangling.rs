//! A double free the heap cannot see when it happens: an object is
//! released more times than its holder owns while some live object
//! still holds a `Ref` to it. The count reaches zero, the object is
//! freed, and the live holder's field now dangles. Every later touch
//! of that dangling `Ref` must be an `AuditError`, never a panic and
//! never a silent success (`spec/method.md` rule 2).

use fibref::{AuditError, Heap, LeakClass, Op, Value};

use crate::{cell, cell_cycle, imm, release_all, shared_ids};

/// A parent holding `x`, after `x` was released once too often. Returns
/// `(parent, x)`; `x` is freed, `parent` is live and its field 0 is a
/// dangling `Ref(x)`.
fn parent_with_dangling_child(heap: &mut Heap) -> (fibref::ObjId, fibref::ObjId) {
    let x = imm(heap, vec![Value::Int(1)]);
    let parent = imm(heap, vec![Value::Ref(x)]);
    assert_eq!(heap.count(x), Ok(2));
    heap.release(x).expect("the binding's count");
    // One too many: the heap sees 1 -> 0 and cannot tell yet.
    assert_eq!(heap.release(x), Ok(0));
    assert!(!heap.is_live(x));
    assert!(heap.is_live(parent));
    assert_eq!(heap.read(parent, 0), Ok(Value::Ref(x)), "the field dangles");
    (parent, x)
}

#[test]
fn a_dangling_ref_read_out_of_its_holder_is_use_after_free_at_the_target() {
    let mut heap = Heap::new();
    let (parent, x) = parent_with_dangling_child(&mut heap);
    let stale = heap.read(parent, 0).expect("read");
    assert_eq!(
        heap.read(x, 0),
        Err(AuditError::UseAfterFree {
            id: x,
            op: Op::Read
        })
    );
    assert_eq!(
        heap.alloc(fibref::Kind::Immutable, vec![stale]),
        Err(AuditError::UseAfterFree {
            id: x,
            op: Op::Store
        })
    );
    assert_eq!(
        heap.retain(x),
        Err(AuditError::UseAfterFree {
            id: x,
            op: Op::Retain
        })
    );
}

#[test]
fn cascade_into_an_over_released_child_is_an_error_not_a_panic() {
    let mut heap = Heap::new();
    let (parent, x) = parent_with_dangling_child(&mut heap);
    // Freeing the parent releases its dangling Ref: that is the release
    // of a freed object, the double free of method.md rule 2. It must
    // come back as an error; a panic (or a wrapped count) is the audit
    // failing to audit.
    let result = heap.release(parent);
    assert!(
        matches!(
            result,
            Err(AuditError::ReleaseOfFreed { id }) | Err(AuditError::UseAfterFree { id, .. })
                if id == x
        ),
        "releasing a holder of a freed object gave {result:?}"
    );
    assert!(!heap.is_live(x));
}

#[test]
fn write_cascade_into_an_over_released_grandchild_is_an_error_not_a_panic() {
    let mut heap = Heap::new();
    let (parent, x) = parent_with_dangling_child(&mut heap);
    let c = cell(&mut heap, Value::Ref(parent));
    heap.release(parent).expect("the cell holds it now");
    assert_eq!(heap.count(parent), Ok(1));
    // set! nil frees parent, whose cascade reaches the freed x.
    let result = heap.write(c, 0, Value::Nil);
    assert!(
        matches!(
            result,
            Err(AuditError::ReleaseOfFreed { id }) | Err(AuditError::UseAfterFree { id, .. })
                if id == x
        ),
        "set! over a holder of a freed object gave {result:?}"
    );
}

#[test]
fn releasing_a_self_holding_cell_past_its_owner_count_is_an_error_not_a_panic() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    heap.write(c, 0, Value::Ref(c)).expect("self reference");
    assert_eq!(heap.count(c), Ok(2), "the binding and the slot");
    heap.release(c).expect("the binding's count");
    // The slot's count is not the caller's to release. The heap sees
    // 1 -> 0, which would free c and then release c's own slot, which
    // names the object just freed. The cascade is planned before it is
    // applied, so the error is found first and the heap is exactly as
    // it was: c is still live with the slot's count.
    let events = heap.trace().len();
    let result = heap.release(c);
    assert!(
        result.is_err(),
        "over-releasing a self-holding cell gave {result:?}"
    );
    assert!(heap.is_live(c), "a refused release freed the object");
    assert_eq!(heap.count(c), Ok(1));
    assert_eq!(heap.trace().len(), events, "a refused release was traced");
}

#[test]
fn mark_shared_through_a_dangling_ref_is_use_after_free_and_marks_nothing() {
    let mut heap = Heap::new();
    let (parent, x) = parent_with_dangling_child(&mut heap);
    let events = heap.trace().len();
    // `Op::Share` is documented as "mark_shared, or reached while
    // marking": a freed object reached while marking is a use after
    // free, not a newly shared object.
    assert_eq!(
        heap.mark_shared(parent),
        Err(AuditError::UseAfterFree {
            id: x,
            op: Op::Share
        })
    );
    assert_eq!(heap.is_shared(parent), Ok(false), "nothing was marked");
    assert_eq!(heap.trace().len(), events, "nothing was traced");
    assert!(
        !shared_ids(heap.trace()).contains(&x),
        "a freed object was traced as shared"
    );
}

#[test]
fn mark_shared_through_a_dangling_ref_to_a_freed_cell_is_use_after_free_not_shared_cell() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Int(1));
    let holder = imm(&mut heap, vec![Value::Ref(c)]);
    heap.release(c).expect("the binding's count");
    assert_eq!(heap.release(c), Ok(0), "one too many");
    assert!(!heap.is_live(c));
    let result = heap.mark_shared(holder);
    assert_eq!(
        result,
        Err(AuditError::UseAfterFree {
            id: c,
            op: Op::Share
        }),
        "a freed cell is a freed object first"
    );
}

#[test]
fn finish_does_not_accept_a_permitted_cycle_that_holds_a_dangling_ref() {
    // c -> v -> c is the permitted leak of §6. v also holds x, and x
    // was released once too often, so v's second field dangles. The
    // run freed an object that was still referenced (method.md rule
    // 2: a double free). The audit must not report it as "only the
    // permitted cycle leak".
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![Value::Int(1)]);
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c), Value::Ref(x)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    release_all(&mut heap, &[v, c, x]);
    assert_eq!(heap.count(x), Ok(1), "held by v only");
    assert_eq!(heap.release(x), Ok(0), "one too many: x is freed under v");
    assert_eq!(heap.read(v, 1), Ok(Value::Ref(x)), "v's field dangles");
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert!(
        !report.is_cycle_leak_only(),
        "a run with a dangling reference passed as a permitted cycle leak: {:?}",
        report.leaks
    );
}

#[test]
fn a_dangling_ref_does_not_make_the_freed_object_live_again_at_finish() {
    let mut heap = Heap::new();
    let (c, v) = cell_cycle(&mut heap);
    let (parent, x) = parent_with_dangling_child(&mut heap);
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![parent]);
    assert!(
        report.leaks.iter().all(|l| l.id != x),
        "a freed object was reported as a leak"
    );
    assert_eq!(report.freed, 1);
}
