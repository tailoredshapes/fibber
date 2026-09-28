//! The interpreter's shared state, the [`World`]: everything two
//! threads of the program can reach (the heap, the object table, the
//! program and its plans, the `def`s, the scheduler). The thread that
//! holds the turn owns it; `threads` hands it from one thread to the
//! next by value.

use std::collections::HashMap;

use crate::expand::ExpandCtx;
use crate::heap::{Heap, ObjId, ScopeId};
use crate::own::program::OwnedProgram;
use crate::types::ast::{Expr, ExprId};
use crate::types::TypedProgram;

use super::fx::{FxMap, FxSet};
use super::object::Objects;
use super::plan::{literals, value_sites, Plans};
use super::raw::RawMemory;
use super::sched::Sched;
use super::value::Val;

/// Values cached per program: immortal literals and function values.
#[derive(Debug, Default)]
pub struct Statics {
    /// String and `Form` literals by expression.
    pub lits: FxMap<ExprId, ObjId>,
    /// Immortal closures of named functions, builtins, methods, ctors.
    pub closures: HashMap<String, ObjId>,
    /// Interned keywords.
    pub keywords: Vec<String>,
    /// Keyword ids by name.
    pub keyword_ids: HashMap<String, u32>,
}

/// The state all threads of a run share, over one checked program.
pub struct World<'p> {
    /// The typed program.
    pub p: &'p TypedProgram,
    /// Its ownership plan.
    pub o: &'p OwnedProgram,
    /// The audited heap.
    pub heap: Heap,
    /// What each object is.
    pub objs: Objects,
    /// Every body's plan, indexed.
    pub plans: Plans<'p>,
    /// Every `fn` and `async` literal, by id.
    pub lits: FxMap<ExprId, &'p Expr>,
    /// The instance of every method use the checker resolved to one.
    pub instances: FxMap<ExprId, usize>,
    /// The expressions whose values some operation names.
    pub value_sites: FxSet<ExprId>,
    /// The scope of each live stack object.
    pub stack_scopes: FxMap<ObjId, ScopeId>,
    /// Immortal literals and function values.
    pub statics: Statics,
    /// The value of every `def`, once evaluated.
    pub defs: Vec<Option<Val>>,
    /// The `unsafe` byte arena.
    pub raw: RawMemory,
    /// The expansion context, when running a macro (gensym, reflection).
    pub ctx: Option<&'p ExpandCtx>,
    /// Gensyms handed out outside a macro run.
    pub gensyms: u64,
    /// The expansion error a reflection call raised, for the runner.
    pub expand_error: Option<crate::expand::ExpandError>,
    /// The input form behind each `Form` object built from a macro's
    /// arguments, so that the macro's result keeps their positions
    /// (syntax §1.3); filled while `recording_inputs`.
    pub input_forms: FxMap<ObjId, crate::syntax::Form>,
    /// Whether `Form` objects being built are a macro's input.
    pub recording_inputs: bool,
    /// The weak box of each object that has one (§8.7: the first `weak`
    /// of an object allocates its box, later ones find it).
    pub weak_boxes: FxMap<ObjId, ObjId>,
    /// The executor's scheduler (`sched`, `threads`).
    pub sched: Sched<'p>,
}

impl<'p> World<'p> {
    /// The world of a run of `p` with its plan `o`: an empty heap.
    pub fn new(p: &'p TypedProgram, o: &'p OwnedProgram) -> Self {
        World {
            p,
            o,
            heap: Heap::new(),
            objs: Objects::default(),
            plans: Plans::new(o),
            lits: literals(p),
            instances: super::plan::instances(p),
            value_sites: value_sites(o),
            stack_scopes: FxMap::default(),
            statics: Statics::default(),
            defs: vec![None; p.globals.defs.len()],
            raw: RawMemory::default(),
            ctx: None,
            gensyms: 0,
            expand_error: None,
            input_forms: FxMap::default(),
            recording_inputs: false,
            weak_boxes: FxMap::default(),
            sched: Sched::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::World;

    /// The world moves between OS threads by value: it must be `Send`
    /// by construction, with no hand-written `Send` impl.
    #[test]
    fn the_world_is_send() {
        fn send<T: Send>() {}
        send::<Box<World<'static>>>();
    }
}
