//! The link step of `aot::build_executable` (spec/lir.md §11): the
//! `cc` command that turns the object file into an executable.
//!
//! Besides the libraries the module names (`-lNAME`), a program may
//! need a library that is in none of the system's directories. Each
//! `Options::lib_dirs` entry is therefore both a directory the linker
//! searches (`-L`) and an **rpath**, so that the executable finds its
//! libraries when it runs, from any directory and without
//! `LD_LIBRARY_PATH`. The rpath is the directory's absolute canonical
//! path: a relative `-L` is relative to the process that links, and the
//! executable is not run from there.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};

/// The libraries every executable links: libm (LLVM lowers `frem` to
/// `fmod`, and C links libm only on request) and libpthread (a module
/// may call `pthread_create`, as the fibber runtime's `spawn` does; on
/// a C library that keeps threads in libc it is an empty stub).
const BASE_LIBS: [&str; 2] = ["m", "pthread"];

/// A library directory as the linker and the loader are given it: its
/// absolute, canonical path. `ld.so` splits an rpath at `:` and reads
/// `$` as a variable (`$ORIGIN`), so a name with either cannot be
/// written as an rpath and is refused here, not found missing at run
/// time. The error is `-L DIR: REASON`.
pub fn library_dir(dir: &str) -> std::result::Result<PathBuf, String> {
    let path = std::fs::canonicalize(dir).map_err(|e| format!("-L {dir}: {e}"))?;
    if !path.is_dir() {
        return Err(format!("-L {dir}: not a directory"));
    }
    if path
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .any(|b| matches!(b, b':' | b'$'))
    {
        return Err(format!(
            "-L {dir}: {} has a ':' or a '$', which an rpath cannot hold",
            path.display()
        ));
    }
    Ok(path)
}

/// The arguments `cc` gets after the object file and `-o OUT`: the base
/// libraries, then for each directory `-L DIR` and an rpath
/// (`-Xlinker`, which unlike `-Wl,` does not split at a comma), then
/// `-lNAME` for each library, after everything that may need it.
pub(super) fn args(dirs: &[PathBuf], libs: &[String]) -> Vec<OsString> {
    let mut args: Vec<OsString> = BASE_LIBS.iter().map(|l| format!("-l{l}").into()).collect();
    for d in dirs {
        let mut search = OsString::from("-L");
        search.push(d);
        args.extend([
            search,
            "-Xlinker".into(),
            "-rpath".into(),
            "-Xlinker".into(),
            d.into(),
        ]);
    }
    args.extend(libs.iter().map(|l| format!("-l{l}").into()));
    args
}

/// Writes `object` beside `out`, links it into the executable `out` with
/// `cc`, and removes it again. A failed link is the linker's own words.
pub(super) fn link(object: &[u8], out: &Path, dirs: &[PathBuf], libs: &[String]) -> Result<()> {
    let obj_path = out.with_file_name(format!(
        "{}.lair.o",
        out.file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    std::fs::write(&obj_path, object)
        .map_err(|e| Error::Backend(format!("cannot write {}: {e}", obj_path.display())))?;
    let ran = Command::new("cc")
        .arg(&obj_path)
        .arg("-o")
        .arg(out)
        .args(args(dirs, libs))
        .output();
    let _ = std::fs::remove_file(&obj_path);
    match ran {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(Error::Backend(format!(
            "linker failed: {}",
            String::from_utf8_lossy(&o.stderr)
        ))),
        Err(e) => Err(Error::Backend(format!("cannot run cc: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(args: Vec<OsString>) -> Vec<String> {
        args.into_iter()
            .map(|a| a.into_string().expect("utf-8"))
            .collect()
    }

    #[test]
    fn a_program_with_no_directories_or_libraries_links_the_base_libraries() {
        assert_eq!(strs(args(&[], &[])), ["-lm", "-lpthread"]);
    }

    #[test]
    fn each_directory_is_searched_and_an_rpath_and_each_library_comes_last() {
        let got = args(
            &[PathBuf::from("/a"), PathBuf::from("/b c")],
            &["x".to_string(), "y".to_string()],
        );
        assert_eq!(
            strs(got),
            [
                "-lm",
                "-lpthread",
                "-L/a",
                "-Xlinker",
                "-rpath",
                "-Xlinker",
                "/a",
                "-L/b c",
                "-Xlinker",
                "-rpath",
                "-Xlinker",
                "/b c",
                "-lx",
                "-ly"
            ]
        );
    }

    #[test]
    fn a_directory_is_made_absolute_and_a_missing_one_is_refused() {
        let here = library_dir(".").expect("the current directory exists");
        assert!(here.is_absolute(), "{here:?}");
        let err = library_dir("/no/such/lair-dir").expect_err("missing");
        assert!(err.starts_with("-L /no/such/lair-dir: "), "{err}");
        let file = library_dir("Cargo.toml").expect_err("a file");
        assert_eq!(file, "-L Cargo.toml: not a directory");
    }

    #[test]
    fn a_directory_an_rpath_cannot_hold_is_refused() {
        let base = std::env::temp_dir().join(format!("lair-link-{}", std::process::id()));
        for odd in ["a:b", "a$b"] {
            let dir = base.join(odd);
            std::fs::create_dir_all(&dir).expect("a temp dir");
            let err = library_dir(dir.to_str().expect("utf-8")).expect_err(odd);
            assert!(err.contains("an rpath cannot hold"), "{err}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
