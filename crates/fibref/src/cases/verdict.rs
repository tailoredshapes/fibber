//! Comparing what a case's header expects with what the evaluator did.
//!
//! The rules (`spec/method.md`, rule 3):
//!
//! | header   | outcome                                   | status  |
//! |----------|-------------------------------------------|---------|
//! | accept   | Compiled, result and audit both match     | Pass    |
//! | accept   | Compiled, result or audit differs          | Fail    |
//! | accept, `allocs: <= N` | Compiled, more than N allocations | Fail |
//! | accept, `allocs: <= N` | Compiled, no count reported       | Fail |
//! | accept   | Rejected                                  | Fail    |
//! | reject   | Rejected, message contains the error text | Pass    |
//! | reject   | Rejected, message lacks it                | Fail    |
//! | reject   | Compiled                                  | Fail    |
//! | accept, reject | Trapped                             | Fail    |
//! | trap     | Trapped, message contains the trap text, no audit error | Pass |
//! | trap     | Trapped otherwise, Compiled or Rejected   | Fail    |
//! | any      | Failed (the run stopped other than by a trap) | Fail |
//! | reject, trap | expected text blank (any outcome but Unsupported) | Fail |
//! | either   | Unsupported                               | Pending |
//!
//! An empty or whitespace-only expected text is a Fail rather than a
//! vacuous Pass: every message contains `""`, nearly every one contains
//! a space, and a test that cannot fail is worse than no test
//! (`spec/method.md`). The header parser trims such a value to empty
//! and refuses it from files; this guard covers headers built in code.
//!
//! `audit: clean` means the summary is clean: nothing live at exit and
//! no check fired. `audit: leak-cycle` means the only thing wrong is at
//! least one cycle through cells: `leak_cycles > 0`, no other leaks, no
//! errors. Pending is reported separately and is never a pass.
//!
//! `allocs: <= N` means the run allocated at most `N` heap objects: the
//! `A` lines of its free trace (`spec/compiler.md` §4), counted by the
//! evaluator that ran the case and handed to [`judge_counted`]. More
//! than `N` is a Fail, never a Pending; so is an evaluator that gave no
//! count, since a bound nobody checked is a test that cannot fail.
//!
//! A case that carries an `open` label (`header::Labels`) fails today
//! for the §7 items it names, and [`judge_labelled`] says so: it judges
//! as above and then, for an `accept` case whose failure is the program
//! being refused or giving another answer, reports [`Status::Open`]
//! (listed, counted apart, not a failure, never a pass); a pass is
//! [`Status::OpenPassed`], a failure, because the label is out of date;
//! and any failure that is not the program's own (the two tools
//! disagreeing, a memory error, a run that failed) stays a failure.

use std::fmt;

use super::evaluator::{AuditSummary, Outcome, Value};
use super::header::{AuditExpect, Expected, Header, HeaderError, Verdict};

/// What an `open` case that passed is told.
const OPEN_PASSED: &str = "the item landed: remove `open`";

/// How one case fared against its header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// The verdict held.
    Pass,
    /// The verdict did not hold; the string says how.
    Fail(String),
    /// The evaluator could not decide the case; the string says why.
    Pending(String),
    /// The case carries an `open` label and failed as the label says it
    /// does; the string names the items and how it failed. Not a
    /// pass, and not a failure of the suite.
    Open(String),
    /// The case carries an `open` label and passed: the items landed,
    /// so the label is out of date. A failure of the suite.
    OpenPassed,
    /// The header could not be parsed, so nothing ran.
    HeaderError(HeaderError),
}

impl Status {
    /// The one-word label used in reports.
    pub fn label(&self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail(_) => "FAIL",
            Status::Pending(_) => "PENDING",
            Status::Open(_) => "OPEN",
            Status::OpenPassed => "FAIL",
            Status::HeaderError(_) => "HEADER",
        }
    }

    /// The explanation, if the status has one.
    pub fn detail(&self) -> String {
        match self {
            Status::Pass => String::new(),
            Status::Fail(why) | Status::Pending(why) | Status::Open(why) => why.clone(),
            Status::OpenPassed => OPEN_PASSED.to_string(),
            Status::HeaderError(e) => format!("line {}: {}", e.line, e.kind),
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = self.detail();
        if detail.is_empty() {
            f.write_str(self.label())
        } else {
            write!(f, "{}: {}", self.label(), detail)
        }
    }
}

