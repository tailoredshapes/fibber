//! Adversarial tests for unique writes (`spec/types.md` §6.6, §8.2),
//! through the heap's public API only. Each test tries to make
//! `write_unique` change, in place, an object that another binding,
//! another thread, static data or a stack frame can see, or to tie a
//! cycle through no cell. A failing test here is a finding about
//! `crates/fibref/src/heap`, not about the test.

use fibref::{AuditError, Event, Heap, Kind, LeakClass, ObjId, Uniqueness, Value};

fn imm(heap: &mut Heap, fields: Vec<Value>) -> ObjId {
    heap.alloc(Kind::Immutable, fields)
        .expect("alloc immutable")
}

fn cell(heap: &mut Heap, value: Value) -> ObjId {
    heap.alloc(Kind::Cell, vec![value]).expect("alloc cell")
}

/// A cell alone holding a fresh immutable object with `fields`.
fn place(heap: &mut Heap, fields: Vec<Value>) -> (ObjId, ObjId) {
    let x = imm(heap, fields);
    let c = cell(heap, Value::Ref(x));
    heap.release(x).expect("the cell holds it now");
    (c, x)
}

fn refused(id: ObjId, why: Uniqueness) -> Result<ObjId, AuditError> {
    Err(AuditError::NotUnique { id, why })
}

/// A refused write must change nothing: no event, the field as it was,
/// and the new value's count as it was. The probe value is freed after.
fn assert_refused_cleanly(heap: &mut Heap, c: ObjId, x: ObjId, expected: AuditError) {
    let new = imm(heap, vec![]);
    let old = heap.read(x, 0).expect("read");
    let before = heap.trace().len();
    assert_eq!(heap.write_unique(c, 0, Value::Ref(new)), Err(expected));
    assert_eq!(heap.trace().len(), before);
    assert_eq!(heap.read(x, 0), Ok(old));
    assert_eq!(heap.count(new), Ok(1));
    heap.release(new).expect("the probe goes");
}

#[test]
fn two_variables_holding_one_object_see_no_in_place_write() {
    let mut heap = Heap::new();
    let (c1, x) = place(&mut heap, vec![Value::Int(1)]);
    let c2 = cell(&mut heap, Value::Ref(x));
    let why = Uniqueness::Count(2);
    assert_refused_cleanly(&mut heap, c1, x, AuditError::NotUnique { id: x, why });
    assert_refused_cleanly(&mut heap, c2, x, AuditError::NotUnique { id: x, why });
}

#[test]
fn a_temporary_count_on_the_object_blocks_the_write() {
    // (set-field! &c f @c): the argument's acquire makes it count 2.
    let mut heap = Heap::new();
    let (c, x) = place(&mut heap, vec![Value::Int(1)]);
    heap.retain(x).expect("@c acquires");
    assert_eq!(
        heap.write_unique(c, 0, Value::Ref(x)),
        refused(x, Uniqueness::Count(2))
    );
}

#[test]
fn an_object_another_thread_can_upgrade_to_is_shared_and_refused() {
    // Only the cell counts x, but a weak to it crossed in an atom: the
    // other thread can upgrade it and read x while this one writes.
    let mut heap = Heap::new();
    let (c, x) = place(&mut heap, vec![Value::Int(1)]);
    let a = heap.alloc(Kind::Atom, vec![Value::Nil]).expect("atom");
    heap.mark_shared(a).expect("the atom crosses");
    let w = heap.weak(x).expect("weak");
    heap.write(a, 0, w).expect("reset! with the weak");
    assert_eq!(heap.count(x), Ok(1));
    assert_eq!(heap.is_shared(x), Ok(true));
    let why = Uniqueness::Shared;
    assert_refused_cleanly(&mut heap, c, x, AuditError::NotUnique { id: x, why });
}

