//! The builtins and primitives of syntax §4.3 as a table (types §2.9–
//! §2.13), with the escape kind of every parameter recorded for the
//! ownership pass (types §6.3), and the built-in protocols of §2.9 and
//! §2.12 as fibber source.
//!
//! Signatures are written in the type syntax of §1 and parsed at start
//! up by the same code that reads annotations; `(& T)` marks an `&`
//! position (a signature, not a type, §1.4). An omitted colour on a
//! function-typed parameter is a fresh quantified colour variable (as
//! on a `defun` parameter, §1.4). Every builtin is a named function, so
//! its own colour is `:send`. `deref` is the method of the built-in
//! `Deref` protocol; `concat` is variadic and has its own rule
//! (lowered to [`ExprKind::Concat`](super::ast::ExprKind::Concat)).
//!
//! The primitive forms `set-field!`, `dyn` and the conversions have
//! rules of their own, not signatures, and are not in the table.

/// How a builtin treats one parameter, for the ownership pass (types
/// §6.3; every object result of a builtin is owned by the caller, §6.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Escape {
    /// A scalar: no count.
    Scalar,
    /// An object read during the call only; never escapes.
    Borrow,
    /// E2: stored into the object the builtin returns or updates.
    Store,
    /// E4: consumed by a thread crossing (`spawn`).
    Thread,
    /// An `&` position: the variable's own cell, no copy-in (§6.6).
    InOut,
    /// The operand of `weak`: no count operation, but it escapes and
    /// forces its binding onto the heap (§6.3, §6.11).
    Weak,
    /// The operand of `raw`: no count; the binding must outlive the
    /// pointer (§6.13).
    Raw,
}

/// One builtin function.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinSig {
    /// Its name.
    pub name: &'static str,
    /// Its signature, `(fn (P..) R)`.
    pub sig: &'static str,
    /// Its bounds, `((Send a) ..)`, or `""`.
    pub bounds: &'static str,
    /// The escape kind of each parameter.
    pub escapes: &'static [Escape],
    /// Whether it may be called only lexically inside `unsafe` (§3.15).
    pub unsafe_only: bool,
}

const fn b(
    name: &'static str,
    sig: &'static str,
    bounds: &'static str,
    escapes: &'static [Escape],
) -> BuiltinSig {
    BuiltinSig {
        name,
        sig,
        bounds,
        escapes,
        unsafe_only: false,
    }
}

const fn u(name: &'static str, sig: &'static str, escapes: &'static [Escape]) -> BuiltinSig {
    BuiltinSig {
        name,
        sig,
        bounds: "",
        escapes,
        unsafe_only: true,
    }
}

use Escape::{Borrow as B, InOut as IO, Raw, Scalar as S, Store as St, Thread as T, Weak as W};

