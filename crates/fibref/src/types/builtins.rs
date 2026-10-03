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
    b("str-from-bytes", "(fn ((Array i8)) str)", "", &[B]),
    b("args", "(fn () (Vec str))", "", &[]),
    b("read-file", "(fn (str) (Option str))", "", &[B]),
    b("write-file", "(fn (str str) bool)", "", &[B, B]),
    b("sys-open", "(fn (str i64 i64) i64)", "", &[B, S, S]),
    b("sys-close", "(fn (i64) i64)", "", &[S]),
    b("sys-read", "(fn (i64 i64) (Array i8))", "", &[S, S]),
    b(
        "sys-write",
        "(fn (i64 (Array i8) i64 i64) i64)",
        "",
        &[S, B, S, S],
    ),
    b("sys-seek", "(fn (i64 i64 i64) i64)", "", &[S, S, S]),
    b("sys-pipe", "(fn () i64)", "", &[]),
    b("sys-dup", "(fn (i64) i64)", "", &[S]),
    b("sys-isatty", "(fn (i64) bool)", "", &[S]),
    b("sys-unlink", "(fn (str) i64)", "", &[B]),
    b("sys-mkdir", "(fn (str i64) i64)", "", &[B, S]),
    b("sys-rmdir", "(fn (str) i64)", "", &[B]),
    b("sys-errno-text", "(fn (i64) str)", "", &[S]),
    b("sys-getenv", "(fn (str) (Option str))", "", &[B]),
    b("sys-clock-now", "(fn () i64)", "", &[]),
    b("sys-wall-now", "(fn () i64)", "", &[]),
    b("sys-sleep", "(fn (i64) i64)", "", &[S]),
    b("str-concat", "(fn (str str) str)", "", &[B, B]),
    b("str-slice", "(fn (str i64 i64) str)", "", &[B, S, S]),
    b("str-byte-at", "(fn (str i64) i8)", "", &[B, S]),
    b(
        "str-find",
        "(fn (str str i64) (Option i64))",
        "",
        &[B, B, S],
    ),
    b("str-eq", "(fn (str str) bool)", "", &[B, B]),
    b("starts-with?", "(fn (str str) bool)", "", &[B, B]),
    b("char->i32", "(fn (char) i32)", "", &[S]),
    b("i32->char", "(fn (i32) char)", "", &[S]),
    b("f64->bits", "(fn (f64) i64)", "", &[S]),
    b("bits->f64", "(fn (i64) f64)", "", &[S]),
    b("f32->bits", "(fn (f32) i32)", "", &[S]),
    b("bits->f32", "(fn (i32) f32)", "", &[S]),
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
/// instances are registered by `init`. `Num` has `quot`, the truncating
/// division, and no `/`: `/` is the method of the library's `Div`
/// (stdlib §7 L30, `fib.core`), whose instances for `f32` and `f64` wrap
/// `fdiv`, the IEEE division that `Float` gives the two float types.
pub const BUILTIN_PROTOCOLS: &str = "
(defprotocol (Deref c t) (deref (self) -> t))
(defprotocol Num
  (+ (self y: Self) -> Self) (- (self y: Self) -> Self) (* (self y: Self) -> Self)
  (quot (self y: Self) -> Self) (rem (self y: Self) -> Self)
  (neg (self) -> Self))
(defprotocol Float (fdiv (self y: Self) -> Self))
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

    /// The messages of the type errors of `src`, none when it checks.
    fn type_errors(src: &str) -> Vec<String> {
        match crate::types::check_source(src, "t.fib") {
            Ok(_) => Vec::new(),
            Err(crate::types::SourceError::Type(es)) => {
                es.iter().map(|e| e.message.clone()).collect()
            }
            Err(e) => panic!("{src}\n{e:?}"),
        }
    }

    #[test]
    fn quot_is_a_num_method_and_fdiv_a_method_of_the_float_types_only() {
        let both = "(defun main () -> i64 (do (fdiv 1.0 2.0) (fdiv 1.0f32 2.0f32) (quot 7 2)))";
        assert_eq!(type_errors(both), Vec::<String>::new());
        let int = type_errors("(defun main () -> i64 (fdiv 1 2))");
        assert_eq!(int, ["no implementation of Float for i64"]);
        let own = "(defstruct P (x: i64)) (defun main () -> i64 (do (quot (P 1) (P 2)) 0))";
        assert_eq!(type_errors(own), ["no implementation of Num for P"]);
    }

    #[test]
    fn slash_is_no_method_of_num() {
        // `/` is `Div`'s, in the library (fib.core): with the prelude alone it is
        // unbound, whatever the operands, and a `Num` bound does not supply it.
        for src in [
            "(defun main () -> i64 (/ 4 2))",
            "(defun main () -> i64 (do (/ 4.0 2.0) 0))",
            "(defun d (a: t b: t) :where ((Num t)) -> t (/ a b)) (defun main () -> i64 0)",
        ] {
            assert_eq!(type_errors(src), ["unbound name /"], "{src}");
        }
        assert_eq!(
            type_errors("(defun main () -> i64 (quot 4 2))"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn names_are_unique() {
        for (i, s) in BUILTINS.iter().enumerate() {
            assert_eq!(builtin_index(s.name), Some(i), "{}", s.name);
        }
    }
}
