//! Parse and check, no code (`lair check`).

use std::ffi::c_char;

use super::error::{guard, text, LairError};

/// Parse and check the lIR module `src` as a whole: null when it is
/// valid, else an error holding the diagnostics `lair check` would
/// print, one per line, without a file name in front.
///
/// # Safety
/// `src` points to `src_len` readable bytes (or `src_len` is 0).
#[no_mangle]
pub unsafe extern "C" fn lair_check_source(src: *const c_char, src_len: usize) -> *mut LairError {
    guard(|| {
        let src = text("src", src, src_len)?;
        #[cfg(feature = "test-panic")]
        if src == super::error::TEST_PANIC_SOURCE {
            panic!("injected by the test-panic feature");
        }
        crate::check_source(src)
            .map(drop)
            .map_err(|e| e.to_string())
    })
}
