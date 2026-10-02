//! The mutation operators: each one lists text edits at single sites of a
//! module's code, and `mutants` gathers them all in source order.
//!
//! A mutant is one replacement of the byte span `start..end` of the
//! source by `text`. A mutant that does not compile is the runner's
//! business (it is counted as invalid and never run against a case); the
//! operators only keep the edits that cannot be told apart from the source
//! out of the list. What counts as code (a function body, not a signature,
//! a pattern or a type) is decided by `walk`; the operators are:
//!
//! | name     | edit |
//! |----------|------|
//! | `cmp`    | a comparison head: `<` and `<=`, `>` and `>=`, `=` and `!=` swap |
//! | `arith`  | `+` becomes `-`, `-` becomes `+`, `*` becomes `+` |
//! | `bool`   | `true` and `false` swap; `and` and `or` swap; `(not x)` becomes `x` |
//! | `const`  | an integer literal `n` becomes `n+1`, `n-1` and `0` |
//! | `branch` | `(if c a b)` becomes `(if c b a)` and `(if (not c) a b)` |
//! | `clause` | one clause of a `match` or a `cond` is deleted |
//! | `swap`   | `(f a b)` of two variables becomes `(f b a)` |
//! | `stmt`   | a `(set! ..)`, or a form of a `do` before the last, becomes `()` |
//! | `exit`   | a `true` or `false` in the tail of the callback `(each-while c (fn ..))` flips |

mod atoms;
mod shapes;
mod walk;

use crate::sexp::{self, ParseError};

/// The operator names, in the order the reports list them.
pub const OPERATORS: [&str; 9] = [
    "cmp", "arith", "bool", "const", "branch", "clause", "swap", "stmt", "exit",
];

/// One edit: the bytes `start..end` of the source become `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutant {
    pub op: &'static str,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

impl Mutant {
    /// The source with the edit applied.
    pub fn apply(&self, src: &str) -> String {
        let mut out = String::with_capacity(src.len() + self.text.len());
        out.push_str(&src[..self.start]);
        out.push_str(&self.text);
        out.push_str(&src[self.end..]);
        out
    }

    /// The 1-based line the edit starts on.
    pub fn line(&self, src: &str) -> usize {
        sexp::line_of(src, self.start)
    }
}

/// Every mutant of `src`, in source order. Two operators that make the
/// same edit give one mutant, named for the first in `walk`'s order (`exit`
/// before `bool`); an edit that leaves the text as it was is dropped.
pub fn mutants(src: &str) -> Result<Vec<Mutant>, ParseError> {
    let forms = sexp::parse(src)?;
    let mut walker = walk::Walk::new(src);
    for form in &forms {
        walker.top(form);
    }
    let mut all = walker.finish();
    all.retain(|m| m.text != src[m.start..m.end]);
    let mut seen = std::collections::HashSet::new();
    all.retain(|m| seen.insert((m.start, m.end, m.text.clone())));
    all.sort_by(|a, b| (a.start, a.end, &a.text).cmp(&(b.start, b.end, &b.text)));
    Ok(all)
}

#[cfg(test)]
mod tests;
