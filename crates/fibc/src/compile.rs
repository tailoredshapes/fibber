//! Lowering a checked program to one lIR module (spec/compiler.md §2,
//! types §8): the runtime, the type table, the static data, every
//! specialised body reached from `main`, and lIR's `main`.

use std::fmt::Write;

use fibref::own::program::BodyKey;
use fibref::own::Checked;

use crate::lower::{emit_body, emit_closure};
use crate::program::{Program, Work};

/// Why a program cannot be compiled yet (reported as Pending by the
/// harness, never as a pass).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported(pub String);

/// The runtime module `fib.rt` (compiler.md §3), lIR source.
const RUNTIME: &[&str] = &[
    include_str!("../rt/core.lir"),
    include_str!("../rt/str.lir"),
    include_str!("../rt/array.lir"),
    include_str!("../rt/vec.lir"),
    include_str!("../rt/vecbuild.lir"),
    include_str!("../rt/atom.lir"),
    include_str!("../rt/thread.lir"),
    include_str!("../rt/weak.lir"),
];

/// The lIR text of a whole program: runtime, static data, bodies.
pub fn compile(checked: &Checked) -> Result<String, Unsupported> {
    let main = checked
        .typed
        .fun("main")
        .ok_or_else(|| Unsupported("the program has no main".into()))?;
    let mut p = Program::new(checked);
    let defs = crate::defs::emit_defs(&mut p)?;
    p.def_values = defs.values;
    let entry = p.request(BodyKey::Fun(main), Vec::new());
    while let Some(w) = p.next_work() {
        match w {
            Work::Body(inst) => emit_body(&mut p, inst)?,
            Work::Closure { owner, lit, name } => emit_closure(&mut p, owner, lit, &name)?,
        }
    }
    Ok(assemble(&p, &defs.text, &entry))
}

fn assemble(p: &Program<'_>, defs: &str, entry: &str) -> String {
    let mut out = String::new();
    for part in RUNTIME {
        out.push_str(part);
        out.push('\n');
    }
    out.push_str(&p.render_externs());
    out.push_str(&p.objects.render());
    out.push_str(&p.statics.render());
    out.push_str(defs);
    out.push_str(&p.quote_text);
    for f in &p.funcs {
        out.push_str(f);
    }
    // §8.8: main's return joins every thread still running before
    // the result is returned to the OS.
    let _ = write!(
        out,
        "(declare printf i32 (ptr ...))
(define (main i32) ()
  (block entry
    (call @fib.init)
    (let ((r (call @{entry})))
      (call @fib.join-all)
      (call @printf (string \"%lld\\n\") r)
      (ret (i32 0)))))
"
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lir_of(src: &str) -> String {
        let c = fibref::own::check_source(src, "t").expect("checks");
        compile(&c).expect("compiles")
    }

    #[test]
    fn a_scalar_program_checks_as_lir() {
        let text =
            lir_of("(defun sq (x: i64) -> i64 (* x x))\n(defun main () -> i64 (+ (sq 6) 6))");
        if let Err(e) = lir::parse_and_check(&text) {
            panic!("{}\n{text}", e[0]);
        }
        assert!(text.contains("(define internal tailcc (f.sq i64) ((i64 p0))"));
        assert!(text.contains("smul-overflow"));
    }

    #[test]
    fn a_struct_program_checks_as_lir() {
        let text = lir_of(
            "(defstruct P (x: i64 s: str))\n(defun main () -> i64 (let ((p (P 4 \"ab\"))) (+ (. p x) (str-len (. p s)))))",
        );
        if let Err(e) = lir::parse_and_check(&text) {
            panic!("{}\n{text}", e[0]);
        }
        assert!(text.contains("(defstruct o.P (i64 i32 i32 i64 ptr))"));
    }
}