#[test]
fn a_literal_and_its_immortal_tail_are_never_written() {
    let mut heap = Heap::new();
    let tail = heap
        .alloc_immortal(Kind::Immutable, vec![Value::Int(2)])
        .expect("tail");
    let lit = heap
        .alloc_immortal(Kind::Immutable, vec![Value::Ref(tail)])
        .expect("literal");
    // (match f ((List xs) xs)) acquired into a private cell.
    let c = cell(&mut heap, Value::Ref(tail));
    let why = Uniqueness::Immortal;
    assert_refused_cleanly(&mut heap, c, tail, AuditError::NotUnique { id: tail, why });
    let c2 = cell(&mut heap, Value::Ref(lit));
    assert_refused_cleanly(&mut heap, c2, lit, AuditError::NotUnique { id: lit, why });
    assert_eq!(heap.count(lit), Ok(0), "no retain ever reached a literal");
}

#[test]
fn a_stack_object_is_never_written_in_place() {
    let mut heap = Heap::new();
    let s = heap.open_scope();
    let x = heap
        .alloc_in_scope(s, Kind::Immutable, vec![Value::Nil])
        .expect("scope-local");
    let c = heap
        .alloc_in_scope(s, Kind::Cell, vec![Value::Ref(x)])
        .expect("stack cell");
    let why = Uniqueness::Stack;
    assert_refused_cleanly(&mut heap, c, x, AuditError::NotUnique { id: x, why });
}

#[test]
fn a_private_stack_cell_may_write_its_unique_heap_content() {
    // An & parameter's private cell is a stack cell of the call; the
    // copy it holds is a heap object held by it alone.
    let mut heap = Heap::new();
    let (_, copy) = place(&mut heap, vec![Value::Int(1)]);
    let s = heap.open_scope();
    let p = heap
        .alloc_in_scope(s, Kind::Cell, vec![Value::Ref(copy)])
        .expect("private cell");
    assert_eq!(heap.count(copy), Ok(2), "the variable and the private cell");
    let why = Uniqueness::Count(2);
    assert_refused_cleanly(&mut heap, p, copy, AuditError::NotUnique { id: copy, why });
    let fresh = imm(&mut heap, vec![Value::Int(1)]);
    heap.write(p, 0, Value::Ref(fresh))
        .expect("copy on first update");
    heap.release(fresh).expect("the private cell holds it");
    assert_eq!(heap.write_unique(p, 0, Value::Int(9)), Ok(fresh));
    let stack_value = heap
        .alloc_in_scope(s, Kind::Immutable, vec![])
        .expect("a stack object");
    assert_eq!(
        heap.write_unique(p, 0, Value::Ref(stack_value)),
        Err(AuditError::StackRefInHeap { id: stack_value })
    );
}

#[test]
fn an_object_on_a_cell_cycle_with_a_second_holder_is_refused() {
    // c -> x -> y -> c, and y also holds x: x has count 2.
    let mut heap = Heap::new();
    let (c, x) = place(&mut heap, vec![Value::Nil]);
    let y = imm(&mut heap, vec![Value::Ref(x), Value::Ref(c)]);
    let why = Uniqueness::Count(2);
    assert_refused_cleanly(&mut heap, c, x, AuditError::NotUnique { id: x, why });
    heap.release(y).expect("y goes");
    assert_eq!(heap.write_unique(c, 0, Value::Ref(c)), Ok(x));
    heap.release(c).expect("the let ends");
    let report = heap.finish();
    assert!(report.is_cycle_leak_only(), "{:?}", report.leaks);
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![x, c]);
}

#[test]
fn an_object_held_only_through_the_place_on_a_cycle_is_written_and_stays_a_cell_cycle() {
    // c -> x -> c: x is on a cycle, but only through its own place.
    let mut heap = Heap::new();
    let (c, x) = place(&mut heap, vec![Value::Nil, Value::Nil]);
    assert_eq!(heap.write_unique(c, 0, Value::Ref(c)), Ok(x));
    assert_eq!(heap.write_unique(c, 1, Value::Int(5)), Ok(x));
    let other = imm(&mut heap, vec![]);
    assert_eq!(heap.write_unique(c, 1, Value::Ref(other)), Ok(x));
    heap.release(other).expect("x holds it");
    heap.release(c).expect("the let ends");
    let report = heap.finish();
    assert!(report.is_cycle_leak_only(), "{:?}", report.leaks);
    assert!(report.ids_in(LeakClass::ImmutableCycle).is_empty());
}

