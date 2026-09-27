//! Adversarial tests for stack scopes (`spec/types.md` §6.11), through
//! the heap's public API only. Each test tries to smuggle a reference to
//! a `STACK` object past the audit, so that something could reach it
//! after its scope ends, or to make the audit mistake a stack object
//! for a leak. A failing test here is a finding about
//! `crates/fibref/src/heap`, not about the test.

use fibref::{AuditError, Event, Heap, Kind, LeakClass, ObjId, Op, ScopeId, Value};

fn imm(heap: &mut Heap, fields: Vec<Value>) -> ObjId {
    heap.alloc(Kind::Immutable, fields)
        .expect("alloc immutable")
}

fn cell(heap: &mut Heap, value: Value) -> ObjId {
    heap.alloc(Kind::Cell, vec![value]).expect("alloc cell")
}

fn stack(heap: &mut Heap, scope: ScopeId, kind: Kind, fields: Vec<Value>) -> ObjId {
    heap.alloc_in_scope(scope, kind, fields)
        .expect("alloc in scope")
}

/// Every failing call below must leave the trace as it was.
fn unchanged(heap: &Heap, before: usize) {
    assert_eq!(heap.trace().len(), before, "a failing call left events");
}

#[test]
fn laundering_a_stack_ref_through_a_stack_cell_into_a_heap_cell_is_caught() {
    let mut heap = Heap::new();
    let hc = cell(&mut heap, Value::Nil);
    let s = heap.open_scope();
    let h = stack(&mut heap, s, Kind::Immutable, vec![]);
    let sc = stack(&mut heap, s, Kind::Cell, vec![Value::Ref(h)]);
    let laundered = heap.read(sc, 0).expect("read the stack cell");
    let before = heap.trace().len();
    assert_eq!(
        heap.write(hc, 0, laundered),
        Err(AuditError::StackRefInHeap { id: h })
    );
    unchanged(&heap, before);
    assert_eq!(heap.read(hc, 0), Ok(Value::Nil));
}

#[test]
fn a_stack_ref_cannot_reach_another_thread_through_a_shared_atom() {
    let mut heap = Heap::new();
    let a = heap.alloc(Kind::Atom, vec![Value::Nil]).expect("atom");
    heap.mark_shared(a).expect("the atom crosses");
    let s = heap.open_scope();
    let h = stack(&mut heap, s, Kind::Immutable, vec![]);
    let before = heap.trace().len();
    assert_eq!(
        heap.write(a, 0, Value::Ref(h)),
        Err(AuditError::StackRefInHeap { id: h })
    );
    unchanged(&heap, before);
    assert_eq!(heap.is_shared(h), Ok(false));
}

#[test]
fn a_forged_weak_to_a_stack_object_is_caught_everywhere() {
    let mut heap = Heap::new();
    let hc = cell(&mut heap, Value::Nil);
    let outer = heap.open_scope();
    let oc = stack(&mut heap, outer, Kind::Cell, vec![Value::Nil]);
    let inner = heap.open_scope();
    let h = stack(&mut heap, inner, Kind::Immutable, vec![]);
    let forged = Value::Weak(h);
    let err = AuditError::WeakToStack { id: h };
    let before = heap.trace().len();
    assert_eq!(heap.weak(h), Err(err));
    assert_eq!(heap.write(hc, 0, forged), Err(err));
    assert_eq!(heap.write(oc, 0, forged), Err(err));
    assert_eq!(heap.alloc(Kind::Immutable, vec![forged]), Err(err));
    assert_eq!(
        heap.alloc_in_scope(inner, Kind::Immutable, vec![forged]),
        Err(err)
    );
    assert_eq!(heap.upgrade(h), Err(err));
    unchanged(&heap, before);
    heap.end_scope(inner).expect("end inner");
    // Still an error once the target has ended: no weak to it ever.
    assert_eq!(heap.upgrade(h), Err(err));
    assert_eq!(heap.write(hc, 0, forged), Err(err));
}

#[test]
fn a_stack_ref_nested_inside_stack_structs_cannot_reach_the_heap() {
    let mut heap = Heap::new();
    let s = heap.open_scope();
    let leaf = stack(&mut heap, s, Kind::Immutable, vec![Value::Int(1)]);
    let mid = stack(&mut heap, s, Kind::Immutable, vec![Value::Ref(leaf)]);
    let top = stack(&mut heap, s, Kind::Immutable, vec![Value::Ref(mid)]);
    let before = heap.trace().len();
    assert_eq!(
        heap.alloc(Kind::Immutable, vec![Value::Int(0), Value::Ref(top)]),
        Err(AuditError::StackRefInHeap { id: top })
    );
    unchanged(&heap, before);
    heap.end_scope(s).expect("end");
    let report = heap.finish();
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.allocated, 0);
}

