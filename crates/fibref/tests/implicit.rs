//! The implicit modules (spec/syntax.md §5, stdlib design §6.2): modules
//! that every module of a program that is not the library's own sees
//! without a `:use`, as it sees the prelude. The list is
//! `modules::IMPLICIT_LIB`, the four facades of the library since the flip, so these
//! tests give the loader a list of their own (`try_load_with`) and
//! run the whole front end and the interpreter over a few files in a
//! scratch directory.

use std::path::PathBuf;

use fibref::eval::{run_checked, MacroEvaluator, STACK_BYTES};
use fibref::expand::ExpandCtx;
use fibref::expand_dump::{expand_files, Options};
use fibref::modules::{expand_all, try_load_with};
use fibref::roots::Roots;
use fibref::syntax::Form;
use fibref::types::prelude_forms;

/// A scratch directory holding `files` (path, text), removed on drop.
struct Tree(PathBuf);

impl Tree {
    fn new(label: &str, files: &[(&str, &str)]) -> Tree {
        let dir =
            std::env::temp_dir().join(format!("fibber-implicit-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in files {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().expect("a parent")).expect("dirs");
            std::fs::write(file, text).expect("write");
        }
        Tree(dir)
    }

    fn main(&self) -> String {
        self.0.join("main.fib").to_string_lossy().into_owned()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The result of `main` of `files`' `main.fib` with `implicit` as the
/// implicit modules, or the words of the first thing that stopped it; the
/// audit of the run must be clean.
fn run(label: &str, files: &[(&str, &str)], implicit: &[&str]) -> Result<i64, String> {
    let tree = Tree::new(label, files);
    let file = tree.main();
    let source = std::fs::read_to_string(&file).expect("main.fib");
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, || pipeline(&source, &file, implicit))
            .expect("a thread")
            .join()
            .expect("the pipeline does not panic")
    })
}

fn pipeline(source: &str, file: &str, implicit: &[&str]) -> Result<i64, String> {
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx)?;
    let loaded =
        try_load_with(source, file, &Roots::default(), implicit).map_err(|e| e.to_string())?;
    let all: Vec<Form> = loaded.iter().flat_map(|l| l.forms.clone()).collect();
    let mut runner = MacroEvaluator::new(&all, prelude.clone());
    let modules = expand_all(loaded, &mut ctx, &mut runner).map_err(|e| e.to_string())?;
    let checked = fibref::own::check_modules(&modules, &prelude).map_err(|e| e.to_string())?;
    let (result, report) = run_checked(&checked);
    assert!(report.is_clean(), "the audit is not clean: {report:?}");
    result.map_err(|e| e.to_string())
}

/// A library module `fib.x`: a function, a macro whose template names
/// the module's function by its full name, and a private helper.
const FIB_X: &str = "(ns fib.x)
(defun twice (n: i64) -> i64 (+ n n))
(defun secret :private (n: i64) -> i64 n)
(defmacro double-it (e) `(fib.x/twice ~e))
";

fn is_err_with(r: Result<i64, String>, text: &str) {
    match r {
        Err(e) => assert!(e.contains(text), "wanted `{text}` in: {e}"),
        Ok(n) => panic!("wanted an error with `{text}`, got result {n}"),
    }
}

#[test]
fn a_program_with_no_ns_sees_the_implicit_names_unqualified_and_qualified() {
    let main = "(defun main () -> i64 (+ (twice 20) (fib.x/twice 1)))";
    let files = [("main.fib", main), ("fib/x.fib", FIB_X)];
    assert_eq!(run("unqual", &files, &["fib.x"]), Ok(42));
    // the control: with no implicit module the same program is refused
    is_err_with(run("unqual-off", &files, &[]), "unbound name twice");
}

#[test]
fn a_macro_of_an_implicit_module_is_visible_and_its_qualified_template_expands_anywhere() {
    let main = "(defun main () -> i64 (double-it 4))";
    let files = [("main.fib", main), ("fib/x.fib", FIB_X)];
    assert_eq!(run("macro", &files, &["fib.x"]), Ok(8));
    let qualified = "(defun main () -> i64 (fib.x/double-it 5))";
    let files = [("main.fib", qualified), ("fib/x.fib", FIB_X)];
    assert_eq!(run("macro-qualified", &files, &["fib.x"]), Ok(10));
    is_err_with(run("macro-off", &files, &[]), "double-it");
}

#[test]
fn a_template_naming_an_implicit_module_expands_in_a_module_that_never_named_it() {
    // `lib.m` is no implicit module; its macro's template names fib.x/twice and is
    // expanded in `main`, which has not used fib.x (probes m2 and m3 of the plan)
    let main = "(ns main (:use lib.m))\n(defun main () -> i64 (twice-it 4))";
    let lib = "(ns lib.m)\n(defmacro twice-it (e) `(fib.x/twice ~e))\n";
    let files = [("main.fib", main), ("lib/m.fib", lib), ("fib/x.fib", FIB_X)];
    assert_eq!(run("template", &files, &["fib.x"]), Ok(8));
    is_err_with(run("template-off", &files, &[]), "twice");
}

