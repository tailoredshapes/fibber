//! The types the generator builds programs over: a fixed universe of
//! scalars, the preamble's structs and enum, and the library's generic
//! types applied to them (spec/types.md §1).

use std::fmt;

/// A fibber type the generator can produce an expression of.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    /// `unit`.
    Unit,
    /// `i64`.
    Int,
    /// `bool`.
    Bool,
    /// `str`.
    Str,
    /// `(defstruct Pt (x: i64 y: i64))`.
    Pt,
    /// `(defstruct Wrap (s: str v: (Vec i64)))`.
    Wrap,
    /// `(defstruct Holder (f: (fn (i64) i64) c: (Cell i64)))`.
    Holder,
    /// `(defenum Shape (Circle r: i64) (Rect a: Pt b: Pt) (Named n: str w: Wrap))`.
    Shape,
    /// The prelude's `(Box a)`.
    Boxed(Box<Ty>),
    /// `(Option a)`.
    Opt(Box<Ty>),
    /// The prelude's `(Vec a)`.
    Vec(Box<Ty>),
    /// `(List i64)`.
    List,
    /// `(Cell a)`.
    Cell(Box<Ty>),
    /// `(Atom a)`.
    Atom(Box<Ty>),
    /// `(Weak a)`.
    Weak(Box<Ty>),
    /// `(Task a)`.
    Task(Box<Ty>),
    /// A function type `(fn (params) ret)`: `fn_i()` is `(fn (i64) i64)`,
    /// `fn_0()` is `(fn () i64)`.
    Func(Vec<Ty>, Box<Ty>),
    /// `(dyn P)`, or `(dyn P :send)` when the flag is set (types §2.15).
    Dyn(Proto, bool),
    /// `(defstruct (Hook k :colour) (f: (fn k (i64) i64) tag: i64))` at
    /// `:send` (flag set) or `:local` (types §1.3).
    Hook(bool),
    /// `(defenum (Job k :colour) (Idle) (Ready run: (fn k () i64)))` at
    /// `:send` (flag set) or `:local`: a colour-parameterised enum.
    Job(bool),
    /// A number type other than `i64` (types §1.1).
    Num(NumTy),
    /// `(Array a)` (types §2.13).
    Array(Box<Ty>),
    /// A preamble type with derived `Eq` and `Ord`: `Ver` (a struct)
    /// or `Lvl` (an enum with a field-less variant).
    Derived(&'static str),
    /// The type variable `a` of a protocol-bounded generic helper; the
    /// flag says whether the bound is written as `:where ((P a))` or
    /// left to inference. Only ever a parameter's type.
    Gen(Proto, bool),
}

/// The integer widths other than `i64`, and the float types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumTy {
    /// `i8`.
    I8,
    /// `i16`.
    I16,
    /// `i32`.
    I32,
    /// `f32`.
    F32,
    /// `f64`.
    F64,
}

impl NumTy {
    /// The type's name, also a literal's suffix.
    pub fn name(self) -> &'static str {
        match self {
            NumTy::I8 => "i8",
            NumTy::I16 => "i16",
            NumTy::I32 => "i32",
            NumTy::F32 => "f32",
            NumTy::F64 => "f64",
        }
    }

    /// The width in bits.
    pub fn bits(self) -> u32 {
        match self {
            NumTy::I8 => 8,
            NumTy::I16 => 16,
            NumTy::I32 | NumTy::F32 => 32,
            NumTy::F64 => 64,
        }
    }

    /// Whether it is a float type.
    pub fn is_float(self) -> bool {
        matches!(self, NumTy::F32 | NumTy::F64)
    }
}

/// The preamble's protocols (syntax §3.10): `Score` has a default
/// method, `Rank` requires `Score` and has a default of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Proto {
    /// `(defprotocol Score (score (self) -> i64) (bonus (self k: i64) -> i64 ..))`.
    Score,
    /// `(defprotocol Rank :requires (Score) (rank (self) -> i64) (tier (self) -> i64 ..))`.
    Rank,
}

impl Proto {
    /// The protocol's name.
    pub fn name(self) -> &'static str {
        match self {
            Proto::Score => "Score",
            Proto::Rank => "Rank",
        }
    }

    /// The methods callable on a value bounded by (or a `dyn` of) this
    /// protocol: its own and its supertraits' (types §4.1 rules 2, 3).
    pub fn methods(self) -> &'static [&'static str] {
        match self {
            Proto::Score => &["score", "bonus"],
            Proto::Rank => &["score", "bonus", "rank", "tier"],
        }
    }

    /// Whether a bound `(self t)` entails `(other t)`.
    pub fn entails(self, other: Proto) -> bool {
        self == other || (self == Proto::Rank && other == Proto::Score)
    }
}

