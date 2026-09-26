//! The case runner.
//!
//! Each file in `cases/` fixes its expected verdict in its header
//! (`spec/method.md`, rule 3): an `accept` case must type-check, run,
//! produce the stated `result` and finish with the stated `audit`; a
//! `reject` case must fail to compile with an error containing the
//! stated `error` text. This module reads those headers ([`header`]),
//! runs each case through an [`Evaluator`] ([`runner`]), compares what
//! came back with the header ([`verdict`]) and renders the result
//! ([`table`]). A case whose rules are not implemented yet is Pending,
//! which is reported separately and is not a pass.

pub mod evaluator;
pub mod header;
pub mod runner;
pub mod table;
pub mod verdict;

pub use evaluator::{AuditSummary, Evaluator, Outcome, PendingEvaluator, Value};
pub use header::{
    parse_header, read_header, AuditExpect, Expected, Header, HeaderError, HeaderErrorKind, Verdict,
};
pub use runner::{list_cases, list_cases_recursive, run_case, run_dir, CaseResult, Counts, Report};
pub use table::render;
pub use verdict::{judge, Status};
