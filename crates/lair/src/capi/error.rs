//! `lair_error`, and what every entry point shares: catching a panic,
//! reading a string argument, checking an out-parameter.

use std::ffi::{c_char, c_int};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

/// What a C caller sees of an error: nothing but a pointer. It is a
/// [`Message`] underneath.
#[repr(C)]
pub struct LairError {
    _private: [u8; 0],
}

/// The text of an error, with a NUL after it so that C may also treat
/// it as a string; its length excludes the NUL.
pub(super) struct Message(Box<[u8]>);

impl Message {
    pub(super) fn new(text: &str) -> Message {
        let mut bytes = Vec::with_capacity(text.len() + 1);
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(0);
        Message(bytes.into_boxed_slice())
    }

    /// The text, then the NUL.
    pub(super) fn with_nul(&self) -> &[u8] {
        &self.0
    }
}

/// An error object for C holding `text`.
pub(super) fn raise(text: &str) -> *mut LairError {
    Box::into_raw(Box::new(Message::new(text))).cast::<LairError>()
}

/// The source text on which `lair_check_source` panics, and the address at
/// which `lair_call_i64` and `lair_call_f64` panic, in a build with the
/// feature `test-panic` (tests/capi_panic.rs): a panic at a real exported
/// entry, which the entry must catch.
#[cfg(feature = "test-panic")]
pub const TEST_PANIC_SOURCE: &str = "(test-panic)";

/// See [`TEST_PANIC_SOURCE`].
#[cfg(feature = "test-panic")]
pub const TEST_PANIC_ADDRESS: usize = 1;

/// What `lair_error_text` returns for a null error.
const NO_ERROR: &[u8] = b"\0";

/// The message of an error, and in `*len` (unless null) its length
/// in bytes; the text is also NUL terminated.
///
/// A null `e` (success) gives an empty string. The pointer is valid
/// until `e` is freed; the text is not to be modified.
///
/// # Safety
/// `e` is null or an error this library returned and has not freed;
/// `len` is null or writable.
#[no_mangle]
pub unsafe extern "C" fn lair_error_text(e: *const LairError, len: *mut usize) -> *const c_char {
    let bytes = if e.is_null() {
        NO_ERROR
    } else {
        (*e.cast::<Message>()).with_nul()
    };
    if !len.is_null() {
        *len = bytes.len() - 1;
    }
    bytes.as_ptr().cast::<c_char>()
}

/// Free an error. A null `e` does nothing.
///
/// # Safety
/// `e` is null or an error this library returned, not yet freed; it is
/// not used afterwards.
#[no_mangle]
pub unsafe extern "C" fn lair_error_free(e: *mut LairError) {
    shield((), || {
        if !e.is_null() {
            drop(Box::from_raw(e.cast::<Message>()));
        }
    });
}

/// Run `f` for a function that has no way to report: a panic gives
/// `fallback`.
pub(super) fn shield<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// Run `f`; its `Err`, or a panic, becomes an error object, and
/// success becomes null.
pub(super) fn guard(f: impl FnOnce() -> Result<(), String>) -> *mut LairError {
    match catch(f) {
        Ok(()) => ptr::null_mut(),
        Err(m) => raise(&m),
    }
}

/// `f`'s result, or the text of the panic that ended it.
pub(super) fn catch<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|p| {
        let what = p
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic".to_string());
        Err(format!("internal error: {what}"))
    })
}

/// A string argument: `len` bytes at `p`, UTF-8. `what` names the
/// argument in the message. Length 0 is the empty string, whatever `p`.
///
/// # Safety
/// `p` points to `len` readable bytes when `len > 0` and `p` is not null.
pub(super) unsafe fn text<'a>(what: &str, p: *const c_char, len: usize) -> Result<&'a str, String> {
    if len == 0 {
        return Ok("");
    }
    if p.is_null() {
        return Err(format!("{what} is null but its length is {len}"));
    }
    if len > isize::MAX as usize {
        return Err(format!("{what} has an impossible length {len}"));
    }
    let bytes = std::slice::from_raw_parts(p.cast::<u8>(), len);
    std::str::from_utf8(bytes).map_err(|e| {
        format!(
            "{what} is not valid UTF-8 (valid up to byte {})",
            e.valid_up_to()
        )
    })
}