/// Every builtin function, by [`BuiltinId`](super::ast::BuiltinId).
pub const BUILTINS: &[BuiltinSig] = &[
    b("cell", "(fn (a) (Cell a))", "", &[St]),
    b("set!", "(fn ((Cell a) a) unit)", "", &[B, St]),
    b("atom", "(fn (a) (Atom a))", "((Send a))", &[St]),
    b("swap!", "(fn ((Atom a) (fn (a) a)) a)", "", &[B, B]),
    b("reset!", "(fn ((Atom a) a) unit)", "", &[B, St]),
    b("weak", "(fn (a) (Weak a))", "((Weakable a))", &[W]),
    b(
        "spawn",
        "(fn ((fn :send () a)) (Task a))",
        "((Send a))",
        &[T],
    ),
    b("join", "(fn ((Task a)) a)", "", &[B]),
    b("trap", "(fn (str) a)", "", &[B]),
    b("not", "(fn (bool) bool)", "", &[S]),
    b("array", "(fn (i64 a) (Array a))", "", &[S, St]),
    b("array-len", "(fn ((Array a)) i64)", "", &[B]),
    b("array-get", "(fn ((Array a) i64) a)", "", &[B, S]),
    b(
        "array-with",
        "(fn ((Array a) i64 a) (Array a))",
        "",
        &[B, S, St],
    ),
    b(
        "array-copy",
        "(fn ((Array a) i64 i64) (Array a))",
        "",
        &[B, S, S],
    ),
    b(
        "array-set!",
        "(fn ((& (Array a)) i64 a) unit)",
        "",
        &[IO, S, St],
    ),
    b("str-len", "(fn (str) i64)", "", &[B]),
    b("str-bytes", "(fn (str) (Array i8))", "", &[B]),
    b("str-concat", "(fn (str str) str)", "", &[B, B]),
    b("str-slice", "(fn (str i64 i64) str)", "", &[B, S, S]),
    b("str-eq", "(fn (str str) bool)", "", &[B, B]),
    b("starts-with?", "(fn (str str) bool)", "", &[B, B]),
    b("char->i32", "(fn (char) i32)", "", &[S]),
    b("i32->char", "(fn (i32) char)", "", &[S]),
    b("concat", "(fn ((Vec a) (Vec a)) (Vec a))", "", &[B, B]),
    b("gensym", "(fn (str) Form)", "", &[B]),
    b("struct?", "(fn (Form) bool)", "", &[B]),
    b("struct-fields", "(fn (Form) (Vec Form))", "", &[B]),
    b("struct-params", "(fn (Form) (Vec Form))", "", &[B]),
    b("struct-field-types", "(fn (Form) (Vec Form))", "", &[B]),
    b("enum?", "(fn (Form) bool)", "", &[B]),
    b("enum-params", "(fn (Form) (Vec Form))", "", &[B]),
    b("enum-variants", "(fn (Form) (Vec Form))", "", &[B]),
    u("ptr+", "(fn (ptr i64) ptr)", &[S, S]),
    u("load-i8", "(fn (ptr) i8)", &[S]),
    u("load-i16", "(fn (ptr) i16)", &[S]),
    u("load-i32", "(fn (ptr) i32)", &[S]),
    u("load-i64", "(fn (ptr) i64)", &[S]),
    u("load-ptr", "(fn (ptr) ptr)", &[S]),
    u("store-i8", "(fn (ptr i8) unit)", &[S, S]),
    u("store-i16", "(fn (ptr i16) unit)", &[S, S]),
    u("store-i32", "(fn (ptr i32) unit)", &[S, S]),
    u("store-i64", "(fn (ptr i64) unit)", &[S, S]),
    u("store-ptr", "(fn (ptr ptr) unit)", &[S, S]),
    u("alloc", "(fn (i64) ptr)", &[S]),
    u("free", "(fn (ptr) unit)", &[S]),
    BuiltinSig {
        name: "raw",
        sig: "(fn (a) ptr)",
        bounds: "((Object a))",
        escapes: &[Raw],
        unsafe_only: true,
    },
    BuiltinSig {
        name: "raw-retained",
        sig: "(fn (a) ptr)",
        bounds: "((Object a))",
        escapes: &[St],
        unsafe_only: true,
    },
    u("release-raw", "(fn (ptr) unit)", &[S]),
];

/// The index of the builtin `name` in [`BUILTINS`].
pub fn builtin_index(name: &str) -> Option<usize> {
    BUILTINS.iter().position(|s| s.name == name)
}

/// The built-in protocols: `Deref` (§2.9) and the arithmetic,
/// comparison, bit, hash and show protocols of §2.12, whose built-in
/// instances are registered by `init`.
pub const BUILTIN_PROTOCOLS: &str = "
(defprotocol (Deref c t) (deref (self) -> t))
(defprotocol Num
  (+ (self y: Self) -> Self) (- (self y: Self) -> Self) (* (self y: Self) -> Self)
  (/ (self y: Self) -> Self) (rem (self y: Self) -> Self) (neg (self) -> Self))
(defprotocol Eq (= (self y: Self) -> bool) (!= (self y: Self) -> bool (not (= self y))))
(defprotocol Ord :requires (Eq)
  (< (self y: Self) -> bool) (<= (self y: Self) -> bool (not (< y self)))
  (> (self y: Self) -> bool (< y self)) (>= (self y: Self) -> bool (not (< self y))))
(defprotocol Bits
  (bit-and (self y: Self) -> Self) (bit-or (self y: Self) -> Self)
  (bit-xor (self y: Self) -> Self) (bit-not (self) -> Self)
  (shl (self n: Self) -> Self) (shr (self n: Self) -> Self)
  (sar (self n: Self) -> Self) (popcount (self) -> Self))
(defprotocol Hash (hash (self) -> i64))
(defprotocol Show (show (self) -> str))
";

/// The conversions of §2.12 whose first operand is a target type.
pub const CONVERSIONS: [&str; 9] = [
    "trunc", "zext", "sext", "fptrunc", "fpext", "fptosi", "fptoui", "sitofp", "uitofp",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_has_one_escape_kind_per_parameter() {
        for s in BUILTINS {
            let forms = crate::syntax::read_all(s.sig, "<builtin>").expect("reads");
            let params = forms[0].as_list().expect("list")[1]
                .as_list()
                .expect("params")
                .len();
            assert_eq!(params, s.escapes.len(), "{}", s.name);
        }
    }

    #[test]
    fn names_are_unique() {
        for (i, s) in BUILTINS.iter().enumerate() {
            assert_eq!(builtin_index(s.name), Some(i), "{}", s.name);
        }
    }
}
