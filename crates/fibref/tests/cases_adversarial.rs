//! Adversarial tests for the case harness (`crates/fibref/src/cases`) and
//! the `fibref cases` CLI, through their public API and by running the
//! binary. Contract: `spec/method.md` rule 3 and
//! `cases/ownership/README.md`.
//!
//! Split by responsibility so that each file stays under 500 lines:
//! `support` (temp dirs, a scripted evaluator), `header` (parsing),
//! `verdict` (judging), `runner` (directories and reports), `cli` (the
//! binary and its exit codes) and `real_cases` (every file under
//! `cases/`). Round 2 adds `header_edges`, `verdict_edges`,
//! `evaluator_calls` (what the runner hands the evaluator), `table`,
//! `cli_output` and `real_spec` (the cases against the spec they cite).
//! Round 3 adds `header_unicode` (BOM, CR-only endings, invisible
//! characters), `fs_edges` (symlinks, odd names, exact extension),
//! `cli_edges` (closed stdout, stdout/stderr split, symlinked dirs) and
//! `verdict_contradictions` (self-contradictory audits, vacuous texts).

mod cases_adversarial {
    pub mod support;

    mod cli;
    mod cli_edges;
    mod cli_output;
    mod evaluator_calls;
    mod fs_edges;
    mod header;
    mod header_edges;
    mod header_files;
    mod header_unicode;
    mod real_cases;
    mod real_spec;
    mod runner;
    mod table;
    mod verdict;
    mod verdict_contradictions;
    mod verdict_edges;
}
