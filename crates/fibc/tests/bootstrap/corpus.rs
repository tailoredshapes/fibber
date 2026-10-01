//! The fixed inputs of the differential test (spec/bootstrap.md §3): every
//! `.fib` file of `cases/`, `lib/` and `compiler/`, the expander's
//! prelude, and files that cannot be read.

use std::path::{Path, PathBuf};

/// The directories whose `.fib` files are the corpus.
const ROOTS: [&str; 3] = ["cases", "lib", "compiler"];

/// Every `.fib` file under `cases/`, `lib/` and `compiler/` of the
/// repository `root`, in path order (so the corpus, and the mutations of
/// it, are the same on every run).
pub fn corpus_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in ROOTS {
        collect(&root.join(dir), &mut found);
    }
    found.sort();
    found
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, found);
        } else if path.extension().is_some_and(|e| e == "fib") {
            found.push(path);
        }
    }
}

/// Writes the expander's prelude source as a file in `dir`.
pub fn prelude_file(dir: &Path) -> PathBuf {
    let path = dir.join("prelude.fib");
    std::fs::write(&path, fibref::expand::PRELUDE_SOURCE)
        .expect("the scratch directory is writable");
    path
}

/// Paths the reader cannot read, in `dir`: one that does not exist, one
/// that is not UTF-8 (a lone continuation byte, a truncated sequence)
/// and a directory. Each prints `unreadable` and ends with status 2.
pub fn unreadable_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).expect("the scratch directory is writable");
    let bad_utf8 = dir.join("not-utf8.fib");
    std::fs::write(&bad_utf8, b"(a \xFF b)\n").expect("writable");
    let truncated = dir.join("truncated-utf8.fib");
    std::fs::write(&truncated, b"(a \xC3").expect("writable");
    let a_directory = dir.join("a-directory.fib");
    std::fs::create_dir_all(&a_directory).expect("writable");
    vec![
        dir.join("does-not-exist.fib"),
        bad_utf8,
        truncated,
        a_directory,
    ]
}

/// The texts of the corpus files that are UTF-8, the material the
/// mutations start from.
pub fn texts(files: &[PathBuf]) -> Vec<String> {
    files
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;

    #[test]
    fn the_corpus_holds_the_cases_the_library_and_the_compiler_sorted() {
        let root = crate::tool::repo_root();
        let files = corpus_files(&root);
        let under = |d: &str| files.iter().filter(|f| f.starts_with(root.join(d))).count();
        assert!(under("cases/ownership") >= 100, "cases/ownership");
        assert!(under("cases/modules") >= 5, "cases/modules");
        assert!(under("lib") >= 1, "lib");
        assert!(under("compiler") >= 1, "compiler");
        let mut sorted = files.clone();
        sorted.sort();
        assert_eq!(files, sorted);
        assert!(files
            .iter()
            .all(|f| f.extension().is_some_and(|e| e == "fib")));
    }

    #[test]
    fn the_unreadable_files_really_cannot_be_read_as_text() {
        let dir = TempDir::new("corpus-unreadable");
        let files = unreadable_files(dir.path());
        assert_eq!(files.len(), 4);
        for f in &files {
            assert!(std::fs::read_to_string(f).is_err(), "{}", f.display());
        }
    }

    #[test]
    fn the_prelude_file_is_the_expanders_prelude() {
        let dir = TempDir::new("corpus-prelude");
        let path = prelude_file(dir.path());
        assert_eq!(
            std::fs::read_to_string(path).expect("readable"),
            fibref::expand::PRELUDE_SOURCE
        );
    }
}
