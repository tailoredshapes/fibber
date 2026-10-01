//! Lowering a checked program to one lIR module (spec/compiler.md §2,
//! types §8): the runtime, the type table, the static data, every
//! specialised body reached from `main`, and lIR's `main`.

use std::fmt::Write;

use fibref::own::program::BodyKey;
use fibref::own::Checked;

use crate::lower::{emit_body, emit_closure};
use crate::program::{Program, Work};
use crate::value::LirTy;

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
    include_str!("../rt/task.lir"),
    include_str!("../rt/weak.lir"),
    include_str!("../rt/io.lir"),
];

/// The runtime's own `declare` of the C function `name`, if any:
/// a program's `extern` of the same name must agree with it and is
/// then not declared again.
pub(crate) fn runtime_declaration(name: &str) -> Option<String> {
    let head = format!("(declare {name} ");
    RUNTIME
        .iter()
        .flat_map(|part| part.lines())
        .find(|l| l.starts_with(&head))
        .map(str::to_string)
}

/// The lIR text of a whole program for `fibc run`: runtime, static
/// data, bodies, and a `main` that prints the result.
pub fn compile(checked: &Checked) -> Result<String, Unsupported> {
    compile_kind(checked, false)
}

/// The same for an executable, whose `main` returns the result as
/// the process status and prints nothing (compiler.md §1).
pub fn compile_executable(checked: &Checked) -> Result<String, Unsupported> {
    compile_kind(checked, true)
}

fn compile_kind(checked: &Checked, executable: bool) -> Result<String, Unsupported> {
    let main = checked
        .typed
        .fun("main")
        .ok_or_else(|| Unsupported("the program has no main".into()))?;
    let mut p = Program::new(checked);
    let defs = crate::defs::emit_defs(&mut p)?;
    p.def_values = defs.values;
    let entry = p.request(BodyKey::Fun(main), Vec::new());
    emit_all(&mut p)?;
    Ok(assemble(&mut p, &defs.text, &entry, executable))
}

/// Emits every body and closure queued so far.
pub(crate) fn emit_all(p: &mut Program<'_>) -> Result<(), Unsupported> {
    while let Some(w) = p.next_work() {
        match w {
            Work::Body(inst) => emit_body(p, inst)?,
            Work::Closure { owner, lit, name } => emit_closure(p, owner, lit, &name)?,
        }
    }
    Ok(())
}

/// A compiled macro-time module (compiler.md §6, `macros/`).
pub struct MacroModule {
    /// The lIR text, with the surface of `macros/abi.rs` exported,
    /// the table of the keywords the module interned among it.
    pub text: String,
}

