//! How the self-hosted reader is judged (spec/bootstrap.md §3): the tool
//! `compiler/read.fib`, built by `fibc build`, must print exactly what the
//! Rust reader's dump (`fibref::dump`) is, and with `--print` what the
//! Rust printer writes (`fibref::dump::print_source`), with the same exit
//! status, over
//!
//! 1. every `.fib` file of `cases/`, `lib/` and `compiler/` (which holds
//!    the edge inputs of `compiler/tests/reader/`) and the expander's
//!    prelude;
//! 2. paths that cannot be read (missing, not UTF-8, a directory);
//!    and one small file for each scalar value the reader's character
//!    classes decide on, with its neighbours (`unicode.rs`);
//! 3. 300 generated inputs from fixed seeds (`fuzz.rs`): mutations of the
//!    corpus, token soup, clean inputs ending in a snippet that provokes
//!    one named read error, and deep nesting around the limit.
//!
//! Every input is run in both modes (dump, print) with the one build of
//! the tool, and a failure says which mode. The tool with no file (and
//! with `--print` alone) must print nothing, say its usage on standard
//! error and end with status 2 (`usage.rs`).
//!
//! The comparison is byte for byte on standard output and on the exit
//! status (`compare.rs`); a failure names the first line that differs and
//! keeps the input in the scratch directory. Unit tests show that each
//! kind of difference is reported, and a stand-in tool run through the
//! same pipeline shows that it passes when faithful and fails when
//! damaged (`standin.rs`).
//!
//! Run it with `LLVM_SYS_211_PREFIX=... cargo test -p fibc --test
//! bootstrap`. It starts at most four processes at a time (`fibc build` and
//! its linker, or the tool; and a stand-in test's shell script with the
//! command it runs), each tool run capped at 4 GiB of address space and two
//! minutes, and keeps its inputs small: the machine has been taken down by
//! sweeps before. `BOOTSTRAP_FUZZ_COUNT` and `BOOTSTRAP_FUZZ_SEED` size a
//! bigger manual run, below.

#![cfg(unix)]

#[path = "bootstrap/check.rs"]
mod check;
#[path = "bootstrap/compare.rs"]
mod compare;
#[path = "bootstrap/corpus.rs"]
mod corpus;
#[path = "bootstrap/fuzz.rs"]
mod fuzz;
#[path = "bootstrap/nest.rs"]
mod nest;
#[path = "bootstrap/rng.rs"]
mod rng;
#[path = "bootstrap/soup.rs"]
mod soup;
#[path = "bootstrap/standin.rs"]
mod standin;
#[path = "bootstrap/tables.rs"]
mod tables;
#[path = "bootstrap/tmp.rs"]
mod tmp;
#[path = "bootstrap/tool.rs"]
mod tool;
#[path = "bootstrap/unicode.rs"]
mod unicode;
#[path = "bootstrap/usage.rs"]
mod usage;

use std::path::{Path, PathBuf};

use check::{check_files, report, Failure};
use tmp::TempDir;
use tool::{build_reader, repo_root, Mode, Tool};

/// A named list of files to compare.
struct Stage {
    name: String,
    files: Vec<PathBuf>,
}

/// Runs every stage in both modes and returns the report and whether all
/// passed, then the usage check (the tool with no file).
fn run_stages(tool: &Tool, stages: &[Stage]) -> (String, bool) {
    let mut text = String::new();
    let mut ok = true;
    for stage in stages {
        for mode in Mode::ALL {
            let failures = check_files(tool, mode, &stage.files);
            ok &= failures.is_empty();
            let what = format!("{} [{mode}]", stage.name);
            text.push_str(&report(&what, stage.files.len(), &failures));
        }
    }
    let usage: Vec<Failure> = usage::check_usage(tool);
    ok &= usage.is_empty();
    text.push_str(&report("usage (no file)", Mode::ALL.len(), &usage));
    (text, ok)
}

/// The corpus (with the prelude) and the unreadable paths.
fn fixed_stages(dir: &Path) -> (Vec<Stage>, Vec<String>) {
    let mut files = corpus::corpus_files(&repo_root());
    let texts = corpus::texts(&files);
    files.push(corpus::prelude_file(dir));
    assert!(
        files.len() > 150,
        "the corpus has only {} files",
        files.len()
    );
    let unreadable = corpus::unreadable_files(&dir.join("unreadable"));
    let stages = vec![
        Stage {
            name: "corpus".to_string(),
            files,
        },
        Stage {
            name: "unreadable".to_string(),
            files: unreadable,
        },
        Stage {
            name: "unicode classes".to_string(),
            files: unicode::files(&dir.join("unicode")),
        },
    ];
    (stages, texts)
}

fn generated_stage(dir: &Path, texts: &[String], runs: &[(u64, usize)]) -> Stage {
    let inputs: Vec<fuzz::Input> = runs
        .iter()
        .flat_map(|(seed, count)| fuzz::generate(*seed, *count, texts))
        .collect();
    Stage {
        name: format!(
            "generated (seeds {:?})",
            runs.iter().map(|r| r.0).collect::<Vec<_>>()
        ),
        files: fuzz::write_inputs(&dir.join("fuzz"), &inputs),
    }
}

#[test]
fn the_self_hosted_reader_matches_the_rust_reader() {
    let dir = TempDir::new("reader");
    let tool = build_reader(dir.path());
    let (mut stages, texts) = fixed_stages(dir.path());
    let runs: Vec<(u64, usize)> = fuzz::SEEDS.iter().map(|s| (*s, fuzz::PER_SEED)).collect();
    stages.push(generated_stage(dir.path(), &texts, &runs));
    let (mut text, ok) = run_stages(&tool, &stages);
    match standin::fault_in_the_real_tool_is_noticed(dir.path(), &tool) {
        Ok(()) => text.push_str(
            "canary: a fault planted in the real reader is reported on 3 of 3 in each mode\n",
        ),
        Err(why) => panic!("the pipeline missed a fault planted in the real tool: {why}\n{text}"),
    }
    assert!(
        ok,
        "the self-hosted reader differs from the Rust reader:\n{text}"
    );
    eprintln!("{text}");
}

/// A bigger run of generated inputs only, for a person to start: set
/// `BOOTSTRAP_FUZZ_COUNT` (default 1000, at most 2000) and
/// `BOOTSTRAP_FUZZ_SEED` (default 1), and run it alone:
/// `cargo test -p fibc --test bootstrap -- --ignored`.
#[test]
#[ignore = "a manual run sized by BOOTSTRAP_FUZZ_COUNT (at most 2000) and BOOTSTRAP_FUZZ_SEED"]
fn the_self_hosted_reader_matches_on_a_bigger_generated_run() {
    let (seed, count) = fuzz::manual_settings(|name| std::env::var(name).ok());
    let dir = TempDir::new("reader-big");
    let tool = build_reader(dir.path());
    let texts = corpus::texts(&corpus::corpus_files(&repo_root()));
    let stage = generated_stage(dir.path(), &texts, &[(seed, count)]);
    let (text, ok) = run_stages(&tool, &[stage]);
    assert!(
        ok,
        "the self-hosted reader differs from the Rust reader:\n{text}"
    );
    eprintln!("{text}");
}
