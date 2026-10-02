//! An executable from a module, ahead of time (`aot::build_executable`).

use std::ffi::{c_char, c_int};
use std::path::Path;
use std::ptr;

use super::error::{guard, level_arg, text, LairError};
use crate::aot::{self, Options};

/// A list of names as C passes it: what a name is, and the names of the
/// three parameters (the names, their lengths, their count) for the
/// message that says one is null.
struct List {
    what: &'static str,
    params: [&'static str; 3],
}

const LIBS: List = List {
    what: "a library name",
    params: ["libs", "lib_lens", "n_libs"],
};
const DIRS: List = List {
    what: "a library directory",
    params: ["dirs", "dir_lens", "n_dirs"],
};

/// The `n` names `items[i]` of byte length `lens[i]`.
///
/// # Safety
/// For `n > 0`, `items` and `lens` point to `n` readable elements and
/// each `items[i]` to `lens[i]` readable bytes.
unsafe fn names(
    list: &List,
    items: *const *const c_char,
    lens: *const usize,
    n: usize,
) -> Result<Vec<String>, String> {
    if n == 0 {
        return Ok(Vec::new());
    }
    if items.is_null() || lens.is_null() {
        let [a, b, c] = list.params;
        return Err(format!("{a} and {b} must not be null for {c} {n}"));
    }
    (0..n)
        .map(|i| text(list.what, *items.add(i), *lens.add(i)).map(str::to_string))
        .collect()
}

/// Compile the module `src`, which must satisfy the `main` rule of
/// spec/lir.md §7.2, and link it with `cc` into the executable `path`.
/// `opt_level` is 0 to 3; each of the `n_libs` names (`libs[i]`, of
/// byte length `lib_lens[i]`) is linked as `-lNAME`, after libm and
/// libpthread, which are always linked, and is searched for where the
/// system's `cc` searches: no library directory (`-L`, rpath) is given
/// through this function (`lair_build_executable_with` takes them). The
/// executable's exit status is `main`'s result. On failure the error
/// holds the diagnostics, or the linker's output.
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
    lair_build_executable_with(
        src,
        src_len,
        path,
        path_len,
        opt_level,
        libs,
        lib_lens,
        n_libs,
        ptr::null(),
        ptr::null(),
        0,
    )
}

/// As `lair_build_executable`, and each of the `n_dirs` library
/// directories (`dirs[i]`, of byte length `dir_lens[i]`) is a `-L`
/// directory of the link and an absolute rpath (`aot::library_dir` says
/// how a name becomes one), as `aot::Options::lib_dirs` and `fibc build
/// -L` do: the executable finds its libraries without
/// `LD_LIBRARY_PATH`. A directory that is missing, is no directory or
/// cannot be an rpath is an error before anything is compiled.
///
/// # Safety
/// As `lair_build_executable`; for `n_dirs > 0`, `dirs` and `dir_lens`
/// point to `n_dirs` readable elements, each name to its length in
/// readable bytes.
#[no_mangle]
#[allow(clippy::too_many_arguments)] // the shape §9 fixes
pub unsafe extern "C" fn lair_build_executable_with(
    src: *const c_char,
    src_len: usize,
    path: *const c_char,
    path_len: usize,
    opt_level: c_int,
    libs: *const *const c_char,
    lib_lens: *const usize,
    n_libs: usize,
    dirs: *const *const c_char,
    dir_lens: *const usize,
    n_dirs: usize,
) -> *mut LairError {
    guard(|| {
        let src = text("src", src, src_len)?;
        let path = text("path", path, path_len)?;
        if path.is_empty() {
            return Err("path is empty".to_string());
        }
        let opts = Options {
            opt_level: level_arg(opt_level)?,
            libs: names(&LIBS, libs, lib_lens, n_libs)?,
            lib_dirs: names(&DIRS, dirs, dir_lens, n_dirs)?,
        };
        let m = crate::for_executable(src).map_err(|e| e.to_string())?;
        aot::build_executable(&m, path, Path::new(path), &opts).map_err(|e| e.to_string())
    })
}