#[test]
fn an_inner_ref_cannot_hide_in_an_outer_object_through_an_inner_cell() {
    let mut heap = Heap::new();
    let outer = heap.open_scope();
    let oc = stack(&mut heap, outer, Kind::Cell, vec![Value::Nil]);
    let inner = heap.open_scope();
    let i = stack(&mut heap, inner, Kind::Immutable, vec![]);
    let ic = stack(&mut heap, inner, Kind::Cell, vec![Value::Ref(i)]);
    let err = |id| AuditError::StackRefIntoOuterScope { id, scope: outer };
    assert_eq!(heap.write(oc, 0, Value::Ref(ic)), Err(err(ic)));
    assert_eq!(heap.write(oc, 0, Value::Ref(i)), Err(err(i)));
    // Allocating into the outer scope while the inner one is open.
    assert_eq!(
        heap.alloc_in_scope(outer, Kind::Immutable, vec![Value::Ref(i)]),
        Err(err(i))
    );
    // A deeper scope's object is inner to both.
    let deepest = heap.open_scope();
    let d = stack(&mut heap, deepest, Kind::Immutable, vec![]);
    assert_eq!(
        heap.write(ic, 0, Value::Ref(d)),
        Err(AuditError::StackRefIntoOuterScope {
            id: d,
            scope: inner
        })
    );
    heap.end_scope(deepest).expect("end deepest");
    heap.end_scope(inner).expect("end inner");
    heap.end_scope(outer).expect("end outer");
    assert!(heap.finish().is_clean());
}

#[test]
fn a_ref_to_an_ended_object_cannot_be_stored_anywhere() {
    let mut heap = Heap::new();
    let hc = cell(&mut heap, Value::Nil);
    let outer = heap.open_scope();
    let oc = stack(&mut heap, outer, Kind::Cell, vec![Value::Nil]);
    let inner = heap.open_scope();
    let gone = stack(&mut heap, inner, Kind::Immutable, vec![]);
    heap.end_scope(inner).expect("end inner");
    let err = AuditError::StackUseAfterScope {
        id: gone,
        op: Op::Store,
    };
    assert_eq!(heap.write(oc, 0, Value::Ref(gone)), Err(err));
    assert_eq!(heap.write(hc, 0, Value::Ref(gone)), Err(err));
    assert_eq!(
        heap.alloc_in_scope(outer, Kind::Immutable, vec![Value::Ref(gone)]),
        Err(err)
    );
}

#[test]
fn ending_a_scope_out_of_order_changes_nothing() {
    let mut heap = Heap::new();
    let child = imm(&mut heap, vec![]);
    let outer = heap.open_scope();
    let o = stack(&mut heap, outer, Kind::Immutable, vec![Value::Ref(child)]);
    let inner = heap.open_scope();
    let before = heap.trace().len();
    assert_eq!(
        heap.end_scope(outer),
        Err(AuditError::ScopeNotInnermost {
            scope: outer,
            innermost: inner
        })
    );
    unchanged(&heap, before);
    assert_eq!(heap.read(o, 0), Ok(Value::Ref(child)));
    assert_eq!(heap.count(child), Ok(2));
    assert_eq!(heap.open_scopes(), &[outer, inner]);
}

#[test]
fn a_scope_id_used_after_its_end_is_refused_and_never_names_a_new_scope() {
    let mut heap = Heap::new();
    let s = heap.open_scope();
    heap.end_scope(s).expect("end");
    let t = heap.open_scope();
    assert_ne!(s, t, "scope ids are never reused");
    let before = heap.trace().len();
    let err = AuditError::ScopeEnded { scope: s };
    assert_eq!(heap.alloc_in_scope(s, Kind::Immutable, vec![]), Err(err));
    assert_eq!(heap.end_scope(s), Err(err));
    unchanged(&heap, before);
    assert_eq!(heap.open_scopes(), &[t]);
}

#[test]
fn a_scope_id_from_another_heap_is_unknown() {
    let mut other = Heap::new();
    for _ in 0..3 {
        other.open_scope();
    }
    let foreign = other.open_scope();
    let mut heap = Heap::new();
    heap.open_scope();
    assert_eq!(
        heap.alloc_in_scope(foreign, Kind::Immutable, vec![]),
        Err(AuditError::UnknownScope { scope: foreign })
    );
}

