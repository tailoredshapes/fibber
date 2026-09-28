//! A function's control-flow graph: successors, predecessors,
//! reachability and dominators (spec/lir.md §5).

/// Blocks are indices in source order; block 0 is the entry.
#[derive(Clone, Debug)]
pub struct Cfg {
    pub succs: Vec<Vec<usize>>,
    pub preds: Vec<Vec<usize>>,
    /// Reachable blocks in reverse postorder from the entry.
    pub rpo: Vec<usize>,
    /// Immediate dominator of each reachable block (the entry's is
    /// itself); `None` for unreachable blocks.
    pub idom: Vec<Option<usize>>,
}

impl Cfg {
    /// Build from each block's (deduplicated) successor list.
    pub fn new(succs: Vec<Vec<usize>>) -> Self {
        let n = succs.len();
        let mut preds = vec![Vec::new(); n];
        for (b, ss) in succs.iter().enumerate() {
            for &s in ss {
                if !preds[s].contains(&b) {
                    preds[s].push(b);
                }
            }
        }
        let rpo = reverse_postorder(&succs);
        let idom = dominators(&preds, &rpo, n);
        Cfg {
            succs,
            preds,
            rpo,
            idom,
        }
    }

    pub fn reachable(&self, b: usize) -> bool {
        self.idom[b].is_some()
    }

    /// Whether `a` dominates `b` (both reachable; a block dominates
    /// itself).
    pub fn dominates(&self, a: usize, b: usize) -> bool {
        let mut cur = b;
        loop {
            if cur == a {
                return true;
            }
            match self.idom[cur] {
                Some(d) if d != cur => cur = d,
                _ => return false,
            }
        }
    }

    /// Whether a value bound in `def` may be used in `user`: `def`
    /// dominates `user`, or `user` is unreachable and `def` reachable
    /// (spec/lir.md §5.3).
    pub fn available(&self, def: usize, user: usize) -> bool {
        if self.reachable(user) {
            self.reachable(def) && self.dominates(def, user)
        } else {
            self.reachable(def)
        }
    }
}

fn reverse_postorder(succs: &[Vec<usize>]) -> Vec<usize> {
    let n = succs.len();
    let mut seen = vec![false; n];
    let mut post = Vec::with_capacity(n);
    if n == 0 {
        return post;
    }
    // Iterative DFS: (block, next successor index).
    let mut stack = vec![(0usize, 0usize)];
    seen[0] = true;
    while let Some((b, i)) = stack.pop() {
        if i < succs[b].len() {
            stack.push((b, i + 1));
            let s = succs[b][i];
            if !seen[s] {
                seen[s] = true;
                stack.push((s, 0));
            }
        } else {
            post.push(b);
        }
    }
    post.reverse();
    post
}

/// Cooper, Harvey and Kennedy's iterative algorithm.
fn dominators(preds: &[Vec<usize>], rpo: &[usize], n: usize) -> Vec<Option<usize>> {
    let mut order = vec![usize::MAX; n];
    for (i, &b) in rpo.iter().enumerate() {
        order[b] = i;
    }
    let mut idom: Vec<Option<usize>> = vec![None; n];
    if rpo.is_empty() {
        return idom;
    }
    idom[rpo[0]] = Some(rpo[0]);
    let mut changed = true;
    while changed {
        changed = false;
        for &b in &rpo[1..] {
            let mut new: Option<usize> = None;
            for &p in &preds[b] {
                if idom[p].is_none() {
                    continue;
                }
                new = Some(match new {
                    None => p,
                    Some(q) => intersect(&idom, &order, p, q),
                });
            }
            if new.is_some() && idom[b] != new {
                idom[b] = new;
                changed = true;
            }
        }
    }
    idom
}

fn intersect(idom: &[Option<usize>], order: &[usize], a: usize, b: usize) -> usize {
    let (mut x, mut y) = (a, b);
    while x != y {
        while order[x] > order[y] {
            x = idom[x].unwrap_or(x);
        }
        while order[y] > order[x] {
            y = idom[y].unwrap_or(y);
        }
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diamond_and_loop() {
        // 0 -> 1, 2; 1 -> 3; 2 -> 3; 3 -> 3, 4; 5 unreachable -> 4
        let cfg = Cfg::new(vec![
            vec![1, 2],
            vec![3],
            vec![3],
            vec![3, 4],
            vec![],
            vec![4],
        ]);
        assert!(cfg.dominates(0, 3));
        assert!(!cfg.dominates(1, 3));
        assert!(cfg.dominates(3, 4));
        assert!(!cfg.reachable(5));
        assert!(cfg.available(0, 5));
        assert!(!cfg.available(5, 4));
        assert_eq!(cfg.preds[3], vec![1, 2, 3]);
        assert_eq!(cfg.rpo[0], 0);
    }
}
