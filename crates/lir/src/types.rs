//! Types (spec/lir.md §2).

use std::fmt;

/// A first-class lIR type. Equality is structural and nominal as in
/// LLVM: a named struct equals only itself.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// `iN`, N ∈ {1, 8, 16, 32, 64}
    Int(u32),
    Float,
    Double,
    Ptr,
    /// `<N x E>`, E an integer, float, double or ptr
    Vector(u32, Box<Type>),
    /// `%struct.NAME`
    Named(String),
    /// `{ T* }`
    Anon(Vec<Type>),
    /// `[N x T]`
    Array(u64, Box<Type>),
}

/// A calling convention (spec/lir.md §4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Cc {
    #[default]
    C,
    Tail,
}

/// A function's type: convention, result (`None` is `void`),
/// parameters, variadic.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FnType {
    pub cc: Cc,
    pub ret: Option<Type>,
    pub params: Vec<Type>,
    pub varargs: bool,
}

pub const I1: Type = Type::Int(1);
pub const I32: Type = Type::Int(32);

/// Whether `s` names a scalar type keyword.
pub fn scalar_keyword(s: &str) -> Option<Type> {
    Some(match s {
        "i1" => Type::Int(1),
        "i8" => Type::Int(8),
        "i16" => Type::Int(16),
        "i32" => Type::Int(32),
        "i64" => Type::Int(64),
        "float" => Type::Float,
        "double" => Type::Double,
        "ptr" => Type::Ptr,
        _ => return None,
    })
}

impl Type {
    pub fn is_int(&self) -> bool {
        matches!(self, Type::Int(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Type::Float | Type::Double)
    }

    /// The element type of a vector, or the type itself.
    pub fn scalar(&self) -> &Type {
        match self {
            Type::Vector(_, e) => e,
            t => t,
        }
    }

    /// Integer or vector of integers.
    pub fn is_int_like(&self) -> bool {
        self.scalar().is_int()
    }

    /// Float, double or a vector of them.
    pub fn is_float_like(&self) -> bool {
        self.scalar().is_float()
    }

    pub fn is_aggregate(&self) -> bool {
        matches!(self, Type::Named(_) | Type::Anon(_) | Type::Array(..))
    }

    /// Lanes of a vector type.
    pub fn lanes(&self) -> Option<u32> {
        match self {
            Type::Vector(n, _) => Some(*n),
            _ => None,
        }
    }

    /// Size in bits of a non-aggregate, non-pointer type (bitcast).
    pub fn bits(&self) -> Option<u64> {
        match self {
            Type::Int(n) => Some(u64::from(*n)),
            Type::Float => Some(32),
            Type::Double => Some(64),
            Type::Vector(n, e) => e.bits().map(|b| b * u64::from(*n)),
            Type::Ptr | Type::Named(_) | Type::Anon(_) | Type::Array(..) => None,
        }
    }

    /// The same shape with `i1` elements: the type of a comparison.
    pub fn bool_shape(&self) -> Type {
        match self {
            Type::Vector(n, _) => Type::Vector(*n, Box::new(I1)),
            _ => I1,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int(n) => write!(f, "i{n}"),
            Type::Float => write!(f, "float"),
            Type::Double => write!(f, "double"),
            Type::Ptr => write!(f, "ptr"),
            Type::Vector(n, e) => write!(f, "<{n} x {e}>"),
            Type::Named(s) => write!(f, "%struct.{s}"),
            Type::Array(n, e) => write!(f, "[{n} x {e}]"),
            Type::Anon(fs) if fs.is_empty() => write!(f, "{{ }}"),
            Type::Anon(fs) => {
                let parts: Vec<String> = fs.iter().map(|t| t.to_string()).collect();
                write!(f, "{{ {} }}", parts.join(", "))
            }
        }
    }
}

/// `void` or the type.
pub fn show_ret(t: &Option<Type>) -> String {
    match t {
        Some(t) => t.to_string(),
        None => "void".into(),
    }
}

impl fmt::Display for Cc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Cc::C => "ccc",
            Cc::Tail => "tailcc",
        })
    }
}

impl FnType {
    /// The parameter list as `(i64 ptr ...)`.
    pub fn show_params(&self) -> String {
        let mut parts: Vec<String> = self.params.iter().map(|t| t.to_string()).collect();
        if self.varargs {
            parts.push("...".into());
        }
        format!("({})", parts.join(" "))
    }
}

impl fmt::Display for FnType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cc = match self.cc {
            Cc::C => String::new(),
            Cc::Tail => "tailcc ".into(),
        };
        write!(f, "(fn {cc}{} {})", show_ret(&self.ret), self.show_params())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_as_written() {
        let pair = Type::Anon(vec![Type::Ptr, Type::Ptr]);
        assert_eq!(pair.to_string(), "{ ptr, ptr }");
        assert_eq!(Type::Vector(4, Box::new(I32)).to_string(), "<4 x i32>");
        assert_eq!(Type::Named("s".into()).to_string(), "%struct.s");
        let arr = Type::Array(3, Box::new(Type::Array(0, Box::new(Type::Ptr))));
        assert_eq!(arr.to_string(), "[3 x [0 x ptr]]");
        assert!(arr.is_aggregate() && arr.bits().is_none());
        let f = FnType {
            cc: Cc::Tail,
            ret: None,
            params: vec![Type::Ptr],
            varargs: true,
        };
        assert_eq!(f.to_string(), "(fn tailcc void (ptr ...))");
    }

    #[test]
    fn named_and_anonymous_structs_differ() {
        assert_ne!(Type::Named("p".into()), Type::Anon(vec![Type::Ptr]));
    }
}
