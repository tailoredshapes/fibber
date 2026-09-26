//! Counts that are too low: an over-release that has not (yet) reached
//! zero because some other holder still keeps the object above it.
//! Rounds 1 and 2 covered the surplus (a forgotten release) and the
//! over-release that frees under a holder. These tests cover the
//! over-release that a second holder hides: the object stays live with
//! fewer counts than `Ref` fields naming it, and the audit must still
//! call it a bug, at finish and in any cascade that reaches it. Also
//! here: forged ids, the only conceivable way to build an immutable
//! cycle through the public API.

use fibref::{AuditError, Heap, Kind, LeakClass, Value};

use crate::{cell, imm, release_all};

#[test]
fn an_over_release_hidden_by_a_second_holder_is_a_leak_not_a_leak_cycle() {
    // c -> h -> [v, v] -> c. After the bindings go: c held once (by
    // v), h once (by c), v twice (by h). Release v once too often: it
    // stays live with count 1 but two fields name it.
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c)]);
    let h = imm(&mut heap, vec![Value::Ref(v), Value::Ref(v)]);
    heap.write(c, 0, Value::Ref(h)).expect("close the cycle");
    release_all(&mut heap, &[v, h, c]);
    assert_eq!(heap.count(v), Ok(2));
    assert_eq!(
        heap.release(v),
        Ok(1),
        "one too many, hidden by h's second field"
    );
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, h]);
    assert_eq!(
        report.ids_in(LeakClass::Leak),
        vec![v],
        "an over-released cycle member was laundered by the cycle: {:?}",
        report.leaks
    );
    let leak = report
        .leaks
        .iter()
        .find(|l| l.id == v)
        .expect("v is reported");
    assert_eq!((leak.count, leak.held), (1, 2));
    assert!(!report.is_cycle_leak_only());
    assert!(report.dangling.is_empty(), "v is live, nothing dangles yet");
}

#[test]
fn a_cascade_that_reaches_an_over_released_leaf_twice_is_refused_whole() {
    // root -> [x, x], x's binding gone, then x released once too often:
    // x is live with count 1 but root names it twice. Freeing root
    // would release x twice; the second is a release of a freed object.
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![Value::Int(1)]);
    let root = imm(&mut heap, vec![Value::Ref(x), Value::Ref(x)]);
    heap.release(x).expect("the binding's count");
    assert_eq!(heap.release(x), Ok(1), "one too many, hidden by root");
    let events = heap.trace().len();
    let result = heap.release(root);
    assert!(
        matches!(
            result,
            Err(AuditError::ReleaseOfFreed { id }) | Err(AuditError::UseAfterFree { id, .. })
                if id == x
        ),
        "freeing a holder with a deficit leaf gave {result:?}"
    );
    assert!(heap.is_live(root), "a refused cascade freed the root");
    assert!(heap.is_live(x), "a refused cascade freed the leaf");
    assert_eq!((heap.count(root), heap.count(x)), (Ok(1), Ok(1)));
    assert_eq!(heap.trace().len(), events, "a refused cascade was traced");
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::Leak), vec![x, root]);
    assert!(report.dangling.is_empty());
}

