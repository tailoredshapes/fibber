//! Where a module is found (spec/syntax.md §5, spec/compiler.md §1;
//! stdlib design §7 E7), **Proposed**.
//!
//! A module `a.b` is the file `a/b.fib` under a root. The roots, in the
//! order they are tried:
//!
//! 1. the directory of the main file (what the loader did alone until
//!    now, and still does for a program that names no other root);
//! 2. each `-I DIR` of the command line, in the order given;
//! 3. each directory of the environment variable `FIB_LIB` (a list like
//!    `PATH`: `:` on Unix), in order;
//! 4. the built-in root: the library modules of the repository's `lib/`
//!    that the executable carries (`build.rs`), as it carries the prelude.
//!
//! The first root that has the file wins and the others are not read, so
//! a module of the program shadows a library module of the same name and
//! an earlier `-I` shadows a later one. A directory that does not exist
//! is skipped, as `cc` skips an `-I` that is not there; the error that
//! names a module found nowhere lists every place that was tried.
//!
//! The roots are an explicit value: the command line makes them (with
//! [`Roots::from_env`] for the one environment variable) and hands them
//! down, and nothing below the command line reads the environment.

use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// A module the executable carries: its name, the file name positions in
/// it show, its source.
pub type Embedded = (&'static str, &'static str, &'static str);

mod generated {
    include!(concat!(env!("OUT_DIR"), "/lib_modules.rs"));
}

/// The library modules of `lib/` that this executable carries.
pub use generated::LIB_MODULES;

/// Where modules are searched besides the main file's directory.
#[derive(Clone, Debug)]
pub struct Roots {
    dirs: Vec<PathBuf>,
    builtin: &'static [Embedded],
}

impl Default for Roots {
    /// No directories, and the built-in root.
    fn default() -> Roots {
        Roots::new(Vec::new())
    }
}

/// What looking for one module found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    /// The module's file name (as positions show it) and its source.
    Found { file: String, source: String },
    /// Nowhere: where the main file's directory would have it and the
    /// system's words for its absence there, then the files the other
    /// roots would have had it in.
    Missing {
        file: String,
        cause: String,
        also: Vec<String>,
    },
    /// A file is there and cannot be read (a directory, not UTF-8, no
    /// permission): where, and the system's words. Later roots are not
    /// tried, so a broken file is not hidden by another module.
    Unreadable { file: String, cause: String },
}

impl Roots {
    /// These directories, in order, then the built-in root.
    pub fn new(dirs: Vec<PathBuf>) -> Roots {
        Roots::with_builtin(dirs, LIB_MODULES)
    }

    /// [`Roots::new`] with another built-in table, for tests.
    pub fn with_builtin(dirs: Vec<PathBuf>, builtin: &'static [Embedded]) -> Roots {
        Roots { dirs, builtin }
    }

    /// The `-I` directories, then those of the value of `FIB_LIB` (the
    /// caller reads the variable: nothing here does), then the built-in
    /// root. An empty entry of the list names nothing and is skipped.
    pub fn from_env(dirs: &[String], fib_lib: Option<&OsStr>) -> Roots {
        let mut all: Vec<PathBuf> = dirs.iter().map(PathBuf::from).collect();
        if let Some(list) = fib_lib {
            all.extend(std::env::split_paths(list).filter(|p| !p.as_os_str().is_empty()));
        }
        Roots::new(all)
    }

    /// The directories searched after the main file's, in order.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// Looks for module `ns` (`a.b` is `a/b.fib`) beside the main file
    /// (in `main_dir`), under each directory, and last in the built-in
    /// root.
    pub fn find(&self, main_dir: &Path, ns: &str) -> Lookup {
        let rel = format!("{}.fib", ns.replace('.', "/"));
        let mut missing: Vec<(String, String)> = Vec::new();
        for root in std::iter::once(main_dir).chain(self.dirs.iter().map(PathBuf::as_path)) {
            let path = root.join(&rel);
            let file = path.to_string_lossy().into_owned();
            match std::fs::read_to_string(&path) {
                Ok(source) => return Lookup::Found { file, source },
                Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
                    missing.push((file, e.to_string()))
                }
                Err(e) => {
                    let cause = e.to_string();
                    return Lookup::Unreadable { file, cause };
                }
            }
        }
        if let Some((_, file, source)) = self.builtin.iter().find(|(name, ..)| *name == ns) {
            return Lookup::Found {
                file: (*file).to_string(),
                source: (*source).to_string(),
            };
        }
        let mut places = missing.into_iter();
        let (file, cause) = places.next().unwrap_or_default();
        let also = places.map(|(file, _)| file).collect();
        Lookup::Missing { file, cause, also }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMBEDDED: &[Embedded] = &[("lib.x", "lib/lib/x.fib", "(ns lib.x)")];

    fn dirs(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fibber-roots-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        for (path, text) in files {
            let file = dir.join(path);
            std::fs::create_dir_all(file.parent().expect("a parent")).expect("dirs");
            std::fs::write(file, text).expect("write");
        }
        dir
    }

