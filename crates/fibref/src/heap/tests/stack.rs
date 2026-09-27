//! Stack scopes (`spec/types.md` §6.11): scope-local objects are never
//! counted, end with their scope running their drop, and no reference
//! to one outlives the scope.

use super::{atom, cell, imm};
use crate::heap::{AuditError, Event, Heap, Kind, ObjId, Op, ScopeId, Value};

fn stack(heap: &mut Heap, scope: ScopeId, kind: Kind, fields: Vec<Value>) -> ObjId {
    heap.alloc_in_scope(scope, kind, fields)
        .expect("alloc in scope")
}

/// The events of the run below: a heap `child` held by the stack
/// object `h`, beside a stack cell `c`, all in scope `s`.
fn scope_run_events(child: ObjId, s: ScopeId, h: ObjId, c: ObjId) -> Vec<Event> {
    vec![
        Event::Alloc {
            id: child,
            kind: Kind::Immutable,
        },
        Event::ScopeOpen { scope: s },
        Event::AllocStack {
            id: h,
            kind: Kind::Immutable,
            scope: s,
        },
        Event::Retain {
            id: child,
            count_after: 2,
        },
        Event::AllocStack {
            id: c,
            kind: Kind::Cell,
            scope: s,
        },
        Event::Release {
            id: child,
            count_after: 1,
        },
        Event::ScopeEnd { scope: s },
        Event::Drop { id: c },
        Event::Drop { id: h },
        Event::Release {
            id: child,
            count_after: 0,
        },
        Event::Free { id: child },
    ]
}

#[test]
fn a_scope_ends_its_objects_and_releases_what_they_hold() {
    let mut heap = Heap::new();
    let child = imm(&mut heap, vec![]);
    let s = heap.open_scope();
    let h = stack(&mut heap, s, Kind::Immutable, vec![Value::Ref(child)]);
    let c = stack(&mut heap, s, Kind::Cell, vec![Value::Int(0)]);
    assert_eq!(heap.count(h), Ok(0));
    assert_eq!(heap.stack_scope(h), Ok(Some(s)));
    heap.release(child).expect("h alone holds child");
    heap.end_scope(s).expect("end");
    assert!(!heap.is_live(h) && !heap.is_live(c) && !heap.is_live(child));
    let report = heap.finish();
    assert!(report.is_clean(), "{:?}", report);
    assert_eq!((report.allocated, report.freed), (1, 1));
    assert_eq!(report.trace, scope_run_events(child, s, h, c));
}

#[test]
fn retain_and_release_of_a_live_stack_object_are_untraced_no_ops() {
    // types §8.2: STACK retain/release are no-ops (§6.11 passes stack
    // objects to owned parameters whose callee releases them).
    let mut heap = Heap::new();
    let s = heap.open_scope();
    let h = stack(&mut heap, s, Kind::Immutable, vec![]);
    let before = heap.trace().len();
    assert_eq!(heap.retain(h), Ok(0));
    assert_eq!(heap.release(h), Ok(0));
    assert_eq!(heap.release(h), Ok(0));
    assert!(heap.is_live(h));
    assert_eq!(heap.trace().len(), before);
    heap.end_scope(s).expect("end");
    assert_eq!(
        heap.retain(h),
        Err(AuditError::StackUseAfterScope {
            id: h,
            op: Op::Retain
        })
    );
}

#[test]
fn every_access_after_the_scope_ended_is_stack_use_after_scope() {
    let mut heap = Heap::new();
    let s = heap.open_scope();
    let h = stack(&mut heap, s, Kind::Cell, vec![Value::Nil]);
    heap.end_scope(s).expect("end");
    let err = |op| AuditError::StackUseAfterScope { id: h, op };
    assert_eq!(heap.read(h, 0), Err(err(Op::Read)));
    assert_eq!(heap.write(h, 0, Value::Nil), Err(err(Op::Write)));
    assert_eq!(heap.retain(h), Err(err(Op::Retain)));
    assert_eq!(heap.release(h), Err(err(Op::Release)));
    assert_eq!(heap.weak(h), Err(err(Op::Weak)));
    assert_eq!(heap.mark_shared(h), Err(err(Op::Share)));
    assert_eq!(heap.count(h), Err(err(Op::Inspect)));
    assert_eq!(heap.write_unique(h, 0, Value::Nil), Err(err(Op::Write)));
    assert_eq!(
        heap.alloc(Kind::Immutable, vec![Value::Ref(h)]),
        Err(err(Op::Store))
    );
}

#[test]
fn a_stack_ref_stored_into_a_heap_object_is_an_error() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let a = atom(&mut heap, Value::Nil);
    let s = heap.open_scope();
    let h = stack(&mut heap, s, Kind::Immutable, vec![]);
    let err = AuditError::StackRefInHeap { id: h };
    assert_eq!(heap.alloc(Kind::Immutable, vec![Value::Ref(h)]), Err(err));
    assert_eq!(heap.write(c, 0, Value::Ref(h)), Err(err));
    assert_eq!(heap.write(a, 0, Value::Ref(h)), Err(err));
    let x = imm(&mut heap, vec![Value::Nil]);
    let place = cell(&mut heap, Value::Ref(x));
    heap.release(x).expect("the cell holds x");
    assert_eq!(heap.write_unique(place, 0, Value::Ref(h)), Err(err));
    assert_eq!(heap.read(x, 0), Ok(Value::Nil));
}