/// Decides the status of a case from its header and the evaluator's
/// outcome, knowing no allocation count: a header with `allocs` fails
/// (see [`judge_counted`]).
pub fn judge(header: &Header, outcome: &Outcome) -> Status {
    judge_counted(header, outcome, None)
}

/// [`judge`] given the number of heap objects the run allocated, when
/// the evaluator counts them ([`Evaluator::run_counted`]).
///
/// [`Evaluator::run_counted`]: super::evaluator::Evaluator::run_counted
pub fn judge_counted(header: &Header, outcome: &Outcome, allocs: Option<u64>) -> Status {
    match (&header.verdict, outcome) {
        (_, Outcome::Unsupported { reason }) => Status::Pending(reason.clone()),
        (
            Verdict::Accept {
                result,
                audit,
                allocs: maximum,
            },
            outcome,
        ) => judge_accept(result, *audit, *maximum, allocs, outcome),
        (Verdict::Reject { error }, outcome) => judge_reject(error, outcome),
        (Verdict::Trap { trap }, outcome) => judge_trap(trap, outcome),
    }
}

/// [`judge_counted`] for a case whose header carries the `open` items
/// `open` (empty when it has none, and then this is [`judge_counted`]).
///
/// With items, a pass becomes [`Status::OpenPassed`] and a failure
/// [`Status::Open`] when it is the program's own: the checker refused it,
/// it trapped or it finished with a different answer, and in each case
/// both tools agreed and the audit found nothing. A failure of another
/// kind (`Outcome::Failed`, which is a run that broke or two tools that
/// disagree, and an audit error or a divergent trace) stays a
/// [`Status::Fail`]: an excuse for the program is not an excuse for the
/// tools, and a divergence between them must never read as OPEN.
pub fn judge_labelled(
    header: &Header,
    open: &[String],
    outcome: &Outcome,
    allocs: Option<u64>,
) -> Status {
    let status = judge_counted(header, outcome, allocs);
    if open.is_empty() {
        return status;
    }
    match status {
        Status::Pass => Status::OpenPassed,
        Status::Fail(why) if programs_own_failure(outcome) => {
            Status::Open(format!("{}: {why}", open.join(" ")))
        }
        other => other,
    }
}

/// True when the outcome says the program is wrong and nothing else is:
/// refused by the checker, trapped, or finished with a clean audit.
fn programs_own_failure(outcome: &Outcome) -> bool {
    match outcome {
        Outcome::Rejected { .. } => true,
        Outcome::Trapped { errors, .. } => errors.is_empty(),
        Outcome::Compiled { audit, .. } => {
            audit.errors.is_empty() && audit.leaks == 0 && (audit.clean || audit.leak_cycles > 0)
        }
        Outcome::Failed { .. } | Outcome::Unsupported { .. } => false,
    }
}

/// `expect: trap`: the program must compile, pass the checker and
/// trap with a message containing `expected`, with no audit error at
/// the abort (spec/method.md rule 3; types §2.12).
fn judge_trap(expected: &str, outcome: &Outcome) -> Status {
    if expected.trim().is_empty() {
        return Status::Fail(
            "expected trap text is empty or only whitespace: nearly every message contains it, so this case could never fail"
                .to_string(),
        );
    }
    match outcome {
        Outcome::Trapped { message, errors } if message.contains(expected) && errors.is_empty() => {
            Status::Pass
        }
        Outcome::Trapped { message, errors } if message.contains(expected) => Status::Fail(
            format!("trapped as expected, but the audit at the abort failed: {}", errors.join("; ")),
        ),
        Outcome::Trapped { message, .. } => Status::Fail(format!(
            "trapped, but the trap does not contain the expected text: expected \"{expected}\", got \"{message}\""
        )),
        Outcome::Compiled { result, audit } => Status::Fail(format!(
            "expected a trap with \"{expected}\", but the run finished: result {result}, audit {audit}"
        )),
        Outcome::Rejected { message } => Status::Fail(format!(
            "expected a trap with \"{expected}\", but rejected: {message}"
        )),
        Outcome::Failed { message } => Status::Fail(format!(
            "expected a trap with \"{expected}\", but the run failed: {message}"
        )),
        Outcome::Unsupported { reason } => Status::Pending(reason.clone()),
    }
}

