//! Counting semantics (§2): storing retains, freeing releases what was
//! held exactly once, and freeing is iterative.

use super::{cell, imm};
use crate::heap::{Event, Heap, Value};

#[test]
fn alloc_retains_each_ref_field() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    let parent = imm(&mut heap, vec![Value::Ref(a), Value::Int(1), Value::Ref(a)]);
    assert_eq!(heap.count(a), Ok(3));
    assert_eq!(heap.count(parent), Ok(1));
}

#[test]
fn free_releases_children_exactly_once() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    let b = imm(&mut heap, vec![]);
    let parent = imm(&mut heap, vec![Value::Ref(a), Value::Ref(a), Value::Ref(b)]);
    assert_eq!((heap.count(a), heap.count(b)), (Ok(3), Ok(2)));
    assert_eq!(heap.release(parent), Ok(0));
    assert!(!heap.is_live(parent));
    assert_eq!((heap.count(a), heap.count(b)), (Ok(1), Ok(1)));
    let releases_of = |id| {
        heap.trace()
            .iter()
            .filter(|e| matches!(e, Event::Release { id: r, .. } if *r == id))
            .count()
    };
    assert_eq!((releases_of(a), releases_of(b)), (2, 1));
    assert_eq!(heap.release(a), Ok(0));
    assert_eq!(heap.release(b), Ok(0));
    assert!(heap.finish().is_clean());
}

#[test]
fn release_above_zero_does_not_free() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Int(9)]);
    assert_eq!(heap.retain(a), Ok(2));
    assert_eq!(heap.release(a), Ok(1));
    assert!(heap.is_live(a));
    assert_eq!(heap.read(a, 0), Ok(Value::Int(9)));
    assert!(!heap.trace().contains(&Event::Free { id: a }));
}

/// Builds a cons list of `len` nodes where each node is held only by
/// its successor, and returns the head.
fn cons_list(heap: &mut Heap, len: usize) -> crate::heap::ObjId {
    let mut head = imm(heap, vec![Value::Int(0), Value::Nil]);
    for i in 1..len {
        let next = imm(heap, vec![Value::Int(i as i64), Value::Ref(head)]);
        heap.release(head).expect("release previous head");
        head = next;
    }
    head
}

#[test]
fn a_long_list_frees_iteratively() {
    let mut heap = Heap::new();
    let len = 100_000;
    let head = cons_list(&mut heap, len);
    assert_eq!(heap.release(head), Ok(0));
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!(report.allocated, len);
    assert_eq!(report.freed, len);
    let frees = report
        .trace
        .iter()
        .filter(|e| matches!(e, Event::Free { .. }))
        .count();
    assert_eq!(frees, len);
}

#[test]
fn write_retains_new_and_releases_old() {
    let mut heap = Heap::new();
    let old = imm(&mut heap, vec![]);
    let new = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Ref(old));
    assert_eq!(heap.release(old), Ok(1)); // only the cell holds it now
    assert_eq!(heap.write(c, 0, Value::Ref(new)), Ok(()));
    assert!(!heap.is_live(old));
    assert_eq!(heap.count(new), Ok(2));
    assert_eq!(heap.read(c, 0), Ok(Value::Ref(new)));
}

#[test]
fn writing_the_value_already_held_keeps_it_alive() {
    let mut heap = Heap::new();
    let v = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Ref(v));
    assert_eq!(heap.release(v), Ok(1));
    assert_eq!(heap.write(c, 0, Value::Ref(v)), Ok(()));
    assert!(heap.is_live(v));
    assert_eq!(heap.count(v), Ok(1));
}

#[test]
fn releasing_a_cell_releases_its_slot() {
    let mut heap = Heap::new();
    let v = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Ref(v));
    assert_eq!(heap.count(v), Ok(2));
    assert_eq!(heap.release(c), Ok(0));
    assert_eq!(heap.count(v), Ok(1));
    assert_eq!(heap.release(v), Ok(0));
    assert!(heap.finish().is_clean());
}

#[test]
fn cascade_releases_in_field_order_depth_first() {
    let mut heap = Heap::new();
    let leaf = imm(&mut heap, vec![]);
    let left = imm(&mut heap, vec![Value::Ref(leaf)]);
    let right = imm(&mut heap, vec![]);
    let root = imm(&mut heap, vec![Value::Ref(left), Value::Ref(right)]);
    for held in [leaf, left, right] {
        heap.release(held).expect("drop caller's count");
    }
    let start = heap.trace().len();
    assert_eq!(heap.release(root), Ok(0));
    let order: Vec<Event> = heap.trace()[start..].to_vec();
    assert_eq!(
        order,
        vec![
            Event::Release {
                id: root,
                count_after: 0
            },
            Event::Free { id: root },
            Event::Release {
                id: left,
                count_after: 0
            },
            Event::Free { id: left },
            Event::Release {
                id: leaf,
                count_after: 0
            },
            Event::Free { id: leaf },
            Event::Release {
                id: right,
                count_after: 0
            },
            Event::Free { id: right },
        ]
    );
}