#[test]
fn a_user_module_sees_them_too_and_a_private_name_is_not_exported() {
    let main = "(ns main (:use util))\n(defun main () -> i64 (go))";
    let util = "(ns util)\n(defun go () -> i64 (twice 5))\n";
    let files = [("main.fib", main), ("util.fib", util), ("fib/x.fib", FIB_X)];
    assert_eq!(run("user-module", &files, &["fib.x"]), Ok(10));
    let hidden = "(defun main () -> i64 (secret 1))";
    let files = [("main.fib", hidden), ("fib/x.fib", FIB_X)];
    is_err_with(
        run("private", &files, &["fib.x"]),
        "secret is private to fib.x; it is not exported",
    );
}

#[test]
fn a_used_module_or_a_local_definition_shadows_an_implicit_name_with_no_clash() {
    let mine = "(ns mine)\n(defun twice (n: i64) -> i64 (* n 3))\n";
    let used = "(ns main (:use mine))\n(defun main () -> i64 (twice 5))";
    let files = [("main.fib", used), ("mine.fib", mine), ("fib/x.fib", FIB_X)];
    assert_eq!(run("shadow-use", &files, &["fib.x"]), Ok(15));
    let local = "(defun twice (n: i64) -> i64 (* n 4))\n(defun main () -> i64 (twice 5))";
    let files = [("main.fib", local), ("fib/x.fib", FIB_X)];
    assert_eq!(run("shadow-local", &files, &["fib.x"]), Ok(20));
    // an alias of the program shadows the full name of an implicit module
    let alias = "(ns main (:require [mine :as fib.x]))\n(defun main () -> i64 (fib.x/twice 5))";
    let files = [
        ("main.fib", alias),
        ("mine.fib", mine),
        ("fib/x.fib", FIB_X),
    ];
    assert_eq!(run("shadow-alias", &files, &["fib.x"]), Ok(15));
}

#[test]
fn two_implicit_modules_exporting_one_name_are_an_error_that_names_both() {
    let x = "(ns fib.x)\n(defun pick () -> i64 1)\n";
    let y = "(ns fib.y)\n(defun pick () -> i64 2)\n";
    let bare = "(defun main () -> i64 (pick))";
    let files = [("main.fib", bare), ("fib/x.fib", x), ("fib/y.fib", y)];
    is_err_with(
        run("twice", &files, &["fib.x", "fib.y"]),
        "pick is exported by both fib.x and fib.y; write fib.x/pick or fib.y/pick",
    );
    let qualified = "(defun main () -> i64 (+ (fib.x/pick) (* 10 (fib.y/pick))))";
    let files = [("main.fib", qualified), ("fib/x.fib", x), ("fib/y.fib", y)];
    assert_eq!(run("twice-qualified", &files, &["fib.x", "fib.y"]), Ok(21));
    // the same definition exported by both (a part each re-exports) is one name
    let part = "(ns fib.p)\n(defun pick () -> i64 3)\n";
    let (x, y) = (
        "(ns fib.x (:export-from fib.p))",
        "(ns fib.y (:export-from fib.p))",
    );
    let files = [
        ("main.fib", bare),
        ("fib/x.fib", x),
        ("fib/y.fib", y),
        ("fib/p.fib", part),
    ];
    assert_eq!(run("twice-same", &files, &["fib.x", "fib.y"]), Ok(3));
}

