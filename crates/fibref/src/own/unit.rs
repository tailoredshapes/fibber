//! One unit of the ownership pass (types §3.5 step 5f, §6.10 "the order
//! of the decisions"): the bodies of an SCC of `defun`s (or of their
//! all-owned bodies), of one `impl` method, of a `def`. In order:
//!
//! 1. **tail sites**: a walk with every inferred kind borrowed and every
//!    closure on the stack decides which calls in tail position §6.10
//!    admits; those are the tail sites, fixed from here on;
//! 2. **the fixpoint** of §6.4: kinds, escape summaries and heap
//!    closures, walked until the facts stop growing;
//! 3. **admission**: a walk that applies rule (e) to the tail sites with
//!    the finished facts; its events decide the scope-local bindings
//!    (§6.11), for which E6 is the admitted tail calls;
//! 4. **emission**: a last walk with the scope-local bindings known.

use std::collections::{HashMap, HashSet};

use crate::types::ast::{ExprId, FunId};
use crate::types::TypedProgram;

use super::facts::Facts;
use super::program::{Alloc, BodyKey, BodyOwn, ParamKind, ParamOwn, Site, Summary, Tail};
use super::stack::scope_local;
use super::walk::{BodySpec, Ctx, Event, Walker};

/// A unit to decide.
pub(super) struct Unit<'p> {
    pub bodies: Vec<(BodyKey, BodySpec<'p>)>,
    /// The `defun`s of the SCC (empty for a method or a `def`).
    pub scc: Vec<FunId>,
    /// Whether the bodies are all-owned bodies.
    pub all_owned: bool,
}

/// What one walk over every body of a unit found.
struct Walked {
    outs: Vec<BodyOwn>,
    events: Vec<Event>,
    tails: HashMap<ExprId, Tail>,
    candidates: HashMap<Site, ExprId>,
    literals: HashSet<ExprId>,
}

/// The decisions for a unit.
pub(super) struct Decided {
    pub bodies: Vec<(BodyKey, BodyOwn)>,
    pub facts: Facts,
}

struct Run<'a, 'p> {
    p: &'a TypedProgram,
    summaries: &'a HashMap<FunId, Summary>,
    unit: &'a Unit<'p>,
}

impl Run<'_, '_> {
    fn walk(
        &self,
        facts: &Facts,
        sites: Option<&HashMap<ExprId, Tail>>,
        admit: bool,
        local: &HashSet<Site>,
    ) -> Walked {
        let mut w = Walked {
            outs: Vec::new(),
            events: Vec::new(),
            tails: HashMap::new(),
            candidates: HashMap::new(),
            literals: HashSet::new(),
        };
        for (_, spec) in &self.unit.bodies {
            let cx = Ctx {
                p: self.p,
                summaries: self.summaries,
                facts,
                scc: &self.unit.scc,
                all_owned: self.unit.all_owned,
                tail_sites: sites,
                admit,
                scope_local: local,
            };
            let mut walker = Walker::new(cx);
            walker.walk_body(spec);
            w.events.append(&mut walker.events);
            w.tails.extend(walker.tails.drain());
            w.candidates.extend(walker.candidates.drain());
            w.literals.extend(
                walker
                    .out
                    .closures
                    .iter()
                    .filter(|(_, c)| !c.is_async)
                    .map(|(l, _)| *l),
            );
            w.outs.push(walker.out);
        }
        w
    }
}

/// Decides a unit, starting from `facts`.
pub(super) fn decide(
    p: &TypedProgram,
    summaries: &HashMap<FunId, Summary>,
    unit: &Unit<'_>,
    mut facts: Facts,
) -> Decided {
    let run = Run { p, summaries, unit };
    let none = HashSet::new();
    let sites = run.walk(&facts, None, false, &none).tails;
    loop {
        let w = run.walk(&facts, Some(&sites), false, &none);
        if !facts.absorb(p, &w.events, &w.literals) {
            break;
        }
    }
    let admitted = run.walk(&facts, Some(&sites), true, &none);
    facts.absorb(p, &admitted.events, &admitted.literals);
    let local = scope_local(
        &admitted.events,
        &admitted.candidates,
        &admitted.literals,
        &facts.heap,
    );
    let emitted = run.walk(&facts, Some(&sites), true, &local);
    let bodies = unit
        .bodies
        .iter()
        .zip(emitted.outs)
        .map(|((key, spec), mut out)| {
            finish_allocs(&mut out, &emitted.candidates, &emitted.literals, &local);
            out.params = params(&facts, spec);
            (*key, out)
        })
        .collect();
    Decided { bodies, facts }
}

/// The allocation sites of a body that are scope-local.
fn finish_allocs(
    out: &mut BodyOwn,
    cands: &HashMap<Site, ExprId>,
    lits: &HashSet<ExprId>,
    local: &HashSet<Site>,
) {
    for (s, init) in cands {
        if !local.contains(s) || lits.contains(init) || !out.allocs.contains_key(init) {
            continue;
        }
        out.allocs.insert(*init, Alloc::Stack);
        if let Site::Value(e) = s {
            out.stack_temps.insert(*e);
        }
    }
}

/// The parameters of a body as the facts decided them.
fn params(facts: &Facts, spec: &BodySpec<'_>) -> Vec<ParamOwn> {
    use super::walk::ParamIn;
    let declared: Vec<bool> = spec.declared_borrow.clone();
    spec.params
        .iter()
        .enumerate()
        .map(|(i, (b, pin))| {
            let kind = match pin {
                ParamIn::Scalar => ParamKind::Scalar,
                ParamIn::Amp => ParamKind::Amp,
                ParamIn::Obj { .. } => match facts.owned.get(b) {
                    Some(ws) => ParamKind::Owned(ws.iter().cloned().collect()),
                    None => ParamKind::Borrowed,
                },
            };
            let declared_borrow = declared.get(i).copied().unwrap_or(false);
            ParamOwn {
                binding: *b,
                escapes: facts.escapes.contains_key(b) && !declared_borrow,
                kind,
                declared_borrow,
            }
        })
        .collect()
}
