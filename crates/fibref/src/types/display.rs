//! Printing types in the surface syntax of §1, for messages and for the
//! typed program's readers.
//!
//! Unification variables print as `a`, `b`, ... in order of first
//! appearance within one [`Printer`] (so one message names its
//! variables consistently); quantified variables by their scheme's names
//! (or `t0`, `t1`, ... when none are given); rigid variables by their
//! written names. A colour prints only when it is `:send` or `:local`
//! (`(fn :local () i64)`); a scheme's quantified colour variable prints
//! as `ς0`, `ς1`, ... (`(fn ς0 (a) a)`); an unsolved colour variable
//! prints nothing.

use std::cell::RefCell;

use super::decls::Globals;
use super::ty::{Colour, Con, Pred, TvId, Ty};

/// Prints types against the global tables.
pub struct Printer<'a> {
    g: &'a Globals,
    gen_names: &'a [String],
    rigid_names: &'a [String],
    vars: RefCell<Vec<TvId>>,
}

impl<'a> Printer<'a> {
    /// A printer with no scheme or rigid names.
    pub fn new(g: &'a Globals) -> Self {
        Printer::with_names(g, &[], &[])
    }

    /// A printer that names `Gen(i)` by `gen_names[i]` and `Rigid(i)` by
    /// `rigid_names[i]`.
    pub fn with_names(g: &'a Globals, gen_names: &'a [String], rigid_names: &'a [String]) -> Self {
        Printer {
            g,
            gen_names,
            rigid_names,
            vars: RefCell::new(Vec::new()),
        }
    }

    /// The type as text. It should be zonked first.
    pub fn ty(&self, t: &Ty) -> String {
        let mut out = String::new();
        self.write(t, &mut out);
        out
    }

    /// A predicate as text: `(P T..)`, `(Send T)`, `(Object T)`,
    /// `(Weakable T)`.
    pub fn pred(&self, p: &Pred) -> String {
        let (head, args): (String, Vec<&Ty>) = match p {
            Pred::Proto(id, args) => (self.g.proto(*id).name.clone(), args.iter().collect()),
            Pred::Send(t) => ("Send".to_string(), vec![t]),
            Pred::Object(t) => ("Object".to_string(), vec![t]),
            Pred::Weakable(t) => ("Weakable".to_string(), vec![t]),
        };
        let parts: Vec<String> = args.iter().map(|t| self.ty(t)).collect();
        format!("({head} {})", parts.join(" "))
    }

    fn var_name(&self, v: TvId) -> String {
        let mut vars = self.vars.borrow_mut();
        let i = match vars.iter().position(|x| *x == v) {
            Some(i) => i,
            None => {
                vars.push(v);
                vars.len() - 1
            }
        };
        letter_name(i)
    }

    fn write(&self, t: &Ty, out: &mut String) {
        match t {
            Ty::Var(v) => out.push_str(&self.var_name(*v)),
            Ty::Gen(i) => match self.gen_names.get(*i as usize) {
                Some(n) => out.push_str(n),
                None => out.push_str(&format!("t{i}")),
            },
            Ty::Rigid(i) => match self.rigid_names.get(*i as usize) {
                Some(n) => out.push_str(n),
                None => out.push_str(&format!("r{i}")),
            },
            Ty::Con(c, args) => self.write_con(*c, args, out),
            Ty::Fn(k, ps, r) => {
                out.push_str("(fn ");
                match k {
                    Colour::Send => out.push_str(":send "),
                    Colour::Local => out.push_str(":local "),
                    Colour::Gen(i) => out.push_str(&format!("ς{i} ")),
                    Colour::Var(_) => {}
                }
                self.write_list(ps, out);
                out.push(' ');
                self.write(r, out);
                out.push(')');
            }
        }
    }

    fn write_list(&self, ts: &[Ty], out: &mut String) {
        out.push('(');
        for (i, t) in ts.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            self.write(t, out);
        }
        out.push(')');
    }

    fn write_con(&self, c: Con, args: &[Ty], out: &mut String) {
        let name = match c {
            Con::Scalar(s) => s.name().to_string(),
            Con::Str => "str".to_string(),
            Con::Array => "Array".to_string(),
            Con::Cell => "Cell".to_string(),
            Con::Atom => "Atom".to_string(),
            Con::Weak => "Weak".to_string(),
            Con::Task => "Task".to_string(),
            Con::Nominal(id) => self.g.ty(id).name.clone(),
            Con::Dyn(p, send) => {
                let pname = self.g.proto(p).name.clone();
                out.push_str("(dyn ");
                if args.is_empty() {
                    out.push_str(&pname);
                } else {
                    self.write_con_args(&pname, args, out);
                }
                if send {
                    out.push_str(" :send");
                }
                out.push(')');
                return;
            }
        };
        if args.is_empty() {
            out.push_str(&name);
        } else if let Con::Nominal(id) = c {
            self.write_nominal_args(&name, self.g.ty(id), args, out);
        } else {
            self.write_con_args(&name, args, out);
        }
    }

    fn write_con_args(&self, name: &str, args: &[Ty], out: &mut String) {
        out.push('(');
        out.push_str(name);
        for a in args {
            out.push(' ');
            self.write(a, out);
        }
        out.push(')');
    }

    /// A nominal type's arguments; one at a colour parameter (§1.3)
    /// prints as its colour: `:send`, `:local`, `ς0`, or `_` unsolved.
    fn write_nominal_args(
        &self,
        name: &str,
        def: &super::decls::TypeDef,
        args: &[Ty],
        out: &mut String,
    ) {
        out.push('(');
        out.push_str(name);
        for (i, a) in args.iter().enumerate() {
            out.push(' ');
            match (def.is_colour(i), a) {
                (true, Ty::Fn(k, _, _)) => out.push_str(&match k {
                    Colour::Send => ":send".to_string(),
                    Colour::Local => ":local".to_string(),
                    Colour::Gen(i) => format!("ς{i}"),
                    Colour::Var(_) => "_".to_string(),
                }),
                _ => self.write(a, out),
            }
        }
        out.push(')');
    }
}

/// `a`, `b`, ..., `z`, `a1`, `b1`, ...
pub fn letter_name(i: usize) -> String {
    let letter = (b'a' + (i % 26) as u8) as char;
    match i / 26 {
        0 => letter.to_string(),
        n => format!("{letter}{n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_cycle_with_a_suffix() {
        assert_eq!(letter_name(0), "a");
        assert_eq!(letter_name(25), "z");
        assert_eq!(letter_name(26), "a1");
    }
}
