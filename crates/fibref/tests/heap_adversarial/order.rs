//! Orderings the compiler will be compared against (`spec/method.md`
//! rule 6), and identities that must hold at scale.

use std::collections::HashSet;

use fibref::{Event, Heap, Kind, LeakClass, Value};

use crate::{atom, cell, chain, imm, release_all, shared_ids};

#[test]
fn writing_a_ref_that_only_the_old_value_kept_alive_keeps_it_alive() {
    // c holds v, v holds y, and the bindings on v and y are gone. set!
    // c to y: the new value must be retained before the old one is
    // released, or y is freed under the write.
    let mut heap = Heap::new();
    let y = imm(&mut heap, vec![Value::Int(1)]);
    let v = imm(&mut heap, vec![Value::Ref(y)]);
    let c = cell(&mut heap, Value::Ref(v));
    release_all(&mut heap, &[y, v]);
    assert_eq!(heap.count(y), Ok(1));
    let before = heap.trace().len();
    heap.write(c, 0, Value::Ref(y)).expect("set! y");
    assert!(heap.is_live(y));
    assert!(!heap.is_live(v));
    assert_eq!(heap.count(y), Ok(1));
    assert_eq!(
        &heap.trace()[before..],
        &[
            Event::Retain {
                id: y,
                count_after: 2
            },
            Event::Write { id: c, field: 0 },
            Event::Release {
                id: v,
                count_after: 0
            },
            Event::Free { id: v },
            Event::Release {
                id: y,
                count_after: 1
            },
        ]
    );
    assert_eq!(heap.read(c, 0), Ok(Value::Ref(y)));
    heap.release(c).expect("scope ends");
    assert!(heap.finish().is_clean());
}

#[test]
fn alloc_traces_one_retain_per_ref_field_in_field_order() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    let b = imm(&mut heap, vec![]);
    let w = heap.weak(a).expect("weak");
    let before = heap.trace().len();
    let v = heap
        .alloc(
            Kind::Immutable,
            vec![
                w,
                Value::Ref(b),
                Value::Int(0),
                Value::Ref(a),
                Value::Ref(b),
            ],
        )
        .expect("alloc");
    assert_eq!(
        &heap.trace()[before..],
        &[
            Event::Alloc {
                id: v,
                kind: Kind::Immutable
            },
            Event::Retain {
                id: b,
                count_after: 2
            },
            Event::Retain {
                id: a,
                count_after: 2
            },
            Event::Retain {
                id: b,
                count_after: 3
            },
        ]
    );
    assert_eq!((heap.count(a), heap.count(b)), (Ok(2), Ok(3)));
}

#[test]
fn mark_shared_traces_depth_first_in_field_order() {
    let mut heap = Heap::new();
    let leaf1 = imm(&mut heap, vec![]);
    let leaf2 = imm(&mut heap, vec![]);
    let left = imm(&mut heap, vec![Value::Ref(leaf1), Value::Ref(leaf2)]);
    let right = atom(&mut heap, Value::Ref(leaf2));
    let root = imm(&mut heap, vec![Value::Ref(left), Value::Ref(right)]);
    heap.mark_shared(root).expect("cross");
    assert_eq!(
        shared_ids(heap.trace()),
        vec![root, left, leaf1, leaf2, right],
        "one Shared per object, depth first, in field order"
    );
}

#[test]
fn write_into_a_shared_atom_traces_shared_before_retain_and_write() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    heap.mark_shared(a).expect("cross");
    let y = imm(&mut heap, vec![]);
    let x = imm(&mut heap, vec![Value::Ref(y)]);
    let before = heap.trace().len();
    heap.write(a, 0, Value::Ref(x)).expect("reset!");
    assert_eq!(
        &heap.trace()[before..],
        &[
            Event::Shared { id: x },
            Event::Shared { id: y },
            Event::Retain {
                id: x,
                count_after: 2
            },
            Event::Write { id: a, field: 0 },
        ]
    );
}

#[test]
fn ids_stay_unique_across_interleaved_alloc_and_free() {
    let mut heap = Heap::new();
    let mut seen: HashSet<fibref::ObjId> = HashSet::new();
    let mut freed = Vec::new();
    for round in 0..2000 {
        let keep = imm(&mut heap, vec![Value::Int(round)]);
        let drop = imm(&mut heap, vec![Value::Ref(keep)]);
        assert!(seen.insert(keep), "id {keep} handed out twice");
        assert!(seen.insert(drop), "id {drop} handed out twice");
        heap.weak(drop).expect("weak");
        heap.release(drop).expect("free");
        freed.push(drop);
        heap.release(keep).expect("free");
        freed.push(keep);
    }
    for id in &freed {
        assert_eq!(heap.upgrade(*id), Ok(None), "{id} came back to life");
        assert!(!heap.is_live(*id));
    }
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!((report.allocated, report.freed), (4000, 4000));
}

#[test]
fn a_200000_long_chain_of_cells_frees_without_stack_overflow() {
    let mut heap = Heap::new();
    let mut head = cell(&mut heap, Value::Nil);
    for _ in 1..200_000 {
        let next = cell(&mut heap, Value::Ref(head));
        heap.release(head).expect("predecessor holds it");
        head = next;
    }
    assert_eq!(heap.release(head), Ok(0));
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!(report.freed, 200_000);
}

#[test]
fn a_200000_long_tail_off_a_cycle_is_leak_cycle_without_overflow() {
    let mut heap = Heap::new();
    let tail = chain(&mut heap, 200_000, Value::Nil);
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c), Value::Ref(tail)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    release_all(&mut heap, &[tail, v, c]);
    let report = heap.finish();
    assert_eq!(report.leaks.len(), 200_002);
    assert!(report.leaks.iter().all(|l| l.class == LeakClass::LeakCycle));
    assert!(report.is_cycle_leak_only());
}

#[test]
fn a_200000_long_chain_is_freed_by_the_last_upgraded_count_going_away() {
    let mut heap = Heap::new();
    let head = chain(&mut heap, 200_000, Value::Nil);
    heap.weak(head).expect("weak");
    assert_eq!(heap.upgrade(head), Ok(Some(head)));
    heap.release(head).expect("the binding's count");
    assert!(heap.is_live(head), "the upgraded count keeps it");
    assert_eq!(heap.release(head), Ok(0));
    assert_eq!(heap.upgrade(head), Ok(None));
    assert!(heap.finish().is_clean());
}
