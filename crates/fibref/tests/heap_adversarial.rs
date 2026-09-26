//! Adversarial tests for the instrumented heap, through its public API
//! only (`spec/ownership.md` §2, §6, §7; `spec/method.md` rule 2).
//!
//! Each test tries to make the audit miss a misuse, mis-classify a
//! leak, or change state on a failing call. A failing test here is a
//! finding about `crates/fibref/src/heap`, not about the test.
//!
//! One file per concern: `errors` (every `AuditError` variant firing),
//! `free` (release cascades), `leaks` (end-of-run classification),
//! `weak` (weak references), `shared` (thread crossing), `trace` (event
//! order) and `scale` (nothing recursive). Round 2 adds `dangling` (a
//! double free the heap only sees later), `rollback` (state after a
//! call that fails at its last step), `masked` (leaks a permitted cycle
//! could hide) and `order` (more event orderings, and identities at
//! scale). Round 3 adds `acyclic` (random acyclic programs checked
//! against an independent counting model and against a replay of the
//! heap's own trace, both in `replay`), `reentrant` (writes whose
//! cascade frees the cell being written), `deficit` (over-releases a
//! second holder hides, and forged ids), `report` (the report's parts
//! agreeing with each other),
//! `bigweak` (more shapes at scale), `cyclic` (random cyclic programs
//! against an independent classifier) and `fault` (one injected fault
//! must always be caught).

#[path = "heap_adversarial/acyclic.rs"]
mod acyclic;
#[path = "heap_adversarial/bigweak.rs"]
mod bigweak;
#[path = "heap_adversarial/cyclic.rs"]
mod cyclic;
#[path = "heap_adversarial/dangling.rs"]
mod dangling;
#[path = "heap_adversarial/deficit.rs"]
mod deficit;
#[path = "heap_adversarial/errors.rs"]
mod errors;
#[path = "heap_adversarial/fault.rs"]
mod fault;
#[path = "heap_adversarial/free.rs"]
mod free;
#[path = "heap_adversarial/leaks.rs"]
mod leaks;
#[path = "heap_adversarial/masked.rs"]
mod masked;
#[path = "heap_adversarial/order.rs"]
mod order;
#[path = "heap_adversarial/reentrant.rs"]
mod reentrant;
#[path = "heap_adversarial/replay.rs"]
mod replay;
#[path = "heap_adversarial/report.rs"]
mod report;
#[path = "heap_adversarial/rollback.rs"]
mod rollback;
#[path = "heap_adversarial/scale.rs"]
mod scale;
#[path = "heap_adversarial/shared.rs"]
mod shared;
#[path = "heap_adversarial/trace.rs"]
mod trace;
#[path = "heap_adversarial/weak.rs"]
mod weak;

use fibref::{Event, Heap, Kind, ObjId, Value};

/// An immutable object holding `fields`.
pub fn imm(heap: &mut Heap, fields: Vec<Value>) -> ObjId {
    heap.alloc(Kind::Immutable, fields)
        .expect("alloc immutable")
}

/// A cell holding `value`.
pub fn cell(heap: &mut Heap, value: Value) -> ObjId {
    heap.alloc(Kind::Cell, vec![value]).expect("alloc cell")
}

/// An atom holding `value`.
pub fn atom(heap: &mut Heap, value: Value) -> ObjId {
    heap.alloc(Kind::Atom, vec![value]).expect("alloc atom")
}

/// Releases the caller's count on each id, in order.
pub fn release_all(heap: &mut Heap, ids: &[ObjId]) {
    for &id in ids {
        heap.release(id).expect("release");
    }
}

/// An id that some other heap handed out, so this heap has never seen
/// it. Several objects are allocated so the index is not zero.
pub fn foreign_id() -> ObjId {
    let mut other = Heap::new();
    let mut last = imm(&mut other, vec![]);
    for _ in 0..3 {
        last = imm(&mut other, vec![]);
    }
    last
}

/// How many `Free` events name `id`.
pub fn frees_of(trace: &[Event], id: ObjId) -> usize {
    trace
        .iter()
        .filter(|e| matches!(e, Event::Free { id: f } if *f == id))
        .count()
}

/// How many `Release` events name `id`.
pub fn releases_of(trace: &[Event], id: ObjId) -> usize {
    trace
        .iter()
        .filter(|e| matches!(e, Event::Release { id: r, .. } if *r == id))
        .count()
}

/// How many `Retain` events name `id`.
pub fn retains_of(trace: &[Event], id: ObjId) -> usize {
    trace
        .iter()
        .filter(|e| matches!(e, Event::Retain { id: r, .. } if *r == id))
        .count()
}

/// The ids of every `Shared` event, in order.
pub fn shared_ids(trace: &[Event]) -> Vec<ObjId> {
    trace
        .iter()
        .filter_map(|e| match e {
            Event::Shared { id } => Some(*id),
            _ => None,
        })
        .collect()
}

/// A singly linked list of `len` immutable nodes whose last node holds
/// `tail`. Every node but the head is held only by its predecessor.
/// Returns the head, which the caller owns with count 1.
pub fn chain(heap: &mut Heap, len: usize, tail: Value) -> ObjId {
    assert!(len >= 1, "a chain has at least one node");
    let mut head = imm(heap, vec![Value::Int(0), tail]);
    for i in 1..len {
        let next = imm(heap, vec![Value::Int(i as i64), Value::Ref(head)]);
        heap.release(head).expect("predecessor holds the node now");
        head = next;
    }
    head
}

/// A cell `c` and a vector `v` with `c -> v -> c`, both held only by
/// each other: the cycle of case 15. Returns `(c, v)`.
pub fn cell_cycle(heap: &mut Heap) -> (ObjId, ObjId) {
    let c = cell(heap, Value::Nil);
    let v = imm(heap, vec![Value::Ref(c)]);
    heap.write(c, 0, Value::Ref(v)).expect("set!");
    release_all(heap, &[v, c]);
    (c, v)
}
