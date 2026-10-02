//! The operators that change the shape of a form: `branch`, `clause`,
//! `swap`, `stmt` and `exit`.

use super::Mutant;
use crate::sexp::{Kind, Node};

fn replace(op: &'static str, start: usize, end: usize, text: String) -> Mutant {
    Mutant {
        op,
        start,
        end,
        text,
    }
}

/// `(if c a b)` to `(if c b a)` and `(if (not c) a b)`.
pub fn branch(src: &str, n: &Node, out: &mut Vec<Mutant>) {
    if n.head(src) != Some("if") || n.kids.len() != 4 {
        return;
    }
    let (cond, then, other) = (&n.kids[1], &n.kids[2], &n.kids[3]);
    let between = &src[then.end..other.start];
    let swapped = format!("{}{between}{}", other.text(src), then.text(src));
    out.push(replace("branch", then.start, other.end, swapped));
    let negated = format!("(not {})", cond.text(src));
    out.push(replace("branch", cond.start, cond.end, negated));
}

/// One clause of a `match` or a `cond` is deleted; a form with one clause
/// is left alone (it would only stop compiling).
pub fn clause(src: &str, n: &Node, out: &mut Vec<Mutant>) {
    let first = match n.head(src) {
        Some("match") => 2,
        Some("cond") => 1,
        _ => return,
    };
    let clauses: Vec<&Node> = n.kids.iter().skip(first).collect();
    if clauses.len() < 2 {
        return;
    }
    for c in clauses.iter().filter(|c| c.kind == Kind::List) {
        out.push(replace("clause", c.start, c.end, String::new()));
    }
}

/// The heads that are not calls of a function: forms with a meaning of
/// their own, whose operands are not both values.
const SPECIAL: &[&str] = &[
    "if", "do", "let", "loop", "fn", "match", "cond", "when", "unless", "and", "or", "not", "set!",
    ".", "quote", "defun", "def", "impl", "unsafe", "if-let", "when-let", "dotimes", "while", "->",
    "&",
];

/// The heads whose two operands may be exchanged without changing the
/// result: swapping them makes only an equivalent mutant, so it is not offered.
const COMMUTATIVE: &[&str] = &["+", "*", "=", "!=", "bit-and", "bit-or", "bit-xor"];

/// A symbol that can be a variable: no keyword, number, literal, marker or `_`.
fn is_variable(n: &Node, src: &str) -> bool {
    let Some(a) = n.atom(src) else { return false };
    let numeric = a.starts_with(|c: char| c.is_ascii_digit())
        || (a.starts_with('-') && a[1..].starts_with(|c: char| c.is_ascii_digit()));
    !(numeric
        || a.starts_with(':')
        || a.ends_with(':')
        || matches!(a, "true" | "false" | "nil" | "_" | "->" | "."))
}

/// `(f a b)` where `a` and `b` are two different variables: `(f b a)`, unless
/// `f` is commutative.
pub fn swap(src: &str, n: &Node, out: &mut Vec<Mutant>) {
    let Some(head) = n.head(src) else { return };
    if n.kids.len() != 3
        || SPECIAL.contains(&head)
        || COMMUTATIVE.contains(&head)
        || !is_variable(&n.kids[0], src)
    {
        return;
    }
    let (a, b) = (&n.kids[1], &n.kids[2]);
    if !is_variable(a, src) || !is_variable(b, src) || a.text(src) == b.text(src) {
        return;
    }
    let between = &src[a.end..b.start];
    let text = format!("{}{between}{}", b.text(src), a.text(src));
    out.push(replace("swap", a.start, b.end, text));
}

/// A `(set! ..)`, and every form of a `do` before the last, becomes `()`.
pub fn stmt(src: &str, n: &Node, out: &mut Vec<Mutant>) {
    match n.head(src) {
        Some("set!") => out.push(replace("stmt", n.start, n.end, "()".to_string())),
        Some("do") if n.kids.len() > 2 => {
            for k in &n.kids[1..n.kids.len() - 1] {
                out.push(replace("stmt", k.start, k.end, "()".to_string()));
            }
        }
        _ => {}
    }
}

/// The nodes whose value is the value of `n`: the branches of an `if`, the
/// last form of a `do`, `let`, `when` or `fn`, the body of each clause.
fn tails<'a>(n: &'a Node, src: &str, found: &mut Vec<&'a Node>) {
    let last = |n: &'a Node, found: &mut Vec<&'a Node>| {
        if let Some(k) = n.kids.last().filter(|_| n.kids.len() > 1) {
            tails(k, src, found);
        }
    };
    match n.head(src) {
        Some("if") if n.kids.len() >= 3 => {
            tails(&n.kids[2], src, found);
            if let Some(other) = n.kids.get(3) {
                tails(other, src, found);
            }
        }
        Some("do" | "let" | "when" | "unless" | "fn") => last(n, found),
        Some("match") => n.kids.iter().skip(2).for_each(|c| last(c, found)),
        Some("cond") => n.kids.iter().skip(1).for_each(|c| last(c, found)),
        _ => found.push(n),
    }
}

/// `(each-while c (fn (x) .. true))`: a `true` or `false` in a tail of the
/// callback flips, so the walk stops where it ran on, or runs on where it
/// stopped.
pub fn exit(src: &str, n: &Node, out: &mut Vec<Mutant>) {
    if n.head(src) != Some("each-while") {
        return;
    }
    let Some(callback) = n.kids.last().filter(|k| k.head(src) == Some("fn")) else {
        return;
    };
    let mut found = Vec::new();
    tails(callback, src, &mut found);
    for tail in found {
        let flipped = match tail.atom(src) {
            Some("true") => "false",
            Some("false") => "true",
            _ => continue,
        };
        out.push(replace("exit", tail.start, tail.end, flipped.to_string()));
    }
}