#[test]
fn a_facade_is_implicit_with_its_parts_and_the_library_itself_sees_no_implicit_module() {
    let facade = "(ns fib.f (:export-from fib.f.a fib.f.b))";
    let a = "(ns fib.f.a)\n(defun from-a () -> i64 1)\n(defmacro mac-a (e) `(+ ~e 100))\n";
    let b = "(ns fib.f.b)\n(defun from-b () -> i64 20)\n";
    let main = "(defun main () -> i64 (+ (mac-a (from-a)) (from-b)))";
    let lib = [
        ("fib/f.fib", facade),
        ("fib/f/a.fib", a),
        ("fib/f/b.fib", b),
    ];
    let files = [&[("main.fib", main)][..], &lib].concat();
    assert_eq!(run("facade", &files, &["fib.f"]), Ok(121));
    // a library module that does not name the facade does not see it
    let user = "(ns main (:require [fib.w :as w]))\n(defun main () -> i64 (w/go))";
    let w = "(ns fib.w)\n(defun go () -> i64 (from-b))\n";
    let files = [&[("main.fib", user), ("fib/w.fib", w)][..], &lib].concat();
    is_err_with(
        run("library-blind", &files, &["fib.f"]),
        "unbound name from-b",
    );
    let w = "(ns fib.w (:use fib.f))\n(defun go () -> i64 (from-b))\n";
    let files = [&[("main.fib", user), ("fib/w.fib", w)][..], &lib].concat();
    assert_eq!(run("library-uses", &files, &["fib.f"]), Ok(20));
}

#[test]
fn the_expansion_dump_leaves_the_implicit_modules_out_unless_asked() {
    let main = "(defun main () -> i64 (double-it 4))";
    let tree = Tree::new("dump", &[("main.fib", main), ("fib/x.fib", FIB_X)]);
    let file = tree.main();
    let dump = |opts: Options| expand_files(std::slice::from_ref(&file), &opts);
    let lib = Some(vec!["fib.x".to_string()]);
    let (plain, status) = dump(Options {
        implicit_lib: lib.clone(),
        ..Options::default()
    });
    assert_eq!(status, 0, "{plain}");
    assert!(plain.contains("-- module main "), "{plain}");
    assert!(!plain.contains("-- module fib.x"), "{plain}");
    // the macro of the implicit module was expanded all the same
    assert!(plain.contains("sym \"fib.x/twice\""), "{plain}");
    let (shown, _) = dump(Options {
        implicit: true,
        implicit_lib: lib,
        ..Options::default()
    });
    assert!(shown.contains("-- module fib.x "), "{shown}");
    assert!(
        shown.find("-- module fib.x ") < shown.find("-- module main "),
        "{shown}"
    );
    // no implicit module: what the dump has always been
    let (none, _) = dump(Options::default());
    assert!(none.contains("sym \"double-it\""), "{none}");
    assert!(!none.contains("fib.x/twice"), "{none}");
}

#[test]
fn the_context_names_the_implicit_modules_a_module_sees_only_with_the_flag() {
    let main = "(defun main () -> i64 (double-it 4))";
    let tree = Tree::new("context", &[("main.fib", main), ("fib/x.fib", FIB_X)]);
    let file = tree.main();
    let with = |implicit: bool| {
        let opts = Options {
            context: true,
            implicit,
            implicit_lib: Some(vec!["fib.x".to_string()]),
            ..Options::default()
        };
        expand_files(std::slice::from_ref(&file), &opts).0
    };
    assert!(!with(false).contains("implicit \"fib.x\""));
    assert!(with(true).contains("  implicit \"fib.x\""));
}

#[test]
fn a_module_an_implicit_module_uses_is_read_before_it_and_sees_no_implicit_module() {
    // `helper` is read for `fib.x`, so it cannot see `fib.x` as an implicit module
    // (it is loaded before it); the program sees `fib.x`'s names, not `helper`'s
    let x = "(ns fib.x (:use helper))\n(defun twice-base () -> i64 (+ (base) (base)))\n";
    let helper = "(ns helper)\n(defun base () -> i64 21)\n";
    let main = "(defun main () -> i64 (twice-base))";
    let files = [("main.fib", main), ("fib/x.fib", x), ("helper.fib", helper)];
    assert_eq!(run("dependency", &files, &["fib.x"]), Ok(42));
    let bare = "(defun main () -> i64 (base))";
    let files = [("main.fib", bare), ("fib/x.fib", x), ("helper.fib", helper)];
    is_err_with(
        run("dependency-hidden", &files, &["fib.x"]),
        "unbound name base",
    );
}

