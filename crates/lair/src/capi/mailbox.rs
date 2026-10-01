//! The mailbox and the hooks (spec/compiler.md §9): a macro-time module
//! calls back into the compiler for `gensym` and reflection through two
//! function pointers and a context pointer. Fibber cannot give C a
//! pointer to one of its functions, so `lair` supplies the hooks, and
//! the context is a mailbox: a hook call parks the worker thread the
//! macro runs on and wakes the compiler's thread, which reads the
//! request, builds the answer with the module's own constructors and
//! replies. The state machine, and what each out-of-order use does, is
//! in `exchange.rs`; this file is its C surface.

use std::ffi::c_char;
use std::ptr;
use std::sync::Arc;

use super::error::shield;
use super::exchange::Mailbox;

/// What a C caller sees of a mailbox: a pointer from `Arc::into_raw`.
#[repr(C)]
pub struct Call {
    _private: [u8; 0],
}

/// The mailbox of a handle, or `None` for null.
///
/// # Safety
/// `c` is null or a mailbox from [`lair_call_new`], not yet freed (or
/// a worker's own, which holds a count of its own while it runs).
unsafe fn mailbox<'a>(c: *mut Call) -> Option<&'a Mailbox> {
    if c.is_null() {
        None
    } else {
        Some(&*c.cast::<Mailbox>())
    }
}

/// A new, idle mailbox; null if it cannot be made.
#[no_mangle]
pub extern "C" fn lair_call_new() -> *mut Call {
    shield(ptr::null_mut(), || {
        Arc::into_raw(Arc::new(Mailbox::new()))
            .cast_mut()
            .cast::<Call>()
    })
}

/// Free a mailbox; null does nothing. Freed when no call was started
/// or the last one is finished (`lair_call_wait` returned 0), it leaves
/// nothing behind. Freed while a call is running, or parked at a hook
/// nobody answered, it detaches it: the worker thread is not stopped
/// (it cannot be, safely) but parks forever at its next hook call; its
/// thread and a few words stay allocated until the process ends. Not
/// undefined behaviour, but not clean: finish the call first. Above
/// all do not free the session (`lair_jit_free`) while a detached
/// worker might still be running its code.
///
/// # Safety
/// `c` is null or a mailbox not yet freed, and is not used afterwards,
/// except as the context pointer of hook calls the call itself still
/// makes (which is why a call that has not finished may be freed at all).
#[no_mangle]
pub unsafe extern "C" fn lair_call_free(c: *mut Call) {
    shield((), || {
        if !c.is_null() {
            Arc::from_raw(c.cast::<Mailbox>()).release();
        }
    });
}

/// The hook `(cx, a) -> word`, which the compiler installs in the
/// macro module with the mailbox as `cx`: it parks the calling worker
/// and returns what the compiler replies.
#[no_mangle]
pub extern "C" fn lair_hook1_address() -> usize {
    let hook: unsafe extern "C" fn(*mut Call, usize) -> usize = hook1;
    hook as usize
}

/// The hook `(cx, a, b) -> word`; as [`lair_hook1_address`].
#[no_mangle]
pub extern "C" fn lair_hook2_address() -> usize {
    let hook: unsafe extern "C" fn(*mut Call, usize, usize) -> usize = hook2;
    hook as usize
}

/// A null `cx` (no mailbox was installed) gives 0, as does a hook
/// called off its worker thread (see `exchange.rs`).
///
/// # Safety
/// Called by module code with the `cx` installed for it: null or a
/// mailbox whose call is running.
unsafe extern "C" fn hook1(cx: *mut Call, a: usize) -> usize {
    shield(0, || {
        mailbox(cx).map_or(0, |m| m.hook(1, a as i64, 0) as usize)
    })
}

/// # Safety
/// As [`hook1`].
unsafe extern "C" fn hook2(cx: *mut Call, a: usize, b: usize) -> usize {
    shield(0, || {
        mailbox(cx).map_or(0, |m| m.hook(2, a as i64, b as i64) as usize)
    })
}

