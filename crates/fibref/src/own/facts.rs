//! The facts of one unit's fixpoint (types §6.4, §6.5): which inferred
//! parameters are owned and why, which parameters and loop variables
//! escape, which closure literals are escaping and which are on the
//! heap. All of them only grow; [`Facts::absorb`] adds what one walk's
//! events imply and says whether anything changed.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::types::ast::{BindingId, BindingKind, ExprId, FunId};
use crate::types::TypedProgram;

use super::objects::is_object;
use super::program::{OwnedWhy, Site, Summary};
use super::walk::{At, Class, Event, Val};

/// The unit's facts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// Owned parameters with every reason found (inferred ones and the
    /// presets: closure, `:owned`, all-owned).
    pub owned: BTreeMap<BindingId, BTreeSet<OwnedWhy>>,
    /// Parameters and loop variables that escape, with the first reason.
    pub escapes: BTreeMap<BindingId, String>,
    /// Loop variables to which rule 1 applies.
    pub loop_rule1: BTreeSet<BindingId>,
    /// Escaping closure literals, with the first use of kind (d).
    pub escaping: HashMap<ExprId, String>,
    /// Heap closure literals, with the reason.
    pub heap: HashMap<ExprId, String>,
    /// Values consumed into loop variables: (from, loop variable).
    edges: BTreeSet<(BindingId, BindingId)>,
    /// The parameters whose kind this fixpoint infers.
    inferred: BTreeSet<BindingId>,
    /// Each SCC member's parameters, by position.
    members: BTreeMap<FunId, Vec<BindingId>>,
}

/// The description of a rule-1 or escape position.
fn describe(at: &At) -> Option<&'static str> {
    Some(match at {
        At::Return => "returned",
        At::Store => "stored",
        At::Thread => "spawned",
        At::Join => "join retain",
        At::RawRetained => "raw-retained",
        At::Capture { heap: true, .. } => "captured by a heap closure",
        _ => return None,
    })
}

impl Facts {
    /// Facts for a unit whose members are `members` (a `defun`'s
    /// parameter bindings, by position), inferring the kinds of
    /// `inferred`, with the parameters of `preset` owned from the start.
    pub fn new(
        members: BTreeMap<FunId, Vec<BindingId>>,
        inferred: BTreeSet<BindingId>,
        preset: &[BindingId],
    ) -> Self {
        let mut f = Facts {
            members,
            inferred,
            ..Facts::default()
        };
        for b in preset {
            f.owned.entry(*b).or_default().insert(OwnedWhy::Declared);
        }
        f
    }

    /// The summary of SCC member `f` under these facts.
    pub fn summary(&self, p: &TypedProgram, f: FunId) -> Summary {
        let def = p.globals.fun(f);
        let params = def
            .params
            .iter()
            .map(|d| {
                let obj = !d.amp
                    && p.binding_types
                        .get(&d.binding)
                        .is_none_or(|t| is_object(&p.globals, t));
                if !obj {
                    return (false, false);
                }
                let owned = self.owned.contains_key(&d.binding);
                let escapes = self.escapes.contains_key(&d.binding) && !d.borrow;
                (owned, escapes)
            })
            .collect();
        Summary { params }
    }

    /// Adds what `events` imply about the unit whose `fn` literals are
    /// `literals`; returns whether anything changed.
    pub(super) fn absorb(
        &mut self,
        p: &TypedProgram,
        events: &[Event],
        literals: &HashSet<ExprId>,
    ) -> bool {
        let before = self.clone();
        for ev in events {
            if let Val::B(Site::Bind(b)) = ev.val {
                self.binding_event(p, b, &ev.at);
            }
            self.rule3(ev);
        }
        self.propagate(p);
        let (escaping, heap) = super::classify::classify(events, literals);
        for (l, why) in escaping {
            self.escaping.entry(l).or_insert(why);
        }
        for (l, why) in heap {
            self.heap.insert(l, why);
        }
        *self != before
    }

    fn own(&mut self, b: BindingId, why: OwnedWhy) {
        if self.inferred.contains(&b) {
            self.owned.entry(b).or_default().insert(why);
        }
    }

    fn escape(&mut self, b: BindingId, why: &str) {
        self.escapes.entry(b).or_insert_with(|| why.to_string());
    }

    fn binding_event(&mut self, p: &TypedProgram, b: BindingId, at: &At) {
        let kind = p.globals.binding(b).kind;
        let is_loop = kind == BindingKind::Loop;
        if kind != BindingKind::Param && !is_loop {
            return;
        }
        if let Some(what) = describe(at) {
            self.escape(b, what);
            if is_loop {
                self.loop_rule1.insert(b);
            } else {
                self.own(b, OwnedWhy::Rule1(what.to_string()));
            }
            return;
        }
        match at {
            At::MoveOut if is_loop => {
                self.escape(b, "the loop's value");
                self.loop_rule1.insert(b);
            }
            At::Weak => self.escape(b, "operand of weak"),
            At::Arg(a) => {
                if a.escapes || a.closure_value {
                    self.escape(b, "passed to an escaping parameter");
                }
                if a.tail_site && a.class != Class::Borrow {
                    self.own(
                        b,
                        OwnedWhy::Rule2(format!("argument {} of a tail site", a.index + 1)),
                    );
                }
            }
            At::Head(h) if h.tail_site && !h.self_tail => {
                self.own(b, OwnedWhy::Rule2("called at a tail site".to_string()));
            }
            At::LoopInit { var } | At::Recur { var } => {
                self.edges.insert((b, *var));
            }
            _ => {}
        }
    }

    /// Rule 3: a tail call from a member of the SCC passes a frame-owned
    /// argument at a parameter of a member.
    fn rule3(&mut self, ev: &Event) {
        let At::Arg(a) = &ev.at else { return };
        let Some(g) = a.scc_callee else { return };
        if !(a.tail_site && a.frame_owned) {
            return;
        }
        if let Some(b) = self.members.get(&g).and_then(|ps| ps.get(a.index)).copied() {
            self.own(
                b,
                OwnedWhy::Rule3("a tail call in the SCC passes a frame-owned argument".into()),
            );
        }
    }

    /// Follows values through the loop variables they were consumed into.
    fn propagate(&mut self, p: &TypedProgram) {
        let mut changed = true;
        while changed {
            changed = false;
            for (b, v) in self.edges.clone() {
                if let Some(why) = self.escapes.get(&v).cloned() {
                    if let std::collections::btree_map::Entry::Vacant(e) = self.escapes.entry(b) {
                        e.insert(format!("through loop variable {}: {why}", name(p, v)));
                        changed = true;
                    }
                }
                if self.loop_rule1.contains(&v) {
                    if p.globals.binding(b).kind == BindingKind::Loop {
                        changed |= self.loop_rule1.insert(b);
                    } else if self.inferred.contains(&b) {
                        let why = OwnedWhy::Loop(format!("through loop variable {}", name(p, v)));
                        changed |= self.owned.entry(b).or_default().insert(why);
                    }
                }
            }
        }
    }
}

fn name(p: &TypedProgram, b: BindingId) -> String {
    p.globals.binding(b).name.clone()
}
