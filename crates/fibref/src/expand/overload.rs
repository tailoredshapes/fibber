//! Arity overloading (stdlib spec §7 L1): `(defn name ([a: T] -> R body)
//! ([a: T b: T] -> R body))` defines one function `name$N` per clause, `N`
//! being the clause's parameter count, and a call `(name a b)` is rewritten
//! to the clause of its argument count (`(name$2 a b)`), so the checker
//! never sees an overloaded name. `$` is a symbol character of the reader
//! and of lIR.
//!
//! The table of a module is read off its unexpanded forms before the first
//! one is expanded (a call may come before the definition), and a module
//! that `:use`s or `:require`s another resolves its calls against that
//! module's table, through its re-exports (`ExpandCtx::overload_in`). A
//! protocol method of the same name is recorded as a *method* of its
//! arity: a call of that count is left alone.
//!
//! Choices where the spec is silent: a local binding (a parameter, a
//! `let`) that is called is not told from the overloaded function, as the
//! expander tracks no scopes; the bare name as a function value is not
//! rewritten, so it is `unbound name`; `(gensym)` is the builtin's
//! `(gensym "G__")`; a `defn` of one clause is the plain function.

use std::collections::{BTreeSet, HashMap};

use crate::syntax::{Form, FormKind, Pos};

use super::build::{head_name, string};
use super::ctx::ExpandCtx;
use super::error::{ExpandError, ExpandErrorKind as K};
use super::params;

/// What one name means in one module: the arities of its clauses and of
/// the protocol methods of that name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Overload {
    pub(crate) clauses: BTreeSet<usize>,
    pub(crate) methods: BTreeSet<usize>,
}

/// The overloaded names of one module.
pub(crate) type Table = HashMap<String, Overload>;

/// The name of the clause of `name` that takes `arity` arguments.
pub(crate) fn clause_name(name: &str, arity: usize) -> String {
    format!("{name}${arity}")
}

/// `(base, arity)` for a name written `base$N`.
fn split_clause(name: &str) -> Option<(&str, usize)> {
    let (base, n) = name.rsplit_once('$')?;
    let n = n.parse().ok()?;
    (!base.is_empty()).then_some((base, n))
}

/// The clauses of a `defn` call (`items` is the whole call) when its
/// parameters are written as clauses, `([x] ..) ([x y] ..)`.
pub(crate) fn clauses_of(items: &[Form]) -> Option<&[Form]> {
    let mut at = 2;
    if matches!(items.get(at).map(|f| &f.kind), Some(FormKind::Str(_))) {
        at += 1;
    }
    let first = items.get(at)?.as_list()?.first()?;
    matches!(first.kind, FormKind::Vec(_)).then(|| &items[at..])
}

/// The parameter count of a clause `([params] ..)`.
pub(crate) fn clause_arity(clause: &Form) -> Option<usize> {
    match &clause.as_list()?.first()?.kind {
        FormKind::Vec(ps) => Some(params::arity(ps)),
        _ => None,
    }
}

/// The top-level forms, a top-level `do` looked through.
fn top_forms(forms: &[Form]) -> Vec<&Form> {
    let mut out = Vec::new();
    let mut stack: Vec<&Form> = forms.iter().rev().collect();
    while let Some(form) = stack.pop() {
        if head_name(form) == Some("do") {
            stack.extend(form.as_list().unwrap_or(&[]).iter().skip(1).rev());
        } else {
            out.push(form);
        }
    }
    out
}

/// The methods `(name, arity)` of a `defprotocol` form.
fn methods_of(form: &Form) -> Vec<(String, usize)> {
    let items = form.as_list().unwrap_or(&[]);
    let start = match items.get(2).map(|f| &f.kind) {
        Some(FormKind::Kw(k)) if k == "requires" => 4,
        _ => 2,
    };
    let method = |m: &Form| {
        let parts = m.as_list()?;
        let name = parts.first()?.as_sym()?;
        Some((name.to_string(), params::arity(parts.get(1)?.as_list()?)))
    };
    items.iter().skip(start).filter_map(method).collect()
}

