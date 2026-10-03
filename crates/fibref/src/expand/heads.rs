//! What the head of a list form means to the expander (§4.2, §4.3).

/// The twenty-five core forms of §4.2 (`and` and `or` since stdlib §7 L20).
pub const CORE_FORMS: [&str; 25] = [
    "defun",
    "def",
    "fn",
    "let",
    "if",
    "do",
    "match",
    "loop",
    "recur",
    "defstruct",
    ".",
    "defenum",
    "defprotocol",
    "impl",
    "&",
    "async",
    "await",
    "unsafe",
    "extern",
    "quote",
    "defmacro",
    "ns",
    "var",
    "and",
    "or",
];

/// The core forms that §2 admits only at top level.
pub(crate) const DEFINITIONS: [&str; 9] = [
    "defun",
    "def",
    "defstruct",
    "defenum",
    "defprotocol",
    "impl",
    "defmacro",
    "extern",
    "ns",
];

/// The conversions of §4.3 whose first operand is a target type.
const CONVERSIONS: [&str; 9] = [
    "trunc", "zext", "sext", "fptrunc", "fpext", "fptosi", "fptoui", "sitofp", "uitofp",
];

/// Forms that exist only until expansion (§4.2).
pub(crate) const QUASI_FORMS: [&str; 3] = ["quasiquote", "unquote", "unquote-splicing"];

/// Whether `name` is a core form.
pub fn is_core(name: &str) -> bool {
    CORE_FORMS.contains(&name)
}

/// Whether `name` is a top-level-only core form.
pub(crate) fn is_definition(name: &str) -> bool {
    DEFINITIONS.contains(&name)
}

/// For a primitive form of §4.3, the index (in the list, head at 0) of
/// the operand that is not an expression and that the expander leaves
/// alone: the field name of `set-field!`, the protocol of `dyn`, the
/// target type of a conversion.
pub(crate) fn primitive_operand(name: &str) -> Option<usize> {
    match name {
        "set-field!" => Some(2),
        "dyn" => Some(1),
        _ if CONVERSIONS.contains(&name) => Some(1),
        _ => None,
    }
}
