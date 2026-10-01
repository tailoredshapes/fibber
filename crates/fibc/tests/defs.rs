//! `def` initialisers through the JIT (types §8.10, compiler.md §8
//! item 4): on every case that has a `def`, the constants the JIT
//! backend emits must equal the interpreter backend's, text for text,
//! and every `def`'s value text too.

use std::path::Path;

use fibc::defs::{interp, jit};
use fibc::front::{check, Front};
use fibc::program::Program;
use fibref::cases::list_cases;

/// The constants' text with each header's type id replaced by the
/// object type's symbol (`sname`): the two backends register object
/// types in a different order, so the ids differ where the types do
/// not. The symbol, not the display name: every `(Array T)` of a
/// pointer is one object type, `fib.array.ptr`, whose display name is the
/// first array registered, which is not the same array in both backends.
fn by_type_name(text: &str, p: &Program<'_>) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let mut line = line.to_string();
        if let Some(i) = line.find("(i64 0) (i32 ") {
            let start = i + "(i64 0) (i32 ".len();
            let end = start + line[start..].find(')').unwrap_or(0);
            if let Ok(tid) = line[start..end].parse::<u32>() {
                let name = p.objects.get(tid).sname.clone();
                line.replace_range(start..end, &name);
            }
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

#[test]
fn jit_def_constants_equal_the_interpreters() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/ownership");
    let cases = list_cases(&dir).expect("cases/ownership readable");
    let (mut compared, mut bad) = (0, Vec::new());
    for path in &cases {
        let source = std::fs::read_to_string(path).expect("readable");
        if !source.contains("(def ") {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let Front::Checked(checked) = check(&source, name) else {
            continue;
        };
        let mut pa = Program::new(&checked);
        let a = interp::emit_defs(&mut pa).map(|d| (by_type_name(&d.text, &pa), d.values));
        let mut pb = Program::new(&checked);
        let b = jit::emit_defs(&mut pb).map(|d| (by_type_name(&d.text, &pb), d.values));
        match (&a, &b) {
            (Ok(x), Ok(y)) if x == y => compared += 1,
            (Err(x), Err(y)) if x == y => compared += 1,
            _ => bad.push(format!(
                "{name}:\n  interpreter: {a:?}\n  jit:         {b:?}"
            )),
        }
    }
    assert!(bad.is_empty(), "def constants differ:\n{}", bad.join("\n"));
    assert!(compared > 0, "no case has a def");
    eprintln!("compared {compared} programs' defs");
}