/// `accept`: the result and the audit must match, and the allocation
/// count `counted` must be within the `maximum`, when there is one.
fn judge_accept(
    expected: &Expected,
    audit: AuditExpect,
    maximum: Option<u64>,
    counted: Option<u64>,
    outcome: &Outcome,
) -> Status {
    match outcome {
        Outcome::Compiled {
            result,
            audit: summary,
        } => {
            let mismatches: Vec<String> = result_mismatch(expected, result)
                .into_iter()
                .chain(audit_mismatch(audit, summary))
                .chain(allocs_mismatch(maximum, counted))
                .collect();
            if mismatches.is_empty() {
                Status::Pass
            } else {
                Status::Fail(mismatches.join("; "))
            }
        }
        Outcome::Rejected { message } => {
            Status::Fail(format!("expected accept, but rejected: {message}"))
        }
        Outcome::Trapped { message, .. } => Status::Fail(format!("the run trapped: {message}")),
        Outcome::Failed { message } => Status::Fail(format!("the run failed: {message}")),
        Outcome::Unsupported { reason } => Status::Pending(reason.clone()),
    }
}

fn judge_reject(expected: &str, outcome: &Outcome) -> Status {
    // Both strings are shown verbatim (Display, not Debug) so a reader can
    // find the header's text and the compiler's message as written.
    match outcome {
        Outcome::Rejected { .. } | Outcome::Compiled { .. } if expected.trim().is_empty() => {
            Status::Fail(
                "expected error text is empty or only whitespace: nearly every message contains it, so this case could never fail"
                    .to_string(),
            )
        }
        Outcome::Rejected { message } if message.contains(expected) => Status::Pass,
        Outcome::Rejected { message } => Status::Fail(format!(
            "rejected, but the error does not contain the expected text: expected \"{expected}\", got \"{message}\""
        )),
        Outcome::Compiled { result, audit } => Status::Fail(format!(
            "expected reject with \"{expected}\", but compiled: result {result}, audit {audit}"
        )),
        Outcome::Failed { message } | Outcome::Trapped { message, .. } => Status::Fail(format!(
            "expected reject with \"{expected}\", but compiled and the run failed: {message}"
        )),
        Outcome::Unsupported { reason } => Status::Pending(reason.clone()),
    }
}

fn result_mismatch(expected: &Expected, got: &Value) -> Option<String> {
    let matches = match (expected, got) {
        (Expected::Int(e), Value::Int(g)) => e == g,
    };
    (!matches).then(|| format!("result: expected {expected}, got {got}"))
}

fn allocs_mismatch(maximum: Option<u64>, counted: Option<u64>) -> Option<String> {
    match (maximum, counted) {
        (None, _) => None,
        (Some(max), Some(n)) if n <= max => None,
        (Some(max), Some(n)) => Some(format!(
            "allocs: expected at most {max} heap objects, the run allocated {n}"
        )),
        (Some(max), None) => Some(format!(
            "allocs: the header says `<= {max}` but the evaluator reported no allocation count"
        )),
    }
}

fn audit_mismatch(expected: AuditExpect, summary: &AuditSummary) -> Option<String> {
    let matches = match expected {
        AuditExpect::Clean => {
            summary.clean
                && summary.leak_cycles == 0
                && summary.leaks == 0
                && summary.errors.is_empty()
        }
        AuditExpect::LeakCycle => {
            summary.leak_cycles > 0 && summary.leaks == 0 && summary.errors.is_empty()
        }
    };
    (!matches).then(|| format!("audit: expected {expected}, got {summary}"))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod open_tests;
