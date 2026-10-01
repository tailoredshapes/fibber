//! The programs of the differential test (spec/bootstrap.md §5): every
//! program of `cases/`, `lib/` and `compiler/`, each told apart by
//! whether any module of it defines a macro, since only those need the
//! macro runner of stage 2b.

use std::path::{Path, PathBuf};

use crate::corpus::corpus_files;

/// One program: its main file, and whether it defines a macro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub main: PathBuf,
    pub defmacro: bool,
}

/// Every program under `root`, in path order: a file of
/// `cases/ownership`, the `main.fib` of each directory of `cases/modules`
/// (its other files are the modules it loads), every file of `lib/`
/// except `lib/prelude.fib` (which `--prelude` judges) and every file of
/// `compiler/`, each as a main file.
pub fn programs(root: &Path) -> Vec<Program> {
    let modules = root.join("cases/modules");
    let prelude = root.join("lib/prelude.fib");
    corpus_files(root)
        .into_iter()
        .filter(|f| f != &prelude)
        .filter(|f| !f.starts_with(&modules) || f.file_name().is_some_and(|n| n == "main.fib"))
        .map(|main| Program {
            defmacro: defines_macro(&main),
            main,
        })
        .collect()
}

/// Whether the program whose main file is `main` defines a macro in any
/// module: a `defmacro` form anywhere in the forms of the modules it
/// loads, or, when it does not load, in the text of the file.
pub fn defines_macro(main: &Path) -> bool {
    let Ok(source) = std::fs::read_to_string(main) else {
        return false;
    };
    let file = main.to_string_lossy();
    match fibref::modules::try_load(&source, &file) {
        Ok(loaded) => loaded.iter().any(|l| l.forms.iter().any(mentions_defmacro)),
        Err(_) => source.contains("defmacro"),
    }
}

fn mentions_defmacro(form: &fibref::syntax::Form) -> bool {
    use fibref::syntax::FormKind;
    match &form.kind {
        FormKind::Sym(s) => s == "defmacro",
        FormKind::List(items) | FormKind::Vec(items) | FormKind::Map(items) => {
            items.iter().any(mentions_defmacro)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;
    use crate::tool::repo_root;

    #[test]
    fn the_programs_are_the_cases_the_main_files_of_the_module_cases_and_the_compiler() {
        let root = repo_root();
        let all = programs(&root);
        let under = |d: &str| {
            all.iter()
                .filter(|p| p.main.starts_with(root.join(d)))
                .count()
        };
        assert!(
            under("cases/ownership") >= 190,
            "{}",
            under("cases/ownership")
        );
        assert!(under("compiler") >= 30, "{}", under("compiler"));
        let modules: Vec<&Program> = all
            .iter()
            .filter(|p| p.main.starts_with(root.join("cases/modules")))
            .collect();
        assert!(modules.len() >= 6, "{}", modules.len());
        assert!(modules
            .iter()
            .all(|p| p.main.file_name().is_some_and(|n| n == "main.fib")));
        assert!(all.iter().all(|p| p.main != root.join("lib/prelude.fib")));
        let mut sorted = all.clone();
        sorted.sort_by(|a, b| a.main.cmp(&b.main));
        assert_eq!(all, sorted);
    }

    #[test]
    fn a_macro_in_a_module_a_program_loads_counts_and_one_in_a_comment_does_not() {
        let dir = TempDir::new("programs-defmacro");
        let write = |name: &str, text: &str| {
            std::fs::write(dir.path().join(name), text).expect("writable");
            dir.path().join(name)
        };
        write("util.fib", "(ns util)\n(defmacro m () 1)\n");
        let uses = write(
            "uses.fib",
            "(ns main (:use util))\n(defun main () -> i64 1)\n",
        );
        let plain = write("plain.fib", "(defun main () -> i64 1) ; no defmacro here\n");
        let own = write("own.fib", "(do (defmacro m () 1))\n");
        let broken = write("broken.fib", "(defmacro m (");
        assert!(defines_macro(&uses), "through a :use");
        assert!(!defines_macro(&plain), "a comment is not a form");
        assert!(defines_macro(&own), "inside a do");
        assert!(defines_macro(&broken), "unreadable: the text decides");
        assert!(!defines_macro(&dir.path().join("absent.fib")));
    }

    #[test]
    fn the_corpus_has_programs_of_both_kinds() {
        let all = programs(&repo_root());
        let with = all.iter().filter(|p| p.defmacro).count();
        assert!(with >= 10, "only {with} programs define a macro");
        assert!(all.len() - with >= 100);
    }
}
