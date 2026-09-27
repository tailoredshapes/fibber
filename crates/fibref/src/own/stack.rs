//! Scope-local bindings (types §6.11): a binding, `match` temporary or
//! implicit temporary whose initialiser allocates its object in this
//! frame is on the stack iff no occurrence of `Borrowed(b)`, on any path
//! of its scope, is at an escape position E1–E6 (E6: an admitted tail
//! call), passed to an escaping parameter or to a closure value, moved
//! out or retained by a scope exit or a join, the initialiser of a loop
//! variable, or the operand of `weak` or `raw-retained`. A closure
//! literal is decided by §6.5 instead: a stack closure's binding or
//! temporary is scope-local.

use std::collections::{HashMap, HashSet};

use crate::types::ast::ExprId;

use super::program::Site;
use super::walk::{At, Event, Val};

/// Whether an occurrence at `at` keeps its binding off the stack.
fn disqualifies(at: &At) -> bool {
    match at {
        At::Return
        | At::Store
        | At::Thread
        | At::Join
        | At::RawRetained
        | At::Weak
        | At::MoveOut
        | At::LoopInit { .. }
        | At::Recur { .. } => true,
        At::Capture { heap, .. } => *heap,
        At::Arg(a) => a.admitted || a.escapes || a.closure_value,
        At::Head(h) => h.admitted,
        At::LetInit { .. } | At::Other => false,
    }
}

/// The scope-local sites among `candidates` (site → the allocating
/// initialiser), given the events of the admission walk and the heap
/// literals.
pub fn scope_local(
    events: &[Event],
    candidates: &HashMap<Site, ExprId>,
    literals: &HashSet<ExprId>,
    heap: &HashMap<ExprId, String>,
) -> HashSet<Site> {
    let mut out_of_scope = HashSet::new();
    for ev in events {
        if let Val::B(s) = ev.val {
            if disqualifies(&ev.at) {
                out_of_scope.insert(s);
            }
        }
    }
    candidates
        .iter()
        .filter(|(s, init)| {
            if literals.contains(init) {
                !heap.contains_key(init)
            } else {
                !out_of_scope.contains(s)
            }
        })
        .map(|(s, _)| *s)
        .collect()
}
