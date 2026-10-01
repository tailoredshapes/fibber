//! An executable from a module, ahead of time (`aot::build_executable`).

use std::ffi::{c_char, c_int};
use std::path::Path;

use super::error::{guard, level_arg, text, LairError};
use crate::aot::{self, Options};

/// The `n` library names `libs[i]` of byte length `lens[i]`.
///
/// # Safety
/// For `n > 0`, `libs` and `lens` point to `n` readable elements and
/// each `libs[i]` to `lens[i]` readable bytes.
unsafe fn names(
    libs: *const *const c_char,
    lens: *const usize,
    n: usize,
) -> Result<Vec<String>, String> {
    if n == 0 {
        return Ok(Vec::new());
    }
    if libs.is_null() || lens.is_null() {
        return Err(format!("libs and lib_lens must not be null for n_libs {n}"));
    }
    (0..n)
        .map(|i| text("a library name", *libs.add(i), *lens.add(i)).map(str::to_string))
        .collect()
}

/// Compile the module `src`, which must satisfy the `main` rule of
/// spec/lir.md §7.2, and link it with `cc` into the executable `path`.
/// `opt_level` is 0 to 3; each of the `n_libs` names (`libs[i]`, of
/// byte length `lib_lens[i]`) is linked as `-lNAME`, after libm and
/// libpthread, which are always linked, and is searched for where the
/// system's `cc` searches: no library directory (`-L`, rpath) is given
/// through this function. The executable's exit status is `main`'s
/// result. On failure the error holds the diagnostics, or the
/// linker's output.
///
/// # Safety
/// `src` and `path` point to as many readable bytes as their lengths
/// say; for `n_libs > 0`, `libs` and `lib_lens` point to `n_libs`
/// readable elements, each name to its length in readable bytes.
#[no_mangle]
#[allow(clippy::too_many_arguments)] // the shape §9 fixes
pub unsafe extern "C" fn lair_build_executable(
    src: *const c_char,
    src_len: usize,
    path: *const c_char,
    path_len: usize,
    opt_level: c_int,
    libs: *const *const c_char,
    lib_lens: *const usize,
    n_libs: usize,
) -> *mut LairError {
    guard(|| {
        let src = text("src", src, src_len)?;
        let path = text("path", path, path_len)?;
        if path.is_empty() {
            return Err("path is empty".to_string());
        }
        let opts = Options {
            opt_level: level_arg(opt_level)?,
            libs: names(libs, lib_lens, n_libs)?,
            // Libraries are found as the system's `cc` finds them: no
            // directory is named through this interface (spec §9).
            lib_dirs: Vec::new(),
        };
        let m = crate::for_executable(src).map_err(|e| e.to_string())?;
        aot::build_executable(&m, path, Path::new(path), &opts).map_err(|e| e.to_string())
    })
}
