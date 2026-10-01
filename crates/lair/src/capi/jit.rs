//! A JIT session as a C handle (`Jit`, spec/lir.md §11).

use std::ffi::{c_char, c_int};
use std::ptr;

use super::error::{guard, level_arg, out_param, shield, text, LairError};
use crate::{Jit, JitOptions};

/// What a C caller sees of a [`Jit`]: a pointer and nothing else.
#[repr(C)]
pub struct LairJit {
    _private: [u8; 0],
}

/// The session a handle stands for.
///
/// # Safety
/// `p` is null or a live handle from [`lair_jit_new`], used by this
/// thread alone for as long as the reference lives.
unsafe fn session<'a>(p: *mut LairJit) -> Result<&'a mut Jit, String> {
    if p.is_null() {
        Err("jit is null".to_string())
    } else {
        Ok(&mut *p.cast::<Jit>())
    }
}

/// A new session: modules added to it see each other's functions.
/// `opt_level` is 0 (no IR optimisation) to 3. On success `*out` is the
/// session, which [`lair_jit_free`] ends; on failure `*out` is null
/// (when `out` is not).
///
/// # Safety
/// `out` is null or writable.
#[no_mangle]
pub unsafe extern "C" fn lair_jit_new(opt_level: c_int, out: *mut *mut LairJit) -> *mut LairError {
    if !out.is_null() {
        *out = ptr::null_mut();
    }
    guard(|| {
        let out = out_param("out", out)?;
        let level = level_arg(opt_level)?;
        let jit = Jit::new(JitOptions { opt_level: level }).map_err(|e| e.to_string())?;
        *out = Box::into_raw(Box::new(jit)).cast::<LairJit>();
        Ok(())
    })
}

/// End a session. Every address it gave dies with it: calling one
/// afterwards, or while this call runs, is undefined behaviour. A null
/// `jit` does nothing.
///
/// # Safety
/// `jit` is null or a session not yet freed, with no call into its
/// code in progress, and is not used afterwards.
#[no_mangle]
pub unsafe extern "C" fn lair_jit_free(jit: *mut LairJit) {
    shield((), || {
        if !jit.is_null() {
            drop(Box::from_raw(jit.cast::<Jit>()));
        }
    });
}

/// Parse, check, lower, verify and add the module `src` to the
/// session, under the module name `name` (it is what a later duplicate
/// definition's message names). On failure nothing of the module was
/// added and the error holds the text `Jit::add_source` reports; the
/// session stays usable.
///
/// # Safety
/// `jit` is null or a live session; `name` and `src` point to as many
/// readable bytes as their lengths say.
#[no_mangle]
pub unsafe extern "C" fn lair_jit_add_source(
    jit: *mut LairJit,
    name: *const c_char,
    name_len: usize,
    src: *const c_char,
    src_len: usize,
) -> *mut LairError {
    guard(|| {
        let jit = session(jit)?;
        let name = text("name", name, name_len)?;
        let src = text("src", src, src_len)?;
        jit.add_source(name, src).map_err(|e| e.to_string())
    })
}

/// The address of the function `name`, an exported function of a module
/// of this session, compiled if need be. A function that is not `ccc`
/// has the address of its own convention: use [`lair_jit_c_entry`] to
/// call it from C.
///
/// # Safety
/// As [`lair_jit_add_source`]; `out` is null or writable.
#[no_mangle]
pub unsafe extern "C" fn lair_jit_address(
    jit: *mut LairJit,
    name: *const c_char,
    name_len: usize,
    out: *mut usize,
) -> *mut LairError {
    guard(|| {
        let jit = session(jit)?;
        let name = text("name", name, name_len)?;
        let out = out_param("out", out)?;
        *out = jit.address(name).map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// A `ccc` entry to the function `name`: the function itself when it is
/// `ccc`, otherwise a trampoline the session generates once. The
/// address is valid until the session is freed.
///
/// # Safety
/// As [`lair_jit_address`].
#[no_mangle]
pub unsafe extern "C" fn lair_jit_c_entry(
    jit: *mut LairJit,
    name: *const c_char,
    name_len: usize,
    out: *mut usize,
) -> *mut LairError {
    guard(|| {
        let jit = session(jit)?;
        let name = text("name", name, name_len)?;
        let out = out_param("out", out)?;
        *out = jit.c_entry(name).map_err(|e| e.to_string())?;
        Ok(())
    })
}
