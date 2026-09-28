//! The global tables: structs and enums, protocols, instances, `defun`s,
//! `def`s, `extern`s, macros, binding sites, and the names each module
//! binds (spec/types.md §2.7, §3.5 step 3; syntax §5).
//!
//! Three namespaces are chained: the builtins (the §4.3 names and the
//! built-in protocols of §2.9 and §2.12), the prelude `fib.prelude`, and
//! the user module. A name defined in a module shadows one it `:use`s
//! (syntax §5), so user names shadow prelude names, which shadow
//! builtins; `fib.prelude/x` skips the user module (syntax §1.4).

use std::collections::HashMap;

use crate::syntax::Pos;

use super::ast::{BindingId, BindingInfo, DefId, Expr, ExprId, ExternId, FunId, TypeAnn};
use super::scheme::Scheme;
use super::ty::{Con, Pred, ProtoId, Ty, TypeId};

pub use super::names::{Names, Space};

/// The three modules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModuleId {
    /// The builtin names of syntax §4.3 and the built-in protocols.
    Builtin,
    /// `fib.prelude`, written in fibber (`lib/`).
    Prelude,
    /// The program's module.
    User,
}

impl ModuleId {
    /// The module's name, for messages.
    pub fn name(self) -> &'static str {
        match self {
            ModuleId::Builtin => "the builtins",
            ModuleId::Prelude => crate::expand::PRELUDE_NS,
            ModuleId::User => "the program's module",
        }
    }

    fn index(self) -> usize {
        match self {
            ModuleId::Builtin => 0,
            ModuleId::Prelude => 1,
            ModuleId::User => 2,
        }
    }
}

/// A struct field or a variant field. Its type mentions the
/// definition's parameters as `Ty::Gen(i)`.
#[derive(Clone, Debug)]
pub struct FieldDef {
    /// The field's name; a positional variant field is named by its
    /// index.
    pub name: String,
    /// Its type over the definition's parameters.
    pub ty: Ty,
}

/// One variant of an enum.
#[derive(Clone, Debug)]
pub struct VariantDef {
    /// The variant's name.
    pub name: String,
    /// Its fields in order.
    pub fields: Vec<FieldDef>,
}

/// A struct's fields or an enum's variants.
#[derive(Clone, Debug)]
pub enum Shape {
    /// A struct.
    Struct(Vec<FieldDef>),
    /// An enum.
    Enum(Vec<VariantDef>),
}

/// A `defstruct` or `defenum` (or the built-in `Option` and `Form`).
#[derive(Clone, Debug)]
pub struct TypeDef {
    /// The name.
    pub name: String,
    /// Which module defines it.
    pub module: ModuleId,
    /// The parameters in order.
    pub params: Vec<String>,
    /// Which parameters are colour parameters (`k :colour`, §1.3); empty
    /// when none is.
    pub colours: Vec<bool>,
    /// Fields or variants (empty until the declarations are resolved).
    pub shape: Shape,
    /// Where it is defined.
    pub pos: Pos,
}

impl TypeDef {
    /// Whether parameter `i` is a colour parameter.
    pub fn is_colour(&self, i: usize) -> bool {
        self.colours.get(i).copied().unwrap_or(false)
    }

    /// The definition applied to its own parameters: `Gen(i)`, or the
    /// colour argument `Colour::Gen(i)` at a colour parameter (§1.3).
    pub fn head_args(&self) -> Vec<Ty> {
        (0..self.params.len())
            .map(|i| match self.is_colour(i) {
                true => Ty::colour_arg(super::ty::Colour::Gen(i as u32)),
                false => Ty::Gen(i as u32),
            })
            .collect()
    }

    /// Whether this is an enum whose variants all have no fields: a
    /// scalar (§1, §2.12).
    pub fn is_fieldless_enum(&self) -> bool {
        match &self.shape {
            Shape::Enum(vs) => vs.iter().all(|v| v.fields.is_empty()),
            Shape::Struct(_) => false,
        }
    }
}