#[test]
fn a_stack_ref_into_an_outer_scope_object_is_an_error() {
    let mut heap = Heap::new();
    let outer = heap.open_scope();
    let o = stack(&mut heap, outer, Kind::Cell, vec![Value::Nil]);
    let inner = heap.open_scope();
    let i = stack(&mut heap, inner, Kind::Immutable, vec![]);
    assert_eq!(
        heap.write(o, 0, Value::Ref(i)),
        Err(AuditError::StackRefIntoOuterScope {
            id: i,
            scope: outer
        })
    );
    assert_eq!(
        heap.alloc_in_scope(outer, Kind::Immutable, vec![Value::Ref(i)]),
        Err(AuditError::StackRefIntoOuterScope {
            id: i,
            scope: outer
        })
    );
    // The other way round, and within one scope, is fine.
    stack(&mut heap, inner, Kind::Immutable, vec![Value::Ref(o)]);
    let same = stack(&mut heap, inner, Kind::Cell, vec![Value::Ref(i)]);
    heap.write(same, 0, Value::Ref(o))
        .expect("outer into inner");
    heap.end_scope(inner).expect("end inner");
    heap.end_scope(outer).expect("end outer");
    assert!(heap.finish().is_clean());
}

#[test]
fn a_weak_to_a_stack_object_is_an_error() {
    let mut heap = Heap::new();
    let s = heap.open_scope();
    let h = stack(&mut heap, s, Kind::Immutable, vec![]);
    let err = AuditError::WeakToStack { id: h };
    assert_eq!(heap.weak(h), Err(err));
    assert_eq!(heap.upgrade(h), Err(err));
    assert_eq!(heap.alloc(Kind::Immutable, vec![Value::Weak(h)]), Err(err));
}

#[test]
fn sharing_a_stack_object_is_an_error() {
    let mut heap = Heap::new();
    let s = heap.open_scope();
    let h = stack(&mut heap, s, Kind::Atom, vec![Value::Nil]);
    assert_eq!(heap.mark_shared(h), Err(AuditError::SharedStack { id: h }));
    assert_eq!(heap.is_shared(h), Ok(false));
}

#[test]
fn scopes_end_innermost_first_and_once() {
    let mut heap = Heap::new();
    let outer = heap.open_scope();
    let inner = heap.open_scope();
    assert_eq!(heap.open_scopes(), &[outer, inner]);
    assert_eq!(
        heap.end_scope(outer),
        Err(AuditError::ScopeNotInnermost {
            scope: outer,
            innermost: inner
        })
    );
    heap.end_scope(inner).expect("end inner");
    assert_eq!(
        heap.end_scope(inner),
        Err(AuditError::ScopeEnded { scope: inner })
    );
    assert_eq!(
        heap.alloc_in_scope(inner, Kind::Immutable, vec![]),
        Err(AuditError::ScopeEnded { scope: inner })
    );
    let unknown = ScopeId::from_index(9);
    assert_eq!(
        heap.end_scope(unknown),
        Err(AuditError::UnknownScope { scope: unknown })
    );
    assert_eq!(
        heap.alloc_in_scope(unknown, Kind::Immutable, vec![]),
        Err(AuditError::UnknownScope { scope: unknown })
    );
    heap.end_scope(outer).expect("end outer");
    assert!(heap.finish().is_clean());
}

#[test]
fn an_open_scope_at_finish_is_reported_and_its_objects_are_not_leaks() {
    let mut heap = Heap::new();
    let s = heap.open_scope();
    stack(&mut heap, s, Kind::Cell, vec![Value::Int(1)]);
    let report = heap.finish();
    assert_eq!(report.open_scopes, vec![s]);
    assert!(report.leaks.is_empty());
    assert!(!report.is_clean());
    assert!(!report.is_cycle_leak_only());
}

#[test]
fn a_drop_that_would_release_a_freed_object_ends_nothing() {
    let mut heap = Heap::new();
    let child = imm(&mut heap, vec![]);
    let s = heap.open_scope();
    let h = stack(&mut heap, s, Kind::Immutable, vec![Value::Ref(child)]);
    heap.release(child).expect("h holds child");
    heap.release(child)
        .expect("over-release frees child under h");
    let before = heap.trace().len();
    assert_eq!(
        heap.end_scope(s),
        Err(AuditError::ReleaseOfFreed { id: child })
    );
    assert_eq!(heap.trace().len(), before);
    assert!(heap.is_live(h));
    assert_eq!(heap.open_scopes(), &[s]);
}

#[test]
fn stack_errors_display_distinctly() {
    let id = ObjId::from_index(4);
    let after = AuditError::StackUseAfterScope { id, op: Op::Read }.to_string();
    let freed = AuditError::UseAfterFree { id, op: Op::Read }.to_string();
    assert!(after.contains("#4") && after.contains("scope"), "{after}");
    assert_ne!(after, freed);
    let scope = ScopeId::from_index(2);
    let text = AuditError::StackRefIntoOuterScope { id, scope }.to_string();
    assert!(text.contains("scope 2"), "{text}");
}