/// The macro-time module of `checked`, whose `defmacro` is `name`,
/// taking `n` `Form` (or rest vector) arguments.
pub fn compile_macro(
    checked: &Checked,
    name: &str,
    n: usize,
    k: usize,
) -> Result<MacroModule, Unsupported> {
    use crate::macros::abi::{render, FormLayout, VARIANTS, WIDTHS};
    use fibref::types::decls::Shape;
    use fibref::types::ty::Ty;
    let g = &checked.typed.globals;
    let f = g
        .funs
        .iter()
        .position(|d| d.is_macro && d.name == name)
        .ok_or_else(|| Unsupported(format!("no macro {name}")))?;
    let mut p = Program::new(checked);
    for w in WIDTHS {
        p.statics.keyword(w);
    }
    let form_id = g
        .form
        .ok_or_else(|| Unsupported("the prelude defines no Form".into()))?;
    let form_ty = Ty::nominal(form_id, Vec::new());
    let (tid, sname) = p.object(&form_ty)?;
    let size = p.objects.get(tid).size();
    let Shape::Enum(vs) = &g.ty(form_id).shape else {
        return Err(Unsupported("Form is not an enum".into()));
    };
    let mut tags = [0u32; 11];
    for (i, v) in VARIANTS.iter().enumerate() {
        let tag = vs
            .iter()
            .position(|x| x.name == *v)
            .ok_or_else(|| Unsupported(format!("Form has no variant {v}")))?;
        if tag != i {
            return Err(Unsupported(
                "Form's variants are not in the expected order".into(),
            ));
        }
        tags[i] = tag as u32;
    }
    let vec_id = g
        .vec
        .ok_or_else(|| Unsupported("the prelude defines no Vec".into()))?;
    let vec_ty = Ty::nominal(vec_id, vec![form_ty.clone()]);
    let vec = crate::lower::vec_ids_of(&mut p, &vec_ty, &form_ty)?;
    let defs = crate::defs::emit_defs(&mut p)?;
    p.def_values = defs.values;
    let body = p.request(
        BodyKey::Fun(fibref::types::ast::FunId(f as u32)),
        Vec::new(),
    );
    emit_all(&mut p)?;
    // No keyword is interned after the bodies are emitted: the table
    // the module exports is complete here.
    let keyword_strs = p
        .statics
        .keywords()
        .iter()
        .map(|k| p.statics.string(k, 0))
        .collect();
    let layout = FormLayout {
        sname,
        tid,
        tags,
        size,
        vec,
        keyword_strs,
    };
    let mut text = assemble_parts(&mut p, &defs.text);
    text.push_str(&render(&layout, &body, n, k));
    Ok(MacroModule { text })
}

/// The runtime, the tables, the static data and every function: what
/// a program and a macro module share.
pub(crate) fn assemble_parts(p: &mut Program<'_>, defs: &str) -> String {
    let mut out = String::new();
    for part in RUNTIME {
        out.push_str(part);
        out.push('\n');
    }
    let keyword_helpers = p.render_keyword_helpers();
    out.push_str(&p.render_externs());
    out.push_str(&p.objects.render());
    out.push_str(&p.statics.render());
    out.push_str(&keyword_helpers);
    out.push_str(defs);
    out.push_str(&p.quote_text);
    for f in &p.funcs {
        out.push_str(f);
    }
    out
}

fn assemble(p: &mut Program<'_>, defs: &str, entry: &str, executable: bool) -> String {
    let mut out = assemble_parts(p, defs);
    let _ = LirTy::I64;
    // §8.8: main's return joins every thread still running before
    // the result is printed (fibc run) or returned (an executable).
    let end = if executable {
        "(ret (trunc i32 r))"
    } else {
        "(call @printf (string \"%lld\\n\") r)\n      (ret (i32 0))"
    };
    let _ = write!(
        out,
        "(declare printf i32 (ptr ...))
(define (main i32) ((i32 argc) (ptr argv))
  (block entry
    (call @fib.init)
    (call @fib.set-args argc argv)
    (let ((r (call @{entry})))
      (call @fib.join-all)
      (call @fib.pool-quiesce)
      {end})))
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
    fn the_float_bit_casts_lower_to_one_bitcast_each() {
        // syntax §4.3: no check, no call; the lIR `bitcast` between a
        // float and the integer of its width keeps every bit.
        let text = lir_of(
            "(defun a (x: f64) -> i64 (f64->bits x))\n\
             (defun b (n: i64) -> f64 (bits->f64 n))\n\
             (defun c (x: f32) -> i32 (f32->bits x))\n\
             (defun d (n: i32) -> f32 (bits->f32 n))\n\
             (defun main () -> i64 (+ (a (b 1)) (sext i64 (c (d 2i32)))))",
        );
        if let Err(e) = lir::parse_and_check(&text) {
            panic!("{}\n{text}", e[0]);
        }
        for cast in [
            "(bitcast i64 p0)",
            "(bitcast double p0)",
            "(bitcast i32 p0)",
            "(bitcast float p0)",
        ] {
            assert_eq!(text.matches(cast).count(), 1, "{cast}\n{text}");
        }
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