/// A protocol method parameter with its declared kinds (syntax §3.10).
#[derive(Clone, Debug)]
pub struct MethodParam {
    /// The name (`self` first).
    pub name: String,
    /// Declared `:borrow` (does not escape).
    pub borrow: bool,
    /// Declared `:owned` (count handed over).
    pub owned: bool,
}

/// One method of a protocol.
#[derive(Clone, Debug)]
pub struct MethodDef {
    /// The name.
    pub name: String,
    /// The parameters, `self` first.
    pub params: Vec<MethodParam>,
    /// `∀ s d̄ b̄. (P s d̄) ⇒ (fn :send (s T..) R)`; `Gen(0)` is `s`,
    /// `Gen(1..=k)` the determined parameters.
    pub scheme: Scheme,
    /// Whether `Self` occurs anywhere but the receiver (then the method
    /// is not callable through `dyn`, §4.4).
    pub self_elsewhere: bool,
    /// The default (§4.1): the whole method form, signature and body,
    /// expanded, when the signature is followed by a body.
    pub default: Option<crate::syntax::Form>,
    /// Where it is declared.
    pub pos: Pos,
}

/// A `defprotocol`.
#[derive(Clone, Debug)]
pub struct ProtoDef {
    /// The name.
    pub name: String,
    /// Which module defines it.
    pub module: ModuleId,
    /// The dispatch parameter and the determined ones.
    pub params: Vec<String>,
    /// The methods.
    pub methods: Vec<MethodDef>,
    /// The supertraits (§4.1): `(Q s ē)` over the protocol's parameters
    /// as `Gen(0)` (`s`) and `Gen(1..)` (`d̄`).
    pub supers: Vec<Pred>,
    /// Where it is declared.
    pub pos: Pos,
}

/// A method body of an `impl`.
#[derive(Clone, Debug)]
pub struct ImplMethod {
    /// The method's index in its protocol.
    pub index: usize,
    /// The parameters, `self` first.
    pub params: Vec<BindingId>,
    /// The result annotation, if written.
    pub ret: Option<TypeAnn>,
    /// The body.
    pub body: Expr,
    /// Where it is.
    pub pos: Pos,
}

/// An instance `∀ā. (P (K ā) D̄) ⇐ C` (§2.7, §4.1).
#[derive(Clone, Debug)]
pub struct InstanceDef {
    /// The protocol.
    pub proto: ProtoId,
    /// The head constructor `K`: the instance's key with `proto`.
    pub con: Con,
    /// Which module declares it (`Builtin` for the built-in instances).
    pub module: ModuleId,
    /// The names of the instance's variables `ā` (`Gen(i)` below).
    pub var_names: Vec<String>,
    /// `(K ā)`.
    pub head: Ty,
    /// `D̄`.
    pub dets: Vec<Ty>,
    /// The declared context `C`.
    pub context: Vec<Pred>,
    /// The method bodies (none for a built-in instance).
    pub methods: Vec<ImplMethod>,
    /// Where it is declared.
    pub pos: Pos,
}

/// A `defun` parameter.
#[derive(Clone, Debug)]
pub struct ParamDecl {
    /// Its binding.
    pub binding: BindingId,
    /// Its name.
    pub name: String,
    /// `&v`.
    pub amp: bool,
    /// Its annotation.
    pub ann: Option<TypeAnn>,
    /// Declared `:borrow`.
    pub borrow: bool,
}

/// A bound written in `:where`.
#[derive(Clone, Debug)]
pub enum PredAnn {
    /// `(P T..)`.
    Proto(ProtoId, Vec<TypeAnn>),
    /// `(Send T)`.
    Send(TypeAnn),
}

