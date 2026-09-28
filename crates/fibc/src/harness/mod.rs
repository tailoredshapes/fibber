//! The rule-6 harness (spec/compiler.md §5): every case runs in the
//! reference interpreter and, compiled, in a child process; the results
//! and the free traces must agree, and the verdict is the header's.

pub mod child;
pub mod interp;

use std::path::{Path, PathBuf};

use fibref::cases::{
    judge, list_cases, parse_header, AuditSummary, CaseResult, HeaderError, HeaderErrorKind,
    Outcome, Report, Status, Value,
};

use crate::trace::{compare, Trace};
use child::Said;

/// Where the compiler binary is.
pub struct Harness {
    pub fibc: PathBuf,
}

impl Harness {
    /// Runs every case in `dir`.
    pub fn run_dir(&self, dir: &Path) -> std::io::Result<Report> {
        let results = list_cases(dir)?.iter().map(|p| self.run_case(p)).collect();
        Ok(Report::from_results(results))
    }

    /// Runs one case both ways and judges the compiled outcome.
    pub fn run_case(&self, path: &Path) -> CaseResult {
        let status = match std::fs::read_to_string(path) {
            Err(e) => Status::HeaderError(HeaderError {
                path: path.to_path_buf(),
                line: 0,
                kind: HeaderErrorKind::Unreadable(e.to_string()),
            }),
            Ok(source) => match parse_header(path, &source) {
                Err(e) => Status::HeaderError(e),
                Ok(header) => judge(&header, &self.outcome(path, &source)),
            },
        };
        CaseResult {
            path: path.to_path_buf(),
            status,
        }
    }

    /// The compiled outcome of a case, checked against the
    /// interpreter's (compiler.md §5 step 4).
    pub fn outcome(&self, path: &Path, source: &str) -> Outcome {
        let file = path.to_string_lossy();
        let i = interp::run(source, &file);
        let c = child::run(&self.fibc, path);
        combine(i.outcome, &i.trace, c.said, &c.trace)
    }
}

/// The compiled outcome in the harness's terms, given the
/// interpreter's.
pub fn combine(interp: Outcome, itrace: &Trace, said: Said, ctrace: &Trace) -> Outcome {
    match (interp, said) {
        (Outcome::Unsupported { reason }, _) => Outcome::Unsupported { reason },
        (_, Said::Unsupported(reason)) => Outcome::Unsupported { reason },
        (Outcome::Compiled { result, audit }, Said::Result(n)) => {
            compiled(result, audit, n, compare(itrace, ctrace))
        }
        (Outcome::Rejected { message }, Said::Rejected(m)) => {
            if message.trim() == m.trim() {
                Outcome::Rejected { message }
            } else {
                Outcome::Failed {
                    message: format!(
                        "both rejected, with different messages: interpreter \"{message}\", compiled \"{m}\""
                    ),
                }
            }
        }
        (Outcome::Trapped { message, errors }, Said::Trapped(m)) => {
            trapped(&message, errors, m, compare(itrace, ctrace))
        }
        (i, c) => Outcome::Failed {
            message: format!(
                "the two sides disagree: interpreter {}, compiled {c:?}",
                brief(&i)
            ),
        },
    }
}

fn compiled(result: Value, audit: AuditSummary, n: i64, trace: Result<(), String>) -> Outcome {
    let Value::Int(expected) = result;
    let mut errors = Vec::new();
    if expected != n {
        errors.push(format!(
            "results differ: interpreter {expected}, compiled {n}"
        ));
    }
    if let Err(e) = trace {
        errors.push(e);
    }
    if errors.is_empty() {
        return Outcome::Compiled {
            result: Value::Int(n),
            audit,
        };
    }
    let mut audit = audit;
    audit.clean = false;
    audit.errors.extend(errors);
    Outcome::Compiled {
        result: Value::Int(n),
        audit,
    }
}

fn trapped(
    imsg: &str,
    mut errors: Vec<String>,
    cmsg: String,
    trace: Result<(), String>,
) -> Outcome {
    if !cmsg.contains(imsg.trim()) && !imsg.contains(cmsg.trim()) {
        errors.push(format!(
            "trap messages differ: interpreter \"{imsg}\", compiled \"{cmsg}\""
        ));
    }
    if let Err(e) = trace {
        errors.push(e);
    }
    Outcome::Trapped {
        message: cmsg,
        errors,
    }
}

fn brief(o: &Outcome) -> String {
    match o {
        Outcome::Compiled { result, .. } => format!("result {result}"),
        Outcome::Rejected { message } => format!("rejected ({message})"),
        Outcome::Trapped { message, .. } => format!("trapped ({message})"),
        Outcome::Failed { message } => format!("failed ({message})"),
        Outcome::Unsupported { reason } => format!("unsupported ({reason})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Trace {
        Trace::parse(s)
    }

    fn ok(n: i64) -> Outcome {
        Outcome::Compiled {
            result: Value::Int(n),
            audit: AuditSummary::clean(),
        }
    }

    #[test]
    fn agreeing_runs_keep_the_interpreters_audit() {
        let o = combine(
            ok(3),
            &t("A 1 o\nF 1\n"),
            Said::Result(3),
            &t("A 1 o\nF 1\n"),
        );
        assert_eq!(o, ok(3));
    }

    #[test]
    fn a_differing_trace_or_result_is_an_audit_error() {
        let o = combine(ok(3), &t("A 1 o\nF 1\n"), Said::Result(3), &t("A 1 o\n"));
        match o {
            Outcome::Compiled { audit, .. } => {
                assert!(!audit.clean && audit.errors[0].contains("trace line 2"))
            }
            o => panic!("{o:?}"),
        }
        let o = combine(ok(3), &t(""), Said::Result(4), &t(""));
        match o {
            Outcome::Compiled { audit, .. } => assert!(audit.errors[0].contains("results differ")),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn rejections_must_carry_the_same_message() {
        let r = |m: &str| Outcome::Rejected { message: m.into() };
        assert_eq!(
            combine(r("x"), &t(""), Said::Rejected("x".into()), &t("")),
            r("x")
        );
        assert!(matches!(
            combine(r("x"), &t(""), Said::Rejected("y".into()), &t("")),
            Outcome::Failed { .. }
        ));
    }

    #[test]
    fn unsupported_on_either_side_is_pending() {
        assert!(matches!(
            combine(ok(1), &t(""), Said::Unsupported("later".into()), &t("")),
            Outcome::Unsupported { .. }
        ));
    }

    #[test]
    fn traps_compare_messages_and_the_trace_before_the_abort() {
        let tr = Outcome::Trapped {
            message: "integer / by zero".into(),
            errors: vec![],
        };
        let o = combine(
            tr.clone(),
            &t("A 1 o\n"),
            Said::Trapped("integer / by zero".into()),
            &t("A 1 o\n"),
        );
        assert!(matches!(o, Outcome::Trapped { errors, .. } if errors.is_empty()));
        let o = combine(tr, &t("A 1 o\n"), Said::Trapped("other".into()), &t(""));
        assert!(matches!(o, Outcome::Trapped { errors, .. } if errors.len() == 2));
    }
}
