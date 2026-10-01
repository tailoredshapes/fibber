//! The arity diagnostic of stdlib §7 D1 for the library's own modules (a
//! module named `fib.*`): `sort` and `reduce`, which no prelude function
//! of the checker's unit tests is, and a program's own module that defines
//! the same names, which gets no hint. The whole front end runs over a few
//! files in a scratch directory; only the type check is asked.

use std::path::PathBuf;

use fibref::expand::ExpandCtx;
use fibref::modules::{expand_all, try_load_with};
use fibref::roots::Roots;
use fibref::syntax::Form;
use fibref::types::{check_modules, prelude_forms};

/// A scratch directory holding `files` (path, text), removed on drop.
struct Tree(PathBuf);

impl Tree {
    fn new(label: &str, files: &[(&str, &str)]) -> Tree {
        let dir = std::env::temp_dir().join(format!("fibber-diag-{}-{label}", std::process::id()));
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

/// The library module `fib.d`, with the functions of the table of D1 as the
/// real library has them: `sort` of one argument, `reduce` of three.
const FIB_D: &str = "(ns fib.d)
(defun sort (xs: (Vec i64)) -> (Vec i64) xs)
(defun reduce (f: (fn (i64 i64) i64) init: i64 xs: (Vec i64)) -> i64 init)
";

/// The same names in a module of the program's own.
const MINE: &str = "(ns mine)
(defun sort (xs: (Vec i64)) -> (Vec i64) xs)
(defun reduce (f: (fn (i64 i64) i64) init: i64 xs: (Vec i64)) -> i64 init)
";

/// The first type error of `main.fib` of `files`, in words.
fn first_error(label: &str, files: &[(&str, &str)]) -> String {
    let tree = Tree::new(label, files);
    let file = tree.main();
    let source = std::fs::read_to_string(&file).expect("main.fib");
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(fibref::eval::STACK_BYTES)
            .spawn_scoped(scope, || {
                let mut ctx = ExpandCtx::new();
                let prelude = prelude_forms(&mut ctx).expect("the prelude expands");
                let loaded = try_load_with(&source, &file, &Roots::default(), &[])
                    .map_err(|e| e.to_string())
                    .expect("the program loads");
                let all: Vec<Form> = loaded.iter().flat_map(|l| l.forms.clone()).collect();
                let mut runner = fibref::eval::MacroEvaluator::new(&all, prelude.clone());
                let modules = expand_all(loaded, &mut ctx, &mut runner).expect("it expands");
                match check_modules(&modules, &prelude) {
                    Ok(_) => "accepted".to_string(),
                    Err(es) => es[0].message.clone(),
                }
            })
            .expect("a thread")
            .join()
            .expect("the checker does not panic")
    })
}

#[test]
fn sort_with_a_comparator_says_sort_with() {
    let main = "(ns main (:use fib.d))\n(defun main () -> i64 (do (sort > [1 2]) 0))";
    let files = [("main.fib", main), ("fib/d.fib", FIB_D)];
    assert_eq!(
        first_error("sort", &files),
        "sort takes 1 argument(s), got 2; use sort-with"
    );
}

#[test]
fn reduce_with_two_arguments_says_reduce1_where_the_macro_does_not_serve_the_call() {
    // `(reduce f c)` is the macro's; the qualified head is the function's.
    let main = "(ns main (:use fib.d))\n(defun main () -> i64 (fib.d/reduce + [1 2]))";
    let files = [("main.fib", main), ("fib/d.fib", FIB_D)];
    assert_eq!(
        first_error("reduce", &files),
        "reduce takes 3 argument(s), got 2; use reduce1"
    );
}

#[test]
fn other_counts_and_a_programs_own_module_have_no_hint() {
    let four = "(ns main (:use fib.d))\n(defun main () -> i64 (fib.d/reduce + 0 [1 2] 4))";
    let files = [("main.fib", four), ("fib/d.fib", FIB_D)];
    assert_eq!(
        first_error("four", &files),
        "reduce takes 3 argument(s), got 4"
    );
    let own = "(ns main (:use mine))\n(defun main () -> i64 (do (sort > [1 2]) 0))";
    let files = [("main.fib", own), ("mine.fib", MINE)];
    assert_eq!(
        first_error("own-sort", &files),
        "sort takes 1 argument(s), got 2"
    );
    let own = "(ns main (:use mine))\n(defun main () -> i64 (mine/reduce + [1 2]))";
    let files = [("main.fib", own), ("mine.fib", MINE)];
    assert_eq!(
        first_error("own-reduce", &files),
        "reduce takes 3 argument(s), got 2"
    );
}