#[test]
fn a_stack_object_read_after_its_scope_in_the_same_activation_is_not_a_plain_use_after_free() {
    let mut heap = Heap::new();
    let child = imm(&mut heap, vec![]);
    let s = heap.open_scope();
    let h = stack(&mut heap, s, Kind::Immutable, vec![Value::Ref(child)]);
    heap.end_scope(s).expect("end");
    // The site runs again, as a loop body would, and makes a new object.
    let s2 = heap.open_scope();
    let h2 = stack(&mut heap, s2, Kind::Immutable, vec![Value::Ref(child)]);
    assert_ne!(h, h2);
    let err = AuditError::StackUseAfterScope {
        id: h,
        op: Op::Read,
    };
    assert_eq!(heap.read(h, 0), Err(err));
    assert_ne!(
        heap.read(h, 0),
        Err(AuditError::UseAfterFree {
            id: h,
            op: Op::Read
        })
    );
    assert_eq!(heap.read(h2, 0), Ok(Value::Ref(child)));
    heap.end_scope(s2).expect("end");
    heap.release(child).expect("free");
    assert!(heap.finish().is_clean());
}

#[test]
fn sharing_a_heap_object_through_a_stack_object_marks_nothing() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let s = heap.open_scope();
    let sa = stack(&mut heap, s, Kind::Atom, vec![Value::Ref(x)]);
    let before = heap.trace().len();
    assert_eq!(
        heap.mark_shared(sa),
        Err(AuditError::SharedStack { id: sa })
    );
    unchanged(&heap, before);
    assert_eq!(heap.is_shared(x), Ok(false));
}

#[test]
fn counting_a_stack_object_is_refused_even_through_a_read_ref() {
    let mut heap = Heap::new();
    let s = heap.open_scope();
    let h = stack(&mut heap, s, Kind::Immutable, vec![]);
    let sc = stack(&mut heap, s, Kind::Cell, vec![Value::Ref(h)]);
    let read = heap.read(sc, 0).expect("read").as_ref().expect("a ref");
    assert_eq!(
        heap.retain(read),
        Err(AuditError::CountOnStack {
            id: h,
            op: Op::Retain
        })
    );
    assert_eq!(heap.count(h), Ok(0));
}

#[test]
fn a_self_holding_stack_cell_ends_cleanly() {
    let mut heap = Heap::new();
    let s = heap.open_scope();
    let c = stack(&mut heap, s, Kind::Cell, vec![Value::Nil]);
    heap.write(c, 0, Value::Ref(c)).expect("same scope");
    heap.end_scope(s).expect("end");
    let report = heap.finish();
    assert!(report.is_clean(), "{report:?}");
}

#[test]
fn an_unended_scope_is_reported_and_what_it_holds_is_a_plain_leak() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let s = heap.open_scope();
    stack(&mut heap, s, Kind::Cell, vec![Value::Ref(x)]);
    heap.release(x).expect("the stack cell holds x");
    let report = heap.finish();
    assert_eq!(report.open_scopes, vec![s]);
    assert_eq!(report.leaks.len(), 1, "only x; the stack cell is no leak");
    assert_eq!(report.leaks[0].id, x);
    assert_eq!(report.leaks[0].class, LeakClass::Leak);
    assert!(!report.is_clean());
}

#[test]
fn a_stack_cycle_through_a_heap_cell_cannot_form() {
    // A stack cell holding a heap cell that would hold the stack cell
    // back: the heap cell could outlive the scope.
    let mut heap = Heap::new();
    let hc = cell(&mut heap, Value::Nil);
    let s = heap.open_scope();
    let sc = stack(&mut heap, s, Kind::Cell, vec![Value::Ref(hc)]);
    assert_eq!(
        heap.write(hc, 0, Value::Ref(sc)),
        Err(AuditError::StackRefInHeap { id: sc })
    );
    heap.end_scope(s).expect("end");
    assert_eq!(heap.count(hc), Ok(1));
    heap.release(hc).expect("free");
    assert!(heap.finish().is_clean());
}

#[test]
fn scope_events_are_recorded_in_order_with_drops_most_recent_first() {
    let mut heap = Heap::new();
    let outer = heap.open_scope();
    let a = stack(&mut heap, outer, Kind::Immutable, vec![]);
    let inner = heap.open_scope();
    let b = stack(&mut heap, inner, Kind::Immutable, vec![]);
    let c = stack(&mut heap, inner, Kind::Immutable, vec![]);
    heap.end_scope(inner).expect("end inner");
    heap.end_scope(outer).expect("end outer");
    let alloc = |id, scope| Event::AllocStack {
        id,
        kind: Kind::Immutable,
        scope,
    };
    let expected = vec![
        Event::ScopeOpen { scope: outer },
        alloc(a, outer),
        Event::ScopeOpen { scope: inner },
        alloc(b, inner),
        alloc(c, inner),
        Event::ScopeEnd { scope: inner },
        Event::Drop { id: c },
        Event::Drop { id: b },
        Event::ScopeEnd { scope: outer },
        Event::Drop { id: a },
    ];
    assert_eq!(heap.trace(), &expected[..]);
    let ids: Vec<_> = expected.iter().map(|e| e.id()).collect();
    assert_eq!(ids[0], None);
    assert_eq!(ids[1], Some(a));
}
