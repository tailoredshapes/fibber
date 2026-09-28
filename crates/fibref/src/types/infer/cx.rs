//! The inference state (spec/types.md §3.1): the substitution, the
//! worklist of deferred constraints, the colour constraints, and the
//! per-unit bookkeeping that generalisation and the typed program need.

use std::collections::HashMap;

use crate::syntax::Pos;

use crate::types::ast::{BindingId, ExprId, FunId};
use crate::types::decls::Globals;
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::scheme::Scheme;
use crate::types::store::Store;
use crate::types::ty::{Colour, Pred, ProtoId, Ty};

/// The schemes and `def` types computed so far (the global part of Γ).
#[derive(Clone, Debug, Default)]
pub struct Env {
    /// By `FunId`; `None` until the function's SCC is generalised.
    pub funs: Vec<Option<Scheme>>,
    /// By `DefId`; `None` until the `def` is typed.
    pub defs: Vec<Option<Ty>>,
}

/// How a scheme was instantiated at a use: the arguments of its
/// quantified variables, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instantiation {
    /// One type per quantified type variable.
    pub tys: Vec<Ty>,
    /// One colour per quantified colour variable.
    pub colours: Vec<Colour>,
}

/// How a protocol constraint at a method use was discharged (§4.1–§4.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// By instance `index` of the global instance table, whose variables
    /// were instantiated at `args`.
    Instance {
        /// Index into `Globals::instances`.
        index: usize,
        /// One type per instance variable.
        args: Vec<Ty>,
    },
    /// By a bound of the enclosing scheme (or of the `impl`'s declared
    /// context): the constraint, over that scheme's `Gen` variables.
    /// Monomorphisation resolves it per specialisation (§4.3).
    Bound(Pred),
    /// Through a `(dyn P)` receiver: a vtable call (§4.4).
    Dyn,
}

/// The typed program's tables, filled unit by unit.
#[derive(Clone, Debug, Default)]
pub struct Tables {
    /// The type of every expression.
    pub expr_types: HashMap<ExprId, Ty>,
    /// The type of every binding site.
    pub binding_types: HashMap<BindingId, Ty>,
    /// For every use of a global with a scheme: its instantiation.
    pub instantiations: HashMap<ExprId, Instantiation>,
    /// For every protocol method use: how its dispatch constraint was
    /// discharged.
    pub resolutions: HashMap<ExprId, Resolution>,
    /// The colour of every `fn` literal (`Send`, `Local`, or a
    /// quantified `Gen` of the enclosing scheme).
    pub fn_colours: HashMap<ExprId, Colour>,
}

/// A deferred constraint (§3.3) with where it came from.
#[derive(Clone, Debug)]
pub struct Deferred {
    /// The constraint.
    pub kind: DKind,
    /// The form whose rule emitted it.
    pub pos: Pos,
    /// The method use it is the dispatch constraint of, if any.
    pub site: Option<ExprId>,
    /// The definition being checked, for messages.
    pub fun: String,
}

/// The deferred constraints.
#[derive(Clone, Debug)]
pub enum DKind {
    /// `(P T₁ ..)`; `method` is the method index when this is the
    /// dispatch constraint of a method use.
    Proto(ProtoId, Vec<Ty>, Option<usize>),
    /// `(Send T)`, with the path from the requiring position.
    Send(Ty, Vec<String>),
    /// `(Object T)`; the text names what requires it (`dyn`, `raw`, a
    /// function whose scheme has the bound), for the message.
    Object(Ty, String),
    /// `(Weakable T)`: the operand of `weak` (§2.11).
    Weakable(Ty),
    /// `HasField(T, f, R)`; `text` is the printed receiver.
    Field(Ty, String, Ty, String),
    /// `HasDeref(T, R)`; `text` is the printed operand.
    Deref(Ty, Ty, String),
    /// `T` is a float type (the operand of `fptrunc` and the like).
    Float(Ty),
}

/// Why `Send` failed (§5.3): the path to the offending type.
#[derive(Clone, Debug)]
pub struct Witness {
    /// Closure captures, fields, payloads, elements, outermost first.
    pub path: Vec<String>,
    /// The offending type (zonked): a `Cell`, `ptr`, `dyn`, or a
    /// `local` function type.
    pub offending: Ty,
}

/// `κ_from ⊑ κ_to` (§5.4).
#[derive(Clone, Debug)]
pub struct ColourCon {
    /// The flowing colour.
    pub from: Colour,
    /// The receiving colour.
    pub to: Colour,
    /// A path step for witnesses (a capture), if any.
    pub label: Option<String>,
    /// When `from` is `local` by a capture or an annotation: why.
    pub origin: Option<Witness>,
    /// Where it was emitted.
    pub pos: Pos,
}

