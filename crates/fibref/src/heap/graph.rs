//! The live-object graph the end-of-run audit reads: nodes are live
//! objects, edges are `Ref` fields between them (`Weak` fields keep
//! nothing alive and are not edges). Building it also finds every `Ref`
//! that dangles, and counts how many live `Ref` fields name each node.

use std::collections::{HashMap, HashSet};

use super::audit::DanglingRef;
use super::scc::{is_cyclic, strongly_connected_components};
use super::value::{Kind, ObjId};
use super::Heap;

/// The live-object graph, indexed by node position (allocation order).
pub(super) struct LiveGraph {
    pub(super) ids: Vec<ObjId>,
    pub(super) kinds: Vec<Kind>,
    pub(super) counts: Vec<usize>,
    /// How many live `Ref` fields name each node: its in-degree.
    pub(super) held: Vec<usize>,
    pub(super) adj: Vec<Vec<usize>>,
    /// `Ref` fields of live objects whose target is freed.
    pub(super) dangling: Vec<DanglingRef>,
}

impl LiveGraph {
    pub(super) fn build(heap: &Heap) -> LiveGraph {
        let ids = heap.live_ids();
        let position: HashMap<ObjId, usize> =
            ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        let mut graph = LiveGraph {
            kinds: ids.iter().map(|id| heap.objects[id.index()].kind).collect(),
            counts: ids
                .iter()
                .map(|id| heap.objects[id.index()].count)
                .collect(),
            held: vec![0; ids.len()],
            adj: vec![Vec::new(); ids.len()],
            dangling: Vec::new(),
            ids,
        };
        for from in 0..graph.ids.len() {
            let id = graph.ids[from];
            let object = &heap.objects[id.index()];
            for (field, value) in object.fields.iter().enumerate() {
                if let Some(target) = value.as_ref() {
                    graph.add_edge(&position, from, id, field, target);
                }
            }
        }
        graph
    }

    /// Records the `Ref` in field `field` of `holder` (node `from`): an
    /// edge if its target is live, a dangling reference otherwise.
    fn add_edge(
        &mut self,
        position: &HashMap<ObjId, usize>,
        from: usize,
        holder: ObjId,
        field: usize,
        target: ObjId,
    ) {
        match position.get(&target) {
            Some(&to) => {
                self.adj[from].push(to);
                self.held[to] += 1;
            }
            None => self.dangling.push(DanglingRef {
                holder,
                field,
                target,
            }),
        }
    }

    /// The same graph with only the edges between `Immutable` objects.
    fn immutable_edges(&self) -> Vec<Vec<usize>> {
        self.adj
            .iter()
            .enumerate()
            .map(|(from, targets)| {
                if self.kinds[from] != Kind::Immutable {
                    return Vec::new();
                }
                targets
                    .iter()
                    .copied()
                    .filter(|&to| self.kinds[to] == Kind::Immutable)
                    .collect()
            })
            .collect()
    }

    /// Nodes on a cycle that passes through no `Cell` or `Atom`.
    pub(super) fn immutable_cycle_nodes(&self) -> HashSet<usize> {
        let adj = self.immutable_edges();
        strongly_connected_components(&adj)
            .into_iter()
            .filter(|component| is_cyclic(&adj, component))
            .flatten()
            .collect()
    }

    /// Nodes reachable from a `Cell` or `Atom` that lies on a cycle.
    pub(super) fn cell_cycle_nodes(&self) -> HashSet<usize> {
        let roots: Vec<usize> = strongly_connected_components(&self.adj)
            .into_iter()
            .filter(|component| is_cyclic(&self.adj, component))
            .flatten()
            .filter(|&node| self.kinds[node].is_mutable())
            .collect();
        let mut reached: HashSet<usize> = HashSet::new();
        let mut stack = roots;
        while let Some(node) = stack.pop() {
            if reached.insert(node) {
                stack.extend(self.adj[node].iter().copied());
            }
        }
        reached
    }
}
