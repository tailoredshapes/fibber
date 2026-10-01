//! Linking an executable against libraries the program names with
//! `extern` (spec/compiler.md §1, `fibc build FILE -o OUT -L DIR -l LIB`).
//!
//! `lair::aot::build_executable` links `-lNAME` for each library but has
//! no library directory and no rpath, so a library that is in none of
//! the system's directories could be named and not found, and one that
//! was found at link time would not be found when the executable ran
//! unless `LD_LIBRARY_PATH` said where. This module is the link step
//! with both: the object file is `lair`'s (`aot::emit`), the `cc`
//! command line is built here. It belongs in `lair::aot::Options` (a
//! `lib_dirs` beside `libs`) and moves there when that crate takes it.

use std::path::{Path, PathBuf};
use std::process::Command;

use lair::aot::{emit, Options, Output};
use lair::Error;

/// The libraries every executable links: libm (`frem` is lowered to
/// `fmod`) and libpthread (`spawn`, types §8.8).
const BASE_LIBS: [&str; 2] = ["m", "pthread"];

/// A `-L` directory as the linker and the loader are given it: its
/// absolute, canonical path, so that the executable finds the library
/// from any working directory. `ld.so` splits an rpath at `:` and reads
/// `$` as a variable (`$ORIGIN`), so a name with either cannot be written
/// as an rpath and is refused here, not found missing at run time.
pub fn library_dir(dir: &str) -> Result<PathBuf, String> {
    let path = std::fs::canonicalize(dir).map_err(|e| format!("-L {dir}: {e}"))?;
    if !path.is_dir() {
        return Err(format!("-L {dir}: not a directory"));
    }
    let text = path.to_string_lossy();
    if text.contains(':') || text.contains('$') {
        return Err(format!(
            "-L {dir}: {text} has a ':' or a '$', which an rpath cannot hold"
        ));
    }
    Ok(path)
}

/// The arguments `cc` gets after the object file and `-o OUT`: the
/// base libraries, then for each directory `-L DIR` and an rpath
/// (`-Xlinker`, which does not split at commas), then `-lNAME` for each
/// library, after everything that may need it.
pub fn link_args(dirs: &[PathBuf], libs: &[String]) -> Vec<String> {
    let mut args: Vec<String> = BASE_LIBS.iter().map(|l| format!("-l{l}")).collect();
    for d in dirs {
        let d = d.to_string_lossy();
        args.extend([
            format!("-L{d}"),
            "-Xlinker".into(),
            "-rpath".into(),
            "-Xlinker".into(),
            d.into_owned(),
        ]);
    }
    args.extend(libs.iter().map(|l| format!("-l{l}")));
    args
}

/// Compiles `module` (which satisfies the `main` rule) and links it into
/// the executable `out`, against the directories (from [`library_dir`])
/// and library names of `fibc build`'s `-L` and `-l`.
pub fn build_executable(
    module: &lir::Module,
    name: &str,
    out: &Path,
    dirs: &[PathBuf],
    libs: &[String],
) -> Result<(), Error> {
    let object = emit(module, name, Output::Object, &Options::default())?;
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
        .args(link_args(dirs, libs))
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

    #[test]
    fn a_program_with_no_flags_links_what_it_always_did() {
        assert_eq!(
            link_args(&[], &[]),
            vec!["-lm".to_string(), "-lpthread".to_string()]
        );
    }

    #[test]
    fn each_directory_is_searched_and_an_rpath_and_each_library_comes_last() {
        let args = link_args(
            &[PathBuf::from("/a"), PathBuf::from("/b c")],
            &["x".to_string(), "y".to_string()],
        );
        assert_eq!(
            args,
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
        let err = library_dir("/no/such/fibc-dir").expect_err("missing");
        assert!(err.starts_with("-L /no/such/fibc-dir: "), "{err}");
        let file = library_dir("Cargo.toml").expect_err("a file");
        assert_eq!(file, "-L Cargo.toml: not a directory");
    }

    #[test]
    fn a_directory_an_rpath_cannot_hold_is_refused() {
        let base = std::env::temp_dir().join(format!("fibc-link-{}", std::process::id()));
        for odd in ["a:b", "a$b"] {
            let dir = base.join(odd);
            std::fs::create_dir_all(&dir).expect("a temp dir");
            let err = library_dir(dir.to_str().expect("utf-8")).expect_err(odd);
            assert!(err.contains("an rpath cannot hold"), "{err}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
