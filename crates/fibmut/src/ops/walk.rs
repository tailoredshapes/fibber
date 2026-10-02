//! Which parts of a module are code, and the walk that offers each one to
//! the operators.
//!
//! The code of a module is the body of every `defun`, `defmacro`, `defn`,
//! the methods of an `impl` and the default bodies of a `defprotocol`, and
//! the value of a `def`. A signature, a `:where` clause, a type after `->` or
//! after a `name:` marker, the name side of a `let` binding, a pattern
//! (except a literal that is the whole pattern) and anything quoted are not
//! code: an edit there changes a declaration, not a computation. A form the
//! walk does not know is walked as a call.

use super::{atoms, shapes, Mutant};
use crate::sexp::{Kind, Node};

/// The offered nodes' collector.
pub struct Walk<'a> {
    src: &'a str,
    out: Vec<Mutant>,
}

/// An annotation marker: `x:` says the next form is a type.
fn is_marker(n: &Node, src: &str) -> bool {
    n.atom(src).is_some_and(|a| a.len() > 1 && a.ends_with(':'))
}

/// `(name (params) ..)`: the shape of a method of an `impl` or a `defprotocol`.
fn is_method(n: &Node) -> bool {
    n.kind == Kind::List
        && n.kids.len() >= 2
        && n.kids[0].kind == Kind::Atom
        && n.kids[1].kind == Kind::List
}

impl<'a> Walk<'a> {
    pub fn new(src: &'a str) -> Self {
        Walk {
            src,
            out: Vec::new(),
        }
    }

    pub fn finish(self) -> Vec<Mutant> {
        self.out
    }

    /// Walks the code of one top-level form.
    pub fn top(&mut self, form: &Node) {
        match form.head(self.src) {
            Some("defun" | "defmacro" | "defn" | "defn-") => self.callable(&form.kids[1..]),
            Some("impl") => self.methods(form.kids.iter().skip(3)),
            Some("defprotocol") => self.methods(form.kids.iter().skip(2)),
            Some("def") => self.typed(&form.kids[1..]),
            _ => {}
        }
    }

    fn methods<'n>(&mut self, kids: impl Iterator<Item = &'n Node>) {
        for method in kids.filter(|m| is_method(m)) {
            self.callable(&method.kids);
        }
    }

    /// `name .. (params) .. body`: the body after the first parameter list.
    fn callable(&mut self, kids: &[Node]) {
        let params = kids
            .iter()
            .skip(1)
            .position(|k| matches!(k.kind, Kind::List | Kind::Vector));
        if let Some(p) = params {
            self.body(&kids[p + 2..]);
        }
    }

    /// The forms after a parameter list: the keywords, `:where` and its
    /// constraints and `-> T` are skipped, the rest is code.
    fn body(&mut self, kids: &[Node]) {
        let mut i = 0;
        while i < kids.len() {
            match kids[i].atom(self.src) {
                Some("->" | ":where") => i += 2,
                Some(a) if a.starts_with(':') => i += 1,
                _ => {
                    self.code(&kids[i]);
                    i += 1;
                }
            }
        }
    }

    /// Forms in which a marker `x:` makes the next form a type.
    fn typed(&mut self, kids: &[Node]) {
        let mut i = 0;
        while i < kids.len() {
            if is_marker(&kids[i], self.src) {
                i += 2;
            } else {
                self.code(&kids[i]);
                i += 1;
            }
        }
    }

    /// Offers `n` to the operators, then walks what is inside it.
    fn code(&mut self, n: &Node) {
        self.offer(n);
        match n.kind {
            Kind::List => self.list(n),
            Kind::Vector | Kind::Map => self.typed(&n.kids),
            Kind::Prefix if self.src.as_bytes()[n.start] != b'\'' => self.code(&n.kids[0]),
            _ => {}
        }
    }

    fn offer(&mut self, n: &Node) {
        let (src, out) = (self.src, &mut self.out);
        shapes::exit(src, n, out);
        atoms::cmp(src, n, out);
        atoms::arith(src, n, out);
        atoms::boolean(src, n, out);
        atoms::constant(src, n, out);
        shapes::branch(src, n, out);
        shapes::clause(src, n, out);
        shapes::swap(src, n, out);
        shapes::stmt(src, n, out);
    }

    fn list(&mut self, n: &Node) {
        match n.head(self.src) {
            Some("quote") => {}
            Some("fn") => self.callable_anon(&n.kids[1..]),
            Some("let" | "loop") => self.binding_form(n, false),
            Some("dotimes" | "if-let" | "when-let") => self.binding_form(n, true),
            Some("match") => self.match_form(n),
            Some("cond") => self.cond_form(n),
            _ => self.typed(&n.kids),
        }
    }

    /// `(fn (params) body)`: no name before the parameter list.
    fn callable_anon(&mut self, kids: &[Node]) {
        match kids.first() {
            Some(p) if matches!(p.kind, Kind::List | Kind::Vector) => self.body(&kids[1..]),
            _ => self.body(kids),
        }
    }

    /// `(let ((name init) ..) body)` and kin: the init of each binding and
    /// the body are code, the names are not. `single` is for the forms
    /// whose second form is one binding, `(dotimes (i n) ..)`.
    fn binding_form(&mut self, n: &Node, single: bool) {
        let Some(binds) = n.kids.get(1) else { return };
        if !single && binds.kind == Kind::List && binds.kids.iter().all(|b| b.kind == Kind::List) {
            for pair in &binds.kids {
                self.binding(pair);
            }
        } else {
            self.binding(binds);
        }
        for k in n.kids.iter().skip(2) {
            self.code(k);
        }
    }

    /// `(name init)`, `(name: T init)`, `(pattern init)`; the first form
    /// (and the type after a marker) is a name, the rest is code.
    fn binding(&mut self, pair: &Node) {
        let skip = if pair.kids.first().is_some_and(|k| is_marker(k, self.src)) {
            2
        } else {
            1
        };
        for k in pair.kids.iter().skip(skip) {
            self.code(k);
        }
    }

    /// `(match scrutinee (pattern body..) ..)`; a pattern that is one atom
    /// is offered (it may be a literal), any other pattern is skipped.
    fn match_form(&mut self, n: &Node) {
        if let Some(scrutinee) = n.kids.get(1) {
            self.code(scrutinee);
        }
        for clause in n.kids.iter().skip(2).filter(|c| c.kind == Kind::List) {
            if let Some(pattern) = clause.kids.first().filter(|p| p.kind == Kind::Atom) {
                self.offer(pattern);
            }
            for k in clause.kids.iter().skip(1) {
                self.code(k);
            }
        }
    }

    /// `(cond (test expr..) ..)`: every form of a clause is code.
    fn cond_form(&mut self, n: &Node) {
        for clause in n.kids.iter().skip(1) {
            match clause.kind {
                Kind::List => clause.kids.iter().for_each(|k| self.code(k)),
                _ => self.code(clause),
            }
        }
    }
}