/// A `defun` (or a `defmacro`, typed as one over `Form`, §2.8).
#[derive(Clone, Debug)]
pub struct FunDef {
    /// The name.
    pub name: String,
    /// Which module defines it.
    pub module: ModuleId,
    /// The parameters.
    pub params: Vec<ParamDecl>,
    /// The result annotation.
    pub ret: Option<TypeAnn>,
    /// The `:where` bounds.
    pub bounds: Vec<PredAnn>,
    /// The body.
    pub body: Expr,
    /// Whether this is a `defmacro` (not a value, not callable).
    pub is_macro: bool,
    /// Where it is defined.
    pub pos: Pos,
}

/// A `def`.
#[derive(Clone, Debug)]
pub struct DefDef {
    /// The name.
    pub name: String,
    /// Which module defines it.
    pub module: ModuleId,
    /// The annotation.
    pub ann: Option<TypeAnn>,
    /// The initialiser, a constant expression.
    pub init: Expr,
    /// Where it is defined.
    pub pos: Pos,
}

/// An `extern`.
#[derive(Clone, Debug)]
pub struct ExternDef {
    /// The C name.
    pub name: String,
    /// `(fn :send (T̄) R)`.
    pub ty: Ty,
    /// `:varargs`.
    pub varargs: bool,
    /// Where it is declared.
    pub pos: Pos,
}

/// Every global table.
#[derive(Clone, Debug)]
pub struct Globals {
    /// Structs and enums.
    pub types: Vec<TypeDef>,
    /// Protocols.
    pub protos: Vec<ProtoDef>,
    /// Instances, built-in ones included.
    pub instances: Vec<InstanceDef>,
    /// `defun`s and `defmacro`s.
    pub funs: Vec<FunDef>,
    /// `def`s.
    pub defs: Vec<DefDef>,
    /// `extern`s.
    pub externs: Vec<ExternDef>,
    /// The scheme of each builtin function, by `BuiltinId`.
    pub builtin_schemes: Vec<Scheme>,
    /// Every binding site, by `BindingId`.
    pub bindings: Vec<BindingInfo>,
    /// The names of each module.
    pub modules: [Names; 3],
    /// Instance by key.
    pub instance_index: HashMap<(ProtoId, Con), usize>,
    /// The built-in `Option`.
    pub option: TypeId,
    /// The built-in `Form`, once declared.
    pub form: Option<TypeId>,
    /// The prelude's `Vec`, once declared (`Form`'s items, `concat`).
    pub vec: Option<TypeId>,
    /// The built-in `Deref` protocol, once declared.
    pub deref_proto: Option<ProtoId>,
    /// The number of expressions made so far.
    pub expr_count: u32,
}

impl Globals {
    /// The names of `m`.
    pub fn names(&self, m: ModuleId) -> &Names {
        &self.modules[m.index()]
    }

    /// The names of `m`, for adding to.
    pub fn names_mut(&mut self, m: ModuleId) -> &mut Names {
        &mut self.modules[m.index()]
    }

    /// The definition of a nominal type.
    pub fn ty(&self, id: TypeId) -> &TypeDef {
        &self.types[id.0 as usize]
    }

    /// The definition of a protocol.
    pub fn proto(&self, id: ProtoId) -> &ProtoDef {
        &self.protos[id.0 as usize]
    }

    /// The binding site `b`.
    pub fn binding(&self, b: BindingId) -> &BindingInfo {
        &self.bindings[b.0 as usize]
    }

    /// The instance for `(proto, con)`, if any.
    pub fn instance(&self, proto: ProtoId, con: Con) -> Option<&InstanceDef> {
        let i = self.instance_index.get(&(proto, con))?;
        self.instances.get(*i)
    }

    /// The name of a definition, for messages.
    pub fn fun(&self, f: FunId) -> &FunDef {
        &self.funs[f.0 as usize]
    }

    /// A `def`.
    pub fn def(&self, d: DefId) -> &DefDef {
        &self.defs[d.0 as usize]
    }

    /// An `extern`.
    pub fn ext(&self, x: ExternId) -> &ExternDef {
        &self.externs[x.0 as usize]
    }

    /// A fresh expression id.
    pub fn next_expr(&mut self) -> ExprId {
        self.expr_count += 1;
        ExprId(self.expr_count - 1)
    }
}
