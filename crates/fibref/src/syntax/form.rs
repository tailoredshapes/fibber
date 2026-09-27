//! The `Form` type (spec/syntax.md §3.16): what the reader produces and
//! what macros receive and return.

use super::pos::Pos;

/// A form: one [`FormKind`] plus the position it was read from (§1.3).
///
/// Equality compares the data only, never the positions: two forms are
/// equal when their kinds are, recursively. §3.16's `Form` enum has no
/// position field, so a position is metadata carried beside the value;
/// this is what makes "prints and reads back to an equal form" a
/// meaningful property.
#[derive(Clone, Debug)]
pub struct Form {
    /// The datum.
    pub kind: FormKind,
    /// Where it was read, or the macro call that built it (§1.3).
    pub pos: Pos,
}

/// The variants of §3.16's built-in enum
///
/// ```text
/// (defenum Form
///   (Sym name: str) (Kw name: str)
///   (Int v: i64 width: keyword) (Flt v: f64 width: keyword)
///   (Str v: str) (Chr v: char) (Bool v: bool) (Nil)
///   (List items: (Vec Form)) (Vec items: (Vec Form)) (Map items: (Vec Form)))
/// ```
///
/// The `width: keyword` fields are the closed enums [`IntWidth`] and
/// [`FltWidth`] here, since only four and two keywords are possible. A
/// keyword's name is stored without its leading `:`.
#[derive(Clone, Debug)]
pub enum FormKind {
    /// A symbol, such as `x`, `x:`, `->`, `seq/first`.
    Sym(String),
    /// A keyword, name without the colon: `:borrow` is `Kw("borrow")`.
    Kw(String),
    /// An integer literal, already range-checked against its width.
    Int {
        /// The value.
        v: i64,
        /// The width from the suffix, `I64` without one.
        width: IntWidth,
    },
    /// A float literal. For `F32` the value is the `f32` nearest the
    /// literal, widened exactly to `f64`.
    Flt {
        /// The value, always finite.
        v: f64,
        /// The width from the suffix, `F64` without one.
        width: FltWidth,
    },
    /// A string literal, escapes decoded.
    Str(String),
    /// A character literal: one Unicode scalar value.
    Chr(char),
    /// `true` or `false`.
    Bool(bool),
    /// `nil`.
    Nil,
    /// `( ... )`, including the prefix-macro expansions of §1.2.
    List(Vec<Form>),
    /// `[ ... ]`.
    Vec(Vec<Form>),
    /// `{ ... }`: keys and values alternating, always an even count.
    Map(Vec<Form>),
}

/// The width of an integer literal (§1.1: `i8` `i16` `i32` `i64`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntWidth {
    /// `i8`
    I8,
    /// `i16`
    I16,
    /// `i32`
    I32,
    /// `i64`, the width of an unsuffixed literal.
    I64,
}

/// The width of a float literal (§1.1: `f32` `f64`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FltWidth {
    /// `f32`
    F32,
    /// `f64`, the width of an unsuffixed literal.
    F64,
}

impl IntWidth {
    /// The width named by a suffix such as `"i32"`.
    pub fn from_suffix(s: &str) -> Option<Self> {
        match s {
            "i8" => Some(IntWidth::I8),
            "i16" => Some(IntWidth::I16),
            "i32" => Some(IntWidth::I32),
            "i64" => Some(IntWidth::I64),
            _ => None,
        }
    }

    /// The suffix text, e.g. `"i32"`.
    pub fn suffix(self) -> &'static str {
        match self {
            IntWidth::I8 => "i8",
            IntWidth::I16 => "i16",
            IntWidth::I32 => "i32",
            IntWidth::I64 => "i64",
        }
    }

    /// The smallest and largest value of the width.
    pub fn range(self) -> (i128, i128) {
        let bits = match self {
            IntWidth::I8 => 8,
            IntWidth::I16 => 16,
            IntWidth::I32 => 32,
            IntWidth::I64 => 64,
        };
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    }
}

impl FltWidth {
    /// The width named by a suffix such as `"f32"`.
    pub fn from_suffix(s: &str) -> Option<Self> {
        match s {
            "f32" => Some(FltWidth::F32),
            "f64" => Some(FltWidth::F64),
            _ => None,
        }
    }

    /// The suffix text, e.g. `"f32"`.
    pub fn suffix(self) -> &'static str {
        match self {
            FltWidth::F32 => "f32",
            FltWidth::F64 => "f64",
        }
    }
}

impl Form {
    /// A form with the given kind and position.
    pub fn new(kind: FormKind, pos: Pos) -> Self {
        Form { kind, pos }
    }

    /// The items of a `List`, if this is one.
    pub fn as_list(&self) -> Option<&[Form]> {
        match &self.kind {
            FormKind::List(items) => Some(items),
            _ => None,
        }
    }

    /// The name of a `Sym`, if this is one.
    pub fn as_sym(&self) -> Option<&str> {
        match &self.kind {
            FormKind::Sym(name) => Some(name),
            _ => None,
        }
    }
}

impl PartialEq for Form {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Eq for Form {}

/// Structural equality. Floats compare by bit pattern, so `-0.0` and
/// `0.0` are different forms (they print differently) and the relation
/// is a true equivalence.
impl PartialEq for FormKind {
    fn eq(&self, other: &Self) -> bool {
        use FormKind as K;
        match (self, other) {
            (K::Sym(a), K::Sym(b)) | (K::Kw(a), K::Kw(b)) | (K::Str(a), K::Str(b)) => a == b,
            (K::Int { v: a, width: w }, K::Int { v: b, width: x }) => a == b && w == x,
            (K::Flt { v: a, width: w }, K::Flt { v: b, width: x }) => {
                a.to_bits() == b.to_bits() && w == x
            }
            (K::Chr(a), K::Chr(b)) => a == b,
            (K::Bool(a), K::Bool(b)) => a == b,
            (K::Nil, K::Nil) => true,
            (K::List(a), K::List(b)) | (K::Vec(a), K::Vec(b)) | (K::Map(a), K::Map(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for FormKind {}