/// Reads the arity table of the module being expanded off its unexpanded
/// forms; an error for two clauses of one count, or a clause of the arity
/// of a protocol method of its name.
pub(crate) fn scan(ctx: &mut ExpandCtx, forms: &[Form]) -> Result<(), ExpandError> {
    let forms = top_forms(forms);
    for form in &forms {
        if head_name(form) == Some("defprotocol") {
            for (name, arity) in methods_of(form) {
                let entry = ctx.table_mut().entry(name).or_default();
                entry.methods.insert(arity);
            }
        }
    }
    for form in &forms {
        if !matches!(head_name(form), Some("defn" | "defn-")) {
            continue;
        }
        let items = form.as_list().unwrap_or(&[]);
        let (Some(name), Some(clauses)) = (items.get(1).and_then(Form::as_sym), clauses_of(items))
        else {
            continue;
        };
        if clauses.len() < 2 {
            continue;
        }
        for clause in clauses {
            if let Some(arity) = clause_arity(clause) {
                ctx.add_clause(name, arity, &clause.pos)?;
            }
        }
    }
    Ok(())
}

impl ExpandCtx {
    fn table_mut(&mut self) -> &mut Table {
        let ns = self.scope.ns.clone();
        self.overloads.entry(ns).or_default()
    }

    /// Records a clause of `name` taking `arity` arguments in the module
    /// being expanded: an error if it has one already, or a protocol
    /// method of that name and count is visible.
    fn add_clause(&mut self, name: &str, arity: usize, pos: &Pos) -> Result<(), ExpandError> {
        let clash = self.visible_methods(name).contains(&arity);
        let entry = self.table_mut().entry(name.to_string()).or_default();
        let name = name.to_string();
        if entry.clauses.contains(&arity) {
            return Err(ExpandError::new(K::DuplicateClause { name, arity }, pos));
        }
        if clash {
            return Err(ExpandError::new(K::ClauseOfMethod { name, arity }, pos));
        }
        entry.clauses.insert(arity);
        Ok(())
    }

    /// Records a clause function `base$N` that a macro produced, as it is
    /// defined; one the scan saw is there already.
    pub(crate) fn note_clause(&mut self, defined: &str) {
        if let Some((base, arity)) = split_clause(defined) {
            let entry = self.table_mut().entry(base.to_string()).or_default();
            entry.clauses.insert(arity);
        }
    }

    /// The overload of `name` that module `ns` exports, through the
    /// modules it re-exports.
    fn overload_in(&self, ns: &str, name: &str) -> Option<&Overload> {
        let own = self.overloads.get(ns).and_then(|t| t.get(name));
        own.filter(|o| !o.clauses.is_empty()).or_else(|| {
            let reexported = self.exports.get(ns)?;
            reexported.iter().find_map(|r| self.overload_in(r, name))
        })
    }

    /// The arities of the protocol methods called `name` that the module
    /// being expanded sees.
    fn visible_methods(&self, name: &str) -> BTreeSet<usize> {
        let scope = &self.scope;
        let mut out = BTreeSet::new();
        let nss = std::iter::once(&scope.ns)
            .chain(&scope.uses)
            .chain(&scope.implicit);
        for ns in nss {
            if let Some(o) = self.overloads.get(ns).and_then(|t| t.get(name)) {
                out.extend(&o.methods);
            }
        }
        out
    }

    /// The overload a call head `name` reaches from the module being
    /// expanded, with the head's qualifier (`alias/` or empty) and base
    /// name. A name the module defines itself, not as clauses, is none;
    /// neither is one that a `:use`d program module defines plainly.
    fn overload_of<'a>(&'a self, name: &'a str) -> Option<(&'a str, &'a str, &'a Overload)> {
        let scope = &self.scope;
        if let Some((q, base)) = name.split_once('/') {
            let ns = match scope.aliases.get(q) {
                Some(ns) => ns.as_str(),
                None if q == super::PRELUDE_NS => return None,
                None if q == scope.ns => q,
                None if scope.implicit.iter().chain(&scope.uses).any(|m| m == q) => q,
                None => return None,
            };
            return self
                .overload_in(ns, base)
                .map(|o| (&name[..q.len() + 1], base, o));
        }
        if let Some(o) = self.overload_in(&scope.ns, name) {
            return Some(("", name, o));
        }
        if self.own.contains(name) {
            return None;
        }
        for ns in scope.uses.iter().chain(&scope.implicit) {
            if let Some(o) = self.overload_in(ns, name) {
                return Some(("", name, o));
            }
            if self.exported_names(ns).contains(name) {
                return None;
            }
        }
        None
    }

    /// The head a call of `name` with `argc` arguments has once its clause
    /// is picked; `None` if `name` is not an overloaded function, or the
    /// call is of a protocol method's arity.
    fn pick_clause(
        &self,
        name: &str,
        argc: usize,
        pos: &Pos,
    ) -> Result<Option<String>, ExpandError> {
        let Some((qualifier, base, o)) = self.overload_of(name) else {
            return Ok(None);
        };
        if o.clauses.contains(&argc) {
            return Ok(Some(format!("{qualifier}{}", clause_name(base, argc))));
        }
        if o.methods.contains(&argc) {
            return Ok(None);
        }
        let counts = o.clauses.union(&o.methods).copied().collect();
        let kind = K::NoClause {
            name: name.to_string(),
            counts,
            found: argc,
        };
        Err(ExpandError::new(kind, pos))
    }
}

