//! Round 3: what the runner does with directory entries that are named
//! like cases but are not plain files, and with file names the table
//! cannot show on one line. The contract: `run_dir` runs the `*.fib`
//! case files in a directory, in file-name order, and reports every
//! result; a case that cannot be read is a HeaderError, never silently
//! absent.

use std::fs;
use std::os::unix::fs::symlink;

use fibref::cases::{
    list_cases, list_cases_recursive, render, run_dir, HeaderErrorKind, Report, Status,
};

use super::support::{accept_header, reject_header, Scripted, TempDir};

fn names(report: &Report) -> Vec<String> {
    report.results.iter().map(|r| r.name()).collect()
}

#[test]
fn a_symlink_to_a_case_runs_under_the_link_name() {
    let dir = TempDir::new("symlink-file");
    let real = dir.write(
        "real/target.fib",
        &format!("{}(scripted-compiled 1)\n", accept_header(1)),
    );
    symlink(&real, dir.path().join("01-link.fib")).unwrap();
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert_eq!(names(&report), vec!["01-link.fib"]);
    assert_eq!(report.results[0].status, Status::Pass);
    assert_eq!(report.results[0].path, dir.path().join("01-link.fib"));
}

#[test]
fn a_dangling_symlink_named_like_a_case_is_a_header_error_not_silently_absent() {
    // Interpretation: an entry `NN-name.fib` in the cases directory is
    // a case. If it cannot be read, that is a HeaderError (Unreadable)
    // like any other unreadable case, so the run fails and the row is
    // shown. Dropping it silently would let a broken checkout pass CI
    // with one case fewer.
    let dir = TempDir::new("symlink-dangling");
    dir.write("01-ok.fib", &reject_header("x"));
    symlink(
        dir.path().join("nowhere.fib"),
        dir.path().join("02-gone.fib"),
    )
    .unwrap();
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert_eq!(
        names(&report),
        vec!["01-ok.fib", "02-gone.fib"],
        "the dangling link must be reported, not dropped"
    );
    match &report.results[1].status {
        Status::HeaderError(e) => {
            assert_eq!(e.line, 0);
            assert!(matches!(e.kind, HeaderErrorKind::Unreadable(_)), "{e}");
        }
        other => panic!("expected a header error, got {other:?}"),
    }
    assert!(!report.ok());
}

#[test]
fn a_dangling_symlink_is_listed_recursively_too() {
    // Same interpretation for the recursive listing that CI uses over
    // the real cases directory.
    let dir = TempDir::new("symlink-dangling-recursive");
    dir.write("sub/01-ok.fib", &reject_header("x"));
    symlink(
        dir.path().join("nowhere.fib"),
        dir.path().join("sub/02-gone.fib"),
    )
    .unwrap();
    let listed = list_cases_recursive(dir.path()).unwrap();
    let names: Vec<String> = listed
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["01-ok.fib", "02-gone.fib"]);
}

#[test]
fn a_symlink_to_a_directory_named_like_a_case_is_not_a_case() {
    // A directory is not a case whether or not it is reached through a
    // link; consistent with the round-2 pin for a real `dir.fib/`.
    let dir = TempDir::new("symlink-dir");
    dir.write("target/inner.fib", &reject_header("x"));
    symlink(dir.path().join("target"), dir.path().join("01-link.fib")).unwrap();
    dir.write("02-ok.fib", &reject_header("x"));
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert_eq!(names(&report), vec!["02-ok.fib"]);
}

#[test]
fn a_symlinked_directory_is_run_like_a_real_one() {
    let dir = TempDir::new("symlink-root");
    dir.write("real/01.fib", &reject_header("x"));
    dir.write("real/02.fib", &accept_header(1));
    let link = dir.path().join("link");
    symlink(dir.path().join("real"), &link).unwrap();
    let report = run_dir(&link, &Scripted).unwrap();
    assert_eq!(names(&report), vec!["01.fib", "02.fib"]);
    assert_eq!(report.counts.pending, 2);
    for result in &report.results {
        assert!(result.path.starts_with(&link), "{}", result.path.display());
    }
}

