//! Release cascades: a freed object releases what it held exactly
//! once, in field order, depth first, and never through recursion.

use fibref::{Event, Heap, Value};

use crate::{cell, frees_of, imm, release_all, releases_of};

#[test]
fn free_releases_each_child_exactly_once() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![Value::Int(1)]);
    let parent = imm(&mut heap, vec![Value::Ref(a), Value::Int(2)]);
    assert_eq!(heap.count(a), Ok(2));
    heap.release(parent).expect("frees parent only");
    assert!(!heap.is_live(parent));
    assert!(heap.is_live(a));
    assert_eq!(heap.count(a), Ok(1), "released once by the cascade");
    assert_eq!(releases_of(heap.trace(), a), 1);
    assert_eq!(frees_of(heap.trace(), a), 0);
}

#[test]
fn a_child_held_twice_by_one_parent_is_released_twice() {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    let parent = imm(&mut heap, vec![Value::Ref(a), Value::Ref(a)]);
    assert_eq!(heap.count(a), Ok(3));
    heap.release(a).expect("caller's count");
    heap.release(parent).expect("frees parent and then a");
    assert!(!heap.is_live(a));
    assert_eq!(releases_of(heap.trace(), a), 3);
    assert_eq!(frees_of(heap.trace(), a), 1);
}

fn two_parents_freed_in_order(first_p1: bool) {
    let mut heap = Heap::new();
    let a = imm(&mut heap, vec![]);
    let p1 = imm(&mut heap, vec![Value::Ref(a)]);
    let p2 = imm(&mut heap, vec![Value::Int(0), Value::Ref(a)]);
    assert_eq!(heap.count(a), Ok(3));
    heap.release(a).expect("caller's count");
    let (first, second) = if first_p1 { (p1, p2) } else { (p2, p1) };
    heap.release(first).expect("first parent");
    assert!(heap.is_live(a));
    assert_eq!(heap.count(a), Ok(1));
    heap.release(second).expect("second parent");
    assert!(!heap.is_live(a));
    assert_eq!(releases_of(heap.trace(), a), 3);
    assert_eq!(frees_of(heap.trace(), a), 1);
}

#[test]
fn child_released_once_per_parent_when_first_parent_goes_first() {
    two_parents_freed_in_order(true);
}

#[test]
fn child_released_once_per_parent_when_second_parent_goes_first() {
    two_parents_freed_in_order(false);
}

#[test]
fn diamond_frees_the_shared_leaf_once_depth_first() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let l = imm(&mut heap, vec![Value::Ref(x)]);
    let r = imm(&mut heap, vec![Value::Ref(x)]);
    let p = imm(&mut heap, vec![Value::Ref(l), Value::Ref(r)]);
    release_all(&mut heap, &[x, l, r]);
    let before = heap.trace().len();
    assert_eq!(heap.release(p), Ok(0));
    assert_eq!(
        &heap.trace()[before..],
        &[
            Event::Release {
                id: p,
                count_after: 0
            },
            Event::Free { id: p },
            Event::Release {
                id: l,
                count_after: 0
            },
            Event::Free { id: l },
            Event::Release {
                id: x,
                count_after: 1
            },
            Event::Release {
                id: r,
                count_after: 0
            },
            Event::Free { id: r },
            Event::Release {
                id: x,
                count_after: 0
            },
            Event::Free { id: x },
        ]
    );
    for id in [p, l, r, x] {
        assert!(!heap.is_live(id));
        assert_eq!(frees_of(heap.trace(), id), 1);
    }
}

#[test]
fn release_above_zero_frees_nothing() {
    let mut heap = Heap::new();
    let child = imm(&mut heap, vec![]);
    let a = imm(&mut heap, vec![Value::Ref(child)]);
    assert_eq!(heap.retain(a), Ok(2));
    assert_eq!(heap.release(a), Ok(1));
    assert!(heap.is_live(a));
    assert_eq!(heap.count(child), Ok(2), "children untouched");
    assert_eq!(frees_of(heap.trace(), a), 0);
    assert_eq!(releases_of(heap.trace(), child), 0);
}

#[test]
fn writing_over_a_ref_releases_it_and_cascades() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let v = imm(&mut heap, vec![Value::Ref(x)]);
    let c = cell(&mut heap, Value::Ref(v));
    release_all(&mut heap, &[x, v]);
    assert_eq!(heap.count(v), Ok(1));
    heap.write(c, 0, Value::Nil).expect("set! nil");
    assert!(!heap.is_live(v));
    assert!(!heap.is_live(x));
    assert_eq!(frees_of(heap.trace(), v), 1);
    assert_eq!(frees_of(heap.trace(), x), 1);
    assert!(
        heap.read(v, 0).is_err(),
        "stale ref out of the cell is dead"
    );
    heap.release(c).expect("scope ends");
    assert!(heap.finish().is_clean());
}

#[test]
fn writing_the_same_ref_back_neither_frees_nor_leaks() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let c = cell(&mut heap, Value::Ref(x));
    heap.release(x).expect("caller's count");
    assert_eq!(heap.count(x), Ok(1));
    heap.write(c, 0, Value::Ref(x)).expect("set! same value");
    assert!(heap.is_live(x));
    assert_eq!(heap.count(x), Ok(1));
    heap.release(c).expect("scope ends");
    assert!(!heap.is_live(x));
    assert!(heap.finish().is_clean());
}

#[test]
fn a_cell_holding_itself_frees_once_the_cycle_is_broken() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    heap.write(c, 0, Value::Ref(c)).expect("self reference");
    assert_eq!(heap.count(c), Ok(2));
    heap.write(c, 0, Value::Nil).expect("break the cycle");
    assert_eq!(heap.count(c), Ok(1));
    assert_eq!(heap.release(c), Ok(0));
    assert!(!heap.is_live(c));
    assert_eq!(frees_of(heap.trace(), c), 1);
    assert!(heap.finish().is_clean());
}

#[test]
fn freeing_a_holder_of_a_weak_does_not_release_the_target() {
    let mut heap = Heap::new();
    let t = imm(&mut heap, vec![]);
    let w = heap.weak(t).expect("weak");
    let holder = imm(&mut heap, vec![w, Value::Int(1)]);
    heap.release(holder).expect("frees holder");
    assert!(heap.is_live(t));
    assert_eq!(heap.count(t), Ok(1));
    assert_eq!(releases_of(heap.trace(), t), 0);
}

#[test]
fn cascade_releases_in_field_order_depth_first() {
    let mut heap = Heap::new();
    let x = imm(&mut heap, vec![]);
    let a = imm(&mut heap, vec![Value::Ref(x)]);
    let b = imm(&mut heap, vec![]);
    let v = imm(&mut heap, vec![Value::Ref(a), Value::Ref(b)]);
    release_all(&mut heap, &[x, a, b]);
    let before = heap.trace().len();
    heap.release(v).expect("frees everything");
    let order: Vec<_> = heap.trace()[before..]
        .iter()
        .filter_map(|e| match e {
            Event::Free { id } => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(order, vec![v, a, x, b]);
}