/// `form`, a call, with its head replaced by the clause its argument count
/// picks; `(gensym)` is `(gensym "G__")`.
pub(crate) fn pick(ctx: &ExpandCtx, mut form: Form) -> Result<Form, ExpandError> {
    let Some(name) = head_name(&form).map(str::to_string) else {
        return Ok(form);
    };
    let pos = form.pos.clone();
    let FormKind::List(items) = &mut form.kind else {
        return Ok(form);
    };
    let argc = items.len() - 1;
    if name == "gensym" && argc == 0 && !ctx.own.contains("gensym") {
        items.push(string("G__", &pos));
    } else if let Some(head) = ctx.pick_clause(&name, argc, &pos)? {
        items[0] = Form::new(FormKind::Sym(head), items[0].pos.clone());
    }
    Ok(form)
}

#[cfg(test)]
mod tests {
    use crate::expand::{expand_program, ExpandCtx, NoRunner};
    use crate::syntax::{read_all, Form};

    fn expand(src: &str) -> Result<Vec<Form>, String> {
        let forms = read_all(src, "t.fib").map_err(|e| e.to_string())?;
        let mut ctx = ExpandCtx::new();
        expand_program(forms, &mut ctx, &mut NoRunner).map_err(|e| e.kind.to_string())
    }

    fn heads(forms: &[Form]) -> Vec<String> {
        let name = |f: &Form| Some(f.as_list()?.get(1)?.as_sym()?.to_string());
        forms.iter().filter_map(name).collect()
    }

    const TWO: &str = "(defn f ([x: i64] -> i64 x) ([x: i64 y: i64] -> i64 (+ x y)))";

    #[test]
    fn each_clause_is_a_defun_named_for_its_count() {
        assert_eq!(heads(&expand(TWO).expect("expands")), ["f$1", "f$2"]);
    }

    #[test]
    fn a_call_takes_the_clause_of_its_argument_count() {
        let out = expand(&format!("{TWO} (defun g () -> i64 (f 1 2))")).expect("expands");
        let g = out[2].as_list().expect("a defun");
        assert_eq!(g[5].as_list().expect("a call")[0].as_sym(), Some("f$2"));
    }

    #[test]
    fn a_count_with_no_clause_lists_the_counts() {
        let e = expand(&format!("{TWO} (defun g () -> i64 (f 1 2 3))"));
        assert_eq!(
            e.err().as_deref(),
            Some("f takes 1 or 2 argument(s), got 3")
        );
    }

    #[test]
    fn two_clauses_of_one_count_are_an_error() {
        let e = expand("(defn f ([x: i64] -> i64 x) ([y: i64] -> i64 y))");
        assert_eq!(
            e.err().as_deref(),
            Some("two clauses of f take 1 argument(s)")
        );
    }

    #[test]
    fn a_single_clause_is_the_plain_function() {
        let one = expand("(defn f ([x: i64] -> i64 x))").expect("expands");
        assert_eq!(heads(&one), ["f"]);
    }

    #[test]
    fn a_zero_argument_gensym_is_the_default_prefix() {
        let out = expand("(defun g () (gensym))").expect("expands");
        let call = out[0].as_list().expect("a defun")[3]
            .as_list()
            .expect("a call");
        assert_eq!(call.len(), 2);
    }
}
