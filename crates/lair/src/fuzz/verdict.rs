//! Judging what `lair fuzz-one` did with a mutant.

use crate::cases::exec::Outcome;
use crate::cases::signal_name;

use super::{CHECKED, COMPILED};

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The checker (or the JIT's namespace rules) refused it with a message.
    Rejected,
    /// It compiled and `main` returned.
    Ran,
    /// It compiled and `main` was killed by a signal: the mutant's own
    /// undefined behaviour (spec/lir.md §6.12), not a finding.
    RuntimeCrash,
    /// It compiled and `main` did not finish in time.
    RuntimeTimeout,
    /// The checker or the backend panicked, crashed, hung, or let LLVM
    /// reject what the checker accepted: method.md rule 7 broken.
    Finding(String),
}

/// The verdict on a worker's result (`Err` is the harness's timeout).
pub fn classify(out: &Result<Outcome, String>) -> Verdict {
    let o = match out {
        Ok(o) => o,
        Err(msg) => return Verdict::Finding(format!("harness: {msg}")),
    };
    let checked = o.stderr.contains(CHECKED);
    let compiled = o.stderr.contains(COMPILED);
    let tail = last_lines(&o.stderr, 3);
    if o.stderr.contains("panicked") {
        return Verdict::Finding(format!("panic: {tail}"));
    }
    if o.stderr.contains("internal error") {
        return Verdict::Finding(format!("internal error: {tail}"));
    }
    if o.timed_out {
        return if compiled {
            Verdict::RuntimeTimeout
        } else if checked {
            Verdict::Finding("the backend timed out".into())
        } else {
            Verdict::Finding("the checker timed out".into())
        };
    }
    match (o.signal, compiled) {
        (Some(_), true) => Verdict::RuntimeCrash,
        (Some(sig), false) => Verdict::Finding(format!(
            "killed by {} {}: {tail}",
            signal_name(sig),
            if checked {
                "in the backend"
            } else {
                "in the checker"
            }
        )),
        (None, true) => Verdict::Ran,
        (None, false) if o.status != 0 && o.stderr.contains("error") => Verdict::Rejected,
        (None, false) => Verdict::Finding(format!(
            "exit {} before compiling, with no error message: {tail}",
            o.status
        )),
    }
}

fn last_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.starts_with("fuzz-one:")).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(status: i32, signal: Option<i32>, stderr: &str) -> Result<Outcome, String> {
        Ok(Outcome {
            status,
            signal,
            timed_out: status == 124,
            stdout: String::new(),
            stderr: stderr.to_string(),
        })
    }

    #[test]
    fn each_ending_gets_its_verdict() {
        let both = format!("{CHECKED}\n{COMPILED}\n");
        assert_eq!(classify(&out(0, None, &both)), Verdict::Ran);
        assert_eq!(classify(&out(7, None, &both)), Verdict::Ran);
        assert_eq!(classify(&out(139, Some(11), &both)), Verdict::RuntimeCrash);
        assert_eq!(classify(&out(124, None, &both)), Verdict::RuntimeTimeout);
        assert_eq!(
            classify(&out(
                1,
                None,
                "m.lir:1:2: error: add expects 2 operands, found 1\n"
            )),
            Verdict::Rejected
        );
        assert!(matches!(
            classify(&out(101, None, "thread 'main' panicked at x\n")),
            Verdict::Finding(m) if m.starts_with("panic")
        ));
        assert!(matches!(
            classify(&out(1, None, &format!("{CHECKED}\ninternal error: LLVM verifier rejected\n"))),
            Verdict::Finding(m) if m.starts_with("internal error")
        ));
        assert!(matches!(
            classify(&out(134, Some(6), &format!("{CHECKED}\n"))),
            Verdict::Finding(m) if m.contains("SIGABRT (6) in the backend")
        ));
        assert_eq!(
            classify(&out(124, None, "")),
            Verdict::Finding("the checker timed out".into())
        );
        assert_eq!(
            classify(&out(124, None, CHECKED)),
            Verdict::Finding("the backend timed out".into())
        );
        assert!(matches!(classify(&out(0, None, "")), Verdict::Finding(_)));
        assert!(matches!(
            classify(&Err("timed out".into())),
            Verdict::Finding(_)
        ));
    }
}
