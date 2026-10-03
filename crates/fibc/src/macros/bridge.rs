//! The hooks a macro-time module calls back into the compiler for
//! `gensym` and the reflection builtins (syntax §3.16, types §2.8):
//! they need the expander's context, which lives in Rust. The module
//! receives the two function pointers and a context pointer through
//! `fibm.set-hooks` and passes the context back on every call, so no
//! global state is involved.

use std::sync::mpsc::Sender;

use fibref::expand::{ExpandCtx, ExpandError, ExpandErrorKind};
use fibref::syntax::{Form, FormKind, Pos};

use super::module::{Fns, Inputs};

/// How a macro call on its own thread ends: the address of the result
/// object, or the expansion's failure by `trap`.
pub enum Outcome {
    Done(usize),
    Failed(ExpandError),
}

/// What a running macro's hooks see: the expander's context, the
/// module's conversion functions, the call position, the forms the macro
/// was given, the first reflection error, which ends the expansion, and
/// where a `trap` reports to (the macro's name, the thread's channel).
pub struct Current<'a> {
    pub ctx: &'a ExpandCtx,
    pub fns: &'a Fns,
    pub pos: Pos,
    pub inputs: &'a Inputs<'a>,
    pub error: Option<ExpandError>,
    pub name: &'a str,
    pub done: Sender<Outcome>,
}

/// `(trap msg)` in a macro body (syntax §3.16): the macro fails and
/// the expansion with it, as the interpreter's `MacroFailed` does, with
/// the text `POS: trap: MSG` of its run error. The call cannot return
/// into the macro, and a frame of compiled code cannot be unwound, so
/// the thread that runs the macro (`JitRunner::run`) reports the failure
/// to the thread that waits for it and stays here for good: the abort of
/// the program's own `trap` would end the whole compiler.
///
/// The message is that of `trap` and of the faults of the macro's own
/// code, which know their position; a fault of the runtime (an index
/// out of range inside `array-get`) does not, and `pos` is null: the
/// text is then `trap: MSG`, without the position the interpreter adds.
///
/// # Safety
/// As [`gensym_hook`]; `pos` is null or a NUL-terminated string constant
/// of the module, and `msg` points at `n` bytes.
pub unsafe extern "C" fn trap_hook(
    cur: *mut Current<'_>,
    pos: *const std::os::raw::c_char,
    msg: *const u8,
    n: i64,
) -> ! {
    let cur = &mut *cur;
    let text = String::from_utf8_lossy(std::slice::from_raw_parts(msg, n as usize));
    let message = if pos.is_null() {
        format!("trap: {text}")
    } else {
        let pos = std::ffi::CStr::from_ptr(pos).to_string_lossy();
        format!("{pos}: trap: {text}")
    };
    let kind = ExpandErrorKind::MacroFailed {
        name: cur.name.to_string(),
        message,
    };
    // The waiting side is gone only if its call was abandoned.
    let _ = cur
        .done
        .send(Outcome::Failed(ExpandError::new(kind, &cur.pos)));
    loop {
        std::thread::park();
    }
}

/// `(gensym prefix)`: the expander's fresh symbol.
///
/// # Safety
/// Called only by the module, with the `cur` that `fibm.set-hooks`
/// installed for the duration of one macro call (`JitRunner::run`)
/// and a `str` object of that module as `prefix`.
pub unsafe extern "C" fn gensym_hook(cur: *mut Current<'_>, prefix: *const u8) -> *const u8 {
    let cur = &mut *cur;
    let text = cur.fns.read_str(prefix);
    let sym = cur.ctx.gensym(&text, &cur.pos);
    cur.fns.to_object(&sym)
}

/// The operation and the position of a reflection call: the name the
/// module passes is `OP FILE:LINE:COL:START:END`, or `OP` alone.
pub fn split_call(text: &str) -> (&str, Option<Pos>) {
    let Some((op, at)) = text.split_once(' ') else {
        return (text, None);
    };
    let mut it = at.rsplitn(5, ':');
    let (end, start, col, line, file) = (it.next(), it.next(), it.next(), it.next(), it.next());
    let num = |s: Option<&str>| s.and_then(|s| s.parse::<usize>().ok());
    match (num(end), num(start), num(col), num(line), file) {
        (Some(end), Some(start), Some(col), Some(line), Some(file)) => (
            op,
            Some(Pos {
                file: file.into(),
                line,
                col,
                start,
                end,
            }),
        ),
        _ => (op, None),
    }
}

/// `(struct? f)` and the other reflection builtins: the expander's
/// answer, a `Bool` or a `Vec` form. The argument is read back as a
/// result is: an input form keeps its own position (the interpreter's
/// `value_form`).
///
/// # Safety
/// As [`gensym_hook`]; `name` is a NUL-terminated string constant of
/// the module and `form` one of its `Form` objects.
pub unsafe extern "C" fn reflect_hook(
    cur: *mut Current<'_>,
    name: *const std::os::raw::c_char,
    form: *const u8,
) -> *const u8 {
    let cur = &mut *cur;
    let call = std::ffi::CStr::from_ptr(name)
        .to_string_lossy()
        .into_owned();
    let (name, at) = split_call(&call);
    let at = at.unwrap_or_else(|| cur.pos.clone());
    let f = match cur.fns.to_form(form, &cur.pos, cur.inputs) {
        Ok(f) => f,
        Err(_) => Form::new(FormKind::Nil, cur.pos.clone()),
    };
    match cur.ctx.reflect(name, &f, &at) {
        Ok(out) => cur.fns.to_object(&out),
        Err(e) => {
            if cur.error.is_none() {
                cur.error = Some(e);
            }
            cur.fns
                .to_object(&Form::new(FormKind::Nil, cur.pos.clone()))
        }
    }
}