#[test]
fn the_extension_is_matched_exactly() {
    // `.FIB`, `.fib ` and `.fib.` are not `.fib`.
    let dir = TempDir::new("extension-exact");
    dir.write("01.FIB", &reject_header("x"));
    dir.write("02.fib ", &reject_header("x"));
    dir.write("03.fib.", &reject_header("x"));
    dir.write("04.fib", &reject_header("x"));
    let listed = list_cases(dir.path()).unwrap();
    assert_eq!(listed, vec![dir.path().join("04.fib")]);
}

#[test]
fn a_file_named_only_dot_fib_is_not_a_case() {
    // Interpretation: `*.fib` in the shell sense; a dotfile has no
    // stem and no extension. Pinned so the choice is visible.
    let dir = TempDir::new("dot-fib");
    dir.write(".fib", &reject_header("x"));
    dir.write("01.fib", &reject_header("x"));
    let listed = list_cases(dir.path()).unwrap();
    assert_eq!(listed, vec![dir.path().join("01.fib")]);
}

#[test]
fn a_hidden_case_file_is_still_a_case() {
    // `.01-hidden.fib` has the extension `fib`; nothing in the contract
    // skips dotfiles. Pinned.
    let dir = TempDir::new("hidden-case");
    dir.write(".01-hidden.fib", &reject_header("x"));
    dir.write("02.fib", &reject_header("x"));
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert_eq!(names(&report), vec![".01-hidden.fib", "02.fib"]);
}

#[test]
fn file_name_order_is_by_bytes_so_uppercase_sorts_first() {
    let dir = TempDir::new("byte-order");
    for name in ["b.fib", "B.fib", "a.fib", "_.fib", "1.fib", "é.fib"] {
        dir.write(name, &reject_header("x"));
    }
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert_eq!(
        names(&report),
        vec!["1.fib", "B.fib", "_.fib", "a.fib", "b.fib", "é.fib"]
    );
}

#[test]
fn a_newline_in_a_file_name_still_gives_one_table_row_per_case() {
    // Linux allows a newline in a file name. The table promises exactly
    // one row per case whatever the detail contains; the name must not
    // break that promise either, or the row count lies.
    let dir = TempDir::new("newline-name");
    dir.write("01.fib", &reject_header("x"));
    fs::write(dir.path().join("02\nsplit.fib"), reject_header("x")).unwrap();
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert_eq!(report.counts.total(), 2);
    let text = render(&report);
    let table: Vec<&str> = text.split("\n\n").next().unwrap_or("").lines().collect();
    assert_eq!(
        table.len(),
        3,
        "heading plus one row per case, got:\n{text}"
    );
}

#[test]
fn a_case_that_is_a_broken_link_does_not_stop_the_other_cases() {
    // Whatever the broken entry is reported as, the readable cases
    // beside it must still be run and counted.
    let dir = TempDir::new("symlink-dangling-others");
    dir.write(
        "01-ok.fib",
        &format!("{}(scripted-compiled 1)\n", accept_header(1)),
    );
    symlink(
        dir.path().join("nowhere.fib"),
        dir.path().join("02-gone.fib"),
    )
    .unwrap();
    dir.write(
        "03-ok.fib",
        &format!("{}(scripted-reject x)\n", reject_header("x")),
    );
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert_eq!(report.counts.pass, 2, "{report:?}");
    assert!(names(&report).contains(&"01-ok.fib".to_string()));
    assert!(names(&report).contains(&"03-ok.fib".to_string()));
}

#[test]
fn an_unreadable_directory_entry_does_not_lose_the_report_when_listing_succeeds() {
    // `run_dir` only fails when the directory cannot be listed; a bad
    // entry becomes a row. Here every entry is fine, so the report has
    // exactly the files on disk, and the count agrees with `list_cases`.
    let dir = TempDir::new("listing-agrees");
    for i in 0..25 {
        dir.write(&format!("{i:02}.fib"), &reject_header("x"));
    }
    let listed = list_cases(dir.path()).unwrap();
    let report = run_dir(dir.path(), &Scripted).unwrap();
    assert_eq!(listed.len(), 25);
    assert_eq!(report.counts.total(), 25);
    let paths: Vec<_> = report.results.iter().map(|r| r.path.clone()).collect();
    assert_eq!(paths, listed);
}