#[test]
fn no_legal_sequence_of_unique_writes_ties_an_immutable_cycle() {
    // Try every pair of two immutable objects in cells writing each
    // other and themselves; each attempt either is refused or passes
    // through a cell.
    let mut heap = Heap::new();
    let (ca, a) = place(&mut heap, vec![Value::Nil]);
    let (cb, b) = place(&mut heap, vec![Value::Nil]);
    assert_eq!(heap.write_unique(ca, 0, Value::Ref(b)), Ok(a));
    // b is now held by cb and a: count 2, so b -> a is refused.
    assert_eq!(
        heap.write_unique(cb, 0, Value::Ref(a)),
        refused(b, Uniqueness::Count(2))
    );
    assert_eq!(
        heap.write_unique(ca, 0, Value::Ref(a)),
        refused(a, Uniqueness::StoresItself)
    );
    heap.release(ca).expect("free ca, a");
    heap.release(cb).expect("free cb, b");
    assert!(heap.finish().is_clean());
}

#[test]
fn an_over_release_that_fakes_a_count_of_one_is_still_caught_by_the_audit() {
    // A deficit: the caller releases x once too often, so x reads count
    // 1 while the cell and z both hold it. The unique write then ties
    // x -> z -> x through no cell. The heap cannot see whose count is
    // missing at the write, but the end-of-run audit must not pass it.
    let mut heap = Heap::new();
    let (c, x) = place(&mut heap, vec![Value::Nil]);
    let z = imm(&mut heap, vec![Value::Ref(x)]);
    heap.release(x).expect("the over-release");
    assert_eq!(heap.write_unique(c, 0, Value::Ref(z)), Ok(x));
    heap.release(z).expect("the caller lets go of z");
    // Ending the let would free c, then x, then z, then reach x again.
    assert_eq!(heap.release(c), Err(AuditError::ReleaseOfFreed { id: x }));
    let report = heap.finish();
    assert!(!report.is_clean());
    assert!(!report.is_cycle_leak_only());
    assert!(report.ids_in(LeakClass::ImmutableCycle).contains(&x));
}

#[test]
fn a_freed_place_or_content_is_use_after_free() {
    let mut heap = Heap::new();
    let (c, x) = place(&mut heap, vec![Value::Nil]);
    heap.release(x).expect("over-release frees x under c");
    assert_eq!(
        heap.write_unique(c, 0, Value::Int(1)),
        Err(AuditError::UseAfterFree {
            id: x,
            op: fibref::Op::Write
        })
    );
    let (c2, _) = place(&mut heap, vec![Value::Nil]);
    heap.release(c2).expect("free c2");
    assert_eq!(
        heap.write_unique(c2, 0, Value::Int(1)),
        Err(AuditError::UseAfterFree {
            id: c2,
            op: fibref::Op::Write
        })
    );
}

#[test]
fn storing_a_literal_by_unique_write_retains_nothing() {
    let mut heap = Heap::new();
    let lit = heap.alloc_immortal(Kind::Immutable, vec![]).expect("lit");
    let (c, x) = place(&mut heap, vec![Value::Nil]);
    let start = heap.trace().len();
    assert_eq!(heap.write_unique(c, 0, Value::Ref(lit)), Ok(x));
    assert_eq!(
        heap.trace()[start..].to_vec(),
        vec![Event::WriteUnique {
            place: c,
            id: x,
            field: 0
        }]
    );
    heap.release(c).expect("free");
    assert!(heap.finish().is_clean());
}