/// Run the function at `addr` (a `ccc` function of `n <= 8` integer or
/// pointer arguments `args[0..n]`) on a worker thread of this mailbox
/// with a 64 MiB stack, and return at once. The arguments are copied.
/// Collect the call with [`lair_call_wait`]. A mailbox runs one call at
/// a time and can run another once the last has finished (its result
/// is then dropped).
///
/// Refused, as a fault ([`lair_call_fault`]) and not a start: a call
/// still running; `addr` 0; `n > 8`; null `args` with `n > 0`; no
/// worker thread to be had. A null mailbox does nothing.
///
/// # Safety
/// `c` is null or a live mailbox; `addr` is the address of a `ccc`
/// function alive until the call has finished (a function of a JIT
/// session not freed before then); `args` is null or points to `n`
/// readable `int64_t`.
#[no_mangle]
pub unsafe extern "C" fn lair_call_start(c: *mut Call, addr: usize, args: *const i64, n: usize) {
    shield((), || {
        if c.is_null() {
            return;
        }
        // The Arc the handle stands for, borrowed: start clones it for the worker.
        let arc = std::mem::ManuallyDrop::new(Arc::from_raw(c.cast::<Mailbox>()));
        let m: &Mailbox = &arc;
        if n > 0 && args.is_null() {
            return m.misuse("lair_call_start: args is null but n is not 0");
        }
        if n > 8 {
            return m.misuse("lair_call_start: more than 8 arguments");
        }
        let args = if n == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(args, n)
        };
        arc.start(addr, args);
    });
}

/// Block until the call returned (0) or is parked at a hook call (its
/// arity, 1 or 2: read the request with [`lair_call_hook_arg`], answer
/// with [`lair_call_hook_reply`], and wait again). Returns -1 once after
/// one or more misuses of the mailbox ([`lair_call_fault`] has the text): the
/// call that was running is untouched and the next wait goes on with
/// it. Also -1 for a null mailbox and for one on which no call was
/// started. Waiting again on a call that is done returns 0; on one
/// still parked, its arity.
///
/// # Safety
/// `c` is null or a live mailbox.
#[no_mangle]
pub unsafe extern "C" fn lair_call_wait(c: *mut Call) -> i32 {
    shield(-1, || mailbox(c).map_or(-1, Mailbox::wait))
}

/// Argument `i` (0 or 1) of the hook call the worker is parked at. When
/// no hook call is waiting or `i` is not below its arity, returns 0 and
/// records a fault.
///
/// # Safety
/// `c` is null (returns 0) or a live mailbox.
#[no_mangle]
pub unsafe extern "C" fn lair_call_hook_arg(c: *mut Call, i: usize) -> i64 {
    shield(0, || mailbox(c).map_or(0, |m| m.hook_arg(i)))
}

/// Answer the hook call the worker is parked at with `v`, which the
/// hook returns to the module; the worker resumes. With no hook call
/// waiting it does nothing and records a fault.
///
/// # Safety
/// `c` is null (does nothing) or a live mailbox.
#[no_mangle]
pub unsafe extern "C" fn lair_call_hook_reply(c: *mut Call, v: i64) {
    shield((), || {
        if let Some(m) = mailbox(c) {
            m.answer(v);
        }
    });
}

/// The value the finished call returned (as [`lair_call_i64`]
/// reads results). Before the call has finished, returns 0 and records
/// a fault.
///
/// # Safety
/// `c` is null (returns 0) or a live mailbox.
///
/// [`lair_call_i64`]: super::lair_call_i64
#[no_mangle]
pub unsafe extern "C" fn lair_call_result(c: *mut Call) -> i64 {
    shield(0, || mailbox(c).map_or(0, Mailbox::result))
}

/// The text of the first misuse of this mailbox since the last
/// accepted [`lair_call_start`]: NUL terminated, and in `*len` (unless
/// null) its length without the NUL. Null, with `*len` 0, when there
/// is none. The text lives until the next accepted start or the free.
/// A null mailbox has the fault "the mailbox is null".
///
/// A misuse also makes the next [`lair_call_wait`] return -1.
///
/// # Safety
/// `c` is null or a live mailbox; `len` is null or writable.
#[no_mangle]
pub unsafe extern "C" fn lair_call_fault(c: *mut Call, len: *mut usize) -> *const c_char {
    const NULL_MAILBOX: &[u8] = b"the mailbox is null\0";
    let (p, n) = shield((ptr::null(), 0), || {
        mailbox(c).map_or(
            (NULL_MAILBOX.as_ptr(), NULL_MAILBOX.len() - 1),
            Mailbox::fault_text,
        )
    });
    if !len.is_null() {
        *len = n;
    }
    p.cast::<c_char>()
}