impl Ty {
    /// `(Box t)`.
    pub fn boxed(t: Ty) -> Ty {
        Ty::Boxed(Box::new(t))
    }
    /// `(Option t)`.
    pub fn opt(t: Ty) -> Ty {
        Ty::Opt(Box::new(t))
    }
    /// `(Vec t)`.
    pub fn vec(t: Ty) -> Ty {
        Ty::Vec(Box::new(t))
    }
    /// `(Cell t)`.
    pub fn cell(t: Ty) -> Ty {
        Ty::Cell(Box::new(t))
    }
    /// `(Atom t)`.
    pub fn atom(t: Ty) -> Ty {
        Ty::Atom(Box::new(t))
    }
    /// `(Weak t)`.
    pub fn weak(t: Ty) -> Ty {
        Ty::Weak(Box::new(t))
    }
    /// `(Task t)`.
    pub fn task(t: Ty) -> Ty {
        Ty::Task(Box::new(t))
    }

    /// `(fn (i64) i64)`.
    pub fn fn_i() -> Ty {
        Ty::Func(vec![Ty::Int], Box::new(Ty::Int))
    }
    /// `(fn () i64)`.
    pub fn fn_0() -> Ty {
        Ty::Func(Vec::new(), Box::new(Ty::Int))
    }
    /// `(dyn p)` or `(dyn p :send)`.
    pub fn dyn_of(p: Proto, send: bool) -> Ty {
        Ty::Dyn(p, send)
    }
    /// Whether this is a function type.
    pub fn is_fn(&self) -> bool {
        matches!(self, Ty::Func(..))
    }

    /// The argument of a one-parameter type constructor.
    pub fn inner(&self) -> Option<&Ty> {
        match self {
            Ty::Boxed(t)
            | Ty::Opt(t)
            | Ty::Vec(t)
            | Ty::Cell(t)
            | Ty::Atom(t)
            | Ty::Weak(t)
            | Ty::Array(t)
            | Ty::Task(t) => Some(t),
            _ => None,
        }
    }

    /// Whether values of this type may cross a thread (types §5.1).
    /// Function types answer false: their colour depends on captures,
    /// which the generator tracks per variable instead.
    pub fn is_send(&self) -> bool {
        match self {
            Ty::Holder | Ty::Cell(_) | Ty::Func(..) | Ty::Gen(..) => false,
            Ty::Dyn(_, s) | Ty::Hook(s) | Ty::Job(s) => *s,
            Ty::Boxed(t) | Ty::Opt(t) | Ty::Vec(t) | Ty::Weak(t) | Ty::Task(t) | Ty::Array(t) => {
                t.is_send()
            }
            _ => true,
        }
    }

    /// Whether a value of this type may hold a strong reference to a
    /// cell (directly, or through a closure's captures). Storing such a
    /// value in a cell could close a cycle (ownership.md §6), which the
    /// generator avoids so that every program's audit must be clean.
    pub fn may_reach_cell(&self) -> bool {
        match self {
            Ty::Holder | Ty::Cell(_) | Ty::Func(..) | Ty::Gen(..) => true,
            Ty::Dyn(_, s) | Ty::Hook(s) | Ty::Job(s) => !*s,
            Ty::Boxed(t) | Ty::Opt(t) | Ty::Vec(t) | Ty::Array(t) => t.may_reach_cell(),
            _ => false,
        }
    }

    /// Whether the type is an atom.
    pub fn is_atom(&self) -> bool {
        matches!(self, Ty::Atom(_))
    }

    /// Whether the type is a `dyn`, or a vector, weak reference or atom
    /// of one: it exists only in programs that declare the protocols.
    pub fn needs_protocols(&self) -> bool {
        match self {
            Ty::Dyn(..) | Ty::Gen(..) => true,
            Ty::Vec(t) | Ty::Weak(t) | Ty::Atom(t) => t.needs_protocols(),
            _ => false,
        }
    }

    /// Whether `weak` accepts the type (an object type, types §2.11).
    pub fn is_object(&self) -> bool {
        !matches!(self, Ty::Unit | Ty::Int | Ty::Bool | Ty::Num(_))
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Unit => f.write_str("unit"),
            Ty::Int => f.write_str("i64"),
            Ty::Bool => f.write_str("bool"),
            Ty::Str => f.write_str("str"),
            Ty::Pt => f.write_str("Pt"),
            Ty::Wrap => f.write_str("Wrap"),
            Ty::Holder => f.write_str("Holder"),
            Ty::Shape => f.write_str("Shape"),
            Ty::Boxed(t) => write!(f, "(Box {t})"),
            Ty::Opt(t) => write!(f, "(Option {t})"),
            Ty::Vec(t) => write!(f, "(Vec {t})"),
            Ty::List => f.write_str("(List i64)"),
            Ty::Cell(t) => write!(f, "(Cell {t})"),
            Ty::Atom(t) => write!(f, "(Atom {t})"),
            Ty::Weak(t) => write!(f, "(Weak {t})"),
            Ty::Task(t) => write!(f, "(Task {t})"),
            Ty::Func(ps, r) => {
                let ps: Vec<String> = ps.iter().map(|p| p.to_string()).collect();
                write!(f, "(fn ({}) {r})", ps.join(" "))
            }
            Ty::Dyn(p, true) => write!(f, "(dyn {} :send)", p.name()),
            Ty::Dyn(p, false) => write!(f, "(dyn {})", p.name()),
            Ty::Hook(true) => f.write_str("(Hook :send)"),
            Ty::Hook(false) => f.write_str("(Hook :local)"),
            Ty::Job(true) => f.write_str("(Job :send)"),
            Ty::Job(false) => f.write_str("(Job :local)"),
            Ty::Gen(..) => f.write_str("a"),
            Ty::Num(n) => f.write_str(n.name()),
            Ty::Array(t) => write!(f, "(Array {t})"),
            Ty::Derived(n) => f.write_str(n),
        }
    }
}

