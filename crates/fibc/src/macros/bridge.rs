//! The hooks a macro-time module calls back into the compiler for
//! `gensym` and the reflection builtins (syntax §3.16, types §2.8):
//! they need the expander's context, which lives in Rust. The module
//! receives the two function pointers and a context pointer through
//! `fibm.set-hooks` and passes the context back on every call, so no
//! global state is involved.

use fibref::expand::{ExpandCtx, ExpandError};
use fibref::syntax::{Form, FormKind, Pos};

use super::module::Fns;

/// What a running macro's hooks see: the expander's context, the
/// module's conversion functions, the call position, and the first
/// reflection error, which ends the expansion.
pub struct Current<'a> {
    pub ctx: &'a ExpandCtx,
    pub fns: &'a Fns,
    pub pos: Pos,
    pub error: Option<ExpandError>,
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

/// `(struct? f)` and the other reflection builtins: the expander's
/// answer, a `Bool` or a `Vec` form.
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
    let name = std::ffi::CStr::from_ptr(name)
        .to_string_lossy()
        .into_owned();
    let f = match cur.fns.to_form(form, &cur.pos) {
        Ok(f) => f,
        Err(_) => Form::new(FormKind::Nil, cur.pos.clone()),
    };
    match cur.ctx.reflect(&name, &f, &cur.pos) {
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
