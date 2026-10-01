//! Embeds the library modules of `lib/` (spec/syntax.md §5, stdlib design
//! §7 E7) as the table `LIB_MODULES` that `roots.rs` includes, so that a
//! copy of the executable finds the library wherever it is run, as it
//! finds the prelude (`types::PRELUDE_LIB`, which is `lib/prelude.fib`
//! and is not one of these). A module `a.b` is `lib/a/b.fib`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

/// Every `.fib` file under `dir` other than the prelude, as (module
/// name, path relative to `lib`, absolute path).
fn collect(lib: &Path, dir: &Path, out: &mut Vec<(String, String, PathBuf)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(lib, &path, out)?;
            continue;
        }
        let rel = path.strip_prefix(lib).unwrap_or(&path);
        if path.extension().is_none_or(|e| e != "fib") || rel == Path::new("prelude.fib") {
            continue;
        }
        let name = rel
            .with_extension("")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(".");
        let shown = rel.to_string_lossy().replace('\\', "/");
        out.push((name, format!("lib/{shown}"), fs::canonicalize(&path)?));
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("no CARGO_MANIFEST_DIR")?);
    let lib = manifest.join("../../lib");
    println!("cargo:rerun-if-changed={}", lib.display());
    let mut modules = Vec::new();
    if lib.is_dir() {
        collect(&lib, &lib, &mut modules)?;
    }
    modules.sort();
    let mut text = String::from(
        "/// The library modules of `lib/` (name, file, source), by name.\n\
         pub const LIB_MODULES: &[(&str, &str, &str)] = &[\n",
    );
    for (name, shown, path) in &modules {
        let path = path.to_string_lossy();
        writeln!(text, "    ({name:?}, {shown:?}, include_str!({path:?})),")?;
    }
    text.push_str("];\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("no OUT_DIR")?);
    fs::write(out.join("lib_modules.rs"), text)?;
    Ok(())
}
