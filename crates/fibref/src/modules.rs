//! Modules (syntax §5): a program is the file given and the modules its
//! `ns` clauses `:require`, `:use` or `:export-from`, found beside that
//! file and then under the library roots (`a.b` at `a/b.fib`; the order
//! is that of `roots.rs`), read once each in dependency order, the main
//! module last. A module's macros reach the modules that `:use` it
//! unqualified and the ones that `:require` it through the alias, a
//! `:private` macro its own module only (`ExpandCtx::macro_def`).

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::expand::{expand_program, ExpandCtx, ExpandError, MacroRunner};
use crate::roots::{Lookup, Roots};
use crate::syntax::{read_all, Form, FormKind, Pos, ReadError};

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

/// A module as read: its spec, its file and its forms before expansion.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub spec: ModuleSpec,
    pub file: String,
    pub forms: Vec<Form>,
}

/// Why a program could not be loaded: what [`try_load`] reports, which
/// [`load`] shows as the text of its `Display` (the expansion dump,
/// spec/bootstrap.md §5, tells the kinds apart).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// A module's text did not read.
    Read(ReadError),
    /// An `ns` form that does not parse: where it is and what is wrong.
    Spec { pos: Pos, what: String },
    /// A module is not at the file its name gives (missing, a directory,
    /// not UTF-8); `cause` is the operating system's words.
    Missing {
        ns: String,
        file: String,
        cause: String,
    },
    /// Modules that require each other: `path` leads from the main
    /// module to the one that `ns` requires again.
    Cycle { path: Vec<String>, ns: String },
    /// A file declares another `ns` than the one it is required as.
    Mismatch {
        file: String,
        declared: String,
        required: String,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Read(e) => write!(f, "{e}"),
            LoadError::Spec { pos, what } => write!(f, "{pos}: {what}"),
            LoadError::Missing { ns, file, cause } => {
                write!(f, "module {ns} is not at {file}: {cause}")
            }
            LoadError::Cycle { path, ns } => write!(
                f,
                "modules require each other in a cycle: {} -> {ns}",
                path.join(" -> ")
            ),
            LoadError::Mismatch {
                file,
                declared,
                required,
            } => write!(
                f,
                "{file} declares (ns {declared}) but is required as {required}"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// The spec a module's forms give: the first form when it is an `ns`,
/// else the default with no clauses.
pub fn spec_of(forms: &[Form], default_ns: &str) -> Result<ModuleSpec, String> {
    spec_in(forms, default_ns).map_err(|e| e.to_string())
}

fn spec_in(forms: &[Form], default_ns: &str) -> Result<ModuleSpec, LoadError> {
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

/// Loads the program whose main module is `source` (read from `file`):
/// every module it depends on, found beside `file` or under the built-in
/// root only, once each in dependency order, then the main module.
pub fn load(source: &str, file: &str) -> Result<Vec<Loaded>, String> {
    load_in(source, file, &Roots::default())
}

/// [`load`] with the library roots of the command line (spec/syntax.md §5).
pub fn load_in(source: &str, file: &str, roots: &Roots) -> Result<Vec<Loaded>, String> {
    try_load_in(source, file, roots).map_err(|e| e.to_string())
}

/// [`load`] with the failure as a [`LoadError`], which says which of the
/// ways it was.
pub fn try_load(source: &str, file: &str) -> Result<Vec<Loaded>, LoadError> {
    try_load_in(source, file, &Roots::default())
}

/// [`try_load`] with the library roots of the command line: a module is
/// found beside `file`, then under each root in order, then in the
/// built-in root (`roots.rs`).
pub fn try_load_in(source: &str, file: &str, roots: &Roots) -> Result<Vec<Loaded>, LoadError> {
    let forms = read_all(source, file).map_err(LoadError::Read)?;
    let spec = spec_in(&forms, "main")?;
    let mut loader = Loader {
        main_dir: Path::new(file)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        roots,
        active: vec![spec.ns.clone()],
        done: HashSet::new(),
        out: Vec::new(),
    };
    for dep in spec.deps() {
        loader.visit(dep)?;
    }
    let mut out = loader.out;
    out.push(Loaded {
        spec,
        file: file.to_string(),
        forms,
    });
    Ok(out)
}

/// The modules read so far and the ones being read.
struct Loader<'a> {
    main_dir: PathBuf,
    roots: &'a Roots,
    /// The modules whose dependencies are being read, outermost first.
    active: Vec<String>,
    done: HashSet<String>,
    out: Vec<Loaded>,
}

impl Loader<'_> {
    fn visit(&mut self, ns: &str) -> Result<(), LoadError> {
        if self.done.contains(ns) {
            return Ok(());
        }
        if self.active.iter().any(|a| a == ns) {
            return Err(LoadError::Cycle {
                path: self.active.clone(),
                ns: ns.to_string(),
            });
        }
        let (file, source) = match self.roots.find(&self.main_dir, ns) {
            Lookup::Found { file, source } => (file, source),
            Lookup::Unreadable { file, cause } => {
                return Err(LoadError::Missing {
                    ns: ns.to_string(),
                    file,
                    cause,
                })
            }
            Lookup::Missing { file, cause, also } => {
                let cause = match also.is_empty() {
                    true => cause,
                    false => format!("{cause}; nor at {}", also.join(", ")),
                };
                return Err(LoadError::Missing {
                    ns: ns.to_string(),
                    file,
                    cause,
                });
            }
        };
        let forms = read_all(&source, &file).map_err(LoadError::Read)?;
        let spec = spec_in(&forms, ns)?;
        if spec.ns != ns {
            return Err(LoadError::Mismatch {
                file,
                declared: spec.ns,
                required: ns.to_string(),
            });
        }
        self.active.push(ns.to_string());
        for dep in spec.deps() {
            self.visit(dep)?;
        }
        self.active.pop();
        self.done.insert(ns.to_string());
        self.out.push(Loaded { spec, file, forms });
        Ok(())
    }
}

/// Starts expanding the module `spec` describes in `ctx`: its `:use`s
/// and `:require` aliases are what a macro name reaches
/// (`ExpandCtx::begin_module`). [`expand_all`] does this for each module;
/// the expansion dump does it step by step.
pub fn begin_spec(ctx: &mut ExpandCtx, spec: &ModuleSpec) {
    let aliases = spec
        .requires
        .iter()
        .map(|(alias, ns)| (alias.clone(), ns.clone()))
        .collect();
    ctx.begin_module(&spec.ns, &spec.uses, aliases);
    ctx.reexport(&spec.ns, &spec.exports);
}

/// Expands every loaded module in order in one context, each ended
/// for the next (syntax §5: private types), with the macro runner.
pub fn expand_all(
    loaded: Vec<Loaded>,
    ctx: &mut ExpandCtx,
    runner: &mut dyn MacroRunner,
) -> Result<Vec<(ModuleSpec, Vec<Form>)>, ExpandError> {
    let mut out = Vec::with_capacity(loaded.len());
    for l in loaded {
        begin_spec(ctx, &l.spec);
        let forms = expand_program(l.forms, ctx, runner)?;
        ctx.end_module();
        out.push((l.spec, forms));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn loading_finds_modules_under_the_main_files_directory_in_dependency_order() {
        let dir = std::env::temp_dir().join(format!("fibber-modules-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("geo")).expect("dirs");
        std::fs::write(
            dir.join("geo/point.fib"),
            "(ns geo.point (:use util))\n(defun p () -> i64 (u))",
        )
        .expect("write");
        std::fs::write(dir.join("util.fib"), "(ns util)\n(defun u () -> i64 1)").expect("write");
        let main =
            "(ns main (:require [geo.point :as pt]) (:use util))\n(defun main () -> i64 (pt/p))";
        let file = dir.join("main.fib").to_string_lossy().into_owned();
        let loaded = load(main, &file).expect("loads");
        let order: Vec<&str> = loaded.iter().map(|l| l.spec.ns.as_str()).collect();
        assert_eq!(order, ["util", "geo.point", "main"]);
        std::fs::write(
            dir.join("util.fib"),
            "(ns util (:use geo.point))\n(defun u () -> i64 1)",
        )
        .expect("write");
        let err = load(main, &file).expect_err("a cycle");
        assert!(err.contains("cycle"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Writes `files` (name, text) into a fresh directory and returns it
    /// with the path of `main.fib`.
    fn program(label: &str, files: &[(&str, &str)]) -> (PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("fibber-load-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dirs");
        for (name, text) in files {
            std::fs::write(dir.join(name), text).expect("write");
        }
        let main = dir.join("main.fib").to_string_lossy().into_owned();
        (dir, main)
    }

    #[test]
    fn a_load_error_says_which_way_the_load_failed_and_prints_as_load_did() {
        let main = "(ns main (:use u))\n(defun main () -> i64 1)";
        let (dir, file) = program("missing", &[("main.fib", main)]);
        let e = try_load(main, &file).expect_err("u is missing");
        assert!(
            matches!(&e, LoadError::Missing { ns, .. } if ns == "u"),
            "{e:?}"
        );
        assert_eq!(load(main, &file).expect_err("u is missing"), e.to_string());
        assert!(e.to_string().starts_with("module u is not at "), "{e}");

        std::fs::write(dir.join("u.fib"), "(ns v)").expect("write");
        let e = try_load(main, &file).expect_err("u declares v");
        assert!(
            matches!(&e, LoadError::Mismatch { declared, required, .. }
                if declared == "v" && required == "u"),
            "{e:?}"
        );
        assert!(
            e.to_string()
                .ends_with("declares (ns v) but is required as u"),
            "{e}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cycle_a_module_that_does_not_read_and_a_bad_ns_are_load_errors_of_their_own() {
        let main = "(ns main (:use u))\n(defun main () -> i64 1)";
        let (dir, file) = program("cycle", &[("main.fib", main)]);
        std::fs::write(dir.join("u.fib"), "(ns u (:use u))").expect("write");
        let e = try_load(main, &file).expect_err("u requires itself");
        assert_eq!(
            e,
            LoadError::Cycle {
                path: vec!["main".to_string(), "u".to_string()],
                ns: "u".to_string()
            }
        );
        assert_eq!(
            e.to_string(),
            "modules require each other in a cycle: main -> u -> u"
        );

        std::fs::write(dir.join("u.fib"), "(a").expect("write");
        let e = try_load(main, &file).expect_err("u does not read");
        assert!(
            matches!(&e, LoadError::Read(r) if r.pos.file.ends_with("u.fib")),
            "{e:?}"
        );

        let bad = "(ns main (:use 1))";
        let e = try_load(bad, &file).expect_err("a bad ns");
        assert!(
            matches!(&e, LoadError::Spec { what, .. } if what == "a :use item is a module name")
        );
        assert!(
            e.to_string().ends_with(": a :use item is a module name"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn begin_spec_gives_the_context_the_uses_and_aliases_of_the_module() {
        let forms = read_all("(ns a (:require [b.c :as bc]) (:use d))", "t").expect("reads");
        let spec = spec_of(&forms, "main").expect("a spec");
        let mut ctx = ExpandCtx::new();
        begin_spec(&mut ctx, &spec);
        let scope = ctx.scope();
        assert_eq!(scope.ns, "a");
        assert_eq!(scope.uses, vec!["d".to_string()]);
        assert_eq!(scope.aliases.get("bc").map(String::as_str), Some("b.c"));
    }
}
