//! Writes whose own release cascade reaches the cell being written.
//!
//! Under §2 a `set!` retains the new value, stores it, then releases
//! the old one. When the old value was all that kept the cell alive
//! (a cycle held by nothing else), the release frees the cell in the
//! middle of the write. The heap must finish the write consistently:
//! the new value ends up with exactly the counts §2 gives it, the
//! cell's own slot is released once, and the trace says so in order.
//! A heap that reads the cell after the cascade, or applies the
//! cascade before the store, gets a different count or a panic here.

use fibref::{Event, Heap, LeakClass, Value};

use crate::{atom, cell, cell_cycle, frees_of, imm, release_all, releases_of};

#[test]
fn set_nil_on_a_self_holding_cell_with_no_other_owner_frees_it_during_the_write() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    heap.write(c, 0, Value::Ref(c)).expect("self reference");
    heap.release(c).expect("the binding's count");
    assert_eq!(heap.count(c), Ok(1), "only its own slot holds it");
    let before = heap.trace().len();
    assert_eq!(heap.write(c, 0, Value::Nil), Ok(()));
    assert!(!heap.is_live(c), "the cell's last count was its own slot");
    assert_eq!(
        &heap.trace()[before..],
        &[
            Event::Write { id: c, field: 0 },
            Event::Release {
                id: c,
                count_after: 0
            },
            Event::Free { id: c },
        ]
    );
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!(report.freed, 1);
}

#[test]
fn breaking_a_cycle_by_writing_a_fresh_value_frees_the_cell_under_the_write() {
    // c -> v -> c, held by nothing else. set! c to y: y is retained,
    // stored, then v is released, which frees v, then c, whose slot now
    // holds y, so y goes back to the binding's one count.
    let mut heap = Heap::new();
    let (c, v) = cell_cycle(&mut heap);
    let y = imm(&mut heap, vec![Value::Int(9)]);
    let before = heap.trace().len();
    assert_eq!(heap.write(c, 0, Value::Ref(y)), Ok(()));
    assert!(!heap.is_live(v));
    assert!(!heap.is_live(c));
    assert!(heap.is_live(y));
    assert_eq!(heap.count(y), Ok(1), "the freed cell gave y's count back");
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
                id: c,
                count_after: 0
            },
            Event::Free { id: c },
            Event::Release {
                id: y,
                count_after: 1
            },
        ]
    );
    heap.release(y).expect("scope ends");
    let report = heap.finish();
    assert!(report.is_clean());
    assert_eq!(report.freed, 3);
}

#[test]
fn writing_a_self_holding_cell_into_itself_keeps_its_count_at_one() {
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    heap.write(c, 0, Value::Ref(c)).expect("self reference");
    heap.release(c).expect("the binding's count");
    let before = heap.trace().len();
    assert_eq!(heap.write(c, 0, Value::Ref(c)), Ok(()));
    assert!(heap.is_live(c));
    assert_eq!(heap.count(c), Ok(1));
    assert_eq!(
        &heap.trace()[before..],
        &[
            Event::Retain {
                id: c,
                count_after: 2
            },
            Event::Write { id: c, field: 0 },
            Event::Release {
                id: c,
                count_after: 1
            },
        ]
    );
    let report = heap.finish();
    assert_eq!(report.ids_in(LeakClass::LeakCycle), vec![c]);
    assert!(report.is_cycle_leak_only());
}

#[test]
fn a_cascade_that_reaches_the_written_cell_twice_releases_it_twice_and_frees_it_once() {
    // c -> a, a -> [c, c]. Only the cycle holds either. set! c nil
    // frees a, which releases c twice: the first leaves 1, the second
    // frees c, whose slot (now nil) releases nothing.
    let mut heap = Heap::new();
    let c = cell(&mut heap, Value::Nil);
    let a = imm(&mut heap, vec![Value::Ref(c), Value::Ref(c)]);
    heap.write(c, 0, Value::Ref(a)).expect("close the cycle");
    release_all(&mut heap, &[a, c]);
    assert_eq!((heap.count(a), heap.count(c)), (Ok(1), Ok(2)));
    let before = heap.trace().len();
    assert_eq!(heap.write(c, 0, Value::Nil), Ok(()));
    assert!(!heap.is_live(a));
    assert!(!heap.is_live(c));
    let tail = &heap.trace()[before..];
    assert_eq!(
        tail,
        &[
            Event::Write { id: c, field: 0 },
            Event::Release {
                id: a,
                count_after: 0
            },
            Event::Free { id: a },
            Event::Release {
                id: c,
                count_after: 1
            },
            Event::Release {
                id: c,
                count_after: 0
            },
            Event::Free { id: c },
        ]
    );
    assert_eq!(releases_of(tail, c), 2);
    assert_eq!(frees_of(tail, c), 1);
    assert!(heap.finish().is_clean());
}

#[test]
fn a_shared_self_holding_atom_frees_itself_on_reset_to_nil() {
    let mut heap = Heap::new();
    let a = atom(&mut heap, Value::Nil);
    heap.write(a, 0, Value::Ref(a)).expect("self reference");
    heap.mark_shared(a).expect("cross");
    heap.release(a).expect("the binding's count");
    assert_eq!(heap.count(a), Ok(1));
    let before = heap.trace().len();
    assert_eq!(heap.write(a, 0, Value::Nil), Ok(()));
    assert!(!heap.is_live(a));
    assert_eq!(
        &heap.trace()[before..],
        &[
            Event::Write { id: a, field: 0 },
            Event::Release {
                id: a,
                count_after: 0
            },
            Event::Free { id: a },
        ]
    );
    assert!(heap.finish().is_clean());
}

#[test]
fn the_new_value_survives_a_cascade_that_frees_the_cell_and_its_only_other_holder() {
    // c -> v -> c, v also holds y, and y's binding is gone: y is held
    // only by v. set! c to y must retain y before the cascade frees v
    // (which releases y) and c (whose slot, now y, releases y again).
    // y ends with exactly the binding-free count of one: the write's.
    let mut heap = Heap::new();
    let y = imm(&mut heap, vec![Value::Int(1)]);
    let c = cell(&mut heap, Value::Nil);
    let v = imm(&mut heap, vec![Value::Ref(c), Value::Ref(y)]);
    heap.write(c, 0, Value::Ref(v)).expect("close the cycle");
    release_all(&mut heap, &[y, v, c]);
    assert_eq!(heap.count(y), Ok(1), "held by v only");
    assert_eq!(heap.write(c, 0, Value::Ref(y)), Ok(()));
    assert!(!heap.is_live(v));
    assert!(!heap.is_live(c));
    assert!(
        !heap.is_live(y),
        "y was held by v, then by c's slot; both are gone, so is y"
    );
    let report = heap.finish();
    assert!(report.is_clean(), "{:?}", report.leaks);
    assert_eq!(report.freed, 3);
}