/// `ς ⊒ Caps{T}` for one capture (§5.4).
#[derive(Clone, Debug)]
pub struct CapsCon {
    /// The closure's colour.
    pub colour: Colour,
    /// The capture's type.
    pub ty: Ty,
    /// `closure capture x`.
    pub label: String,
    /// The `fn` form.
    pub pos: Pos,
}

/// A monomorphic signature of a function whose SCC is being checked.
#[derive(Clone, Debug)]
pub struct MonoSig {
    /// Parameter value types.
    pub params: Vec<Ty>,
    /// Which parameters are `&`.
    pub amps: Vec<bool>,
    /// Parameter names.
    pub names: Vec<String>,
    /// Result type.
    pub ret: Ty,
}

/// The state of one unit (an SCC of `defun`s, a `def`, an `impl`
/// method, a macro).
#[derive(Clone, Debug, Default)]
pub struct Unit {
    /// The worklist.
    pub deferred: Vec<Deferred>,
    /// Colour flows.
    pub colours: Vec<ColourCon>,
    /// Capture constraints.
    pub caps: Vec<CapsCon>,
    /// Names of the rigid variables.
    pub rigid_names: Vec<String>,
    /// The current definition's variable names to rigid indices.
    pub rigid_map: HashMap<String, u32>,
    /// The current definition's named colour variables (§1.3).
    pub rigid_colours: HashMap<String, Colour>,
    /// The definition being checked.
    pub fun: String,
    /// The members of the SCC, monomorphic.
    pub mono: HashMap<FunId, MonoSig>,
    /// Fully annotated members: their annotation as a scheme, for
    /// polymorphic recursion (§3.6).
    pub poly: HashMap<FunId, Scheme>,
    /// The members whose annotation scheme was instantiated.
    pub poly_used: std::collections::HashSet<FunId>,
    /// The variable types of the enclosing loops.
    pub loops: Vec<Vec<Ty>>,
    /// In an `impl` body: the declared context (§2.7).
    pub givens: Option<Vec<Pred>>,
    /// Expressions typed in this unit.
    pub exprs: Vec<ExprId>,
    /// Bindings typed in this unit.
    pub bindings: Vec<BindingId>,
    /// `fn` literals of this unit and their colours.
    pub fn_lits: Vec<(ExprId, Colour)>,
    /// Global uses instantiated in this unit.
    pub insts: Vec<ExprId>,
    /// Method uses whose resolution is recorded in this unit.
    pub sites: Vec<ExprId>,
}

/// The inference context of one unit.
pub struct Cx<'a> {
    /// The global tables.
    pub g: &'a Globals,
    /// Schemes so far.
    pub env: &'a Env,
    /// The substitution.
    pub st: &'a mut Store,
    /// The typed program's tables.
    pub t: &'a mut Tables,
    /// This unit.
    pub u: Unit,
}

impl Cx<'_> {
    /// A fresh unification variable.
    pub fn fresh(&mut self) -> Ty {
        self.st.fresh()
    }

    /// Records the type of an expression.
    pub fn record(&mut self, id: ExprId, ty: &Ty) {
        self.t.expr_types.insert(id, ty.clone());
        self.u.exprs.push(id);
    }

    /// Records the type of a binding.
    pub fn bind(&mut self, b: BindingId, ty: &Ty) {
        self.t.binding_types.insert(b, ty.clone());
        self.u.bindings.push(b);
    }

    /// The type of a binding (bound earlier in this unit).
    pub fn binding(&self, b: BindingId, pos: &Pos) -> TResult<Ty> {
        match self.t.binding_types.get(&b) {
            Some(t) => Ok(t.clone()),
            None => Err(TypeError::other(
                pos,
                format!("internal: {} has no type", self.g.binding(b).name),
            )),
        }
    }

    /// Adds a deferred constraint.
    pub fn defer(&mut self, kind: DKind, pos: &Pos, site: Option<ExprId>) {
        let fun = self.u.fun.clone();
        self.u.deferred.push(Deferred {
            kind,
            pos: pos.clone(),
            site,
            fun,
        });
    }

    /// A zonked type.
    pub fn zonk(&mut self, t: &Ty) -> Ty {
        self.st.zonk(t)
    }

    /// The type as text (zonked first).
    pub fn show(&mut self, t: &Ty) -> String {
        let z = self.st.zonk(t);
        crate::types::display::Printer::with_names(self.g, &[], &self.u.rigid_names).ty(&z)
    }

    /// `cannot unify a with b`.
    pub fn mismatch(&mut self, a: &Ty, b: &Ty, pos: &Pos) -> TypeError {
        let (za, zb) = (self.st.zonk(a), self.st.zonk(b));
        let p = crate::types::display::Printer::with_names(self.g, &[], &self.u.rigid_names);
        let msg = format!("cannot unify {} with {}", p.ty(&za), p.ty(&zb));
        TypeError::new(ErrorKind::Unify, pos, msg)
    }
}
