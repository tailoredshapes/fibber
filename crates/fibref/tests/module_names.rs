//! How a name used in a module is resolved between modules (syntax §5):
//! two `:use`d modules that export different definitions of one name
//! make the name an error when it is referenced unqualified, in each
//! space of names (values, types, protocols); a local definition and a
//! qualified reference are no clash; a `:use`d module shadows the prelude
//! silently, as it always did. The programs are checked, not run: the
//! cases under cases/modules run the same rules in both tools.

use std::path::PathBuf;

use fibref::own::check_source;

/// A directory of modules, removed when it goes out of scope.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str, files: &[(&str, &str)]) -> Dir {
        let dir = std::env::temp_dir().join(format!("fibber-{name}-{}", std::process::id()));
        for (path, text) in files {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().expect("a parent")).expect("dirs");
            std::fs::write(file, text).expect("write");
        }
        Dir(dir)
    }

    /// What the front end says about `main`: its error, or `ok`.
    fn check(&self, main: &str) -> String {
        let file = self.0.join("main.fib");
        std::fs::write(&file, main).expect("write main");
        match check_source(main, &file.to_string_lossy()) {
            Ok(_) => "ok".to_string(),
            Err(e) => e.to_string(),
        }
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const BOTH: &str = "(ns main (:use one two))\n";

fn two_modules(name: &str, body: &str) -> Dir {
    let one = format!("(ns one)\n{body}");
    let two = format!("(ns two)\n{body}");
    Dir::new(name, &[("one.fib", &one), ("two.fib", &two)])
}

#[test]
fn two_functions_of_one_name_are_an_error_where_the_name_is_used_bare() {
    let d = two_modules("fn", "(defun peek (x: i64) -> i64 x)");
    let said = d.check(&format!("{BOTH}(defun main () -> i64 (peek 3))"));
    assert!(
        said.ends_with("peek is exported by both one and two; write one/peek or two/peek"),
        "{said}"
    );
}

#[test]
fn the_error_is_at_the_reference_so_an_unused_clash_is_none() {
    let d = two_modules("unused", "(defun peek (x: i64) -> i64 x)");
    assert_eq!(d.check(&format!("{BOTH}(defun main () -> i64 3)")), "ok");
}

#[test]
fn a_local_definition_and_a_qualified_name_are_no_clash() {
    let d = two_modules("local", "(defun peek (x: i64) -> i64 x)");
    let local = format!("{BOTH}(defun peek (x: i64) -> i64 x)\n(defun main () -> i64 (peek 3))");
    assert_eq!(d.check(&local), "ok");
    let qualified = format!("{BOTH}(defun main () -> i64 (+ (one/peek 1) (two/peek 2)))");
    assert_eq!(d.check(&qualified), "ok");
}

#[test]
fn two_structs_of_one_name_are_an_error_as_a_type_and_as_a_constructor() {
    let d = two_modules("struct", "(defstruct Pt (x: i64))");
    let ty = d.check(&format!(
        "{BOTH}(defun f (p: Pt) -> i64 0)\n(defun main () -> i64 0)"
    ));
    assert!(
        ty.ends_with("Pt is exported by both one and two; write one/Pt or two/Pt"),
        "{ty}"
    );
    let ctor = d.check(&format!("{BOTH}(defun main () -> i64 (. (Pt 1) x))"));
    assert!(
        ctor.ends_with("Pt is exported by both one and two; write one/Pt or two/Pt"),
        "{ctor}"
    );
}

#[test]
fn two_variants_of_one_name_are_an_error_in_a_pattern() {
    let d = two_modules("variant", "(defenum Shape (Dot x: i64) (Gap))");
    let said = d.check(&format!(
        "{BOTH}(defun main () -> i64 (match 1 ((Dot x) x) (_ 0)))"
    ));
    assert!(
        said.contains("Dot is exported by both one and two"),
        "{said}"
    );
}

#[test]
fn two_protocols_of_one_name_are_an_error() {
    let d = two_modules("proto", "(defprotocol Sized (size (self) -> i64))");
    let said = d.check(&format!(
        "{BOTH}(impl Sized str (size (self) 1))\n(defun main () -> i64 0)"
    ));
    assert!(
        said.ends_with("Sized is exported by both one and two; write one/Sized or two/Sized"),
        "{said}"
    );
}

#[test]
fn a_private_definition_is_not_exported_so_it_clashes_with_nothing() {
    let one = "(ns one)\n(defun peek :private (x: i64) -> i64 x)";
    let two = "(ns two)\n(defun peek (x: i64) -> i64 (+ x 1))";
    let d = Dir::new("private", &[("one.fib", one), ("two.fib", two)]);
    assert_eq!(
        d.check(&format!("{BOTH}(defun main () -> i64 (peek 3))")),
        "ok"
    );
}

#[test]
fn a_used_module_shadows_the_prelude_and_the_prelude_is_not_a_second_use() {
    // `unwrap-or` is the prelude's; one module defines its own. A program
    // that :uses it and the prelude implicitly is not a clash.
    let one = "(ns one)\n(defun unwrap-or (o: (Option i64) d: i64) -> i64 d)";
    let d = Dir::new("prelude", &[("one.fib", one)]);
    let main = "(ns main (:use one))\n(defun main () -> i64 (unwrap-or (some 1) 7))";
    assert_eq!(d.check(main), "ok");
}

#[test]
fn an_export_from_clause_is_a_use_that_is_also_an_export() {
    use fibref::modules::spec_of;
    use fibref::syntax::read_all;
    let forms = read_all("(ns f (:use a) (:export-from a b))", "t").expect("reads");
    let spec = spec_of(&forms, "main").expect("a spec");
    // a is used once, though it is named twice; b is used because it is
    // re-exported.
    assert_eq!(spec.uses, ["a", "b"]);
    assert_eq!(spec.exports, ["a", "b"]);
    assert!(
        spec.deps().any(|d| d == "b"),
        "a re-exported module is required"
    );
    let bad = read_all("(ns f (:export-from 1))", "t").expect("reads");
    let e = spec_of(&bad, "main").expect_err("not a module name");
    assert!(e.ends_with("an :export-from item is a module name"), "{e}");
}

#[test]
fn a_facade_re_exports_every_space_of_names_and_not_what_is_private() {
    let a = "(ns a)\n(defstruct Pt (x: i64))\n(defprotocol Sized (size (self) -> i64))\n\
             (defun fa () -> i64 1)\n(defun hidden :private () -> i64 2)\n";
    let d = Dir::new(
        "facade",
        &[("a.fib", a), ("f.fib", "(ns f (:export-from a))\n")],
    );
    let ok = |body: &str| d.check(&format!("(ns main (:use f))\n{body}"));
    assert_eq!(
        ok("(defun g (p: Pt) -> i64 (fa))\n(defun main () -> i64 0)"),
        "ok"
    );
    assert_eq!(
        ok("(impl Sized str (size (self) 1))\n(defun main () -> i64 0)"),
        "ok"
    );
    let private = ok("(defun main () -> i64 (hidden))");
    assert!(private.ends_with("unbound name hidden"), "{private}");
    // Not a :use of the facade: nothing is in scope bare, everything by alias.
    let by_alias = d.check(
        "(ns main (:require [f :as f]))\n(defun g (p: f/Pt) -> i64 (f/fa))\n(defun main () -> i64 0)",
    );
    assert_eq!(by_alias, "ok");
}