#[test]
fn a_write_whose_cascade_frees_the_cell_under_a_deficit_leaves_a_dangling_ref_that_is_caught() {
    // c holds itself; v also holds c. c's binding is released, then c
    // once more (the deficit): count 1, held 2. set! c to v: the
    // release of the old value (c itself) frees c while v still names
    // it. The next release of v must be refused, and finish must list
    // the dangling field.
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    heap.write(c, 0, Value::Ref(c)).expect("self reference");
    let v = imm(&mut heap, vec![Value::Ref(c)]);
    heap.release(c).expect("the binding's count");
    assert_eq!(heap.release(c), Ok(1), "one too many, hidden by v");
    let result = heap.write(c, 0, Value::Ref(v));
    match result {
        Ok(()) => {
            assert!(!heap.is_live(c), "the slot's count was c's last");
            assert_eq!(heap.count(v), Ok(1), "c's slot gave v's count back");
            let release = heap.release(v);
            assert!(
                matches!(
                    release,
                    Err(AuditError::ReleaseOfFreed { id })
                        | Err(AuditError::UseAfterFree { id, .. })
                        if id == c
                ),
                "releasing the holder of a freed cell gave {release:?}"
            );
            assert!(heap.is_live(v));
            let report = heap.finish();
            assert_eq!(report.ids_in(LeakClass::Leak), vec![v]);
            assert_eq!(report.dangling.len(), 1);
            assert_eq!(
                (report.dangling[0].holder, report.dangling[0].target),
                (v, c)
            );
        }
        Err(AuditError::ReleaseOfFreed { id }) | Err(AuditError::UseAfterFree { id, .. }) => {
            // Also acceptable: refusing the write because its cascade
            // frees an object still held. Then nothing changed.
            assert_eq!(id, c);
            assert!(heap.is_live(c));
            assert_eq!(heap.count(v), Ok(1));
        }
        Err(other) => panic!("set! under a deficit gave {other:?}"),
    }
}

#[test]
fn an_object_cannot_hold_itself_through_a_forged_future_id() {
    // ObjId cannot be built by a caller, but another heap hands out ids
    // with any index. Forge the id this heap will give its next object
    // and try to store it into that very object: the only way to make
    // an Immutable refer to itself or to a newer object (§1).
    let mut heap = Heap::new();
    let older = imm(&mut heap, vec![]);
    let mut other = Heap::new();
    let mut forged = imm(&mut other, vec![]);
    while forged.index() <= older.index() {
        forged = imm(&mut other, vec![]);
    }
    assert!(
        !heap.is_live(forged),
        "the forged id is not yet an object here"
    );
    let events = heap.trace().len();
    for fields in [
        vec![Value::Ref(forged)],
        vec![Value::Ref(older), Value::Ref(forged)],
        vec![Value::Weak(forged)],
    ] {
        assert_eq!(
            heap.alloc(Kind::Immutable, fields),
            Err(AuditError::UnknownId { id: forged }),
            "an object was allowed to name an id that does not exist yet"
        );
    }
    assert_eq!(
        heap.alloc(Kind::Cell, vec![Value::Ref(forged)]),
        Err(AuditError::UnknownId { id: forged })
    );
    assert_eq!(heap.count(older), Ok(1), "a refused alloc retained a field");
    assert_eq!(heap.trace().len(), events, "a refused alloc was traced");
    // The forgery did target the slot the next allocation takes.
    let next = imm(&mut heap, vec![]);
    assert_eq!(next, forged, "the test forged the wrong index");
    release_all(&mut heap, &[older, next]);
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!(report.allocated, 2, "a refused alloc took a slot");
    assert!(report.ids_in(LeakClass::ImmutableCycle).is_empty());
}

#[test]
fn a_forged_id_stored_as_a_weak_never_upgrades_into_a_newer_object() {
    // A Weak may name a freed object, so the heap accepts one at alloc
    // as long as the id is known. The question is whether a Weak to a
    // *freed* id can later name a different object: it must not, and
    // a Weak that names a live object that is then freed must go dead
    // for good, however many allocations follow (ids never reused).
    let mut heap = Heap::new();
    let doomed = imm(&mut heap, vec![]);
    let holder = cell(&mut heap, Value::Weak(doomed));
    heap.release(doomed).expect("free");
    for i in 0..1000 {
        let filler = imm(&mut heap, vec![Value::Int(i)]);
        heap.release(filler).expect("free");
    }
    let stale = heap.read(holder, 0).expect("read");
    assert_eq!(stale, Value::Weak(doomed));
    assert_eq!(heap.upgrade(doomed), Ok(None));
    assert!(!heap.is_live(doomed));
    heap.release(holder).expect("free");
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!(report.allocated, 1002);
}