    fn found(l: Lookup) -> String {
        match l {
            Lookup::Found { source, .. } => source,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_main_directory_comes_first_then_each_root_in_order_then_the_built_in_root() {
        let main = dirs("main", &[("a.fib", "main a"), ("b/c.fib", "main b.c")]);
        let one = dirs("one", &[("a.fib", "one a"), ("d.fib", "one d")]);
        let two = dirs(
            "two",
            &[("a.fib", "two a"), ("d.fib", "two d"), ("e.fib", "two e")],
        );
        let roots = Roots::with_builtin(vec![one.clone(), two.clone()], EMBEDDED);
        assert_eq!(found(roots.find(&main, "a")), "main a");
        assert_eq!(found(roots.find(&main, "b.c")), "main b.c");
        assert_eq!(found(roots.find(&main, "d")), "one d");
        assert_eq!(found(roots.find(&main, "e")), "two e");
        assert_eq!(found(roots.find(&main, "lib.x")), "(ns lib.x)");
        // A file in the main directory shadows the built-in module.
        let shadow = dirs("shadow", &[("lib/x.fib", "main lib.x")]);
        assert_eq!(found(roots.find(&shadow, "lib.x")), "main lib.x");
        for d in [main, one, two, shadow] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn a_module_in_two_roots_is_the_first_ones_and_the_second_is_not_read() {
        let one = dirs("first", &[("m.fib", "first")]);
        // The second root's file is not even valid UTF-8: it is never read.
        let two = dirs("second", &[]);
        std::fs::write(two.join("m.fib"), [0xff, 0xfe]).expect("write");
        let roots = Roots::with_builtin(vec![one.clone(), two.clone()], &[]);
        let main = dirs("nomain", &[]);
        assert_eq!(found(roots.find(&main, "m")), "first");
        for d in [main, one, two] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn a_module_found_nowhere_names_the_main_directorys_file_and_the_others() {
        let main = dirs("m3", &[]);
        let root = dirs("r3", &[]);
        let roots = Roots::with_builtin(vec![root.clone()], &[]);
        match roots.find(&main, "p.q") {
            Lookup::Missing { file, cause, also } => {
                assert_eq!(file, format!("{}/p/q.fib", main.display()));
                assert!(cause.contains("No such file"), "{cause}");
                assert_eq!(also, [format!("{}/p/q.fib", root.display())]);
            }
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(main);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_that_cannot_be_read_stops_the_search() {
        let main = dirs("m4", &[]);
        std::fs::write(main.join("bad.fib"), [0xff, 0xfe]).expect("write");
        let ok = dirs("r4", &[("bad.fib", "fine")]);
        let roots = Roots::with_builtin(vec![ok.clone()], &[]);
        match roots.find(&main, "bad") {
            Lookup::Unreadable { file, cause } => {
                assert!(file.ends_with("bad.fib"), "{file}");
                assert!(cause.contains("UTF-8"), "{cause}");
            }
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(main);
        let _ = std::fs::remove_dir_all(ok);
    }

    /// Every `.fib` file under `dir`, with its name relative to `lib`.
    fn library_files(lib: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).expect("lib is readable") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                library_files(lib, &path, out);
            } else if path.extension().is_some_and(|e| e == "fib") {
                let rel = path
                    .strip_prefix(lib)
                    .expect("under lib")
                    .with_extension("");
                let name = rel.to_string_lossy().replace('/', ".");
                out.push((name, std::fs::read_to_string(&path).expect("readable")));
            }
        }
    }

    /// `build.rs` embeds what `lib/` holds beside the prelude, which
    /// `types::PRELUDE_LIB` embeds on its own. (Until a module other than
    /// the prelude is added to `lib/` the two lists are both empty.)
    #[test]
    fn the_built_in_root_carries_every_library_module_of_lib_but_the_prelude() {
        let lib = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib");
        let mut files = Vec::new();
        library_files(&lib, &lib, &mut files);
        files.retain(|(name, _)| name != "prelude");
        files.sort();
        let mut carried: Vec<(String, String)> = LIB_MODULES
            .iter()
            .map(|(name, _, source)| (name.to_string(), source.to_string()))
            .collect();
        carried.sort();
        assert_eq!(carried, files);
        for (name, file, _) in LIB_MODULES {
            let rel = format!("lib/{}.fib", name.replace('.', "/"));
            assert_eq!(*file, rel);
        }
    }

    #[test]
    fn the_roots_are_the_flags_then_the_variable_and_an_empty_entry_names_nothing() {
        let list = std::env::join_paths(["x", "", "y"]).expect("joins");
        let roots = Roots::from_env(&["i1".to_string(), "i2".to_string()], Some(&list));
        let got: Vec<&str> = roots.dirs().iter().filter_map(|p| p.to_str()).collect();
        assert_eq!(got, ["i1", "i2", "x", "y"]);
        assert!(Roots::from_env(&[], None).dirs().is_empty());
    }
}
