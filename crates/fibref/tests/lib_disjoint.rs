//! The gate on the library's layout (stdlib design §6.2, plan §4.2): the
//! four facades `fib.core`, `fib.seq`, `fib.coll` and `fib.print` are the
//! implicit modules, so a name that two of them export would be an error
//! wherever it is used bare; and every file under `lib/fib/{core,seq,
//! coll,print}` is a part that exactly one facade lists, in a file of
//! fewer than 500 lines. [`problems`] says what is wrong with a library
//! root; the tests show it is silent on the repository's `lib/` and loud
//! on each fault planted in a copy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fibref::eval::MacroEvaluator;
use fibref::expand::ExpandCtx;
use fibref::modules::{expand_all, spec_of, try_load_with};
use fibref::roots::Roots;
use fibref::syntax::{read_all, Form};
use fibref::types::{lower_modules, prelude_forms, CHECK_STACK};

/// The facades, in the order they are loaded.
const FACADES: [&str; 4] = ["fib.core", "fib.seq", "fib.coll", "fib.print"];

/// The most lines a part may have: under 500 (CLAUDE.md).
const MAX_LINES: usize = 499;

fn lib_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lib")
}

/// Every fault of the library under `lib`: the names two facades export,
/// the files no facade lists or two list, and the files that are too long.
fn problems(lib: &Path) -> Vec<String> {
    let mut found = file_problems(lib);
    found.extend(name_problems(lib));
    found
}

/// The `.fib` files of the directory `dir`, sorted.
fn fib_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "fib"))
        .collect();
    files.sort();
    files
}

/// The modules each facade of `lib` re-exports, by facade.
fn listed(lib: &Path) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for facade in FACADES {
        let file = lib.join(format!("{}.fib", facade.replace('.', "/")));
        let forms = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| read_all(&text, &file.to_string_lossy()).ok())
            .unwrap_or_default();
        let exports = spec_of(&forms, facade)
            .map(|s| s.exports)
            .unwrap_or_default();
        out.insert(facade.to_string(), exports);
    }
    out
}

fn file_problems(lib: &Path) -> Vec<String> {
    let listed = listed(lib);
    let mut found = Vec::new();
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for facade in FACADES {
        let rel = facade.replace('.', "/");
        files.push((facade.to_string(), lib.join(format!("{rel}.fib"))));
        for part in fib_files(&lib.join(&rel)) {
            let stem = part.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            files.push((format!("{facade}.{stem}"), part));
        }
    }
    for (ns, file) in files {
        let lines = std::fs::read_to_string(&file).map_or(0, |t| t.lines().count());
        if lines > MAX_LINES {
            found.push(format!("{ns} has {lines} lines: a file is under 500"));
        }
        if FACADES.contains(&ns.as_str()) {
            continue;
        }
        let by: Vec<&str> = listed
            .iter()
            .filter(|(_, parts)| parts.contains(&ns))
            .map(|(f, _)| f.as_str())
            .collect();
        match by.len() {
            1 => {}
            0 => found.push(format!("{ns} is listed by no facade")),
            _ => found.push(format!("{ns} is listed by {}", by.join(" and "))),
        }
    }
    found
}

/// A program that requires the four facades.
fn facade_main() -> String {
    let aliases: Vec<String> = FACADES.iter().map(|f| format!("[{f} :as {f}]")).collect();
    format!(
        "(ns main (:require {}))\n(defun main () -> i64 0)\n",
        aliases.join(" ")
    )
}