#[test]
fn the_private_name_a_message_names_is_a_used_modules_before_an_implicit_ones() {
    // both `mine` and `fib.x` have a private `secret`, so neither is visible to `main`;
    // the message names the first of the chain: itself, its `:use`s, the implicit modules
    let mine = "(ns mine)\n(defun secret :private () -> i64 1)\n";
    let main = "(ns main (:use mine))\n(defun main () -> i64 (secret 1))";
    let files = [("main.fib", main), ("mine.fib", mine), ("fib/x.fib", FIB_X)];
    is_err_with(
        run("private-order", &files, &["fib.x"]),
        "secret is private to mine; it is not exported",
    );
}

#[test]
fn the_full_name_of_a_used_module_shadows_an_alias_of_the_same_spelling() {
    // `types/decls.rs` (`add_module`): a used module's full name beats an alias, for names
    // of functions. (A *macro* `mine/m` goes to the alias: `ExpandCtx::macro_def` looks at
    // aliases first and knows no used module's full name; an edge the reviewer reported.)
    let (mine, other) = (
        "(ns mine)\n(defun f () -> i64 1)\n",
        "(ns other)\n(defun f () -> i64 2)\n",
    );
    let main = "(ns main (:use mine) (:require [other :as mine]))\n(defun main () -> i64 (mine/f))";
    let files = [("main.fib", main), ("mine.fib", mine), ("other.fib", other)];
    assert_eq!(run("alias-vs-use", &files, &[]), Ok(1));
}

#[test]
fn two_parts_of_one_implicit_facade_that_export_one_name_are_an_error_too() {
    let facade = "(ns fib.f (:export-from fib.f.a fib.f.b))";
    let a = "(ns fib.f.a)\n(defun pick () -> i64 1)\n";
    let b = "(ns fib.f.b)\n(defun pick () -> i64 2)\n";
    let main = "(defun main () -> i64 (pick))";
    let files = [
        ("main.fib", main),
        ("fib/f.fib", facade),
        ("fib/f/a.fib", a),
        ("fib/f/b.fib", b),
    ];
    is_err_with(
        run("parts", &files, &["fib.f"]),
        "pick is exported by both fib.f.a and fib.f.b",
    );
}

#[test]
fn an_error_in_an_implicit_module_is_the_last_record_under_its_own_module_line() {
    let bad = "(ns fib.bad)\n(defmacro if (a) a)\n";
    let main = "(defun main () -> i64 1)";
    let tree = Tree::new("bad-implicit", &[("main.fib", main), ("fib/bad.fib", bad)]);
    let file = tree.main();
    let opts = Options {
        implicit_lib: Some(vec!["fib.bad".to_string()]),
        ..Options::default()
    };
    let (text, status) = expand_files(std::slice::from_ref(&file), &opts);
    assert_eq!(status, 1, "{text}");
    let head = text
        .find("-- module fib.bad ")
        .unwrap_or_else(|| panic!("no section: {text}"));
    assert!(text[head..].contains("\nerror "), "{text}");
    assert!(!text.contains("-- module main "), "{text}");
}

#[test]
fn the_context_of_an_implicit_module_is_printed_only_with_the_flag_too() {
    let main = "(defun main () -> i64 (double-it 4))";
    let tree = Tree::new(
        "context-sections",
        &[("main.fib", main), ("fib/x.fib", FIB_X)],
    );
    let file = tree.main();
    let with = |implicit: bool| {
        let opts = Options {
            context: true,
            implicit,
            implicit_lib: Some(vec!["fib.x".to_string()]),
            ..Options::default()
        };
        expand_files(std::slice::from_ref(&file), &opts).0
    };
    let hidden = with(false);
    assert!(hidden.contains("-- context main"), "{hidden}");
    assert!(!hidden.contains("-- context fib.x"), "{hidden}");
    let shown = with(true);
    assert!(shown.contains("-- context fib.x"), "{shown}");
    assert!(
        shown.find("-- context fib.x") < shown.find("-- context main"),
        "{shown}"
    );
}
