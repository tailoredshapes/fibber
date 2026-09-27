//! The end-of-run audit: every object still live is a leak, classified
//! so that the one leak the spec permits, a cycle through cells
//! (`spec/ownership.md` §6), is told apart from every other failure.
//!
//! At the end of the run every binding is gone, so under the counting
//! semantics of §2 a live object's count must equal the number of live
//! `Ref` fields that name it. A surplus is a count some binding never
//! gave back (a missing release); a deficit is an over-release the heap
//! has not yet caught. Either is a bug even on an object that sits on,
//! or hangs off, a cycle through a cell, so `LeakCycle` requires the
//! count to match. A live `Ref` to a freed object (dangling after an
//! over-release) is the double free of `spec/method.md` rule 2 seen
//! late, and is reported beside the leaks.
//!
//! Only counted objects are audited. Immortal objects are static data,
//! never freed and not live objects at exit (`spec/types.md` §6.7,
//! §8.2); stack objects end with their scope (§6.11), so one still live
//! at exit means its scope never ended, which is reported as an open
//! scope, never as a leak.

use super::event::Event;
use super::graph::LiveGraph;
use super::value::{Kind, ObjId, ScopeId};
use super::Heap;

/// Why an object was still live at the end of the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeakClass {
    /// Reachable from a live `Cell` or `Atom` that lies on a cycle of
    /// the live-object graph, and held by exactly as many live `Ref`
    /// fields as its count: the cycle is all that keeps it. Permitted by
    /// §6; not a memory-safety violation.
    LeakCycle,
    /// Live for any other reason: not kept by a cycle through a cell,
    /// or with a count that does not match the live `Ref` fields naming
    /// it. A bug in the program or the interpreter.
    Leak,
    /// On a cycle that passes through no `Cell` or `Atom`. Impossible
    /// under §1 (immutable objects can only refer to older objects), so
    /// this is a bug in the heap's caller and is reported loudly.
    ImmutableCycle,
}

/// One object still live at the end of the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leak {
    pub id: ObjId,
    pub kind: Kind,
    /// The count the object still had.
    pub count: usize,
    /// How many live `Ref` fields named it. Equal to `count` unless
    /// some binding's count was never released, or released too often.
    pub held: usize,
    pub class: LeakClass,
}

/// A `Ref` field of a live object whose target was freed: the target
/// was released once more than it was held, while `holder` still held
/// it. A double free (`spec/method.md` rule 2) that the heap could only
/// see later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DanglingRef {
    pub holder: ObjId,
    pub field: usize,
    pub target: ObjId,
}

/// The result of [`Heap::finish`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditReport {
    /// Every object still live, in allocation order.
    pub leaks: Vec<Leak>,
    /// Every live `Ref` to a freed object, in allocation order of the
    /// holder, then field order.
    pub dangling: Vec<DanglingRef>,
    /// Every scope still open at the end, outermost first: a failure,
    /// since every scope a program opens ends before the run does.
    pub open_scopes: Vec<ScopeId>,
    /// The whole trace of the run.
    pub trace: Vec<Event>,
    /// How many counted objects were allocated over the run (immortal
    /// and stack objects are not counted here).
    pub allocated: usize,
    /// How many of those were freed.
    pub freed: usize,
}

impl AuditReport {
    /// Nothing was live at the end, so nothing dangled either, and
    /// every scope ended.
    pub fn is_clean(&self) -> bool {
        self.leaks.is_empty() && self.dangling.is_empty() && self.open_scopes.is_empty()
    }

    /// Something was live, all of it is on the permitted leak path
    /// (`audit: leak-cycle` in a case header), and nothing dangles.
    pub fn is_cycle_leak_only(&self) -> bool {
        !self.leaks.is_empty()
            && self.bugs().next().is_none()
            && self.dangling.is_empty()
            && self.open_scopes.is_empty()
    }

    /// The leaks that are failures: everything but `LeakCycle`.
    pub fn bugs(&self) -> impl Iterator<Item = &Leak> {
        self.leaks
            .iter()
            .filter(|leak| leak.class != LeakClass::LeakCycle)
    }

    /// The ids of the leaks in one class, in allocation order.
    pub fn ids_in(&self, class: LeakClass) -> Vec<ObjId> {
        self.leaks
            .iter()
            .filter(|leak| leak.class == class)
            .map(|leak| leak.id)
            .collect()
    }
}

impl Heap {
    /// Ends the run. Every still-live object is reported as a leak with
    /// its [`LeakClass`], every dangling `Ref` is listed, every scope
    /// still open is listed, and the heap and its trace are consumed.
    /// Immortal and stack objects are never leaks.
    pub fn finish(self) -> AuditReport {
        let graph = LiveGraph::build(&self);
        let classes = classify(&graph);
        let leaks = graph
            .ids
            .iter()
            .enumerate()
            .map(|(node, &id)| Leak {
                id,
                kind: graph.kinds[node],
                count: graph.counts[node],
                held: graph.held[node],
                class: classes[node],
            })
            .collect();
        let counted = self.objects.iter().filter(|o| o.counted());
        let allocated = counted.clone().count();
        let freed = counted.filter(|o| !o.live).count();
        AuditReport {
            leaks,
            dangling: graph.dangling,
            open_scopes: self.open,
            trace: self.trace,
            allocated,
            freed,
        }
    }
}

/// The class of every node. `ImmutableCycle` wins over `LeakCycle`,
/// which wins over `Leak`; `LeakCycle` also needs the node's count to
/// equal the number of live `Ref` fields naming it.
fn classify(graph: &LiveGraph) -> Vec<LeakClass> {
    let immutable_cycles = graph.immutable_cycle_nodes();
    let cell_cycles = graph.cell_cycle_nodes();
    (0..graph.ids.len())
        .map(|node| {
            if immutable_cycles.contains(&node) {
                LeakClass::ImmutableCycle
            } else if cell_cycles.contains(&node) && graph.counts[node] == graph.held[node] {
                LeakClass::LeakCycle
            } else {
                LeakClass::Leak
            }
        })
        .collect()
}
