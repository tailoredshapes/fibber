//! Invariants of the end-of-run report: its numbers must agree with
//! its trace and with each other, its dangling list must be complete
//! and ordered as documented, and `held` must count every `Ref` field
//! naming an object, duplicates included. A report whose parts
//! disagree cannot be compared against the compiler (`spec/method.md`
//! rule 6).

use fibref::{Event, Heap, Kind, LeakClass, Value};

use crate::{atom, cell, cell_cycle, imm, release_all};

#[test]
fn allocated_freed_and_leaks_agree_with_each_other_and_with_the_trace() {
    let mut heap = Heap::new();
    let (c, v) = cell_cycle(&mut heap);
    let dead = imm(&mut heap, vec![Value::Int(1)]);
    let gone_with_parent = imm(&mut heap, vec![]);
    let parent = imm(&mut heap, vec![Value::Ref(gone_with_parent)]);
    let forgotten = atom(&mut heap, Value::Ref(dead));
    release_all(&mut heap, &[dead, gone_with_parent, parent]);
    let report = heap.finish();
    let allocs = report
        .trace
        .iter()
        .filter(|e| matches!(e, Event::Alloc { .. }))
        .count();
    let frees = report
        .trace
        .iter()
        .filter(|e| matches!(e, Event::Free { .. }))
        .count();
    assert_eq!(
        report.allocated, allocs,
        "allocated disagrees with the trace"
    );
    assert_eq!(report.freed, frees, "freed disagrees with the trace");
    assert_eq!(
        report.allocated - report.freed,
        report.leaks.len(),
        "the leaks are not exactly the unfreed objects"
    );
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![dead, forgotten]);
    assert!(report.dangling.is_empty());
}

#[test]
fn an_empty_run_and_a_fully_freed_run_are_both_clean() {
    let empty = Heap::new().finish();
    assert!(empty.is_clean());
    assert!(
        !empty.is_cycle_leak_only(),
        "nothing leaked, so not a cycle leak"
    );
    assert_eq!((empty.allocated, empty.freed), (0, 0));
    assert!(empty.trace.is_empty());

    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    let b = cell(&mut heap, Value::Ref(a));
    release_all(&mut heap, &[a, b]);
    let report = heap.finish();
    assert!(report.is_clean());
    assert!(!report.is_cycle_leak_only());
    assert_eq!((report.allocated, report.freed), (2, 2));
    assert!(report.bugs().next().is_none());
}

#[test]
fn dangling_refs_are_listed_by_holder_then_field_and_all_of_them() {
    // Two holders, three dangling fields, freed in an order unrelated
    // to the report's documented order (holder allocation, then field).
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let y = imm(&mut heap, vec![]);
    let h1 = imm(&mut heap, vec![Value::Ref(y), Value::Int(0), Value::Ref(x)]);
    let h2 = imm(&mut heap, vec![Value::Ref(x)]);
    release_all(&mut heap, &[x, y]);
    assert_eq!((heap.count(x), heap.count(y)), (Ok(2), Ok(1)));
    assert_eq!(heap.release(x), Ok(1), "one too many");
    assert_eq!(
        heap.release(x),
        Ok(0),
        "two too many: x is freed under h1 and h2"
    );
    assert_eq!(heap.release(y), Ok(0), "one too many: y is freed under h1");
    let report = heap.finish();
    let listed: Vec<(fibref::ObjId, usize, fibref::ObjId)> = report
        .dangling
        .iter()
        .map(|d| (d.holder, d.field, d.target))
        .collect();
    assert_eq!(listed, vec![(h1, 0, y), (h1, 2, x), (h2, 0, x)]);
    assert!(!report.is_clean());
    assert!(!report.is_cycle_leak_only());
    assert_eq!(report.ids_in(LeakClass::Leak), vec![h1, h2]);
    assert_eq!(report.freed, 2);
}

#[test]
fn held_counts_duplicate_ref_fields_and_ignores_weak_fields() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let w = heap.weak(x).expect("weak");
    let a = imm(&mut heap, vec![Value::Ref(x), w, Value::Ref(x)]);
    let b = cell(&mut heap, w);
    heap.release(x).expect("the binding's count");
    assert_eq!(heap.count(x), Ok(2));
    let report = heap.finish();
    let leak_x = report.leaks.iter().find(|l| l.id == x).expect("x leaks");
    assert_eq!((leak_x.count, leak_x.held), (2, 2));
    assert_eq!(leak_x.kind, Kind::Immutable);
    let leak_a = report.leaks.iter().find(|l| l.id == a).expect("a leaks");
    assert_eq!((leak_a.count, leak_a.held), (1, 0));
    let leak_b = report.leaks.iter().find(|l| l.id == b).expect("b leaks");
    assert_eq!((leak_b.count, leak_b.held, leak_b.kind), (1, 0, Kind::Cell));
    assert!(report.leaks.iter().all(|l| l.class == LeakClass::Leak));
}

#[test]
fn a_cycle_member_held_also_by_a_leaked_outsider_is_reported_beside_the_outsider() {
    // c -> v -> c is permitted. A leaked holder h -> v (its binding
    // never released) is not. Both v (count 2, held 2) and h must be
    // in the report, h as a bug; the run is not "cycle leak only".
    let mut heap = Heap::new();
    let (c, v) = cell_cycle(&mut heap);
    heap.retain(v).expect("a binding on v");
    let h = imm(&mut heap, vec![Value::Ref(v)]);
    heap.release(v).expect("that binding goes");
    assert_eq!(heap.count(v), Ok(2));
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c, v]);
    assert_eq!(report.ids_in(LeakClass::Leak), vec![h]);
    assert!(!report.is_cycle_leak_only());
    assert_eq!(report.bugs().map(|l| l.id).collect::<Vec<_>>(), vec![h]);
}
