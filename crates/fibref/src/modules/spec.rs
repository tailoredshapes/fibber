//! What a module's `ns` form says (syntax §5): its name, its `:require`s,
//! `:use`s and `:export-from`s, and the implicit modules the loader adds.

use crate::syntax::{Form, FormKind};

use super::LoadError;

/// What a module's `ns` form says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleSpec {
    pub ns: String,
    /// `(:require [a.b :as ab])`: alias, module.
    pub requires: Vec<(String, String)>,
    /// `(:use a.b)`, and the modules of an `:export-from`, which are
    /// used too.
    pub uses: Vec<String>,
    /// `(:export-from a.b)`: the modules this one re-exports, each also
    /// one of its `uses`: what they export, it exports (syntax §5).
    pub exports: Vec<String>,
    /// The implicit modules this module sees (`IMPLICIT_LIB`, syntax §5):
    /// not written in the `ns` form, set by the loader, which gives them
    /// to every module but the library's own (`fib.*`, `Long`, `Math`)
    /// and the implicit modules. Names a module of this program loaded
    /// before it.
    pub implicit: Vec<String>,
}

impl ModuleSpec {
    /// The spec of a single-file program with no `ns` form.
    pub fn main() -> ModuleSpec {
        ModuleSpec {
            ns: "main".into(),
            ..ModuleSpec::default()
        }
    }

    /// Every module this one depends on: its requires and its uses.
    pub fn deps(&self) -> impl Iterator<Item = &str> {
        self.requires
            .iter()
            .map(|(_, ns)| ns.as_str())
            .chain(self.uses.iter().map(String::as_str))
    }
}

/// The spec a module's forms give: the first form when it is an `ns`,
/// else the default with no clauses.
pub fn spec_of(forms: &[Form], default_ns: &str) -> Result<ModuleSpec, String> {
    spec_in(forms, default_ns).map_err(|e| e.to_string())
}

pub(super) fn spec_in(forms: &[Form], default_ns: &str) -> Result<ModuleSpec, LoadError> {
    let default = || ModuleSpec {
        ns: default_ns.into(),
        ..ModuleSpec::default()
    };
    let Some(first) = forms.first() else {
        return Ok(default());
    };
    let items = match first.as_list() {
        Some(items) if items.first().and_then(Form::as_sym) == Some("ns") => items,
        _ => return Ok(default()),
    };
    let bad = |what: &str| LoadError::Spec {
        pos: first.pos.clone(),
        what: what.to_string(),
    };
    let ns = items
        .get(1)
        .and_then(Form::as_sym)
        .ok_or_else(|| bad("ns needs a name"))?;
    let mut spec = ModuleSpec {
        ns: ns.to_string(),
        ..ModuleSpec::default()
    };
    for clause in &items[2..] {
        ns_clause(&mut spec, clause).map_err(|what| bad(&what))?;
    }
    Ok(spec)
}

/// One clause of an `ns` form, added to `spec`.
fn ns_clause(spec: &mut ModuleSpec, clause: &Form) -> Result<(), String> {
    let parts = clause
        .as_list()
        .ok_or("an ns clause is a list: (:require ..), (:use ..) or (:export-from ..)")?;
    let Some(FormKind::Kw(key)) = parts.first().map(|f| &f.kind) else {
        return Err("an ns clause starts with :require, :use or :export-from".into());
    };
    let items = &parts[1..];
    match key.as_str() {
        "require" => require_clause(spec, items).map_err(str::to_string),
        "use" => {
            let names = module_names(items).ok_or("a :use item is a module name")?;
            spec.uses.extend(names);
            Ok(())
        }
        "export-from" => {
            let names = module_names(items).ok_or("an :export-from item is a module name")?;
            for name in names {
                if !spec.uses.contains(&name) {
                    spec.uses.push(name.clone());
                }
                spec.exports.push(name);
            }
            Ok(())
        }
        other => Err(format!("unknown ns clause :{other}")),
    }
}

/// The module names of a `:use` or `:export-from` clause, if every item
/// is a symbol.
fn module_names(items: &[Form]) -> Option<Vec<String>> {
    items
        .iter()
        .map(|f| f.as_sym().map(str::to_string))
        .collect()
}

/// The items of a `:require` clause: `[name :as alias]` each.
fn require_clause(spec: &mut ModuleSpec, items: &[Form]) -> Result<(), &'static str> {
    const WHAT: &str = "a :require item is [name :as alias]";
    for r in items {
        let FormKind::Vec(v) = &r.kind else {
            return Err(WHAT);
        };
        match (
            v.first().and_then(Form::as_sym),
            v.get(1).map(|f| &f.kind),
            v.get(2).and_then(Form::as_sym),
        ) {
            (Some(name), Some(FormKind::Kw(k)), Some(alias)) if k == "as" && v.len() == 3 => {
                spec.requires.push((alias.to_string(), name.to_string()));
            }
            _ => return Err(WHAT),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::read_all;

    #[test]
    fn an_ns_form_gives_the_spec() {
        let forms = read_all(
            "(ns a.b (:require [c.d :as cd] [e :as e]) (:use f.g))\n(defun main () -> i64 1)",
            "t",
        )
        .expect("reads");
        let s = spec_of(&forms, "main").expect("a spec");
        assert_eq!(s.ns, "a.b");
        assert_eq!(
            s.requires,
            vec![
                ("cd".to_string(), "c.d".to_string()),
                ("e".to_string(), "e".to_string())
            ]
        );
        assert_eq!(s.uses, vec!["f.g".to_string()]);
        let none = read_all("(defun main () -> i64 1)", "t").expect("reads");
        assert_eq!(spec_of(&none, "main").expect("a spec"), ModuleSpec::main());
        let bad = read_all("(ns a (:require c.d))", "t").expect("reads");
        assert!(spec_of(&bad, "main").is_err());
    }
}
