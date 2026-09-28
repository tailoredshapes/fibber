//! The lIR abstract syntax (spec/lir.md §3 to §7).

use crate::diag::Pos;
use crate::types::{FnType, Type};

/// A whole module, items in source order.
#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Struct(StructDef),
    Global(GlobalDef),
    Declare(Declare),
    Define(Function),
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<Type>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlobalDef {
    pub name: String,
    pub ty: Type,
    pub init: Expr,
    pub constant: bool,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Declare {
    pub name: String,
    pub ty: FnType,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub ty: FnType,
    pub params: Vec<(String, Pos)>,
    pub blocks: Vec<Block>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub label: String,
    pub body: Vec<Expr>,
    pub pos: Pos,
}

/// An expression with the position of its form.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: Kind,
    pub pos: Pos,
}

/// A binding `(NAME value)` of a `let`, or a phi's `(LABEL value)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub name: String,
    pub value: Expr,
    pub pos: Pos,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    SDiv,
    UDiv,
    SRem,
    URem,
    FAdd,
    FSub,
    FMul,
    FDiv,
    FRem,
    And,
    Or,
    Xor,
    Shl,
    LShr,
    AShr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    FNeg,
    Ctpop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastOp {
    Trunc,
    ZExt,
    SExt,
    FpTrunc,
    FpExt,
    FpToSi,
    FpToUi,
    SiToFp,
    UiToFp,
    PtrToInt,
    IntToPtr,
    Bitcast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IPred {
    Eq,
    Ne,
    Slt,
    Sle,
    Sgt,
    Sge,
    Ult,
    Ule,
    Ugt,
    Uge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FPred {
    Oeq,
    One,
    Olt,
    Ole,
    Ogt,
    Oge,
    Ord,
    Ueq,
    Une,
    Ult,
    Ule,
    Ugt,
    Uge,
    Uno,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ordering {
    Unordered,
    Monotonic,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RmwOp {
    Xchg,
    Add,
    Sub,
    And,
    Nand,
    Or,
    Xor,
    Max,
    Min,
    UMax,
    UMin,
    FAdd,
    FSub,
    FMax,
    FMin,
}

/// Synchronisation scope (spec/lir.md §6.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    System,
    SingleThread,
}

/// The callee of a call.
#[derive(Clone, Debug, PartialEq)]
pub enum Callee {
    Direct(String),
    Indirect(Box<Expr>, FnType),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Local(String),
    Global(String),
    Int(Type, i128),
    Float(Type, f64),
    Null,
    Vector(Type, Vec<Expr>),
    Str(Vec<u8>),
    /// `{ … }` (None) or `(%struct.S …)`
    Struct(Option<String>, Vec<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Un(UnOp, Box<Expr>),
    ICmp(IPred, Box<Expr>, Box<Expr>),
    FCmp(FPred, Box<Expr>, Box<Expr>),
    Cast(CastOp, Type, Box<Expr>),
    Select(Box<Expr>, Box<Expr>, Box<Expr>),
    ExtractElement(Box<Expr>, Box<Expr>),
    InsertElement(Box<Expr>, Box<Expr>, Box<Expr>),
    Shuffle(Box<Expr>, Box<Expr>, Box<Expr>),
    ExtractValue(Box<Expr>, Vec<i128>),
    InsertValue(Box<Expr>, Box<Expr>, Vec<i128>),
    Alloca(Type, Option<Box<Expr>>),
    Load(Type, Box<Expr>),
    Store(Box<Expr>, Box<Expr>),
    Gep {
        inbounds: bool,
        ty: Type,
        ptr: Box<Expr>,
        indices: Vec<Expr>,
    },
    AtomicLoad(Scope, Ordering, Type, Box<Expr>),
    AtomicStore(Scope, Ordering, Box<Expr>, Box<Expr>),
    AtomicRmw(RmwOp, Scope, Ordering, Box<Expr>, Box<Expr>),
    CmpXchg {
        weak: bool,
        scope: Scope,
        success: Ordering,
        failure: Ordering,
        ptr: Box<Expr>,
        expected: Box<Expr>,
        new: Box<Expr>,
    },
    Fence(Scope, Ordering),
    Call {
        callee: Callee,
        args: Vec<Expr>,
        tail: bool,
    },
    Ret(Option<Box<Expr>>),
    Br(String),
    CondBr(Box<Expr>, String, String),
    Switch(Box<Expr>, String, Vec<(Expr, String)>),
    Unreachable,
    Phi(Type, Vec<Binding>),
    Let(Vec<Binding>, Vec<Expr>),
}

impl Kind {
    /// Whether this form ends a block (spec/lir.md §5.1).
    pub fn is_terminator(&self) -> bool {
        match self {
            Kind::Ret(_)
            | Kind::Br(_)
            | Kind::CondBr(..)
            | Kind::Switch(..)
            | Kind::Unreachable => true,
            Kind::Call { tail, .. } => *tail,
            _ => false,
        }
    }
}

impl Expr {
    pub fn new(kind: Kind, pos: Pos) -> Self {
        Expr { kind, pos }
    }

    /// A literal integer written in place, as the checker's constant
    /// rules mean it (spec/lir.md §6.11).
    pub fn int_literal(&self) -> Option<i128> {
        match &self.kind {
            Kind::Int(_, v) => Some(*v),
            _ => None,
        }
    }
}

impl Function {
    pub fn is_variadic(&self) -> bool {
        self.ty.varargs
    }
}
