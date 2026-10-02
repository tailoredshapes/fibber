//! Macros through the JIT (spec/compiler.md §6): on every case that
//! defines a macro, the expansion the JIT runner produces must equal
//! the reference interpreter's, form for form and position for position
//! (syntax §1.3), and so must the expansion of the generated macros of
//! `positions`.

#[path = "macros/positions.rs"]
mod positions;

use std::path::Path;

use fibc::macros::JitRunner;
use fibref::cases::list_cases;
use fibref::dump::dump_forms_in;
use fibref::eval::MacroEvaluator;
use fibref::expand::{expand_program, ExpandCtx};
use fibref::syntax::{read_all, Form};
use fibref::types::prelude_forms;

fn expand_with(source: &str, name: &str, jit: bool) -> Result<Vec<Form>, String> {
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx).map_err(|m| m.to_string())?;
    let forms = read_all(source, name).map_err(|e| e.to_string())?;
    if jit {
        let mut runner = JitRunner::new(prelude).map_err(|u| u.0)?;
        expand_program(forms, &mut ctx, &mut runner).map_err(|e| e.to_string())
    } else {
        let mut runner = MacroEvaluator::new(&forms, prelude);
        expand_program(forms, &mut ctx, &mut runner).map_err(|e| e.to_string())
    }
}

#[test]
fn jit_expansions_equal_the_interpreters() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership");
    let cases = list_cases(&dir).expect("cases/ownership readable");
    let (mut compared, mut bad) = (0, Vec::new());
    for path in &cases {
        let source = std::fs::read_to_string(path).expect("readable");
        if !source.contains("defmacro") {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let a = expand_with(&source, name, false);
        let b = expand_with(&source, name, true);
        match (&a, &b) {
            // `Form` equality ignores positions; the dump prints them all.
            (Ok(x), Ok(y)) if x == y && dump_forms_in(x, name) == dump_forms_in(y, name) => {
                compared += 1
            }
            (Err(x), Err(y)) if x == y => compared += 1,
            _ => bad.push(format!(
                "{name}:\n  interpreter: {a:?}\n  jit:         {b:?}"
            )),
        }
    }
    assert!(bad.is_empty(), "expansions differ:\n{}", bad.join("\n"));
    assert!(compared > 0, "no case defines a macro");
    eprintln!("compared {compared} expansions");
}

/// A `trap` in a macro body rejects the program with the interpreter's
/// text, `macro NAME failed: POS: trap: MSG`, and the compiler goes on:
/// it did not abort the process (before, `fib.trap` ended it with exit
/// status 134), and a second rejection in the same process follows.
#[test]
fn a_trap_in_a_macro_is_a_rejection_and_the_process_goes_on() {
    let source =
        "(defmacro boom (x) (match x ((Sym s) (Sym s)) (_ (trap \"boom: not a symbol\"))))\n\
                  (defun main () -> i64 (boom 1))";
    let interpreted = expand_with(source, "t.fib", false);
    let message = interpreted.clone().expect_err("the interpreter rejects it");
    assert!(message.contains("macro boom failed: t.fib:1:"), "{message}");
    assert!(message.contains(": trap: boom: not a symbol"), "{message}");
    for _ in 0..2 {
        assert_eq!(expand_with(source, "t.fib", true), interpreted);
    }
}

/// The expanded modules of the program whose main file is `main` with the
/// interpreter's evaluator or the JIT's runner, as the expansion dump
/// prints them (every position of every node).
fn expand_modules(main: &Path, jit: bool) -> Result<String, String> {
    let source = std::fs::read_to_string(main).map_err(|e| e.to_string())?;
    let file = main.to_string_lossy();
    let loaded = fibref::modules::load(&source, &file)?;
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx)?;
    let all: Vec<Form> = loaded.iter().flat_map(|l| l.forms.clone()).collect();
    let files: Vec<String> = loaded.iter().map(|l| l.file.clone()).collect();
    let modules = if jit {
        let mut runner = JitRunner::new(prelude).map_err(|u| u.0)?;
        fibref::modules::expand_all(loaded, &mut ctx, &mut runner)
    } else {
        let mut runner = MacroEvaluator::new(&all, prelude);
        fibref::modules::expand_all(loaded, &mut ctx, &mut runner)
    }
    .map_err(|e| e.to_string())?;
    let dumps = modules
        .iter()
        .zip(&files)
        .map(|((_, forms), f)| format!("-- {f}\n{}", fibref::dump::dump_forms_in(forms, f)));
    Ok(dumps.collect())
}

/// The first line at which two expansions differ, each side quoted.
fn first_difference(a: &Result<String, String>, b: &Result<String, String>) -> String {
    let (Ok(x), Ok(y)) = (a, b) else {
        return format!("  interpreter: {a:?}\n  jit:         {b:?}");
    };
    let (mut xs, mut ys) = (x.lines(), y.lines());
    let mut n = 0;
    loop {
        n += 1;
        match (xs.next(), ys.next()) {
            (Some(p), Some(q)) if p == q => {}
            (p, q) => return format!("  line {n}\n  interpreter: {p:?}\n  jit:         {q:?}"),
        }
    }
}

/// Syntax §1.3, spec/bootstrap.md §4 and §5.7: a form a macro took from
/// its arguments keeps the position it had (`int 5 i64 12:13 437..438`),
/// every other node of a result has the position of the call. Compared
/// through the expansion dump, which prints every position of every node
/// (`Form` equality ignores positions, so
/// `jit_expansions_equal_the_interpreters` cannot see them).
#[test]
fn jit_and_interpreter_expand_the_module_cases_alike_at_every_position() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/modules");
    let mut mains: Vec<_> = std::fs::read_dir(&dir)
        .expect("cases/modules readable")
        .flatten()
        .map(|e| e.path().join("main.fib"))
        .filter(|p| p.is_file())
        .collect();
    mains.sort();
    let mut bad = Vec::new();
    for main in &mains {
        let (a, b) = (expand_modules(main, false), expand_modules(main, true));
        if a != b {
            bad.push(format!("{}:\n{}", main.display(), first_difference(&a, &b)));
        }
    }
    assert!(bad.is_empty(), "expansions differ:\n{}", bad.join("\n"));
    assert!(mains.len() >= 6, "{} module cases", mains.len());
    eprintln!("compared {} module programs", mains.len());
}
