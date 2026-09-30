//! Modules (syntax §5): a program is the file given and the modules its
//! `ns` clauses `:require` or `:use`, found under that file's directory
//! (`a.b` at `a/b.fib`), read once each in dependency order, the main
//! module last. Macros are visible across the modules loaded together
//! unqualified (the expander keeps one table), which is wider than §5's
//! `:use` alone gives; qualifying a macro by an alias is not supported.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::expand::{expand_program, ExpandCtx, ExpandError, MacroRunner};
use crate::syntax::{read_all, Form, FormKind};

/// What a module's `ns` form says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleSpec {
    pub ns: String,
    /// `(:require [a.b :as ab])`: alias, module.
    pub requires: Vec<(String, String)>,
    /// `(:use a.b)`.
    pub uses: Vec<String>,
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

/// The spec a module's forms give: the first form when it is an `ns`,
/// else the default with no clauses.
pub fn spec_of(forms: &[Form], default_ns: &str) -> Result<ModuleSpec, String> {
    let Some(first) = forms.first() else {
        return Ok(ModuleSpec {
            ns: default_ns.into(),
            ..ModuleSpec::default()
        });
    };
    let Some(items) = first.as_list() else {
        return Ok(ModuleSpec {
            ns: default_ns.into(),
            ..ModuleSpec::default()
        });
    };
    if items.first().and_then(Form::as_sym) != Some("ns") {
        return Ok(ModuleSpec {
            ns: default_ns.into(),
            ..ModuleSpec::default()
        });
    }
    let bad = |what: &str| format!("{}: {what}", first.pos);
    let ns = items
        .get(1)
        .and_then(Form::as_sym)
        .ok_or_else(|| bad("ns needs a name"))?;
    let mut spec = ModuleSpec {
        ns: ns.to_string(),
        ..ModuleSpec::default()
    };
    for clause in &items[2..] {
        let parts = clause
            .as_list()
            .ok_or_else(|| bad("an ns clause is a list: (:require ..) or (:use ..)"))?;
        let key = match parts.first().map(|f| &f.kind) {
            Some(FormKind::Kw(k)) => k.as_str(),
            _ => return Err(bad("an ns clause starts with :require or :use")),
        };
        match key {
            "require" => {
                for r in &parts[1..] {
                    let FormKind::Vec(v) = &r.kind else {
                        return Err(bad("a :require item is [name :as alias]"));
                    };
                    let (name, alias) = match (
                        v.first().and_then(Form::as_sym),
                        v.get(1).map(|f| &f.kind),
                        v.get(2).and_then(Form::as_sym),
                    ) {
                        (Some(n), Some(FormKind::Kw(k)), Some(a)) if k == "as" && v.len() == 3 => {
                            (n, a)
                        }
                        _ => return Err(bad("a :require item is [name :as alias]")),
                    };
                    spec.requires.push((alias.to_string(), name.to_string()));
                }
            }
            "use" => {
                for u in &parts[1..] {
                    let name = u
                        .as_sym()
                        .ok_or_else(|| bad("a :use item is a module name"))?;
                    spec.uses.push(name.to_string());
                }
            }
            other => return Err(bad(&format!("unknown ns clause :{other}"))),
        }
    }
    Ok(spec)
}

/// The file of module `ns` under `root`.
fn file_of(root: &Path, ns: &str) -> PathBuf {
    root.join(format!("{}.fib", ns.replace('.', "/")))
}

/// Loads the program whose main module is `source` (read from `file`):
/// every module it depends on, from `file`'s directory, once each in
/// dependency order, then the main module.
pub fn load(source: &str, file: &str) -> Result<Vec<Loaded>, String> {
    let forms = read_all(source, file).map_err(|e| e.to_string())?;
    let spec = spec_of(&forms, "main")?;
    let root = Path::new(file)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut done: HashSet<String> = HashSet::new();
    let mut active: Vec<String> = vec![spec.ns.clone()];
    for dep in spec.deps() {
        visit(&root, dep, &mut active, &mut done, &mut out)?;
    }
    out.push(Loaded {
        spec,
        file: file.to_string(),
        forms,
    });
    Ok(out)
}

fn visit(
    root: &Path,
    ns: &str,
    active: &mut Vec<String>,
    done: &mut HashSet<String>,
    out: &mut Vec<Loaded>,
) -> Result<(), String> {
    if done.contains(ns) {
        return Ok(());
    }
    if active.iter().any(|a| a == ns) {
        return Err(format!(
            "modules require each other in a cycle: {} -> {ns}",
            active.join(" -> ")
        ));
    }
    let path = file_of(root, ns);
    let file = path.to_string_lossy().into_owned();
    let source =
        std::fs::read_to_string(&path).map_err(|e| format!("module {ns} is not at {file}: {e}"))?;
    let forms = read_all(&source, &file).map_err(|e| e.to_string())?;
    let spec = spec_of(&forms, ns)?;
    if spec.ns != ns {
        return Err(format!(
            "{file} declares (ns {}) but is required as {ns}",
            spec.ns
        ));
    }
    active.push(ns.to_string());
    for dep in spec.deps() {
        visit(root, dep, active, done, out)?;
    }
    active.pop();
    done.insert(ns.to_string());
    out.push(Loaded { spec, file, forms });
    Ok(())
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
}
