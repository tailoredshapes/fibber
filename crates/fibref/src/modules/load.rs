//! Finding and reading a program's modules: the main file's directory, the
//! library roots (`roots.rs`), each module once in dependency order, the
//! implicit modules first (syntax §5, `IMPLICIT_LIB`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::roots::{Lookup, Roots};
use crate::syntax::read_all;

use super::spec::spec_in;
use super::{LoadError, Loaded, ModuleSpec, IMPLICIT_LIB};

/// Whether a module named `ns` sees the implicit modules `implicit`
/// unqualified (syntax §5): every module but the library's own, which are
/// named `fib.*` (and `Long` and `Math`, the Java-named ones) and write
/// their `:use`s by hand, in the order the layers need, and but the
/// implicit modules themselves.
pub fn sees_implicit(ns: &str, implicit: &[&str]) -> bool {
    !(ns.starts_with("fib.") || ns == "Long" || ns == "Math" || implicit.contains(&ns))
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
/// built-in root (`roots.rs`), and the implicit modules of
/// [`IMPLICIT_LIB`] come before the program's.
pub fn try_load_in(source: &str, file: &str, roots: &Roots) -> Result<Vec<Loaded>, LoadError> {
    try_load_with(source, file, roots, IMPLICIT_LIB)
}

/// [`try_load_in`] with the implicit modules `implicit` instead of
/// [`IMPLICIT_LIB`], in the order they are loaded: the program sees them
/// as it sees the prelude (syntax §5), unless its main module is the
/// library's own or an implicit module, which loads none of them. Each is
/// read, with what it depends on, before anything else, and its `Loaded`
/// says so (`implicit`).
pub fn try_load_with(
    source: &str,
    file: &str,
    roots: &Roots,
    implicit: &[&str],
) -> Result<Vec<Loaded>, LoadError> {
    let forms = read_all(source, file).map_err(LoadError::Read)?;
    let mut spec = spec_in(&forms, "main")?;
    let mut loader = Loader {
        main_dir: Path::new(file)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        roots,
        implicit,
        in_implicit: false,
        active: vec![spec.ns.clone()],
        done: HashSet::new(),
        out: Vec::new(),
    };
    if sees_implicit(&spec.ns, implicit) {
        loader.in_implicit = true;
        for ns in implicit {
            loader.visit(ns)?;
        }
        loader.in_implicit = false;
        spec.implicit = implicit.iter().map(|s| s.to_string()).collect();
    }
    for dep in spec.deps() {
        loader.visit(dep)?;
    }
    let mut out = loader.out;
    out.push(Loaded {
        spec,
        file: file.to_string(),
        forms,
        implicit: false,
    });
    Ok(out)
}

/// The modules read so far and the ones being read.
struct Loader<'a> {
    main_dir: PathBuf,
    roots: &'a Roots,
    implicit: &'a [&'a str],
    /// Whether the modules read now are the implicit ones and what they
    /// depend on.
    in_implicit: bool,
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
        let mut spec = spec_in(&forms, ns)?;
        if spec.ns != ns {
            return Err(LoadError::Mismatch {
                file,
                declared: spec.ns,
                required: ns.to_string(),
            });
        }
        self.give_implicit(&mut spec);
        self.active.push(ns.to_string());
        for dep in spec.deps() {
            self.visit(dep)?;
        }
        self.active.pop();
        self.done.insert(ns.to_string());
        self.out.push(Loaded {
            spec,
            file,
            forms,
            implicit: self.in_implicit,
        });
        Ok(())
    }

    /// Gives `spec` the implicit modules, all of which are read before it
    /// (those loading them have none).
    fn give_implicit(&self, spec: &mut ModuleSpec) {
        if !self.in_implicit && sees_implicit(&spec.ns, self.implicit) {
            spec.implicit = self.implicit.iter().map(|s| s.to_string()).collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::{spec_of, LoadError};

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
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
            std::fs::write(path, text).expect("write");
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
    fn the_library_and_the_implicit_modules_themselves_see_no_implicit_module() {
        let implicit = ["fib.x", "y"];
        assert!(sees_implicit("main", &implicit));
        assert!(sees_implicit("util.fib", &implicit));
        for ns in ["fib.string", "fib.x.part", "Long", "Math", "y"] {
            assert!(!sees_implicit(ns, &implicit), "{ns}");
        }
    }

    const MAIN: &str = "(ns main (:use util))\n(defun main () -> i64 1)";

    #[test]
    fn the_implicit_modules_are_read_first_with_what_they_depend_on_and_marked() {
        let (dir, file) = program(
            "implicit",
            &[
                ("main.fib", MAIN),
                ("util.fib", "(ns util)"),
                ("fib/x.fib", "(ns fib.x (:export-from fib.x.part))"),
                ("fib/x/part.fib", "(ns fib.x.part)"),
                ("fib/z.fib", "(ns fib.z (:use fib.x))"),
            ],
        );
        let loaded =
            try_load_with(MAIN, &file, &Roots::default(), &["fib.z", "fib.x"]).expect("loads");
        let seen: Vec<(&str, bool)> = loaded
            .iter()
            .map(|l| (l.spec.ns.as_str(), l.implicit))
            .collect();
        assert_eq!(
            seen,
            [
                ("fib.x.part", true),
                ("fib.x", true),
                ("fib.z", true),
                ("util", false),
                ("main", false)
            ]
        );
        let implicit = |ns: &str| -> Vec<String> {
            let l = loaded.iter().find(|l| l.spec.ns == ns).expect("loaded");
            l.spec.implicit.clone()
        };
        assert_eq!(implicit("main"), ["fib.z", "fib.x"]);
        assert_eq!(implicit("util"), ["fib.z", "fib.x"]);
        for lib in ["fib.x.part", "fib.x", "fib.z"] {
            assert!(implicit(lib).is_empty(), "{lib}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_main_loads_no_implicit_module_and_none_means_none() {
        let lib_main = "(ns fib.m (:use fib.x))\n(defun main () -> i64 1)";
        let (dir, file) = program(
            "implicit-lib-main",
            &[("main.fib", lib_main), ("fib/x.fib", "(ns fib.x)")],
        );
        let none = try_load_with(lib_main, &file, &Roots::default(), &["fib.x"]).expect("loads");
        assert_eq!(none.len(), 2);
        assert!(none
            .iter()
            .all(|l| l.spec.implicit.is_empty() && !l.implicit));
        let plain = try_load_with(MAIN, &file, &Roots::default(), &[]);
        assert!(matches!(plain, Err(LoadError::Missing { ns, .. }) if ns == "util"));
        let spec = spec_of(&read_all(MAIN, "t").expect("reads"), "main").expect("a spec");
        assert!(spec.implicit.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_implicit_module_that_is_missing_is_the_error_of_a_missing_module() {
        let (dir, file) = program(
            "implicit-missing",
            &[("main.fib", "(defun main () -> i64 1)")],
        );
        let e = try_load_with(
            "(defun main () -> i64 1)",
            &file,
            &Roots::default(),
            &["fib.nope"],
        )
        .expect_err("fib.nope is nowhere");
        assert!(
            matches!(&e, LoadError::Missing { ns, .. } if ns == "fib.nope"),
            "{e:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
