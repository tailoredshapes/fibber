//! Macros through the JIT (spec/compiler.md §6): on every case that
//! defines a macro, the expansion the JIT runner produces must equal
//! the reference interpreter's, form for form.

use std::path::Path;

use fibc::macros::JitRunner;
use fibref::cases::list_cases;
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
            (Ok(x), Ok(y)) if x == y => compared += 1,
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