/// The types a `let` may bind, a helper may take or return, and a
/// program folds into its result.
pub fn universe() -> Vec<Ty> {
    vec![
        Ty::Int,
        Ty::Bool,
        Ty::Str,
        Ty::Pt,
        Ty::Wrap,
        Ty::Holder,
        Ty::Shape,
        Ty::boxed(Ty::Str),
        Ty::boxed(Ty::vec(Ty::Int)),
        Ty::boxed(Ty::Wrap),
        Ty::boxed(Ty::fn_i()),
        Ty::opt(Ty::Int),
        Ty::opt(Ty::Str),
        Ty::opt(Ty::Wrap),
        Ty::vec(Ty::Int),
        Ty::vec(Ty::Str),
        Ty::vec(Ty::Wrap),
        Ty::vec(Ty::fn_i()),
        Ty::List,
        Ty::cell(Ty::Int),
        Ty::cell(Ty::vec(Ty::Int)),
        Ty::cell(Ty::Str),
        Ty::cell(Ty::fn_i()),
        Ty::cell(Ty::Pt),
        Ty::cell(Ty::Wrap),
        Ty::atom(Ty::Int),
        Ty::atom(Ty::vec(Ty::Int)),
        Ty::fn_i(),
        Ty::fn_0(),
        Ty::task(Ty::Int),
        Ty::task(Ty::Str),
        Ty::Hook(true),
        Ty::Hook(false),
        Ty::Dyn(Proto::Score, false),
        Ty::Dyn(Proto::Score, true),
        Ty::Dyn(Proto::Rank, false),
        Ty::Dyn(Proto::Rank, true),
        Ty::vec(Ty::Dyn(Proto::Score, false)),
        Ty::vec(Ty::Dyn(Proto::Score, true)),
        Ty::Job(true),
        Ty::Job(false),
        Ty::weak(Ty::Dyn(Proto::Score, true)),
        Ty::atom(Ty::Dyn(Proto::Rank, true)),
    ]
}

/// The content types an `&` parameter may have.
pub fn inout_contents() -> Vec<Ty> {
    vec![Ty::Int, Ty::Str, Ty::vec(Ty::Int), Ty::Pt, Ty::Wrap]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_type_syntax() {
        assert_eq!(Ty::cell(Ty::vec(Ty::Int)).to_string(), "(Cell (Vec i64))");
        assert_eq!(Ty::boxed(Ty::fn_i()).to_string(), "(Box (fn (i64) i64))");
    }

    #[test]
    fn send_follows_types_5_1() {
        assert!(Ty::vec(Ty::Wrap).is_send());
        assert!(Ty::atom(Ty::Int).is_send());
        assert!(!Ty::cell(Ty::Int).is_send());
        assert!(!Ty::Holder.is_send());
        assert!(!Ty::boxed(Ty::fn_i()).is_send());
        assert!(Ty::vec(Ty::Dyn(Proto::Score, true)).is_send());
        assert!(!Ty::Dyn(Proto::Rank, false).is_send());
        assert!(Ty::Hook(true).is_send() && !Ty::Hook(false).is_send());
        assert!(Ty::Job(true).is_send() && !Ty::Job(false).is_send());
        assert!(Ty::weak(Ty::Dyn(Proto::Score, true)).is_send());
        assert!(Ty::atom(Ty::Dyn(Proto::Rank, true)).needs_protocols());
    }

    #[test]
    fn prints_dyn_and_colour_arguments() {
        assert_eq!(Ty::Dyn(Proto::Score, true).to_string(), "(dyn Score :send)");
        assert_eq!(Ty::vec(Ty::Hook(false)).to_string(), "(Vec (Hook :local))");
        assert!(Proto::Rank.entails(Proto::Score) && !Proto::Score.entails(Proto::Rank));
    }

    #[test]
    fn cell_reach_is_conservative_for_closures() {
        assert!(Ty::vec(Ty::fn_i()).may_reach_cell());
        assert!(!Ty::vec(Ty::Wrap).may_reach_cell());
        assert!(!Ty::weak(Ty::Wrap).may_reach_cell());
    }
}
