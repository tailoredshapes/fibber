//! Shapes at scale that rounds 1 and 2 did not free or cross: a list
//! with weak back-pointers (the §6 use case for `weak`), a cycle
//! broken by a single write, and a thread crossing over a cycle. None
//! may recurse; every one runs on the ordinary test-thread stack.

use fibref::{Event, Heap, LeakClass, Value};

use crate::{atom, cell, chain, imm, release_all, shared_ids};

const LONG: usize = 200_000;

/// A doubly linked list of `len` nodes: node `i` is an immutable
/// `[i, Ref(node i-1), Ref(back i)]` where `back i` is a cell that,
/// once node `i+1` exists, holds `Weak(node i+1)`. Returns the head
/// (the last node) and every node in allocation order.
fn weak_backed_list(heap: &mut Heap, len: usize) -> (fibref::ObjId, Vec<fibref::ObjId>) {
    let mut nodes = Vec::with_capacity(len);
    let mut prev_node = Value::Nil;
    let mut prev_back = None;
    for i in 0..len {
        let back = cell(heap, Value::Nil);
        let node = imm(
            heap,
            vec![Value::Int(i as i64), prev_node, Value::Ref(back)],
        );
        heap.release(back).expect("the node holds the cell");
        if let Value::Ref(older) = prev_node {
            heap.release(older).expect("the node holds its predecessor");
        }
        if let Some(older_back) = prev_back {
            let w = heap.weak(node).expect("weak");
            heap.write(older_back, 0, w).expect("back pointer");
        }
        nodes.push(node);
        prev_node = Value::Ref(node);
        prev_back = Some(back);
    }
    (nodes[len - 1], nodes)
}

#[test]
fn a_200000_long_list_with_weak_back_pointers_frees_without_overflow_and_every_weak_dies() {
    let mut heap = Heap::new();
    let (head, nodes) = weak_backed_list(&mut heap, LONG);
    assert_eq!(heap.count(head), Ok(1));
    assert_eq!(heap.count(nodes[0]), Ok(1), "held by its successor only");
    assert_eq!(heap.release(head), Ok(0));
    let frees = heap
        .trace()
        .iter()
        .filter(|e| matches!(e, Event::Free { .. }))
        .count();
    assert_eq!(frees, 2 * LONG, "every node and every back cell");
    for &node in nodes.iter().step_by(LONG / 16) {
        assert!(!heap.is_live(node));
        assert_eq!(heap.upgrade(node), Ok(None), "{node} came back");
    }
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!((report.allocated, report.freed), (2 * LONG, 2 * LONG));
}

#[test]
fn a_weak_back_pointer_in_a_long_list_upgrades_while_the_list_lives() {
    let mut heap = Heap::new();
    let (head, nodes) = weak_backed_list(&mut heap, 1000);
    let back = heap.read(nodes[499], 2).expect("the back cell");
    let Value::Ref(back) = back else {
        panic!("field 2 is the back cell, got {back:?}");
    };
    let w = heap.read(back, 0).expect("the weak");
    assert_eq!(w, Value::Weak(nodes[500]));
    assert_eq!(heap.upgrade(nodes[500]), Ok(Some(nodes[500])));
    assert_eq!(heap.count(nodes[500]), Ok(2), "upgrade retained");
    heap.release(nodes[500]).expect("the upgraded count");
    heap.release(head).expect("frees the list");
    assert_eq!(heap.upgrade(nodes[500]), Ok(None));
    assert!(heap.finish().is_clean());
}

#[test]
fn breaking_a_200000_long_cycle_with_one_write_frees_it_all_without_overflow() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let head = chain(&mut heap, LONG, Value::Ref(c));
    heap.write(c, 0, Value::Ref(head)).expect("close the cycle");
    release_all(&mut heap, &[head, c]);
    assert_eq!((heap.count(c), heap.count(head)), (Ok(1), Ok(1)));
    // set! c nil: the cascade runs the whole chain and ends at c
    // itself, which the write is in the middle of.
    assert_eq!(heap.write(c, 0, Value::Nil), Ok(()));
    assert!(!heap.is_live(head));
    assert!(!heap.is_live(c), "the chain's tail held c's last count");
    let report = heap.finish();
    assert!(report.is_clean(), "{} leaks", report.leaks.len());
    assert_eq!(report.freed, LONG + 1);
}

#[test]
fn a_thread_crossing_over_a_200000_long_cycle_through_an_atom_terminates_and_marks_once() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    let head = chain(&mut heap, LONG, Value::Ref(a));
    heap.write(a, 0, Value::Ref(head)).expect("close the cycle");
    release_all(&mut heap, &[head, a]);
    heap.mark_shared(a).expect("an atom may cross");
    let marked = shared_ids(heap.trace());
    assert_eq!(marked.len(), LONG + 1, "each object marked exactly once");
    assert_eq!(marked[0], a);
    assert_eq!(heap.is_shared(head), Ok(true));
    heap.mark_shared(head).expect("already shared");
    assert_eq!(
        shared_ids(heap.trace()).len(),
        LONG + 1,
        "a second crossing marks nothing"
    );
    let report = heap.finish();
    assert_eq!(report.leaks.len(), LONG + 1);
    assert!(report.leaks.iter().all(|l| l.class == LeakClass::LeakCycle));
    assert!(report.is_cycle_leak_only());
}