/// The names that two facades of `lib` export, one problem per pair of
/// facades: the front end is run over a program that requires all four.
fn name_problems(lib: &Path) -> Vec<String> {
    let tag = lib.to_string_lossy().replace(['/', '.'], "_");
    let dir =
        std::env::temp_dir().join(format!("fibber-lib-disjoint-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let main = facade_main();
    let file = dir.join("main.fib").to_string_lossy().into_owned();
    let roots = Roots::new(vec![lib.to_path_buf()]);
    let result = exports_of_facades(&main, &file, &roots);
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(sets) => collisions(&sets),
        Err(message) => vec![format!("the facades do not load and check: {message}")],
    }
}

/// The names each facade exports, by facade, from the front end's tables,
/// on a thread with the stack the checker needs.
fn exports_of_facades(
    main: &str,
    file: &str,
    roots: &Roots,
) -> Result<Vec<(String, Vec<String>)>, String> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(CHECK_STACK)
            .spawn_scoped(scope, || exports_here(main, file, roots))
            .expect("a thread")
            .join()
            .expect("the front end does not panic")
    })
}

fn exports_here(
    main: &str,
    file: &str,
    roots: &Roots,
) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx)?;
    let loaded = try_load_with(main, file, roots, &[]).map_err(|e| e.to_string())?;
    let all: Vec<Form> = loaded.iter().flat_map(|l| l.forms.clone()).collect();
    let mut runner = MacroEvaluator::new(&all, prelude.clone());
    let modules = expand_all(loaded, &mut ctx, &mut runner).map_err(|e| e.to_string())?;
    let lowered = lower_modules(&modules, &prelude).map_err(|e| {
        e.iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let g = &lowered.globals;
    FACADES
        .iter()
        .map(|f| {
            let id = g.module_id(f).ok_or_else(|| format!("{f} is not loaded"))?;
            Ok((f.to_string(), g.exports(id).into_iter().collect()))
        })
        .collect()
}

/// One problem for each pair of facades that export a name in common.
fn collisions(sets: &[(String, Vec<String>)]) -> Vec<String> {
    let mut found = Vec::new();
    for (i, (a, names_a)) in sets.iter().enumerate() {
        for (b, names_b) in &sets[i + 1..] {
            let both: Vec<&str> = names_a
                .iter()
                .filter(|n| names_b.contains(n))
                .map(String::as_str)
                .collect();
            if !both.is_empty() {
                found.push(format!("{a} and {b} both export {}", both.join(" ")));
            }
        }
    }
    found
}

#[test]
fn the_librarys_facades_export_disjoint_names_and_list_every_part_once() {
    let found = problems(&lib_dir());
    assert!(
        found.is_empty(),
        "the library's layout is wrong:\n{}",
        found.join("\n")
    );
}

/// A library root with a clean layout in a fresh directory: four facades
/// with one part each, `defun`s `a b c d` apart.
struct Fixture(PathBuf);

impl Fixture {
    fn new(label: &str) -> Fixture {
        let dir =
            std::env::temp_dir().join(format!("fibber-lib-fixture-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let fixture = Fixture(dir);
        for (facade, name) in FACADES.iter().zip(["a", "b", "c", "d"]) {
            let short = facade.trim_start_matches("fib.");
            let part = format!("{facade}.part");
            fixture.write(
                &format!("{short}.fib"),
                &format!("(ns {facade} (:export-from {part}))\n"),
            );
            let body = format!("(ns {part})\n(defun {name} () -> i64 1)\n");
            fixture.write(&format!("{short}/part.fib"), &body);
        }
        fixture
    }

    fn write(&self, rel: &str, text: &str) {
        let file = self.0.join("fib").join(rel);
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("dirs");
        std::fs::write(file, text).expect("write");
    }

    /// The library root: the directory that holds `fib/`.
    fn root(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_clean_layout_has_no_problem() {
    let f = Fixture::new("clean");
    assert_eq!(problems(f.root()), Vec::<String>::new());
}

#[test]
fn a_name_that_two_facades_export_is_found() {
    let f = Fixture::new("collision");
    f.write("seq/part.fib", "(ns fib.seq.part)\n(defun a () -> i64 2)\n");
    assert_eq!(problems(f.root()), ["fib.core and fib.seq both export a"]);
}

#[test]
fn a_part_that_no_facade_lists_or_two_list_is_found() {
    let f = Fixture::new("listing");
    f.write(
        "core/orphan.fib",
        "(ns fib.core.orphan)\n(defun orphan () -> i64 1)\n",
    );
    assert_eq!(
        problems(f.root()),
        ["fib.core.orphan is listed by no facade"]
    );
    f.write(
        "print.fib",
        "(ns fib.print (:export-from fib.print.part fib.core.orphan))\n",
    );
    f.write(
        "core.fib",
        "(ns fib.core (:export-from fib.core.part fib.core.orphan))\n",
    );
    assert_eq!(
        problems(f.root()),
        [
            "fib.core.orphan is listed by fib.core and fib.print",
            "fib.core and fib.print both export orphan"
        ]
    );
}

#[test]
fn a_file_of_500_lines_is_found_and_one_of_499_is_not() {
    let f = Fixture::new("long");
    let filler = |n: usize| "; filler\n".repeat(n);
    let head = "(ns fib.coll.part)\n(defun c () -> i64 1)\n";
    f.write("coll/part.fib", &format!("{head}{}", filler(497)));
    assert_eq!(problems(f.root()), Vec::<String>::new());
    f.write("coll/part.fib", &format!("{head}{}", filler(498)));
    assert_eq!(
        problems(f.root()),
        ["fib.coll.part has 500 lines: a file is under 500"]
    );
}

/// A copy of the repository's `lib/` in a fresh directory, to plant faults
/// in: the gate must be able to fail on the real facades, not only on
/// fixtures.
fn copy_of_lib(label: &str) -> Fixture {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).expect("dirs");
        for e in std::fs::read_dir(from).expect("a directory").flatten() {
            let (src, dst) = (e.path(), to.join(e.file_name()));
            match src.is_dir() {
                true => copy(&src, &dst),
                false => drop(std::fs::copy(&src, &dst).expect("copy")),
            }
        }
    }
    let dir = std::env::temp_dir().join(format!("fibber-lib-copy-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(&lib_dir(), &dir);
    Fixture(dir)
}

#[test]
fn a_collision_planted_in_a_copy_of_the_real_library_is_found() {
    let copy = copy_of_lib("planted");
    let roots = Roots::new(vec![copy.root().to_path_buf()]);
    let scratch = copy.root().join("scratch");
    std::fs::create_dir_all(&scratch).expect("dirs");
    let file = scratch.join("main.fib").to_string_lossy().into_owned();
    let sets = exports_of_facades(&facade_main(), &file, &roots).expect("the real facades load");
    let core = &sets
        .iter()
        .find(|(f, _)| f == "fib.core")
        .expect("fib.core")
        .1;
    let plain =
        |n: &&String| n.chars().all(|c| c.is_ascii_lowercase() || c == '-') && !n.is_empty();
    let name = core
        .iter()
        .find(plain)
        .expect("fib.core exports a plain lower-case name");
    let part = copy.root().join("fib/print/io.fib");
    std::fs::write(
        &part,
        format!("(ns fib.print.io)\n(defun {name} () -> i64 1)\n"),
    )
    .expect("write");
    let found = name_problems(copy.root());
    assert_eq!(
        found,
        [format!("fib.core and fib.print both export {name}")]
    );
}

#[test]
fn a_facade_exports_every_space_of_names_of_its_parts_and_not_what_is_private() {
    let f = Fixture::new("exports");
    let part =
        "(ns fib.core.part)\n(defstruct Pt (x: i64))\n(defprotocol Sized (size (self) -> i64))\n\
                (defun fa () -> i64 1)\n(defun hidden :private () -> i64 2)\n";
    f.write("core/part.fib", part);
    let roots = Roots::new(vec![f.root().to_path_buf()]);
    let file = f.root().join("main.fib").to_string_lossy().into_owned();
    let sets = exports_of_facades(&facade_main(), &file, &roots).expect("loads");
    let core = &sets
        .iter()
        .find(|(f, _)| f == "fib.core")
        .expect("fib.core")
        .1;
    assert_eq!(core, &["Pt", "Sized", "fa", "size"]);
}