/// An out-parameter that must not be null.
pub(super) fn out_param<T>(what: &str, out: *mut T) -> Result<*mut T, String> {
    if out.is_null() {
        Err(format!("{what} is null"))
    } else {
        Ok(out)
    }
}

/// An optimisation level argument: 0 to 3.
pub(super) fn level_arg(level: c_int) -> Result<u8, String> {
    u8::try_from(level)
        .ok()
        .filter(|l| *l <= 3)
        .ok_or_else(|| format!("opt_level is {level}, not 0 to 3"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_owns_its_text_and_reports_its_length() {
        let e = raise("bad thing: 1:2");
        let mut len = 0usize;
        // SAFETY: e is a fresh error; len is writable.
        unsafe {
            let p = lair_error_text(e, &mut len);
            assert_eq!(len, 14);
            let s = std::slice::from_raw_parts(p.cast::<u8>(), len + 1);
            assert_eq!(s, b"bad thing: 1:2\0");
            lair_error_free(e);
        }
    }

    #[test]
    fn a_null_error_is_the_empty_text_and_freeing_it_is_a_no_op() {
        let mut len = 99usize;
        // SAFETY: null is allowed for both.
        unsafe {
            let p = lair_error_text(ptr::null(), &mut len);
            assert_eq!((len, *p), (0, 0));
            assert!(!lair_error_text(ptr::null(), ptr::null_mut()).is_null());
            lair_error_free(ptr::null_mut());
        }
    }

    #[test]
    fn a_panic_becomes_an_error_text() {
        let m = catch::<()>(|| panic!("boom {}", 7)).unwrap_err();
        assert_eq!(m, "internal error: boom 7");
        let m = catch::<()>(|| std::panic::panic_any(5u8)).unwrap_err();
        assert_eq!(m, "internal error: a panic");
        assert_eq!(shield(-1, || -> i32 { panic!("x") }), -1);
    }

    #[test]
    fn a_panic_in_an_entry_is_an_error_object_and_success_is_null() {
        assert!(guard(|| Ok(())).is_null());
        let e = guard(|| panic!("inside lair"));
        let mut len = 0usize;
        // SAFETY: e is a fresh error; len is writable.
        unsafe {
            let p = lair_error_text(e, &mut len);
            let text = std::slice::from_raw_parts(p.cast::<u8>(), len);
            assert_eq!(text, b"internal error: inside lair");
            lair_error_free(e);
        }
        let e = guard(|| Err("plain failure".into()));
        // SAFETY: as above.
        unsafe {
            let mut len = 0usize;
            let p = lair_error_text(e, &mut len);
            assert_eq!(
                std::slice::from_raw_parts(p.cast::<u8>(), len),
                b"plain failure"
            );
            lair_error_free(e);
        }
    }

    #[test]
    fn string_arguments_are_checked() {
        let s = "héllo";
        // SAFETY: pointers and lengths come from real slices or are null with 0.
        unsafe {
            assert_eq!(text("a", s.as_ptr().cast(), s.len()).unwrap(), "héllo");
            assert_eq!(text("a", ptr::null(), 0).unwrap(), "");
            assert_eq!(
                text("name", ptr::null(), 3).unwrap_err(),
                "name is null but its length is 3"
            );
            let bad = [b'a', 0xff, b'b'];
            assert_eq!(
                text("src", bad.as_ptr().cast(), 3).unwrap_err(),
                "src is not valid UTF-8 (valid up to byte 1)"
            );
        }
        assert!(level_arg(3).is_ok() && level_arg(0).is_ok());
        assert_eq!(level_arg(4).unwrap_err(), "opt_level is 4, not 0 to 3");
        assert!(level_arg(-1).is_err());
    }
}
