//! Closure literals: escaping and heap (types §6.5). Every use of a
//! literal — the literal itself, the `let` binding it directly
//! initialises, a named `fn`'s self-name inside its body — is one of
//! (a) a direct call, (b) an argument to a non-escaping parameter of a
//! known callee, (c) the direct initialiser of a `let` binding all of
//! whose uses are (a) or (b) and which no closure captures, or (d)
//! anything else. Escaping iff some use is (d); heap iff escaping or
//! some use is the head or an argument of a tail site (the self-name as
//! the head of its own self tail call excepted).

use std::collections::{HashMap, HashSet};

use crate::types::ast::{BindingId, ExprId};

use super::program::Site;
use super::walk::{At, Event, Val};

/// The literal a use is a use of.
fn literal_of(v: &Val, bound: &HashMap<BindingId, ExprId>) -> Option<ExprId> {
    match v {
        Val::Lit(l) => Some(*l),
        Val::B(Site::Bind(f)) => bound.get(f).copied(),
        Val::B(Site::Env(l)) => Some(*l),
        _ => None,
    }
}

/// A use of kind (d), described; `None` for (a), (b) and (c).
fn escaping_use(ev: &Event) -> Option<String> {
    match &ev.at {
        At::Head(_) => None,
        At::Arg(a) if !a.escapes && !a.closure_value => None,
        At::LetInit { .. } if matches!(ev.val, Val::Lit(_)) => None,
        At::Arg(a) if a.closure_value => Some("arg-to-closure-value".into()),
        At::Arg(_) => Some("arg-to-escapes".into()),
        At::Return => Some("returned".into()),
        At::Store => Some("stored".into()),
        At::Thread => Some("spawned".into()),
        At::Capture { .. } => Some("captured by a closure".into()),
        At::Join => Some("a branch of a join".into()),
        At::LoopInit { .. } => Some("initialises a loop variable".into()),
        At::Recur { .. } => Some("arg-of-recur".into()),
        At::LetInit { .. } => Some("bound again by let".into()),
        At::MoveOut => Some("moved out of its scope".into()),
        At::Weak | At::RawRetained | At::Other => Some("used as a value".into()),
    }
}

/// Why a use at a tail site puts the literal on the heap, if it does.
fn tail_use(ev: &Event) -> Option<&'static str> {
    match &ev.at {
        At::Head(h) if h.tail_site && !h.self_tail => Some(if h.admitted {
            "head-of-tail-call"
        } else {
            "head-of-tail-site"
        }),
        At::Arg(a) if a.tail_site => Some(if a.admitted {
            "arg-of-tail-call"
        } else {
            "arg-of-tail-site"
        }),
        At::Recur { .. } => Some("arg-of-recur"),
        _ => None,
    }
}

/// The escaping literals and the heap literals among `literals`, each
/// with its reason. A literal whose value reaches no recorded position
/// (inside a `dyn`, say) is taken as escaping, the safe side.
pub fn classify(
    events: &[Event],
    literals: &HashSet<ExprId>,
) -> (HashMap<ExprId, String>, HashMap<ExprId, String>) {
    let mut bound = HashMap::new();
    for ev in events {
        if let (Val::Lit(l), At::LetInit { binding }) = (&ev.val, &ev.at) {
            bound.insert(*binding, *l);
        }
    }
    let mut escaping: HashMap<ExprId, String> = HashMap::new();
    let mut heap: HashMap<ExprId, String> = HashMap::new();
    let mut used = HashSet::new();
    for ev in events {
        let Some(l) = literal_of(&ev.val, &bound) else {
            continue;
        };
        used.insert(l);
        if let Some(why) = escaping_use(ev) {
            escaping.entry(l).or_insert(why);
        }
        if let Some(why) = tail_use(ev) {
            heap.entry(l).or_insert_with(|| why.to_string());
        }
    }
    for l in literals.difference(&used) {
        escaping.insert(*l, "used as a value".into());
    }
    for (l, why) in &escaping {
        heap.insert(*l, why.clone());
    }
    (escaping, heap)
}
