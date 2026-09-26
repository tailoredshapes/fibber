//! Strongly connected components of a directed graph, by Tarjan's
//! algorithm written with an explicit stack so that a graph of any
//! depth (a long list) does not overflow the call stack.

/// One frame of the simulated recursion: a node and how many of its
/// edges have been followed.
struct Frame {
    node: usize,
    next_edge: usize,
}

/// Tarjan's bookkeeping for one search.
struct Search<'a> {
    adj: &'a [Vec<usize>],
    /// Visit order, or `usize::MAX` while unvisited.
    index: Vec<usize>,
    low: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    next_index: usize,
    components: Vec<Vec<usize>>,
}

/// The strongly connected components of a graph given as adjacency
/// lists (`adj[v]` are the successors of `v`). Every node is in exactly
/// one component; a component is a cycle iff [`is_cyclic`] says so.
pub fn strongly_connected_components(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = adj.len();
    let mut search = Search {
        adj,
        index: vec![usize::MAX; n],
        low: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        next_index: 0,
        components: Vec::new(),
    };
    for root in 0..n {
        if search.index[root] == usize::MAX {
            search.run_from(root);
        }
    }
    search.components
}

/// Whether a component lies on a cycle: it has more than one node, or
/// its one node has an edge to itself.
pub fn is_cyclic(adj: &[Vec<usize>], component: &[usize]) -> bool {
    match component {
        [single] => adj[*single].contains(single),
        _ => component.len() > 1,
    }
}

impl Search<'_> {
    fn visit(&mut self, node: usize) {
        self.index[node] = self.next_index;
        self.low[node] = self.next_index;
        self.next_index += 1;
        self.stack.push(node);
        self.on_stack[node] = true;
    }

    /// The depth-first search from an unvisited root, iteratively.
    fn run_from(&mut self, root: usize) {
        self.visit(root);
        let mut frames = vec![Frame {
            node: root,
            next_edge: 0,
        }];
        while let Some(frame) = frames.last_mut() {
            let node = frame.node;
            if let Some(&target) = self.adj[node].get(frame.next_edge) {
                frame.next_edge += 1;
                if self.index[target] == usize::MAX {
                    self.visit(target);
                    frames.push(Frame {
                        node: target,
                        next_edge: 0,
                    });
                } else if self.on_stack[target] {
                    self.low[node] = self.low[node].min(self.index[target]);
                }
                continue;
            }
            frames.pop();
            if let Some(parent) = frames.last() {
                self.low[parent.node] = self.low[parent.node].min(self.low[node]);
            }
            if self.low[node] == self.index[node] {
                self.pop_component(node);
            }
        }
    }

    /// Pops the component rooted at `node` off the Tarjan stack.
    fn pop_component(&mut self, node: usize) {
        let mut component = Vec::new();
        while let Some(member) = self.stack.pop() {
            self.on_stack[member] = false;
            component.push(member);
            if member == node {
                break;
            }
        }
        self.components.push(component);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut components: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
        for c in &mut components {
            c.sort_unstable();
        }
        components.sort();
        components
    }

    #[test]
    fn acyclic_graph_has_singleton_components() {
        let adj = vec![vec![1], vec![2], vec![]];
        let components = strongly_connected_components(&adj);
        assert_eq!(sorted(components.clone()), vec![vec![0], vec![1], vec![2]]);
        assert!(components.iter().all(|c| !is_cyclic(&adj, c)));
    }

    #[test]
    fn two_cycle_is_one_component() {
        let adj = vec![vec![1], vec![0], vec![0]];
        let components = strongly_connected_components(&adj);
        assert_eq!(sorted(components.clone()), vec![vec![0, 1], vec![2]]);
        let cyclic: Vec<bool> = sorted(components)
            .iter()
            .map(|c| is_cyclic(&adj, c))
            .collect();
        assert_eq!(cyclic, vec![true, false]);
    }

    #[test]
    fn self_loop_is_cyclic() {
        let adj = vec![vec![0]];
        let components = strongly_connected_components(&adj);
        assert_eq!(components, vec![vec![0]]);
        assert!(is_cyclic(&adj, &components[0]));
    }

    #[test]
    fn long_chain_does_not_overflow() {
        let n = 200_000;
        let adj: Vec<Vec<usize>> = (0..n)
            .map(|i| if i + 1 < n { vec![i + 1] } else { vec![] })
            .collect();
        let components = strongly_connected_components(&adj);
        assert_eq!(components.len(), n);
    }

    #[test]
    fn long_cycle_is_one_component() {
        let n = 100_000;
        let adj: Vec<Vec<usize>> = (0..n).map(|i| vec![(i + 1) % n]).collect();
        let components = strongly_connected_components(&adj);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].len(), n);
    }
}
