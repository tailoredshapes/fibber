//! Programs of several modules (spec/bootstrap.md §5): the module driver
//! and the scoping of macros and private types, in generated variants.
//! `util` and `more` define macros (the same name in both, one `:private`),
//! structs (one `:private`), an enum and a reflection macro; `main` requires
//! and uses them in the ways of syntax §5, and calls what they define.
//! A few variants break the program: a module that is missing, a cycle,
//! a name that does not match its file, a malformed `ns`, a module that
//! does not read.

use crate::rng::Rng;

/// The text of each module's definitions that may be present; each is a
/// top-level form.
const UTIL_ITEMS: [&str; 8] = [
    "(defmacro twice (x) `(do ~x ~x))",
    "(defmacro hid :private (x) `(do ~x))",
    "(defmacro nf (n) (Int (count (struct-fields n)) :i64))",
    "(defstruct S (a: i64 b: str))",
    "(defstruct PS :private (a: i64))",
    "(defenum UE (UA) (UB n: i64))",
    "(defun one () -> i64 1)",
    "(derive Eq S)",
];

const MORE_ITEMS: [&str; 5] = [
    "(defmacro twice (x) `(do ~x ~x ~x))",
    "(defmacro thrice (x) `(do ~x ~x ~x))",
    "(defstruct S2 (z: i64))",
    "(defstruct PS2 :private (z: i64))",
    "(derive Eq S2)",
];

/// Calls and derives that `main` may make, in the spellings that each
/// `ns` form of `main` reaches or does not.
const MAIN_ITEMS: [&str; 14] = [
    "(defun c1 () -> i64 (u/twice 1))",
    "(defun c2 () -> i64 (twice 1))",
    "(defun c3 () -> i64 (u/hid 1))",
    "(defun c4 () -> i64 (thrice 1))",
    "(defun c5 () -> i64 (m/thrice 1))",
    "(defun c6 () -> i64 (nf S))",
    "(defun c7 () -> i64 (u/nf PS))",
    "(derive Eq S)",
    "(derive Eq PS)",
    "(derive Eq S2)",
    "(derive Show UE)",
    "(defun c8 () -> i64 (m/twice 1))",
    "(defstruct S (c: i64))",
    "(defun main () -> i64 (c1))",
];

const NS_LINES: [&str; 6] = [
    "(ns main (:require [util :as u]) (:use more))",
    "(ns main (:require [util :as u] [more :as m]))",
    "(ns main (:use util more))",
    "(ns main (:use more util))",
    "(ns main (:require [more :as m]) (:use util))",
    "(ns main (:require [util :as u] [more :as m]) (:use util more))",
];

/// Some of `items`, each with probability one half, one per line.
fn some(items: &[&str], rng: &mut Rng) -> String {
    let kept: Vec<&str> = items.iter().copied().filter(|_| rng.one_in(2)).collect();
    kept.join("\n")
}

/// The files of one program: `main.fib` first, then the modules, as
/// `(path, text)`.
pub fn module_program(rng: &mut Rng) -> Vec<(String, String)> {
    let more_header = if rng.one_in(2) {
        "(ns more (:use util))"
    } else {
        "(ns more)"
    };
    let mut files = vec![
        (
            "main.fib".to_string(),
            format!("{}\n{}\n", rng.pick(&NS_LINES), some(&MAIN_ITEMS, rng)),
        ),
        (
            "util.fib".to_string(),
            format!("(ns util)\n{}\n", some(&UTIL_ITEMS, rng)),
        ),
        (
            "more.fib".to_string(),
            format!("{more_header}\n{}\n", some(&MORE_ITEMS, rng)),
        ),
    ];
    if rng.one_in(5) {
        break_one(&mut files, rng);
    }
    files
}

/// Breaks one of the files in one of the ways a program may not load.
fn break_one(files: &mut [(String, String)], rng: &mut Rng) {
    let at = rng.between(0, 2);
    let (name, text) = &mut files[at];
    let stem = name.trim_end_matches(".fib").to_string();
    *text = match rng.below(6) {
        0 => format!("(ns {stem} (:use nowhere))\n"),
        1 => format!("(ns {stem} (:use main util more))\n"),
        2 => "(ns wrong)\n".to_string(),
        3 => format!("(ns {stem} (:require [util]))\n"),
        4 => format!("(ns {stem}\n(defun f () -> i64 (\n"),
        _ => format!("(ns {stem} (:bogus x))\n"),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use fibref::syntax::read_all;

    #[test]
    fn a_program_is_a_main_file_and_its_modules_and_is_deterministic() {
        for seed in 0..100 {
            let files = module_program(&mut Rng::new(seed));
            let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
            assert_eq!(names, ["main.fib", "util.fib", "more.fib"]);
            assert_eq!(files, module_program(&mut Rng::new(seed)));
        }
    }

    #[test]
    fn most_programs_read_and_some_are_broken_each_way() {
        let programs: Vec<_> = (0..300).map(|s| module_program(&mut Rng::new(s))).collect();
        let unread = programs
            .iter()
            .filter(|files| files.iter().any(|(_, t)| read_all(t, "m").is_err()))
            .count();
        assert!((1..=60).contains(&unread), "{unread} do not read");
        for marker in [
            "nowhere",
            "(ns wrong)",
            "(:bogus x)",
            "(:require [util])",
            "(:use main util more)",
        ] {
            assert!(
                programs.iter().flatten().any(|(_, t)| t.contains(marker)),
                "no program is broken by {marker}"
            );
        }
    }

    #[test]
    fn the_items_are_forms_that_read() {
        for item in UTIL_ITEMS
            .iter()
            .chain(&MORE_ITEMS)
            .chain(&MAIN_ITEMS)
            .chain(&NS_LINES)
        {
            assert!(read_all(item, "i").is_ok(), "{item}");
        }
    }
}
