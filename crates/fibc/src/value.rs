//! The lIR types this compiler uses and the values it passes around
//! while lowering (an SSA name, a global address or a literal, with
//! its type).

/// An lIR first-class type this compiler uses, as text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LirTy {
    I1,
    I8,
    I16,
    I32,
    I64,
    Float,
    Double,
    /// The pointer to an object (types §8.2): a counted pointer, which
    /// retain, release, drop and trace follow.
    Ptr,
    /// A raw `ptr` of a program (types §8.1, syntax §3.15): an address
    /// from `alloc` or an extern, uncounted. It is `ptr` in lIR text and
    /// in layout, but its own variant, because the compiler decides
    /// whether a slot is counted from this type and a raw pointer and an
    /// object pointer must not be told apart by their lIR text.
    Raw,
    /// A `(dyn P)` value: `{ ptr ptr }` by value (types §8.1).
    Dyn,
}

impl LirTy {
    pub fn text(self) -> &'static str {
        match self {
            LirTy::I1 => "i1",
            LirTy::I8 => "i8",
            LirTy::I16 => "i16",
            LirTy::I32 => "i32",
            LirTy::I64 => "i64",
            LirTy::Float => "float",
            LirTy::Double => "double",
            LirTy::Ptr | LirTy::Raw => "ptr",
            LirTy::Dyn => "{ ptr ptr }",
        }
    }

    pub fn is_int(self) -> bool {
        matches!(
            self,
            LirTy::I1 | LirTy::I8 | LirTy::I16 | LirTy::I32 | LirTy::I64
        )
    }

    pub fn bits(self) -> u32 {
        match self {
            LirTy::I1 => 1,
            LirTy::I8 => 8,
            LirTy::I16 => 16,
            LirTy::I32 => 32,
            LirTy::I64 | LirTy::Double | LirTy::Ptr | LirTy::Raw => 64,
            LirTy::Float => 32,
            LirTy::Dyn => 128,
        }
    }
}

/// A value: an SSA name, a global address or a literal, with its type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum V {
    /// No value (`unit`).
    Unit,
    /// A value of the type.
    Val(String, LirTy),
}

impl V {
    pub fn text(&self) -> &str {
        match self {
            V::Unit => "",
            V::Val(s, _) => s,
        }
    }

    pub fn ty(&self) -> Option<LirTy> {
        match self {
            V::Unit => None,
            V::Val(_, t) => Some(*t),
        }
    }

    pub fn null() -> V {
        V::Val("(ptr null)".into(), LirTy::Ptr)
    }

    pub fn int(t: LirTy, n: i64) -> V {
        V::Val(format!("({} {n})", t.text()), t)
    }
}
